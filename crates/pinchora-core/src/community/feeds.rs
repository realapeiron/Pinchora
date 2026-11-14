//! Social feeds: which sources a server follows and how a new post is announced. Pure: the bot,
//! `/feeds` and the dashboard read sources, messages, filters and poll timing with these rules.

use std::net::IpAddr;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use url::{Host, Url};
mod messages;
mod sources;

pub use messages::{Fields, TextError, check_message, filter_words, passes_filters, render};
pub use sources::{
    Kind, Parsed, SourceError, account_label, feed_address, feed_url, is_public, is_youtube_channel_id, page_url,
    parse_source, youtube_channel_in_page,
};

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
