//! Tickets: what a panel and its form may hold, the life of a ticket (open, claimed, closed, reopened)
//! and who may move it on, the name its channel gets, and its transcript as a web page. Pure: the bot,
//! `/tickets` and the dashboard all decide with these rules.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::admin::settings::FieldError;
use crate::infra::i18n::Lang;
use crate::t;

// /Limits

/// Panels one server may have.
pub const MAX_PANELS: usize = 10;
/// Topics a panel of buttons shows (one action row).
pub const MAX_BUTTONS: usize = 5;
/// Topics a panel with a menu offers (Discord's options per menu).
pub const MAX_OPTIONS: usize = 25;
/// Questions one form asks across up to four five-input Discord modals.
pub const MAX_QUESTIONS: usize = 20;
pub const PANEL_NAME_MAX: usize = 32;
pub const TITLE_MAX: usize = 100;
pub const DESCRIPTION_MAX: usize = 2_000;
/// A button's or a menu option's label.
pub const LABEL_MAX: usize = 80;
pub const OPTION_DESCRIPTION_MAX: usize = 100;
pub const WELCOME_MAX: usize = 1_500;
/// A question's label (Discord's limit for a text input's label).
pub const QUESTION_MAX: usize = 45;
pub const PLACEHOLDER_MAX: usize = 100;
/// The longest answer a form takes.
pub const ANSWER_MAX: u16 = 1_000;
pub const REASON_MAX: usize = 200;
pub const NAMING_MAX: usize = 60;
pub const AUTO_CLOSE_MAX: i32 = 720;
pub const OPEN_LIMIT_MAX: i32 = 10;
pub const MAX_SUPPORT_ROLES: usize = 10;
/// Channels a server may have before `auto` mode opens private threads instead (Discord stops at 500).
pub const THREAD_THRESHOLD: usize = 400;
/// How long a member waits between two tickets they open.
pub const OPEN_COOLDOWN_SECS: u64 = 60;
/// Most messages one transcript keeps, newest first when a ticket ran longer.
pub const TRANSCRIPT_MAX: usize = 2_000;
pub const DEFAULT_NAMING: &str = "ticket-{number}";
pub const DEFAULT_COLOR: u32 = 0xff6b4a;
/// The placeholders a channel name template understands.
pub const NAMING_PLACEHOLDERS: [&str; 3] = ["{number}", "{user}", "{topic}"];
/// The placeholders the welcome text understands.
pub const WELCOME_PLACEHOLDERS: [&str; 4] = ["{user}", "{server}", "{number}", "{topic}"];

// /Panels

/// How a panel offers its topics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelStyle {
    #[default]
    Buttons,
    Select,
}

impl PanelStyle {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Buttons => "buttons",
            Self::Select => "select",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        [Self::Buttons, Self::Select].into_iter().find(|s| s.key() == key)
    }

    /// How many topics the style can show.
    pub const fn max_topics(self) -> usize {
        match self {
            Self::Buttons => MAX_BUTTONS,
            Self::Select => MAX_OPTIONS,
        }
    }
}

/// A button's colour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    #[default]
    Primary,
    Secondary,
    Success,
    Danger,
}

/// One way into a ticket: a button, or an option of the panel's menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topic {
    pub label: String,
    /// One emoji drawn before the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    /// The line under the option in a menu; buttons do not show it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub tone: Tone,
}

impl Topic {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            emoji: None,
            description: None,
            tone: Tone::Primary,
        }
    }
}

/// One question of a form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub label: String,
    #[serde(default)]
    pub placeholder: String,
    /// A paragraph box rather than one line.
    #[serde(default)]
    pub long: bool,
    #[serde(default = "yes")]
    pub required: bool,
    /// A modal select replaces the text box when choices are present.
    #[serde(default)]
    pub choices: Vec<String>,
    /// A modal file upload replaces the text box.
    #[serde(default)]
    pub upload: bool,
}

fn yes() -> bool {
    true
}

/// A question and what the member answered, kept on the ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub question: String,
    pub answer: String,
}

/// Everything a panel says and offers, as a manager wrote it.
#[derive(Debug, Clone, Copy)]
pub struct PanelDraft<'a> {
    pub name: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub style: PanelStyle,
    pub topics: &'a [Topic],
    pub questions: &'a [Question],
    pub welcome: &'a str,
    pub support_roles: usize,
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

/// Whether `text` is one emoji (or a short run of the characters that make one, like a flag), and not a
/// word, a custom emoji's code or a mention.
pub fn is_emoji(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && text.len() <= 32
        && text.chars().count() <= 8
        && !text
            .chars()
            .any(|c| c.is_ascii_alphanumeric() || c.is_whitespace() || "<>:@#*_~`|".contains(c))
}

/// Why a panel cannot be saved, every problem at once; empty when it can.
pub fn check_panel(draft: &PanelDraft<'_>, lang: Lang) -> Vec<FieldError> {
    let mut problems = Vec::new();
    let name = draft.name.trim();
    if name.is_empty() {
        problems.push(FieldError::new("name", t!(lang, "tickets.check.name-empty")));
    } else if chars(name) > PANEL_NAME_MAX {
        problems.push(FieldError::new(
            "name",
            t!(lang, "tickets.check.name-long", max = PANEL_NAME_MAX),
        ));
    } else if name.parse::<i64>().is_ok() {
        problems.push(FieldError::new("name", t!(lang, "tickets.check.name-number")));
    }
    if draft.title.trim().is_empty() {
        problems.push(FieldError::new("title", t!(lang, "tickets.check.title-empty")));
    } else if chars(draft.title) > TITLE_MAX {
        problems.push(FieldError::new(
            "title",
            t!(lang, "tickets.check.title-long", max = TITLE_MAX),
        ));
    }
    if chars(draft.description) > DESCRIPTION_MAX {
        problems.push(FieldError::new(
            "description",
            t!(lang, "tickets.check.description-long", max = DESCRIPTION_MAX),
        ));
    }
    if chars(draft.welcome) > WELCOME_MAX {
        problems.push(FieldError::new(
            "welcome_text",
            t!(lang, "tickets.check.welcome-long", max = WELCOME_MAX),
        ));
    }
    if draft.support_roles > MAX_SUPPORT_ROLES {
        problems.push(FieldError::new(
            "support_roles",
            t!(lang, "tickets.check.roles-many", max = MAX_SUPPORT_ROLES),
        ));
    }
    let most = draft.style.max_topics();
    if draft.topics.is_empty() {
        problems.push(FieldError::new("buttons", t!(lang, "tickets.check.topics-none")));
    } else if draft.topics.len() > most {
        problems.push(FieldError::new(
            "buttons",
            t!(lang, "tickets.check.topics-many", max = most),
        ));
    }
    let mut seen: Vec<String> = Vec::new();
    for (n, topic) in draft.topics.iter().enumerate() {
        let number = n + 1;
        let label = topic.label.trim();
        if label.is_empty() {
            problems.push(FieldError::new(
                "buttons",
                t!(lang, "tickets.check.label-empty", number = number),
            ));
        } else if chars(label) > LABEL_MAX {
            problems.push(FieldError::new(
                "buttons",
                t!(lang, "tickets.check.label-long", number = number, max = LABEL_MAX),
            ));
        }
        let folded = label.to_lowercase();
        if !label.is_empty() && seen.contains(&folded) {
            problems.push(FieldError::new(
                "buttons",
                t!(lang, "tickets.check.label-twice", label = label),
            ));
        }
        seen.push(folded);
        if topic.emoji.as_deref().is_some_and(|e| !is_emoji(e)) {
            problems.push(FieldError::new(
                "buttons",
                t!(lang, "tickets.check.emoji", number = number),
            ));
        }
        if topic
            .description
            .as_deref()
            .is_some_and(|d| chars(d) > OPTION_DESCRIPTION_MAX)
        {
            problems.push(FieldError::new(
                "buttons",
                t!(
                    lang,
                    "tickets.check.option-long",
                    number = number,
                    max = OPTION_DESCRIPTION_MAX
                ),
            ));
        }
    }
    problems.extend(check_questions(draft.questions, lang));
    problems
}

/// Why a form cannot be saved; empty when it can. No questions at all is no form.
pub fn check_questions(questions: &[Question], lang: Lang) -> Vec<FieldError> {
    let mut problems = Vec::new();
    if questions.len() > MAX_QUESTIONS {
        problems.push(FieldError::new(
            "questions",
            t!(lang, "tickets.check.questions-many", max = MAX_QUESTIONS),
        ));
    }
    for (n, question) in questions.iter().enumerate() {
        let number = n + 1;
        let label = question.label.trim();
        if label.is_empty() {
            problems.push(FieldError::new(
                "questions",
                t!(lang, "tickets.check.question-empty", number = number),
            ));
        } else if chars(label) > QUESTION_MAX {
            problems.push(FieldError::new(
                "questions",
                t!(lang, "tickets.check.question-long", number = number, max = QUESTION_MAX),
            ));
        }
        if chars(&question.placeholder) > PLACEHOLDER_MAX {
            problems.push(FieldError::new(
                "questions",
                t!(
                    lang,
                    "tickets.check.placeholder-long",
                    number = number,
                    max = PLACEHOLDER_MAX
                ),
            ));
        }
        if (question.upload && !question.choices.is_empty())
            || question.choices.len() > 25
            || question
                .choices
                .iter()
                .any(|choice| choice.trim().is_empty() || chars(choice) > 100)
        {
            problems.push(FieldError::new(
                "questions",
                t!(lang, "tickets.check.question-kind", number = number),
            ));
        }
    }
    problems
}

/// Topics typed as one line, `Support, Billing, Report a member`: each becomes a button or option.
pub fn topics_from_list(list: &str) -> Vec<Topic> {
    list.split([',', '|', '\n'])
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(Topic::new)
        .collect()
}

// /Settings

/// Where a server's tickets live.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Channels until the server has more than [`THREAD_THRESHOLD`] of them, then private threads.
    #[default]
    Auto,
    Channel,
    Thread,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Channel, Self::Thread];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Channel => "channel",
            Self::Thread => "thread",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }

    /// Whether a ticket opened now is a thread, in a server with `channels` channels.
    pub fn threads(self, channels: usize) -> bool {
        match self {
            Self::Auto => channels > THREAD_THRESHOLD,
            Self::Channel => false,
            Self::Thread => true,
        }
    }
}

/// A channel name template: trimmed, at most [`NAMING_MAX`] characters, and one that gives a name.
pub fn check_naming(template: &str, lang: Lang) -> Result<String, FieldError> {
    let template = template.trim();
    if template.is_empty() || channel_name(template, 1, "x", "x").is_empty() {
        return Err(FieldError::new("naming", t!(lang, "tickets.check.naming-empty")));
    }
    if chars(template) > NAMING_MAX {
        return Err(FieldError::new(
            "naming",
            t!(lang, "tickets.check.naming-long", max = NAMING_MAX),
        ));
    }
    Ok(template.to_owned())
}

pub fn check_auto_close(hours: i64, lang: Lang) -> Result<i32, FieldError> {
    i32::try_from(hours)
        .ok()
        .filter(|h| (0..=AUTO_CLOSE_MAX).contains(h))
        .ok_or_else(|| {
            FieldError::new(
                "auto_close_hours",
                t!(lang, "tickets.check.auto-close", max = AUTO_CLOSE_MAX),
            )
        })
}

pub fn check_open_limit(limit: i64, lang: Lang) -> Result<i32, FieldError> {
    i32::try_from(limit)
        .ok()
        .filter(|n| (1..=OPEN_LIMIT_MAX).contains(n))
        .ok_or_else(|| {
            FieldError::new(
                "max_open_per_user",
                t!(lang, "tickets.check.open-limit", max = OPEN_LIMIT_MAX),
            )
        })
}

/// The name of ticket `number`'s channel or thread from the server's template: `{number}` (four digits),
/// `{user}` and `{topic}` filled in, then made a valid Discord channel name: lower case, dashes for spaces,
/// nothing but letters, digits, `-` and `_`, at most 100 characters. Empty only when nothing is left.
pub fn channel_name(template: &str, number: i32, user: &str, topic: &str) -> String {
    let filled = template
        .replace("{number}", &format!("{number:04}"))
        .replace("{user}", user)
        .replace("{topic}", topic);
    let mut name = String::new();
    for c in filled.chars().flat_map(char::to_lowercase) {
        let c = if c.is_whitespace() { '-' } else { c };
        if !(c.is_alphanumeric() || c == '-' || c == '_') {
            continue;
        }
        if c == '-' && (name.is_empty() || name.ends_with('-')) {
            continue;
        }
        name.push(c);
    }
    let name: String = name.trim_end_matches('-').chars().take(100).collect();
    name.trim_end_matches('-').to_owned()
}

/// The welcome text with its placeholders filled; the values must already be safe to post.
pub fn fill_welcome(template: &str, user: &str, server: &str, number: i32, topic: &str) -> String {
    template
        .replace("{user}", user)
        .replace("{server}", server)
        .replace("{number}", &number.to_string())
        .replace("{topic}", topic)
}

// /Life of a ticket

/// Where a ticket stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Open,
    /// Open, and a staff member said they handle it.
    Claimed,
    Closed,
}

impl State {
    pub fn of(closed: bool, claimed: bool) -> Self {
        match (closed, claimed) {
            (true, _) => Self::Closed,
            (false, true) => Self::Claimed,
            (false, false) => Self::Open,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Claimed => "claimed",
            Self::Closed => "closed",
        }
    }
}

/// What somebody does to a ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Claim,
    Unclaim,
    Close,
    Reopen,
    Lock,
    Unlock,
    AddMember,
    RemoveMember,
    Rename,
    Move,
    Transcript,
    Delete,
}

/// Why an action does not apply to the ticket as it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    AlreadyClaimed,
    NotClaimed,
    AlreadyClosed,
    NotClosed,
    /// Only staff (a support role, Manage Channels or Manage Server) may do that.
    StaffOnly,
}

impl Refusal {
    pub fn text(self, lang: Lang) -> String {
        let key = match self {
            Self::AlreadyClaimed => "tickets.refuse.already-claimed",
            Self::NotClaimed => "tickets.refuse.not-claimed",
            Self::AlreadyClosed => "tickets.refuse.already-closed",
            Self::NotClosed => "tickets.refuse.not-closed",
            Self::StaffOnly => "tickets.refuse.staff-only",
        };
        t!(lang, key)
    }
}

/// The state `action` leaves a ticket in, or why it does not apply. A reopened ticket goes back to the
/// queue unclaimed; the actions that change no state only need the ticket to be open (or, for a
/// transcript, nothing at all; deleting needs it closed first).
pub fn transition(state: State, action: Action) -> Result<State, Refusal> {
    use {Action as A, State as S};
    match (state, action) {
        (S::Open, A::Claim) => Ok(S::Claimed),
        (S::Claimed, A::Claim) => Err(Refusal::AlreadyClaimed),
        (S::Claimed, A::Unclaim) => Ok(S::Open),
        (S::Open, A::Unclaim) => Err(Refusal::NotClaimed),
        (S::Open | S::Claimed, A::Close) => Ok(S::Closed),
        (S::Closed, A::Reopen) => Ok(S::Open),
        (S::Open | S::Claimed, A::Reopen) => Err(Refusal::NotClosed),
        (S::Closed, A::Delete | A::Transcript) => Ok(S::Closed),
        (S::Open | S::Claimed, A::Delete) => Err(Refusal::NotClosed),
        (S::Closed, _) => Err(Refusal::AlreadyClosed),
        (open, _) => Ok(open),
    }
}

/// Whether the member may do `action`: staff may do anything, the member who opened the ticket may close
/// it and read its transcript.
pub fn may(action: Action, staff: bool, opener: bool) -> Result<(), Refusal> {
    if staff || (opener && matches!(action, Action::Close | Action::Transcript)) {
        Ok(())
    } else {
        Err(Refusal::StaffOnly)
    }
}

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
    let full = render_html_inner(head, lines, authors, mentions, lang, 0);
    if full.len() <= TRANSCRIPT_HTML_MAX {
        return full;
    }
    let mut low = 1;
    let mut high = lines.len();
    while low < high {
        let cut = low + (high - low) / 2;
        if render_html_inner(head, &lines[cut..], authors, mentions, lang, cut).len() <= TRANSCRIPT_HTML_MAX {
            high = cut;
        } else {
            low = cut + 1;
        }
    }
    let html = render_html_inner(head, &lines[low..], authors, mentions, lang, low);
    if html.len() <= TRANSCRIPT_HTML_MAX {
        html
    } else {
        render_html_inner(head, &[], authors, mentions, lang, lines.len())
    }
}

fn render_html_inner(
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
        let author = authors.iter().find(|a| a.idx == line.a);
        let (name, class) = match author.and_then(|a| a.name.as_deref().map(|n| (n, a.bot))) {
            Some((name, true)) => (name.to_owned(), "who bot"),
            Some((name, false)) => (name.to_owned(), "who"),
            None => (deleted.clone(), "who gone"),
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
    html.push_str("</main></body></html>\n");
    html
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::community::tickets::*;

    #[test]
    fn transcript_html_keeps_newest_messages_under_cap() {
        let head = TranscriptHead {
            server: "Reef",
            number: 1,
            topic: "Help",
            opener: None,
            opened_at: at(0),
            closed_at: None,
            closed_by: None,
            reason: None,
            with_content: true,
        };
        let lines: Vec<Line> = (0..20)
            .map(|n| Line {
                a: 0,
                t: at(n),
                c: Some(format!("MARKER-{n} {}", "x".repeat(500_000))),
                f: vec![],
                e: 0,
            })
            .collect();
        let html = render_html(&head, &lines, &[], Lang::En);
        assert!(html.len() <= TRANSCRIPT_HTML_MAX);
        assert!(html.contains("MARKER-19"));
        assert!(!html.contains("MARKER-0"));
        assert!(html.contains("older messages were omitted"));
    }

    #[test]
    fn mention_names_are_resolved_only_when_rendered() {
        let raw = "Hello <@42> and <@!99>.";
        let names = HashMap::from([(42, "Sam".to_owned())]);
        assert_eq!(render_mentions(raw, &names, Lang::En), "Hello @Sam and @Deleted user.");
        assert_eq!(
            render_mentions(raw, &HashMap::new(), Lang::Tr),
            "Hello @Silinmiş kullanıcı and @Silinmiş kullanıcı."
        );
        assert_eq!(raw, "Hello <@42> and <@!99>.");
    }

    fn draft<'a>(topics: &'a [Topic], questions: &'a [Question]) -> PanelDraft<'a> {
        PanelDraft {
            name: "Support",
            title: "Need a hand?",
            description: "Press a button and we answer in private.",
            style: PanelStyle::Buttons,
            topics,
            questions,
            welcome: "Hi {user}, tell us more.",
            support_roles: 1,
        }
    }

    #[test]
    fn a_good_panel_passes_and_every_problem_is_named() {
        let topics = [Topic::new("Support"), Topic::new("Billing")];
        assert!(check_panel(&draft(&topics, &[]), Lang::En).is_empty());

        let six: Vec<Topic> = (0..6).map(|n| Topic::new(&format!("Topic {n}"))).collect();
        let errors = check_panel(&draft(&six, &[]), Lang::En);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].field, "buttons");
        let menu = PanelDraft {
            style: PanelStyle::Select,
            ..draft(&six, &[])
        };
        assert!(check_panel(&menu, Lang::En).is_empty(), "a menu takes 25");

        let twice = [Topic::new("Help"), Topic::new("help")];
        let errors = check_panel(&draft(&twice, &[]), Lang::En);
        assert!(errors[0].message.to_lowercase().contains("help"), "{errors:?}");

        let bad = PanelDraft {
            name: "",
            title: " ",
            ..draft(&[], &[])
        };
        let fields: Vec<&str> = check_panel(&bad, Lang::Tr).iter().map(|e| e.field).collect();
        assert_eq!(fields, ["name", "title", "buttons"]);
        let number = PanelDraft {
            name: "12",
            ..draft(&topics, &[])
        };
        assert_eq!(
            check_panel(&number, Lang::En)[0].field,
            "name",
            "a number would read as an id"
        );
    }

    #[test]
    fn emoji_are_emoji_and_nothing_else() {
        for good in ["🎫", "❤️", "🇹🇷", "👩‍💻"] {
            assert!(is_emoji(good), "{good}");
        }
        for bad in ["", "ticket", "<:pin:123>", "@here", "a🎫", "🎫 🎫"] {
            assert!(!is_emoji(bad), "{bad}");
        }
        let mut topic = Topic::new("Help");
        topic.emoji = Some("help".into());
        let errors = check_panel(&draft(&[topic], &[]), Lang::En);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn forms_ask_up_to_twenty_questions_across_steps() {
        let q = |label: &str| Question {
            label: label.into(),
            placeholder: String::new(),
            long: false,
            required: true,
            choices: Vec::new(),
            upload: false,
        };
        assert!(check_questions(&[q("Your order number")], Lang::En).is_empty());
        let six: Vec<Question> = (0..6).map(|n| q(&format!("Q{n}"))).collect();
        assert!(check_questions(&six, Lang::En).is_empty());
        let twenty_one: Vec<Question> = (0..21).map(|n| q(&format!("Q{n}"))).collect();
        assert_eq!(check_questions(&twenty_one, Lang::En).len(), 1);
        assert_eq!(check_questions(&[q(""), q(&"x".repeat(46))], Lang::En).len(), 2);
        let json: Vec<Question> = serde_json::from_str("[{\"label\":\"Why?\"}]").expect("valid JSON");
        assert!(json[0].required && !json[0].long, "required unless said otherwise");
    }

    #[test]
    fn topics_come_from_one_line() {
        let topics = topics_from_list("Support, Billing |Report a member,,");
        let labels: Vec<&str> = topics.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(labels, ["Support", "Billing", "Report a member"]);
    }

    #[test]
    fn channel_names_are_valid_discord_names() {
        assert_eq!(channel_name(DEFAULT_NAMING, 42, "Sam", "Help"), "ticket-0042");
        assert_eq!(
            channel_name("{topic}-{user}", 7, "Mira Çelik", "Billing & Refunds"),
            "billing-refunds-mira-çelik"
        );
        assert_eq!(channel_name("  🎫 {user}!! ", 1, "a  b", ""), "a-b");
        assert_eq!(channel_name("{user}", 1, "@#$%", ""), "");
        assert_eq!(channel_name(&"x".repeat(150), 1, "", "").chars().count(), 100);
        assert!(check_naming("{user}", Lang::En).is_ok());
        assert!(check_naming("!!!", Lang::En).is_err());
        assert!(check_naming(&"a".repeat(61), Lang::En).is_err());
    }

    #[test]
    fn auto_mode_switches_to_threads_past_the_threshold() {
        assert!(!Mode::Auto.threads(THREAD_THRESHOLD));
        assert!(Mode::Auto.threads(THREAD_THRESHOLD + 1));
        assert!(Mode::Thread.threads(0));
        assert!(!Mode::Channel.threads(499));
        assert_eq!(Mode::parse("thread"), Some(Mode::Thread));
        assert!(check_auto_close(721, Lang::En).is_err() && check_auto_close(0, Lang::En).is_ok());
        assert!(check_open_limit(0, Lang::En).is_err() && check_open_limit(10, Lang::En).is_ok());
    }

    #[test]
    fn a_ticket_goes_open_claimed_closed_and_reopens_unclaimed() {
        use {Action as A, Refusal as R, State as S};
        let table = [
            (S::Open, A::Claim, Ok(S::Claimed)),
            (S::Claimed, A::Claim, Err(R::AlreadyClaimed)),
            (S::Claimed, A::Unclaim, Ok(S::Open)),
            (S::Open, A::Unclaim, Err(R::NotClaimed)),
            (S::Claimed, A::Close, Ok(S::Closed)),
            (S::Open, A::Close, Ok(S::Closed)),
            (S::Closed, A::Close, Err(R::AlreadyClosed)),
            (S::Closed, A::Claim, Err(R::AlreadyClosed)),
            (S::Closed, A::Lock, Err(R::AlreadyClosed)),
            (S::Closed, A::Reopen, Ok(S::Open)),
            (S::Open, A::Reopen, Err(R::NotClosed)),
            (S::Claimed, A::Lock, Ok(S::Claimed)),
            (S::Open, A::AddMember, Ok(S::Open)),
            (S::Closed, A::Transcript, Ok(S::Closed)),
            (S::Open, A::Transcript, Ok(S::Open)),
            (S::Open, A::Delete, Err(R::NotClosed)),
            (S::Closed, A::Delete, Ok(S::Closed)),
        ];
        for (state, action, wanted) in table {
            assert_eq!(transition(state, action), wanted, "{state:?} + {action:?}");
        }
        assert_eq!(State::of(false, true), S::Claimed);
        assert_eq!(State::of(true, true), S::Closed);
    }

    #[test]
    fn only_staff_run_a_ticket_but_its_opener_may_close_it() {
        assert_eq!(may(Action::Close, false, true), Ok(()));
        assert_eq!(may(Action::Transcript, false, true), Ok(()));
        assert_eq!(may(Action::Claim, false, true), Err(Refusal::StaffOnly));
        assert_eq!(may(Action::Close, false, false), Err(Refusal::StaffOnly));
        assert_eq!(may(Action::Delete, true, false), Ok(()));
        for refusal in [Refusal::AlreadyClaimed, Refusal::StaffOnly] {
            assert_ne!(refusal.text(Lang::Tr), refusal.text(Lang::En));
        }
    }

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 6, 18, minute, 0).unwrap()
    }

    #[test]
    fn the_first_reply_is_the_first_human_who_is_not_the_opener() {
        let authors = [
            Author {
                idx: 0,
                name: Some("Pinchora".into()),
                bot: true,
            },
            Author {
                idx: 1,
                name: Some("Sam".into()),
                bot: false,
            },
            Author {
                idx: 2,
                name: Some("Staff".into()),
                bot: false,
            },
        ];
        let line = |a: u16, minute: u32| Line {
            a,
            t: at(minute),
            c: Some("hi".into()),
            f: vec![],
            e: 0,
        };
        let lines = [line(0, 0), line(1, 1), line(2, 5), line(1, 6)];
        assert_eq!(first_reply(&lines, &authors, Some(1)), Some(at(5)));
        assert_eq!(first_reply(&lines[..2], &authors, Some(1)), None);
    }

    /// The transcript golden: hostile text stays text, deleted members read as such, nothing loads from
    /// anywhere else.
    #[test]
    fn transcripts_escape_everything_a_member_wrote() {
        let authors = [
            Author {
                idx: 0,
                name: Some("<b>Sam</b>".into()),
                bot: false,
            },
            Author {
                idx: 1,
                name: Some("Pinchora".into()),
                bot: true,
            },
        ];
        let lines = [
            Line {
                a: 0,
                t: at(1),
                c: Some("<script>alert('x')</script>\n<img src=x onerror=alert(1)> & \"quotes\"".into()),
                f: vec!["\"><svg onload=alert(1)>.png".into()],
                e: 0,
            },
            Line {
                a: 1,
                t: at(2),
                c: Some(String::new()),
                f: vec![],
                e: 2,
            },
            Line {
                a: 7,
                t: at(3),
                c: Some("gone".into()),
                f: vec![],
                e: 0,
            },
        ];
        let head = TranscriptHead {
            server: "Reef & <Friends>",
            number: 42,
            topic: "</title><script>",
            opener: Some("<b>Sam</b>"),
            opened_at: at(0),
            closed_at: Some(at(4)),
            closed_by: None,
            reason: Some("Solved <3"),
            with_content: true,
        };
        let html = render_html(&head, &lines, &authors, Lang::En);
        let golden = "<div class=\"msg\"><div class=\"who\">&lt;b&gt;Sam&lt;/b&gt;<time class=\"when\" \
                      datetime=\"2026-11-06T18:01:00+00:00\">2026-11-06 18:01 UTC</time></div><div><div class=\"text\">\
                      &lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;\n&lt;img src=x onerror=alert(1)&gt; &amp; \
                      &quot;quotes&quot;</div><div class=\"files\">1 file: &quot;&gt;&lt;svg onload=alert(1)&gt;.png\
                      </div></div></div>";
        assert!(html.contains(golden), "{html}");
        for hostile in ["<script", "<img", "<svg", "<b>", "</title><script>"] {
            assert!(!html.contains(hostile), "{hostile} got through");
        }
        assert!(html.contains("Reef &amp; &lt;Friends&gt;"));
        assert!(html.contains("2 embeds"));
        assert!(html.contains("who gone\">Deleted user"));
        assert!(html.contains("Closed by Deleted user"));
        assert!(!html.contains("http"), "the page loads nothing from anywhere");
        assert!(!html.contains("<script"));

        let quiet = TranscriptHead {
            with_content: false,
            ..head
        };
        let tr = render_html(&quiet, &[], &authors, Lang::Tr);
        assert!(tr.contains("lang=\"tr\"") && tr.contains("class=\"note\"") && tr.contains("class=\"empty\""));
    }

    #[test]
    fn the_welcome_text_fills_its_placeholders() {
        assert_eq!(
            fill_welcome(
                "Hi {user}, ticket {number} on {server} ({topic})",
                "<@1>",
                "Reef",
                7,
                "Billing"
            ),
            "Hi <@1>, ticket 7 on Reef (Billing)"
        );
    }
}
