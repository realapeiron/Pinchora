use std::{sync::Arc, time::Duration};

use pinchora_cache::{RedisQueue, StoredSession};
use pinchora_commands::{
    App, PermissionChange, Responder, handle_gateway_interaction, on_ban_add, on_ban_remove, on_guild_delete,
    on_member_add, on_member_remove, on_member_update, on_message, on_permissions_changed, on_voice_state_update,
    rebuild_voice, voice_snapshot,
};
use pinchora_commands::{ReactionEvent, on_reaction};
use pinchora_core::{
    Config, cluster,
    telemetry::{self, Readiness},
};
use tokio::sync::watch;
use twilight_gateway::{
    CloseFrame, ConfigBuilder, Event, EventTypeFlags, Intents, Session, Shard, ShardId, StreamExt as _,
};
use twilight_model::gateway::{
    payload::outgoing::update_presence::UpdatePresencePayload,
    presence::{ActivityType, MinimalActivity, Status},
};

/// Discord keeps a closed session resumable for a short while; a rolling restart
/// must come back within this window to skip a full identify.
const SESSION_TTL: Duration = Duration::from_secs(240);

fn wanted_events() -> EventTypeFlags {
    // Only events we handle get deserialized. GUILD_CREATE is the costly one (the whole server in one
    // payload, once per guild per identify); it is here for the voice states it carries, which put
    // voice XP right after a restart. Gateway close events always come through.
    EventTypeFlags::INTERACTION_CREATE
        | EventTypeFlags::MESSAGE_CREATE
        | EventTypeFlags::MEMBER_ADD
        | EventTypeFlags::MEMBER_REMOVE
        | EventTypeFlags::MEMBER_UPDATE
        | EventTypeFlags::BAN_ADD
        | EventTypeFlags::BAN_REMOVE
        | EventTypeFlags::VOICE_STATE_UPDATE
        | EventTypeFlags::GUILD_CREATE
        | EventTypeFlags::GUILD_DELETE
        | EventTypeFlags::REACTION_ADD
        | EventTypeFlags::REACTION_REMOVE
        | EventTypeFlags::REACTION_REMOVE_ALL
        | EventTypeFlags::REACTION_REMOVE_EMOJI
        // What text commands judge a member's permissions by: the server's owner and roles, a
        // channel's overwrites and the Integrations settings. Cheap, and they come under the
        // Guilds intent (the last one with no intent at all).
        | EventTypeFlags::GUILD_UPDATE
        | EventTypeFlags::ROLE_CREATE
        | EventTypeFlags::ROLE_UPDATE
        | EventTypeFlags::ROLE_DELETE
        | EventTypeFlags::CHANNEL_UPDATE
        | EventTypeFlags::CHANNEL_DELETE
        | EventTypeFlags::COMMAND_PERMISSIONS_UPDATE
        | EventTypeFlags::READY
        | EventTypeFlags::RESUMED
}

pub async fn run(config: &Config, app: Arc<App>, ready: Readiness) -> anyhow::Result<()> {
    let info = app.http.gateway().authed().await?.model().await?;
    let limit = info.session_start_limit;
    let cluster_count = config.gateway.cluster_count.max(1);
    let total = match config.gateway.shards {
        Some(total) => total,
        None if cluster_count == 1 => cluster::round_shards(info.shards, limit.max_concurrency),
        None => anyhow::bail!("set gateway.shards when running more than one cluster so every pod agrees on the total"),
    };
    let cluster_id = cluster::resolve_cluster_id(config.gateway.cluster_id);
    anyhow::ensure!(
        cluster_id < cluster_count,
        "cluster {cluster_id} is outside 0..{cluster_count}"
    );
    let ids: Vec<ShardId> = twilight_gateway::bucket(cluster_id, cluster_count, total).collect();
    anyhow::ensure!(
        limit.remaining as usize >= ids.len(),
        "only {} session starts left today (resets in {} s); refusing to start {} shards",
        limit.remaining,
        limit.reset_after / 1000,
        ids.len()
    );
    tracing::info!(
        cluster_id,
        cluster_count,
        total,
        first = ids.first().map(|s| s.number()),
        last = ids.last().map(|s| s.number()),
        max_concurrency = limit.max_concurrency,
        "starting shard cluster"
    );

    // Bans and voice states feed the server log; neither intent is privileged.
    let mut intents =
        Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::GUILD_MODERATION | Intents::GUILD_VOICE_STATES;
    // Reaction roles and the starboard listen to reactions; not privileged either.
    intents |= Intents::GUILD_MESSAGE_REACTIONS;
    if config.gateway.member_events {
        intents |= Intents::GUILD_MEMBERS;
    }
    // Keyword auto-responders and text commands read chat; the intent is privileged, so it is off until
    // the operator turns it on. Without it, Discord only sends the text of messages that mention the bot:
    // text commands then need `@Pinchora profile` instead of `p!profile`, and keyword auto-responders
    // stay deaf (`auto::chat` checks the same flag).
    if config.gateway.message_content {
        intents |= Intents::MESSAGE_CONTENT;
    }
    let presence = UpdatePresencePayload::new(
        vec![
            MinimalActivity {
                kind: ActivityType::Playing,
                name: "/help · pinchora".into(),
                url: None,
            }
            .into(),
        ],
        false,
        None,
        Status::Online,
    )?;
    let base = ConfigBuilder::new(config.discord.token.expose().to_owned(), intents)
        .presence(presence)
        .queue(app.cache.identify_queue(limit.max_concurrency))
        .build();

    let (stop, stopped) = watch::channel(false);
    let mut tasks = Vec::with_capacity(ids.len());
    for id in ids {
        let mut builder = ConfigBuilder::from(base.clone());
        match app.cache.take_session(id.number()).await {
            Ok(Some(saved)) if saved.intents != intents.bits() => {
                tracing::info!(shard = id.number(), "intents changed; identifying instead of resuming");
            }
            Ok(Some(saved)) => {
                tracing::info!(shard = id.number(), "resuming saved session");
                builder = builder.session(Session::new(saved.sequence, saved.session_id));
                if let Some(url) = saved.resume_url {
                    builder = builder.resume_url(url);
                }
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(shard = id.number(), %error, "could not load saved session"),
        }
        let shard = Shard::with_config(id, builder.build());
        tasks.push(tokio::spawn(run_shard(shard, app.clone(), stopped.clone())));
    }
    ready.set(true);

    telemetry::shutdown_signal().await;
    ready.set(false);
    tracing::info!("shutdown requested; closing shards with RESUME");
    let _ = stop.send(true);
    for task in tasks {
        match tokio::time::timeout(Duration::from_secs(10), task).await {
            Ok(Ok((shard, Some(session)))) => {
                if let Err(error) = app.cache.store_session(shard, &session, SESSION_TTL).await {
                    tracing::warn!(shard, %error, "could not save session");
                }
            }
            Ok(Ok((shard, None))) => tracing::debug!(shard, "no session to save"),
            Ok(Err(error)) => tracing::error!(%error, "shard task panicked"),
            Err(_) => tracing::warn!("shard did not close in time"),
        }
    }
    app.drain(Duration::from_secs(10)).await;
    tracing::info!("gateway stopped");
    Ok(())
}

/// Owns one shard: reads events until shutdown, then returns what is needed to resume.
async fn run_shard(
    mut shard: Shard<RedisQueue>,
    app: Arc<App>,
    mut stopped: watch::Receiver<bool>,
) -> (u32, Option<StoredSession>) {
    let id = shard.id().number();
    let flags = wanted_events();
    let mut closing = false;
    loop {
        tokio::select! {
            changed = stopped.changed(), if !closing => {
                if changed.is_err() || *stopped.borrow() {
                    shard.close(CloseFrame::RESUME);
                    closing = true;
                }
            }
            item = shard.next_event(flags) => match item {
                None => break,
                Some(Ok(Event::GatewayClose(_))) if closing => break,
                Some(Ok(event)) => dispatch(&app, id, event),
                Some(Err(error)) => tracing::warn!(shard = id, %error, "gateway receive error"),
            },
        }
    }
    let session = shard.session().map(|s| StoredSession {
        session_id: s.id().to_owned(),
        sequence: s.sequence(),
        resume_url: shard.resume_url().map(ToOwned::to_owned),
        intents: shard.config().intents().bits(),
    });
    (id, session)
}

fn dispatch(app: &Arc<App>, shard: u32, event: Event) {
    if let Some(name) = event.kind().name() {
        metrics::counter!("pinchora_gateway_events_total", "event" => name).increment(1);
    }
    match event {
        Event::InteractionCreate(interaction) => {
            let interaction = interaction.0;
            let responder = Arc::new(Responder::rest(app.clone(), &interaction));
            app.spawn(handle_gateway_interaction(app.clone(), interaction, responder));
        }
        Event::MessageCreate(message) => {
            let app2 = app.clone();
            app.spawn_event("message_create", async move {
                if let Err(error) = on_message(app2, *message).await {
                    tracing::warn!(%error, "message handler failed");
                }
            });
        }
        Event::MemberAdd(member) => {
            let app2 = app.clone();
            app.spawn_event("member_add", async move {
                if let Err(error) = on_member_add(app2, *member).await {
                    tracing::warn!(%error, "welcome handler failed");
                }
            });
        }
        Event::MemberRemove(event) => {
            let app2 = app.clone();
            app.spawn_event("member_remove", async move {
                if let Err(error) = on_member_remove(app2, event).await {
                    tracing::warn!(%error, "goodbye handler failed");
                }
            });
        }
        Event::MemberUpdate(event) => {
            let app2 = app.clone();
            app.spawn_event("member_update", async move {
                if let Err(error) = on_member_update(app2, *event).await {
                    tracing::warn!(%error, "member update handler failed");
                }
            });
        }
        Event::BanAdd(event) => {
            let app2 = app.clone();
            app.spawn_event("ban_add", async move {
                if let Err(error) = on_ban_add(app2, event).await {
                    tracing::warn!(%error, "ban handler failed");
                }
            });
        }
        Event::BanRemove(event) => {
            let app2 = app.clone();
            app.spawn_event("ban_remove", async move {
                if let Err(error) = on_ban_remove(app2, event).await {
                    tracing::warn!(%error, "unban handler failed");
                }
            });
        }
        Event::VoiceStateUpdate(event) => {
            let app2 = app.clone();
            app.spawn_event("voice_state_update", async move {
                if let Err(error) = on_voice_state_update(app2, *event).await {
                    tracing::warn!(%error, "voice handler failed");
                }
            });
        }
        Event::GuildCreate(event) => {
            // Everything the rebuild needs is taken here, and the guild is dropped: a storm of them at
            // identify must not sit in the event slots (a message or a member joining would be dropped)
            // holding every server's member list while the database answers.
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
        Event::GuildDelete(event) => {
            let app2 = app.clone();
            app.spawn_event("guild_delete", async move {
                if let Err(error) = on_guild_delete(app2, event).await {
                    tracing::warn!(%error, "guild delete handler failed");
                }
            });
        }
        Event::ReactionAdd(event) => reacted(app, ReactionEvent::Add(event)),
        Event::ReactionRemove(event) => reacted(app, ReactionEvent::Remove(event)),
        Event::ReactionRemoveAll(event) => reacted(app, ReactionEvent::RemoveAll(event)),
        Event::ReactionRemoveEmoji(event) => reacted(app, ReactionEvent::RemoveEmoji(event)),
        Event::GuildUpdate(event) => forget(app, PermissionChange::Guild(event.id.get())),
        Event::RoleCreate(event) => forget(app, PermissionChange::Guild(event.guild_id.get())),
        Event::RoleUpdate(event) => forget(app, PermissionChange::Guild(event.guild_id.get())),
        Event::RoleDelete(event) => forget(app, PermissionChange::Guild(event.guild_id.get())),
        Event::ChannelUpdate(event) => forget(app, PermissionChange::Channel(event.id.get())),
        Event::ChannelDelete(event) => forget(app, PermissionChange::Channel(event.id.get())),
        Event::CommandPermissionsUpdate(event) => {
            forget(app, PermissionChange::CommandPermissions(event.guild_id.get()));
        }
        Event::Ready(ready) => {
            tracing::info!(shard, guilds = ready.guilds.len(), "shard ready");
            metrics::gauge!("pinchora_shard_guilds", "shard" => shard.to_string()).set(ready.guilds.len() as f64);
        }
        Event::Resumed => tracing::info!(shard, "shard resumed"),
        _ => {}
    }
}

/// Drops what text commands cached about a permission that changed. Not a droppable event like a
/// chat message: a missed one would leave the old permissions for up to a minute (five for command
/// permissions).
fn forget(app: &Arc<App>, change: PermissionChange) {
    let app2 = app.clone();
    app.spawn(async move { on_permissions_changed(&app2, change).await });
}

/// Hands a reaction change to the feature packages. Droppable like a chat message: a reaction storm
/// must not use every event slot.
fn reacted(app: &Arc<App>, event: ReactionEvent) {
    let app2 = app.clone();
    app.spawn_event("reaction", async move {
        if let Err(error) = on_reaction(app2, event).await {
            tracing::warn!(%error, "reaction handler failed");
        }
    });
}
