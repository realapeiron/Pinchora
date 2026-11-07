//! `/feeds`: follow YouTube channels, RSS feeds and podcasts, subreddits and Bluesky accounts, and post
//! what is new in a channel. The poller and the message live in `crate::feeds`; this is the manager's
//! side, the same rules as the dashboard's Feeds page.

use std::str::Split;

use pinchora_core::{
    feeds::{FILTER_WORD_MAX, FILTER_WORDS_MAX, Kind, MAX_FEEDS, MESSAGE_MAX, check_message, filter_words},
    i18n::Lang,
    t,
};
use pinchora_db::{AuditSource, FeedAdd, FeedChange, FeedEdit, FeedListRow, FeedRow, NewAudit, NewFeed, PostOutcome};
use serde_json::{Value, json};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{Component, Embed, MessageFlags, component::ButtonStyle},
    guild::{Permissions, Role},
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder};

use crate::{
    Cx,
    commands::games::quiet,
    custom::scrub_value,
    feeds::{self, error_text, kind_label},
    server_events::{button, row},
    util::BRAND,
};

/// Feeds on one page of `/feeds list`.
const PAGE: usize = 5;

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "feeds",
    desc = "Post new YouTube videos, RSS and podcast episodes, Reddit and Bluesky posts in a channel",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
pub enum FeedsCommand {
    #[command(name = "add")]
    Add(FeedsAdd),
    #[command(name = "list")]
    List(FeedsList),
    #[command(name = "edit")]
    Edit(FeedsEdit),
    #[command(name = "test")]
    Test(FeedsTest),
    #[command(name = "remove")]
    Remove(FeedsRemove),
}

/// Where a feed's posts come from.
#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedKind {
    #[option(name = "YouTube", value = "youtube")]
    YouTube,
    #[option(name = "RSS or podcast", value = "rss")]
    Rss,
    #[option(name = "Reddit", value = "reddit")]
    Reddit,
    #[option(name = "Bluesky", value = "bluesky")]
    Bluesky,
    #[option(name = "X, Instagram or TikTok (bridge)", value = "bridge")]
    Bridge,
    #[option(name = "Twitch", value = "twitch")]
    Twitch,
    #[option(name = "Kick", value = "kick")]
    Kick,
}

impl FeedKind {
    fn kind(self) -> Kind {
        match self {
            Self::YouTube => Kind::YouTube,
            Self::Rss => Kind::Rss,
            Self::Reddit => Kind::Reddit,
            Self::Bluesky => Kind::Bluesky,
            Self::Bridge => Kind::Bridge,
            Self::Twitch => Kind::Twitch,
            Self::Kick => Kind::Kick,
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "add", desc = "Follow a source and post what is new in a channel")]
pub struct FeedsAdd {
    /// Where the posts come from
    kind: FeedKind,
    /// A channel link, @handle or id; a feed address; r/subreddit; a Bluesky handle
    #[command(min_length = 1, max_length = 500)]
    source: String,
    /// Where new posts go
    #[command(channel_types = "guild_text guild_announcement")]
    channel: InteractionChannel,
    /// The text: {title} {url} {author} {account} {published} (default: one per kind)
    #[command(max_length = 500)]
    message: Option<String>,
    /// A role to ping with every new post
    role: Option<Role>,
    /// Give this role to the verified streamer while live (Twitch or Kick only)
    live_role: Option<Role>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "list", desc = "The feeds this server follows, with how they are doing")]
pub struct FeedsList;

#[derive(CommandModel, CreateCommand)]
#[command(name = "edit", desc = "Change a feed's channel, message, role, card or filters")]
pub struct FeedsEdit {
    /// The feed's number, from /feeds list
    #[command(min_value = 1)]
    id: i64,
    /// Post in this channel instead
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// The new text; - goes back to the default
    #[command(max_length = 500)]
    message: Option<String>,
    /// Ping this role with every new post
    role: Option<Role>,
    /// Stop pinging a role
    no_role: Option<bool>,
    /// Give this role to the verified streamer while live
    live_role: Option<Role>,
    /// Stop giving a live role
    no_live_role: Option<bool>,
    /// Add Pinchora's card (title, picture, summary) under the text
    embed: Option<bool>,
    /// Only post titles with one of these words, comma-separated; - clears
    #[command(max_length = 450)]
    include: Option<String>,
    /// Never post titles with these words, comma-separated; - clears
    #[command(max_length = 450)]
    exclude: Option<String>,
    /// Switch the feed on or off
    enabled: Option<bool>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "test", desc = "Post the source's latest item in the feed's channel now")]
pub struct FeedsTest {
    /// The feed's number, from /feeds list
    #[command(min_value = 1)]
    id: i64,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "remove", desc = "Stop following a source")]
pub struct FeedsRemove {
    /// The feed's number, from /feeds list
    #[command(min_value = 1)]
    id: i64,
}

impl FeedsCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let lang = cx.lang();
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(lang, "error.guild-only")).await;
        };
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(lang, "error.needs-manage-server")).await;
        }
        match self {
            Self::Add(c) => add(cx, guild, c).await,
            Self::List(_) => list(cx, guild).await,
            Self::Edit(c) => edit(cx, guild, c).await,
            Self::Test(c) => test(cx, guild, c.id).await,
            Self::Remove(c) => remove(cx, guild, c.id).await,
        }
    }
}

/// How a feed is named in answers: its number, kind and source.
fn feed_name(lang: Lang, feed: &FeedRow) -> String {
    let kind = Kind::parse(&feed.kind).unwrap_or(Kind::Rss);
    let account = pinchora_core::feeds::account_label(kind, &feed.source, feed.display_name.as_deref());
    t!(
        lang,
        "feeds.name",
        id = feed.id,
        kind = kind_label(lang, kind),
        account = scrub_value(&account)
    )
}

/// What the audit log keeps of a feed.
fn feed_json(feed: &FeedRow) -> Value {
    json!({
        "kind": feed.kind,
        "source": feed.source,
        "channel_id": feed.channel_id.to_string(),
        "message": feed.message,
        "embed": feed.embed,
        "ping_role_id": feed.ping_role_id.map(|r| r.to_string()),
        "live_role_id": feed.live_role_id.map(|r| r.to_string()),
        "filter_include": feed.filter_include,
        "filter_exclude": feed.filter_exclude,
        "enabled": feed.enabled,
    })
}

async fn audit(cx: &Cx, guild: u64, action: &str, id: i64, before: Option<Value>, after: Option<Value>) {
    let Ok(actor) = cx.author() else { return };
    let (target, before, after) = (
        id.to_string(),
        before.map(|v| v.to_string()),
        after.map(|v| v.to_string()),
    );
    let written = cx
        .app
        .db
        .audit(&NewAudit {
            guild,
            actor: actor.id.get(),
            actor_name: &cx.author_display_name(),
            source: AuditSource::Discord,
            action,
            target: Some(&target),
            before: before.as_deref(),
            after: after.as_deref(),
        })
        .await;
    if let Err(error) = written {
        tracing::error!(error = ?error, guild, action, "a feed change was made but not written to the audit log");
    }
}

/// The role a feed may ping: never @everyone, whose id is the server's.
fn ping_role(guild: u64, role: Option<&Role>) -> Option<u64> {
    role.map(|r| r.id.get()).filter(|r| *r != guild)
}

async fn add(cx: &Cx, guild: u64, c: FeedsAdd) -> anyhow::Result<()> {
    let lang = cx.lang();
    let kind = c.kind.kind();
    let message = match check_message(c.message.as_deref()) {
        Ok(message) => message,
        Err(error) => return cx.reply_ephemeral(t!(lang, error.key(), max = MESSAGE_MAX)).await,
    };
    if c.role.as_ref().is_some_and(|r| r.id.get() == guild) {
        return cx.reply_ephemeral(t!(lang, "feeds.no-everyone")).await;
    }
    if let Some(role) = c.live_role.as_ref() {
        if !matches!(kind, Kind::Twitch | Kind::Kick) {
            return cx.reply_ephemeral(t!(lang, "feeds.api.live-only")).await;
        }
        if let Some(why) = crate::roles::unassignable(cx, guild, role).await {
            return cx.reply_ephemeral(why).await;
        }
    }
    if !feeds::spend_lookup(&cx.app, guild).await {
        return cx.reply_ephemeral(t!(lang, "feeds.slow-down")).await;
    }
    let found = match feeds::look_up(kind, &c.source).await {
        Ok(found) => found,
        Err(problem) => return cx.reply_ephemeral(problem.text(lang)).await,
    };
    let seen: Vec<String> = found.items.iter().map(|i| i.id.clone()).collect();
    let channel = c.channel.id.get();
    let added = cx
        .app
        .db
        .add_feed(&NewFeed {
            guild,
            kind: kind.key(),
            source: &found.source,
            title: found.title.as_deref(),
            channel,
            message: message.as_deref(),
            // YouTube links get Discord's own video player; everything else reads better as a card.
            embed: kind != Kind::YouTube,
            ping_role: ping_role(guild, c.role.as_ref()),
            live_role: c.live_role.as_ref().map(|role| role.id.get()),
            include: &[],
            exclude: &[],
            created_by: cx.author()?.id.get(),
            seen: &seen,
            interval_secs: found.interval_secs,
        })
        .await?;
    let feed = match added {
        FeedAdd::Added(feed) => feed,
        FeedAdd::Full => return cx.reply_ephemeral(t!(lang, "feeds.full", max = MAX_FEEDS)).await,
        FeedAdd::Duplicate => {
            return cx
                .reply_ephemeral(t!(lang, "feeds.duplicate", channel = format!("<#{channel}>")))
                .await;
        }
    };
    audit(cx, guild, "feeds.add", feed.id, None, Some(feed_json(&feed))).await;
    let latest = found.latest().map_or_else(
        || t!(lang, "feeds.added-empty"),
        |item| t!(lang, "feeds.added-latest", title = scrub_value(&item.title)),
    );
    let minutes = (found.interval_secs + 59) / 60;
    cx.reply_ephemeral(t!(
        lang,
        "feeds.added",
        name = feed_name(lang, &feed),
        channel = format!("<#{channel}>"),
        latest = latest,
        minutes = minutes,
        id = feed.id
    ))
    .await
}

/// A feed's state in a word or two: off, failing (and why), or when it last looked and posted.
fn health(lang: Lang, row: &FeedListRow) -> String {
    let (feed, source) = (&row.feed, &row.source);
    if !feed.enabled {
        return match &feed.last_error {
            Some(error) => t!(lang, "feeds.health.off-because", why = error_text(lang, error)),
            None => t!(lang, "feeds.health.off"),
        };
    }
    if let Some(error) = &feed.last_error {
        return t!(lang, "feeds.health.cannot-post", why = error_text(lang, error));
    }
    if let Some(error) = source.last_error.as_deref().filter(|_| source.fail_count > 0) {
        return t!(lang, "feeds.health.failing", why = error_text(lang, error));
    }
    let checked = source.last_checked_at.map_or_else(
        || t!(lang, "feeds.health.not-yet"),
        |at| format!("<t:{}:R>", at.timestamp()),
    );
    let posted = feed.last_sent_at.map_or_else(
        || t!(lang, "feeds.health.not-yet"),
        |at| format!("<t:{}:R>", at.timestamp()),
    );
    t!(lang, "feeds.health.ok", checked = checked, posted = posted)
}

/// One page of the list, and the buttons to the others.
fn page_view(lang: Lang, rows: &[FeedListRow], page: usize) -> (Embed, Vec<Component>) {
    let pages = rows.len().div_ceil(PAGE).max(1);
    let page = page.min(pages - 1);
    let lines: Vec<String> = rows
        .iter()
        .skip(page * PAGE)
        .take(PAGE)
        .map(|row| {
            let feed = &row.feed;
            let mut line = format!(
                "{} → <#{}>\n{}",
                feed_name(lang, feed),
                feed.channel_id,
                health(lang, row)
            );
            if let Some(role) = feed.ping_role_id {
                line.push_str(&t!(lang, "feeds.list-pings", role = format!("<@&{role}>")));
            }
            if let Some(role) = feed.live_role_id {
                line.push_str(&t!(lang, "feeds.list-live-role", role = format!("<@&{role}>")));
            }
            line
        })
        .collect();
    let description = if lines.is_empty() {
        t!(lang, "feeds.list-empty")
    } else {
        lines.join("\n\n")
    };
    let embed = EmbedBuilder::new()
        .title(t!(lang, "feeds.list-title", count = rows.len(), max = MAX_FEEDS))
        .description(description)
        .color(BRAND)
        .footer(EmbedFooterBuilder::new(t!(
            lang,
            "feeds.list-footer",
            page = page + 1,
            pages = pages
        )))
        .build();
    let mut components = Vec::new();
    if pages > 1 {
        components.push(row(vec![
            button(
                format!("sn:l:{}", page.saturating_sub(1)),
                &t!(lang, "feeds.previous"),
                None,
                ButtonStyle::Secondary,
                page == 0,
            ),
            button(
                format!("sn:l:{}", page + 1),
                &t!(lang, "feeds.next"),
                None,
                ButtonStyle::Secondary,
                page + 1 >= pages,
            ),
        ]));
    }
    (embed, components)
}

async fn list(cx: &Cx, guild: u64) -> anyhow::Result<()> {
    let rows = cx.app.db.feeds_of(guild).await?;
    let (embed, components) = page_view(cx.lang(), &rows, 0);
    cx.reply(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(components),
        flags: Some(MessageFlags::EPHEMERAL),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

/// `sn:l:<page>`: another page of `/feeds list`.
pub(crate) async fn on_component(cx: &Cx, mut parts: Split<'_, char>) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(guild) = cx.guild_id() else {
        return cx.reply_ephemeral(t!(lang, "error.guild-only")).await;
    };
    if parts.next() != Some("l") {
        return cx.reply_ephemeral(t!(lang, "feeds.expired")).await;
    }
    if !cx.can_manage_guild() {
        return cx.reply_ephemeral(t!(lang, "error.needs-manage-server")).await;
    }
    let page = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let rows = cx.app.db.feeds_of(guild).await?;
    let (embed, components) = page_view(lang, &rows, page);
    cx.update(InteractionResponseData {
        embeds: Some(vec![embed]),
        components: Some(components),
        allowed_mentions: quiet(),
        ..Default::default()
    })
    .await
}

/// A filter option: `-` clears it, otherwise comma-separated words.
fn filter_option(lang: Lang, text: Option<&str>) -> Result<Option<Vec<String>>, String> {
    match text.map(str::trim) {
        None => Ok(None),
        Some("-") => Ok(Some(Vec::new())),
        Some(text) => filter_words([text])
            .map(Some)
            .map_err(|error| t!(lang, error.key(), max = FILTER_WORDS_MAX, length = FILTER_WORD_MAX)),
    }
}

async fn edit(cx: &Cx, guild: u64, c: FeedsEdit) -> anyhow::Result<()> {
    let lang = cx.lang();
    let message = match c.message.as_deref().map(str::trim) {
        None => None,
        Some("-") => Some(None),
        Some(text) => match check_message(Some(text)) {
            Ok(message) => Some(message),
            Err(error) => return cx.reply_ephemeral(t!(lang, error.key(), max = MESSAGE_MAX)).await,
        },
    };
    if c.role.as_ref().is_some_and(|r| r.id.get() == guild) {
        return cx.reply_ephemeral(t!(lang, "feeds.no-everyone")).await;
    }
    let (include, exclude) = match (
        filter_option(lang, c.include.as_deref()),
        filter_option(lang, c.exclude.as_deref()),
    ) {
        (Ok(include), Ok(exclude)) => (include, exclude),
        (Err(why), _) | (_, Err(why)) => return cx.reply_ephemeral(why).await,
    };
    let ping_role = match (ping_role(guild, c.role.as_ref()), c.no_role) {
        (Some(role), _) => Some(Some(role)),
        (None, Some(true)) => Some(None),
        _ => None,
    };
    if let Some(role) = c.live_role.as_ref()
        && let Some(why) = crate::roles::unassignable(cx, guild, role).await
    {
        return cx.reply_ephemeral(why).await;
    }
    let live_role = match (c.live_role.as_ref(), c.no_live_role) {
        (Some(role), _) => Some(Some(role.id.get())),
        (None, Some(true)) => Some(None),
        _ => None,
    };
    if live_role.flatten().is_some() {
        let feed = cx.app.db.feed(guild, c.id).await?;
        if feed.is_some_and(|feed| !matches!(Kind::parse(&feed.kind), Some(Kind::Twitch | Kind::Kick))) {
            return cx.reply_ephemeral(t!(lang, "feeds.api.live-only")).await;
        }
    }
    let change = FeedEdit {
        channel: c.channel.as_ref().map(|ch| ch.id.get()),
        message,
        embed: c.embed,
        ping_role,
        include,
        exclude,
        enabled: c.enabled,
        live_role,
    };
    let nothing = change.channel.is_none()
        && change.message.is_none()
        && change.embed.is_none()
        && change.ping_role.is_none()
        && change.include.is_none()
        && change.exclude.is_none()
        && change.enabled.is_none()
        && change.live_role.is_none();
    if nothing {
        return cx.reply_ephemeral(t!(lang, "feeds.edit-nothing")).await;
    }
    let current = cx.app.db.feed(guild, c.id).await?;
    let guard = match feeds::before_live_change(&cx.app, current.as_ref(), Some(&change)).await {
        Ok(guard) => guard,
        Err(_) => return cx.reply_ephemeral(t!(lang, "feeds.link.role-retry")).await,
    };
    let result = cx.app.db.edit_feed(guild, c.id, &change).await?;
    drop(guard);
    match result {
        FeedChange::NotFound => cx.reply_ephemeral(t!(lang, "feeds.no-such", id = c.id)).await,
        FeedChange::Duplicate => {
            let channel = change.channel.map_or_else(String::new, |ch| format!("<#{ch}>"));
            cx.reply_ephemeral(t!(lang, "feeds.duplicate", channel = channel)).await
        }
        FeedChange::Changed { before, after } => {
            feeds::reconcile_live_edit(&cx.app, &before, &after).await;
            audit(
                cx,
                guild,
                "feeds.edit",
                after.id,
                Some(feed_json(&before)),
                Some(feed_json(&after)),
            )
            .await;
            cx.reply_ephemeral(t!(
                lang,
                "feeds.edited",
                name = feed_name(lang, &after),
                channel = format!("<#{}>", after.channel_id)
            ))
            .await
        }
    }
}

async fn test(cx: &Cx, guild: u64, id: i64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let Some(feed) = cx.app.db.feed(guild, id).await? else {
        return cx.reply_ephemeral(t!(lang, "feeds.no-such", id = id)).await;
    };
    let Some(kind) = Kind::parse(&feed.kind) else {
        return cx.reply_ephemeral(t!(lang, "feeds.no-such", id = id)).await;
    };
    if !feeds::spend_lookup(&cx.app, guild).await {
        return cx.reply_ephemeral(t!(lang, "feeds.slow-down")).await;
    }
    let read = match feeds::fetch_source(kind, &feed.source, None, None).await {
        Ok(Some(read)) => read,
        Ok(None) => return cx.reply_ephemeral(t!(lang, "feeds.test-empty")).await,
        Err(problem) => {
            return cx
                .reply_ephemeral(t!(
                    lang,
                    "feeds.test-cannot-read",
                    why = error_text(lang, &problem.code())
                ))
                .await;
        }
    };
    let Some(item) = read.parsed.items.first() else {
        return cx.reply_ephemeral(t!(lang, "feeds.test-empty")).await;
    };
    let channel = format!("<#{}>", feed.channel_id);
    match feeds::send(&cx.app, &feed, item, false).await {
        PostOutcome::Sent => {
            cx.reply_ephemeral(t!(
                lang,
                "feeds.test-sent",
                name = feed_name(lang, &feed),
                channel = channel
            ))
            .await
        }
        PostOutcome::Refused(code) | PostOutcome::Retry(code) => {
            cx.reply_ephemeral(t!(
                lang,
                "feeds.test-refused",
                channel = channel,
                why = error_text(lang, &code)
            ))
            .await
        }
        PostOutcome::Dropped => cx.reply_ephemeral(t!(lang, "feeds.no-such", id = id)).await,
    }
}

async fn remove(cx: &Cx, guild: u64, id: i64) -> anyhow::Result<()> {
    let lang = cx.lang();
    let current = cx.app.db.feed(guild, id).await?;
    let guard = match feeds::before_live_change(&cx.app, current.as_ref(), None).await {
        Ok(guard) => guard,
        Err(_) => return cx.reply_ephemeral(t!(lang, "feeds.link.role-retry")).await,
    };
    let removed = cx.app.db.remove_feed(guild, id).await?;
    drop(guard);
    let Some(feed) = removed else {
        return cx.reply_ephemeral(t!(lang, "feeds.no-such", id = id)).await;
    };
    if feed.live_message_id.is_some() {
        feeds::end_live(&cx.app, &feed).await;
    }
    audit(cx, guild, "feeds.remove", feed.id, Some(feed_json(&feed)), None).await;
    cx.reply_ephemeral(t!(lang, "feeds.removed", name = feed_name(lang, &feed)))
        .await
}
