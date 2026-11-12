//! Server settings rules shared by `/config` and the web dashboard: limits, defaults, what a valid
//! value is, and the sentence that says why one is not. Pure: no Discord, no database.

use std::ops::RangeInclusive;

use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;

use crate::infra::i18n;
use crate::infra::i18n::Lang;
use crate::progression::leveling::LevelUpMode;
use crate::progression::leveling::MAX_MULTIPLIER;
use crate::progression::leveling::VOICE_XP_RANGE;
use crate::t;

/// Welcome, goodbye and level-up messages, in characters.
pub const MESSAGE_MAX: usize = 500;
/// The welcome card's headline, in characters.
pub const WELCOME_TITLE_MAX: usize = 18;
pub const POINTS_NAME_MAX: usize = 32;
pub const POINTS_EMOJI_MAX: usize = 64;
/// Points one message can earn, at least and at most.
pub const POINTS_RANGE: RangeInclusive<i64> = 0..=1_000;
/// The level a level role is given at.
pub const LEVEL_RANGE: RangeInclusive<i64> = 1..=1_000;
/// The reputation a reputation role is given at.
pub const REPUTATION_RANGE: RangeInclusive<i64> = 1..=100_000;
/// A store role's price in server points.
pub const PRICE_RANGE: RangeInclusive<i64> = 1..=1_000_000_000;
/// How many of a store role can be sold; no stock is unlimited.
pub const STOCK_RANGE: RangeInclusive<i64> = 1..=1_000_000;

/// The welcome card themes by key, with the name the pickers show.
pub const WELCOME_THEMES: [(&str, &str); 5] = [
    ("reef", "Reef (navy & coral)"),
    ("sunset", "Sunset"),
    ("forest", "Forest"),
    ("candy", "Candy"),
    ("midnight", "Midnight"),
];
pub const DEFAULT_THEME: &str = "reef";
pub const DEFAULT_WELCOME_TITLE: &str = "Welcome";

pub const DEFAULT_LEVELUP: &str = "🎉 {user} reached **level {level}**!";
pub const DEFAULT_WELCOME: &str = "Welcome to **{server}**, {user}!";
pub const DEFAULT_GOODBYE: &str = "👋 **{user}** has left **{server}**.";

/// What text commands start with when a server picked none (`p!profile`).
pub const DEFAULT_PREFIX: &str = "p!";
/// A text command prefix, in characters.
pub const PREFIX_MAX: usize = 5;

/// Every way a level-up can be announced, in the order the pickers list them.
pub const LEVELUP_MODES: [LevelUpMode; 4] = [
    LevelUpMode::Channel,
    LevelUpMode::Current,
    LevelUpMode::Dm,
    LevelUpMode::Off,
];

/// What is wrong with one field, worded for the person who filled it in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldError {
    pub field: &'static str,
    pub message: String,
    /// The catalog key of this error's text and its arguments, for [`FieldError::text`]; empty for an
    /// error that only has the English `message`. Not part of the API.
    #[serde(skip)]
    key: &'static str,
    #[serde(skip)]
    args: Vec<(&'static str, String)>,
}

impl FieldError {
    pub fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
            key: "",
            args: Vec::new(),
        }
    }

    /// Says where the error's text lives in the catalogs (`settings-error.<key>`), with the values its
    /// `{arguments}` take. `message` stays the English text the dashboard shows.
    #[must_use]
    pub fn keyed<const N: usize>(mut self, key: &'static str, args: [(&'static str, String); N]) -> Self {
        self.key = key;
        self.args = args.into();
        self
    }

    /// The error worded in `lang`; English (and any error without a key) is `message`.
    pub fn text(&self, lang: Lang) -> String {
        if lang == Lang::En || self.key.is_empty() {
            return self.message.clone();
        }
        let values: Vec<(&str, String)> = self
            .args
            .iter()
            .map(|(key, value)| {
                if self.key == "whole-server" && *key == "module" {
                    let name = crate::admin::modules::Module::ALL
                        .into_iter()
                        .find(|module| module.key() == value)
                        .map_or_else(|| value.clone(), |module| module.name(lang));
                    (*key, name)
                } else {
                    (*key, value.clone())
                }
            })
            .collect();
        let args: Vec<(&str, &dyn std::fmt::Display)> = values
            .iter()
            .map(|(key, value)| (*key, value as &dyn std::fmt::Display))
            .collect();
        i18n::text(lang, &format!("settings-error.{}", self.key), &args)
    }

    /// The catalog key and arguments, for the test that keeps every English catalog text equal to
    /// `message`.
    #[cfg(test)]
    fn catalog_text(&self) -> Option<String> {
        if self.key.is_empty() {
            return None;
        }
        let values: Vec<(&str, String)> = self
            .args
            .iter()
            .map(|(key, value)| {
                if self.key == "whole-server" && *key == "module" {
                    let name = crate::admin::modules::Module::ALL
                        .into_iter()
                        .find(|module| module.key() == value)
                        .map_or_else(|| value.clone(), |module| module.label().to_owned());
                    (*key, name)
                } else {
                    (*key, value.clone())
                }
            })
            .collect();
        let args: Vec<(&str, &dyn std::fmt::Display)> = values
            .iter()
            .map(|(key, value)| (*key, value as &dyn std::fmt::Display))
            .collect();
        Some(i18n::text(Lang::En, &format!("settings-error.{}", self.key), &args))
    }
}

/// The messages a server writes itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Welcome,
    Goodbye,
    LevelUp,
}

impl MessageKind {
    pub const ALL: [Self; 3] = [Self::Welcome, Self::Goodbye, Self::LevelUp];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::Goodbye => "goodbye",
            Self::LevelUp => "levelup",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// The settings column the message is stored in.
    pub const fn field(self) -> &'static str {
        match self {
            Self::Welcome => "welcome_message",
            Self::Goodbye => "goodbye_message",
            Self::LevelUp => "levelup_message",
        }
    }

    /// The placeholders the bot fills in, as the command options describe them.
    pub const fn placeholders(self) -> &'static [&'static str] {
        match self {
            Self::Welcome => &["{user}", "{server}", "{count}"],
            Self::Goodbye => &["{user}", "{server}", "{members}"],
            Self::LevelUp => &["{user}", "{level}", "{server}", "{role}"],
        }
    }

    /// What the bot says when the server wrote nothing.
    pub const fn default_message(self) -> &'static str {
        match self {
            Self::Welcome => DEFAULT_WELCOME,
            Self::Goodbye => DEFAULT_GOODBYE,
            Self::LevelUp => DEFAULT_LEVELUP,
        }
    }
}

/// `#ff6b4a` (the `#` optional) as a colour.
pub fn parse_hex_color(s: &str) -> Option<u32> {
    let s = s.trim();
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

pub fn check_accent(s: &str) -> Result<u32, FieldError> {
    parse_hex_color(s)
        .ok_or_else(|| FieldError::new("welcome_accent", "Colours look like `#ff6b4a`.").keyed("accent", []))
}

/// A welcome, goodbye or level-up message: something to say, at most [`MESSAGE_MAX`] characters.
pub fn check_message(kind: MessageKind, s: &str) -> Result<(), FieldError> {
    let field = kind.field();
    if s.trim().is_empty() {
        return Err(
            FieldError::new(field, "Write a message, or reset it to use the default.").keyed("message-empty", []),
        );
    }
    let length = s.chars().count();
    if length > MESSAGE_MAX {
        return Err(FieldError::new(
            field,
            format!("A message is at most {MESSAGE_MAX} characters; this one has {length}."),
        )
        .keyed(
            "message-long",
            [("max", MESSAGE_MAX.to_string()), ("length", length.to_string())],
        ));
    }
    Ok(())
}

/// The welcome card headline, trimmed.
pub fn check_welcome_title(s: &str) -> Result<String, FieldError> {
    let title = s.trim();
    if title.is_empty() {
        return Err(FieldError::new("welcome_title", "Write a headline, e.g. WELCOME.").keyed("title-empty", []));
    }
    if title.chars().count() > WELCOME_TITLE_MAX {
        return Err(FieldError::new(
            "welcome_title",
            format!("A headline is at most {WELCOME_TITLE_MAX} characters."),
        )
        .keyed("title-long", [("max", WELCOME_TITLE_MAX.to_string())]));
    }
    Ok(title.to_owned())
}

pub fn check_theme(key: &str) -> Result<(), FieldError> {
    if WELCOME_THEMES.iter().any(|(k, _)| *k == key) {
        Ok(())
    } else {
        Err(FieldError::new(
            "welcome_theme",
            "Pick one of the card themes: reef, sunset, forest, candy or midnight.",
        )
        .keyed("theme", []))
    }
}

pub fn parse_levelup_mode(key: &str) -> Result<LevelUpMode, FieldError> {
    LEVELUP_MODES.into_iter().find(|m| m.key() == key).ok_or_else(|| {
        FieldError::new(
            "levelup_mode",
            "Level-ups go to the level-up channel, the current channel, a DM, or nowhere.",
        )
        .keyed("levelup-mode", [])
    })
}

/// A text command prefix, trimmed: 1 to [`PREFIX_MAX`] characters with no spaces, `@`, `#` or
/// backticks, so a prefix can never be (or start) a mention, a channel link or code.
pub fn check_prefix(s: &str) -> Result<String, FieldError> {
    let prefix = s.trim();
    let length = prefix.chars().count();
    if length == 0 || length > PREFIX_MAX {
        return Err(FieldError::new(
            "prefix",
            format!("A prefix is 1 to {PREFIX_MAX} characters, e.g. p! or ?."),
        ));
    }
    if prefix
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '@' | '#' | '`'))
    {
        return Err(FieldError::new(
            "prefix",
            "A prefix can't hold spaces, @, # or backticks.",
        ));
    }
    Ok(prefix.to_owned())
}

/// What the points are called, trimmed.
pub fn check_points_name(s: &str) -> Result<String, FieldError> {
    let name = s.trim();
    if name.is_empty() || name.chars().count() > POINTS_NAME_MAX {
        return Err(FieldError::new(
            "points_name",
            format!("Points need a name of 1 to {POINTS_NAME_MAX} characters, e.g. coins."),
        )
        .keyed("points-name", [("max", POINTS_NAME_MAX.to_string())]));
    }
    Ok(name.to_owned())
}

/// The emoji shown next to amounts: one Unicode emoji or a custom one (`<:name:id>`), trimmed.
pub fn check_points_emoji(s: &str) -> Result<String, FieldError> {
    let emoji = s.trim();
    let one = emoji.graphemes(true).count() == 1 || is_custom_emoji(emoji);
    if emoji.is_empty() || emoji.chars().count() > POINTS_EMOJI_MAX || !one {
        return Err(FieldError::new(
            "points_emoji",
            "Use one emoji: a Unicode one like 🪙, or a custom one like <:coin:123456789012345678>.",
        )
        .keyed("points-emoji", []));
    }
    Ok(emoji.to_owned())
}

/// `<:name:id>` or `<a:name:id>`, as Discord writes a custom emoji.
fn is_custom_emoji(s: &str) -> bool {
    let Some(inner) = s.strip_prefix('<').and_then(|s| s.strip_suffix('>')) else {
        return false;
    };
    let inner = inner.strip_prefix('a').unwrap_or(inner);
    let Some((name, id)) = inner.strip_prefix(':').and_then(|s| s.split_once(':')) else {
        return false;
    };
    (2..=32).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && (17..=20).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_digit())
}

/// Points per message: each in [`POINTS_RANGE`], the least not above the most.
pub fn check_points(min: i64, max: i64) -> Result<(), FieldError> {
    let (low, high) = (POINTS_RANGE.start(), POINTS_RANGE.end());
    for (field, value) in [("points_min", min), ("points_max", max)] {
        if !POINTS_RANGE.contains(&value) {
            return Err(
                FieldError::new(field, format!("Points per message go from {low} to {high}."))
                    .keyed("points-range", [("low", low.to_string()), ("high", high.to_string())]),
            );
        }
    }
    if min > max {
        return Err(
            FieldError::new("points_min", "The least points per message can't be above the most.")
                .keyed("points-order", []),
        );
    }
    Ok(())
}

/// An XP multiplier, rounded to hundredths: 0 (no XP) to [`MAX_MULTIPLIER`].
pub fn check_multiplier(m: f64) -> Result<f64, FieldError> {
    let value = (m * 100.0).round() / 100.0;
    if !value.is_finite() || !(0.0..=MAX_MULTIPLIER).contains(&value) {
        return Err(
            FieldError::new("multiplier", format!("A multiplier goes from 0 to {MAX_MULTIPLIER}."))
                .keyed("multiplier", [("max", MAX_MULTIPLIER.to_string())]),
        );
    }
    Ok(value)
}

/// Server XP a minute in voice earns.
pub fn check_voice_amount(n: i64) -> Result<i32, FieldError> {
    if !VOICE_XP_RANGE.contains(&n) {
        return Err(FieldError::new(
            "voice_xp_amount",
            format!(
                "Voice XP is {} to {} a minute.",
                VOICE_XP_RANGE.start(),
                VOICE_XP_RANGE.end()
            ),
        )
        .keyed(
            "voice-amount",
            [
                ("low", VOICE_XP_RANGE.start().to_string()),
                ("high", VOICE_XP_RANGE.end().to_string()),
            ],
        ));
    }
    Ok(n as i32)
}

fn in_range(field: &'static str, n: i64, range: &RangeInclusive<i64>, what: &str) -> Result<i64, FieldError> {
    if range.contains(&n) {
        Ok(n)
    } else {
        Err(
            FieldError::new(field, format!("{what} goes from {} to {}.", range.start(), range.end())).keyed(
                match field {
                    "level" => "level-range",
                    "reputation" => "reputation-range",
                    "price" => "price-range",
                    _ => "stock-range",
                },
                [("low", range.start().to_string()), ("high", range.end().to_string())],
            ),
        )
    }
}

pub fn check_level(n: i64) -> Result<i32, FieldError> {
    in_range("level", n, &LEVEL_RANGE, "A level role's level").map(|n| n as i32)
}

pub fn check_reputation(n: i64) -> Result<i32, FieldError> {
    in_range("reputation", n, &REPUTATION_RANGE, "A reputation role's count").map(|n| n as i32)
}

pub fn check_price(n: i64) -> Result<i64, FieldError> {
    in_range("price", n, &PRICE_RANGE, "A price")
}

pub fn check_stock(n: i64) -> Result<i32, FieldError> {
    in_range("stock", n, &STOCK_RANGE, "Stock").map(|n| n as i32)
}

/// A change to a server's settings row: `None` leaves a column alone, `Some(None)` clears it.
/// Built by the dashboard from its JSON; [`check_patch`] makes it safe to write.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuildSettingsPatch {
    pub leveling_enabled: Option<bool>,
    pub levelup_channel_id: Option<Option<u64>>,
    pub levelup_message: Option<Option<String>>,
    pub levelup_mode: Option<LevelUpMode>,
    pub level_roles_stack: Option<bool>,
    pub voice_xp_enabled: Option<bool>,
    pub voice_xp_amount: Option<i64>,
    pub welcome_enabled: Option<bool>,
    pub welcome_channel_id: Option<Option<u64>>,
    pub welcome_message: Option<Option<String>>,
    pub welcome_theme: Option<Option<String>>,
    pub welcome_title: Option<Option<String>>,
    pub welcome_accent: Option<Option<u32>>,
    pub join_role_id: Option<Option<u64>>,
    pub goodbye_channel_id: Option<Option<u64>>,
    pub goodbye_message: Option<Option<String>>,
    pub points_enabled: Option<bool>,
    pub points_name: Option<String>,
    pub points_emoji: Option<String>,
    pub points_min: Option<i64>,
    pub points_max: Option<i64>,
    pub log_channel_id: Option<Option<u64>>,
    pub log_members: Option<bool>,
    pub log_names: Option<bool>,
    pub log_bans: Option<bool>,
    pub log_voice: Option<bool>,
    /// The text command prefix; `Some(None)` goes back to [`DEFAULT_PREFIX`].
    pub prefix: Option<Option<String>>,
}

/// Checks every field of `patch` with the rules above and returns it cleaned (names and titles
/// trimmed), or every problem found. `points_now` is the stored `(min, max)`, so a patch that moves
/// only one end is checked against the other.
pub fn check_patch(patch: GuildSettingsPatch, points_now: (i64, i64)) -> Result<GuildSettingsPatch, Vec<FieldError>> {
    check_patch_inner(patch, Some(points_now))
}

/// Validate and normalize each requested value before a conditional write; the resulting points
/// pair is checked against the locked row by the database writer.
pub fn check_patch_fields(patch: GuildSettingsPatch) -> Result<GuildSettingsPatch, Vec<FieldError>> {
    check_patch_inner(patch, None)
}

fn check_patch_inner(
    mut patch: GuildSettingsPatch,
    points_now: Option<(i64, i64)>,
) -> Result<GuildSettingsPatch, Vec<FieldError>> {
    let mut errors = Vec::new();
    let mut note = |result: Result<(), FieldError>| {
        if let Err(error) = result {
            errors.push(error);
        }
    };
    for (kind, message) in [
        (MessageKind::LevelUp, &patch.levelup_message),
        (MessageKind::Welcome, &patch.welcome_message),
        (MessageKind::Goodbye, &patch.goodbye_message),
    ] {
        if let Some(Some(text)) = message {
            note(check_message(kind, text));
        }
    }
    if let Some(amount) = patch.voice_xp_amount {
        note(check_voice_amount(amount).map(drop));
    }
    if let Some(Some(theme)) = &patch.welcome_theme {
        note(check_theme(theme));
    }
    if let Some(Some(title)) = &mut patch.welcome_title {
        match check_welcome_title(title) {
            Ok(clean) => *title = clean,
            Err(error) => note(Err(error)),
        }
    }
    if let Some(name) = &mut patch.points_name {
        match check_points_name(name) {
            Ok(clean) => *name = clean,
            Err(error) => note(Err(error)),
        }
    }
    if let Some(emoji) = &mut patch.points_emoji {
        match check_points_emoji(emoji) {
            Ok(clean) => *emoji = clean,
            Err(error) => note(Err(error)),
        }
    }
    if let Some(Some(prefix)) = &patch.prefix {
        match check_prefix(prefix) {
            // The default is stored as no prefix, as `/config prefix reset` leaves it.
            Ok(clean) => patch.prefix = Some(Some(clean).filter(|p| p != DEFAULT_PREFIX)),
            Err(error) => note(Err(error)),
        }
    }
    if let Some(points_now) = points_now
        && (patch.points_min.is_some() || patch.points_max.is_some())
    {
        let min = patch.points_min.unwrap_or(points_now.0);
        let max = patch.points_max.unwrap_or(points_now.1);
        note(check_points(min, max));
    } else if points_now.is_none() {
        for (field, value) in [("points_min", patch.points_min), ("points_max", patch.points_max)] {
            if let Some(value) = value
                && !POINTS_RANGE.contains(&value)
            {
                note(Err(FieldError::new(
                    field,
                    format!(
                        "Points per message go from {} to {}.",
                        POINTS_RANGE.start(),
                        POINTS_RANGE.end()
                    ),
                )
                .keyed(
                    "points-range",
                    [
                        ("low", POINTS_RANGE.start().to_string()),
                        ("high", POINTS_RANGE.end().to_string()),
                    ],
                )));
            }
        }
    }
    if errors.is_empty() { Ok(patch) } else { Err(errors) }
}

#[cfg(test)]
mod tests;

mod roles;
pub use roles::{
    DANGEROUS_PERMISSIONS, Grantor, RoleFacts, action_role_refusal, action_role_refusal_in, dangerous_permissions,
    dangerous_permissions_in, grant_refusal, grant_refusal_in, role_refusal, role_refusal_in, run_time_refusal,
    run_time_refusal_in,
};
