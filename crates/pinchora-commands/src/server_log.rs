//! The server log: compact embeds for joins and leaves, name changes, bans and voice.

use std::{sync::Arc, time::Duration};

use moka::future::Cache as Moka;
use pinchora_core::snowflake;
use pinchora_db::{GuildSettings, LogKind};
use twilight_model::{
    channel::message::{AllowedMentions, Embed},
    id::Id,
    user::User,
    util::Timestamp,
    voice::VoiceState,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder, ImageSource};

use crate::{
    App,
    util::{DANGER, SUCCESS, WARN, avatar_url, display_name, mention, relative},
};

/// What the log remembers between events: names, to show what changed, and who sits in which
/// voice channel. Both are per process; a restart only means the next change has no "before".
pub(crate) struct LogState {
    seen: Moka<(u64, u64), Arc<Seen>>,
    voice: Moka<(u64, u64), u64>,
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
        }
    }

    /// Stores the names `now` and returns the ones seen before.
    async fn swap_names(&self, guild: u64, user: u64, now: Seen) -> Option<Arc<Seen>> {
        let before = self.seen.get(&(guild, user)).await;
        self.seen.insert((guild, user), Arc::new(now)).await;
        before
    }

    /// Stores the voice channel the member is in now (`None`: not in one) and returns the earlier one.
    async fn swap_voice(&self, guild: u64, user: u64, now: Option<u64>) -> Option<u64> {
        let before = self.voice.get(&(guild, user)).await;
        match now {
            Some(channel) => self.voice.insert((guild, user), channel).await,
            None => self.voice.invalidate(&(guild, user)).await,
        }
        before
    }

    /// Forgets everything about a member who left.
    pub(crate) async fn forget(&self, guild: u64, user: u64) {
        self.seen.invalidate(&(guild, user)).await;
        self.voice.invalidate(&(guild, user)).await;
    }
}

/// A member's names as last seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seen {
    nick: Option<String>,
    username: String,
    display_name: Option<String>,
}

impl Seen {
    fn new(user: &User, nick: Option<&str>) -> Self {
        Self {
            nick: nick.map(str::to_owned),
            username: user.name.clone(),
            display_name: user.global_name.clone(),
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
}

/// A name in a code span, so Discord does not read markdown in it.
fn code(name: &str) -> String {
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

/// A change in which voice channel a member is in.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum VoiceChange {
    Joined(u64),
    Left(u64),
    Moved { from: u64, to: u64 },
}

/// What going from the voice channel `before` to `now` means; `None` for mute, deafen and the like.
pub(crate) fn voice_change(before: Option<u64>, now: Option<u64>) -> Option<VoiceChange> {
    match (before, now) {
        (None, Some(channel)) => Some(VoiceChange::Joined(channel)),
        (Some(channel), None) => Some(VoiceChange::Left(channel)),
        (Some(from), Some(to)) if from != to => Some(VoiceChange::Moved { from, to }),
        _ => None,
    }
}

/// The start of every entry: title, who it is about, their avatar, and the time.
fn entry(title: impl Into<String>, color: u32, user_id: u64, user: Option<&User>, text: String) -> EmbedBuilder {
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

fn ban_embed(user: &User, banned: bool) -> Embed {
    if banned {
        entry("Member banned", DANGER, user.id.get(), Some(user), who(user)).build()
    } else {
        entry("Member unbanned", SUCCESS, user.id.get(), Some(user), who(user)).build()
    }
}

fn names_embed(user: &User, changes: &[NameChange]) -> Embed {
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

fn voice_embed(user_id: u64, user: Option<&User>, change: &VoiceChange) -> Embed {
    let member = mention(user_id);
    let (title, color, text) = match change {
        VoiceChange::Joined(channel) => ("Joined voice", SUCCESS, format!("{member} joined <#{channel}>")),
        VoiceChange::Left(channel) => ("Left voice", DANGER, format!("{member} left <#{channel}>")),
        VoiceChange::Moved { from, to } => ("Moved voice", WARN, format!("{member} moved from <#{from}> to <#{to}>")),
    };
    entry(title, color, user_id, user, text).build()
}

/// Posts an entry. A log channel that is gone or closed to the bot is not worth an error each time.
async fn post(app: &App, channel: u64, embed: Embed) {
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

pub(crate) async fn member_joined(app: &App, settings: &GuildSettings, user: &User) {
    if let Some(channel) = settings.log_channel(LogKind::Members) {
        post(app, channel, joined_embed(user)).await;
    }
}

pub(crate) async fn member_left(app: &App, settings: &GuildSettings, user: &User) {
    if let Some(channel) = settings.log_channel(LogKind::Members) {
        post(app, channel, left_embed(user)).await;
    }
}

pub(crate) async fn ban_changed(app: &App, settings: &GuildSettings, user: &User, banned: bool) {
    if let Some(channel) = settings.log_channel(LogKind::Bans) {
        post(app, channel, ban_embed(user, banned)).await;
    }
}

/// Remembers a member's names, so a later change can show the old ones. Only done where the log
/// reports name changes.
pub(crate) async fn note_names(app: &App, settings: &GuildSettings, guild: u64, user: &User, nick: Option<&str>) {
    if settings.log_channel(LogKind::Names).is_some() {
        app.log_state
            .swap_names(guild, user.id.get(), Seen::new(user, nick))
            .await;
    }
}

/// Logs what changed in a member's names, then remembers them.
pub(crate) async fn names_updated(app: &App, settings: &GuildSettings, guild: u64, user: &User, nick: Option<&str>) {
    let Some(channel) = settings.log_channel(LogKind::Names) else {
        return;
    };
    let now = Seen::new(user, nick);
    let before = app.log_state.swap_names(guild, user.id.get(), now.clone()).await;
    let changes = name_changes(before.as_deref(), &now);
    if !changes.is_empty() {
        post(app, channel, names_embed(user, &changes)).await;
    }
}

/// Logs a member joining, leaving or switching voice channels. Mute and deafen changes are ignored,
/// and so are bots (music bots would drown the log).
pub(crate) async fn voice_updated(app: &App, settings: &GuildSettings, guild: u64, state: &VoiceState) {
    let Some(channel) = settings.log_channel(LogKind::Voice) else {
        return;
    };
    let user = state.member.as_ref().map(|m| &m.user);
    if user.is_some_and(|u| u.bot) {
        return;
    }
    let user_id = state.user_id.get();
    let now = state.channel_id.map(|c| c.get());
    let before = app.log_state.swap_voice(guild, user_id, now).await;
    if let Some(change) = voice_change(before, now) {
        post(app, channel, voice_embed(user_id, user, &change)).await;
    }
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
    fn entries_say_what_happened_and_to_whom() {
        let sim = user(42, "sim", Some("Sim"));
        let joined = joined_embed(&sim);
        assert_eq!(joined.title.as_deref(), Some("Member joined"));
        assert!(joined.description.as_deref().unwrap().contains("<@42>"));
        assert_eq!(joined.fields[0].name, "Account created");
        assert!(joined.thumbnail.is_some() && joined.timestamp.is_some());
        assert_eq!(ban_embed(&sim, true).title.as_deref(), Some("Member banned"));
        assert_eq!(ban_embed(&sim, false).title.as_deref(), Some("Member unbanned"));
        assert_eq!(left_embed(&sim).title.as_deref(), Some("Member left"));
        let moved = voice_embed(42, None, &VoiceChange::Moved { from: 1, to: 2 });
        assert!(moved.description.as_deref().unwrap().contains("<#1> to <#2>"));
        assert!(moved.thumbnail.is_none(), "no user, no avatar");
    }

    #[test]
    fn one_change_names_itself_in_the_title() {
        let sim = user(1, "sim", None);
        let one = name_changes(Some(&seen(None, "sim", None)), &seen(Some("Zed"), "sim", None));
        assert_eq!(names_embed(&sim, &one).title.as_deref(), Some("Nickname changed"));
        let two = name_changes(Some(&seen(None, "sim", None)), &seen(Some("Zed"), "sam", None));
        assert_eq!(names_embed(&sim, &two).title.as_deref(), Some("Names changed"));
    }
}
