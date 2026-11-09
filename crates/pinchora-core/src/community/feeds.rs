//! Social feeds: which sources a server follows and how a new post is announced. Pure: the bot,
//! `/feeds` and the dashboard read sources, messages, filters and poll timing with these rules.

use std::net::IpAddr;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use url::{Host, Url};

/// Feeds one server may follow.
pub const MAX_FEEDS: i64 = 10;
/// The longest message a feed may announce with, before placeholders are filled.
pub const MESSAGE_MAX: usize = 500;
/// Words in one filter list, and the longest word.
pub const FILTER_WORDS_MAX: usize = 10;
pub const FILTER_WORD_MAX: usize = 40;
/// The longest source a manager may type (a feed address, a bridge route).
pub const SOURCE_MAX: usize = 500;
/// What a message may name; anything else in braces stays as written.
pub const PLACEHOLDERS: [&str; 5] = ["{title}", "{url}", "{author}", "{account}", "{published}"];
/// The longest a source waits between polls while it keeps failing.
pub const BACKOFF_MAX_SECS: i64 = 6 * 3600;
/// The most new items one poll announces: a feed that suddenly shows many "new" items (its ids
/// changed, it came back after a long outage) posts its newest few, not a flood.
pub const ANNOUNCE_MAX: usize = 5;
/// Items published longer ago than this are recorded as seen but not announced.
pub const ANNOUNCE_MAX_AGE: Duration = Duration::days(3);
/// An item published before a feed was added is not announced to it; this much clock difference is forgiven.
pub const SUBSCRIBE_SLACK: Duration = Duration::minutes(10);

/// One post of a source, as the poller read it and the announcement shows it. Texts are as the source
/// wrote them (HTML taken out); they are made safe to post when the message is put together.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// The source's own id for it (a GUID, a video id, a post URI).
    pub id: String,
    pub title: String,
    pub url: Option<String>,
    pub author: Option<String>,
    pub summary: Option<String>,
    pub published: Option<DateTime<Utc>>,
    /// A picture for it: a video's thumbnail, a post's first image, an episode's cover.
    pub image: Option<String>,
    /// A podcast episode's audio file.
    pub audio: Option<String>,
    pub duration_secs: Option<u64>,
}

/// Which of the items a poll found for the first time get announced: the newest [`ANNOUNCE_MAX`] that
/// are not older than [`ANNOUNCE_MAX_AGE`], oldest first so a channel reads them in order.
pub fn to_announce(mut new: Vec<Item>, now: DateTime<Utc>) -> Vec<Item> {
    new.retain(|item| item.published.is_none_or(|at| now - at <= ANNOUNCE_MAX_AGE));
    // Items without a date keep the source's order (newest first) behind the dated ones.
    new.sort_by_key(|item| std::cmp::Reverse(item.published));
    new.truncate(ANNOUNCE_MAX);
    new.reverse();
    new
}

/// Whether a feed added at `since` with these filters announces `item`.
pub fn feed_wants(item: &Item, since: DateTime<Utc>, include: &[String], exclude: &[String]) -> bool {
    item.published.is_none_or(|at| at >= since - SUBSCRIBE_SLACK) && passes_filters(&item.title, include, exclude)
}

/// Where a feed comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    YouTube,
    /// RSS or Atom, podcasts included.
    Rss,
    /// A subreddit's newest posts, read from its public RSS.
    Reddit,
    Bluesky,
    /// A route on the operator's RSSHub-style bridge (X, Instagram, TikTok), read as RSS.
    Bridge,
    Twitch,
    Kick,
}

impl Kind {
    pub const ALL: [Self; 7] = [
        Self::YouTube,
        Self::Rss,
        Self::Reddit,
        Self::Bluesky,
        Self::Bridge,
        Self::Twitch,
        Self::Kick,
    ];

    /// The key stored in the database and used in the API.
    pub const fn key(self) -> &'static str {
        match self {
            Self::YouTube => "youtube",
            Self::Rss => "rss",
            Self::Reddit => "reddit",
            Self::Bluesky => "bluesky",
            Self::Bridge => "bridge",
            Self::Twitch => "twitch",
            Self::Kick => "kick",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// How often a healthy source is fetched, before a feed's own `ttl` is taken into account.
    pub const fn interval_secs(self) -> i32 {
        match self {
            Self::YouTube | Self::Reddit => 300,
            Self::Rss => 600,
            Self::Bluesky => 120,
            Self::Bridge => 900,
            Self::Twitch | Self::Kick => 120,
        }
    }

    /// Whether the feed's own `ttl` may set the interval (within 5 to 15 minutes).
    const fn reads_ttl(self) -> bool {
        matches!(self, Self::Rss | Self::Bridge)
    }
}

/// A source as a manager typed it, checked and written one way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// The canonical source: a YouTube channel id, a feed address, a subreddit, a Bluesky handle or DID,
    /// a bridge route.
    Ready(String),
    /// A YouTube page (`/@handle`, `/c/name`, `/user/name`) whose channel id has to be read from the
    /// page itself.
    YouTubePage(String),
}

/// Why a source cannot be followed. [`SourceError::key`] is the catalog key of the sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceError {
    Empty,
    TooLong,
    YouTube,
    Url,
    /// The address names a private, local or reserved network.
    Private,
    Reddit,
    Bluesky,
    Bridge,
    Twitch,
    Kick,
}

impl SourceError {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Empty => "feeds.source.empty",
            Self::TooLong => "feeds.source.too-long",
            Self::YouTube => "feeds.source.youtube",
            Self::Url => "feeds.source.url",
            Self::Private => "feeds.source.private",
            Self::Reddit => "feeds.source.reddit",
            Self::Bluesky => "feeds.source.bluesky",
            Self::Bridge => "feeds.source.bridge",
            Self::Twitch => "feeds.source.twitch",
            Self::Kick => "feeds.source.kick",
        }
    }
}

/// Reads what a manager typed for a source of `kind`: a link, an id, a handle or a name.
pub fn parse_source(kind: Kind, input: &str) -> Result<Parsed, SourceError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(SourceError::Empty);
    }
    if input.chars().count() > SOURCE_MAX {
        return Err(SourceError::TooLong);
    }
    match kind {
        Kind::YouTube => youtube_source(input),
        Kind::Rss => feed_address(input).map(Parsed::Ready),
        Kind::Reddit => subreddit(input).map(Parsed::Ready).ok_or(SourceError::Reddit),
        Kind::Bluesky => bluesky_actor(input).map(Parsed::Ready).ok_or(SourceError::Bluesky),
        Kind::Bridge => bridge_route(input).map(Parsed::Ready).ok_or(SourceError::Bridge),
        Kind::Twitch => stream_login(input, "twitch.tv", 25)
            .map(Parsed::Ready)
            .ok_or(SourceError::Twitch),
        Kind::Kick => stream_login(input, "kick.com", 25)
            .map(Parsed::Ready)
            .ok_or(SourceError::Kick),
    }
}

/// A public streamer login or its profile URL. Query strings and extra path segments are refused.
fn stream_login(input: &str, host: &str, max: usize) -> Option<String> {
    let login = if input.contains("/") || input.contains("://") {
        let url = loose_url(input)?;
        if !matches!(url.host_str()?, h if h == host || h == format!("www.{host}"))
            || !matches!(url.scheme(), "http" | "https")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return None;
        }
        let mut parts = url.path_segments()?.filter(|part| !part.is_empty());
        let name = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        name.to_owned()
    } else {
        input.trim_start_matches('@').to_owned()
    };
    ((2..=max).contains(&login.len())
        && login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || (host == "kick.com" && b == b'-')))
    .then(|| login.to_ascii_lowercase())
}

/// Whether `id` looks like a YouTube channel id: `UC` and 22 more URL-safe characters.
pub fn is_youtube_channel_id(id: &str) -> bool {
    id.len() == 24 && id.starts_with("UC") && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn youtube_source(input: &str) -> Result<Parsed, SourceError> {
    if is_youtube_channel_id(input) {
        return Ok(Parsed::Ready(input.to_owned()));
    }
    if let Some(handle) = input.strip_prefix('@') {
        return youtube_handle(handle).ok_or(SourceError::YouTube);
    }
    let Some(url) = loose_url(input) else {
        return Err(SourceError::YouTube);
    };
    let host = url.host_str().unwrap_or_default();
    let youtube = ["youtube.com", "www.youtube.com", "m.youtube.com", "music.youtube.com"];
    if !youtube.contains(&host) {
        return Err(SourceError::YouTube);
    }
    if url.path() == "/feeds/videos.xml"
        && let Some((_, id)) = url.query_pairs().find(|(k, _)| k == "channel_id")
        && is_youtube_channel_id(&id)
    {
        return Ok(Parsed::Ready(id.into_owned()));
    }
    let mut segments = url.path_segments().into_iter().flatten().filter(|s| !s.is_empty());
    match (segments.next(), segments.next()) {
        (Some("channel"), Some(id)) if is_youtube_channel_id(id) => Ok(Parsed::Ready(id.to_owned())),
        (Some(first), _) if first.starts_with('@') => youtube_handle(&first[1..]).ok_or(SourceError::YouTube),
        (Some(kind @ ("c" | "user")), Some(name)) if is_page_name(name) => {
            Ok(Parsed::YouTubePage(format!("/{kind}/{name}")))
        }
        _ => Err(SourceError::YouTube),
    }
}

/// A YouTube handle (without the `@`): 3 to 30 letters, digits, `.`, `_` or `-`.
fn youtube_handle(handle: &str) -> Option<Parsed> {
    let ok = (3..=30).contains(&handle.chars().count())
        && handle
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'));
    ok.then(|| Parsed::YouTubePage(format!("/@{handle}")))
}

fn is_page_name(name: &str) -> bool {
    (1..=100).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// A link written with or without `https://`.
fn loose_url(input: &str) -> Option<Url> {
    let with_scheme = if input.contains("://") {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    Url::parse(&with_scheme).ok()
}

/// A feed's address, `https://` added when none was typed: http or https, a host, no password, no
/// fragment, and not an address inside a private network.
pub fn feed_address(input: &str) -> Result<String, SourceError> {
    let mut url = loose_url(input.trim()).ok_or(SourceError::Url)?;
    if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() || url.password().is_some() {
        return Err(SourceError::Url);
    }
    match url.host() {
        None => return Err(SourceError::Url),
        Some(Host::Domain(domain)) => {
            // A bare word (`localhost`, `intranet`) is a name only a local network knows.
            if !domain.contains('.') || domain.ends_with(".local") || domain.ends_with(".internal") {
                return Err(SourceError::Private);
            }
        }
        Some(Host::Ipv4(ip)) if !is_public(IpAddr::V4(ip)) => return Err(SourceError::Private),
        Some(Host::Ipv6(ip)) if !is_public(IpAddr::V6(ip)) => return Err(SourceError::Private),
        Some(_) => {}
    }
    url.set_fragment(None);
    let text = url.to_string();
    if text.chars().count() > SOURCE_MAX {
        return Err(SourceError::TooLong);
    }
    Ok(text)
}

/// A subreddit's name in lower case, from `r/name`, `/r/name`, `name` or a reddit.com link.
fn subreddit(input: &str) -> Option<String> {
    let name = if input.contains("reddit.com") {
        let url = loose_url(input)?;
        let host = url.host_str()?;
        if host != "reddit.com" && !host.ends_with(".reddit.com") {
            return None;
        }
        let mut segments = url.path_segments()?.filter(|s| !s.is_empty());
        (segments.next()? == "r").then(|| segments.next())??.to_owned()
    } else {
        let bare = input.trim_start_matches('/');
        bare.strip_prefix("r/").unwrap_or(bare).trim_end_matches('/').to_owned()
    };
    let ok = (2..=21).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && name.as_bytes()[0] != b'_';
    ok.then(|| name.to_ascii_lowercase())
}

/// A Bluesky handle in lower case (`alice.bsky.social`) or a DID, from either, `@handle` or a profile link.
fn bluesky_actor(input: &str) -> Option<String> {
    let actor = if input.contains("bsky.app/") {
        let url = loose_url(input)?;
        if url.host_str()? != "bsky.app" {
            return None;
        }
        let mut segments = url.path_segments()?.filter(|s| !s.is_empty());
        (segments.next()? == "profile").then(|| segments.next())??.to_owned()
    } else {
        input.trim_start_matches('@').to_owned()
    };
    if let Some(rest) = actor.strip_prefix("did:") {
        let (method, id) = rest.split_once(':')?;
        let ok = !method.is_empty()
            && method.bytes().all(|b| b.is_ascii_lowercase())
            && (1..=100).contains(&id.len())
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'%' | b'-'));
        return ok.then_some(actor);
    }
    let handle = actor.to_ascii_lowercase();
    let labels: Vec<&str> = handle.split('.').collect();
    let ok = handle.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|l| {
            (1..=63).contains(&l.len())
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !l.starts_with('-')
                && !l.ends_with('-')
        })
        && !labels.last()?.bytes().all(|b| b.is_ascii_digit());
    ok.then_some(handle)
}

/// A route on the bridge, without its leading `/`: path characters only, no `..`.
fn bridge_route(input: &str) -> Option<String> {
    let route = input.trim_start_matches('/');
    let ok = !route.is_empty()
        && route.len() <= 200
        && route
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'@' | b':' | b'/' | b'-'))
        && !route.split('/').any(|s| s == "..");
    ok.then(|| route.to_owned())
}

/// Whether an address is on the public internet: not loopback, private, link-local, carrier-grade NAT,
/// multicast, documentation, benchmarking or otherwise reserved, and no IPv6 form that wraps one of them.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_multicast()
                || ip.is_documentation()
                || a == 0
                || (a == 100 && (64..=127).contains(&b))
                || (a == 192 && b == 0 && c == 0)
                || (a == 198 && (18..=19).contains(&b))
                || a >= 240)
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            let s = ip.segments();
            // 64:ff9b::/96 (NAT64) and 2002::/16 (6to4) carry an IPv4 address inside.
            if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
                let [a, b] = s[6].to_be_bytes();
                let [c, d] = s[7].to_be_bytes();
                return is_public(IpAddr::V4([a, b, c, d].into()));
            }
            // IPv4-translated form (::ffff:0:0/96) also carries an IPv4 target.
            if s[..6] == [0, 0, 0, 0, 0xffff, 0] {
                let [a, b] = s[6].to_be_bytes();
                let [c, d] = s[7].to_be_bytes();
                return is_public(IpAddr::V4([a, b, c, d].into()));
            }
            // RFC 8215 local-use translation is 64:ff9b:1::/48. The IPv4 bits can
            // occupy several positions, so none of this prefix is safe to fetch.
            if s[0..3] == [0x64, 0xff9b, 1] {
                return false;
            }
            if s[0] == 0x2002 {
                let [a, b] = s[1].to_be_bytes();
                let [c, d] = s[2].to_be_bytes();
                return is_public(IpAddr::V4([a, b, c, d].into()));
            }
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                // IPv4-compatible (::a.b.c.d), unique local (fc00::/7), link-local (fe80::/10) and
                // the old site-local (fec0::/10).
                || (s[0..6] == [0, 0, 0, 0, 0, 0])
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || (s[0] & 0xffc0) == 0xfec0
                // Documentation (2001:db8::/32) and Teredo (2001::/32), which hides an IPv4 address.
                || (s[0] == 0x2001 && (s[1] == 0x0db8 || s[1] == 0)))
        }
    }
}

/// Where the poller fetches a source. `None` for a bridge route when the instance has no bridge.
pub fn feed_url(kind: Kind, source: &str, bridge: Option<&str>) -> Option<String> {
    Some(match kind {
        Kind::YouTube => format!("https://www.youtube.com/feeds/videos.xml?channel_id={source}"),
        Kind::Rss => source.to_owned(),
        Kind::Reddit => format!("https://www.reddit.com/r/{source}/new/.rss"),
        Kind::Bluesky => format!(
            "https://public.api.bsky.app/xrpc/app.bsky.feed.getAuthorFeed?actor={source}&limit=25&filter=posts_no_replies"
        ),
        Kind::Bridge => {
            let bridge = bridge?;
            if bridge.contains("{route}") {
                bridge.replace("{route}", source)
            } else {
                format!("{}/{source}", bridge.trim_end_matches('/'))
            }
        }
        Kind::Twitch | Kind::Kick => return None,
    })
}

/// The page people open to see the source, when it has one.
pub fn page_url(kind: Kind, source: &str) -> Option<String> {
    match kind {
        Kind::YouTube => Some(format!("https://www.youtube.com/channel/{source}")),
        Kind::Rss => Some(source.to_owned()),
        Kind::Reddit => Some(format!("https://www.reddit.com/r/{source}/")),
        Kind::Bluesky => Some(format!("https://bsky.app/profile/{source}")),
        Kind::Bridge => None,
        Kind::Twitch => Some(format!("https://www.twitch.tv/{source}")),
        Kind::Kick => Some(format!("https://kick.com/{source}")),
    }
}

/// How a source is named when nothing better is known (or always, for a subreddit or a handle).
pub fn account_label(kind: Kind, source: &str, title: Option<&str>) -> String {
    let title = title.map(str::trim).filter(|t| !t.is_empty());
    match kind {
        Kind::Reddit => format!("r/{source}"),
        Kind::Bluesky => title.map_or_else(|| format!("@{source}"), str::to_owned),
        _ => title.map_or_else(|| source.to_owned(), str::to_owned),
    }
}

/// The channel id a YouTube page names for itself: its canonical link, then the ids in its data.
pub fn youtube_channel_in_page(html: &str) -> Option<String> {
    let after = |marker: &str| -> Option<String> {
        html.match_indices(marker).find_map(|(at, _)| {
            let id = html.get(at + marker.len()..at + marker.len() + 24)?;
            is_youtube_channel_id(id).then(|| id.to_owned())
        })
    };
    after("rel=\"canonical\" href=\"https://www.youtube.com/channel/")
        .or_else(|| after("\"externalId\":\""))
        .or_else(|| after("<meta itemprop=\"identifier\" content=\""))
        .or_else(|| after("\"channelId\":\""))
        .or_else(|| after("youtube.com/channel/"))
}

/// When a source is fetched again after `fails` failures in a row: its interval, doubled for each
/// failure, at most [`BACKOFF_MAX_SECS`].
pub fn backoff_secs(interval: i32, fails: i32) -> i64 {
    let doublings = fails.clamp(0, 16) as u32;
    (i64::from(interval.max(60)) << doublings).min(BACKOFF_MAX_SECS)
}

/// How often a healthy source is fetched: the kind's interval, or for RSS the feed's own `ttl` in
/// minutes, kept between 5 and 15 minutes.
pub fn poll_interval(kind: Kind, ttl_minutes: Option<u32>) -> i32 {
    match ttl_minutes {
        Some(ttl) if kind.reads_ttl() => (ttl.saturating_mul(60)).clamp(300, 900) as i32,
        _ => kind.interval_secs(),
    }
}

/// Why a message or a filter cannot be saved. [`TextError::key`] is the catalog key of the sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextError {
    MessageTooLong,
    TooManyWords,
    WordTooLong,
}

impl TextError {
    pub const fn key(self) -> &'static str {
        match self {
            Self::MessageTooLong => "feeds.message-too-long",
            Self::TooManyWords => "feeds.filter-too-many",
            Self::WordTooLong => "feeds.filter-too-long",
        }
    }
}

/// A feed's message: trimmed, `None` (the kind's default) when empty, at most [`MESSAGE_MAX`] characters.
pub fn check_message(text: Option<&str>) -> Result<Option<String>, TextError> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > MESSAGE_MAX {
        return Err(TextError::MessageTooLong);
    }
    Ok(Some(text.to_owned()))
}

/// Filter words from a comma-separated list (or several entries): trimmed, lower case, no duplicates.
pub fn filter_words<'a>(entries: impl IntoIterator<Item = &'a str>) -> Result<Vec<String>, TextError> {
    let mut words: Vec<String> = Vec::new();
    for word in entries.into_iter().flat_map(|e| e.split(',')) {
        let word = word.trim().to_lowercase();
        if word.is_empty() || words.contains(&word) {
            continue;
        }
        if word.chars().count() > FILTER_WORD_MAX {
            return Err(TextError::WordTooLong);
        }
        words.push(word);
    }
    if words.len() > FILTER_WORDS_MAX {
        return Err(TextError::TooManyWords);
    }
    Ok(words)
}

/// Whether an item's title passes a feed's filters: one of `include` in it (when there are any), none
/// of `exclude`. Capital letters do not matter.
pub fn passes_filters(title: &str, include: &[String], exclude: &[String]) -> bool {
    let title = title.to_lowercase();
    (include.is_empty() || include.iter().any(|w| title.contains(w.as_str())))
        && !exclude.iter().any(|w| title.contains(w.as_str()))
}

/// What fills a message's placeholders, already made safe to post.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    pub title: String,
    pub url: String,
    pub author: String,
    pub account: String,
    pub published: String,
}

/// `template` with each of [`PLACEHOLDERS`] replaced; anything else stays as written.
pub fn render(template: &str, fields: &Fields) -> String {
    let mut out = String::with_capacity(template.len() + 64);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let here = &rest[start..];
        let value = [
            ("{title}", &fields.title),
            ("{url}", &fields.url),
            ("{author}", &fields.author),
            ("{account}", &fields.account),
            ("{published}", &fields.published),
        ]
        .into_iter()
        .find(|(token, _)| here.starts_with(token));
        match value {
            Some((token, value)) => {
                out.push_str(value);
                rest = &here[token.len()..];
            }
            None => {
                out.push('{');
                rest = &here[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use crate::community::feeds::*;

    fn ready(kind: Kind, input: &str) -> String {
        match parse_source(kind, input) {
            Ok(Parsed::Ready(source)) => source,
            other => panic!("{input}: {other:?}"),
        }
    }

    #[test]
    fn youtube_sources_by_id_link_and_handle() {
        let id = "UCBR8-60-B28hp2BmDPdntcQ";
        assert_eq!(ready(Kind::YouTube, id), id);
        for link in [
            "https://www.youtube.com/channel/UCBR8-60-B28hp2BmDPdntcQ",
            "youtube.com/channel/UCBR8-60-B28hp2BmDPdntcQ/videos",
            "https://m.youtube.com/channel/UCBR8-60-B28hp2BmDPdntcQ?si=x",
            "https://www.youtube.com/feeds/videos.xml?channel_id=UCBR8-60-B28hp2BmDPdntcQ",
        ] {
            assert_eq!(ready(Kind::YouTube, link), id, "{link}");
        }
        for (input, page) in [
            ("@YouTube", "/@YouTube"),
            ("https://www.youtube.com/@LinusTechTips/videos", "/@LinusTechTips"),
            ("youtube.com/c/Vsauce", "/c/Vsauce"),
            ("https://www.youtube.com/user/pewdiepie", "/user/pewdiepie"),
        ] {
            assert_eq!(parse_source(Kind::YouTube, input), Ok(Parsed::YouTubePage(page.into())));
        }
        for bad in [
            "",
            "@a",
            "https://vimeo.com/channel/x",
            "UCshort",
            "https://www.youtube.com/watch?v=x",
        ] {
            assert!(parse_source(Kind::YouTube, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn feed_addresses_get_https_and_stay_public() {
        assert_eq!(ready(Kind::Rss, "example.com/feed.xml"), "https://example.com/feed.xml");
        assert_eq!(
            ready(Kind::Rss, "http://Example.COM/rss?x=1#top"),
            "http://example.com/rss?x=1"
        );
        for (bad, why) in [
            ("ftp://example.com/feed", SourceError::Url),
            ("https://user:pw@example.com/feed", SourceError::Url),
            ("http://localhost/feed", SourceError::Private),
            ("http://127.0.0.1/feed", SourceError::Private),
            ("http://10.0.0.8/feed", SourceError::Private),
            ("http://169.254.169.254/latest/meta-data", SourceError::Private),
            ("http://[::1]/feed", SourceError::Private),
            ("http://[::ffff:192.168.1.1]/feed", SourceError::Private),
            ("http://[64:ff9b:1::a9fe:a9fe]/feed", SourceError::Private),
            ("http://[::ffff:0:127.0.0.1]/feed", SourceError::Private),
            ("http://printer.local/feed", SourceError::Private),
        ] {
            assert_eq!(parse_source(Kind::Rss, bad), Err(why), "{bad}");
        }
        assert_eq!(
            parse_source(Kind::Rss, &format!("https://example.com/{}", "a".repeat(600))),
            Err(SourceError::TooLong)
        );
    }

    #[test]
    fn subreddits_and_bluesky_actors() {
        for input in [
            "rust",
            "r/Rust",
            "/r/rust/",
            "https://www.reddit.com/r/rust/new/",
            "old.reddit.com/r/RUST",
        ] {
            assert_eq!(ready(Kind::Reddit, input), "rust", "{input}");
        }
        for bad in ["r/", "r/a", "r/no spaces", "_under", "https://example.com/r/rust"] {
            assert!(parse_source(Kind::Reddit, bad).is_err(), "{bad}");
        }
        for input in [
            "alice.bsky.social",
            "@Alice.bsky.social",
            "https://bsky.app/profile/alice.bsky.social",
        ] {
            assert_eq!(ready(Kind::Bluesky, input), "alice.bsky.social", "{input}");
        }
        assert_eq!(
            ready(Kind::Bluesky, "did:plc:z72i7hdynmk6r22z27h6tvur"),
            "did:plc:z72i7hdynmk6r22z27h6tvur"
        );
        for bad in ["alice", "alice..bsky.social", "-a.bsky.social", "a.123", "did::x"] {
            assert!(parse_source(Kind::Bluesky, bad).is_err(), "{bad}");
        }
        assert_eq!(ready(Kind::Bridge, "/twitter/user/jack"), "twitter/user/jack");
        assert!(parse_source(Kind::Bridge, "twitter/../admin").is_err());
        assert!(parse_source(Kind::Bridge, "x y").is_err());
    }

    #[test]
    fn live_sources_are_canonical_and_reject_extra_url_parts() {
        assert_eq!(ready(Kind::Twitch, "@Streamer_12"), "streamer_12");
        assert_eq!(ready(Kind::Twitch, "https://www.twitch.tv/Streamer_12"), "streamer_12");
        assert_eq!(ready(Kind::Kick, "https://kick.com/John-Doe"), "john-doe");
        for bad in [
            "https://twitch.tv/name/videos",
            "https://twitch.tv/name?x=1",
            "https://example.com/name",
        ] {
            assert!(parse_source(Kind::Twitch, bad).is_err(), "{bad}");
        }
        assert_eq!(Kind::Twitch.interval_secs(), 120);
        assert_eq!(Kind::Kick.interval_secs(), 120);
    }

    #[test]
    fn private_and_reserved_addresses_are_not_public() {
        for ip in [
            "0.0.0.0",
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "192.0.0.8",
            "192.0.2.1",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "240.0.0.1",
            "::",
            "::1",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "fec0::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:0:127.0.0.1",
            "::127.0.0.1",
            "64:ff9b::a00:1",
            "64:ff9b:1::a9fe:a9fe",
            "64:ff9b:1::7f00:1",
            "64:ff9b:1:0:a9fe:a9fe::",
            "2002:c0a8:101::1",
            "2001:db8::1",
            "2001:0:4136:e378::1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip} counted as public");
        }
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "140.82.112.3",
            "2606:4700:4700::1111",
            "64:ff9b::808:808",
            "::ffff:0:8.8.8.8",
        ] {
            assert!(is_public(ip.parse().unwrap()), "{ip} counted as private");
        }
    }

    #[test]
    fn urls_for_every_kind() {
        assert_eq!(
            feed_url(Kind::YouTube, "UCx", None).unwrap(),
            "https://www.youtube.com/feeds/videos.xml?channel_id=UCx"
        );
        assert_eq!(
            feed_url(Kind::Reddit, "rust", None).unwrap(),
            "https://www.reddit.com/r/rust/new/.rss"
        );
        assert!(
            feed_url(Kind::Bluesky, "a.bsky.social", None)
                .unwrap()
                .contains("actor=a.bsky.social")
        );
        assert_eq!(feed_url(Kind::Bridge, "x/user/a", None), None);
        assert_eq!(
            feed_url(Kind::Bridge, "x/user/a", Some("https://rss.example/{route}?k=1")).unwrap(),
            "https://rss.example/x/user/a?k=1"
        );
        assert_eq!(
            feed_url(Kind::Bridge, "x/user/a", Some("https://rss.example/")).unwrap(),
            "https://rss.example/x/user/a"
        );
        assert_eq!(account_label(Kind::Reddit, "rust", Some("newest")), "r/rust");
        assert_eq!(account_label(Kind::Bluesky, "a.bsky.social", None), "@a.bsky.social");
        assert_eq!(account_label(Kind::YouTube, "UCx", Some(" Kurzgesagt ")), "Kurzgesagt");
        assert_eq!(page_url(Kind::Bridge, "x"), None);
    }

    #[test]
    fn channel_ids_are_found_in_a_youtube_page() {
        let page = r#"<html><head><link rel="canonical" href="https://www.youtube.com/channel/UCsXVk37bltHxD1rDPwtNM8Q"></head>
            <script>var x = {"channelId":"UCaaaaaaaaaaaaaaaaaaaaaa"}</script>"#;
        assert_eq!(youtube_channel_in_page(page).unwrap(), "UCsXVk37bltHxD1rDPwtNM8Q");
        let data = r#"{"metadata":{"externalId":"UCsXVk37bltHxD1rDPwtNM8Q","x":"channelId"}}"#;
        assert_eq!(youtube_channel_in_page(data).unwrap(), "UCsXVk37bltHxD1rDPwtNM8Q");
        assert_eq!(youtube_channel_in_page("<html>consent</html>"), None);
        assert_eq!(youtube_channel_in_page(r#""channelId":"UCshort""#), None);
    }

    #[test]
    fn backoff_doubles_up_to_six_hours() {
        let curve: Vec<i64> = (0..10).map(|fails| backoff_secs(300, fails)).collect();
        assert_eq!(curve, [300, 600, 1200, 2400, 4800, 9600, 19200, 21600, 21600, 21600]);
        assert_eq!(backoff_secs(120, 1), 240);
        assert_eq!(backoff_secs(10, 0), 60, "never faster than a minute");
        assert_eq!(backoff_secs(300, 1000), BACKOFF_MAX_SECS);
        assert_eq!(poll_interval(Kind::Rss, Some(1)), 300);
        assert_eq!(poll_interval(Kind::Rss, Some(10)), 600);
        assert_eq!(poll_interval(Kind::Rss, Some(1440)), 900);
        assert_eq!(poll_interval(Kind::Rss, None), 600);
        assert_eq!(poll_interval(Kind::YouTube, Some(1)), 300, "YouTube keeps its own pace");
        assert_eq!(poll_interval(Kind::Bluesky, None), 120);
    }

    #[test]
    fn messages_filters_and_placeholders() {
        assert_eq!(check_message(Some("  ")), Ok(None));
        assert_eq!(check_message(Some(" New: {title} ")), Ok(Some("New: {title}".into())));
        assert_eq!(check_message(Some(&"é".repeat(501))), Err(TextError::MessageTooLong));
        assert_eq!(
            check_message(Some(&"é".repeat(500))).unwrap().unwrap().chars().count(),
            500
        );

        let words = filter_words(["Live, Trailer ,live", ""]).unwrap();
        assert_eq!(words, ["live", "trailer"]);
        assert_eq!(filter_words([&"x".repeat(41)[..]]), Err(TextError::WordTooLong));
        let many: Vec<String> = (0..11).map(|i| format!("w{i}")).collect();
        assert_eq!(
            filter_words(many.iter().map(String::as_str)),
            Err(TextError::TooManyWords)
        );

        let (include, exclude) = (vec!["live".to_owned()], vec!["short".to_owned()]);
        assert!(passes_filters("We are LIVE now", &include, &exclude));
        assert!(!passes_filters("A new video", &include, &exclude));
        assert!(!passes_filters("Live short", &include, &exclude));
        assert!(passes_filters("Anything", &[], &[]));

        let fields = Fields {
            title: "Hello".into(),
            url: "https://x.test/1".into(),
            author: "Mira".into(),
            account: "Mira's channel".into(),
            published: "<t:1:f>".into(),
        };
        assert_eq!(
            render("{account}: {title} {url} by {author} at {published} {other} {", &fields),
            "Mira's channel: Hello https://x.test/1 by Mira at <t:1:f> {other} {"
        );
        assert_eq!(render("{{title}}", &fields), "{Hello}");
    }

    #[test]
    fn only_the_newest_fresh_items_are_announced_in_order() {
        let now = DateTime::parse_from_rfc3339("2026-11-06T12:00:00Z").unwrap().to_utc();
        let item = |id: &str, hours_ago: Option<i64>| Item {
            id: id.into(),
            title: id.into(),
            published: hours_ago.map(|h| now - Duration::hours(h)),
            ..Item::default()
        };
        let ids = |items: Vec<Item>| items.into_iter().map(|i| i.id).collect::<Vec<_>>();
        let found = vec![item("b", Some(1)), item("a", Some(2)), item("old", Some(24 * 4))];
        assert_eq!(ids(to_announce(found, now)), ["a", "b"]);
        let flood: Vec<Item> = (0..9).map(|h| item(&format!("n{h}"), Some(h))).collect();
        assert_eq!(ids(to_announce(flood, now)), ["n4", "n3", "n2", "n1", "n0"]);
        assert_eq!(ids(to_announce(vec![item("x", None)], now)), ["x"]);

        let since = now - Duration::hours(1);
        assert!(feed_wants(&item("new", Some(0)), since, &[], &[]));
        assert!(
            feed_wants(&item("slack", Some(1)), since, &[], &[]),
            "a little before is forgiven"
        );
        assert!(!feed_wants(&item("before", Some(2)), since, &[], &[]));
        assert!(feed_wants(&item("undated", None), since, &[], &[]));
        assert!(!feed_wants(&item("short", Some(0)), since, &[], &["short".into()]));
    }
}
