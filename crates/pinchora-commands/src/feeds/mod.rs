//! Social feeds: the poller that fetches every followed source once, however many servers follow it,
//! and posts what is new in each server's channel.
//!
//! Every pod runs the ticker. Each step leases the sources that are due (`FOR UPDATE SKIP LOCKED`),
//! fetches them a few at a time through the guarded [`Fetcher`], records what is new and queues one
//! post per subscription in the same transaction, then sends the queued posts it can lease. A source
//! that fails waits longer each time ([`backoff_secs`]) and says why on the dashboard.

pub(crate) mod fetch;
pub mod live;
pub(crate) mod message;
pub(crate) mod parse;

use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use chrono::Duration as Span;
use futures_util::StreamExt;
use pinchora_core::{
    feeds::{
        BACKOFF_MAX_SECS, Item, Kind, Parsed as Source, SourceError, account_label, backoff_secs, feed_url,
        parse_source, poll_interval, youtube_channel_in_page,
    },
    i18n::Lang,
    modules::Module,
    t,
};
use pinchora_db::{FeedPost, FeedRow, FeedSourceRow, Polled, PostOutcome};
use sha2::{Digest, Sha256};
use twilight_http::request::AuditLogReason;
use twilight_model::{channel::message::AllowedMentions, id::Id};

pub use fetch::{FetchError, Fetched, Fetcher, Request, fetcher, install_fetcher};
pub use message::{Draft, FeedEmbed, FeedMessage, default_message, kind_label, render, sample_item};
pub use parse::ParseError;

use crate::{App, roles::error_code, server_events::is_permanent};

/// The ticker wakes this often.
const WAKE: Duration = Duration::from_secs(30);
/// Sources one step leases, and how many it fetches at once.
const POLL_BATCH: i64 = 25;
const POLL_AT_ONCE: usize = 4;
/// Posts one step sends.
const POST_BATCH: i64 = 50;
/// What a feed no longer shows is forgotten after this long.
const SEEN_KEPT: Span = Span::days(7);
/// Cleanup runs every this many wakes (hourly).
const CLEANUP_EVERY: u32 = 120;

/// Lookups (adding, checking or testing a feed) one server may make a minute: each one fetches a page
/// from the internet.
const LOOKUPS_PER_MINUTE: i64 = 20;

/// Counts a lookup for `guild`; false once it made [`LOOKUPS_PER_MINUTE`] this minute. When Redis
/// cannot be asked, the lookup goes ahead: every fetch is guarded anyway.
pub async fn spend_lookup(app: &App, guild: u64) -> bool {
    match app
        .cache
        .incr(&format!("feeds:lookups:{guild}"), Duration::from_secs(60))
        .await
    {
        Ok(count) => count <= LOOKUPS_PER_MINUTE,
        Err(error) => {
            tracing::warn!(%error, guild, "feed lookup budget could not be read");
            true
        }
    }
}

/// What this instance offers, from its environment: the bridge for X, Instagram and TikTok
/// (`PINCHORA_FEEDS__BRIDGE_URL`, an RSSHub-style address with `{route}`), and Reddit, which an operator
/// can switch off (`PINCHORA_FEEDS__REDDIT=false`) if Reddit blocks the instance.
pub struct Instance {
    pub bridge: Option<String>,
    pub reddit: bool,
    pub twitch: bool,
    pub kick: bool,
}

static INSTANCE: LazyLock<Instance> = LazyLock::new(|| Instance {
    bridge: std::env::var("PINCHORA_FEEDS__BRIDGE_URL")
        .ok()
        .map(|b| b.trim().to_owned())
        .filter(|b| b.starts_with("https://") || b.starts_with("http://")),
    reddit: !std::env::var("PINCHORA_FEEDS__REDDIT")
        .is_ok_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "false" | "off" | "0" | "no")),
    twitch: live::configured(Kind::Twitch),
    kick: live::configured(Kind::Kick),
});

pub fn instance() -> &'static Instance {
    &INSTANCE
}

/// Whether this instance can follow sources of `kind`.
pub fn available(kind: Kind) -> bool {
    available_in(kind, instance())
}

fn available_in(kind: Kind, settings: &Instance) -> bool {
    match kind {
        Kind::Bridge => settings.bridge.is_some(),
        Kind::Reddit => settings.reddit,
        Kind::Twitch => settings.twitch,
        Kind::Kick => settings.kick,
        _ => true,
    }
}

/// Why a source cannot be followed or read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Source(SourceError),
    Unavailable(Kind),
    Fetch(FetchError),
    Parse(ParseError),
    /// A YouTube page that names no channel (or a consent page in its place).
    YouTubePage,
}

impl Problem {
    /// The stored form, for the dashboard; [`error_text`] words it.
    pub fn code(&self) -> String {
        match self {
            Self::Source(error) => error.key().to_owned(),
            Self::Unavailable(kind) => format!("feeds.unavailable.{}", kind.key()),
            Self::Fetch(error) => error.code(),
            Self::Parse(error) => error.code().to_owned(),
            Self::YouTubePage => "feeds.error.youtube-page".to_owned(),
        }
    }

    /// The sentence for a manager who asked to follow the source.
    pub fn text(&self, lang: Lang) -> String {
        match self {
            Self::Source(_) | Self::Unavailable(_) => error_text(lang, &self.code()),
            _ => t!(lang, "feeds.cannot-read", why = error_text(lang, &self.code())),
        }
    }
}

/// A stored problem (`key` or `key|status`) in `lang`.
pub fn error_text(lang: Lang, code: &str) -> String {
    let (key, status) = code.split_once('|').unwrap_or((code, ""));
    t!(lang, key, status = status)
}

/// A source that was found and read.
#[derive(Debug, Clone)]
pub struct Found {
    pub kind: Kind,
    /// The canonical source, as stored.
    pub source: String,
    pub title: Option<String>,
    /// Newest first.
    pub items: Vec<Item>,
    pub interval_secs: i32,
}

impl Found {
    pub fn latest(&self) -> Option<&Item> {
        self.items.first()
    }

    /// How the source is named in messages and lists.
    pub fn account(&self) -> String {
        account_label(self.kind, &self.source, self.title.as_deref())
    }
}

/// What a fetch of a known source brought: `None` when it has not changed.
pub struct Read {
    pub parsed: parse::Parsed,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// Reads what a manager typed for a source of `kind`, finds it (a YouTube handle is looked up on its
/// page) and fetches it once, so they see what they are about to follow.
pub async fn look_up(kind: Kind, input: &str) -> Result<Found, Problem> {
    if !available(kind) {
        return Err(Problem::Unavailable(kind));
    }
    let source = match parse_source(kind, input).map_err(Problem::Source)? {
        Source::Ready(source) => source,
        Source::YouTubePage(path) => youtube_channel(&path).await?,
    };
    let read = fetch_source(kind, &source, None, None)
        .await?
        .ok_or(Problem::Fetch(FetchError::Network))?;
    Ok(Found {
        kind,
        interval_secs: poll_interval(kind, read.parsed.ttl),
        title: read.parsed.title,
        items: read.parsed.items,
        source,
    })
}

/// The channel id a YouTube page (`/@handle`, `/c/name`) belongs to.
async fn youtube_channel(path: &str) -> Result<String, Problem> {
    let url = format!("https://www.youtube.com{path}");
    let page = fetcher()
        .get(&Request {
            url: &url,
            page: true,
            ..Request::default()
        })
        .await;
    match page {
        Ok(Fetched::Body { bytes, .. }) => {
            youtube_channel_in_page(&String::from_utf8_lossy(&bytes)).ok_or(Problem::YouTubePage)
        }
        Ok(Fetched::NotModified) | Err(FetchError::NotFound) => Err(Problem::YouTubePage),
        Err(error) => Err(Problem::Fetch(error)),
    }
}

/// Fetches a stored source; with validators from the last fetch, `None` means it did not change.
pub async fn fetch_source(
    kind: Kind,
    source: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<Option<Read>, Problem> {
    if matches!(kind, Kind::Twitch | Kind::Kick) {
        if !available(kind) {
            return Err(Problem::Unavailable(kind));
        }
        let read = live::lookup(kind, source)
            .await
            .map_err(|_| Problem::Fetch(FetchError::Network))?;
        return Ok(Some(Read {
            parsed: parse::Parsed {
                title: Some(read.title),
                ttl: None,
                items: read.item.into_iter().collect(),
            },
            etag: None,
            last_modified: None,
        }));
    }
    let url = source_url(kind, source, instance())?;
    let bridge = instance().bridge.as_deref();
    let bridge_host = bridge
        .filter(|_| kind == Kind::Bridge)
        .and_then(|b| reqwest::Url::parse(&b.replace("{route}", "")).ok())
        .and_then(|u| u.host_str().map(str::to_owned));
    let request = Request {
        url: &url,
        etag,
        last_modified,
        json: kind == Kind::Bluesky,
        page: false,
        trusted_host: bridge_host.as_deref(),
    };
    match fetcher().get(&request).await {
        Ok(Fetched::NotModified) => Ok(None),
        Ok(Fetched::Body {
            bytes,
            etag,
            last_modified,
        }) => Ok(Some(Read {
            parsed: parse::parse(kind, &bytes).map_err(Problem::Parse)?,
            etag,
            last_modified,
        })),
        // Bluesky answers an unknown handle with 400 and a JSON error.
        Err(FetchError::Status(400) | FetchError::NotFound) if kind == Kind::Bluesky => {
            Err(Problem::Parse(ParseError::NoAccount))
        }
        Err(error) => Err(Problem::Fetch(error)),
    }
}

fn source_url(kind: Kind, source: &str, settings: &Instance) -> Result<String, Problem> {
    if !available_in(kind, settings) {
        return Err(Problem::Unavailable(kind));
    }
    feed_url(kind, source, settings.bridge.as_deref()).ok_or(Problem::Unavailable(kind))
}

/// What one step of the ticker did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FeedsTick {
    pub polled: usize,
    pub queued: usize,
    pub posted: usize,
}

/// One step: polls the sources that are due and sends the posts that are waiting.
pub async fn tick(app: &Arc<App>) -> anyhow::Result<FeedsTick> {
    tick_for(app, None).await
}

/// [`tick`] for the sources and posts of `guilds` only (`None`: every server), for tests that share a
/// database with others.
pub async fn tick_for(app: &Arc<App>, guilds: Option<&[u64]>) -> anyhow::Result<FeedsTick> {
    let due = app.db.claim_due_sources(app.now(), POLL_BATCH, guilds).await?;
    let polled = due.len();
    let (twitch, other): (Vec<_>, Vec<_>) = due.into_iter().partition(|row| row.kind == "twitch");
    let mut queued: usize = futures_util::stream::iter(other)
        .map(|source| poll(app, source))
        .buffer_unordered(POLL_AT_ONCE)
        .collect::<Vec<usize>>()
        .await
        .into_iter()
        .sum();
    if !twitch.is_empty() {
        let names: Vec<String> = twitch.iter().map(|row| row.source.clone()).collect();
        let reads = if available(Kind::Twitch) {
            live::lookup_twitch_batch(&names).await
        } else {
            Err(anyhow::anyhow!("Twitch credentials are missing"))
        };
        match reads {
            Ok(mut reads) => {
                for row in twitch {
                    if let Some(read) = reads.remove(&row.source) {
                        queued += poll_live_read(app, &row, Kind::Twitch, read).await;
                    } else {
                        live_failure(app, &row).await;
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "batched Twitch poll failed");
                for row in twitch {
                    if available(Kind::Twitch) {
                        live_failure(app, &row).await;
                    } else {
                        live_unavailable(app, &row).await;
                    }
                }
            }
        }
    }
    let posted = deliver(app, guilds).await?;
    Ok(FeedsTick { polled, queued, posted })
}

/// Fetches one leased source and records the outcome; returns the posts it queued.
async fn poll(app: &App, row: FeedSourceRow) -> usize {
    let now = app.now();
    let Some(kind) = Kind::parse(&row.kind) else {
        let later = now + Span::seconds(BACKOFF_MAX_SECS);
        let _ = app
            .db
            .record_feed_failure(&row.kind, &row.source, "feeds.error.parse", later)
            .await;
        return 0;
    };
    if matches!(kind, Kind::Twitch | Kind::Kick) {
        return poll_live(app, &row, kind).await;
    }
    // A little spread, so sources added together do not stay in step.
    let spread = |secs: i64| Span::seconds(secs + rand::random_range(0..=secs / 10));
    let result = fetch_source(kind, &row.source, row.etag.as_deref(), row.last_modified.as_deref()).await;
    let recorded = match result {
        Ok(None) => {
            let next = now + spread(i64::from(row.interval_secs));
            app.db
                .record_feed_unchanged(&row.kind, &row.source, next)
                .await
                .map(|()| 0)
        }
        Ok(Some(read)) => {
            let interval = poll_interval(kind, read.parsed.ttl);
            app.db
                .record_feed_poll(&Polled {
                    kind: &row.kind,
                    source: &row.source,
                    title: read.parsed.title.as_deref(),
                    etag: read.etag.as_deref(),
                    last_modified: read.last_modified.as_deref(),
                    interval_secs: interval,
                    next_poll_at: now + spread(i64::from(interval)),
                    items: &read.parsed.items,
                    now,
                })
                .await
        }
        Err(problem) => {
            let wait = match problem {
                Problem::Fetch(FetchError::RateLimited(Some(secs))) => {
                    (secs as i64).clamp(i64::from(row.interval_secs), BACKOFF_MAX_SECS)
                }
                _ => backoff_secs(row.interval_secs, row.fail_count + 1),
            };
            tracing::debug!(kind = %row.kind, source = %row.source, problem = %problem.code(), "feed fetch failed");
            app.db
                .record_feed_failure(&row.kind, &row.source, &problem.code(), now + Span::seconds(wait))
                .await
                .map(|()| 0)
        }
    };
    recorded.unwrap_or_else(|error| {
        tracing::warn!(%error, kind = %row.kind, source = %row.source, "feed poll not recorded");
        0
    })
}

async fn poll_live(app: &App, row: &FeedSourceRow, kind: Kind) -> usize {
    if !available(kind) {
        live_unavailable(app, row).await;
        return 0;
    }
    let read = match live::lookup(kind, &row.source).await {
        Ok(read) => read,
        Err(error) => {
            tracing::warn!(%error, source = %row.source, "live source lookup failed");
            live_failure(app, row).await;
            return 0;
        }
    };
    poll_live_read(app, row, kind, read).await
}

async fn live_unavailable(app: &App, row: &FeedSourceRow) {
    let later = app.now() + Span::seconds(i64::from(row.interval_secs));
    let code = format!("feeds.unavailable.{}", row.kind);
    if let Err(error) = app.db.record_feed_failure(&row.kind, &row.source, &code, later).await {
        tracing::warn!(%error, source = %row.source, "unavailable live source was not recorded");
    }
}

async fn live_failure(app: &App, row: &FeedSourceRow) {
    let later = app.now() + Span::seconds(backoff_secs(row.interval_secs, row.fail_count + 1));
    if let Err(error) = app
        .db
        .record_feed_failure(&row.kind, &row.source, "feeds.error.network", later)
        .await
    {
        tracing::warn!(%error, source = %row.source, "live source failure was not recorded");
    }
}

async fn poll_live_read(app: &App, row: &FeedSourceRow, kind: Kind, read: live::LiveRead) -> usize {
    let now = app.now();
    let stream_id = read.item.as_ref().map(|item| item.id.as_str());
    match app
        .db
        .set_live_stream(&row.kind, &row.source, &read.broadcaster_id, stream_id)
        .await
    {
        Ok(ended) => {
            for feed in ended {
                end_live(app, &feed).await;
            }
        }
        Err(error) => tracing::warn!(%error, "live stream state was not saved"),
    }
    if kind == Kind::Twitch && row.eventsub_registered_at.is_none() && live::webhook_callback().is_some() {
        match live::subscribe_twitch(&read.broadcaster_id).await {
            Ok(()) => {
                if let Err(error) = app.db.mark_eventsub_registered(&row.source).await {
                    tracing::warn!(%error, "EventSub registration was not recorded");
                }
            }
            Err(error) => {
                tracing::warn!(%error, source = %row.source, "EventSub registration failed; polling continues")
            }
        }
    }
    if read.item.is_some()
        && let Ok(feeds) = app.db.live_feeds_for_channel(&row.kind, &row.source).await
    {
        for feed in feeds {
            change_live_role(app, &feed, true).await;
        }
    }
    let items: Vec<Item> = read.item.into_iter().collect();
    match app
        .db
        .record_feed_poll(&Polled {
            kind: &row.kind,
            source: &row.source,
            title: Some(&read.title),
            etag: None,
            last_modified: None,
            interval_secs: kind.interval_secs(),
            next_poll_at: now + Span::seconds(i64::from(kind.interval_secs())),
            items: &items,
            now,
        })
        .await
    {
        Ok(queued) => queued,
        Err(error) => {
            tracing::warn!(%error, "live poll was not recorded");
            0
        }
    }
}

/// Sends the posts this pod can lease; returns how many went out.
async fn deliver(app: &App, guilds: Option<&[u64]>) -> anyhow::Result<usize> {
    let mut sent = 0;
    // Claim just before sending: a rate-limit wait for one post cannot age the others' leases.
    for _ in 0..POST_BATCH {
        let Some(post) = app.db.claim_feed_posts(1, guilds).await?.pop() else {
            break;
        };
        let send = deliver_one(app, &post);
        tokio::pin!(send);
        let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
        heartbeat.tick().await;
        let outcome = loop {
            tokio::select! {
                outcome = &mut send => break outcome,
                _ = heartbeat.tick() => {
                    if !app.db.renew_feed_post(&post).await? {
                        // Another pod reclaimed it. The in-flight Discord request may still
                        // finish, but its stable nonce deduplicates the competing send.
                        break PostOutcome::Dropped;
                    }
                }
            }
        };
        if outcome == PostOutcome::Sent {
            sent += 1;
        }
        app.db.finish_feed_post(&post, &outcome).await?;
    }
    Ok(sent)
}

async fn deliver_one(app: &App, post: &FeedPost) -> PostOutcome {
    if !post.feed.enabled
        || Kind::parse(&post.feed.kind).is_none_or(|kind| !available(kind))
        || !app.module_enabled(post.feed.guild_id as u64, Module::Feeds).await
    {
        return PostOutcome::Dropped;
    }
    send(app, &post.feed, &post.item, true).await
}

/// Posts `item` in the feed's channel; `ping` lets the feed's role be pinged (a test post pings nobody).
pub async fn send(app: &App, feed: &FeedRow, item: &Item, ping: bool) -> PostOutcome {
    let Some(draft) = Draft::of(feed) else {
        return PostOutcome::Dropped;
    };
    let lang = app.guild_lang(feed.guild_id as u64).await;
    let message = render(lang, &draft, item);
    let mentions = AllowedMentions {
        roles: draft.ping_role.filter(|_| ping).map(Id::new).into_iter().collect(),
        ..AllowedMentions::default()
    };
    let embeds: Vec<_> = message.embed.iter().map(|e| e.to_embed(draft.kind)).collect();
    // Twilight 0.18 has `nonce` but omits Discord's `enforce_nonce` field. Its raw JSON
    // escape hatch sends the complete payload, including that field.
    let mut payload = delivery_payload(
        &message,
        &embeds,
        &mentions,
        ping.then(|| delivery_nonce(feed.id, &item.id)),
    );
    if matches!(draft.kind, Kind::Twitch | Kind::Kick)
        && let Some(url) = pinchora_core::feeds::page_url(draft.kind, &feed.source)
    {
        let mut value: serde_json::Value = serde_json::from_slice(&payload).expect("feed payload");
        value["components"] = serde_json::json!([{ "type": 1, "components": [{
            "type": 2, "style": 5, "label": t!(lang, "feeds.live.watch"), "url": url
        }]}]);
        payload = serde_json::to_vec(&value).expect("live payload");
    }
    let request = app
        .http
        .create_message(Id::new((feed.channel_id as u64).max(1)))
        .payload_json(&payload);
    match request.await {
        Ok(response) => {
            if ping && matches!(draft.kind, Kind::Twitch | Kind::Kick) {
                match response.model().await {
                    Ok(message) => match app.db.set_live_message(feed.id, message.id.get(), &item.id).await {
                        Ok(true) => {
                            change_live_role(app, feed, true).await;
                        }
                        Ok(false) => {
                            let mut ended = feed.clone();
                            ended.live_message_id = Some(message.id.get() as i64);
                            end_live(app, &ended).await;
                        }
                        Err(error) => tracing::warn!(%error, feed = feed.id, "live message id was not saved"),
                    },
                    Err(error) => tracing::warn!(%error, feed = feed.id, "live response was not decoded"),
                }
            }
            PostOutcome::Sent
        }
        Err(error) => {
            tracing::debug!(%error, feed = feed.id, "feed post refused");
            let code = match error_code(&error) {
                Some(50001 | 50013) => "feeds.error.post-permission",
                Some(10003) => "feeds.error.post-channel",
                _ if is_permanent(&error) => "feeds.error.post-refused",
                _ => return PostOutcome::Retry("feeds.error.post-down".to_owned()),
            };
            PostOutcome::Refused(code.to_owned())
        }
    }
}

/// The role is only handed out to a verified owner and only while it passes the shared role rule.
pub async fn change_live_role(app: &App, feed: &FeedRow, add: bool) -> bool {
    let user = match app.db.linked_streamer(&feed.kind, &feed.source).await {
        Ok(Some(user)) => user,
        Ok(None) => return true,
        Err(_) => return false,
    };
    let Ok(_guard) = app.db.lock_feed_link(user).await else {
        return false;
    };
    if app.db.linked_streamer(&feed.kind, &feed.source).await.ok().flatten() != Some(user) {
        return true;
    }
    change_live_role_for(app, feed, user, add).await
}

/// Caller holds the member's link lock, so unlink cannot race a role grant.
pub async fn change_live_role_for(app: &App, feed: &FeedRow, user: u64, add: bool) -> bool {
    if add
        && (!app.module_enabled(feed.guild_id as u64, Module::Feeds).await
            || !app
                .db
                .feed_channel_is_live(&feed.kind, &feed.source)
                .await
                .unwrap_or(false)
            || app
                .db
                .feed(feed.guild_id as u64, feed.id)
                .await
                .ok()
                .flatten()
                .is_none_or(|current| !current.enabled || current.live_role_id != feed.live_role_id))
    {
        return false;
    }
    let Some(role) = feed.live_role_id.map(|id| id as u64) else {
        return true;
    };
    let guild = feed.guild_id as u64;
    let lang = app.guild_lang(guild).await;
    if !add {
        return match app
            .http
            .remove_guild_member_role(Id::new(guild), Id::new(user), Id::new(role))
            .reason(&t!(lang, "feeds.live.role-removed"))
            .await
        {
            Ok(_) => true,
            Err(error) => matches!(error_code(&error), Some(10007 | 10011 | 10013)),
        };
    }
    let Ok(roles) = app.http.roles(Id::new(guild)).await else {
        return false;
    };
    let Ok(roles) = roles.models().await else {
        return false;
    };
    let Some(target) = roles.iter().find(|r| r.id.get() == role) else {
        return false;
    };
    let Ok(me) = app
        .http
        .guild_member(Id::new(guild), Id::new(app.application_id.get()))
        .await
    else {
        return false;
    };
    let Ok(me) = me.model().await else {
        return false;
    };
    let top = roles
        .iter()
        .filter(|r| me.roles.contains(&r.id))
        .map(|r| r.position)
        .max()
        .unwrap_or(0);
    let facts = pinchora_core::settings::RoleFacts {
        everyone: role == guild,
        managed: target.managed,
        position: target.position,
        permissions: target.permissions.bits(),
    };
    if pinchora_core::settings::role_refusal(facts, Some(top), "Live now").is_some()
        || pinchora_core::settings::run_time_refusal(facts.permissions, "Live now").is_some()
    {
        return false;
    }
    let request = app
        .http
        .add_guild_member_role(Id::new(guild), Id::new(user), Id::new(role))
        .reason(&t!(lang, "feeds.live.role-added"))
        .await;
    if let Err(error) = request {
        tracing::warn!(%error, guild, user, role, "live role update failed");
        return false;
    }
    true
}

/// Remove a granted role before its configuration is erased; retain the member lock through the write.
pub async fn before_live_change(
    app: &App,
    feed: Option<&FeedRow>,
    edit: Option<&pinchora_db::FeedEdit>,
) -> anyhow::Result<impl Send> {
    let Some(feed) = feed.filter(|f| f.live_message_id.is_some()) else {
        return Ok(None);
    };
    let changes_role = edit.is_none_or(|edit| {
        edit.enabled == Some(false)
            || edit.channel.is_some_and(|id| id as i64 != feed.channel_id)
            || edit
                .live_role
                .is_some_and(|id| id.map(|id| id as i64) != feed.live_role_id)
    });
    if !changes_role {
        return Ok(None);
    }
    let Some(user) = app.db.linked_streamer(&feed.kind, &feed.source).await? else {
        return Ok(None);
    };
    let guard = app.db.lock_feed_link(user).await?;
    if app.db.linked_streamer(&feed.kind, &feed.source).await? != Some(user) {
        anyhow::bail!("channel owner changed during cleanup");
    }
    let current = app.db.feed(feed.guild_id as u64, feed.id).await?;
    if let Some(current) = current.as_ref()
        && !change_live_role_for(app, current, user, false).await
    {
        anyhow::bail!("live role cleanup failed");
    }
    Ok(Some(guard))
}

/// Keep role grants stopped until privacy deletion has erased the member's links.
pub(crate) async fn before_delete(app: &App, user: u64) -> anyhow::Result<impl Send> {
    let guard = app.db.lock_feed_link(user).await?;
    for link in app.db.channel_links(user).await? {
        if link.verified_at.is_none() {
            continue;
        }
        for feed in app.db.live_feeds_for_channel(&link.kind, &link.source).await? {
            if !change_live_role_for(app, &feed, user, false).await {
                anyhow::bail!("live role cleanup failed");
            }
        }
    }
    Ok(guard)
}

pub async fn reconcile_live_edit(app: &App, before: &FeedRow, after: &FeedRow) {
    if before.live_message_id.is_none() {
        return;
    }
    if !after.enabled || before.channel_id != after.channel_id {
        end_live(app, before).await;
    } else if before.live_role_id != after.live_role_id {
        change_live_role(app, before, false).await;
        change_live_role(app, after, true).await;
    }
}

pub async fn end_live(app: &App, feed: &FeedRow) {
    let Some(id) = feed.live_message_id.map(|id| id as u64) else {
        return;
    };
    let lang = app.guild_lang(feed.guild_id as u64).await;
    let kind = Kind::parse(&feed.kind).unwrap_or(Kind::Twitch);
    let vod = format!(
        "{}/videos",
        pinchora_core::feeds::page_url(kind, &feed.source).unwrap_or_default()
    );
    let embed = twilight_util::builder::embed::EmbedBuilder::new()
        .title(t!(lang, "feeds.live.ended"))
        .url(vod)
        .color(0x6b7280)
        .build();
    let body = serde_json::to_vec(&serde_json::json!({
        "content": "", "embeds": [embed], "components": [], "allowed_mentions": {"parse": []}
    }))
    .expect("ended payload");
    let edited = app
        .http
        .update_message(Id::new(feed.channel_id as u64), Id::new(id))
        .payload_json(&body)
        .await;
    let removed = change_live_role(app, feed, false).await;
    if !removed {
        return;
    }
    if let Err(error) = edited {
        tracing::warn!(%error, feed = feed.id, "live message edit failed");
        return;
    }
    if let Err(error) = app.db.clear_live_message(feed.id, id).await {
        tracing::warn!(%error, feed = feed.id, "live message id was not cleared");
    }
}

fn delivery_nonce(feed: i64, item: &str) -> u64 {
    let mut hash = Sha256::new();
    hash.update(feed.to_be_bytes());
    hash.update(item.as_bytes());
    u64::from_be_bytes(hash.finalize()[..8].try_into().expect("SHA-256 has eight bytes"))
}

fn delivery_payload(
    message: &FeedMessage,
    embeds: &[twilight_model::channel::message::Embed],
    mentions: &AllowedMentions,
    nonce: Option<u64>,
) -> Vec<u8> {
    let mut payload = serde_json::json!({
        "content": message.content,
        "embeds": embeds,
        "allowed_mentions": mentions,
    });
    if let Some(nonce) = nonce {
        payload["nonce"] = serde_json::json!(nonce.to_string());
        payload["enforce_nonce"] = serde_json::json!(true);
    }
    serde_json::to_vec(&payload).expect("message payload is serializable")
}

/// Runs [`tick`] for the life of the process, and forgets old items once an hour.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            wake.tick().await;
            if let Err(error) = Box::pin(tick(&app)).await {
                tracing::warn!(%error, "feeds tick failed");
            }
            rounds = rounds.wrapping_add(1);
            if rounds.is_multiple_of(CLEANUP_EVERY) {
                match app.db.forget_old_feed_items(app.now() - SEEN_KEPT).await {
                    Ok(0) => {}
                    Ok(rows) => tracing::debug!(rows, "old feed items forgotten"),
                    Err(error) => tracing::warn!(%error, "old feed items not forgotten"),
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reddit_off_rejects_stored_sources_before_fetch() {
        let off = Instance {
            bridge: None,
            reddit: false,
            twitch: false,
            kick: false,
        };
        assert_eq!(
            source_url(Kind::Reddit, "existing-subscription", &off),
            Err(Problem::Unavailable(Kind::Reddit))
        );
        assert!(source_url(Kind::Rss, "https://example.com/rss", &off).is_ok());
    }

    #[test]
    fn delivery_nonce_is_stable_and_enforced_only_for_announcements() {
        let message = FeedMessage {
            content: "Fresh".into(),
            embed: None,
        };
        let mentions = AllowedMentions::default();
        let nonce = delivery_nonce(42, "entry-1");
        assert_eq!(nonce, delivery_nonce(42, "entry-1"));
        assert_ne!(nonce, delivery_nonce(42, "entry-2"));
        assert_ne!(nonce, delivery_nonce(43, "entry-1"));
        let payload: serde_json::Value =
            serde_json::from_slice(&delivery_payload(&message, &[], &mentions, Some(nonce))).unwrap();
        assert_eq!(payload["nonce"], nonce.to_string());
        assert_eq!(payload["enforce_nonce"], true);
        let test_post: serde_json::Value =
            serde_json::from_slice(&delivery_payload(&message, &[], &mentions, None)).unwrap();
        assert!(test_post.get("nonce").is_none());
    }

    #[test]
    fn problems_are_worded_in_both_languages() {
        for problem in [
            Problem::Source(SourceError::YouTube),
            Problem::Unavailable(Kind::Bridge),
            Problem::Unavailable(Kind::Reddit),
            Problem::Fetch(FetchError::Status(503)),
            Problem::Fetch(FetchError::Blocked),
            Problem::Parse(ParseError::NoAccount),
            Problem::YouTubePage,
        ] {
            for lang in Lang::ALL {
                let text = problem.text(lang);
                assert!(!text.starts_with("feeds."), "{problem:?} in {lang:?} has no text");
            }
        }
        assert!(error_text(Lang::En, "feeds.error.status|503").contains("503"));
        assert!(error_text(Lang::Tr, "feeds.error.status|503").contains("503"));
    }
}
