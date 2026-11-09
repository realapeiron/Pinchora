use crate::App;
use pinchora_core::{
    anti_nuke::{self as rules, Config},
    i18n::Lang,
    settings::{self, Grantor, RoleFacts},
    t,
};
use pinchora_db::{AntiNukeEvent, NewStaffAction};
use twilight_http::request::AuditLogReason;
use twilight_model::{
    channel::message::{
        AllowedMentions, Component,
        component::{ActionRow, ButtonStyle},
    },
    guild::{Member, Permissions, Role},
    id::Id,
};
pub(super) async fn owner(app: &App, guild: u64) -> anyhow::Result<u64> {
    Ok(app.http.guild(Id::new(guild)).await?.model().await?.owner_id.get())
}
pub(super) fn top(roles: &[Role], member: &Member) -> i64 {
    roles
        .iter()
        .filter(|r| member.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0)
}
pub(super) fn powers(roles: &[Role], guild: u64, member: &Member) -> Permissions {
    roles
        .iter()
        .filter(|r| r.id.get() == guild || member.roles.contains(&r.id))
        .fold(Permissions::empty(), |p, r| p | r.permissions)
}
pub(super) fn facts(r: &Role, guild: u64) -> RoleFacts {
    RoleFacts {
        everyone: r.id.get() == guild,
        managed: r.managed,
        position: r.position,
        permissions: r.permissions.bits(),
    }
}
pub(super) async fn require(
    app: &App,
    guild: u64,
    needed: Permissions,
    lang: Lang,
) -> anyhow::Result<(Vec<Role>, Member)> {
    let roles = app.http.roles(Id::new(guild)).await?.models().await?;
    let bot = app
        .http
        .guild_member(Id::new(guild), app.application_id.cast())
        .await?
        .model()
        .await?;
    let held = powers(&roles, guild, &bot);
    if !held.contains(Permissions::ADMINISTRATOR) && !held.contains(needed) {
        let missing = needed - held;
        let names = missing
            .iter_names()
            .map(|(key, _)| t!(lang, &format!("anti_nuke.perm.{}", key.to_lowercase())))
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::bail!(t!(lang, "anti_nuke.permission", permission = names));
    }
    Ok((roles, bot))
}
/// Generic module switches must not bypass the owner's protection policy.
pub async fn authorize_module(app: &App, guild: u64, actor: u64) -> anyhow::Result<()> {
    if actor != owner(app, guild).await? {
        anyhow::bail!(t!(app.guild_lang(guild).await, "anti_nuke.owner_only"));
    }
    Ok(())
}
pub async fn authorize(app: &App, guild: u64, actor: u64) -> anyhow::Result<()> {
    let c = app.db.antinuke_config(guild).await?;
    let lang = app.guild_lang(guild).await;
    if !app
        .guild_modules(guild)
        .await?
        .rule(pinchora_core::modules::Module::AntiNuke)
        .enabled
    {
        anyhow::bail!(t!(lang, "anti_nuke.off"));
    }
    if actor != owner(app, guild).await? && !c.trusted_users.contains(&actor.to_string()) {
        anyhow::bail!(t!(lang, "anti_nuke.trusted_only"));
    }
    Ok(())
}
pub async fn save_config(app: &std::sync::Arc<App>, guild: u64, actor: u64, next: &Config) -> anyhow::Result<()> {
    let lang = app.guild_lang(guild).await;
    let mut guard = Some(app.db.lock_antinuke(guild).await?);
    let result = async {
        let old = app.db.antinuke_config(guild).await?;
        let owner = owner(app, guild).await?;
        rules::check_edit(&old, next, owner, actor, lang).map_err(anyhow::Error::msg)?;
        let (roles, bot) = require(
            app,
            guild,
            if next.enabled {
                Permissions::VIEW_AUDIT_LOG
            } else {
                Permissions::empty()
            },
            lang,
        )
        .await?;
        for id in &next.vip_roles {
            if !roles.iter().any(|r| r.id.to_string() == *id && r.id.get() != guild) {
                anyhow::bail!(t!(lang, "anti_nuke.invalid"));
            }
        }
        if let Some(id) = &next.quarantine_role_id {
            let role = roles
                .iter()
                .find(|r| r.id.to_string() == *id)
                .ok_or_else(|| anyhow::anyhow!(t!(lang, "anti_nuke.invalid")))?;
            let member = app
                .http
                .guild_member(Id::new(guild), Id::new(actor))
                .await?
                .model()
                .await?;
            let grantor = Grantor {
                owner: actor == owner,
                administrator: powers(&roles, guild, &member).contains(Permissions::ADMINISTRATOR),
                top_role: Some(top(&roles, &member)),
            };
            if next.vip_roles.contains(id)
                || role
                    .permissions
                    .intersects(Permissions::SEND_MESSAGES | Permissions::CONNECT)
            {
                anyhow::bail!(t!(lang, "anti_nuke.quarantine_unsafe"));
            }
            if let Some(why) = settings::action_role_refusal_in(lang, facts(role, guild), grantor, &role.name)
                .or_else(|| settings::role_refusal_in(lang, facts(role, guild), Some(top(&roles, &bot)), &role.name))
            {
                anyhow::bail!(why);
            }
        }
        if let Some(channel) = &next.log_channel_id {
            let ch = app.http.channel(Id::new(channel.parse()?)).await?.model().await?;
            if ch.guild_id.map(|g| g.get()) != Some(guild) {
                anyhow::bail!(t!(lang, "anti_nuke.invalid"));
            }
        }
        if next.enabled && !old.enabled {
            app.cache
                .antinuke_activation(guild, app.now().timestamp_millis(), true)
                .await?;
        }
        app.db.save_antinuke(guild, actor, next).await?;
        app.announce_guild_change(guild).await;
        staff(app, guild, actor, "antinuke.settings", &t!(lang, "anti_nuke.saved")).await?;
        guard.take().unwrap().rollback().await?;
        if next.enabled {
            let owned = app.clone();
            app.spawn_rebuild(async move {
                if let Err(error) = super::seed(&owned, guild).await {
                    tracing::warn!(%error,guild,"anti-nuke snapshot seed failed");
                }
            });
        }
        Ok(())
    }
    .await;
    if let Some(guard) = guard {
        guard.rollback().await?;
    }
    result
}
pub(super) async fn is_vip(app: &App, guild: u64, user: u64, c: &Config) -> anyhow::Result<bool> {
    if c.vip_roles.is_empty() {
        return Ok(false);
    }
    match app.http.guild_member(Id::new(guild), Id::new(user)).await {
        Ok(r) => Ok(rules::vip(c, r.model().await?.roles.iter().map(|r| r.get()))),
        Err(e) if crate::roles::error_code(&e) == Some(10007) => Ok(app
            .db
            .antinuke_backup(guild, user)
            .await?
            .is_some_and(|b| rules::vip(c, b.role_ids.iter().map(|r| *r as u64)))),
        Err(e) => Err(e.into()),
    }
}
pub(super) async fn punish(app: &App, guild: u64, actor: u64, c: &Config) -> anyhow::Result<String> {
    let lang = app.guild_lang(guild).await;
    let owner = owner(app, guild).await?;
    // Re-read trust immediately before any REST mutation, including when it changed after counting.
    let current = app.db.antinuke_config(guild).await?;
    if !current.enabled || rules::exempt(&current, owner, app.application_id.get(), actor) {
        return Ok(t!(lang, "anti_nuke.exempt"));
    }
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(actor))
        .await?
        .model()
        .await?;
    if member.user.system.unwrap_or(false)
        || (member.user.bot && protected_bot(app, guild, actor, owner, &current).await?)
    {
        return Ok(t!(lang, "anti_nuke.exempt"));
    }
    let mut effective = current.clone();
    // remove_bot explicitly requests a kick, but only while the live switch still allows it.
    if current.remove_bots && member.user.bot && c.remove_bots {
        effective.response = "kick".into();
    }
    let c = &effective;
    if c.response == "alert" {
        return Ok(t!(lang, "anti_nuke.response.alert"));
    }
    let needed = match c.response.as_str() {
        "quarantine" => Permissions::MANAGE_ROLES,
        "kick" => Permissions::KICK_MEMBERS,
        "ban" => Permissions::BAN_MEMBERS,
        _ => Permissions::empty(),
    };
    let (roles, bot) = require(app, guild, needed, lang).await?;
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(actor))
        .await?
        .model()
        .await?;
    if top(&roles, &member) >= top(&roles, &bot) {
        anyhow::bail!(t!(lang, "anti_nuke.hierarchy"));
    }
    let reason = t!(lang, "anti_nuke.reason");
    match c.response.as_str() {
        "quarantine" => {
            let id = c
                .quarantine_role_id
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or_else(|| anyhow::anyhow!(t!(lang, "anti_nuke.invalid")))?;
            let role = roles
                .iter()
                .find(|r| r.id.get() == id)
                .ok_or_else(|| anyhow::anyhow!(t!(lang, "anti_nuke.invalid")))?;
            if let Some(why) = settings::run_time_refusal_in(lang, role.permissions.bits(), &role.name)
                .or_else(|| settings::role_refusal_in(lang, facts(role, guild), Some(top(&roles, &bot)), &role.name))
            {
                anyhow::bail!(why);
            }
            if role
                .permissions
                .intersects(Permissions::SEND_MESSAGES | Permissions::CONNECT)
                || roles.iter().any(|r| {
                    (r.id.get() == guild || (r.managed && member.roles.contains(&r.id)))
                        && rules::dangerous_grant(0, r.permissions.bits())
                })
            {
                anyhow::bail!(t!(lang, "anti_nuke.quarantine_unsafe"));
            }
            app.db
                .backup_antinuke_roles(
                    guild,
                    actor,
                    &member.roles.iter().map(|r| r.get() as i64).collect::<Vec<_>>(),
                    true,
                )
                .await?;
            let kept = member
                .roles
                .iter()
                .filter(|id| roles.iter().any(|r| r.id == **id && r.managed))
                .copied()
                .chain([role.id])
                .collect::<Vec<_>>();
            app.http
                .update_guild_member(Id::new(guild), Id::new(actor))
                .roles(&kept)
                .reason(&reason)
                .await?;
        }
        "kick" => {
            app.http
                .remove_guild_member(Id::new(guild), Id::new(actor))
                .reason(&reason)
                .await?;
        }
        "ban" => {
            let _ban = app.db.lock_ban(guild, actor).await?;
            app.http
                .create_ban(Id::new(guild), Id::new(actor))
                .reason(&reason)
                .await?;
        }
        _ => {}
    }
    staff(app, guild, actor, "antinuke.response", &reason).await?;
    Ok(t!(lang, &format!("anti_nuke.response.{}", c.response)))
}
/// Resolve the current installation, not an old BotAdd audit after a removal/reinvite.
/// Discord may omit integration ownership or refuse this read; both fail closed.
async fn protected_bot(app: &App, guild: u64, user: u64, owner: u64, c: &Config) -> anyhow::Result<bool> {
    let integrations = app.http.guild_integrations(Id::new(guild)).await?.models().await?;
    let installer = integrations
        .iter()
        .find(|i| {
            i.application
                .as_ref()
                .is_some_and(|a| a.bot.as_ref().is_some_and(|b| b.id.get() == user))
        })
        .and_then(|i| i.user.as_ref())
        .map(|u| u.id.get());
    Ok(installer.is_none_or(|installer| rules::exempt(c, owner, app.application_id.get(), installer)))
}
pub(super) async fn protected_actor(app: &App, guild: u64, actor: u64, owner: u64, c: &Config) -> anyhow::Result<bool> {
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(actor))
        .await?
        .model()
        .await?;
    Ok(member.user.system.unwrap_or(false) || (member.user.bot && protected_bot(app, guild, actor, owner, c).await?))
}
pub(super) async fn remove_bot(app: &App, guild: u64, user: u64, c: &Config) -> anyhow::Result<()> {
    let mut c = c.clone();
    c.response = "kick".into();
    let lang = app.guild_lang(guild).await;
    if rules::exempt(&c, owner(app, guild).await?, app.application_id.get(), user) {
        anyhow::bail!(t!(lang, "anti_nuke.exempt"));
    }
    if !app
        .http
        .guild_member(Id::new(guild), Id::new(user))
        .await?
        .model()
        .await?
        .user
        .bot
    {
        anyhow::bail!(t!(lang, "anti_nuke.invalid"));
    }
    let outcome = punish(app, guild, user, &c).await?;
    if outcome != t!(lang, "anti_nuke.response.kick") {
        anyhow::bail!("{outcome}");
    }
    Ok(())
}
pub(super) async fn staff(app: &App, guild: u64, actor: u64, action: &str, details: &str) -> anyhow::Result<()> {
    app.db
        .record_staff_action(&NewStaffAction {
            guild,
            actor,
            actor_name: &actor.to_string(),
            source: "command",
            action,
            target: None,
            target_name: None,
            case_number: None,
            reason: None,
            details: Some(details),
        })
        .await
}
pub(super) async fn alert(app: &App, c: &Config, e: &AntiNukeEvent) -> anyhow::Result<()> {
    let guild = e.guild_id as u64;
    let lang = app.guild_lang(guild).await;
    let channel = match c.log_channel_id.as_deref().and_then(|s| s.parse::<u64>().ok()) {
        Some(c) => Some(c),
        None => app.db.mod_settings(guild).await?.log_channel_id.map(|n| n as u64),
    };
    let text = t!(
        lang,
        "anti_nuke.alert",
        actor = e.actor_id,
        kind = t!(lang, &format!("anti_nuke.kind.{}", e.kind)),
        count = pinchora_core::server_stats::grouped_in(lang, e.count as i64),
        action = crate::util::strip_mentions(&e.action_taken)
    );
    let actions = vec![Component::ActionRow(ActionRow {
        id: None,
        components: vec![
            crate::server_events::button(
                format!("an:undo:{}:{guild}", e.id),
                &t!(lang, "anti_nuke.undo"),
                None,
                ButtonStyle::Danger,
                false,
            ),
            crate::server_events::button(
                format!("vp:roles:{}:{guild}", e.id),
                &t!(lang, "anti_nuke.restore_roles"),
                None,
                ButtonStyle::Secondary,
                false,
            ),
        ],
    })];
    if let Some(channel) = channel {
        match app
            .http
            .create_message(Id::new(channel))
            .content(&text)
            .components(&actions)
            .allowed_mentions(Some(&AllowedMentions::default()))
            .await
        {
            Ok(_) => return Ok(()),
            Err(error) => tracing::warn!(%error,guild,"anti-nuke alert channel unavailable; trying the owner"),
        }
    }
    let owner = owner(app, guild).await?;
    let channel = app.http.create_private_channel(Id::new(owner)).await?.model().await?.id;
    app.http
        .create_message(channel)
        .content(&text)
        .components(&actions)
        .allowed_mentions(Some(&AllowedMentions::default()))
        .await?;
    Ok(())
}
/// Shared by moderation and automatic punishments; failure to read protection fails closed.
pub(crate) async fn vip_refusal(app: &App, guild: u64, user: u64) -> anyhow::Result<Option<String>> {
    let c = app.db.antinuke_config(guild).await?;
    Ok(if is_vip(app, guild, user, &c).await? {
        Some(t!(app.guild_lang(guild).await, "anti_nuke.vip_refusal"))
    } else {
        None
    })
}
