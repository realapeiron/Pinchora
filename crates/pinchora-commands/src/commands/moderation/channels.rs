//! The commands that act on a channel: `/purge`, `/slowmode`, `/lock` and `/unlock`.

use chrono::Duration;
use pinchora_core::{
    i18n::Lang,
    moderation::{CaseKind, PURGE_AGE, check_reason, parse_slowmode},
    snowflake, t,
};
use twilight_http::request::AuditLogReason;
use twilight_interactions::command::{CommandModel, CreateCommand, ResolvedUser};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::{Message, permission_overwrite::PermissionOverwrite as Overwrite},
    guild::Permissions,
    http::permission_overwrite::{PermissionOverwrite, PermissionOverwriteType},
    id::Id,
};

use super::{
    Act, Moderator,
    members::{audit_reason, private, public, refusal},
    permitted, record, reply_embed, words,
};
use crate::Cx;

fn manage_messages() -> Permissions {
    Permissions::MANAGE_MESSAGES
}

fn manage_channels() -> Permissions {
    Permissions::MANAGE_CHANNELS
}

/// What `/lock` takes from @everyone, and `/unlock` gives back.
pub(crate) const LOCKED: Permissions = Permissions::SEND_MESSAGES
    .union(Permissions::SEND_MESSAGES_IN_THREADS)
    .union(Permissions::CREATE_PUBLIC_THREADS)
    .union(Permissions::CREATE_PRIVATE_THREADS);

/// The channel a command is about: the one it names, else the one it was used in, as (id, name).
fn target_channel(cx: &Cx, named: Option<&InteractionChannel>) -> Option<(u64, String)> {
    match named {
        Some(channel) => Some((channel.id.get(), channel.name.clone())),
        None => cx
            .interaction
            .channel
            .as_ref()
            .map(|c| (c.id.get(), c.name.clone().unwrap_or_default())),
    }
}

/// Whether the moderator has `needed` in the channel named (Discord sends their permissions there), or
/// here when none is named.
fn allowed_in(cx: &Cx, named: Option<&InteractionChannel>, needed: Permissions) -> bool {
    match named {
        Some(channel) => {
            channel.permissions.contains(needed) || channel.permissions.contains(Permissions::ADMINISTRATOR)
        }
        None => super::allowed(cx, needed),
    }
}

/// [`permitted`] for Manage Channels, in the channel the command names.
async fn channel_permitted(cx: &Cx, named: Option<&InteractionChannel>) -> anyhow::Result<Option<u64>> {
    let Some(guild) = cx.guild_id() else {
        cx.reply_ephemeral(t!(cx.lang(), "mod.guild-only")).await?;
        return Ok(None);
    };
    if allowed_in(cx, named, Permissions::MANAGE_CHANNELS) {
        return Ok(Some(guild));
    }
    let permission = words::permission(cx.lang(), "manage-channels");
    cx.reply_ephemeral(t!(cx.lang(), "mod.needs", permission = permission))
        .await?;
    Ok(None)
}

/// Which messages `/purge` takes.
#[derive(Debug, Clone, Default)]
pub(crate) struct PurgeFilter {
    pub(crate) user: Option<u64>,
    pub(crate) bots: bool,
    pub(crate) humans: bool,
    pub(crate) embeds: bool,
    pub(crate) regex: Option<String>,
    pub(crate) after: Option<u64>,
    pub(crate) before: Option<u64>,
    pub(crate) starts: Option<String>,
    pub(crate) ends: Option<String>,
    pub(crate) not: Option<String>,
    pub(crate) mentions: bool,
    pub(crate) reactions: bool,
    pub(crate) contains: Option<String>,
    pub(crate) links: bool,
    pub(crate) attachments: bool,
}

impl PurgeFilter {
    /// Whether `message` (sent at `sent`) is one to delete: not pinned, young enough for Discord to
    /// bulk delete, and matching every filter given.
    pub(crate) fn takes(&self, message: &Message, now: chrono::DateTime<chrono::Utc>) -> bool {
        let sent = snowflake::timestamp(message.id.get());
        // A minute of margin: Discord counts the 14 days on its own clock.
        if message.pinned || sent <= now - PURGE_AGE + Duration::minutes(1) {
            return false;
        }
        let content = message.content.to_lowercase();
        self.user.is_none_or(|u| message.author.id.get() == u)
            && (!self.bots || message.author.bot)
            && (!self.humans || !message.author.bot)
            && (!self.embeds || !message.embeds.is_empty())
            && (!self.mentions
                || message.mention_everyone
                || !message.mentions.is_empty()
                || !message.mention_roles.is_empty())
            && (!self.reactions || !message.reactions.is_empty())
            && self.after.is_none_or(|id| message.id.get() > id)
            && self.before.is_none_or(|id| message.id.get() < id)
            && self
                .starts
                .as_ref()
                .is_none_or(|s| content.starts_with(&s.to_lowercase()))
            && self.ends.as_ref().is_none_or(|s| content.ends_with(&s.to_lowercase()))
            && self.not.as_ref().is_none_or(|s| !content.contains(&s.to_lowercase()))
            && self
                .regex
                .as_ref()
                .is_none_or(|s| pinchora_core::automod::compile(s).is_ok_and(|r| r.is_match(&message.content)))
            && self
                .contains
                .as_deref()
                .is_none_or(|text| content.contains(&text.to_lowercase()))
            && (!self.links
                || ["http://", "https://", "discord.gg/"]
                    .iter()
                    .any(|l| content.contains(l)))
            && (!self.attachments || !message.attachments.is_empty())
    }

    /// Whether the filters look at what a message says or carries, which Discord only shows a bot
    /// with the Message Content intent.
    pub(crate) fn reads(&self) -> bool {
        self.contains.is_some()
            || self.links
            || self.attachments
            || self.embeds
            || self.regex.is_some()
            || self.starts.is_some()
            || self.ends.is_some()
            || self.not.is_some()
            || self.mentions
    }

    /// The filters in words, for the case: "from @Mia, with links".
    fn describe(&self, lang: Lang, user_name: Option<&str>) -> String {
        let mut parts = Vec::new();
        if let Some(name) = user_name {
            parts.push(t!(lang, "mod.details.from", name = name));
        }
        if self.bots {
            parts.push(t!(lang, "mod.details.from-bots"));
        }
        if let Some(text) = &self.contains {
            let text = format!("`{}`", text.replace('`', "'"));
            parts.push(t!(lang, "mod.details.containing", text = text));
        }
        if self.links {
            parts.push(t!(lang, "mod.details.with-links"));
        }
        if self.attachments {
            parts.push(t!(lang, "mod.details.with-attachments"));
        }
        parts.join(", ")
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "purge",
    desc = "Delete recent messages here (up to 100)",
    default_permissions = "manage_messages",
    contexts = "guild"
)]
pub struct Purge {
    /// How many messages to delete
    #[command(min_value = 1, max_value = 100)]
    count: i64,
    /// Only messages from this member
    user: Option<ResolvedUser>,
    /// Only messages from bots
    bots: Option<bool>,
    /// Only human authors
    humans: Option<bool>,
    /// Only messages with embeds
    embeds: Option<bool>,
    /// Regular expression
    #[command(max_length = 100)]
    regex: Option<String>,
    /// After this message ID
    after: Option<String>,
    /// Before this message ID
    before: Option<String>,
    /// Starts with text
    starts: Option<String>,
    /// Ends with text
    ends: Option<String>,
    /// Exclude this text
    not: Option<String>,
    /// Only messages with mentions
    mentions: Option<bool>,
    /// Only messages with reactions
    reactions: Option<bool>,
    /// Only messages containing this text
    #[command(max_length = 100)]
    contains: Option<String>,
    /// Only messages with links
    links: Option<bool>,
    /// Only messages with attachments
    attachments: Option<bool>,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}

impl Purge {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = permitted(cx, Permissions::MANAGE_MESSAGES, "manage-messages").await? else {
            return Ok(());
        };
        let reason = match check_reason(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(error) => return cx.reply_ephemeral(error.text(lang)).await,
        };
        if self
            .regex
            .as_ref()
            .is_some_and(|s| pinchora_core::automod::compile(s).is_err())
            || [&self.after, &self.before]
                .into_iter()
                .flatten()
                .any(|s| pinchora_core::mod_parity::id(s).is_none())
        {
            return cx.reply_ephemeral(t!(lang, "parity.invalid")).await;
        }
        let filter = PurgeFilter {
            humans: self.humans == Some(true),
            embeds: self.embeds == Some(true),
            regex: self.regex,
            after: self.after.as_deref().and_then(pinchora_core::mod_parity::id),
            before: self.before.as_deref().and_then(pinchora_core::mod_parity::id),
            starts: self.starts,
            ends: self.ends,
            not: self.not,
            mentions: self.mentions == Some(true),
            reactions: self.reactions == Some(true),
            user: self.user.as_ref().map(|u| u.resolved.id.get()),
            bots: self.bots == Some(true),
            contains: self.contains.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()),
            links: self.links == Some(true),
            attachments: self.attachments == Some(true),
        };
        let Some((channel, channel_name)) = target_channel(cx, None) else {
            return cx.reply_ephemeral(t!(lang, "mod.which-channel")).await;
        };
        // Filters that look at what messages say or carry need the Message Content intent: switched off,
        // the bot reads no chat.
        if filter.reads() && !cx.app.message_content {
            return cx.reply_ephemeral(t!(lang, "mod.purge-needs-content")).await;
        }
        let request = cx.app.http.channel_messages(Id::new(channel)).limit(100);
        let response = if let Some(before) = filter.before {
            request.before(Id::new(before)).await
        } else {
            request.await
        };
        let messages = match response {
            Ok(response) => response.models().await?,
            Err(error) => {
                let text = t!(lang, "mod.read-failed", why = refusal(lang, &error));
                return cx.reply_ephemeral(text).await;
            }
        };
        // Switched on but not granted, Discord sends other people's messages without their text,
        // attachments or embeds: filters that look at them would match nothing.
        if filter.reads() && !readable(&messages, cx.app.application_id.get()) {
            return cx.reply_ephemeral(t!(lang, "mod.purge-needs-content")).await;
        }
        let now = cx.app.now();
        let ids: Vec<_> = messages
            .iter()
            .filter(|m| filter.takes(m, now))
            .take(self.count.clamp(1, 100) as usize)
            .map(|m| m.id)
            .collect();
        if ids.is_empty() {
            return cx.reply_ephemeral(t!(lang, "mod.purge-none")).await;
        }
        let moderator = Moderator::of(cx)?;
        let audit = audit_reason(lang, &moderator, reason.as_deref());
        let http = &cx.app.http;
        let deleted = if let [one] = ids.as_slice() {
            http.delete_message(Id::new(channel), *one)
                .reason(&audit)
                .await
                .map(drop)
        } else {
            http.delete_messages(Id::new(channel), &ids)
                .reason(&audit)
                .await
                .map(drop)
        };
        if let Err(error) = deleted {
            let text = t!(lang, "mod.refused-delete", why = refusal(lang, &error));
            return cx.reply_ephemeral(text).await;
        }
        // The case's details are the server's record, in the server's language.
        let server = cx.app.guild_lang(guild).await;
        let purged = t!(server, "mod.details.purged", count = ids.len());
        let described = filter.describe(server, self.user.as_ref().map(|u| u.resolved.name.as_str()));
        let details = if described.is_empty() {
            purged
        } else {
            t!(server, "mod.details.filters", purged = purged, filters = described)
        };
        let act = Act {
            kind: CaseKind::Purge,
            target: channel,
            target_name: format!("#{channel_name}"),
            reason,
            duration: None,
            details: Some(details),
        };
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, None).await?;
        cx.reply(private(reply_embed(&opened.case, &[], lang))).await
    }
}

/// Whether Discord showed the bot what other people's messages say: without the Message Content
/// intent every one of them comes without text, attachments and embeds.
fn readable(messages: &[Message], me: u64) -> bool {
    messages
        .iter()
        .filter(|m| m.author.id.get() != me)
        .any(|m| !m.content.is_empty() || !m.attachments.is_empty() || !m.embeds.is_empty())
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "slowmode",
    desc = "How long members wait between messages in a channel",
    default_permissions = "manage_channels",
    contexts = "guild"
)]
pub struct Slowmode {
    /// Like 5s, 30s, 2m or 1h (up to 6 hours), or off
    #[command(max_length = 16)]
    duration: String,
    /// The channel (default: this one)
    #[command(channel_types = "guild_text guild_announcement guild_voice guild_forum public_thread private_thread")]
    channel: Option<InteractionChannel>,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}

impl Slowmode {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = channel_permitted(cx, self.channel.as_ref()).await? else {
            return Ok(());
        };
        let reason = match check_reason(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(error) => return cx.reply_ephemeral(error.text(lang)).await,
        };
        let seconds = match parse_slowmode(&self.duration) {
            Ok(s) => s,
            Err(error) => return cx.reply_ephemeral(error.text(lang)).await,
        };
        let Some((channel, name)) = target_channel(cx, self.channel.as_ref()) else {
            return cx.reply_ephemeral(t!(lang, "mod.which-channel")).await;
        };
        let moderator = Moderator::of(cx)?;
        let audit = audit_reason(lang, &moderator, reason.as_deref());
        if let Err(error) = cx
            .app
            .http
            .update_channel(Id::new(channel))
            .rate_limit_per_user(seconds)
            .reason(&audit)
            .await
        {
            return cx
                .reply_ephemeral(t!(lang, "mod.refused", why = refusal(lang, &error)))
                .await;
        }
        let server = cx.app.guild_lang(guild).await;
        let details = if seconds == 0 {
            t!(server, "mod.details.slowmode-off")
        } else {
            let every = words::duration(server, Duration::seconds(seconds.into()));
            t!(server, "mod.details.slowmode-every", duration = every)
        };
        let act = Act {
            kind: CaseKind::Slowmode,
            target: channel,
            target_name: format!("#{name}"),
            reason,
            duration: None,
            details: Some(details),
        };
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, None).await?;
        cx.reply(public(reply_embed(&opened.case, &[], lang))).await
    }
}

/// What @everyone's overwrite becomes when a channel is locked or unlocked, or `None` when it already
/// is: `(allow, deny)`, both empty meaning the overwrite can go. An unlock puts back what the overwrite
/// allowed and denied of the locked permissions before the lock (`before`), when that is known.
pub(crate) fn lock_change(
    allow: Permissions,
    deny: Permissions,
    lock: bool,
    before: Option<(Permissions, Permissions)>,
) -> Option<(Permissions, Permissions)> {
    let locked = deny.contains(Permissions::SEND_MESSAGES);
    let (allowed, denied) = before.unwrap_or((Permissions::empty(), Permissions::empty()));
    match (lock, locked) {
        (true, true) | (false, false) => None,
        (true, false) => Some((allow - LOCKED, deny | LOCKED)),
        (false, true) => Some((
            (allow - LOCKED) | (allowed & LOCKED),
            (deny - LOCKED) | (denied & (LOCKED - Permissions::SEND_MESSAGES)),
        )),
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "lock",
    desc = "Stop members from sending messages in a channel",
    default_permissions = "manage_channels",
    contexts = "guild"
)]
pub struct Lock {
    /// The channel (default: this one)
    #[command(channel_types = "guild_text guild_announcement guild_voice guild_forum")]
    channel: Option<InteractionChannel>,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "unlock",
    desc = "Let members send messages in a locked channel again",
    default_permissions = "manage_channels",
    contexts = "guild"
)]
pub struct Unlock {
    /// The channel (default: this one)
    #[command(channel_types = "guild_text guild_announcement guild_voice guild_forum")]
    channel: Option<InteractionChannel>,
    /// Why
    #[command(max_length = 512)]
    reason: Option<String>,
}

impl Lock {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        lock(cx, self.channel.as_ref(), self.reason.as_deref(), true).await
    }
}

impl Unlock {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        lock(cx, self.channel.as_ref(), self.reason.as_deref(), false).await
    }
}

async fn lock(cx: &Cx, named: Option<&InteractionChannel>, reason: Option<&str>, lock: bool) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(guild) = channel_permitted(cx, named).await? else {
        return Ok(());
    };
    let reason = match check_reason(reason) {
        Ok(reason) => reason,
        Err(error) => return cx.reply_ephemeral(error.text(lang)).await,
    };
    let Some((channel, name)) = target_channel(cx, named) else {
        return cx.reply_ephemeral(t!(lang, "mod.which-channel")).await;
    };
    let http = &cx.app.http;
    let found = match http.channel(Id::new(channel)).await {
        Ok(response) => response.model().await?,
        Err(error) => {
            let text = t!(lang, "mod.read-failed", why = refusal(lang, &error));
            return cx.reply_ephemeral(text).await;
        }
    };
    let everyone: Option<&Overwrite> = found
        .permission_overwrites
        .iter()
        .flatten()
        .find(|o| o.id.get() == guild);
    let (allow, deny) = everyone.map_or((Permissions::empty(), Permissions::empty()), |o| (o.allow, o.deny));
    // What the lock took is put back as it was: an unlock asks the lock's case what that was.
    let before = if lock {
        None
    } else {
        let saved = cx.app.db.lock_state(guild, channel).await?;
        saved.map(|(a, d)| (Permissions::from_bits_truncate(a), Permissions::from_bits_truncate(d)))
    };
    let Some((new_allow, new_deny)) = lock_change(allow, deny, lock, before) else {
        let key = if lock { "mod.already-locked" } else { "mod.not-locked" };
        return cx
            .reply_ephemeral(t!(lang, key, channel = format!("<#{channel}>")))
            .await;
    };
    let moderator = Moderator::of(cx)?;
    let audit = audit_reason(lang, &moderator, reason.as_deref());
    let (allow_before, deny_before) = (allow & LOCKED, deny & LOCKED);
    let (allow, deny) = (new_allow, new_deny);
    let done = if allow.is_empty() && deny.is_empty() {
        http.delete_channel_permission(Id::new(channel))
            .role(Id::new(guild))
            .reason(&audit)
            .await
            .map(drop)
    } else {
        let overwrite = PermissionOverwrite {
            allow: Some(allow),
            deny: Some(deny),
            id: Id::new(guild),
            kind: PermissionOverwriteType::Role,
        };
        http.update_channel_permission(Id::new(channel), &overwrite)
            .reason(&audit)
            .await
            .map(drop)
    };
    if let Err(error) = done {
        return cx
            .reply_ephemeral(t!(lang, "mod.refused", why = refusal(lang, &error)))
            .await;
    }
    let act = Act {
        kind: if lock { CaseKind::Lock } else { CaseKind::Unlock },
        target: channel,
        target_name: format!("#{name}"),
        reason,
        duration: None,
        details: None,
    };
    let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, None).await?;
    if lock {
        let (allowed, denied) = (allow_before.bits(), deny_before.bits());
        cx.app
            .db
            .set_lock_state(guild, opened.case.number, allowed, denied)
            .await?;
    }
    cx.reply(public(reply_embed(&opened.case, &[], lang))).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn message(id_age_days: i64, author_bot: bool, content: &str, pinned: bool, files: bool) -> Message {
        let now = chrono::Utc::now();
        let sent = now - Duration::days(id_age_days);
        let id = ((sent.timestamp_millis() - 1_420_070_400_000) as u64) << 22;
        let attachments = if files {
            json!([{ "id": "1", "filename": "a.png", "size": 1, "url": "https://x", "proxy_url": "https://x" }])
        } else {
            json!([])
        };
        serde_json::from_value(json!({
            "id": id.max(1).to_string(),
            "channel_id": "2",
            "type": 0,
            "author": { "id": if author_bot { "9" } else { "5" }, "username": "a", "discriminator": "0", "avatar": null, "bot": author_bot },
            "content": content,
            "timestamp": "2024-01-01T00:00:00.000000+00:00",
            "edited_timestamp": null,
            "tts": false,
            "mention_everyone": false,
            "mentions": [],
            "mention_roles": [],
            "pinned": pinned,
            "attachments": attachments,
            "embeds": [],
            "flags": 0,
        }))
        .expect("message JSON")
    }

    #[test]
    fn purge_takes_what_every_filter_allows() {
        let now = chrono::Utc::now();
        let all = PurgeFilter::default();
        assert!(all.takes(&message(0, false, "hi", false, false), now));
        assert!(!all.takes(&message(0, false, "hi", true, false), now), "pinned stays");
        assert!(
            !all.takes(&message(15, false, "hi", false, false), now),
            "too old to bulk delete"
        );
        let from = PurgeFilter {
            user: Some(5),
            ..PurgeFilter::default()
        };
        assert!(from.takes(&message(1, false, "hi", false, false), now));
        assert!(!from.takes(&message(1, true, "hi", false, false), now));
        let bots = PurgeFilter {
            bots: true,
            ..PurgeFilter::default()
        };
        assert!(bots.takes(&message(1, true, "hi", false, false), now));
        assert!(!bots.takes(&message(1, false, "hi", false, false), now));
        let links = PurgeFilter {
            links: true,
            contains: Some("FREE".into()),
            ..PurgeFilter::default()
        };
        assert!(links.takes(&message(1, false, "free nitro https://x.y", false, false), now));
        assert!(!links.takes(&message(1, false, "free nitro", false, false), now));
        let files = PurgeFilter {
            attachments: true,
            ..PurgeFilter::default()
        };
        assert!(files.takes(&message(1, false, "", false, true), now));
        assert!(!files.takes(&message(1, false, "", false, false), now));
        assert_eq!(
            PurgeFilter {
                bots: true,
                links: true,
                ..PurgeFilter::default()
            }
            .describe(Lang::En, Some("Mia")),
            "from Mia, from bots, with links"
        );
    }

    #[test]
    fn purge_variants_keep_boundaries_and_combine_content_with_author_filters() {
        let now = chrono::Utc::now();
        let mut msg = message(1, false, "offer 42 end", false, false);
        msg.embeds.push(
            twilight_util::builder::embed::EmbedBuilder::new()
                .description("Evidence")
                .build(),
        );
        let mut filter = PurgeFilter {
            humans: true,
            embeds: true,
            regex: Some(r"offer \d+".into()),
            starts: Some("OFFER".into()),
            ends: Some("END".into()),
            not: Some("safe".into()),
            after: Some(msg.id.get() - 1),
            before: Some(msg.id.get() + 1),
            ..Default::default()
        };
        assert!(filter.reads());
        assert!(filter.takes(&msg, now));
        filter.after = Some(msg.id.get());
        assert!(!filter.takes(&msg, now), "the lower boundary is exclusive");
        filter.after = None;
        filter.before = Some(msg.id.get());
        assert!(!filter.takes(&msg, now), "the upper boundary is exclusive");
        filter.before = None;
        msg.author.bot = true;
        assert!(!filter.takes(&msg, now));
        msg.author.bot = false;
        msg.pinned = true;
        assert!(!filter.takes(&msg, now));
        msg.pinned = false;
        msg.content.push_str(" safe");
        assert!(!filter.takes(&msg, now));
    }

    #[test]
    fn purge_mentions_include_everyone_and_age_boundary_is_strict() {
        let now = chrono::Utc::now();
        let mut msg = message(0, false, "@everyone", false, false);
        msg.mention_everyone = true;
        let filter = PurgeFilter {
            mentions: true,
            ..Default::default()
        };
        assert!(filter.takes(&msg, now));
        for (age, expected) in [
            (PURGE_AGE - Duration::seconds(61), true),
            (PURGE_AGE - Duration::seconds(60), false),
            (PURGE_AGE, false),
        ] {
            msg.id = Id::new(((now - age).timestamp_millis() as u64 - 1_420_070_400_000) << 22);
            assert_eq!(filter.takes(&msg, now), expected);
        }
    }

    #[test]
    fn locking_takes_sending_from_everyone_and_unlocking_gives_it_back() {
        let view = Permissions::VIEW_CHANNEL;
        let send = Permissions::SEND_MESSAGES;
        let (allow, deny) = lock_change(view | send, Permissions::empty(), true, None).unwrap();
        assert_eq!(allow, view);
        assert!(deny.contains(LOCKED));
        assert_eq!(lock_change(allow, deny, true, None), None, "already locked");
        // Unlocking puts back what @everyone had: here an explicit Send Messages.
        let (allow, deny) = lock_change(allow, deny, false, Some((send, Permissions::empty()))).unwrap();
        assert_eq!((allow, deny), (view | send, Permissions::empty()));
        assert_eq!(lock_change(allow, deny, false, None), None, "not locked");
        // Without a lock to ask (locked by hand), the locked permissions go back to neutral.
        let (allow, deny) = lock_change(Permissions::empty(), send, false, None).unwrap();
        assert!(allow.is_empty() && deny.is_empty(), "the overwrite can go");
        // A thread permission that was denied before the lock stays denied after it.
        let threads = Permissions::CREATE_PUBLIC_THREADS;
        let (allow, deny) = lock_change(Permissions::empty(), threads, true, None).unwrap();
        let (_, deny) = lock_change(allow, deny, false, Some((Permissions::empty(), threads))).unwrap();
        assert_eq!(deny, threads);
    }
}
