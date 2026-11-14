use super::*;

/// A feed text as plain text: HTML only when the feed says it is HTML, so a title like “Coral <3” keeps its `<3`.
/// Each RSS item's `itunes:duration` (seconds) and `itunes:image`, in document order.
pub(super) fn podcast_extras(xml: &str) -> Vec<(Option<u64>, Option<String>)> {
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
pub(super) fn clock_secs(text: &str) -> Option<u64> {
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

/// Bound recursive work in feed-rs before it sees untrusted XML. This scan has no recursion
/// and ignores quoted `>` characters, comments, CDATA and processing instructions.
pub(super) fn xml_bounds(body: &[u8]) -> Result<(), ParseError> {
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

pub(super) fn entry(kind: Kind, e: &Entry) -> Option<Item> {
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
