//! Members joining, leaving and changing: the roles given to them on their own (auto roles) and the roles
//! kept for them while they are away (sticky roles).

use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use chrono::Duration as Span;
use moka::future::Cache as Moka;
use pinchora_core::{
    modules::Module,
    roles_pack::{AutoKind, STICKY_KEPT_DAYS},
    settings::{self, RoleFacts},
};
use twilight_model::{
    gateway::payload::incoming::{MemberAdd, MemberRemove, MemberUpdate},
    id::{Id, marker::UserMarker},
};

use super::give::{self, bot_view};
use crate::App;

/// Pages of a thousand members the snapshot reads at most.
const SNAPSHOT_PAGES: usize = 10;
/// One snapshot a server in this long.
const SNAPSHOT_REST: Duration = Duration::from_secs(600);

/// Whether a server has sticky roles on, kept for half a minute: every member update asks.
static STICKY: LazyLock<Moka<u64, bool>> = LazyLock::new(|| {
    Moka::builder()
        .max_capacity(100_000)
        .time_to_live(Duration::from_secs(30))
        .build()
});

async fn sticky_on(app: &App, guild: u64) -> bool {
    STICKY
        .get_with(guild, async {
            app.db.sticky_enabled(guild).await.unwrap_or_else(|error| {
                tracing::warn!(guild, %error, "could not read the sticky roles switch");
                false
            })
        })
        .await
}

/// Drops this pod's copy of a server's sticky switch (`/autoroles sticky` or the dashboard changed it).
pub async fn sticky_changed(guild: u64) {
    STICKY.invalidate(&guild).await;
}

/// A member joined (the `on_member_join` hook): the roles they held when they left come back, then the
/// server's auto roles are given or set to wait.
pub(crate) async fn on_member_join(app: &Arc<App>, event: &MemberAdd) {
    let guild = event.guild_id.get();
    let user = &event.member.user;
    if !app.module_allowed(guild, None, Module::AutoRoles).await {
        return;
    }
    if !user.bot && sticky_on(app, guild).await {
        restore(app, guild, user.id.get()).await;
    }
    let kind = if user.bot { AutoKind::Bot } else { AutoKind::Join };
    give_auto_roles(app, guild, user.id.get(), kind).await;
}

/// A member passed the join gate: the roles of the `after_verify` kind are theirs. For the package that
/// holds new members at the gate to call when it lets one in.
pub async fn give_after_verify(app: &Arc<App>, guild: u64, user: u64) {
    if app.module_allowed(guild, None, Module::AutoRoles).await {
        give_auto_roles(app, guild, user, AutoKind::AfterVerify).await;
    }
}

/// Gives a member the server's auto roles of one kind, those with a delay later.
async fn give_auto_roles(app: &Arc<App>, guild: u64, user: u64, kind: AutoKind) {
    let rows = match app.db.auto_roles_of_kind(guild, kind.key()).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(guild, %error, "could not load the auto roles");
            return;
        }
    };
    let now = app.now();
    for row in rows {
        let role = row.role_id as u64;
        if row.delay_secs > 0 {
            let at = now + Span::seconds(i64::from(row.delay_secs));
            if let Err(error) = app
                .db
                .schedule_grant(guild, user, role, at, row.remove_after_secs)
                .await
            {
                tracing::warn!(guild, user, role, %error, "could not set an auto role to wait");
            }
            continue;
        }
        if let Err(error) = give::give_timed(app, guild, user, role, row.remove_after_secs, "Auto role").await {
            tracing::warn!(guild, user, role, %error, "could not give an auto role");
        }
    }
}

/// Gives back what a member held when they left, as far as the bot can: not a role that is gone, that
/// sits above the bot, that an integration owns, that carries a dangerous permission, or whose time ran
/// out while they were away.
async fn restore(app: &Arc<App>, guild: u64, user: u64) {
    // Most joins are new members with nothing kept: ask the database before asking Discord anything.
    match app.db.sticky_waiting(guild, user).await {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            tracing::warn!(guild, user, %error, "could not look for a member's kept roles");
            return;
        }
    }
    let Some((roles, top)) = bot_view(app, guild).await else {
        tracing::warn!(guild, user, "could not look at the roles to give a member's back");
        return;
    };
    let row = match app.db.take_sticky(guild, user).await {
        Ok(Some(row)) => row,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(guild, user, %error, "could not read a member's kept roles");
            return;
        }
    };
    let now = app.now();
    if row
        .left_at
        .is_some_and(|left| now - left > Span::days(STICKY_KEPT_DAYS))
    {
        return;
    }
    let lapsed = app.db.lapsed_roles(guild, user, now).await.unwrap_or_default();
    for role in lapsed.iter().map(|r| *r as u64) {
        if let Err(error) = app.db.cancel_removal(guild, user, role).await {
            tracing::warn!(guild, user, role, %error, "could not clear a lapsed role's timer");
        }
    }
    for role in row.role_ids.iter().filter(|r| !lapsed.contains(*r)).map(|r| *r as u64) {
        let Some(found) = roles.iter().find(|r| r.id.get() == role) else {
            continue;
        };
        let facts = RoleFacts {
            everyone: role == guild,
            managed: found.managed,
            position: found.position,
            permissions: found.permissions.bits(),
        };
        if settings::role_refusal(facts, Some(top), "").is_some()
            || settings::run_time_refusal(facts.permissions, "").is_some()
        {
            continue;
        }
        if let Err(error) = give::give(app, guild, user, role, "Sticky role").await {
            tracing::debug!(guild, user, role, %error, "could not give a sticky role back");
        }
    }
}

/// A member left (the `on_member_leave` hook): what they held waits for them, and the roles still set to
/// be given to them are forgotten.
pub(crate) async fn on_member_leave(app: &Arc<App>, event: &MemberRemove) {
    if event.user.bot {
        return;
    }
    let (guild, user) = (event.guild_id.get(), event.user.id.get());
    if let Err(error) = app.db.cancel_grants(guild, user).await {
        tracing::warn!(guild, user, %error, "could not forget a leaver's waiting roles");
    }
    if sticky_on(app, guild).await
        && let Err(error) = app.db.note_left(guild, user, app.now()).await
    {
        tracing::warn!(guild, user, %error, "could not note that a member left");
    }
}

/// A member changed, roles included (the `on_member_update` hook): while sticky roles are on, what they
/// hold now is what comes back if they leave.
pub(crate) async fn on_member_update(app: &Arc<App>, event: &MemberUpdate) {
    if event.user.bot {
        return;
    }
    let guild = event.guild_id.get();
    if !sticky_on(app, guild).await {
        return;
    }
    let roles: Vec<u64> = event.roles.iter().map(|r| r.get()).collect();
    if let Err(error) = app.db.remember_roles(guild, event.user.id.get(), &roles).await {
        tracing::warn!(guild, %error, "could not note a member's roles");
    }
}

/// Notes the roles of the server's current members, so a member who leaves before their next role change
/// still has something kept. Runs in the background, a server at a time, at most 10,000 members.
pub async fn start_sticky_snapshot(app: &Arc<App>, guild: u64) {
    match app
        .cache
        .try_cooldown(&format!("stickysnap:{guild}"), SNAPSHOT_REST)
        .await
    {
        Ok(None) => {}
        Ok(Some(_)) => return,
        Err(error) => {
            tracing::warn!(guild, %error, "could not start a sticky roles snapshot");
            return;
        }
    }
    let app2 = app.clone();
    app.spawn(async move { snapshot(&app2, guild).await });
}

async fn snapshot(app: &App, guild: u64) {
    let guild_id = Id::new(guild.max(1));
    let mut after: Option<Id<UserMarker>> = None;
    for _ in 0..SNAPSHOT_PAGES {
        let mut request = app.http.guild_members(guild_id).limit(1000);
        if let Some(last) = after {
            request = request.after(last);
        }
        let members = match request.await {
            Ok(response) => match response.models().await {
                Ok(members) => members,
                Err(error) => {
                    tracing::warn!(guild, %error, "could not read the member list");
                    return;
                }
            },
            Err(error) => {
                tracing::warn!(guild, %error, "could not list the members for a sticky roles snapshot");
                return;
            }
        };
        let Some(last) = members.last().map(|m| m.user.id) else {
            return;
        };
        for member in members.iter().filter(|m| !m.user.bot && !m.roles.is_empty()) {
            let roles: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
            if let Err(error) = app.db.remember_roles(guild, member.user.id.get(), &roles).await {
                tracing::warn!(guild, %error, "could not note a member's roles");
                return;
            }
        }
        if members.len() < 1000 {
            return;
        }
        after = Some(last);
    }
}
