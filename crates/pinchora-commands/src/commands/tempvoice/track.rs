//! Who sits in which voice channel, as far as temp voice needs to know: for every human in a server that
//! uses temp voice, the channel they were last seen in, and for every temp room the members in it.
//!
//! The voice-state event carries the channel a member is in now but not the one they left, so the last one
//! is kept here, in Redis, where every pod sees it and a restart does not lose it. A server that does not use
//! temp voice is not tracked at all.

use std::{collections::HashSet, time::Duration};

use crate::App;

/// How long a member's place is kept without another voice event. GUILD_CREATE and the leave events keep it
/// right; this only stops a leave that was lost from sitting there for ever.
const KEEP: Duration = Duration::from_secs(2 * 86_400);

fn place_key(guild: u64, user: u64) -> String {
    format!("tvoice:at:{guild}:{user}")
}

fn room_key(channel: u64) -> String {
    format!("tvoice:in:{channel}")
}

/// Where `user` was last seen in voice in `guild`.
pub(crate) async fn place_of(app: &App, guild: u64, user: u64) -> anyhow::Result<Option<u64>> {
    Ok(app
        .cache
        .get(&place_key(guild, user))
        .await?
        .and_then(|raw| raw.parse().ok()))
}

/// Notes that `user` is now in `to` (`None`: in no voice channel) and returns where they were. A room's
/// member list changes only for the channels in `rooms`.
pub(crate) async fn moved(
    app: &App,
    guild: u64,
    user: u64,
    to: Option<u64>,
    rooms: &HashSet<u64>,
) -> anyhow::Result<Option<u64>> {
    let before = place_of(app, guild, user).await?;
    match to {
        Some(channel) => {
            app.cache
                .put(&place_key(guild, user), &channel.to_string(), KEEP)
                .await?;
        }
        None => app.cache.forget(&place_key(guild, user)).await?,
    }
    if let Some(left) = before.filter(|left| Some(*left) != to && rooms.contains(left)) {
        app.cache.set_remove(&room_key(left), &user.to_string()).await?;
    }
    if let Some(entered) = to.filter(|channel| rooms.contains(channel)) {
        app.cache.set_add(&room_key(entered), &user.to_string()).await?;
    }
    Ok(before)
}

/// The members in a temp room.
pub(crate) async fn members(app: &App, channel: u64) -> anyhow::Result<Vec<u64>> {
    let mut found: Vec<u64> = app
        .cache
        .set_members(&room_key(channel))
        .await?
        .iter()
        .filter_map(|raw| raw.parse().ok())
        .collect();
    found.sort_unstable();
    Ok(found)
}

pub(crate) async fn remove_from_room(app: &App, channel: u64, user: u64) -> anyhow::Result<()> {
    app.cache.set_remove(&room_key(channel), &user.to_string()).await
}

/// Makes `channel`'s member list exactly `now`, and notes where each of them is.
pub(crate) async fn set_members(app: &App, guild: u64, channel: u64, now: &[u64]) -> anyhow::Result<()> {
    let key = room_key(channel);
    let wanted: HashSet<String> = now.iter().map(u64::to_string).collect();
    for stale in app.cache.set_members(&key).await? {
        if !wanted.contains(&stale) {
            app.cache.set_remove(&key, &stale).await?;
        }
    }
    for user in now {
        app.cache.set_add(&key, &user.to_string()).await?;
        app.cache
            .put(&place_key(guild, *user), &channel.to_string(), KEEP)
            .await?;
    }
    Ok(())
}

/// Notes where `user` is without touching any room list, for the members a GUILD_CREATE found elsewhere.
pub(crate) async fn note_place(app: &App, guild: u64, user: u64, channel: u64) -> anyhow::Result<()> {
    app.cache.put(&place_key(guild, user), &channel.to_string(), KEEP).await
}

/// Forgets a room's member list, when the room is gone.
pub(crate) async fn forget_room(app: &App, channel: u64) {
    match app.cache.set_members(&room_key(channel)).await {
        Ok(left) => {
            for user in left {
                if let Err(error) = app.cache.set_remove(&room_key(channel), &user).await {
                    tracing::debug!(%error, channel, "a temp room's member was not forgotten");
                }
            }
        }
        Err(error) => tracing::debug!(%error, channel, "a temp room's members were not read"),
    }
}
