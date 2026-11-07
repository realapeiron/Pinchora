use super::safety;
use crate::App;
use pinchora_core::{settings, t};
use pinchora_db::AntiNukeEvent;
use twilight_http::request::AuditLogReason;
use twilight_model::{
    channel::{Channel, ChannelType, permission_overwrite::PermissionOverwriteType},
    guild::{Permissions, Role, RolePosition},
    id::Id,
};

/// Restore one incident window, scoped by guild and actor. A REST request with an uncertain outcome
/// is never automatically retried: the durable claim remains failed for staff inspection.
pub async fn restore(app: &App, guild: u64, id: i64, actor: u64, automatic: bool) -> anyhow::Result<String> {
    if !automatic {
        safety::authorize(app, guild, actor).await?;
    }
    let lang = app.guild_lang(guild).await;
    let _guard = app.db.lock_antinuke(guild).await?;
    let root = app
        .db
        .antinuke_event(guild, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!(t!(lang, "anti_nuke.not_found")))?;
    if !matches!(root.kind.as_str(), "channel_delete" | "role_delete" | "ban" | "vip_ban") {
        anyhow::bail!(t!(lang, "anti_nuke.not_restorable"));
    }
    if root.reverted || root.recovery_state != "ready" {
        anyhow::bail!(t!(lang, "anti_nuke.busy"));
    }
    let c = app.db.antinuke_config(guild).await?;
    if automatic
        && (!c.enabled
            || pinchora_core::anti_nuke::exempt(
                &c,
                safety::owner(app, guild).await?,
                app.application_id.get(),
                root.actor_id as u64,
            )
            || !(c.auto_restore || (root.kind == "vip_ban" && c.revert_vip_bans)))
    {
        anyhow::bail!(t!(lang, "anti_nuke.exempt"));
    }
    let window = c.limits[&root.kind].window_secs;
    let mut events = app
        .db
        .antinuke_recovery_window(guild, root.actor_id, &root.kind, root.created_at, window)
        .await?;
    // Discord requires categories before their children; role restoration precedes channel remapping.
    if root.kind == "channel_delete" {
        let mut ordered = vec![];
        for e in events {
            let category = if let Some(target) = e.target_id {
                app.db
                    .antinuke_snapshot(guild, "channel", target as u64)
                    .await?
                    .is_some_and(|s| s["type"] == 4)
            } else {
                false
            };
            ordered.push((!category, e));
        }
        ordered.sort_by_key(|(category, _)| *category);
        events = ordered.into_iter().map(|(_, e)| e).collect();
    }
    let mut done = 0;
    let mut failed = 0;
    let mut why = None;
    for event in events {
        if event.reverted || event.recovery_state != "ready" {
            continue;
        }
        // Preflight permissions before claiming: no side effects means the click can be retried.
        let permission = match event.kind.as_str() {
            "role_delete" => Permissions::MANAGE_ROLES,
            "channel_delete" => Permissions::MANAGE_CHANNELS,
            _ => Permissions::BAN_MEMBERS,
        };
        safety::require(app, guild, permission, lang).await?;
        if !app.db.claim_antinuke_restore(guild, event.id).await? {
            continue;
        }
        let mut attempted = false;
        let result = restore_one(app, guild, &event, (!automatic).then_some(actor), &mut attempted).await;
        if result.is_err() && !attempted {
            app.db.reset_antinuke_restore(guild, event.id).await?;
        } else {
            app.db.finish_antinuke_restore(guild, event.id, result.is_ok()).await?;
        }
        match result {
            Ok(()) => done += 1,
            Err(error) => {
                failed += 1;
                why = Some(error.to_string());
            }
        }
    }
    let text = t!(lang, "anti_nuke.restored", done = done, failed = failed);
    let text = why.map_or_else(|| text.clone(), |why| format!("{text} · {why}"));
    safety::staff(app, guild, actor, "antinuke.restore", &text).await?;
    Ok(text)
}
async fn restore_one(
    app: &App,
    guild: u64,
    e: &AntiNukeEvent,
    actor: Option<u64>,
    attempted: &mut bool,
) -> anyhow::Result<()> {
    let lang = app.guild_lang(guild).await;
    let reason = t!(lang, "anti_nuke.restore_reason");
    if matches!(e.kind.as_str(), "ban" | "vip_ban") {
        let user = e
            .target_user_id
            .ok_or_else(|| anyhow::anyhow!(t!(lang, "anti_nuke.not_restorable")))? as u64;
        if user == safety::owner(app, guild).await? {
            anyhow::bail!(t!(lang, "anti_nuke.exempt"));
        }
        // Hold Pinchora's ban lock while checking later audit entries. External Discord actions
        // cannot take this lock, so the final audit read is as close to the unban as possible.
        let _ban = app.db.lock_ban(guild, user).await?;
        let mut after = e.id as u64;
        for page in 0..10 {
            let log = app
                .http
                .audit_log(Id::new(guild))
                .action_type(twilight_model::guild::audit_log::AuditLogEventType::MemberBanAdd)
                .after(after)
                .limit(100)
                .await?
                .model()
                .await?;
            if log.entries.iter().any(|other| {
                other.target_id.map(|n| n.get()) == Some(user)
                    && other.id.get() > e.id as u64
                    && other.user_id.map(|n| n.get()) != Some(e.actor_id as u64)
            }) {
                anyhow::bail!(t!(lang, "anti_nuke.newer_ban"));
            }
            if log.entries.len() < 100 {
                break;
            }
            let next = log.entries.iter().map(|e| e.id.get()).max().unwrap_or(after);
            if next <= after || page == 9 {
                anyhow::bail!(t!(lang, "anti_nuke.audit_incomplete"));
            }
            after = next;
        }
        *attempted = true;
        match app.http.delete_ban(Id::new(guild), Id::new(user)).reason(&reason).await {
            Ok(_) => {}
            Err(e) if crate::roles::error_code(&e) == Some(10026) => {}
            Err(e) => return Err(e.into()),
        }
        if e.kind == "vip_ban" {
            app.db.antinuke_rejoin(guild, user, true).await?;
        }
        return Ok(());
    }
    let target = e
        .target_id
        .ok_or_else(|| anyhow::anyhow!(t!(lang, "anti_nuke.not_restorable")))? as u64;
    let kind = if e.kind == "role_delete" { "role" } else { "channel" };
    if app.db.antinuke_remap(guild, kind, target).await? != target {
        return Ok(());
    }
    let data = app
        .db
        .antinuke_snapshot(guild, kind, target)
        .await?
        .ok_or_else(|| anyhow::anyhow!(t!(lang, "anti_nuke.no_snapshot")))?;
    if kind == "role" {
        let role: Role = serde_json::from_value(data)?;
        let (roles, bot) = safety::require(app, guild, Permissions::MANAGE_ROLES, lang).await?;
        if role.id.get() == guild || role.managed || role.position >= safety::top(&roles, &bot) {
            anyhow::bail!(t!(lang, "anti_nuke.hierarchy"));
        }
        check_restore_role(app, guild, &role, &roles, &bot, actor).await?;
        *attempted = true;
        let restored = app
            .http
            .create_role(Id::new(guild))
            .name(&role.name)
            .permissions(role.permissions)
            .colors(role.colors)
            .hoist(role.hoist)
            .mentionable(role.mentionable)
            .reason(&reason)
            .await?
            .model()
            .await?;
        app.db
            .antinuke_restored_object(guild, "role", target, restored.id.get())
            .await?;
        app.http
            .update_role_positions(
                Id::new(guild),
                &[RolePosition {
                    id: restored.id,
                    position: role.position.max(0) as u64,
                }],
            )
            .reason(&reason)
            .await?;
    } else {
        let mut channel: Channel = serde_json::from_value(data)?;
        if !matches!(
            channel.kind,
            ChannelType::GuildText
                | ChannelType::GuildVoice
                | ChannelType::GuildCategory
                | ChannelType::GuildAnnouncement
                | ChannelType::GuildStageVoice
                | ChannelType::GuildForum
                | ChannelType::GuildMedia
        ) {
            anyhow::bail!(t!(lang, "anti_nuke.not_restorable"));
        }
        let overwrites = channel.permission_overwrites.get_or_insert_default();
        for overwrite in overwrites.iter_mut() {
            if overwrite.kind == PermissionOverwriteType::Role {
                overwrite.id = Id::new(app.db.antinuke_remap(guild, "role", overwrite.id.get()).await?);
            }
        }
        let restored_name = t!(lang, "anti_nuke.restored_channel");
        let mut request = app
            .http
            .create_guild_channel(Id::new(guild), channel.name.as_deref().unwrap_or(&restored_name))
            .kind(channel.kind)
            .permission_overwrites(overwrites)
            .position(channel.position.unwrap_or(0).max(0) as u64)
            .nsfw(channel.nsfw.unwrap_or(false))
            .reason(&reason);
        if let Some(parent) = channel.parent_id {
            request = request.parent_id(Id::new(app.db.antinuke_remap(guild, "channel", parent.get()).await?));
        }
        if let Some(topic) = &channel.topic {
            request = request.topic(topic);
        }
        if let Some(rate) = channel.rate_limit_per_user {
            request = request.rate_limit_per_user(rate);
        }
        if let Some(bitrate) = channel.bitrate {
            request = request.bitrate(bitrate);
        }
        if let Some(limit) = channel.user_limit {
            request = request.user_limit(limit as u16);
        }
        if let Some(region) = &channel.rtc_region {
            request = request.rtc_region(region);
        }
        if let Some(mode) = channel.video_quality_mode {
            request = request.video_quality_mode(mode);
        }
        if let Some(tags) = &channel.available_tags {
            request = request.available_tags(tags);
        }
        if let Some(layout) = channel.default_forum_layout {
            request = request.default_forum_layout(layout);
        }
        if let Some(duration) = channel.default_auto_archive_duration {
            request = request.default_auto_archive_duration(duration);
        }
        if let Some(order) = channel.default_sort_order {
            request = request.default_sort_order(order);
        }
        *attempted = true;
        let restored = request.await?.model().await?;
        app.db
            .antinuke_restored_object(guild, "channel", target, restored.id.get())
            .await?;
    }
    Ok(())
}
pub(super) async fn restore_member(app: &App, guild: u64, user: u64, actor: Option<u64>) -> anyhow::Result<()> {
    let lang = app.guild_lang(guild).await;
    if user == safety::owner(app, guild).await? || user == app.application_id.get() {
        anyhow::bail!(t!(lang, "anti_nuke.exempt"));
    }
    let backup = app
        .db
        .antinuke_backup(guild, user)
        .await?
        .ok_or_else(|| anyhow::anyhow!(t!(lang, "anti_nuke.no_snapshot")))?;
    let (roles, bot) = safety::require(app, guild, Permissions::MANAGE_ROLES, lang).await?;
    let member = app
        .http
        .guild_member(Id::new(guild), Id::new(user))
        .await?
        .model()
        .await?;
    if safety::top(&roles, &member) >= safety::top(&roles, &bot) {
        anyhow::bail!(t!(lang, "anti_nuke.hierarchy"));
    }
    let mut restore = vec![];
    for id in backup.role_ids {
        let id = app.db.antinuke_remap(guild, "role", id as u64).await?;
        let Some(role) = roles.iter().find(|r| r.id.get() == id) else {
            continue;
        };
        if role.managed {
            continue;
        }
        check_restore_role(app, guild, role, &roles, &bot, actor).await?;
        restore.push(role.id);
    }
    let c = app.db.antinuke_config(guild).await?;
    restore.extend(
        member
            .roles
            .iter()
            .filter(|id| c.quarantine_role_id.as_deref() != Some(&id.to_string()))
            .copied(),
    );
    restore.sort();
    restore.dedup();
    app.http
        .update_guild_member(Id::new(guild), Id::new(user))
        .roles(&restore)
        .reason(&t!(lang, "anti_nuke.restore_reason"))
        .await?;
    app.db.antinuke_rejoin(guild, user, false).await?;
    Ok(())
}
pub async fn restore_actor_roles(app: &App, guild: u64, id: i64, actor: u64) -> anyhow::Result<String> {
    safety::authorize(app, guild, actor).await?;
    let lang = app.guild_lang(guild).await;
    let _guard = app.db.lock_antinuke(guild).await?;
    let event = app
        .db
        .antinuke_event(guild, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!(t!(lang, "anti_nuke.not_found")))?;
    let backup = app
        .db
        .antinuke_backup(guild, event.actor_id as u64)
        .await?
        .ok_or_else(|| anyhow::anyhow!(t!(lang, "anti_nuke.no_snapshot")))?;
    if !backup.quarantined {
        anyhow::bail!(t!(lang, "anti_nuke.not_restorable"));
    }
    restore_member(app, guild, event.actor_id as u64, Some(actor)).await?;
    safety::staff(app, guild, actor, "antinuke.roles.restore", &t!(lang, "anti_nuke.done")).await?;
    Ok(t!(lang, "anti_nuke.done"))
}

async fn check_restore_role(
    app: &App,
    guild: u64,
    role: &Role,
    roles: &[Role],
    bot: &twilight_model::guild::Member,
    actor: Option<u64>,
) -> anyhow::Result<()> {
    let lang = app.guild_lang(guild).await;
    let why = if let Some(actor) = actor {
        let member = app
            .http
            .guild_member(Id::new(guild), Id::new(actor))
            .await?
            .model()
            .await?;
        settings::grant_refusal_in(
            lang,
            safety::facts(role, guild),
            settings::Grantor {
                owner: actor == safety::owner(app, guild).await?,
                administrator: safety::powers(roles, guild, &member).contains(Permissions::ADMINISTRATOR),
                top_role: Some(safety::top(roles, &member)),
            },
            &role.name,
        )
    } else {
        settings::run_time_refusal_in(lang, role.permissions.bits(), &role.name)
    };
    if let Some(why) = why.or_else(|| {
        settings::role_refusal_in(
            lang,
            safety::facts(role, guild),
            Some(safety::top(roles, bot)),
            &role.name,
        )
    }) {
        anyhow::bail!(why);
    }
    Ok(())
}
