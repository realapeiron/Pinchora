use super::*;

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

pub(super) fn bluesky(body: &[u8]) -> Result<Parsed, ParseError> {
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
