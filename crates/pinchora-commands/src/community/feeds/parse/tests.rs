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
