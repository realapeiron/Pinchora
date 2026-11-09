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

#[cfg(test)]
mod tests;

mod transcripts;
pub use transcripts::{
    Author, Line, TRANSCRIPT_HTML_MAX, TranscriptHead, escape_html, first_reply, render_html,
    render_html_with_mentions, render_mentions,
};
