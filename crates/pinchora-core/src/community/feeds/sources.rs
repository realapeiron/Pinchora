use super::*;

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
    pub(super) const fn reads_ttl(self) -> bool {
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
