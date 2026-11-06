//! What the owner of a room can do to it, whether they type `/voice …` or press a panel button: one function
//! per action, each answering with the sentence to tell the member.

use std::time::Duration;

use pinchora_core::{
    i18n::Lang,
    t,
    temp_voice::{RENAME_COOLDOWN_SECS, check_limit, clean_name},
};
use pinchora_db::TempRoom;
use twilight_model::{guild::Permissions, id::Id};

use super::{
    panel,
    rooms::{self, Edit, Fail, Op},
    track,
};
use crate::App;

/// Who is asking.
pub(crate) struct Who {
    pub user: u64,
    /// Whether they may manage the server: they may run any room they can reach, owner or not.
    pub manager: bool,
    pub lang: Lang,
}

/// Something to do to a room.
#[derive(Debug, Clone)]
pub(crate) enum Act {
    Lock,
    Unlock,
    Hide,
    Show,
    Limit(i64),
    Rename(String),
    Kick { user: u64, bot: bool },
    Transfer(u64),
    Invite(u64),
    Claim,
}

fn rename_key(channel: u64) -> String {
    format!("vc:rn:{channel}")
}

/// The room a member means: the one whose chat they pressed a button in (`here`), else the one they sit in,
/// else the oldest they own.
pub(crate) async fn resolve(app: &App, guild: u64, user: u64, here: Option<u64>) -> anyhow::Result<Option<TempRoom>> {
    if let Some(here) = here
        && let Some(room) = app.db.temp_room_of(guild, here).await?
    {
        return Ok(Some(room));
    }
    if let Some(place) = track::place_of(app, guild, user).await?
        && let Some(room) = app.db.temp_room(place).await?.filter(|r| r.guild_id as u64 == guild)
    {
        return Ok(Some(room));
    }
    Ok(app.db.rooms_owned_by(guild, user).await?.into_iter().next())
}

/// Whether `user` is in the room: as far as the voice events say, and failing that as Discord says.
pub(crate) async fn is_inside(app: &App, room: &TempRoom, user: u64) -> bool {
    let channel = room.channel_id as u64;
    if track::members(app, channel).await.is_ok_and(|m| m.contains(&user)) {
        return true;
    }
    match app
        .http
        .user_voice_state(Id::new((room.guild_id as u64).max(1)), Id::new(user.max(1)))
        .await
    {
        Ok(response) => response
            .model()
            .await
            .is_ok_and(|state| state.channel_id.is_some_and(|c| c.get() == channel)),
        Err(_) => false,
    }
}

fn not_owner(lang: Lang, room: &TempRoom) -> String {
    match room.owner_id {
        Some(owner) => t!(lang, "voice.not-owner", owner = format!("<@{owner}>")),
        None => t!(lang, "voice.no-owner"),
    }
}

/// Runs `act` on `room` for `who`.
pub(crate) async fn act(app: &App, who: &Who, room: TempRoom, act: Act) -> anyhow::Result<String> {
    if matches!(act, Act::Claim) {
        return claim(app, who, room).await;
    }
    if room.owner_id != Some(who.user as i64) && !who.manager {
        return Ok(not_owner(who.lang, &room));
    }
    let lang = who.lang;
    let (guild, channel) = (room.guild_id as u64, room.channel_id as u64);
    let bot = app.application_id.get();
    match act {
        Act::Lock => {
            let edits = [
                Edit::role(guild, Op::Deny, Permissions::CONNECT),
                Edit::member(bot, Op::Allow, rooms::entry_bits()),
            ];
            if let Err(fail) = rooms::apply(app, channel, &edits).await {
                return Ok(fail.text(lang));
            }
            app.db.set_room_state(channel, Some(true), None, None).await?;
            finished(app, channel, t!(lang, "voice.locked")).await
        }
        Act::Unlock => {
            let op = match category_says(app, channel, guild, Permissions::CONNECT).await {
                Ok(op) => op,
                Err(fail) => return Ok(fail.text(lang)),
            };
            if let Err(fail) = rooms::apply(app, channel, &[Edit::role(guild, op, Permissions::CONNECT)]).await {
                return Ok(fail.text(lang));
            }
            app.db.set_room_state(channel, Some(false), None, None).await?;
            finished(app, channel, t!(lang, "voice.unlocked")).await
        }
        Act::Hide => {
            let edits = [
                Edit::role(guild, Op::Deny, Permissions::VIEW_CHANNEL),
                Edit::member(bot, Op::Allow, rooms::entry_bits()),
            ];
            if let Err(fail) = rooms::apply(app, channel, &edits).await {
                return Ok(fail.text(lang));
            }
            app.db.set_room_state(channel, None, Some(true), None).await?;
            finished(app, channel, t!(lang, "voice.hidden")).await
        }
        Act::Show => {
            let op = match category_says(app, channel, guild, Permissions::VIEW_CHANNEL).await {
                Ok(op) => op,
                Err(fail) => return Ok(fail.text(lang)),
            };
            if let Err(fail) = rooms::apply(app, channel, &[Edit::role(guild, op, Permissions::VIEW_CHANNEL)]).await {
                return Ok(fail.text(lang));
            }
            app.db.set_room_state(channel, None, Some(false), None).await?;
            finished(app, channel, t!(lang, "voice.shown")).await
        }
        Act::Limit(wanted) => {
            let limit = match check_limit(wanted) {
                Ok(limit) => limit,
                Err(_) => return Ok(t!(lang, "voice.limit-nan")),
            };
            if let Err(fail) = rooms::edit_channel(app, channel, None, Some(limit)).await {
                return Ok(fail.text(lang));
            }
            app.db.set_room_state(channel, None, None, Some(limit)).await?;
            let text = if limit == 0 {
                t!(lang, "voice.limit-none")
            } else {
                t!(lang, "voice.limit-set", count = limit)
            };
            finished(app, channel, text).await
        }
        Act::Rename(raw) => rename(app, who, &room, &raw).await,
        Act::Kick { user, bot: is_bot } => kick(app, who, &room, user, is_bot).await,
        Act::Transfer(target) => transfer(app, who, room, target).await,
        Act::Invite(target) => {
            app.db.unban_from_room(channel, target).await?;
            let edit = Edit::member(target, Op::Allow, rooms::entry_bits());
            // Without Manage Roles the invite still lifts a kick; a locked room just stays shut to them.
            let note = match rooms::apply(app, channel, &[edit]).await {
                Ok(()) => String::new(),
                Err(fail) => format!(" {}", fail.text(lang)),
            };
            Ok(format!(
                "{}{note}",
                t!(lang, "voice.invited", user = format!("<@{target}>"))
            ))
        }
        Act::Claim => claim(app, who, room).await,
    }
}

/// What the room's category says about `@everyone` and `bit`: unlocking or showing the room goes back to it.
async fn category_says(app: &App, channel: u64, guild: u64, bit: Permissions) -> Result<Op, Fail> {
    let Some(parent) = rooms::parent_of(app, channel).await? else {
        return Ok(Op::Clear);
    };
    let list = rooms::overwrites_of(app, parent, rooms::VIEW_CHANNEL).await?;
    Ok(rooms::default_op(&list, guild, bit))
}

/// Says `text`, after the panel has been made to show the room as it is now.
async fn finished(app: &App, channel: u64, text: String) -> anyhow::Result<String> {
    if let Some(room) = app.db.temp_room(channel).await? {
        panel::refresh(app, &room).await;
    }
    Ok(text)
}

/// When the room may be renamed again, as Unix seconds, if it was renamed lately.
pub(crate) async fn rename_wait(app: &App, channel: u64) -> anyhow::Result<Option<i64>> {
    let now = app.now().timestamp();
    let cached = app
        .cache
        .get(&rename_key(channel))
        .await?
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|until| *until > now);
    if cached.is_some() {
        return Ok(cached);
    }
    Ok(app
        .db
        .temp_room(channel)
        .await?
        .and_then(|room| room.last_rename_at)
        .map(|renamed| renamed.timestamp() + RENAME_COOLDOWN_SECS as i64)
        .filter(|until| *until > now))
}

async fn rename(app: &App, who: &Who, room: &TempRoom, raw: &str) -> anyhow::Result<String> {
    let lang = who.lang;
    let channel = room.channel_id as u64;
    let now = app.now().timestamp();
    if let Some(until) = rename_wait(app, channel).await? {
        return Ok(t!(lang, "voice.rename-wait", when = format!("<t:{until}:R>")));
    }
    let Some(name) = clean_name(raw) else {
        return Ok(t!(lang, "voice.rename-empty"));
    };
    if let Err(fail) = rooms::edit_channel(app, channel, Some(&name), None).await {
        return Ok(fail.text(lang));
    }
    app.db.mark_room_renamed(channel, app.now()).await?;
    let until = now + RENAME_COOLDOWN_SECS as i64;
    app.cache
        .put(
            &rename_key(channel),
            &until.to_string(),
            Duration::from_secs(RENAME_COOLDOWN_SECS),
        )
        .await?;
    finished(
        app,
        channel,
        t!(lang, "voice.renamed", name = crate::custom::scrub_value(&name)),
    )
    .await
}

async fn kick(app: &App, who: &Who, room: &TempRoom, user: u64, is_bot: bool) -> anyhow::Result<String> {
    let lang = who.lang;
    let channel = room.channel_id as u64;
    if user == who.user {
        return Ok(t!(lang, "voice.kick-self"));
    }
    if room.owner_id == Some(user as i64) {
        return Ok(t!(lang, "voice.kick-owner"));
    }
    app.db.ban_from_room(channel, user).await?;
    // Discord keeps them out of a locked room; this keeps them out of an open one. Without Manage Roles
    // the ban list alone does it: the bot sends them out again whenever they come back.
    let edit = Edit::member(user, Op::Deny, Permissions::CONNECT);
    if let Err(fail) = rooms::apply(app, channel, &[edit]).await {
        tracing::debug!(
            channel,
            ?fail,
            "temp voice: the kicked member's overwrite was not written"
        );
    }
    let there = is_bot || is_inside(app, room, user).await;
    if there
        && let Err(fail) = rooms::move_member(app, room.guild_id as u64, user, None).await
        && !matches!(fail, Fail::NotInVoice | Fail::Gone)
    {
        return Ok(fail.text(lang));
    }
    Ok(t!(lang, "voice.kicked", user = format!("<@{user}>")))
}

async fn transfer(app: &App, who: &Who, room: TempRoom, target: u64) -> anyhow::Result<String> {
    let lang = who.lang;
    let channel = room.channel_id as u64;
    if room.owner_id == Some(target as i64) {
        return Ok(t!(lang, "voice.transfer-already"));
    }
    if !is_inside(app, &room, target).await {
        return Ok(t!(lang, "voice.transfer-absent", user = format!("<@{target}>")));
    }
    if !app
        .db
        .move_room_owner(channel, room.owner_id.map(|o| o as u64), target)
        .await?
    {
        return Ok(t!(lang, "voice.owner-changed"));
    }
    welcome_owner(app, channel, target).await;
    finished(
        app,
        channel,
        t!(lang, "voice.transferred", user = format!("<@{target}>")),
    )
    .await
}

async fn claim(app: &App, who: &Who, room: TempRoom) -> anyhow::Result<String> {
    let lang = who.lang;
    let channel = room.channel_id as u64;
    if room.owner_id == Some(who.user as i64) {
        return Ok(t!(lang, "voice.claim-own"));
    }
    if !is_inside(app, &room, who.user).await {
        return Ok(t!(lang, "voice.claim-outside"));
    }
    if let Some(owner) = room.owner_id
        && is_inside(app, &room, owner as u64).await
    {
        return Ok(t!(lang, "voice.claim-owner-here", owner = format!("<@{owner}>")));
    }
    if !app
        .db
        .move_room_owner(channel, room.owner_id.map(|o| o as u64), who.user)
        .await?
    {
        return Ok(t!(lang, "voice.owner-changed"));
    }
    welcome_owner(app, channel, who.user).await;
    finished(app, channel, t!(lang, "voice.claimed")).await
}

/// The new owner may always come in, and is no longer kept out.
async fn welcome_owner(app: &App, channel: u64, user: u64) {
    if let Err(error) = app.db.unban_from_room(channel, user).await {
        tracing::debug!(%error, channel, "temp voice: a new owner was not let back in");
    }
    let edit = Edit::member(user, Op::Allow, rooms::entry_bits());
    if let Err(fail) = rooms::apply(app, channel, &[edit]).await {
        tracing::debug!(channel, ?fail, "temp voice: a new owner's overwrite was not written");
    }
}
