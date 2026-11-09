//! The announcement of a new item: the server's message with its placeholders filled, and the card.
//! The bot posts it and the dashboard previews it with this one function, so what a manager sees is
//! what the channel gets.
//!
//! Everything that comes from the source (titles, names, summaries) loses its mentions and has its
//! markdown escaped; links are kept only when they are plain http(s). The message itself pings nobody
//! but the feed's role.

use chrono::{DateTime, Utc};
use pinchora_core::community::feeds::Fields;
use pinchora_core::community::feeds::Item;
use pinchora_core::community::feeds::Kind;
use pinchora_core::community::feeds::account_label;
use pinchora_core::community::feeds::render as fill;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::FeedRow;
use serde::Serialize;
use twilight_model::{
    channel::message::{Embed, embed::EmbedField},
    util::Timestamp,
};
use twilight_util::builder::embed::{EmbedAuthorBuilder, EmbedBuilder, EmbedFooterBuilder, ImageSource};

use crate::admin::custom::scrub_value;
use crate::community::feeds::parse::cut;
use crate::community::feeds::parse::web_link;
use crate::infra::util::strip_mentions;

/// Discord's limit for a message's text.
const CONTENT_MAX: usize = 2000;

/// What a feed announces with.
#[derive(Debug, Clone, Copy)]
pub struct Draft<'a> {
    pub kind: Kind,
    pub source: &'a str,
    /// What the source calls itself, when known.
    pub display_name: Option<&'a str>,
    /// `None`: the kind's default message.
    pub message: Option<&'a str>,
    pub embed: bool,
    pub ping_role: Option<u64>,
}

impl<'a> Draft<'a> {
    pub fn of(feed: &'a FeedRow) -> Option<Self> {
        Some(Self {
            kind: Kind::parse(&feed.kind)?,
            source: &feed.source,
            display_name: feed.display_name.as_deref(),
            message: feed.message.as_deref(),
            embed: feed.embed,
            ping_role: feed.ping_role_id.map(|r| r as u64),
        })
    }
}

/// A message ready to post.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeedMessage {
    pub content: String,
    pub embed: Option<FeedEmbed>,
}

/// The card, in the shape the dashboard's message preview reads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeedEmbed {
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    /// `#rrggbb`.
    pub color: String,
    pub author: Option<String>,
    pub footer: Option<String>,
    pub image: Option<String>,
    pub thumbnail: Option<String>,
    pub fields: Vec<FeedField>,
    #[serde(skip)]
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeedField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

/// The colour of each kind's card.
fn color(kind: Kind) -> u32 {
    match kind {
        Kind::YouTube => 0xff0033,
        Kind::Rss => 0xf26522,
        Kind::Reddit => 0xff4500,
        Kind::Bluesky => 0x1185fe,
        Kind::Bridge => 0x8b7cf6,
        Kind::Twitch => 0x9146ff,
        Kind::Kick => 0x53fc18,
    }
}

/// The kind's name, for footers and lists.
pub fn kind_label(lang: Lang, kind: Kind) -> String {
    t!(lang, &format!("feeds.kind.{}", kind.key()))
}

/// The message used when a feed has none of its own.
pub fn default_message(lang: Lang, kind: Kind) -> String {
    t!(lang, &format!("feeds.default.{}", kind.key()))
}

/// `1:02:03` or `42:10`.
fn clock(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// One line, mentions out, markdown escaped, at most `max` characters with the escapes.
fn safe(text: &str, max: usize) -> String {
    let scrubbed = scrub_value(&cut(text, max * 2));
    cut(&scrubbed.split_whitespace().collect::<Vec<_>>().join(" "), max)
}

/// The announcement of `item` for a feed drafted as `draft`, in `lang`.
pub fn render(lang: Lang, draft: &Draft<'_>, item: &Item) -> FeedMessage {
    let account = account_label(draft.kind, draft.source, draft.display_name);
    let link = item.url.as_deref().and_then(web_link);
    let fields = Fields {
        title: safe(&item.title, 256),
        // With Pinchora's card the link is in angle brackets, so Discord does not add a second preview.
        url: link.as_ref().map_or_else(
            String::new,
            |u| {
                if draft.embed { format!("<{u}>") } else { u.to_owned() }
            },
        ),
        author: item
            .author
            .as_deref()
            .map_or_else(|| safe(&account, 100), |a| safe(a, 100)),
        account: safe(&account, 100),
        published: item
            .published
            .map_or_else(String::new, |at| format!("<t:{}:f>", at.timestamp())),
    };
    let template = draft
        .message
        .map_or_else(|| default_message(lang, draft.kind), str::to_owned);
    let mut content = fill(&template, &fields).trim().to_owned();
    if let Some(role) = draft.ping_role {
        content = format!("<@&{role}> {content}");
    }
    let embed = draft
        .embed
        .then(|| card(lang, draft.kind, &account, item, link.as_deref()));
    FeedMessage {
        content: cut(&content, CONTENT_MAX),
        embed,
    }
}

fn card(lang: Lang, kind: Kind, account: &str, item: &Item, link: Option<&str>) -> FeedEmbed {
    let picture = item.image.as_deref().and_then(|url| {
        if matches!(kind, Kind::Twitch | Kind::Kick) {
            crate::community::feeds::live::media_url(kind, url)
        } else {
            web_link(url)
        }
    });
    // A video or a post is its picture; an article or an episode only illustrated by one.
    let big = matches!(kind, Kind::YouTube | Kind::Bluesky | Kind::Twitch | Kind::Kick);
    let mut fields = Vec::new();
    if matches!(kind, Kind::Twitch | Kind::Kick)
        && let Some(game) = item.summary.as_deref().filter(|game| !game.is_empty())
    {
        fields.push(FeedField {
            name: t!(lang, "feeds.live.game"),
            value: safe(game, 100),
            inline: true,
        });
    }
    if let Some(secs) = item.duration_secs {
        fields.push(FeedField {
            name: t!(lang, "feeds.card.length"),
            value: clock(secs),
            inline: true,
        });
    }
    if let Some(audio) = item.audio.as_deref().and_then(web_link) {
        fields.push(FeedField {
            name: t!(lang, "feeds.card.listen"),
            value: format!("[{}]({audio})", t!(lang, "feeds.card.audio-file")),
            inline: true,
        });
    }
    let footer = if item.audio.is_some() {
        t!(lang, "feeds.kind.podcast")
    } else {
        kind_label(lang, kind)
    };
    let title = Some(safe(&item.title, 256)).filter(|t| !t.is_empty());
    FeedEmbed {
        title,
        description: (!matches!(kind, Kind::Twitch | Kind::Kick))
            .then(|| item.summary.as_deref().map(|s| safe(s, 350)).filter(|s| !s.is_empty()))
            .flatten(),
        url: link.map(str::to_owned),
        color: format!("#{:06x}", color(kind)),
        // Discord shows an author line as plain text: mentions out, nothing to escape.
        author: Some(cut(&strip_mentions(account), 256)),
        footer: Some(footer),
        image: picture.clone().filter(|_| big),
        thumbnail: picture.filter(|_| !big),
        fields,
        timestamp: item.published,
    }
}

impl FeedEmbed {
    /// The card as Discord takes it.
    pub fn to_embed(&self, kind: Kind) -> Embed {
        let mut embed = EmbedBuilder::new().color(color(kind));
        if let Some(title) = &self.title {
            embed = embed.title(title);
        }
        if let Some(url) = &self.url {
            embed = embed.url(url);
        }
        if let Some(description) = &self.description {
            embed = embed.description(description);
        }
        if let Some(author) = &self.author {
            embed = embed.author(EmbedAuthorBuilder::new(author));
        }
        if let Some(footer) = &self.footer {
            embed = embed.footer(EmbedFooterBuilder::new(footer));
        }
        if let Some(source) = self.image.as_deref().and_then(|u| ImageSource::url(u).ok()) {
            embed = embed.image(source);
        }
        if let Some(source) = self.thumbnail.as_deref().and_then(|u| ImageSource::url(u).ok()) {
            embed = embed.thumbnail(source);
        }
        for field in &self.fields {
            embed = embed.field(EmbedField {
                inline: field.inline,
                name: field.name.clone(),
                value: field.value.clone(),
            });
        }
        if let Some(at) = self.timestamp.and_then(|at| Timestamp::from_secs(at.timestamp()).ok()) {
            embed = embed.timestamp(at);
        }
        embed.build()
    }
}

/// An item to preview a message with before the source has shown a real one.
pub fn sample_item(lang: Lang, kind: Kind, now: DateTime<Utc>) -> Item {
    Item {
        id: "sample".to_owned(),
        title: t!(lang, &format!("feeds.sample.{}", kind.key())),
        url: Some(match kind {
            Kind::YouTube => "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_owned(),
            Kind::Reddit => "https://www.reddit.com/r/example/comments/abc123/".to_owned(),
            Kind::Bluesky => "https://bsky.app/profile/example.bsky.social/post/3l".to_owned(),
            Kind::Rss | Kind::Bridge => "https://example.com/posts/hello".to_owned(),
            Kind::Twitch => "https://www.twitch.tv/example".to_owned(),
            Kind::Kick => "https://kick.com/example".to_owned(),
        }),
        author: None,
        summary: matches!(kind, Kind::Rss | Kind::Bridge).then(|| t!(lang, "feeds.sample.summary")),
        published: Some(now),
        image: None,
        audio: None,
        duration_secs: None,
    }
}

#[cfg(test)]
mod tests {
    use crate::community::feeds::message::*;

    fn item() -> Item {
        Item {
            id: "1".into(),
            title: "Big **news** @everyone <@&42> [click](https://evil.test)".into(),
            url: Some("https://example.com/a".into()),
            author: Some("<@1> Mira".into()),
            summary: Some("Line one\n\nline two".into()),
            published: Some(DateTime::from_timestamp(1_700_000_000, 0).unwrap()),
            image: Some("https://cdn.example/p.jpg".into()),
            audio: Some("https://cdn.example/e.mp3".into()),
            duration_secs: Some(3723),
        }
    }

    fn draft(embed: bool, message: Option<&'static str>, ping_role: Option<u64>) -> Draft<'static> {
        Draft {
            kind: Kind::Rss,
            source: "https://example.com/feed",
            display_name: Some("The *Reef* Times"),
            message,
            embed,
            ping_role,
        }
    }

    #[test]
    fn source_text_is_made_safe_and_links_kept() {
        let message = render(
            Lang::En,
            &draft(false, Some("{account}: {title} {url} {author} {published}"), None),
            &item(),
        );
        assert!(!message.content.contains("@everyone"), "{}", message.content);
        assert!(!message.content.contains("<@&42>"), "{}", message.content);
        assert!(!message.content.contains("<@1>"), "{}", message.content);
        assert!(message.content.contains(r"Big \*\*news\*\*"), "{}", message.content);
        assert!(
            message.content.contains(r"\[click\]\(https://evil.test\)"),
            "{}",
            message.content
        );
        assert!(message.content.contains(r"The \*Reef\* Times: "), "{}", message.content);
        assert!(
            message.content.contains(" https://example.com/a "),
            "a bare link unfurls: {}",
            message.content
        );
        assert!(message.content.ends_with("<t:1700000000:f>"), "{}", message.content);
        assert!(message.embed.is_none());
    }

    #[test]
    fn the_card_carries_the_episode_and_the_role_pings_first() {
        let message = render(Lang::En, &draft(true, None, Some(77)), &item());
        assert!(message.content.starts_with("<@&77> "), "{}", message.content);
        assert!(
            message.content.contains("<https://example.com/a>"),
            "no second preview: {}",
            message.content
        );
        let card = message.embed.unwrap();
        assert_eq!(card.url.as_deref(), Some("https://example.com/a"));
        assert_eq!(card.thumbnail.as_deref(), Some("https://cdn.example/p.jpg"));
        assert_eq!(card.image, None);
        assert_eq!(card.footer.as_deref(), Some("Podcast"));
        assert_eq!(card.author.as_deref(), Some("The *Reef* Times"));
        assert_eq!(card.description.as_deref(), Some("Line one line two"));
        assert_eq!(card.fields[0].value, "1:02:03");
        assert!(card.fields[1].value.contains("(https://cdn.example/e.mp3)"));
        let embed = card.to_embed(Kind::Rss);
        assert_eq!(embed.color, Some(0xf26522));
        assert!(embed.timestamp.is_some());
    }

    #[test]
    fn bad_links_are_dropped_and_long_text_cut() {
        let mut odd = item();
        odd.url = Some("javascript:alert(1)".into());
        odd.image = Some("https://cdn.example/a b.jpg".into());
        odd.title = "x".repeat(5000);
        let message = render(Lang::En, &draft(true, Some("{title} {url}"), None), &odd);
        assert!(!message.content.contains("javascript"));
        assert!(message.content.chars().count() <= 2000);
        let card = message.embed.unwrap();
        assert_eq!((card.url, card.thumbnail), (None, None));
        assert!(card.title.unwrap().chars().count() <= 256);
    }

    #[test]
    fn enclosure_cannot_create_a_masked_link() {
        let mut hostile = item();
        hostile.audio = Some("https://cdn.example/a)[Official](https://evil.example".into());
        hostile.url = Some("https://cdn.example/a)[Official](https://evil.example".into());
        let rendered = render(Lang::En, &draft(true, Some("{url}"), None), &hostile);
        let card = rendered.embed.unwrap();
        assert!(!card.fields[1].value.contains(")[Official]("));
        assert!(!rendered.content.contains(")[Official]("));
        assert!(card.fields[1].value.contains("%29%5BOfficial%5D%28"));
        assert_eq!(web_link("https://user:secret@example.com/a"), None);
    }

    #[test]
    fn defaults_follow_the_server_language() {
        let mut video = item();
        video.audio = None;
        video.duration_secs = None;
        let youtube = Draft {
            kind: Kind::YouTube,
            source: "UCsXVk37bltHxD1rDPwtNM8Q",
            display_name: Some("Kurzgesagt"),
            message: None,
            embed: false,
            ping_role: None,
        };
        let en = render(Lang::En, &youtube, &video);
        let tr = render(Lang::Tr, &youtube, &video);
        assert_ne!(en.content, tr.content);
        assert!(en.content.contains("Kurzgesagt") && tr.content.contains("Kurzgesagt"));
        assert!(en.content.contains("https://example.com/a"));
        assert_eq!(clock(59), "0:59");
        assert_eq!(sample_item(Lang::Tr, Kind::Reddit, Utc::now()).id, "sample");
    }
}
