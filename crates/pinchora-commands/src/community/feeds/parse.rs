//! Reading what a source sent: RSS and Atom (YouTube, Reddit, podcasts, the bridge) through `feed-rs`,
//! Bluesky's author feed as JSON. Texts come out plain (HTML taken out, spaces collapsed) and short.

use chrono::{DateTime, Utc};
use feed_rs::model::{Entry, Feed, Text};
use pinchora_core::community::feeds::Item;
use pinchora_core::community::feeds::Kind;
use serde::Deserialize;

/// The most items kept from one fetch, newest first.
const ITEMS_MAX: usize = 50;
/// Longest kept title, summary and name, in characters.
const TITLE_MAX: usize = 256;
const SUMMARY_MAX: usize = 350;
const NAME_MAX: usize = 100;
const XML_DEPTH_MAX: usize = 64;
const XML_ELEMENTS_MAX: usize = 10_000;

/// What one fetch of a source holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    /// What the source calls itself.
    pub title: Option<String>,
    /// An RSS feed's own `ttl`, in minutes.
    pub ttl: Option<u32>,
    /// Newest first.
    pub items: Vec<Item>,
}

/// Why a body could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// Not RSS, Atom or the JSON that was expected.
    NotAFeed,
    /// Bluesky knows no such account.
    NoAccount,
}

impl ParseError {
    pub fn code(self) -> &'static str {
        match self {
            Self::NotAFeed => "feeds.error.parse",
            Self::NoAccount => "feeds.error.no-account",
        }
    }
}

pub fn parse(kind: Kind, body: &[u8]) -> Result<Parsed, ParseError> {
    match kind {
        Kind::Bluesky => bluesky(body),
        _ => xml(kind, body),
    }
}

fn xml(kind: Kind, body: &[u8]) -> Result<Parsed, ParseError> {
    xml_bounds(body)?;
    let feed: Feed = feed_rs::parser::parse(body).map_err(|_| ParseError::NotAFeed)?;
    let title = feed
        .title
        .as_ref()
        .map(text)
        .filter(|t| !t.is_empty())
        .map(|t| cut(&t, NAME_MAX));
    // feed-rs drops an episode's `itunes:duration` and `itunes:image` unless the item also has MediaRSS:
    // read them from the items in document order, when every item can be matched.
    let extras = podcast_extras(&String::from_utf8_lossy(body));
    let matched = extras.len() == feed.entries.len();
    let mut items: Vec<Item> = feed
        .entries
        .iter()
        .enumerate()
        .filter_map(|(at, e)| {
            let mut item = entry(kind, e)?;
            if let Some((duration, image)) = extras.get(at).filter(|_| matched) {
                item.duration_secs = item.duration_secs.or(*duration);
                item.image = item.image.or_else(|| image.clone());
            }
            Some(item)
        })
        .collect();
    // Most feeds list newest first; some do not. Undated items keep their place at the end.
    items.sort_by_key(|i| std::cmp::Reverse(i.published));
    items.truncate(ITEMS_MAX);
    Ok(Parsed {
        title,
        ttl: feed.ttl,
        items,
    })
}

/// Bound recursive work in feed-rs before it sees untrusted XML. This scan has no recursion
/// and ignores quoted `>` characters, comments, CDATA and processing instructions.
fn xml_bounds(body: &[u8]) -> Result<(), ParseError> {
    let mut at = 0;
    let mut depth: usize = 0;
    let mut elements = 0;
    while at < body.len() {
        if body[at] != b'<' {
            at += 1;
            continue;
        }
        if body[at..].starts_with(b"<!--") {
            let end = body[at + 4..]
                .windows(3)
                .position(|w| w == b"-->")
                .ok_or(ParseError::NotAFeed)?;
            at += 4 + end + 3;
            continue;
        }
        if body[at..].starts_with(b"<![CDATA[") {
            let end = body[at + 9..]
                .windows(3)
                .position(|w| w == b"]]>")
                .ok_or(ParseError::NotAFeed)?;
            at += 9 + end + 3;
            continue;
        }
        if body[at..].starts_with(b"<?") {
            let end = body[at + 2..]
                .windows(2)
                .position(|w| w == b"?>")
                .ok_or(ParseError::NotAFeed)?;
            at += 2 + end + 2;
            continue;
        }
        // DTDs and other declarations are unnecessary for feeds and may expand entities.
        if body[at..].starts_with(b"<!") {
            return Err(ParseError::NotAFeed);
        }
        let closing = body.get(at + 1) == Some(&b'/');
        let mut quote = None;
        let mut end = at + 1;
        while end < body.len() {
            let byte = body[end];
            match (quote, byte) {
                (Some(q), b) if q == b => quote = None,
                (None, b'\'' | b'"') => quote = Some(byte),
                (None, b'>') => break,
                _ => {}
            }
            end += 1;
        }
        if end == body.len() {
            return Err(ParseError::NotAFeed);
        }
        if closing {
            depth = depth.checked_sub(1).ok_or(ParseError::NotAFeed)?;
        } else {
            elements += 1;
            if elements > XML_ELEMENTS_MAX {
                return Err(ParseError::NotAFeed);
            }
            if body[at + 1..end].iter().rev().find(|b| !b.is_ascii_whitespace()) != Some(&b'/') {
                depth += 1;
                if depth > XML_DEPTH_MAX {
                    return Err(ParseError::NotAFeed);
                }
            }
        }
        at = end + 1;
    }
    (depth == 0).then_some(()).ok_or(ParseError::NotAFeed)
}

fn entry(kind: Kind, e: &Entry) -> Option<Item> {
    let url = e
        .links
        .iter()
        .find(|l| l.rel.as_deref().is_none_or(|r| r == "alternate"))
        .or_else(|| e.links.first())
        .map(|l| l.href.trim().to_owned())
        .filter(|u| is_web_link(u));
    let id = Some(e.id.trim().to_owned())
        .filter(|id| !id.is_empty())
        .or_else(|| url.clone())?;
    let title = e
        .title
        .as_ref()
        .map(text)
        .filter(|t| !t.is_empty())
        .or_else(|| e.summary.as_ref().map(text))
        .unwrap_or_default();
    let author = e
        .authors
        .first()
        .and_then(|a| a.name.as_deref())
        .map(|name| match kind {
            // Reddit names its posters `/u/name`.
            Kind::Reddit => name.trim().trim_start_matches('/').to_owned(),
            _ => name.trim().to_owned(),
        })
        .filter(|n| !n.is_empty())
        .map(|n| cut(&n, NAME_MAX));
    let image = e
        .media
        .iter()
        .flat_map(|m| m.thumbnails.iter().map(|t| t.image.uri.clone()))
        .chain(
            e.media
                .iter()
                .flat_map(|m| m.content.iter())
                .filter(|c| c.content_type.as_ref().is_some_and(|t| t.ty() == "image"))
                .filter_map(|c| c.url.as_ref().map(|u| u.to_string())),
        )
        .find(|u| is_web_link(u))
        .or_else(|| youtube_thumbnail(kind, &e.id))
        .or_else(|| {
            // Reddit and many blogs carry their picture only inside the post's HTML.
            let html = [
                e.content.as_ref().and_then(|c| c.body.as_deref()),
                e.summary.as_ref().map(|s| s.content.as_str()),
            ];
            html.into_iter().flatten().find_map(first_image)
        });
    let audio = e
        .media
        .iter()
        .flat_map(|m| m.content.iter())
        .filter(|c| c.content_type.as_ref().is_some_and(|t| t.ty() == "audio"))
        .find_map(|c| c.url.as_ref().map(|u| u.to_string()))
        .filter(|u| is_web_link(u));
    let duration_secs = e
        .media
        .iter()
        .find_map(|m| m.duration.or_else(|| m.content.iter().find_map(|c| c.duration)))
        .map(|d| d.as_secs())
        .filter(|s| *s > 0);
    // Reddit's summary is a table of links and YouTube's a whole description: only feeds get one.
    let summary = match kind {
        Kind::Rss | Kind::Bridge => e
            .summary
            .as_ref()
            .map(text)
            .or_else(|| {
                let content = e.content.as_ref()?;
                content.body.as_deref().map(|body| {
                    if is_html(content.content_type.subty().as_ref()) {
                        plain(body)
                    } else {
                        one_line(body)
                    }
                })
            })
            .filter(|s| !s.is_empty() && *s != title)
            .map(|s| cut(&s, SUMMARY_MAX)),
        _ => None,
    };
    Some(Item {
        id: cut(&id, 500),
        title: cut(&title, TITLE_MAX),
        url,
        author,
        summary,
        published: e.published.or(e.updated),
        image,
        audio,
        duration_secs,
    })
}

/// A feed text as plain text: HTML only when the feed says it is HTML, so a title like “Coral <3” keeps its `<3`.
/// Each RSS item's `itunes:duration` (seconds) and `itunes:image`, in document order.
fn podcast_extras(xml: &str) -> Vec<(Option<u64>, Option<String>)> {
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<item") {
        let after = &rest[start + 5..];
        // `<item>` or `<item attr…>`, not `<itemref>`.
        if !after.starts_with(['>', ' ', '\n', '\t', '\r']) {
            rest = after;
            continue;
        }
        let end = after.find("</item>").unwrap_or(after.len());
        let block = &after[..end];
        let duration = between(block, "<itunes:duration>", "</itunes:duration>").and_then(|d| clock_secs(d.trim()));
        let image = block.find("<itunes:image").and_then(|at| {
            let tag = &block[at..];
            let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
            between(tag, "href=\"", "\"")
                .map(decode_entities)
                .filter(|u| is_web_link(u))
        });
        found.push((duration, image));
        rest = &after[end..];
    }
    found
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let from = text.find(open)? + open.len();
    let to = from + text[from..].find(close)?;
    Some(&text[from..to])
}

/// `1:02:03`, `42:10` or `2530` as seconds.
fn clock_secs(text: &str) -> Option<u64> {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() > 3 {
        return None;
    }
    let mut secs: u64 = 0;
    for part in parts {
        secs = secs.checked_mul(60)?.checked_add(part.parse::<u64>().ok()?)?;
    }
    (secs > 0).then_some(secs)
}

fn text(t: &Text) -> String {
    if is_html(t.content_type.subty().as_ref()) {
        plain(&t.content)
    } else {
        one_line(&t.content)
    }
}

/// Whether a media subtype (`html` of `text/html`) is HTML.
fn is_html(subtype: &str) -> bool {
    subtype == "html" || subtype == "xhtml+xml"
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A YouTube video's picture, from its id (`yt:video:<id>`), for feeds that leave it out.
fn youtube_thumbnail(kind: Kind, id: &str) -> Option<String> {
    let video = id.strip_prefix("yt:video:").filter(|_| kind == Kind::YouTube)?;
    let ok = !video.is_empty()
        && video
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ok.then(|| format!("https://i.ytimg.com/vi/{video}/hqdefault.jpg"))
}

/// The address of the first `<img src="…">` in some HTML.
fn first_image(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find("<img") {
        let tag_start = from + at;
        let tag_end = lower[tag_start..].find('>').map_or(lower.len(), |e| tag_start + e);
        let tag = &html[tag_start..tag_end];
        if let Some(src) = tag.to_ascii_lowercase().find("src=") {
            let rest = &tag[src + 4..];
            let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'');
            let value = match quote {
                Some(q) => rest[1..].split(q).next(),
                None => rest.split_whitespace().next(),
            };
            if let Some(url) = value.map(decode_entities).filter(|u| is_web_link(u)) {
                return Some(url);
            }
        }
        from = tag_end;
    }
    None
}

/// Parse and serialize an untrusted link for Discord text and embed fields.
pub(crate) fn web_link(raw: &str) -> Option<String> {
    if raw.len() > 1000
        || raw
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"'))
    {
        return None;
    }
    let url = url::Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let url = url.to_string();
    let authority_end = url
        .find("://")
        .and_then(|at| url[at + 3..].find(['/', '?', '#']).map(|end| at + 3 + end))
        .unwrap_or(url.len());
    let mut safe = String::with_capacity(url.len());
    for (at, ch) in url.char_indices() {
        match ch {
            '(' | ')' | '\\' => safe.push_str(&format!("%{:02X}", ch as u32)),
            '[' | ']' if at >= authority_end => safe.push_str(&format!("%{:02X}", ch as u32)),
            _ => safe.push(ch),
        }
    }
    Some(safe)
}

pub(crate) fn is_web_link(url: &str) -> bool {
    web_link(url).is_some()
}

/// HTML as plain text: tags out, the common entities decoded, spaces collapsed.
pub(crate) fn plain(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let decoded = decode_entities(&out);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let here = &rest[at..];
        let end = here.find(';').filter(|e| *e <= 10);
        let decoded = end.and_then(|end| {
            let name = &here[1..end];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                _ => name
                    .strip_prefix("#x")
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|n| n.parse().ok()))
                    .and_then(char::from_u32),
            }?;
            Some((c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &here[end + 1..];
            }
            None => {
                out.push('&');
                rest = &here[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// At most `max` characters, an ellipsis marking the cut.
pub(crate) fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.truncate(out.trim_end().len());
    out.push('…');
    out
}

// /Bluesky

#[derive(Deserialize)]
struct AuthorFeed {
    feed: Vec<FeedViewPost>,
}

#[derive(Deserialize)]
struct FeedViewPost {
    post: PostView,
    /// Set for reposts and the pinned post: neither is a new post of the account.
    reason: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PostView {
    uri: String,
    author: Author,
    record: Record,
    embed: Option<serde_json::Value>,
    indexed_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Author {
    handle: String,
    display_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    #[serde(default)]
    text: String,
    created_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct XrpcError {
    error: String,
}

fn bluesky(body: &[u8]) -> Result<Parsed, ParseError> {
    let feed: AuthorFeed = match serde_json::from_slice(body) {
        Ok(feed) => feed,
        Err(_) => {
            return Err(match serde_json::from_slice::<XrpcError>(body) {
                Ok(e) if e.error == "InvalidRequest" || e.error == "AccountTakedown" => ParseError::NoAccount,
                _ => ParseError::NotAFeed,
            });
        }
    };
    let mut title = None;
    let mut items = Vec::new();
    for view in feed.feed.into_iter().filter(|v| v.reason.is_none()) {
        let post = view.post;
        let name = post
            .author
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(|n| cut(n, NAME_MAX));
        if title.is_none() {
            title.clone_from(&name);
        }
        let Some(rkey) = post.uri.rsplit('/').next().filter(|k| !k.is_empty()) else {
            continue;
        };
        let text = post.record.text.split_whitespace().collect::<Vec<_>>().join(" ");
        items.push(Item {
            url: Some(format!("https://bsky.app/profile/{}/post/{rkey}", post.author.handle)),
            title: cut(&text, TITLE_MAX),
            author: name.or_else(|| Some(format!("@{}", post.author.handle))),
            summary: None,
            // The time Bluesky saw it; the record's own time is whatever the app wrote.
            published: post.indexed_at.or(post.record.created_at),
            image: post.embed.as_ref().and_then(embed_picture),
            audio: None,
            duration_secs: None,
            id: post.uri,
        });
    }
    items.sort_by_key(|i| std::cmp::Reverse(i.published));
    items.truncate(ITEMS_MAX);
    Ok(Parsed {
        title,
        ttl: None,
        items,
    })
}

/// The first picture of a post's embed: an image, a link card's thumbnail, or either inside a quote
/// with media.
fn embed_picture(embed: &serde_json::Value) -> Option<String> {
    let found = embed["images"][0]["thumb"]
        .as_str()
        .or_else(|| embed["external"]["thumb"].as_str())
        .or_else(|| embed["thumbnail"].as_str())
        .map(str::to_owned);
    found
        .filter(|u| is_web_link(u))
        .or_else(|| embed.get("media").and_then(embed_picture))
}

#[cfg(test)]
mod tests {
    use crate::community::feeds::parse::*;

    #[test]
    fn deeply_nested_media_groups_are_rejected_before_feed_rs() {
        let mut xml = String::from(
            r#"<rss xmlns:media="http://search.yahoo.com/mrss/"><channel><title>x</title><item><guid>x</guid>"#,
        );
        xml.push_str(&"<media:group>".repeat(20_000));
        xml.push_str(&"</media:group>".repeat(20_000));
        xml.push_str("</item></channel></rss>");
        assert_eq!(parse(Kind::Rss, xml.as_bytes()), Err(ParseError::NotAFeed));
        let wide = format!("<rss><channel>{}</channel></rss>", "<item/>".repeat(XML_ELEMENTS_MAX));
        assert_eq!(parse(Kind::Rss, wide.as_bytes()), Err(ParseError::NotAFeed));
    }

    #[test]
    fn web_links_keep_ipv6_host_brackets_but_escape_markdown_in_paths() {
        assert_eq!(
            web_link("https://[2606:4700:4700::1111]/a)[spoof](https://evil.example)"),
            Some("https://[2606:4700:4700::1111]/a%29%5Bspoof%5D%28https://evil.example%29".into())
        );
    }

    const YOUTUBE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns:yt="http://www.youtube.com/xml/schemas/2015" xmlns:media="http://search.yahoo.com/mrss/" xmlns="http://www.w3.org/2005/Atom">
 <link rel="self" href="http://www.youtube.com/feeds/videos.xml?channel_id=UCsXVk37bltHxD1rDPwtNM8Q"/>
 <id>yt:channel:sXVk37bltHxD1rDPwtNM8Q</id>
 <yt:channelId>sXVk37bltHxD1rDPwtNM8Q</yt:channelId>
 <title>Kurzgesagt – In a Nutshell</title>
 <author><name>Kurzgesagt – In a Nutshell</name><uri>https://www.youtube.com/channel/UCsXVk37bltHxD1rDPwtNM8Q</uri></author>
 <published>2013-07-09T05:56:39+00:00</published>
 <entry>
  <id>yt:video:older00001</id>
  <yt:videoId>older00001</yt:videoId>
  <title>The Older One</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=older00001"/>
  <author><name>Kurzgesagt – In a Nutshell</name></author>
  <published>2026-10-01T14:00:00+00:00</published>
  <updated>2026-10-02T14:00:00+00:00</updated>
 </entry>
 <entry>
  <id>yt:video:abcDEF12345</id>
  <yt:videoId>abcDEF12345</yt:videoId>
  <title>What If The Sun &amp; Moon Swapped?</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=abcDEF12345"/>
  <author><name>Kurzgesagt – In a Nutshell</name></author>
  <published>2026-11-05T14:00:00+00:00</published>
  <media:group>
   <media:title>What If The Sun &amp; Moon Swapped?</media:title>
   <media:content url="https://www.youtube.com/v/abcDEF12345?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
   <media:thumbnail url="https://i2.ytimg.com/vi/abcDEF12345/hqdefault.jpg" width="480" height="360"/>
   <media:description>A long description with @everyone in it</media:description>
  </media:group>
 </entry>
</feed>"#;

    const PODCAST: &str = r#"<?xml version="1.0"?>
<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
<channel>
 <title>Reef Radio</title>
 <link>https://radio.example/</link>
 <ttl>10</ttl>
 <item>
  <title>Episode 12: Coral &lt;3</title>
  <link>https://radio.example/12</link>
  <guid isPermaLink="false">reef-12</guid>
  <pubDate>Thu, 05 Nov 2026 09:30:00 +0000</pubDate>
  <description><![CDATA[<p>We talk <b>coral</b>&nbsp;reefs.</p>]]></description>
  <enclosure url="https://cdn.radio.example/12.mp3" length="40000000" type="audio/mpeg"/>
  <itunes:duration>42:10</itunes:duration>
  <itunes:image href="https://cdn.radio.example/12.jpg"/>
 </item>
 <item>
  <title>No guid here</title>
  <link>https://radio.example/11</link>
 </item>
</channel>
</rss>"#;

    const REDDIT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:media="http://search.yahoo.com/mrss/">
 <category term="rust" label="r/rust"/>
 <updated>2026-11-06T10:00:00+00:00</updated>
 <id>/r/rust/new/.rss</id>
 <link rel="self" href="https://www.reddit.com/r/rust/new/.rss" type="application/atom+xml"/>
 <title>newest submissions : rust</title>
 <entry>
  <author><name>/u/ferris</name><uri>https://www.reddit.com/user/ferris</uri></author>
  <category term="rust" label="r/rust"/>
  <content type="html">&lt;table&gt;&lt;tr&gt;&lt;td&gt;&lt;a href=&quot;https://www.reddit.com/r/rust/comments/1abcde/&quot;&gt;&lt;img src=&quot;https://b.thumbs.redditmedia.com/x.jpg?a=1&amp;amp;b=2&quot; alt=&quot;x&quot; /&gt;&lt;/a&gt;&lt;/td&gt;&lt;td&gt;submitted by ferris&lt;/td&gt;&lt;/tr&gt;&lt;/table&gt;</content>
  <id>t3_1abcde</id>
  <media:thumbnail url="https://b.thumbs.redditmedia.com/x.jpg"/>
  <link href="https://www.reddit.com/r/rust/comments/1abcde/announcing_rust_2_0/"/>
  <updated>2026-11-06T09:59:00+00:00</updated>
  <published>2026-11-06T09:59:00+00:00</published>
  <title>Announcing Rust 2.0 **not really** &lt;@&amp;123&gt;</title>
 </entry>
</feed>"#;

    const BLUESKY: &str = r#"{"feed":[
 {"post":{"uri":"at://did:plc:abc/app.bsky.feed.post/3lold","cid":"c","author":{"did":"did:plc:abc","handle":"alice.bsky.social","displayName":"Alice"},
   "record":{"$type":"app.bsky.feed.post","text":"pinned","createdAt":"2026-01-01T00:00:00Z"},"indexedAt":"2026-01-01T00:00:00Z"},
  "reason":{"$type":"app.bsky.feed.defs#reasonPin"}},
 {"post":{"uri":"at://did:plc:abc/app.bsky.feed.post/3lnew","cid":"c","author":{"did":"did:plc:abc","handle":"alice.bsky.social","displayName":"Alice"},
   "record":{"$type":"app.bsky.feed.post","text":"Hello\nworld  from the reef","createdAt":"2026-11-06T08:00:00Z"},
   "embed":{"$type":"app.bsky.embed.images#view","images":[{"thumb":"https://cdn.bsky.app/img/feed_thumbnail/plain/x@jpeg","fullsize":"https://cdn.bsky.app/f","alt":""}]},
   "indexedAt":"2026-11-06T08:00:01Z"}},
 {"post":{"uri":"at://did:plc:zzz/app.bsky.feed.post/3lrepost","cid":"c","author":{"did":"did:plc:zzz","handle":"bob.bsky.social"},
   "record":{"text":"someone else"},"indexedAt":"2026-11-06T07:00:00Z"},
  "reason":{"$type":"app.bsky.feed.defs#reasonRepost"}},
 {"post":{"uri":"at://did:plc:abc/app.bsky.feed.post/3lquote","cid":"c","author":{"did":"did:plc:abc","handle":"alice.bsky.social","displayName":"Alice"},
   "record":{"text":"look"},"embed":{"$type":"app.bsky.embed.recordWithMedia#view","media":{"$type":"app.bsky.embed.external#view","external":{"uri":"https://x.test","thumb":"https://cdn.bsky.app/t"}}},
   "indexedAt":"2026-11-05T08:00:00Z"}}
],"cursor":"x"}"#;

    #[test]
    fn youtube_atom() {
        let parsed = parse(Kind::YouTube, YOUTUBE.as_bytes()).unwrap();
        assert_eq!(parsed.title.as_deref(), Some("Kurzgesagt – In a Nutshell"));
        let ids: Vec<&str> = parsed.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["yt:video:abcDEF12345", "yt:video:older00001"], "newest first");
        let video = &parsed.items[0];
        assert_eq!(video.title, "What If The Sun & Moon Swapped?");
        assert_eq!(
            video.url.as_deref(),
            Some("https://www.youtube.com/watch?v=abcDEF12345")
        );
        assert_eq!(video.author.as_deref(), Some("Kurzgesagt – In a Nutshell"));
        assert_eq!(
            video.image.as_deref(),
            Some("https://i2.ytimg.com/vi/abcDEF12345/hqdefault.jpg")
        );
        assert_eq!(video.summary, None, "a video's description is not copied");
        assert_eq!(video.published.unwrap().to_rfc3339(), "2026-11-05T14:00:00+00:00");
        assert_eq!(
            parsed.items[1].image.as_deref(),
            Some("https://i.ytimg.com/vi/older00001/hqdefault.jpg"),
            "a video without a picture in the feed gets YouTube's"
        );
    }

    #[test]
    fn rss_podcast() {
        let parsed = parse(Kind::Rss, PODCAST.as_bytes()).unwrap();
        assert_eq!(parsed.title.as_deref(), Some("Reef Radio"));
        assert_eq!(parsed.ttl, Some(10));
        let episode = &parsed.items[0];
        assert_eq!(episode.id, "reef-12");
        assert_eq!(episode.title, "Episode 12: Coral <3");
        assert_eq!(episode.summary.as_deref(), Some("We talk coral reefs."));
        assert_eq!(episode.audio.as_deref(), Some("https://cdn.radio.example/12.mp3"));
        assert_eq!(episode.duration_secs, Some(42 * 60 + 10));
        assert_eq!(episode.image.as_deref(), Some("https://cdn.radio.example/12.jpg"));
        // An item without a guid still gets an id, the same every time.
        let again = parse(Kind::Rss, PODCAST.as_bytes()).unwrap();
        assert!(!parsed.items[1].id.is_empty());
        assert_eq!(parsed.items[1].id, again.items[1].id);
        assert_eq!(parsed.items[1].published, None);
    }

    #[test]
    fn reddit_atom() {
        let parsed = parse(Kind::Reddit, REDDIT.as_bytes()).unwrap();
        let post = &parsed.items[0];
        assert_eq!(post.id, "t3_1abcde");
        assert_eq!(post.author.as_deref(), Some("u/ferris"));
        assert_eq!(
            post.title, "Announcing Rust 2.0 **not really** <@&123>",
            "made safe later, when posted"
        );
        assert_eq!(
            post.image.as_deref(),
            Some("https://b.thumbs.redditmedia.com/x.jpg?a=1&b=2")
        );
        assert_eq!(post.summary, None);
    }

    #[test]
    fn bluesky_author_feed() {
        let parsed = parse(Kind::Bluesky, BLUESKY.as_bytes()).unwrap();
        assert_eq!(parsed.title.as_deref(), Some("Alice"));
        let ids: Vec<&str> = parsed.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "at://did:plc:abc/app.bsky.feed.post/3lnew",
                "at://did:plc:abc/app.bsky.feed.post/3lquote"
            ],
            "reposts and the pinned post are left out"
        );
        let post = &parsed.items[0];
        assert_eq!(post.title, "Hello world from the reef");
        assert_eq!(
            post.url.as_deref(),
            Some("https://bsky.app/profile/alice.bsky.social/post/3lnew")
        );
        assert_eq!(
            post.image.as_deref(),
            Some("https://cdn.bsky.app/img/feed_thumbnail/plain/x@jpeg")
        );
        assert_eq!(parsed.items[1].image.as_deref(), Some("https://cdn.bsky.app/t"));
        let missing = br#"{"error":"InvalidRequest","message":"Profile not found"}"#;
        assert_eq!(parse(Kind::Bluesky, missing), Err(ParseError::NoAccount));
        assert_eq!(parse(Kind::Bluesky, b"<html>"), Err(ParseError::NotAFeed));
    }

    #[test]
    fn not_a_feed_and_plain_text() {
        assert_eq!(
            parse(Kind::Rss, b"<html><body>hello</body></html>"),
            Err(ParseError::NotAFeed)
        );
        assert_eq!(parse(Kind::Rss, b""), Err(ParseError::NotAFeed));
        assert_eq!(
            plain("<p>a&amp;b &#233;&#x41; &bogus; 5 &lt; 6</p>"),
            "a&b éA &bogus; 5 < 6"
        );
        assert_eq!(clock_secs("1:02:03"), Some(3723));
        assert_eq!(clock_secs("42:10"), Some(2530));
        assert_eq!(clock_secs("2530"), Some(2530));
        assert_eq!(clock_secs("soon"), None);
        assert_eq!(cut("abcdef", 4), "abc…");
        assert_eq!(cut("ab", 4), "ab");
        assert!(!is_web_link("javascript:alert(1)"));
        assert!(!is_web_link("https://x.test/a b"));
        assert!(is_web_link("https://x.test/a?b=1"));
    }
}
