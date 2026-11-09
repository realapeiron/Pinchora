use super::*;

// /Transcripts

/// One message of a transcript. Short keys: a long ticket holds thousands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    /// The author, as an index into the transcript's authors.
    pub a: u16,
    pub t: DateTime<Utc>,
    /// The text; `None` when the bot could not read message text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c: Option<String>,
    /// Names of the attached files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub f: Vec<String>,
    /// How many embeds the message carried.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub e: u16,
}

fn is_zero(n: &u16) -> bool {
    *n == 0
}

/// Somebody who wrote in a ticket. `name` is `None` once they deleted their data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub idx: u16,
    pub name: Option<String>,
    pub bot: bool,
}

/// When the first answer came: the first message after opening by a human who is not the opener.
pub fn first_reply(lines: &[Line], authors: &[Author], opener: Option<u16>) -> Option<DateTime<Utc>> {
    lines
        .iter()
        .find(|line| Some(line.a) != opener && authors.iter().any(|a| a.idx == line.a && !a.bot && a.name.is_some()))
        .map(|line| line.t)
}

/// What the transcript page says about the ticket at the top. Every text is shown as written, escaped.
#[derive(Debug, Clone, Copy)]
pub struct TranscriptHead<'a> {
    pub server: &'a str,
    pub number: i32,
    pub topic: &'a str,
    pub opener: Option<&'a str>,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub closed_by: Option<&'a str>,
    pub reason: Option<&'a str>,
    /// Whether message text was kept; without it the page says why only authors and times are shown.
    pub with_content: bool,
}

/// `text` safe to place in HTML text or a quoted attribute.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

fn stamp(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%d %H:%M UTC").to_string()
}

const PAGE_STYLE: &str = "body{margin:0;background:#0f1631;color:#fff1e8;font:15px/1.5 system-ui,sans-serif}\
main{max-width:860px;margin:0 auto;padding:24px 16px}h1{font-size:20px;margin:0 0 4px;color:#ffd166}\
.facts{color:#8fa0d8;margin:0 0 16px}.facts span{margin-right:14px}.note{border:2px solid #2e3d75;\
padding:8px 12px;color:#8fa0d8;margin-bottom:16px}.msg{display:grid;grid-template-columns:150px 1fr;gap:12px;\
padding:8px 0;border-top:1px solid #2e3d75}.who{font-weight:700;overflow-wrap:anywhere}.who.bot{color:#06d6a0}\
.who.gone{color:#8fa0d8;font-style:italic}.when{display:block;font-weight:400;color:#8fa0d8;font-size:12px}\
.text{white-space:pre-wrap;overflow-wrap:anywhere}.files{color:#ffd166;font-size:13px}.empty{color:#8fa0d8}\
@media(max-width:560px){.msg{grid-template-columns:1fr;gap:2px}}";

/// The transcript as one self-contained web page (no scripts, no outside requests): the ticket at the top,
/// then every message with its author and time. All text a member wrote is escaped, so nothing in it can
/// become markup.
pub const TRANSCRIPT_HTML_MAX: usize = 7 * 1024 * 1024;

pub fn render_html(head: &TranscriptHead<'_>, lines: &[Line], authors: &[Author], lang: Lang) -> String {
    render_html_with_mentions(head, lines, authors, &HashMap::new(), lang)
}

/// Resolves raw Discord mention ids only while rendering. The stored line never contains a name.
pub fn render_mentions(text: &str, names: &HashMap<u64, String>, lang: Lang) -> String {
    let deleted = t!(lang, "tickets.transcript.deleted-user");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<@") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let id_start = after.strip_prefix('!').unwrap_or(after);
        if let Some(end) = id_start.find('>')
            && let Some(id) = id_start[..end]
                .parse::<u64>()
                .ok()
                .filter(|_| id_start[..end].bytes().all(|b| b.is_ascii_digit()))
        {
            out.push('@');
            out.push_str(names.get(&id).map(String::as_str).unwrap_or(&deleted));
            rest = &id_start[end + 1..];
        } else {
            out.push_str("<@");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

pub fn render_html_with_mentions(
    head: &TranscriptHead<'_>,
    lines: &[Line],
    authors: &[Author],
    mentions: &HashMap<u64, String>,
    lang: Lang,
) -> String {
    let full = render_transcript_document(head, lines, authors, mentions, lang, 0);
    if full.len() <= TRANSCRIPT_HTML_MAX {
        return full;
    }
    let mut low = 1;
    let mut high = lines.len();
    while low < high {
        let cut = low + (high - low) / 2;
        if render_transcript_document(head, &lines[cut..], authors, mentions, lang, cut).len() <= TRANSCRIPT_HTML_MAX {
            high = cut;
        } else {
            low = cut + 1;
        }
    }
    let html = render_transcript_document(head, &lines[low..], authors, mentions, lang, low);
    if html.len() <= TRANSCRIPT_HTML_MAX {
        html
    } else {
        render_transcript_document(head, &[], authors, mentions, lang, lines.len())
    }
}

fn render_transcript_document(
    head: &TranscriptHead<'_>,
    lines: &[Line],
    authors: &[Author],
    mentions: &HashMap<u64, String>,
    lang: Lang,
    cut: usize,
) -> String {
    let title = t!(lang, "tickets.transcript.title", number = format!("{:04}", head.number));
    let mut html = String::with_capacity(2_048 + lines.len() * 160);
    html.push_str("<!doctype html><html lang=\"");
    html.push_str(lang.code());
    html.push_str(
        "\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">",
    );
    html.push_str("<meta name=\"robots\" content=\"noindex\"><title>");
    html.push_str(&escape_html(&format!("{title} · {}", head.server)));
    html.push_str("</title><style>");
    html.push_str(PAGE_STYLE);
    html.push_str("</style></head><body><main><h1>");
    html.push_str(&escape_html(&title));
    html.push_str("</h1><p class=\"facts\">");
    let mut fact = |text: String| {
        html.push_str("<span>");
        html.push_str(&escape_html(&text));
        html.push_str("</span>");
    };
    fact(head.server.to_owned());
    if !head.topic.is_empty() {
        fact(t!(lang, "tickets.transcript.topic", topic = head.topic));
    }
    let deleted = t!(lang, "tickets.transcript.deleted-user");
    fact(t!(
        lang,
        "tickets.transcript.opened",
        who = head.opener.unwrap_or(&deleted),
        at = stamp(head.opened_at)
    ));
    if let Some(at) = head.closed_at {
        fact(t!(
            lang,
            "tickets.transcript.closed",
            who = head.closed_by.unwrap_or(&deleted),
            at = stamp(at)
        ));
    }
    if let Some(reason) = head.reason.filter(|r| !r.is_empty()) {
        fact(t!(lang, "tickets.transcript.reason", reason = reason));
    }
    html.push_str("</p>");
    if cut > 0 {
        html.push_str("<p class=\"note\">");
        html.push_str(&escape_html(&t!(lang, "tickets.transcript.cut", count = cut)));
        html.push_str("</p>");
    }
    if !head.with_content {
        html.push_str("<p class=\"note\">");
        html.push_str(&escape_html(&t!(lang, "tickets.transcript.no-content")));
        html.push_str("</p>");
    }
    if lines.is_empty() {
        html.push_str("<p class=\"empty\">");
        html.push_str(&escape_html(&t!(lang, "tickets.transcript.empty")));
        html.push_str("</p>");
    }
    for line in lines {
        render_transcript_line(&mut html, line, authors, mentions, lang, &deleted);
    }
    html.push_str("</main></body></html>\n");
    html
}

fn render_transcript_line(
    html: &mut String,
    line: &Line,
    authors: &[Author],
    mentions: &HashMap<u64, String>,
    lang: Lang,
    deleted: &str,
) {
    let author = authors.iter().find(|a| a.idx == line.a);
    let (name, class) = match author.and_then(|a| a.name.as_deref().map(|n| (n, a.bot))) {
        Some((name, true)) => (name.to_owned(), "who bot"),
        Some((name, false)) => (name.to_owned(), "who"),
        None => (deleted.to_owned(), "who gone"),
    };
    html.push_str("<div class=\"msg\"><div class=\"");
    html.push_str(class);
    html.push_str("\">");
    html.push_str(&escape_html(&name));
    html.push_str("<time class=\"when\" datetime=\"");
    html.push_str(&line.t.to_rfc3339());
    html.push_str("\">");
    html.push_str(&stamp(line.t));
    html.push_str("</time></div><div>");
    if let Some(text) = line.c.as_deref().filter(|c| !c.is_empty()) {
        html.push_str("<div class=\"text\">");
        html.push_str(&escape_html(&render_mentions(text, mentions, lang)));
        html.push_str("</div>");
    }
    if !line.f.is_empty() {
        html.push_str("<div class=\"files\">");
        let names: Vec<String> = line.f.iter().map(|f| escape_html(f)).collect();
        html.push_str(&escape_html(&t!(
            lang,
            "tickets.transcript.files",
            count = line.f.len()
        )));
        html.push(' ');
        html.push_str(&names.join(", "));
        html.push_str("</div>");
    }
    if line.e > 0 {
        html.push_str("<div class=\"files\">");
        html.push_str(&escape_html(&t!(lang, "tickets.transcript.embeds", count = line.e)));
        html.push_str("</div>");
    }
    html.push_str("</div></div>");
}
