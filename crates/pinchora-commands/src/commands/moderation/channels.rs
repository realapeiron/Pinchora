//! The commands that act on a channel: `/purge`, `/slowmode`, `/lock` and `/unlock`.

use chrono::Duration;
use pinchora_core::{
    moderation::{CaseKind, PURGE_AGE, check_reason, format_duration, parse_slowmode},
    snowflake,
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
    permitted, record, reply_embed,
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
        cx.reply_ephemeral("Moderation only works inside a server.").await?;
        return Ok(None);
    };
    if allowed_in(cx, named, Permissions::MANAGE_CHANNELS) {
        return Ok(Some(guild));
    }
    cx.reply_ephemeral("You need the Manage Channels permission to do that.")
        .await?;
    Ok(None)
}

/// Which messages `/purge` takes.
#[derive(Debug, Clone, Default)]
pub(crate) struct PurgeFilter {
    pub(crate) user: Option<u64>,
    pub(crate) bots: bool,
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

    /// The filters in words, for the case: "from @Mia, with links".
    fn describe(&self, user_name: Option<&str>) -> String {
        let mut parts = Vec::new();
        if let Some(name) = user_name {
            parts.push(format!("from {name}"));
        }
        if self.bots {
            parts.push("from bots".to_owned());
        }
        if let Some(text) = &self.contains {
            parts.push(format!("containing `{}`", text.replace('`', "'")));
        }
        if self.links {
            parts.push("with links".to_owned());
        }
        if self.attachments {
            parts.push("with attachments".to_owned());
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
        let Some(guild) = permitted(cx, Permissions::MANAGE_MESSAGES, "Manage Messages").await? else {
            return Ok(());
        };
        let reason = match check_reason(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(error) => return cx.reply_ephemeral(error.message).await,
        };
        let filter = PurgeFilter {
            user: self.user.as_ref().map(|u| u.resolved.id.get()),
            bots: self.bots == Some(true),
            contains: self.contains.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()),
            links: self.links == Some(true),
            attachments: self.attachments == Some(true),
        };
        // Without the Message Content intent Discord sends other people's messages without their text.
        if (filter.contains.is_some() || filter.links) && !cx.app.message_content {
            return cx
                .reply_ephemeral(
                    "The `contains` and `links` filters need to read messages, which this bot does not do. \
                     Use the member, bots or attachments filters instead.",
                )
                .await;
        }
        let Some((channel, channel_name)) = target_channel(cx, None) else {
            return cx.reply_ephemeral("I can't tell which channel this is.").await;
        };
        let messages = match cx.app.http.channel_messages(Id::new(channel)).limit(100).await {
            Ok(response) => response.models().await?,
            Err(error) => {
                return cx
                    .reply_ephemeral(format!("I couldn't read the channel: {}.", refusal(&error)))
                    .await;
            }
        };
        let now = cx.app.now();
        let ids: Vec<_> = messages
            .iter()
            .filter(|m| filter.takes(m, now))
            .take(self.count.clamp(1, 100) as usize)
            .map(|m| m.id)
            .collect();
        if ids.is_empty() {
            return cx
                .reply_ephemeral("No messages from the last 14 days match that, so nothing was deleted.")
                .await;
        }
        let moderator = Moderator::of(cx)?;
        let audit = audit_reason(&moderator, reason.as_deref());
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
            return cx
                .reply_ephemeral(format!("Discord refused to delete them: {}.", refusal(&error)))
                .await;
        }
        let n = ids.len();
        let described = filter.describe(self.user.as_ref().map(|u| u.resolved.name.as_str()));
        let mut details = format!("Deleted {n} message{}", if n == 1 { "" } else { "s" });
        if !described.is_empty() {
            details.push_str(&format!(" ({described})"));
        }
        let act = Act {
            kind: CaseKind::Purge,
            target: channel,
            target_name: format!("#{channel_name}"),
            reason,
            duration: None,
            details: Some(details),
        };
        let opened = record(&cx.app, guild, (moderator.id, &moderator.name), &act, None).await?;
        cx.reply(private(reply_embed(&opened.case, &[]))).await
    }
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
        let Some(guild) = channel_permitted(cx, self.channel.as_ref()).await? else {
            return Ok(());
        };
        let reason = match check_reason(self.reason.as_deref()) {
            Ok(reason) => reason,
            Err(error) => return cx.reply_ephemeral(error.message).await,
        };
        let seconds = match parse_slowmode(&self.duration) {
            Ok(s) => s,
            Err(error) => return cx.reply_ephemeral(error.message).await,
        };
        let Some((channel, name)) = target_channel(cx, self.channel.as_ref()) else {
            return cx.reply_ephemeral("I can't tell which channel this is.").await;
        };
        let moderator = Moderator::of(cx)?;
        let audit = audit_reason(&moderator, reason.as_deref());
        if let Err(error) = cx
            .app
            .http
            .update_channel(Id::new(channel))
            .rate_limit_per_user(seconds)
            .reason(&audit)
            .await
        {
            return cx
                .reply_ephemeral(format!("Discord refused it: {}.", refusal(&error)))
                .await;
        }
        let details = if seconds == 0 {
            "off".to_owned()
        } else {
            format!(
                "one message every {}",
                format_duration(Duration::seconds(seconds.into()))
            )
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
        cx.reply(public(reply_embed(&opened.case, &[]))).await
    }
}

/// What @everyone's overwrite becomes when a channel is locked or unlocked, or `None` when it already
/// is: `(allow, deny)`, both empty meaning the overwrite can go.
pub(crate) fn lock_change(allow: Permissions, deny: Permissions, lock: bool) -> Option<(Permissions, Permissions)> {
    let locked = deny.contains(Permissions::SEND_MESSAGES);
    match (lock, locked) {
        (true, true) | (false, false) => None,
        (true, false) => Some((allow - LOCKED, deny | LOCKED)),
        (false, true) => Some((allow, deny - LOCKED)),
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
    let Some(guild) = channel_permitted(cx, named).await? else {
        return Ok(());
    };
    let reason = match check_reason(reason) {
        Ok(reason) => reason,
        Err(error) => return cx.reply_ephemeral(error.message).await,
    };
    let Some((channel, name)) = target_channel(cx, named) else {
        return cx.reply_ephemeral("I can't tell which channel this is.").await;
    };
    let http = &cx.app.http;
    let found = match http.channel(Id::new(channel)).await {
        Ok(response) => response.model().await?,
        Err(error) => {
            return cx
                .reply_ephemeral(format!("I couldn't read the channel: {}.", refusal(&error)))
                .await;
        }
    };
    let everyone: Option<&Overwrite> = found
        .permission_overwrites
        .iter()
        .flatten()
        .find(|o| o.id.get() == guild);
    let (allow, deny) = everyone.map_or((Permissions::empty(), Permissions::empty()), |o| (o.allow, o.deny));
    let Some((allow, deny)) = lock_change(allow, deny, lock) else {
        let state = if lock { "already locked" } else { "not locked" };
        return cx.reply_ephemeral(format!("<#{channel}> is {state}.")).await;
    };
    let moderator = Moderator::of(cx)?;
    let audit = audit_reason(&moderator, reason.as_deref());
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
            .reply_ephemeral(format!("Discord refused it: {}.", refusal(&error)))
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
    cx.reply(public(reply_embed(&opened.case, &[]))).await
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
            .describe(Some("Mia")),
            "from Mia, from bots, with links"
        );
    }

    #[test]
    fn locking_takes_sending_from_everyone_and_unlocking_gives_it_back() {
        let view = Permissions::VIEW_CHANNEL;
        let (allow, deny) = lock_change(view | Permissions::SEND_MESSAGES, Permissions::empty(), true).unwrap();
        assert_eq!(allow, view);
        assert!(deny.contains(LOCKED));
        assert_eq!(lock_change(allow, deny, true), None, "already locked");
        let (allow, deny) = lock_change(allow, deny, false).unwrap();
        assert_eq!((allow, deny), (view, Permissions::empty()));
        assert_eq!(lock_change(allow, deny, false), None, "not locked");
        let (allow, deny) = lock_change(Permissions::empty(), Permissions::SEND_MESSAGES, false).unwrap();
        assert!(allow.is_empty() && deny.is_empty(), "the overwrite can go");
    }
}
