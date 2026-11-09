//! Audit-driven anti-nuke detection, with bounded gateway snapshots and delayed audit fallback.
mod recovery;
mod safety;
use crate::App;
pub use crate::moderation::anti_nuke::recovery::restore as restore_antinuke;
pub use crate::moderation::anti_nuke::recovery::restore_actor_roles as restore_antinuke_actor_roles;
pub use crate::moderation::anti_nuke::safety::authorize as authorize_antinuke;
pub use crate::moderation::anti_nuke::safety::authorize_module as authorize_antinuke_module;
pub use crate::moderation::anti_nuke::safety::save_config as save_antinuke_config;
pub(crate) use crate::moderation::anti_nuke::safety::vip_refusal;
use pinchora_core::admin::modules::Module;
use pinchora_core::infra::snowflake;
use pinchora_core::moderation::anti_nuke as rules;
use pinchora_core::moderation::anti_nuke::Config;
use pinchora_core::t;
use pinchora_db::AntiNukeEvent;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use twilight_model::{
    gateway::event::Event,
    guild::audit_log::{AuditLogChange, AuditLogEntry, AuditLogEventType as A},
    id::Id,
};

/// A single narrow gateway tap; existing moderation handlers keep their own delivery.
pub fn on_gateway(app: &Arc<App>, event: &Event) {
    if let Event::GuildCreate(e) = event {
        if let twilight_model::gateway::payload::incoming::GuildCreate::Available(g) = e.as_ref() {
            let guild = g.id.get();
            let a = app.clone();
            app.spawn_rebuild(async move {
                if let Err(error) = seed(&a, guild).await {
                    tracing::debug!(%error, guild, "anti-nuke snapshot seed failed");
                }
            });
        }
        return;
    }
    if !matches!(
        event,
        Event::GuildAuditLogEntryCreate(_)
            | Event::ChannelCreate(_)
            | Event::ChannelUpdate(_)
            | Event::ChannelDelete(_)
            | Event::RoleCreate(_)
            | Event::RoleUpdate(_)
            | Event::RoleDelete(_)
            | Event::BanAdd(_)
            | Event::MemberRemove(_)
            | Event::MemberAdd(_)
            | Event::MemberUpdate(_)
            | Event::WebhooksUpdate(_)
            | Event::GuildUpdate(_)
            | Event::GuildEmojisUpdate(_)
    ) {
        return;
    }
    let a = app.clone();
    let e = event.clone();
    app.spawn_event("anti_nuke", async move {
        if let Err(error) = gateway(&a, e).await {
            tracing::warn!(%error,"anti-nuke gateway handler failed");
        }
    });
}
async fn enabled(app: &App, guild: u64) -> anyhow::Result<Option<Config>> {
    // Cache the off switch, including absent rows. Trust/response are always read fresh.
    let active = app
        .antinuke_enabled
        .try_get_with(guild, async { app.db.antinuke_config(guild).await.map(|c| c.enabled) })
        .await
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;
    if !active || !app.guild_modules(guild).await?.rule(Module::AntiNuke).enabled {
        return Ok(None);
    }
    let c = app.db.antinuke_config(guild).await?;
    if !c.enabled {
        app.antinuke_enabled.insert(guild, false).await;
        return Ok(None);
    }
    Ok(Some(c))
}
async fn gateway(app: &Arc<App>, event: Event) -> anyhow::Result<()> {
    let (guild, kind, target) = match event {
        Event::GuildAuditLogEntryCreate(e) => return process(app, e.0).await,
        Event::ChannelCreate(e) => {
            let Some(g) = e.guild_id.map(|g| g.get()) else {
                return Ok(());
            };
            if enabled(app, g).await?.is_none() {
                return Ok(());
            }
            snapshot_channel(app, g, &e, false).await?;
            (g, Some(A::ChannelCreate), Some(e.id.get()))
        }
        Event::ChannelUpdate(e) => {
            let Some(g) = e.guild_id.map(|g| g.get()) else {
                return Ok(());
            };
            if enabled(app, g).await?.is_none() {
                return Ok(());
            }
            snapshot_channel(app, g, &e, false).await?;
            // Overwrite grants have their own audit action types, but share the channel target.
            (g, None, Some(e.id.get()))
        }
        Event::ChannelDelete(e) => {
            let Some(g) = e.guild_id.map(|g| g.get()) else {
                return Ok(());
            };
            if enabled(app, g).await?.is_none() {
                return Ok(());
            }
            snapshot_channel(app, g, &e, true).await?;
            (g, Some(A::ChannelDelete), Some(e.id.get()))
        }
        Event::RoleCreate(e) => {
            let g = e.guild_id.get();
            if enabled(app, g).await?.is_none() {
                return Ok(());
            }
            snapshot_role(app, g, &e.role).await?;
            (g, Some(A::RoleCreate), Some(e.role.id.get()))
        }
        Event::RoleUpdate(e) => {
            let g = e.guild_id.get();
            if enabled(app, g).await?.is_none() {
                return Ok(());
            }
            snapshot_role(app, g, &e.role).await?;
            (g, Some(A::RoleUpdate), Some(e.role.id.get()))
        }
        Event::RoleDelete(e) => (e.guild_id.get(), Some(A::RoleDelete), Some(e.role_id.get())),
        Event::BanAdd(e) => (e.guild_id.get(), Some(A::MemberBanAdd), Some(e.user.id.get())),
        Event::MemberRemove(e) => (e.guild_id.get(), Some(A::MemberKick), Some(e.user.id.get())),
        Event::MemberAdd(e) => {
            let g = e.guild_id.get();
            if enabled(app, g).await?.is_none() {
                return Ok(());
            }
            if app
                .db
                .antinuke_backup(g, e.user.id.get())
                .await?
                .is_some_and(|b| b.restore_on_join)
            {
                let guard = app.db.lock_antinuke(g).await?;
                let result =
                    crate::moderation::anti_nuke::recovery::restore_member(app, g, e.user.id.get(), None).await;
                if let Err(release_error) = guard.rollback().await {
                    tracing::warn!(%release_error, "member restore lock release failed");
                }
                result?;
            }
            remember_member(app, g, e.user.id.get(), &e.roles).await?;
            if !e.user.bot {
                return Ok(());
            }
            (g, Some(A::BotAdd), Some(e.user.id.get()))
        }
        Event::MemberUpdate(e) => {
            let g = e.guild_id.get();
            if enabled(app, g).await?.is_none() {
                return Ok(());
            }
            remember_member(app, g, e.user.id.get(), &e.roles).await?;
            // A timeout is MemberUpdate, while role grants/removals are MemberRoleUpdate.
            (g, None, Some(e.user.id.get()))
        }
        Event::WebhooksUpdate(e) => (e.guild_id.get(), Some(A::WebhookCreate), None),
        Event::GuildUpdate(e) => (e.id.get(), Some(A::GuildUpdate), None),
        Event::GuildEmojisUpdate(e) => (e.guild_id.get(), Some(A::EmojiDelete), None),
        _ => return Ok(()),
    };
    if enabled(app, guild).await?.is_none() {
        return Ok(());
    }
    // One guild-wide batch replaces a REST request for every gateway event. Entries carry
    // their own actor, action and target; never attribute a gateway target to the newest actor.
    let _ = (kind, target);
    if app
        .cache
        .try_cooldown(&format!("antinuke:audit-batch:{guild}"), Duration::from_secs(6))
        .await?
        .is_some()
    {
        return Ok(());
    }
    for delay in [0, 2, 3] {
        if delay > 0 {
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
        if enabled(app, guild).await?.is_none() {
            break;
        }
        if app
            .cache
            .try_cooldown(&format!("antinuke:audit-fetch:{guild}"), Duration::from_secs(2))
            .await?
            .is_some()
        {
            continue;
        }
        let log = app.http.audit_log(Id::new(guild)).limit(100).await?.model().await?;
        let mut entries = log.entries;
        entries.sort_by_key(|e| e.id);
        for mut entry in entries {
            entry.guild_id = Some(Id::new(guild));
            if let Err(error) = process(app, entry).await {
                tracing::warn!(%error, guild, "anti-nuke audit entry failed; continuing the batch");
            }
        }
    }
    Ok(())
}
async fn remember_member(
    app: &App,
    guild: u64,
    user: u64,
    roles: &[Id<twilight_model::id::marker::RoleMarker>],
) -> anyhow::Result<()> {
    let config = app.db.antinuke_config(guild).await?;
    if rules::vip(&config, roles.iter().map(|r| r.get())) || app.db.antinuke_backup(guild, user).await?.is_some() {
        app.db
            .backup_antinuke_roles(
                guild,
                user,
                &roles.iter().map(|r| r.get() as i64).collect::<Vec<_>>(),
                false,
            )
            .await?;
    }
    Ok(())
}
async fn snapshot_channel(
    app: &App,
    guild: u64,
    c: &twilight_model::channel::Channel,
    deleted: bool,
) -> anyhow::Result<()> {
    let mut data = serde_json::to_value(c)?;
    data["deletion_snapshot"] = json!(deleted);
    let mut users = vec![];
    if let Some(overwrites) = data["permission_overwrites"].as_array_mut() {
        overwrites.retain(|o| {
            if o["type"] == 1 {
                if let (Some(id), Some(allow), Some(deny)) = (
                    o["id"].as_str().and_then(|s| s.parse().ok()),
                    o["allow"].as_str(),
                    o["deny"].as_str(),
                ) {
                    users.push((id, allow.to_owned(), deny.to_owned()));
                }
                false
            } else {
                true
            }
        });
    } else {
        // Missing overwrite data is not evidence of an empty ACL.
        return Ok(());
    }
    // Never persist message content or resolved members in structural snapshots.
    data.as_object_mut().unwrap().remove("recipients");
    data.as_object_mut().unwrap().remove("member");
    app.db
        .save_antinuke_snapshot(guild, "channel", c.id.get(), &data, &users)
        .await
}
async fn snapshot_role(app: &App, guild: u64, r: &twilight_model::guild::Role) -> anyhow::Result<()> {
    app.db
        .save_antinuke_snapshot(guild, "role", r.id.get(), &serde_json::to_value(r)?, &[])
        .await
}
pub async fn seed(app: &App, guild: u64) -> anyhow::Result<()> {
    if enabled(app, guild).await?.is_none() {
        return Ok(());
    }
    app.cache
        .antinuke_activation(guild, app.now().timestamp_millis(), false)
        .await?;
    for role in app
        .http
        .roles(Id::new(guild))
        .await?
        .models()
        .await?
        .into_iter()
        .take(250)
    {
        snapshot_role(app, guild, &role).await?;
    }
    for channel in app
        .http
        .guild_channels(Id::new(guild))
        .await?
        .models()
        .await?
        .into_iter()
        .take(500)
    {
        snapshot_channel(app, guild, &channel, false).await?;
    }
    let c = app.db.antinuke_config(guild).await?;
    if !c.vip_roles.is_empty() {
        let mut after = None;
        for _ in 0..5 {
            let mut req = app.http.guild_members(Id::new(guild)).limit(1000);
            if let Some(id) = after {
                req = req.after(id);
            }
            let members = req.await?.models().await?;
            let len = members.len();
            for member in members {
                after = Some(member.user.id);
                remember_member(app, guild, member.user.id.get(), &member.roles).await?;
            }
            if len < 1000 {
                break;
            }
        }
    }
    Ok(())
}
async fn kind(app: &App, guild: u64, entry: &AuditLogEntry) -> anyhow::Result<Option<&'static str>> {
    let normal = match entry.action_type {
        A::ChannelDelete => "channel_delete",
        A::ChannelCreate => "channel_create",
        A::RoleDelete => "role_delete",
        A::RoleCreate => "role_create",
        A::MemberBanAdd => "ban",
        A::MemberKick | A::MemberPrune => "kick",
        A::WebhookCreate => "webhook_create",
        A::BotAdd => "bot_add",
        A::GuildUpdate => "guild_update",
        A::EmojiDelete => "emoji_delete",
        A::RoleUpdate | A::ChannelOverwriteCreate | A::ChannelOverwriteUpdate => {
            let grant = entry.changes.iter().any(|change| match change {
                AuditLogChange::Permissions { new: Some(new), old } | AuditLogChange::Allow { new: Some(new), old } => {
                    rules::dangerous_grant(old.map_or(0, |p| p.bits()), new.bits())
                }
                _ => false,
            });
            if !grant {
                return Ok(None);
            }
            "permission_grant"
        }
        A::MemberRoleUpdate => {
            // `$add` contains only newly granted roles; an unrelated nickname/role removal is not a grant.
            let changes = serde_json::to_value(&entry.changes)?;
            let roles = app.http.roles(Id::new(guild)).await?.models().await?;
            let grant = changes
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["key"] == "$add")
                .flat_map(|c| c["new_value"].as_array().into_iter().flatten())
                .any(|r| {
                    roles.iter().any(|known| {
                        r["id"].as_str() == Some(&known.id.to_string())
                            && rules::dangerous_grant(0, known.permissions.bits())
                    })
                });
            if grant {
                "permission_grant"
            } else if changes
                .as_array()
                .into_iter()
                .flatten()
                .any(|c| c["key"] == "$remove" && c["new_value"].as_array().is_some_and(|roles| !roles.is_empty()))
            {
                "vip_action"
            } else {
                return Ok(None);
            }
        }
        A::MemberUpdate
            if entry
                .changes
                .iter()
                .any(|c| matches!(c, AuditLogChange::CommunicationDisabledUntil { new: Some(_), .. })) =>
        {
            "vip_action"
        }
        _ => return Ok(None),
    };
    Ok(Some(normal))
}
/// Public for the simulator; the exact same path handles gateway and REST audit entries.
pub async fn process(app: &App, entry: AuditLogEntry) -> anyhow::Result<()> {
    let (Some(guild), Some(actor)) = (entry.guild_id.map(|n| n.get()), entry.user_id.map(|n| n.get())) else {
        return Ok(());
    };
    let Some(_) = enabled(app, guild).await? else {
        return Ok(());
    };
    let age = (app.now() - snowflake::timestamp(entry.id.get())).num_milliseconds();
    if !(0..=30_000).contains(&age) {
        return Ok(());
    }
    let mut guard = Some(app.db.lock_antinuke(guild).await?);
    let mut released = Ok(());
    let result = async {
        let config = app.db.antinuke_config(guild).await?;
        if !config.enabled {
            return Ok(());
        }
        let activated = app
            .cache
            .antinuke_activation(guild, app.now().timestamp_millis(), false)
            .await?;
        if snowflake::timestamp(entry.id.get()).timestamp_millis() <= activated {
            return Ok(());
        }
        let owner = crate::moderation::anti_nuke::safety::owner(app, guild).await?;
        if rules::exempt(&config, owner, app.application_id.get(), actor) {
            return Ok(());
        }
        let Some(kind) = kind(app, guild, &entry).await? else {
            return Ok(());
        };
        if crate::moderation::anti_nuke::safety::protected_actor(app, guild, actor, owner, &config).await? {
            return Ok(());
        }
        let Some(incident) = incident_target(app, guild, &entry, &config, kind).await? else {
            return Ok(());
        };
        let IncidentTarget {
            kind,
            user_target,
            is_vip,
            ..
        } = incident;
        let event = incident_event(guild, actor, &entry, &config, &incident);
        if !app.db.open_antinuke_event(&event).await? {
            return Ok(());
        }
        let limit = &config.limits[kind];
        let count = incident_count(app, &event, &entry, activated, limit.window_secs).await?;
        app.db.finish_antinuke_event(guild, event.id, count, "observed").await?;
        // A configured limit is the maximum allowed; VIP violations trigger immediately.
        if count <= limit.count && !is_vip {
            return Ok(());
        }
        let lang = app.guild_lang(guild).await;
        let action = match crate::moderation::anti_nuke::safety::punish(app, guild, actor, &config).await {
            Ok(a) => a,
            Err(e) => e.to_string(),
        };
        app.db.finish_antinuke_event(guild, event.id, count, &action).await?;
        let mut result = action;
        if config.remove_bots
            && kind == "bot_add"
            && let Some(bot) = user_target
        {
            match crate::moderation::anti_nuke::safety::remove_bot(app, guild, bot, &config).await {
                Ok(()) => result.push_str(&t!(lang, "anti_nuke.bot_removed")),
                Err(e) => result.push_str(&format!(" · {e}")),
            }
        }
        released = guard.take().unwrap().rollback().await;
        if (config.auto_restore && matches!(kind, "channel_delete" | "role_delete" | "ban"))
            || (kind == "vip_ban" && config.revert_vip_bans)
        {
            match crate::moderation::anti_nuke::recovery::restore(app, guild, event.id, app.application_id.get(), true)
                .await
            {
                Ok(text) => result.push_str(&format!(" · {text}")),
                Err(e) => result.push_str(&format!(" · {e}")),
            }
        }
        app.db.finish_antinuke_event(guild, event.id, count, &result).await?;
        let event = app.db.antinuke_event(guild, event.id).await?.unwrap();
        notify_detection(app, guild, actor, lang, kind, count).await?;
        crate::moderation::anti_nuke::safety::alert(app, &config, &event).await
    }
    .await;
    if let Some(guard) = guard {
        released = guard.rollback().await;
    }
    result?;
    released?;
    Ok(())
}
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(300));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Err(error) = tick_once(&app).await {
                tracing::warn!(%error,"anti-nuke snapshot maintenance failed");
            }
        }
    });
}
async fn tick_once(app: &App) -> anyhow::Result<()> {
    for guild in app.db.antinuke_guilds().await? {
        if app
            .cache
            .try_cooldown(&format!("antinuke:seed:{guild}"), Duration::from_secs(240))
            .await?
            .is_none()
            && let Err(error) = seed(app, guild as u64).await
        {
            tracing::warn!(%error,guild,"anti-nuke snapshot refresh failed");
        }
    }
    app.db.sweep_antinuke().await
}

struct IncidentTarget {
    kind: &'static str,
    target: Option<u64>,
    user_target: Option<u64>,
    is_vip: bool,
}

async fn incident_target(
    app: &App,
    guild: u64,
    entry: &AuditLogEntry,
    config: &rules::Config,
    mut kind: &'static str,
) -> anyhow::Result<Option<IncidentTarget>> {
    let target = entry.target_id.map(|n| n.get());
    let user_target = matches!(
        entry.action_type,
        A::MemberBanAdd | A::MemberKick | A::MemberRoleUpdate | A::MemberUpdate | A::BotAdd
    )
    .then_some(target)
    .flatten();
    let removed_vip = if entry.action_type == A::MemberRoleUpdate {
        let changes = serde_json::to_value(&entry.changes)?;
        changes
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["key"] == "$remove")
            .flat_map(|c| c["new_value"].as_array().into_iter().flatten())
            .any(|r| {
                r["id"]
                    .as_str()
                    .is_some_and(|id| config.vip_roles.iter().any(|v| v == id))
            })
    } else {
        false
    };
    let is_vip = removed_vip
        || if let Some(user) = user_target {
            crate::moderation::anti_nuke::safety::is_vip(app, guild, user, config).await?
        } else {
            false
        };
    if is_vip && kind == "ban" {
        kind = "vip_ban";
    }
    if kind == "vip_action" && !is_vip {
        return Ok(None);
    }
    Ok(Some(IncidentTarget {
        kind,
        target,
        user_target,
        is_vip,
    }))
}

fn incident_event(
    guild: u64,
    actor: u64,
    entry: &AuditLogEntry,
    config: &rules::Config,
    incident: &IncidentTarget,
) -> AntiNukeEvent {
    let IncidentTarget {
        kind,
        target,
        user_target,
        ..
    } = *incident;
    AntiNukeEvent {
        id: entry.id.get() as i64,
        guild_id: guild as i64,
        actor_id: actor as i64,
        target_user_id: user_target.map(|n| n as i64),
        target_id: if user_target.is_none() {
            target.map(|n| n as i64)
        } else {
            None
        },
        kind: kind.into(),
        count: 0,
        action_taken: "observed".into(),
        details: json!({"window_secs":config.limits[kind].window_secs,
        "deleted_permissions": entry.changes.iter().find_map(|change| match change {
            AuditLogChange::Permissions {old:Some(old), ..} if entry.action_type == A::RoleDelete => Some(old.bits().to_string()),
            _ => None,
        }),
        "deleted_position": entry.changes.iter().find_map(|change| match change {
            AuditLogChange::Position {old:Some(old), ..} if entry.action_type == A::RoleDelete => Some(*old),
            _ => None,
        })}),
        created_at: snowflake::timestamp(entry.id.get()),
        reverted: false,
        recovery_state: "ready".into(),
    }
}

async fn incident_count(
    app: &App,
    event: &AntiNukeEvent,
    entry: &AuditLogEntry,
    activated: i64,
    window_secs: u32,
) -> anyhow::Result<u32> {
    let guild = event.guild_id as u64;
    let actor = event.actor_id as u64;
    let kind = event.kind.as_str();
    app.cache
        .antinuke_window(
            guild,
            actor,
            kind,
            entry.id.get(),
            app.now().timestamp_millis(),
            snowflake::timestamp(entry.id.get()).timestamp_millis(),
            activated,
            window_secs,
            entry
                .options
                .as_ref()
                .and_then(|o| o.members_removed.as_deref())
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(1)
                .clamp(1, 1000),
        )
        .await
}

async fn notify_detection(
    app: &App,
    guild: u64,
    actor: u64,
    lang: pinchora_core::infra::i18n::Lang,
    kind: &str,
    count: u32,
) -> anyhow::Result<()> {
    crate::moderation::anti_nuke::safety::staff(
        app,
        guild,
        actor,
        "antinuke.detect",
        &t!(
            lang,
            "anti_nuke.detected",
            kind = t!(lang, &format!("anti_nuke.kind.{kind}")),
            count = pinchora_core::admin::server_stats::grouped_in(lang, count as i64)
        ),
    )
    .await
}
