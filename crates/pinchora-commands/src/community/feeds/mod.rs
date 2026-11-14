//! Social feeds: the poller that fetches every followed source once, however many servers follow it,
//! and posts what is new in each server's channel.
//!
//! Every pod runs the ticker. Each step leases the sources that are due (`FOR UPDATE SKIP LOCKED`),
//! fetches them a few at a time through the guarded [`Fetcher`], records what is new and queues one
//! post per subscription in the same transaction, then sends the queued posts it can lease. A source
//! that fails waits longer each time ([`backoff_secs`]) and says why on the dashboard.

mod polling;
pub use polling::*;
mod live_roles;
pub use live_roles::*;
#[cfg(test)]
mod tests;

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
use pinchora_core::admin::modules::Module;
use pinchora_core::community::feeds::BACKOFF_MAX_SECS;
use pinchora_core::community::feeds::Item;
use pinchora_core::community::feeds::Kind;
use pinchora_core::community::feeds::Parsed as Source;
use pinchora_core::community::feeds::SourceError;
use pinchora_core::community::feeds::account_label;
use pinchora_core::community::feeds::backoff_secs;
use pinchora_core::community::feeds::feed_url;
use pinchora_core::community::feeds::parse_source;
use pinchora_core::community::feeds::poll_interval;
use pinchora_core::community::feeds::youtube_channel_in_page;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{FeedPost, FeedRow, FeedSourceRow, Polled, PostOutcome};
use sha2::{Digest, Sha256};
use twilight_http::request::AuditLogReason;
use twilight_model::{channel::message::AllowedMentions, id::Id};

pub use crate::community::feeds::fetch::FetchError;
pub use crate::community::feeds::fetch::Fetched;
pub use crate::community::feeds::fetch::Fetcher;
pub use crate::community::feeds::fetch::Request;
pub use crate::community::feeds::fetch::fetcher;
pub use crate::community::feeds::fetch::install_fetcher;
pub use crate::community::feeds::message::Draft;
pub use crate::community::feeds::message::FeedEmbed;
pub use crate::community::feeds::message::FeedMessage;
pub use crate::community::feeds::message::default_message;
pub use crate::community::feeds::message::kind_label;
pub use crate::community::feeds::message::render;
pub use crate::community::feeds::message::sample_item;
pub use crate::community::feeds::parse::ParseError;

use crate::App;
use crate::community::server_events::is_permanent;
use crate::roles::roles::error_code;

/// The ticker wakes this often.
const WAKE: Duration = Duration::from_secs(30);
/// Sources one step leases, and how many it fetches at once.
const POLL_BATCH: i64 = 25;
const POLL_AT_ONCE: usize = 4;
/// Full batches one wake drains back to back before waiting for the next wake.
const MAX_BATCHES_PER_WAKE: usize = 8;
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
    twitch: crate::community::feeds::live::configured(Kind::Twitch),
    kick: crate::community::feeds::live::configured(Kind::Kick),
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
    pub parsed: crate::community::feeds::parse::Parsed,
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
        let read = crate::community::feeds::live::lookup(kind, source)
            .await
            .map_err(|_| Problem::Fetch(FetchError::Network))?;
        return Ok(Some(Read {
            parsed: crate::community::feeds::parse::Parsed {
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
            parsed: crate::community::feeds::parse::parse(kind, &bytes).map_err(Problem::Parse)?,
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

/// Runs [`tick`] for the life of the process, and forgets old items once an hour.
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut wake = tokio::time::interval(WAKE);
        wake.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut rounds = 0u32;
        loop {
            wake.tick().await;
            // A full batch may leave overdue sources behind. Drain those batches with the
            // same bounded fetch concurrency instead of adding 30 seconds per 25 sources,
            // up to a few batches per wake so a source that stays due cannot spin the loop.
            for drained in 1..=MAX_BATCHES_PER_WAKE {
                match Box::pin(tick(&app)).await {
                    Ok(result) if result.polled == POLL_BATCH as usize && drained < MAX_BATCHES_PER_WAKE => {
                        tokio::task::yield_now().await;
                    }
                    Ok(_) => break,
                    Err(error) => {
                        tracing::warn!(%error, "feeds tick failed");
                        break;
                    }
                }
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
