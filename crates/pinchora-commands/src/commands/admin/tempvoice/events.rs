//! What the voice events do for temp voice: a member joining a creator gets a room, a member leaving the last
//! seat of a room takes it away, and a GUILD_CREATE puts the rooms right after a restart.

use std::{collections::HashSet, sync::Arc};

use pinchora_core::admin::modules::Module;
use pinchora_core::admin::temp_voice::EMPTY_GRACE_SECS;
use twilight_model::voice::VoiceState;

use crate::App;
use crate::commands::admin::tempvoice::control;
use crate::commands::admin::tempvoice::lifecycle;
use crate::commands::admin::tempvoice::rooms;
use crate::commands::admin::tempvoice::setups;
use crate::commands::admin::tempvoice::sweep;
use crate::commands::admin::tempvoice::track;

/// A voice state changed: `state` says where the member is now.
pub(crate) async fn on_voice_state(app: &Arc<App>, state: &VoiceState) {
    let Some(guild) = state.guild_id.map(|g| g.get()) else {
        return;
    };
    if let Err(error) = handle(app, guild, state).await {
        tracing::warn!(guild, user = state.user_id.get(), %error, "temp voice: a voice event was not handled");
    }
}

async fn handle(app: &Arc<App>, guild: u64, state: &VoiceState) -> anyhow::Result<()> {
    // A bot sitting in a room is not company: only people keep a room alive.
    if state.member.as_ref().is_some_and(|m| m.user.bot) {
        return Ok(());
    }
    let setup = setups::get(app, guild).await?;
    if setup.is_idle() {
        return Ok(());
    }
    let user = state.user_id.get();
    let to = state.channel_id.map(|c| c.get());
    let prior = track::place_of(app, guild, user).await?;
    let channels: Vec<u64> = [prior, to]
        .into_iter()
        .flatten()
        .filter(|c| setup.rooms.contains(c))
        .collect();
    let _room_fence = app.db.lock_temp_voice_rooms(&channels).await?;
    let from = track::moved(app, guild, user, to, &setup.rooms).await?;
    // Muting, deafening or streaming changes the state without changing the channel.
    if from == to {
        return Ok(());
    }
    if let Some(left) = from.filter(|c| setup.rooms.contains(c)) {
        left_room(app, left, user).await?;
    }
    let Some(to) = to else {
        return Ok(());
    };
    if let Some(creator) = setup.creators.get(&to) {
        // A creator switched off, or a server that switched the module off, makes no rooms; rooms already
        // there go on as they are and are still cleaned up.
        if creator.enabled && app.module_allowed(guild, None, Module::TempVoice).await {
            lifecycle::create_for(app, guild, creator, state).await?;
        }
    } else if setup.rooms.contains(&to) {
        joined_room(app, guild, to, user).await?;
    }
    Ok(())
}

/// A member left a room: the room goes when nobody is left in it.
async fn left_room(app: &Arc<App>, channel: u64, user: u64) -> anyhow::Result<()> {
    let Some(room) = app.db.temp_room(channel).await? else {
        return Ok(());
    };
    lifecycle::thread_member(app, &room, user, false).await;
    if track::members(app, channel).await?.is_empty() {
        sweep::note_empty(app, channel).await?;
    }
    Ok(())
}

/// A member came into a room: somebody the owner kicked is sent out again.
async fn joined_room(app: &Arc<App>, guild: u64, channel: u64, user: u64) -> anyhow::Result<()> {
    let Some(room) = app.db.temp_room(channel).await? else {
        return Ok(());
    };
    sweep::note_join(app, channel).await?;
    if app.db.is_banned_from_room(channel, user).await?
        && control::is_inside(app, &room, user).await
        && room.owner_id.is_some_and(|owner| owner > 0)
        && control::can_disconnect(app, guild, room.owner_id.unwrap() as u64, user).await
    {
        if let Err(fail) = rooms::move_member(app, guild, user, None).await {
            tracing::debug!(guild, user, ?fail, "temp voice: a kicked member was not sent out");
        }
        return Ok(());
    }
    lifecycle::thread_member(app, &room, user, true).await;
    Ok(())
}

/// A snapshot older than this is not used: the voice events since say more than it does.
const MAX_SNAPSHOT_AGE: i64 = 300;

/// Puts a server's rooms right with its GUILD_CREATE: who is in each room, the rooms whose channel is gone,
/// and the rooms nobody is in any more (the last member left while the bot was away). `voice` is who sits in
/// which voice channel (people only) and `channels` every channel and thread of the server, as of `seen`
/// (Unix seconds). Never fails the caller.
pub(crate) async fn reconcile(
    app: &App,
    guild: u64,
    seen: i64,
    voice: impl IntoIterator<Item = (u64, u64)>,
    channels: impl IntoIterator<Item = u64>,
) {
    if app.now().timestamp() - seen > MAX_SNAPSHOT_AGE {
        return;
    }
    let voice: Vec<(u64, u64)> = voice.into_iter().collect();
    let channels: HashSet<u64> = channels.into_iter().collect();
    if let Err(error) = put_right(app, guild, seen, &voice, &channels).await {
        tracing::warn!(guild, %error, "temp voice: the rooms were not put right");
    }
}

async fn put_right(
    app: &App,
    guild: u64,
    seen: i64,
    voice: &[(u64, u64)],
    channels: &HashSet<u64>,
) -> anyhow::Result<()> {
    if channels.is_empty() {
        return Ok(());
    }
    if !app.db.voice_active(guild).await? {
        return Ok(());
    }
    let rooms = app.db.temp_rooms(guild).await?;
    let now = app.now();
    for room in &rooms {
        let channel = room.channel_id as u64;
        let _room_fence = app.db.lock_temp_voice_rooms(&[channel]).await?;
        // A room made after the snapshot is absent from its channel list, but is not gone.
        if !channels.contains(&channel) && room.created_at.timestamp() < seen {
            lifecycle::forget_room(app, guild, channel).await;
            continue;
        }
        let inside: Vec<u64> = voice.iter().filter(|(_, c)| *c == channel).map(|(u, _)| *u).collect();
        // An event after GUILD_CREATE may have put somebody in a room while this snapshot waited in
        // the rebuild queue. Check Discord before replacing that newer presence with an empty list.
        if inside.is_empty()
            && !track::members(app, channel).await?.is_empty()
            && matches!(sweep::still_occupied(app, guild, channel).await?, Some(true) | None)
        {
            continue;
        }
        track::set_members(app, guild, channel, &inside).await?;
        if inside.is_empty() && (now - room.created_at).num_seconds() >= EMPTY_GRACE_SECS {
            lifecycle::delete_room(app, room).await;
        }
    }
    let in_rooms: HashSet<u64> = rooms.iter().map(|r| r.channel_id as u64).collect();
    for (user, channel) in voice.iter().filter(|(_, c)| !in_rooms.contains(c)) {
        track::note_place(app, guild, *user, *channel).await?;
    }
    setups::forget(guild).await;
    Ok(())
}
