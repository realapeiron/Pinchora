//! Social feeds: the poller that fetches every followed source once, however many servers follow it,
//! and posts what is new in each server's channel.
//!
//! Every pod runs the ticker. Each step leases the sources that are due (`FOR UPDATE SKIP LOCKED`),
//! fetches them a few at a time through the guarded [`Fetcher`], records what is new and queues one
//! post per subscription in the same transaction, then sends the queued posts it can lease. A source
//! that fails waits longer each time ([`backoff_secs`]) and says why on the dashboard.

pub(crate) mod fetch;
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
}

static INSTANCE: LazyLock<Instance> = LazyLock::new(|| Instance {
    bridge: std::env::var("PINCHORA_FEEDS__BRIDGE_URL")
        .ok()
        .map(|b| b.trim().to_owned())
        .filter(|b| b.starts_with("https://") || b.starts_with("http://")),
    reddit: !std::env::var("PINCHORA_FEEDS__REDDIT")
        .is_ok_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "false" | "off" | "0" | "no")),
});

pub fn instance() -> &'static Instance {
    &INSTANCE
}

/// Whether this instance can follow sources of `kind`.
pub fn available(kind: Kind) -> bool {
    match kind {
        Kind::Bridge => instance().bridge.is_some(),
        Kind::Reddit => instance().reddit,
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
    let bridge = instance().bridge.as_deref();
    let url = feed_url(kind, source, bridge).ok_or(Problem::Unavailable(kind))?;
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
    let queued: usize = futures_util::stream::iter(due)
        .map(|source| poll(app, source))
        .buffer_unordered(POLL_AT_ONCE)
        .collect::<Vec<usize>>()
        .await
        .into_iter()
        .sum();
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

/// Sends the posts this pod can lease; returns how many went out.
async fn deliver(app: &App, guilds: Option<&[u64]>) -> anyhow::Result<usize> {
    let mut sent = 0;
    for post in app.db.claim_feed_posts(POST_BATCH, guilds).await? {
        let outcome = deliver_one(app, &post).await;
        if outcome == PostOutcome::Sent {
            sent += 1;
        }
        app.db.finish_feed_post(&post, &outcome).await?;
    }
    Ok(sent)
}

async fn deliver_one(app: &App, post: &FeedPost) -> PostOutcome {
    if !post.feed.enabled || !app.module_enabled(post.feed.guild_id as u64, Module::Feeds).await {
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
    let mut request = app
        .http
        .create_message(Id::new((feed.channel_id as u64).max(1)))
        .allowed_mentions(Some(&mentions))
        .embeds(&embeds);
    if !message.content.is_empty() {
        request = request.content(&message.content);
    }
    match request.await {
        Ok(_) => PostOutcome::Sent,
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
