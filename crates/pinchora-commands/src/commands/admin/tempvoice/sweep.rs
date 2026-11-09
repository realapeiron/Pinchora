//! The sweep: every half minute, rooms nobody is in are deleted, as a backstop for a leave the voice events
//! missed (an event dropped under load, a restart in between).

use std::{collections::HashSet, sync::Arc, time::Duration};

use pinchora_core::admin::temp_voice::EMPTY_GRACE_SECS;
use twilight_model::id::Id;

use crate::App;
use crate::commands::admin::tempvoice::lifecycle;
use crate::commands::admin::tempvoice::rooms;
use crate::commands::admin::tempvoice::track;
use crate::roles::roles;

/// The ticker wakes this often.
const WAKE: Duration = Duration::from_secs(30);
/// An empty room must have been seen empty by two rounds in a row, so a slow event cannot cost a room.
const SEEN_EMPTY: Duration = Duration::from_secs(150);
/// Rooms looked at per query.
const BATCH: i64 = 200;
/// Recheck a room's occupants at most this often when no event has changed its Redis set.
const VERIFY_AGAIN: Duration = Duration::from_secs(120);

fn marker(channel: u64) -> String {
    format!("tvoice:empty:{channel}")
}

/// A lost leave event leaves a stale Redis member. Ask Discord where each tracked member actually is;
/// on a Discord error keep the room, since an uncertain room must never be deleted.
pub(crate) async fn still_occupied(app: &App, guild: u64, channel: u64) -> anyhow::Result<Option<bool>> {
    let members = track::members(app, channel).await?;
    if members.is_empty() {
        return Ok(app.cache.get(&marker(channel)).await?.map(|_| false));
    }
    if app
        .cache
        .try_cooldown(&format!("tvoice:verify:{channel}"), VERIFY_AGAIN)
        .await?
        .is_some()
    {
        return Ok(Some(true));
    }
    let mut occupied = false;
    for user in members {
        let current = match app
            .http
            .user_voice_state(Id::new(guild.max(1)), Id::new(user.max(1)))
            .await
        {
            Ok(response) => match response.model().await {
                Ok(state) => state.channel_id.map(|id| id.get()),
                Err(error) => {
                    tracing::debug!(%error, guild, channel, user, "temp voice: a voice state was not read");
                    return Ok(None);
                }
            },
            Err(error) if matches!(roles::error_code(&error), Some(10065 | 10007)) => None,
            Err(error) => {
                tracing::debug!(%error, guild, channel, user, "temp voice: a voice state was not fetched");
                return Ok(None);
            }
        };
        if current == Some(channel) {
            occupied = true;
            continue;
        }
        track::remove_from_room(app, channel, user).await?;
        track::moved(app, guild, user, current, &HashSet::from([channel])).await?;
    }
    Ok(Some(occupied))
}

pub(crate) async fn note_empty(app: &App, channel: u64) -> anyhow::Result<()> {
    app.cache.put(&marker(channel), "1", SEEN_EMPTY).await
}

pub(crate) async fn note_join(app: &App, channel: u64) -> anyhow::Result<()> {
    app.cache.forget(&marker(channel)).await
}

/// One round: deletes the rooms that were empty last round too. Returns how many it deleted.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<usize> {
    let _sweep_fence = app.db.lock_temp_voice_sweep().await?;
    let older_than = app.now() - chrono::Duration::seconds(EMPTY_GRACE_SECS);
    for entry in app.cache.set_members("tvoice:pending").await? {
        let Some((kind, raw)) = entry.split_once(':') else {
            continue;
        };
        let Ok(channel) = raw.parse::<u64>() else {
            continue;
        };
        let created = app
            .cache
            .get(&format!("tvoice:pending-at:{entry}"))
            .await?
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        if created > app.now().timestamp() - 60 {
            continue;
        }
        let tracked = match kind {
            "channel" => app.db.temp_room(channel).await?.is_some(),
            "thread" => app.db.tracked_temp_voice_thread(channel).await?,
            _ => continue,
        };
        if tracked || rooms::delete_channel(app, channel).await.is_ok() {
            lifecycle::clear_pending(app, kind, channel).await;
        }
    }
    let (mut after, mut deleted) = (0u64, 0usize);
    loop {
        let batch = app.db.rooms_to_sweep(older_than, after, BATCH).await?;
        for room in &batch {
            let channel = room.channel_id as u64;
            after = channel;
            let _room_fence = app.db.lock_temp_voice_rooms(&[channel]).await?;
            match still_occupied(app, room.guild_id as u64, channel).await? {
                Some(true) | None => {
                    app.cache.forget(&marker(channel)).await?;
                    continue;
                }
                Some(false) => {}
            }
            if app.cache.get(&marker(channel)).await?.is_none() {
                app.cache.put(&marker(channel), "1", SEEN_EMPTY).await?;
                continue;
            }
            if lifecycle::delete_room(app, room).await {
                deleted += 1;
            }
        }
        if (batch.len() as i64) < BATCH {
            break;
        }
    }
    Ok(deleted)
}

/// Runs [`tick`] for the life of the process.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            wake.tick().await;
            match Box::pin(tick(&app)).await {
                Ok(0) => {}
                Ok(deleted) => tracing::debug!(deleted, "temp voice: empty rooms swept"),
                Err(error) => tracing::warn!(%error, "temp voice sweep failed"),
            }
        }
    });
}
