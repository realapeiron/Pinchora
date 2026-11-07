//! Shared permission checks and moderation/staff logging for every entry path.
use crate::{
    App,
    commands::moderation::{self, Act, BOT_NAME},
};
use pinchora_core::{
    logging::LogCategory,
    moderation::{CaseKind, Rank, may_act},
    settings::{self, Grantor, RoleFacts},
    t,
    verification::Config,
};
use pinchora_db::NewStaffAction;
use twilight_model::{
    channel::message::{AllowedMentions, Component},
    guild::{Member, Permissions, Role},
    id::{Id, marker::RoleMarker},
};
fn top(roles: &[Role], held: &[Id<RoleMarker>]) -> i64 {
    roles
        .iter()
        .filter(|r| held.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0)
}
fn powers(roles: &[Role], guild: u64, held: &[Id<RoleMarker>]) -> Permissions {
    roles
        .iter()
        .filter(|r| r.id.get() == guild || held.contains(&r.id))
        .fold(Permissions::empty(), |p, r| p | r.permissions)
}
fn facts(role: &Role, guild: u64) -> RoleFacts {
    RoleFacts {
        everyone: role.id.get() == guild,
        managed: role.managed,
        position: role.position,
        permissions: role.permissions.bits(),
    }
}
/// Fresh Discord facts: unknown hierarchy never authorizes a mutation.
pub(super) async fn member(app: &App, guild: u64, user: u64, c: &Config) -> anyhow::Result<Option<Member>> {
    let m = request(app, app.http.guild_member(Id::new(guild), Id::new(user)))
        .await?
        .model()
        .await?;
    let g = request(app, app.http.guild(Id::new(guild))).await?.model().await?;
    let roles = request(app, app.http.roles(Id::new(guild))).await?.models().await?;
    let bot = request(app, app.http.guild_member(Id::new(guild), app.application_id.cast()))
        .await?
        .model()
        .await?;
    let staff = Permissions::ADMINISTRATOR
        | Permissions::MANAGE_GUILD
        | Permissions::MANAGE_MESSAGES
        | Permissions::MODERATE_MEMBERS
        | Permissions::KICK_MEMBERS
        | Permissions::BAN_MEMBERS
        | Permissions::MANAGE_ROLES
        | Permissions::MANAGE_CHANNELS
        | Permissions::MANAGE_WEBHOOKS
        | Permissions::MANAGE_THREADS;
    if m.user.bot
        || g.owner_id.get() == user
        || user == app.application_id.get()
        || c.verification.exempt_users.contains(&user.to_string())
        || m.roles
            .iter()
            .any(|r| c.verification.exempt_roles.contains(&r.to_string()))
        || powers(&roles, guild, &m.roles).intersects(staff)
        || top(&roles, &m.roles) >= top(&roles, &bot.roles)
    {
        return Ok(None);
    }
    Ok(Some(m))
}
pub(super) async fn permission(app: &App, guild: u64, needed: Permissions, key: &str) -> anyhow::Result<()> {
    let roles = request(app, app.http.roles(Id::new(guild))).await?.models().await?;
    let bot = request(app, app.http.guild_member(Id::new(guild), app.application_id.cast()))
        .await?
        .model()
        .await?;
    let p = powers(&roles, guild, &bot.roles);
    if !p.contains(Permissions::ADMINISTRATOR) && !p.contains(needed) {
        let lang = app.guild_lang(guild).await;
        anyhow::bail!(t!(
            lang,
            "verification.missing_permission",
            permission = t!(lang, &format!("verification.perm.{key}"))
        ));
    }
    Ok(())
}
pub async fn check_roles(app: &App, guild: u64, actor: u64, c: &Config) -> anyhow::Result<()> {
    let lang = app.guild_lang(guild).await;
    pinchora_core::verification::validate(c, lang).map_err(anyhow::Error::msg)?;
    let roles = request(app, app.http.roles(Id::new(guild))).await?.models().await?;
    let a = request(app, app.http.guild_member(Id::new(guild), Id::new(actor)))
        .await?
        .model()
        .await?;
    let bot = request(app, app.http.guild_member(Id::new(guild), app.application_id.cast()))
        .await?
        .model()
        .await?;
    let g = request(app, app.http.guild(Id::new(guild))).await?.model().await?;
    let grantor = Grantor {
        owner: g.owner_id.get() == actor,
        administrator: powers(&roles, guild, &a.roles).contains(Permissions::ADMINISTRATOR),
        top_role: Some(top(&roles, &a.roles)),
    };
    for id in c
        .verification
        .verified_role_id
        .iter()
        .chain(&c.verification.unverified_role_id)
    {
        let role = roles
            .iter()
            .find(|r| r.id.to_string() == *id)
            .ok_or_else(|| anyhow::anyhow!(t!(lang, "verification.invalid")))?;
        if let Some(why) = settings::action_role_refusal_in(lang, facts(role, guild), grantor, &role.name)
            .or_else(|| settings::role_refusal_in(lang, facts(role, guild), Some(top(&roles, &bot.roles)), &role.name))
        {
            anyhow::bail!(why)
        }
    }
    for id in c
        .verification
        .channel_id
        .iter()
        .chain(&c.verification.log_channel_id)
        .chain(&c.raid.alert_channel_id)
        .chain(&c.raid.channels)
    {
        let channel = request(app, app.http.channel(Id::new(id.parse()?)))
            .await?
            .model()
            .await?;
        anyhow::ensure!(
            channel.guild_id.map(|g| g.get()) == Some(guild),
            t!(lang, "verification.invalid")
        );
    }
    Ok(())
}
pub(super) async fn role(app: &App, guild: u64, user: u64, id: &str, add: bool) -> anyhow::Result<()> {
    let lang = app.guild_lang(guild).await;
    let roles = request(app, app.http.roles(Id::new(guild))).await?.models().await?;
    let bot = request(app, app.http.guild_member(Id::new(guild), app.application_id.cast()))
        .await?
        .model()
        .await?;
    let permissions = powers(&roles, guild, &bot.roles);
    anyhow::ensure!(
        permissions.intersects(Permissions::ADMINISTRATOR | Permissions::MANAGE_ROLES),
        t!(
            lang,
            "verification.missing_permission",
            permission = t!(lang, "verification.perm.manage_roles")
        )
    );
    let r = roles
        .iter()
        .find(|r| r.id.to_string() == id)
        .ok_or_else(|| anyhow::anyhow!(t!(lang, "verification.invalid")))?;
    if let Some(why) = settings::run_time_refusal_in(lang, r.permissions.bits(), &r.name)
        .or_else(|| settings::role_refusal_in(lang, facts(r, guild), Some(top(&roles, &bot.roles)), &r.name))
    {
        anyhow::bail!(why)
    }
    if add {
        request(app, app.http.add_guild_member_role(Id::new(guild), Id::new(user), r.id)).await?;
    } else {
        request(
            app,
            app.http.remove_guild_member_role(Id::new(guild), Id::new(user), r.id),
        )
        .await?;
    }
    Ok(())
}
pub(super) async fn punish(
    app: &App,
    guild: u64,
    user: u64,
    c: &Config,
    ban: bool,
    actor: Option<u64>,
) -> anyhow::Result<bool> {
    let Some(m) = member(app, guild, user, c).await? else {
        return Ok(false);
    };
    let joined = m.joined_at.map(|t| t.as_micros());
    if app.db.raid_join(guild, user).await?.is_none_or(|j| {
        Some(j.joined_at.timestamp_micros()) != joined || matches!(j.status.as_str(), "verified" | "kicked" | "banned")
    }) {
        return Ok(false);
    }
    let lang = app.guild_lang(guild).await;
    let (needed, key, kind) = if ban {
        (Permissions::BAN_MEMBERS, "ban_members", CaseKind::Ban)
    } else {
        (Permissions::KICK_MEMBERS, "kick_members", CaseKind::Kick)
    };
    permission(app, guild, needed, key).await?;
    if let Some(actor) = actor {
        let roles = request(app, app.http.roles(Id::new(guild))).await?.models().await?;
        let a = request(app, app.http.guild_member(Id::new(guild), Id::new(actor)))
            .await?
            .model()
            .await?;
        let bot = request(app, app.http.guild_member(Id::new(guild), app.application_id.cast()))
            .await?
            .model()
            .await?;
        let g = request(app, app.http.guild(Id::new(guild))).await?.model().await?;
        anyhow::ensure!(
            g.owner_id.get() == actor
                || powers(&roles, guild, &a.roles).intersects(Permissions::ADMINISTRATOR | Permissions::MANAGE_GUILD),
            t!(lang, "verification.manage")
        );
        may_act(
            g.owner_id.get(),
            Rank {
                id: actor,
                top: top(&roles, &a.roles),
            },
            Rank {
                id: app.application_id.get(),
                top: top(&roles, &bot.roles),
            },
            Rank {
                id: user,
                top: top(&roles, &m.roles),
            },
        )
        .map_err(|_| anyhow::anyhow!(t!(lang, "verification.protected")))?;
    }
    let _ban = if ban {
        Some(app.db.lock_ban(guild, user).await?)
    } else {
        None
    };
    if ban {
        request(app, app.http.create_ban(Id::new(guild), Id::new(user))).await?;
    } else {
        request(app, app.http.remove_guild_member(Id::new(guild), Id::new(user))).await?;
    }
    moderation::record(
        app,
        guild,
        (actor.unwrap_or(app.application_id.get()), BOT_NAME),
        &Act {
            kind,
            target: user,
            target_name: m.user.name,
            reason: Some(t!(lang, "verification.reason")),
            duration: None,
            details: None,
        },
        if c.verification.dm_on_block { None } else { Some(false) },
    )
    .await?;
    app.db
        .raid_join_status(guild, user, if ban { "banned" } else { "kicked" })
        .await?;
    app.db.forget_verification(guild, user).await?;
    Ok(true)
}
pub(super) async fn log(
    app: &App,
    guild: u64,
    actor: u64,
    target: Option<u64>,
    action: &str,
    text: &str,
    components: Vec<Component>,
) -> anyhow::Result<()> {
    app.db
        .record_staff_action(&NewStaffAction {
            guild,
            actor,
            actor_name: BOT_NAME,
            source: "command",
            action,
            target,
            target_name: None,
            case_number: None,
            reason: Some(text),
            details: None,
        })
        .await?;
    let c = app.db.safety_config(guild).await?;
    let mut channels = Vec::new();
    if let Some(id) = c
        .raid
        .alert_channel_id
        .as_deref()
        .or(c.verification.log_channel_id.as_deref())
        .and_then(|v| v.parse::<u64>().ok())
    {
        channels.push(id)
    }
    if let Some(id) = app.db.mod_settings(guild).await?.log_channel_id {
        channels.push(id as u64)
    } else if let Some(id) = app.log_config(guild).await?.category_channel(LogCategory::Moderation) {
        channels.push(id)
    }
    channels.sort_unstable();
    channels.dedup();
    for channel in channels {
        request(
            app,
            app.http
                .create_message(Id::new(channel))
                .content(text)
                .components(&components)
                .allowed_mentions(Some(&AllowedMentions::default())),
        )
        .await?;
    }
    Ok(())
}

/// Shared across pods, including joins, moderation batches, role repairs and lockdown restoration.
pub(super) async fn request<T, E>(
    app: &App,
    request: impl std::future::IntoFuture<Output = Result<T, E>>,
) -> anyhow::Result<T>
where
    E: Into<anyhow::Error>,
{
    while !app.cache.verification_rest_slot(app.application_id.get()).await? {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    request.await.map_err(Into::into)
}
