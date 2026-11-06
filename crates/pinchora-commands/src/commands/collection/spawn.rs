//! Chat-driven drops: counts the messages of a server that turned them on (never reading what they
//! say) and, once enough people have chatted enough, lets a critter drop or a bottle wash up.

use std::{sync::Arc, time::Duration};

use pinchora_game::collection::{BOTTLE_SHARE, COUNT_EVERY_SECS, SPAWN_CHATTERS, SPAWN_COOLDOWN_SECS, SPAWN_MESSAGES};

use super::{drops, settings::settings};
use crate::App;

const HOURS: Duration = Duration::from_secs(6 * 3600);

/// What one chat message does for the drops of its server. Cheap for servers that have them off.
///
/// A person's messages count at most once every [`COUNT_EVERY_SECS`], so one chatterbox cannot fill the
/// counter alone; a spawn needs [`SPAWN_MESSAGES`] counted messages from at least [`SPAWN_CHATTERS`]
/// people, and a server gets at most one every [`SPAWN_COOLDOWN_SECS`]. The counters live in Redis and the
/// last step is an atomic cooldown, so several pods agree on who spawns. A spawn Discord would not take (the
/// bot cannot post in that channel) gives the cooldown back, so it does not cost the server its next one.
pub(crate) async fn on_chat(app: &Arc<App>, guild: u64, user: u64, channel: u64) -> anyhow::Result<()> {
    let settings = settings(app, guild).await?;
    if !settings.enabled || settings.channel_id.is_some_and(|set| set as u64 != channel) {
        return Ok(());
    }
    let cache = &app.cache;
    if cache
        .try_cooldown(&format!("cdchat:{guild}:{user}"), Duration::from_secs(COUNT_EVERY_SECS))
        .await?
        .is_some()
    {
        return Ok(());
    }
    // A spawn starts a new cycle, which resets both counts at once.
    let cycle = cache.counter(&format!("cdcycle:{guild}")).await?;
    let counted = cache.incr(&format!("cdn:{guild}:{cycle}"), HOURS).await?;
    cache
        .map_set(&format!("cdwho:{guild}:{cycle}"), &user.to_string(), "1", HOURS)
        .await?;
    if counted < SPAWN_MESSAGES {
        return Ok(());
    }
    // Once the counter is past its threshold every counted message gets here: only the size is asked of
    // the set of chatters, never its members.
    if cache.map_len(&format!("cdwho:{guild}:{cycle}")).await? < SPAWN_CHATTERS {
        return Ok(());
    }
    let cooldown = format!("cdspawn:{guild}");
    if cache
        .try_cooldown(&cooldown, Duration::from_secs(SPAWN_COOLDOWN_SECS))
        .await?
        .is_some()
    {
        return Ok(());
    }
    cache
        .incr(&format!("cdcycle:{guild}"), Duration::from_secs(24 * 3600))
        .await?;
    let posted = if rand::random::<f64>() < BOTTLE_SHARE {
        drops::post_bottle(app, guild, channel).await
    } else {
        drops::post_critter_drop(app, guild, channel).await
    };
    metrics::counter!("pinchora_collection_spawns_total", "posted" => if matches!(posted, Ok(true)) { "yes" } else { "no" })
        .increment(1);
    if !matches!(posted, Ok(true)) {
        // Nothing landed, so nothing was spent. The counts start over (the cycle moved on), which keeps a
        // channel the bot cannot post in from being tried again on every message.
        if let Err(error) = cache.release_cooldown(&cooldown).await {
            tracing::debug!(%error, guild, "spawn cooldown not given back");
        }
    }
    posted.map(|_| ())
}
