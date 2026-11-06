use std::{sync::Arc, time::Duration};

use pinchora_cache::{RedisQueue, StoredSession};
use pinchora_commands::{
    App, Responder, handle_interaction, on_ban_add, on_ban_remove, on_guild_create, on_guild_delete, on_member_add,
    on_member_remove, on_member_update, on_message, on_voice_state_update,
};
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
    if config.gateway.member_events {
        intents |= Intents::GUILD_MEMBERS;
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
            app.spawn(handle_interaction(app.clone(), interaction, responder));
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
            let app2 = app.clone();
            app.spawn_event("guild_create", async move {
                if let Err(error) = on_guild_create(app2, *event).await {
                    tracing::warn!(%error, "guild create handler failed");
                }
            });
        }
        Event::GuildDelete(event) => {
            let app2 = app.clone();
            app.spawn_event("guild_delete", async move {
                if let Err(error) = on_guild_delete(app2, event).await {
                    tracing::warn!(%error, "guild delete handler failed");
                }
            });
        }
        Event::Ready(ready) => {
            tracing::info!(shard, guilds = ready.guilds.len(), "shard ready");
            metrics::gauge!("pinchora_shard_guilds", "shard" => shard.to_string()).set(ready.guilds.len() as f64);
        }
        Event::Resumed => tracing::info!(shard, "shard resumed"),
        _ => {}
    }
}
