//! The server log: compact embeds for members, messages, voice and bans, each routed by its event's
//! switch and category channel (`pinchora_core::logging`), with the ignore lists applied. What the
//! audit log reports (roles, channels, the server, invites, emojis, kicks and timeouts) is in
//! `log_audit.rs`.

use std::{sync::Arc, time::Duration};

use moka::future::Cache as Moka;
use pinchora_core::{
    logging::{LogConfig, LogEvent, Subject},
    modules::Module,
    snowflake,
};
use serde::{Deserialize, Serialize};
use twilight_model::{
    channel::{Message, message::AllowedMentions, message::Embed},
    gateway::payload::incoming::{MessageDelete, MessageDeleteBulk, MessageUpdate},
    id::Id,
    user::User,
    util::Timestamp,
    voice::VoiceState,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder, ImageSource};

use crate::{
    App,
    util::{BRAND, DANGER, SUCCESS, WARN, avatar_url, display_name, mention, relative},
};

/// How long a message is kept for the log when message content is on.
const MESSAGE_KEPT: Duration = Duration::from_secs(24 * 3_600);
/// The longest message text kept and shown.
const MESSAGE_TEXT_MAX: usize = 1_000;
/// How long a ban waits for the audit log to say who did it.
const ATTRIBUTION_WAIT: Duration = Duration::from_millis(1_500);

/// What the log remembers between events: names and avatars, to show what changed, who sits in which
/// voice channel and how, who banned whom (from the audit log), and each server's log settings. All
/// per process; a restart only means the next change has no "before".
pub(crate) struct LogState {
    seen: Moka<(u64, u64), Arc<Seen>>,
    voice: Moka<(u64, u64), VoiceSeen>,
    bans: Moka<(u64, u64, bool), Attribution>,
    configs: Moka<u64, Arc<LogConfig>>,
}

/// Who did something, as the audit log says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Attribution {
    pub(crate) actor: u64,
    pub(crate) reason: Option<String>,
}

impl LogState {
    pub(crate) fn new() -> Self {
        Self {
            seen: Moka::builder()
                .max_capacity(100_000)
                .time_to_idle(Duration::from_secs(7 * 86_400))
                .build(),
            voice: Moka::builder()
                .max_capacity(200_000)
                .time_to_idle(Duration::from_secs(12 * 3_600))
                .build(),
            bans: Moka::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
            configs: Moka::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(60))
                .build(),
        }
    }

    /// Stores the names `now` and returns the ones seen before.
    async fn swap_names(&self, guild: u64, user: u64, now: Seen) -> Option<Arc<Seen>> {
        let before = self.seen.get(&(guild, user)).await;
        self.seen.insert((guild, user), Arc::new(now)).await;
        before
    }

    /// Stores how the member sits in voice now and returns how they sat before.
    async fn swap_voice(&self, guild: u64, user: u64, now: VoiceSeen) -> Option<VoiceSeen> {
        let before = self.voice.get(&(guild, user)).await;
        if now.channel.is_some() {
            self.voice.insert((guild, user), now).await;
        } else {
            self.voice.invalidate(&(guild, user)).await;
        }
        before
    }

    /// Forgets everything about a member who left.
    pub(crate) async fn forget(&self, guild: u64, user: u64) {
        self.seen.invalidate(&(guild, user)).await;
        self.voice.invalidate(&(guild, user)).await;
    }

    /// Notes who banned (or unbanned) a member, for the ban's own entry.
    pub(crate) async fn note_ban(&self, guild: u64, target: u64, banned: bool, by: Attribution) {
        self.bans.insert((guild, target, banned), by).await;
    }

    pub(crate) async fn forget_config(&self, guild: u64) {
        self.configs.invalidate(&guild).await;
    }
}

impl App {
    /// A server's log settings, cached for a minute (and dropped when they change).
    pub(crate) async fn log_config(&self, guild: u64) -> anyhow::Result<Arc<LogConfig>> {
        self.log_state
            .configs
            .try_get_with(guild, async { self.db.log_config(guild).await.map(Arc::new) })
            .await
            .map_err(|e| anyhow::anyhow!("{e:#}"))
    }
}

/// A member's names and avatar as last seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seen {
    nick: Option<String>,
    username: String,
    display_name: Option<String>,
    avatar: Option<String>,
}

impl Seen {
    fn new(user: &User, nick: Option<&str>) -> Self {
        Self {
            nick: nick.map(str::to_owned),
            username: user.name.clone(),
            display_name: user.global_name.clone(),
            avatar: user.avatar.map(|a| a.to_string()),
        }
    }
}

/// One name that changed. `known` is false when the earlier value was never seen.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NameChange {
    what: &'static str,
    old: Option<String>,
    new: Option<String>,
    known: bool,
}

impl NameChange {
    /// `old → new`, or just the new value when the old one is unknown.
    fn text(&self) -> String {
        let show = |name: &Option<String>| name.as_deref().map_or_else(|| "*none*".to_owned(), code);
        if self.known {
            format!("{} → {}", show(&self.old), show(&self.new))
        } else {
            show(&self.new)
        }
    }

    fn event(&self) -> LogEvent {
        if self.what == "Nickname" {
            LogEvent::Nickname
        } else {
            LogEvent::Username
        }
    }
}

/// A name in a code span, so Discord does not read markdown in it.
pub(crate) fn code(name: &str) -> String {
    format!("`{}`", name.replace('`', "'"))
}

/// What differs between the names seen before and the current ones. Without a "before" only a
/// nickname that is set is reported, as a bare new value: member updates also fire for roles,
/// timeouts and avatars, so an unknown username or display name would be a false alarm.
pub(crate) fn name_changes(before: Option<&Seen>, now: &Seen) -> Vec<NameChange> {
    let mut changes = Vec::new();
    match before {
        Some(before) => {
            let mut push = |what, old: Option<&String>, new: Option<&String>| {
                if old != new {
                    changes.push(NameChange {
                        what,
                        old: old.cloned(),
                        new: new.cloned(),
                        known: true,
                    });
                }
            };
            push("Nickname", before.nick.as_ref(), now.nick.as_ref());
            push("Username", Some(&before.username), Some(&now.username));
            push("Display name", before.display_name.as_ref(), now.display_name.as_ref());
        }
        None if now.nick.is_some() => changes.push(NameChange {
            what: "Nickname",
            old: None,
            new: now.nick.clone(),
            known: false,
        }),
        None => {}
    }
    changes
}

/// How a member sits in voice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct VoiceSeen {
    channel: Option<u64>,
    mute: bool,
    deaf: bool,
    self_mute: bool,
    self_deaf: bool,
}

impl VoiceSeen {
    fn of(state: &VoiceState) -> Self {
        Self {
            channel: state.channel_id.map(|c| c.get()),
            mute: state.mute,
            deaf: state.deaf,
            self_mute: state.self_mute,
            self_deaf: state.self_deaf,
        }
    }
}

/// A change in which voice channel a member is in, or how they sit there.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum VoiceChange {
    Joined(u64),
    Left(u64),
    Moved {
        from: u64,
        to: u64,
    },
    /// A server mute or deafen, given or taken: (channel, what, on).
    Server(u64, &'static str, bool),
    /// The member muted or deafened themselves, or stopped.
    Own(u64, &'static str, bool),
}

impl VoiceChange {
    fn event(&self) -> LogEvent {
        match self {
            Self::Joined(_) => LogEvent::VoiceJoin,
            Self::Left(_) => LogEvent::VoiceLeave,
            Self::Moved { .. } => LogEvent::VoiceMove,
            Self::Server(..) => LogEvent::VoiceModeration,
            Self::Own(..) => LogEvent::VoiceSelf,
        }
    }

    fn channel(&self) -> u64 {
        match self {
            Self::Joined(c) | Self::Left(c) | Self::Server(c, ..) | Self::Own(c, ..) => *c,
            Self::Moved { to, .. } => *to,
        }
    }
}

/// What going from the voice channel `before` to `now` means.
pub(crate) fn voice_change(before: Option<u64>, now: Option<u64>) -> Option<VoiceChange> {
    match (before, now) {
        (None, Some(channel)) => Some(VoiceChange::Joined(channel)),
        (Some(channel), None) => Some(VoiceChange::Left(channel)),
        (Some(from), Some(to)) if from != to => Some(VoiceChange::Moved { from, to }),
        _ => None,
    }
}

/// Everything that changed between two voice states: a join, leave or move, or (in the same channel,
/// with a known "before") the mutes and deafens.
pub(crate) fn voice_changes(before: Option<VoiceSeen>, now: VoiceSeen) -> Vec<VoiceChange> {
    if let Some(change) = voice_change(before.and_then(|b| b.channel), now.channel) {
        return vec![change];
    }
    let (Some(before), Some(channel)) = (before, now.channel) else {
        return Vec::new();
    };
    let mut changes = Vec::new();
    if before.mute != now.mute {
        changes.push(VoiceChange::Server(channel, "muted", now.mute));
    }
    if before.deaf != now.deaf {
        changes.push(VoiceChange::Server(channel, "deafened", now.deaf));
    }
    if before.self_mute != now.self_mute {
        changes.push(VoiceChange::Own(channel, "muted", now.self_mute));
    }
    if before.self_deaf != now.self_deaf {
        changes.push(VoiceChange::Own(channel, "deafened", now.self_deaf));
    }
    changes
}

/// The start of every entry: title, who it is about, their avatar, and the time.
pub(crate) fn entry(
    title: impl Into<String>,
    color: u32,
    user_id: u64,
    user: Option<&User>,
    text: String,
) -> EmbedBuilder {
    let mut embed = EmbedBuilder::new()
        .color(color)
        .title(title)
        .description(text)
        .footer(EmbedFooterBuilder::new(format!("ID {user_id}")));
    if let Some(Ok(avatar)) = user.map(|u| ImageSource::url(avatar_url(u))) {
        embed = embed.thumbnail(avatar);
    }
    if let Ok(now) = Timestamp::from_secs(chrono::Utc::now().timestamp()) {
        embed = embed.timestamp(now);
    }
    embed
}

fn who(user: &User) -> String {
    format!("{} · {}", mention(user.id), display_name(user, None))
}

fn joined_embed(user: &User) -> Embed {
    let title = if user.bot { "Bot joined" } else { "Member joined" };
    entry(title, SUCCESS, user.id.get(), Some(user), who(user))
        .field(EmbedFieldBuilder::new(
            "Account created",
            relative(snowflake::timestamp(user.id.get())),
        ))
        .build()
}

fn left_embed(user: &User) -> Embed {
    let title = if user.bot { "Bot left" } else { "Member left" };
    entry(title, DANGER, user.id.get(), Some(user), who(user)).build()
}

fn ban_embed(user: &User, banned: bool, by: Option<&Attribution>) -> Embed {
    let mut embed = if banned {
        entry("Member banned", DANGER, user.id.get(), Some(user), who(user))
    } else {
        entry("Member unbanned", SUCCESS, user.id.get(), Some(user), who(user))
    };
    if let Some(by) = by {
        embed = embed.field(EmbedFieldBuilder::new("By", mention(by.actor)).inline());
        if let Some(reason) = &by.reason {
            embed = embed.field(EmbedFieldBuilder::new("Reason", clip(reason, 1_000)));
        }
    }
    embed.build()
}

fn names_embed(user: &User, changes: &[&NameChange]) -> Embed {
    let title = match changes {
        [one] => format!("{} changed", one.what),
        _ => "Names changed".to_owned(),
    };
    let mut embed = entry(title, WARN, user.id.get(), Some(user), who(user));
    for change in changes {
        embed = embed.field(EmbedFieldBuilder::new(change.what, change.text()));
    }
    embed.build()
}

fn avatar_embed(user: &User) -> Embed {
    let mut embed = entry("Avatar changed", BRAND, user.id.get(), None, who(user));
    if let Ok(image) = ImageSource::url(avatar_url(user)) {
        embed = embed.image(image);
    }
    embed.build()
}

fn voice_embed(user_id: u64, user: Option<&User>, change: &VoiceChange) -> Embed {
    let member = mention(user_id);
    let (title, color, text) = match change {
        VoiceChange::Joined(channel) => ("Joined voice", SUCCESS, format!("{member} joined <#{channel}>")),
        VoiceChange::Left(channel) => ("Left voice", DANGER, format!("{member} left <#{channel}>")),
        VoiceChange::Moved { from, to } => ("Moved voice", WARN, format!("{member} moved from <#{from}> to <#{to}>")),
        VoiceChange::Server(channel, what, on) => {
            let un = if *on { "" } else { "un" };
            (
                "Server voice change",
                WARN,
                format!("{member} was server {un}{what} in <#{channel}>"),
            )
        }
        VoiceChange::Own(channel, what, on) => {
            let un = if *on { "" } else { "un" };
            (
                "Voice change",
                BRAND,
                format!("{member} {un}{what} themselves in <#{channel}>"),
            )
        }
    };
    entry(title, color, user_id, user, text).build()
}

/// `text` cut to `max` characters, with an ellipsis when it was longer.
pub(crate) fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Posts an entry. A log channel that is gone or closed to the bot is not worth an error each time.
pub(crate) async fn post(app: &App, channel: u64, embed: Embed) {
    let mentions = AllowedMentions::default();
    if let Err(error) = app
        .http
        .create_message(Id::new(channel.max(1)))
        .embeds(&[embed])
        .allowed_mentions(Some(&mentions))
        .await
    {
        tracing::debug!(%error, channel, "could not post to the server log");
    }
}

/// Where `event` goes in `guild`, if it is logged at all and its subject is not ignored: the event's
/// switch, its category channel, the ignore lists and the log module. `channel` is where it
/// happened, for the channel ignore list.
pub(crate) async fn route(
    app: &App,
    guild: u64,
    event: LogEvent,
    subject: Subject<'_>,
    channel: Option<u64>,
) -> Option<u64> {
    let config = match app.log_config(guild).await {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!(%error, guild, "log settings unavailable");
            return None;
        }
    };
    let to = config.channel(event)?;
    // What a channel sits under is only looked up when something is on the channel list.
    let places = match channel {
        Some(channel) if !config.ignored_channels.is_empty() => app.place_lineage(channel).await,
        Some(channel) => vec![channel],
        None => Vec::new(),
    };
    let subject = Subject {
        places: if subject.places.is_empty() {
            &places
        } else {
            subject.places
        },
        ..subject
    };
    if config.ignores(&subject) || !app.module_allowed(guild, None, Module::Log).await {
        return None;
    }
    Some(to)
}

fn user_subject(user: &User) -> Subject<'static> {
    Subject {
        user: Some(user.id.get()),
        bot: user.bot,
        ..Subject::default()
    }
}

pub(crate) async fn member_joined(app: &App, guild: u64, user: &User, roles: &[u64]) {
    let subject = Subject {
        roles,
        ..user_subject(user)
    };
    if let Some(channel) = route(app, guild, LogEvent::MemberJoin, subject, None).await {
        post(app, channel, joined_embed(user)).await;
    }
}

pub(crate) async fn member_left(app: &App, guild: u64, user: &User) {
    if let Some(channel) = route(app, guild, LogEvent::MemberLeave, user_subject(user), None).await {
        post(app, channel, left_embed(user)).await;
    }
}

/// Logs a ban or unban. The audit log usually says who did it a moment later, so this waits for it
/// once; one done through Pinchora is left to its case in the mod-log.
pub(crate) async fn ban_changed(app: &App, guild: u64, user: &User, banned: bool) {
    let event = if banned { LogEvent::Ban } else { LogEvent::Unban };
    let Some(channel) = route(app, guild, event, user_subject(user), None).await else {
        return;
    };
    let key = (guild, user.id.get(), banned);
    let mut by = app.log_state.bans.get(&key).await;
    if by.is_none() {
        tokio::time::sleep(ATTRIBUTION_WAIT).await;
        by = app.log_state.bans.get(&key).await;
    }
    if by.as_ref().is_some_and(|b| b.actor == app.application_id.get()) {
        return;
    }
    post(app, channel, ban_embed(user, banned, by.as_ref())).await;
}

/// Whether the log reports anything about names or avatars, so they are worth remembering.
async fn names_logged(app: &App, guild: u64) -> bool {
    match app.log_config(guild).await {
        Ok(config) => [LogEvent::Nickname, LogEvent::Username, LogEvent::Avatar]
            .into_iter()
            .any(|e| config.channel(e).is_some()),
        Err(_) => false,
    }
}

/// Remembers a member's names, so a later change can show the old ones. Only done where the log
/// reports name changes.
pub(crate) async fn note_names(app: &App, guild: u64, user: &User, nick: Option<&str>) {
    if names_logged(app, guild).await {
        app.log_state
            .swap_names(guild, user.id.get(), Seen::new(user, nick))
            .await;
    }
}

/// Logs what changed in a member's names and avatar, then remembers them.
pub(crate) async fn names_updated(app: &App, guild: u64, user: &User, nick: Option<&str>, roles: &[u64]) {
    if !names_logged(app, guild).await {
        return;
    }
    let now = Seen::new(user, nick);
    let before = app.log_state.swap_names(guild, user.id.get(), now.clone()).await;
    let changes = name_changes(before.as_deref(), &now);
    let subject = Subject {
        roles,
        ..user_subject(user)
    };
    for event in [LogEvent::Nickname, LogEvent::Username] {
        let these: Vec<&NameChange> = changes.iter().filter(|c| c.event() == event).collect();
        if these.is_empty() {
            continue;
        }
        if let Some(channel) = route(app, guild, event, subject, None).await {
            post(app, channel, names_embed(user, &these)).await;
        }
    }
    if before.is_some_and(|b| b.avatar != now.avatar)
        && let Some(channel) = route(app, guild, LogEvent::Avatar, subject, None).await
    {
        post(app, channel, avatar_embed(user)).await;
    }
}

/// Logs a member joining, leaving or switching voice channels, and server or self mutes and
/// deafens. Bots are left out (music bots would drown the log).
pub(crate) async fn voice_updated(app: &App, guild: u64, state: &VoiceState) {
    let user = state.member.as_ref().map(|m| &m.user);
    if user.is_some_and(|u| u.bot) {
        return;
    }
    let user_id = state.user_id.get();
    let now = VoiceSeen::of(state);
    let before = app.log_state.swap_voice(guild, user_id, now).await;
    let roles: Vec<u64> = state
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|r| r.get()).collect())
        .unwrap_or_default();
    for change in voice_changes(before, now) {
        let subject = Subject {
            user: Some(user_id),
            roles: &roles,
            ..Subject::default()
        };
        if let Some(channel) = route(app, guild, change.event(), subject, Some(change.channel())).await {
            post(app, channel, voice_embed(user_id, user, &change)).await;
        }
    }
}

/// A message as the log keeps it while message content is on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct KeptMessage {
    pub(crate) author: u64,
    pub(crate) name: String,
    pub(crate) content: String,
    #[serde(default)]
    pub(crate) attachments: Vec<String>,
}

fn kept_key(id: u64) -> String {
    format!("logmsg:{id}")
}

/// Keeps a message for a day, so its edit or delete can show what it said. Only with message
/// content on, and only where the log reports message edits or deletes.
pub(crate) async fn keep_message(app: &App, guild: u64, message: &Message) {
    if !app.message_content {
        return;
    }
    let logged = match app.log_config(guild).await {
        Ok(config) => [
            LogEvent::MessageEdit,
            LogEvent::MessageDelete,
            LogEvent::MessageBulkDelete,
        ]
        .into_iter()
        .any(|e| config.channel(e).is_some()),
        Err(_) => false,
    };
    if !logged {
        return;
    }
    let kept = KeptMessage {
        author: message.author.id.get(),
        name: display_name(&message.author, message.member.as_ref().and_then(|m| m.nick.as_deref())),
        content: clip(&message.content, MESSAGE_TEXT_MAX),
        attachments: message.attachments.iter().map(|a| a.filename.clone()).collect(),
    };
    if let Ok(json) = serde_json::to_string(&kept)
        && let Err(error) = app.cache.put(&kept_key(message.id.get()), &json, MESSAGE_KEPT).await
    {
        tracing::debug!(%error, "message not kept for the log");
    }
}

async fn kept(app: &App, id: u64) -> Option<KeptMessage> {
    if !app.message_content {
        return None;
    }
    let json = app.cache.get(&kept_key(id)).await.ok().flatten()?;
    serde_json::from_str(&json).ok()
}

fn jump(guild: u64, channel: u64, message: u64) -> String {
    format!("https://discord.com/channels/{guild}/{channel}/{message}")
}

/// Message text for a field: clipped, and something when it is empty.
fn shown(text: &str) -> String {
    if text.trim().is_empty() {
        "*no text*".to_owned()
    } else {
        clip(text, 1_000)
    }
}

/// Logs an edit: who, where, a link, and the text before and after when it is known.
pub async fn on_message_update(app: Arc<App>, event: MessageUpdate) -> anyhow::Result<()> {
    let message = event.0;
    let Some(guild) = message.guild_id.map(|g| g.get()) else {
        return Ok(());
    };
    // Discord also sends an update when a link unfurls; only a real edit has an edit time.
    if message.author.bot || message.webhook_id.is_some() || message.edited_timestamp.is_none() {
        return Ok(());
    }
    let before = kept(&app, message.id.get()).await;
    if before.as_ref().is_some_and(|b| b.content == message.content) {
        return Ok(());
    }
    let roles: Vec<u64> = message
        .member
        .as_ref()
        .map(|m| m.roles.iter().map(|r| r.get()).collect())
        .unwrap_or_default();
    let subject = Subject {
        roles: &roles,
        ..user_subject(&message.author)
    };
    let channel_id = message.channel_id.get();
    if let Some(to) = route(&app, guild, LogEvent::MessageEdit, subject, Some(channel_id)).await {
        let text = format!(
            "{} edited [a message]({}) in <#{channel_id}>",
            mention(message.author.id),
            jump(guild, channel_id, message.id.get())
        );
        let mut embed = entry(
            "Message edited",
            WARN,
            message.author.id.get(),
            Some(&message.author),
            text,
        );
        if let Some(before) = &before {
            embed = embed.field(EmbedFieldBuilder::new("Before", shown(&before.content)));
        }
        if app.message_content {
            embed = embed.field(EmbedFieldBuilder::new("After", shown(&message.content)));
        }
        post(&app, to, embed.build()).await;
    }
    keep_message(&app, guild, &message).await;
    Ok(())
}

/// Logs a deleted message: what it said and who sent it when it was kept, otherwise where and when.
pub async fn on_message_delete(app: Arc<App>, event: MessageDelete) -> anyhow::Result<()> {
    let Some(guild) = event.guild_id.map(|g| g.get()) else {
        return Ok(());
    };
    let (id, channel_id) = (event.id.get(), event.channel_id.get());
    let message = kept(&app, id).await;
    let subject = Subject {
        user: message.as_ref().map(|m| m.author),
        ..Subject::default()
    };
    let Some(to) = route(&app, guild, LogEvent::MessageDelete, subject, Some(channel_id)).await else {
        return Ok(());
    };
    let sent = relative(snowflake::timestamp(id));
    let embed = match &message {
        Some(message) => {
            let text = format!(
                "A message by {} was deleted in <#{channel_id}>",
                mention(message.author)
            );
            let mut embed = entry("Message deleted", DANGER, message.author, None, text)
                .field(EmbedFieldBuilder::new("Message", shown(&message.content)))
                .field(EmbedFieldBuilder::new("Sent", sent).inline());
            if !message.attachments.is_empty() {
                let files: Vec<String> = message.attachments.iter().map(|f| code(f)).collect();
                embed = embed.field(EmbedFieldBuilder::new("Attachments", clip(&files.join(" "), 1_000)));
            }
            embed
        }
        None => entry(
            "Message deleted",
            DANGER,
            id,
            None,
            format!("A message was deleted in <#{channel_id}>"),
        )
        .field(EmbedFieldBuilder::new("Sent", sent).inline()),
    };
    post(&app, to, embed.build()).await;
    Ok(())
}

/// Logs a bulk delete: how many, where, and the messages that were kept, oldest first.
pub async fn on_message_delete_bulk(app: Arc<App>, event: MessageDeleteBulk) -> anyhow::Result<()> {
    let Some(guild) = event.guild_id.map(|g| g.get()) else {
        return Ok(());
    };
    let channel_id = event.channel_id.get();
    let Some(to) = route(
        &app,
        guild,
        LogEvent::MessageBulkDelete,
        Subject::default(),
        Some(channel_id),
    )
    .await
    else {
        return Ok(());
    };
    let mut ids: Vec<u64> = event.ids.iter().map(|id| id.get()).collect();
    ids.sort_unstable();
    let mut lines = Vec::new();
    for id in &ids {
        if let Some(message) = kept(&app, *id).await {
            lines.push(format!(
                "**{}**: {}",
                message.name.replace('*', ""),
                clip(&message.content.replace('\n', " "), 200)
            ));
        }
    }
    let n = ids.len();
    let mut text = format!(
        "{n} message{} deleted in <#{channel_id}>",
        if n == 1 { " was" } else { "s were" }
    );
    if !lines.is_empty() {
        text.push_str("\n\n");
        text.push_str(&clip(&lines.join("\n"), 3_500));
    }
    let embed = entry("Messages bulk deleted", DANGER, channel_id, None, text).build();
    post(&app, to, embed).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn user(id: u64, username: &str, display: Option<&str>) -> User {
        serde_json::from_value(json!({
            "id": id.to_string(),
            "username": username,
            "discriminator": "0",
            "global_name": display,
            "avatar": null,
        }))
        .expect("user JSON")
    }

    fn seen(nick: Option<&str>, username: &str, display: Option<&str>) -> Seen {
        Seen::new(&user(1, username, display), nick)
    }

    #[test]
    fn a_new_nickname_shows_old_and_new() {
        let before = seen(Some("Old"), "sim", None);
        let now = seen(Some("New"), "sim", None);
        let changes = name_changes(Some(&before), &now);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].what, "Nickname");
        assert_eq!(changes[0].text(), "`Old` → `New`");
    }

    #[test]
    fn setting_and_clearing_a_nickname() {
        let plain = seen(None, "sim", None);
        let nicked = seen(Some("Zed"), "sim", None);
        assert_eq!(name_changes(Some(&plain), &nicked)[0].text(), "*none* → `Zed`");
        assert_eq!(name_changes(Some(&nicked), &plain)[0].text(), "`Zed` → *none*");
    }

    #[test]
    fn username_and_display_name_changes_are_separate() {
        let before = seen(None, "sim", Some("Sim"));
        let now = seen(None, "sim2", Some("Simmy"));
        let changes = name_changes(Some(&before), &now);
        let whats: Vec<_> = changes.iter().map(|c| c.what).collect();
        assert_eq!(whats, ["Username", "Display name"]);
        assert_eq!(changes[0].text(), "`sim` → `sim2`");
        assert!(changes.iter().all(|c| c.event() == LogEvent::Username));
    }

    #[test]
    fn nothing_changed_nothing_to_log() {
        let same = seen(Some("Zed"), "sim", Some("Sim"));
        assert!(name_changes(Some(&same), &same.clone()).is_empty());
    }

    #[test]
    fn without_a_before_only_a_set_nickname_is_reported() {
        let nicked = seen(Some("Zed"), "sim", Some("Sim"));
        let changes = name_changes(None, &nicked);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].text(), "`Zed`", "just the new value, no arrow");
        assert!(name_changes(None, &seen(None, "sim", Some("Sim"))).is_empty());
    }

    #[test]
    fn names_cannot_break_out_of_the_code_span() {
        assert_eq!(code("a`b"), "`a'b`");
    }

    #[test]
    fn voice_changes_need_a_before_and_an_after() {
        assert_eq!(voice_change(None, Some(5)), Some(VoiceChange::Joined(5)));
        assert_eq!(voice_change(Some(5), None), Some(VoiceChange::Left(5)));
        assert_eq!(
            voice_change(Some(5), Some(6)),
            Some(VoiceChange::Moved { from: 5, to: 6 })
        );
        assert_eq!(voice_change(Some(5), Some(5)), None, "mute or deafen");
        assert_eq!(voice_change(None, None), None);
    }

    #[test]
    fn mutes_and_deafens_need_a_known_before_in_the_same_channel() {
        let sitting = VoiceSeen {
            channel: Some(5),
            ..VoiceSeen::default()
        };
        let muted = VoiceSeen { mute: true, ..sitting };
        assert_eq!(
            voice_changes(Some(sitting), muted),
            [VoiceChange::Server(5, "muted", true)]
        );
        assert_eq!(
            voice_changes(None, muted),
            [VoiceChange::Joined(5)],
            "unknown before: a join"
        );
        let own = VoiceSeen {
            self_mute: true,
            self_deaf: true,
            ..sitting
        };
        assert_eq!(
            voice_changes(Some(sitting), own),
            [
                VoiceChange::Own(5, "muted", true),
                VoiceChange::Own(5, "deafened", true)
            ]
        );
        assert_eq!(VoiceChange::Own(5, "muted", true).event(), LogEvent::VoiceSelf);
        let moved = VoiceSeen {
            channel: Some(6),
            mute: true,
            ..sitting
        };
        assert_eq!(
            voice_changes(Some(sitting), moved),
            [VoiceChange::Moved { from: 5, to: 6 }]
        );
    }

    #[test]
    fn entries_say_what_happened_and_to_whom() {
        let sim = user(42, "sim", Some("Sim"));
        let joined = joined_embed(&sim);
        assert_eq!(joined.title.as_deref(), Some("Member joined"));
        assert!(joined.description.as_deref().unwrap().contains("<@42>"));
        assert_eq!(joined.fields[0].name, "Account created");
        assert!(joined.thumbnail.is_some() && joined.timestamp.is_some());
        assert_eq!(ban_embed(&sim, true, None).title.as_deref(), Some("Member banned"));
        assert_eq!(ban_embed(&sim, false, None).title.as_deref(), Some("Member unbanned"));
        let by = Attribution {
            actor: 7,
            reason: Some("spam".into()),
        };
        let attributed = ban_embed(&sim, true, Some(&by));
        assert_eq!(attributed.fields[0].value, "<@7>");
        assert_eq!(attributed.fields[1].value, "spam");
        assert_eq!(left_embed(&sim).title.as_deref(), Some("Member left"));
        let moved = voice_embed(42, None, &VoiceChange::Moved { from: 1, to: 2 });
        assert!(moved.description.as_deref().unwrap().contains("<#1> to <#2>"));
        assert!(moved.thumbnail.is_none(), "no user, no avatar");
        let unmuted = voice_embed(42, None, &VoiceChange::Server(1, "muted", false));
        assert!(unmuted.description.as_deref().unwrap().contains("was server unmuted"));
    }

    #[test]
    fn one_change_names_itself_in_the_title() {
        let sim = user(1, "sim", None);
        let one = name_changes(Some(&seen(None, "sim", None)), &seen(Some("Zed"), "sim", None));
        let one: Vec<&NameChange> = one.iter().collect();
        assert_eq!(names_embed(&sim, &one).title.as_deref(), Some("Nickname changed"));
        let two = name_changes(Some(&seen(None, "sim", None)), &seen(None, "sam", Some("Sam")));
        let two: Vec<&NameChange> = two.iter().collect();
        assert_eq!(names_embed(&sim, &two).title.as_deref(), Some("Names changed"));
    }

    #[test]
    fn long_text_is_clipped_with_an_ellipsis() {
        assert_eq!(clip("hello", 10), "hello");
        assert_eq!(clip("hello world", 6), "hello…");
        assert_eq!(shown("  "), "*no text*");
    }
}
