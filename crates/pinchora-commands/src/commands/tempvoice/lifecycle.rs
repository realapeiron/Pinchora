//! A room's life: made when a member joins a creator, deleted when the last one leaves.

use std::{collections::HashSet, sync::Arc, time::Duration};

use pinchora_core::{
    t,
    temp_voice::{MAX_ROOMS, owner_label, room_name},
};
use pinchora_db::{NewTempRoom, RoomMake, TempRoom, VoiceCreator};
use twilight_model::{
    channel::{ChannelType, message::AllowedMentions},
    guild::Permissions,
    id::Id,
    voice::VoiceState,
};

use super::{
    lang_of, panel,
    rooms::{self, Edit, Fail, Op},
    setups, track,
};
use crate::App;

/// How long a member must wait between two rooms they asked for.
const MAKE_COOLDOWN: Duration = Duration::from_secs(4);
/// How long a notice in a creator's chat stays.
const NOTICE_KEPT: Duration = Duration::from_secs(30);

/// A member's name as the room should be called after them: their nickname, else their display name, else
/// their username.
fn owner_name(state: &VoiceState, lang: pinchora_core::i18n::Lang) -> String {
    let fallback = t!(lang, "voice.fallback-owner");
    let Some(member) = &state.member else {
        return owner_label("", &fallback);
    };
    let name = member
        .nick
        .clone()
        .or_else(|| member.user.global_name.clone())
        .unwrap_or_else(|| member.user.name.clone());
    owner_label(&name, &fallback)
}

/// Says why a room could not be made in the creator's own chat, to the member only by mention, and
/// takes it away again soon. Best effort: the creator's chat may not take messages.
async fn notice(app: &App, creator: u64, user: u64, text: String) {
    let mentions = AllowedMentions::default();
    let sent = app
        .http
        .create_message(Id::new(creator.max(1)))
        .content(&format!("<@{user}> {text}"))
        .allowed_mentions(Some(&mentions))
        .await;
    match sent {
        Ok(response) => {
            if let Ok(message) = response.model().await {
                app.delete_later(message.channel_id, message.id, NOTICE_KEPT);
            }
        }
        Err(error) => tracing::debug!(%error, creator, "temp voice: a notice was not posted"),
    }
}

/// Makes a room for the member who just joined `creator`, and moves them into it. A member who already owns
/// as many rooms from this creator as it allows is taken back to their oldest one instead.
pub(crate) async fn create_for(
    app: &Arc<App>,
    guild: u64,
    creator: &VoiceCreator,
    state: &VoiceState,
) -> anyhow::Result<()> {
    let user = state.user_id.get();
    let source = creator.channel_id as u64;
    if app
        .cache
        .try_cooldown(&format!("tvoice:make:{guild}:{user}"), MAKE_COOLDOWN)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let lang = lang_of(app, guild).await;
    let rooms_now = app.db.temp_rooms(guild).await?;
    let mine: Vec<&TempRoom> = rooms_now
        .iter()
        .filter(|r| r.source_id == creator.channel_id && r.owner_id == Some(user as i64))
        .collect();
    if mine.len() >= creator.max_per_user.max(1) as usize {
        let own = mine[0].channel_id as u64;
        match rooms::move_member(app, guild, user, Some(own)).await {
            Ok(()) => return Ok(()),
            // Their room is gone from Discord already: forget it and make a fresh one below.
            Err(Fail::Gone) => forget_room(app, guild, own).await,
            Err(other) => {
                notice(app, source, user, other.text(lang)).await;
                return Ok(());
            }
        }
    }
    if rooms_now.len() as i64 >= MAX_ROOMS {
        notice(app, source, user, t!(lang, "voice.too-many-rooms", count = MAX_ROOMS)).await;
        return Ok(());
    }

    let number = rooms_now.iter().filter(|r| r.source_id == creator.channel_id).count() + 1;
    let name = room_name(&creator.name_template, &owner_name(state, lang), number);

    // The room starts as the category is (a private category stays private) with its owner let in.
    let mut parent = creator.category_id.map(|c| c as u64);
    let mut list = Vec::new();
    if let Some(category) = parent {
        match rooms::overwrites_of(app, category, rooms::VIEW_CHANNEL).await {
            Ok(found) => list = found,
            Err(Fail::Gone) => parent = None,
            Err(other) => {
                notice(app, source, user, other.text(lang)).await;
                return Ok(());
            }
        }
    }
    let mut edits = vec![Edit::member(user, Op::Allow, rooms::entry_bits())];
    if creator.default_locked {
        edits.push(Edit::role(guild, Op::Deny, Permissions::CONNECT));
        edits.push(Edit::member(app.application_id.get(), Op::Allow, rooms::entry_bits()));
    }
    for edit in edits {
        rooms::edit(&mut list, edit.id, edit.member, |o| edit.apply(o));
    }

    let channel = match rooms::make_channel(app, guild, &name, parent, creator.default_limit, &list).await {
        Ok(channel) => channel,
        Err(other) => {
            tracing::warn!(guild, creator = source, ?other, "temp voice: a room was not made");
            notice(app, source, user, other.text(lang)).await;
            return Ok(());
        }
    };
    let room = match app
        .db
        .create_temp_room(NewTempRoom {
            channel_id: channel,
            guild_id: guild,
            source_id: source,
            owner_id: user,
            locked: creator.default_locked,
            user_limit: creator.default_limit,
        })
        .await?
    {
        RoomMake::Made(room) => room,
        RoomMake::Full => {
            let _ = rooms::delete_channel(app, channel).await;
            notice(app, source, user, t!(lang, "voice.too-many-rooms", count = MAX_ROOMS)).await;
            return Ok(());
        }
    };
    // The room is known (and its owner in it) before the member is moved, so the voice event that the move
    // causes finds everything in place.
    setups::forget(guild).await;
    track::moved(app, guild, user, Some(channel), &HashSet::from([channel])).await?;
    if let Err(fail) = rooms::move_member(app, guild, user, Some(channel)).await {
        // They left the creator meanwhile, or the bot may not move members: no room for nobody.
        tracing::debug!(guild, user, ?fail, "temp voice: the member was not moved");
        delete_room(app, &room).await;
        let _ = track::moved(app, guild, user, None, &HashSet::new()).await;
        if !matches!(fail, Fail::NotInVoice | Fail::Gone) {
            notice(app, source, user, fail.text(lang)).await;
        }
        return Ok(());
    }
    metrics::counter!("pinchora_temp_rooms_made_total").increment(1);
    if creator.panel || creator.panel_channel_id.is_some() {
        attach_panel(app, lang, creator, room, &name).await;
    }
    Ok(())
}

/// Gives the new room its panel, and its paired thread when the creator has one.
async fn attach_panel(app: &App, lang: pinchora_core::i18n::Lang, creator: &VoiceCreator, room: TempRoom, name: &str) {
    let channel = room.channel_id as u64;
    let mut chat = channel;
    let mut thread = None;
    if let Some(parent) = creator.panel_channel_id {
        match make_thread(app, parent as u64, name, room.owner_id.map(|o| o as u64)).await {
            Some(id) => {
                chat = id;
                thread = Some(id);
            }
            None => tracing::debug!(
                channel,
                "temp voice: no paired thread; the panel goes in the room's chat"
            ),
        }
    }
    let message = if creator.panel {
        panel::post(app, lang, &room, chat).await
    } else {
        None
    };
    if thread.is_some() || message.is_some() {
        let saved = app
            .db
            .set_room_panel(channel, thread, message.map(|_| chat), message)
            .await;
        if let Err(error) = saved {
            tracing::warn!(%error, channel, "temp voice: the panel was not recorded");
        }
    }
}

/// A private thread under `parent` for a room's chat, with its owner in it.
async fn make_thread(app: &App, parent: u64, name: &str, owner: Option<u64>) -> Option<u64> {
    let made = match app
        .http
        .create_thread(Id::new(parent.max(1)), name, ChannelType::PrivateThread)
        .invitable(false)
        .await
    {
        Ok(response) => response.model().await.ok()?,
        Err(error) => {
            tracing::debug!(%error, parent, "temp voice: the thread was not made");
            return None;
        }
    };
    if let Some(owner) = owner {
        let _ = app.http.add_thread_member(made.id, Id::new(owner.max(1))).await;
    }
    Some(made.id.get())
}

/// Adds a member to (or takes them out of) a room's paired thread, if it has one. Best effort.
pub(crate) async fn thread_member(app: &App, room: &TempRoom, user: u64, add: bool) {
    let Some(thread) = room.text_channel_id else {
        return;
    };
    let (thread, user) = (Id::new(thread as u64), Id::new(user.max(1)));
    let done = if add {
        app.http.add_thread_member(thread, user).await.map(|_| ())
    } else {
        app.http.remove_thread_member(thread, user).await.map(|_| ())
    };
    if let Err(error) = done {
        tracing::debug!(%error, "temp voice: a thread member was not changed");
    }
}

/// Deletes a room's channel (and its thread) from Discord and forgets the room. If Discord refuses the
/// channel deletion, keep the database row so the sweep can try again instead of leaving an untracked room.
pub(crate) async fn delete_room(app: &App, room: &TempRoom) -> bool {
    let channel = room.channel_id as u64;
    if let Some(thread) = room.text_channel_id
        && let Err(fail) = rooms::delete_channel(app, thread as u64).await
    {
        tracing::debug!(?fail, thread, "temp voice: a thread was not deleted");
    }
    if let Err(fail) = rooms::delete_channel(app, channel).await {
        tracing::warn!(
            guild = room.guild_id,
            channel,
            ?fail,
            "temp voice: a room was not deleted; the sweep will retry"
        );
        return false;
    }
    forget_room(app, room.guild_id as u64, channel).await;
    metrics::counter!("pinchora_temp_rooms_deleted_total").increment(1);
    true
}

/// Forgets a room that no longer exists on Discord (or that is given up on).
pub(crate) async fn forget_room(app: &App, guild: u64, channel: u64) {
    if let Err(error) = app.db.delete_temp_room(channel).await {
        tracing::warn!(%error, channel, "temp voice: a room was not forgotten");
    }
    track::forget_room(app, channel).await;
    let _ = app.cache.forget(&format!("tvoice:empty:{channel}")).await;
    setups::forget(guild).await;
}
