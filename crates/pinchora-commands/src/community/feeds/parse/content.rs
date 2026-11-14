use super::*;

pub(super) fn text(t: &Text) -> String {
    if is_html(t.content_type.subty().as_ref()) {
        plain(&t.content)
    } else {
        one_line(&t.content)
    }
}

/// Whether a media subtype (`html` of `text/html`) is HTML.
pub(super) fn is_html(subtype: &str) -> bool {
    subtype == "html" || subtype == "xhtml+xml"
}

pub(super) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A YouTube video's picture, from its id (`yt:video:<id>`), for feeds that leave it out.
pub(super) fn youtube_thumbnail(kind: Kind, id: &str) -> Option<String> {
    let video = id.strip_prefix("yt:video:").filter(|_| kind == Kind::YouTube)?;
    let ok = !video.is_empty()
        && video
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ok.then(|| format!("https://i.ytimg.com/vi/{video}/hqdefault.jpg"))
}

/// The address of the first `<img src="…">` in some HTML.
pub(super) fn first_image(html: &str) -> Option<String> {
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
