use super::*;
use twilight_model::gateway::payload::incoming::*;

pub(super) fn interaction_create(app: &Arc<App>, interaction: InteractionCreate) {
    let interaction = interaction.0;
    let responder = Arc::new(Responder::rest(app.clone(), &interaction));
    app.spawn(handle_gateway_interaction(app.clone(), interaction, responder));
}

pub(super) fn message_create(app: &Arc<App>, message: Box<MessageCreate>) {
    let app2 = app.clone();
    app.spawn_event("message_create", async move {
        if let Err(error) = on_message(app2, *message).await {
            tracing::warn!(%error, "message handler failed");
        }
    });
}

pub(super) fn message_update(app: &Arc<App>, message: Box<MessageUpdate>) {
    let app2 = app.clone();
    app.spawn_event("message_update", async move {
        if let Err(error) = on_message_update(app2, *message).await {
            tracing::warn!(%error, "message edit handler failed");
        }
    });
}

pub(super) fn message_delete(app: &Arc<App>, event: MessageDelete) {
    let app2 = app.clone();
    app.spawn_event("message_delete", async move {
        if let Err(error) = on_message_delete(app2, event).await {
            tracing::warn!(%error, "message delete handler failed");
        }
    });
}

pub(super) fn message_delete_bulk(app: &Arc<App>, event: MessageDeleteBulk) {
    let app2 = app.clone();
    app.spawn_event("message_delete_bulk", async move {
        if let Err(error) = on_message_delete_bulk(app2, event).await {
            tracing::warn!(%error, "bulk delete handler failed");
        }
    });
}

pub(super) fn guild_audit_log_entry_create(app: &Arc<App>, entry: Box<GuildAuditLogEntryCreate>) {
    let app2 = app.clone();
    app.spawn_event("audit_log_entry", async move {
        if let Err(error) = on_audit_log_entry(app2, *entry).await {
            tracing::warn!(%error, "audit log handler failed");
        }
    });
}

pub(super) fn member_add(app: &Arc<App>, member: Box<MemberAdd>) {
    let app2 = app.clone();
    app.spawn_event("member_add", async move {
        if let Err(error) = on_member_add(app2, *member).await {
            tracing::warn!(%error, "welcome handler failed");
        }
    });
}

pub(super) fn member_remove(app: &Arc<App>, event: MemberRemove) {
    let app2 = app.clone();
    app.spawn_event("member_remove", async move {
        if let Err(error) = on_member_remove(app2, event).await {
            tracing::warn!(%error, "goodbye handler failed");
        }
    });
}

pub(super) fn member_update(app: &Arc<App>, event: Box<MemberUpdate>) {
    let app2 = app.clone();
    app.spawn_event("member_update", async move {
        if let Err(error) = on_member_update(app2, *event).await {
            tracing::warn!(%error, "member update handler failed");
        }
    });
}

pub(super) fn ban_add(app: &Arc<App>, event: BanAdd) {
    let app2 = app.clone();
    app.spawn_event("ban_add", async move {
        if let Err(error) = on_ban_add(app2, event).await {
            tracing::warn!(%error, "ban handler failed");
        }
    });
}

pub(super) fn ban_remove(app: &Arc<App>, event: BanRemove) {
    let app2 = app.clone();
    app.spawn_event("ban_remove", async move {
        if let Err(error) = on_ban_remove(app2, event).await {
            tracing::warn!(%error, "unban handler failed");
        }
    });
}

pub(super) fn voice_state_update(app: &Arc<App>, event: Box<VoiceStateUpdate>) {
    let app2 = app.clone();
    app.spawn_event("voice_state_update", async move {
        pinchora_commands::on_moderation_policy_voice(&app2, &event.0).await;
        if let Err(error) = on_voice_state_update(app2, *event).await {
            tracing::warn!(%error, "voice handler failed");
        }
    });
}

pub(super) fn guild_create(app: &Arc<App>, event: Box<GuildCreate>) {
    if let twilight_model::gateway::payload::incoming::GuildCreate::Available(ref guild) = *event {
        let epoch = app.begin_invite_baseline(guild.id.get());
        let app2 = app.clone();
        let guild_id = guild.id.get();
        app.spawn_rebuild(async move {
            for attempt in 0..3 {
                if !app2.invite_baseline_current(guild_id, epoch) { return; }
                match refresh_invite_snapshot(&app2, guild_id).await {
                    Ok(()) => { app2.finish_invite_baseline(guild_id, epoch); return; }
                    Err(error) => tracing::warn!(guild_id, attempt, %error, "invite snapshot failed; Manage Server permission is required"),
                }
                tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
            }
        });
    }
    // Everything the rebuild needs is taken here, and the guild is dropped: a storm of them at
    // identify must not sit in the event slots (a message or a member joining would be dropped)
    // holding every server's member list while the database answers.
    pinchora_commands::seed_moderation_policy_voice(app, &event);
    if let Some(snapshot) = voice_snapshot(app, *event) {
        let app2 = app.clone();
        app.spawn_rebuild(async move {
            let guild = snapshot.guild();
            if let Err(error) = rebuild_voice(&app2, snapshot).await {
                tracing::warn!(guild, %error, "guild create handler failed");
            }
        });
    }
}

pub(super) fn invite_create(app: &Arc<App>, event: Box<InviteCreate>) {
    let app2 = app.clone();
    let guild = event.guild_id.get();
    app.spawn_event("invite_create", async move {
        if let Err(error) = refresh_created_invite_snapshot(&app2, guild).await {
            tracing::warn!(guild, %error, "invite refresh failed");
        }
    });
}

pub(super) fn invite_delete(app: &Arc<App>, event: InviteDelete) {
    let app2 = app.clone();
    let guild = event.guild_id.get();
    app.spawn_event("invite_delete", async move {
        if let Err(error) = refresh_deleted_invite_snapshot(&app2, guild).await {
            tracing::warn!(guild, %error, "invite refresh failed");
        }
    });
}

pub(super) fn guild_delete(app: &Arc<App>, event: GuildDelete) {
    let app2 = app.clone();
    app.spawn_event("guild_delete", async move {
        if let Err(error) = on_guild_delete(app2, event).await {
            tracing::warn!(%error, "guild delete handler failed");
        }
    });
}

pub(super) fn auto_moderation_rule_create(app: &Arc<App>, event: AutoModerationRuleCreate) {
    pinchora_commands::on_automod_rule_change(app.clone(), event.guild_id.get())
}

pub(super) fn auto_moderation_rule_update(app: &Arc<App>, event: AutoModerationRuleUpdate) {
    pinchora_commands::on_automod_rule_change(app.clone(), event.guild_id.get())
}

pub(super) fn auto_moderation_rule_delete(app: &Arc<App>, event: AutoModerationRuleDelete) {
    pinchora_commands::on_automod_rule_change(app.clone(), event.guild_id.get())
}

pub(super) fn auto_moderation_action_execution(app: &Arc<App>, event: AutoModerationActionExecution) {
    let owned = app.clone();
    app.spawn(async move {
        if let Err(error) = pinchora_commands::on_automod_execution(owned, event).await {
            tracing::warn!(%error, "automod native event failed");
        }
    });
}

pub(super) fn guild_scheduled_event_update(app: &Arc<App>, event: Box<GuildScheduledEventUpdate>) {
    let owned = app.clone();
    app.spawn(async move {
        pinchora_commands::on_scheduled_event_update(owned, event).await;
    });
}

pub(super) fn channel_delete(app: &Arc<App>, event: Box<ChannelDelete>) {
    let guild = event.guild_id.map(|id| id.get());
    let place = event.id.get();
    forget(app, PermissionChange::Channel(place));
    if let Some(guild) = guild {
        let app2 = app.clone();
        app.spawn_event("ticket_channel_delete", async move {
            if let Err(error) = on_ticket_place_deleted(&app2, guild, place).await {
                tracing::warn!(guild, place, %error, "deleted ticket channel not closed");
            }
        });
    }
}

pub(super) fn thread_delete(app: &Arc<App>, event: ThreadDelete) {
    let (guild, place) = (event.guild_id.get(), event.id.get());
    forget(app, PermissionChange::Channel(place));
    let app2 = app.clone();
    app.spawn_event("ticket_thread_delete", async move {
        if let Err(error) = on_ticket_place_deleted(&app2, guild, place).await {
            tracing::warn!(guild, place, %error, "deleted ticket thread not closed");
        }
    });
}

pub(super) fn command_permissions_update(app: &Arc<App>, event: CommandPermissionsUpdate) {
    forget(app, PermissionChange::CommandPermissions(event.guild_id.get()));
}

pub(super) fn ready(shard: u32, ready: Ready) {
    tracing::info!(shard, guilds = ready.guilds.len(), "shard ready");
    metrics::gauge!("pinchora_shard_guilds", "shard" => shard.to_string()).set(ready.guilds.len() as f64);
}
