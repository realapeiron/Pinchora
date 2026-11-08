//! Handing roles out on Discord: level rewards and what the server store sells, and who may set a role up
//! to be handed out.

use pinchora_core::{
    i18n::Lang,
    leveling::{self, LevelReward},
    settings::{self, Grantor, RoleFacts},
    t,
};
use pinchora_db::LevelRoleRow;
use twilight_http::{Error, api_error::ApiError, error::ErrorType, request::AuditLogReason};
use twilight_model::{
    guild::{Permissions, Role},
    id::Id,
};

use crate::{App, Cx};

const UNKNOWN_MEMBER: u64 = 10007;
const UNKNOWN_ROLE: u64 = 10011;
const MISSING_PERMISSIONS: u64 = 50013;

/// Discord's error code, when it sent one.
pub(crate) fn error_code(error: &Error) -> Option<u64> {
    match error.kind() {
        ErrorType::Response {
            error: ApiError::General(general),
            ..
        } => Some(general.code),
        _ => None,
    }
}

pub(crate) fn role_is_gone(error: &Error) -> bool {
    error_code(error) == Some(UNKNOWN_ROLE)
}

pub(crate) fn member_is_gone(error: &Error) -> bool {
    error_code(error) == Some(UNKNOWN_MEMBER)
}

/// Why Discord refused to give a role, in `lang`, worded to follow "I couldn't give you <role>:".
pub(crate) fn refusal_in(lang: Lang, error: &Error) -> String {
    t!(
        lang,
        match error_code(error) {
            Some(MISSING_PERMISSIONS) => "role-refusal.permissions",
            Some(UNKNOWN_ROLE) => "role-refusal.gone",
            Some(_) => "role-refusal.refused",
            None => "role-refusal.unreachable",
        }
    )
}

pub(crate) fn rewards(rows: &[LevelRoleRow]) -> Vec<LevelReward> {
    rows.iter()
        .map(|r| LevelReward {
            level: r.level.max(0) as u32,
            role_id: r.role_id as u64,
        })
        .collect()
}

pub(crate) async fn level_rewards(app: &App, guild: u64) -> anyhow::Result<Vec<LevelReward>> {
    Ok(rewards(&app.db.level_roles(guild).await?))
}

/// The server's roles, and the position of the bot's highest one: the roles below it are the ones it can
/// hand out.
async fn roles_and_bot_position(app: &App, guild: u64) -> anyhow::Result<(Vec<Role>, i64)> {
    let guild = Id::new(guild.max(1));
    let me = app
        .http
        .guild_member(guild, Id::new(app.application_id.get()))
        .await?
        .model()
        .await?;
    let roles = app.http.roles(guild).await?.models().await?;
    let top = roles
        .iter()
        .filter(|r| me.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    Ok((roles, top))
}

/// The member running the command as a [`Grantor`], from what the interaction says about them; whether
/// they own the server is left for [`unassignable`] to ask only when it matters.
fn grantor(cx: &Cx, roles: &[Role]) -> Grantor {
    let member = cx.interaction.member.as_ref();
    Grantor {
        owner: false,
        administrator: member
            .and_then(|m| m.permissions)
            .is_some_and(|p| p.contains(Permissions::ADMINISTRATOR)),
        top_role: member.map(|m| {
            roles
                .iter()
                .filter(|r| m.roles.contains(&r.id))
                .map(|r| r.position)
                .max()
                .unwrap_or(0)
        }),
    }
}

/// Why `role` cannot be set up to be handed out by the member running the command, or `None` when it can:
/// the bot must be able to give it ([`settings::role_refusal`], which the dashboard's role pickers use too),
/// and the member may only set up roles below their own highest role and, unless they are an administrator,
/// none with a dangerous permission ([`settings::grant_refusal`]). The server owner may set up any role
/// the bot can give.
pub(crate) async fn unassignable(cx: &Cx, guild: u64, role: &Role) -> Option<String> {
    let facts = RoleFacts {
        everyone: role.id.get() == guild,
        managed: role.managed,
        position: role.position,
        permissions: role.permissions.bits(),
    };
    let label = format!("<@&{}>", role.id);
    let lang = cx.lang();
    if facts.everyone || facts.managed {
        return settings::role_refusal_in(lang, facts, None, &label);
    }
    let (roles, top) = match roles_and_bot_position(&cx.app, guild).await {
        Ok(found) => found,
        Err(error) => {
            tracing::warn!(guild, %error, "could not look up the bot's roles");
            return settings::role_refusal_in(lang, facts, None, &label);
        }
    };
    if let Some(why) = settings::role_refusal_in(lang, facts, Some(top), &label) {
        return Some(why);
    }
    let why = settings::grant_refusal_in(lang, facts, grantor(cx, &roles), &label)?;
    let user = cx.author().map(|u| u.id.get()).unwrap_or(0);
    match cx.app.guild_facts(guild).await {
        Some(facts) if facts.owner == user => None,
        _ => Some(why),
    }
}

/// Recheck live roles before automatic grants: configuration-time permission is not permanent.
pub(crate) async fn safe_reward_roles(app: &App, guild: u64) -> anyhow::Result<(Vec<u64>, Vec<u64>)> {
    let (roles, top) = roles_and_bot_position(app, guild).await?;
    let existing = roles.iter().map(|role| role.id.get()).collect();
    let safe = roles
        .iter()
        .filter(|role| {
            let facts = RoleFacts {
                everyone: role.id.get() == guild,
                managed: role.managed,
                position: role.position,
                permissions: role.permissions.bits(),
            };
            settings::role_refusal(facts, Some(top), "reward").is_none()
                && settings::run_time_refusal_in(Lang::En, facts.permissions, "reward").is_none()
        })
        .map(|role| role.id.get())
        .collect();
    Ok((safe, existing))
}

/// Gives and takes the level roles a member at `level` should hold, as the roles given and how many
/// were taken. Discord refusals are logged and skipped; a role that no longer exists is dropped from
/// the rewards.
pub(crate) async fn apply_level_roles(
    app: &App,
    guild: u64,
    user: u64,
    level: u32,
    current: &[u64],
    rewards: &[LevelReward],
    stack: bool,
) -> (Vec<u64>, usize) {
    let (add, remove) = leveling::role_changes(rewards, level, stack, current);
    let (guild_id, user_id) = (Id::new(guild.max(1)), Id::new(user.max(1)));
    let (mut added, mut removed) = (Vec::new(), 0);
    let (safe, existing) = if add.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        match safe_reward_roles(app, guild).await {
            Ok(safe) => safe,
            Err(error) => {
                tracing::warn!(%error, guild, "cannot verify reward roles");
                (Vec::new(), add.clone())
            }
        }
    };
    for role in add {
        if !existing.contains(&role)
            && let Err(error) = app.db.forget_role(guild, role).await
        {
            tracing::warn!(%error, guild, role, "cannot clean up deleted reward");
        }
        if !safe.contains(&role) {
            continue;
        }
        match app
            .http
            .add_guild_member_role(guild_id, user_id, Id::new(role.max(1)))
            .reason("Level reward")
            .await
        {
            Ok(_) => added.push(role),
            Err(error) => {
                tracing::warn!(guild, user, role, %error, "could not give a level role");
                if role_is_gone(&error)
                    && let Err(error) = app.db.forget_role(guild, role).await
                {
                    tracing::warn!(guild, role, %error, "could not drop a deleted level role");
                }
            }
        }
    }
    for role in remove {
        match app
            .http
            .remove_guild_member_role(guild_id, user_id, Id::new(role.max(1)))
            .reason("Level reward replaced")
            .await
        {
            Ok(_) => removed += 1,
            Err(error) => tracing::warn!(guild, user, role, %error, "could not take a level role"),
        }
    }
    (added, removed)
}

/// Applies the server's level roles after `user` reached `level`. Never fails the caller. Returns the
/// role they just earned: of the roles given, the one for the highest level.
pub(crate) async fn level_up_roles(
    app: &App,
    guild: u64,
    user: u64,
    level: u32,
    current: &[u64],
    stack: bool,
) -> Option<u64> {
    match level_rewards(app, guild).await {
        Ok(rewards) if !rewards.is_empty() => {
            let (added, _) = apply_level_roles(app, guild, user, level, current, &rewards, stack).await;
            rewards
                .iter()
                .filter(|r| added.contains(&r.role_id))
                .max_by_key(|r| (r.level, r.role_id))
                .map(|r| r.role_id)
        }
        Ok(_) => None,
        Err(error) => {
            tracing::warn!(guild, %error, "could not load the level roles");
            None
        }
    }
}

/// Gives `user` the reputation roles their `total` earns that they do not hold yet, and returns the
/// roles given. Reputation roles always stack. Never fails the caller; a role that no longer exists
/// is dropped from the rewards.
pub(crate) async fn give_rep_roles(app: &App, guild: u64, user: u64, total: i32, current: &[u64]) -> Vec<u64> {
    let rewards: Vec<LevelReward> = match app.db.rep_roles(guild).await {
        Ok(rows) => rows
            .iter()
            .map(|r| LevelReward {
                level: r.reputation.max(0) as u32,
                role_id: r.role_id as u64,
            })
            .collect(),
        Err(error) => {
            tracing::warn!(guild, %error, "could not load the reputation roles");
            return Vec::new();
        }
    };
    let (add, _) = leveling::role_changes(&rewards, total.max(0) as u32, true, current);
    let (guild_id, user_id) = (Id::new(guild.max(1)), Id::new(user.max(1)));
    let mut given = Vec::new();
    for role in add {
        match app
            .http
            .add_guild_member_role(guild_id, user_id, Id::new(role.max(1)))
            .reason("Reputation reward")
            .await
        {
            Ok(_) => given.push(role),
            Err(error) => {
                tracing::warn!(guild, user, role, %error, "could not give a reputation role");
                if role_is_gone(&error)
                    && let Err(error) = app.db.forget_role(guild, role).await
                {
                    tracing::warn!(guild, role, %error, "could not drop a deleted reputation role");
                }
            }
        }
    }
    given
}

/// Gives a new member the server's join role. Never fails the caller; a role that no longer exists
/// is dropped from the settings.
pub(crate) async fn give_join_role(app: &App, guild: u64, user: u64, role: u64) {
    let given = app
        .http
        .add_guild_member_role(Id::new(guild.max(1)), Id::new(user.max(1)), Id::new(role.max(1)))
        .reason("Join role")
        .await;
    if let Err(error) = given {
        tracing::warn!(guild, user, role, %error, "could not give the join role");
        if role_is_gone(&error) {
            match app.db.forget_role(guild, role).await {
                Ok(()) => app.forget_guild_settings(guild).await,
                Err(error) => tracing::warn!(guild, role, %error, "could not drop a deleted join role"),
            }
        }
    }
}

/// The nearest level role above `level` that still exists, as its name and level.
/// `None` when there is none or Discord cannot name the roles.
pub(crate) async fn next_role(app: &App, guild: u64, level: u32) -> anyhow::Result<Option<(String, u32)>> {
    let rewards = level_rewards(app, guild).await?;
    if !rewards.iter().any(|r| r.level > level) {
        return Ok(None);
    }
    let Some(names) = app.guild_roles(guild).await else {
        return Ok(None);
    };
    let existing: Vec<LevelReward> = rewards.into_iter().filter(|r| names.contains_key(&r.role_id)).collect();
    Ok(leveling::next_reward(&existing, level).map(|r| (names[&r.role_id].clone(), r.level)))
}
