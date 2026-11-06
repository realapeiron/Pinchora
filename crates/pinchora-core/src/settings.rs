//! Server settings rules shared by `/config` and the web dashboard: limits, defaults, what a valid
//! value is, and the sentence that says why one is not. Pure: no Discord, no database.

use std::ops::RangeInclusive;

use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    i18n::{self, Lang},
    leveling::{LevelUpMode, MAX_MULTIPLIER, VOICE_XP_RANGE},
    t,
};

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
        let args: Vec<(&str, &dyn std::fmt::Display)> = self
            .args
            .iter()
            .map(|(k, v)| (*k, v as &dyn std::fmt::Display))
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
        let args: Vec<(&str, &dyn std::fmt::Display)> = self
            .args
            .iter()
            .map(|(k, v)| (*k, v as &dyn std::fmt::Display))
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

/// What decides whether the bot can hand a role out, and who may set it up to be handed out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleFacts {
    /// `@everyone`, whose id is the server's.
    pub everyone: bool,
    /// Owned by an integration or a bot.
    pub managed: bool,
    pub position: i64,
    /// The role's permission bits, as Discord sends them.
    pub permissions: u64,
}

/// Permissions that run or police a server, as Discord's bits and the names its settings show. A role
/// holding any of them is only set up to be handed out by the server owner or an administrator.
pub const DANGEROUS_PERMISSIONS: [(u64, &str); 10] = [
    (1 << 3, "Administrator"),
    (1 << 5, "Manage Server"),
    (1 << 28, "Manage Roles"),
    (1 << 4, "Manage Channels"),
    (1 << 29, "Manage Webhooks"),
    (1 << 2, "Ban Members"),
    (1 << 1, "Kick Members"),
    (1 << 40, "Timeout Members"),
    (1 << 17, "Mention @everyone, @here and All Roles"),
    (1 << 13, "Manage Messages"),
];

/// The dangerous permissions among `bits`, by name, in [`DANGEROUS_PERMISSIONS`] order.
pub fn dangerous_permissions(bits: u64) -> Vec<&'static str> {
    DANGEROUS_PERMISSIONS
        .iter()
        .filter(|(bit, _)| bits & bit != 0)
        .map(|(_, name)| *name)
        .collect()
}

/// Catalog keys (`perm.<key>`) of [`DANGEROUS_PERMISSIONS`], in the same order.
const PERMISSION_KEYS: [&str; 10] = [
    "administrator",
    "manage-server",
    "manage-roles",
    "manage-channels",
    "manage-webhooks",
    "ban-members",
    "kick-members",
    "timeout-members",
    "mention-everyone",
    "manage-messages",
];

/// [`dangerous_permissions`] named the way Discord's own settings word them in `lang`.
pub fn dangerous_permissions_in(lang: Lang, bits: u64) -> Vec<String> {
    DANGEROUS_PERMISSIONS
        .iter()
        .zip(PERMISSION_KEYS)
        .filter(|((bit, _), _)| bits & bit != 0)
        .map(|(_, key)| t!(lang, &format!("perm.{key}")))
        .collect()
}

/// Who is setting a role up to be handed out (a level, reputation, store, join or panel role, or a
/// custom action): what decides which roles they may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grantor {
    pub owner: bool,
    /// Holds Administrator (the owner does too, as Discord counts it).
    pub administrator: bool,
    /// The position of their highest role (0 with none but `@everyone`); `None` when it could not be
    /// looked up.
    pub top_role: Option<i64>,
}

fn powers(lang: Lang, role: &RoleFacts) -> Option<String> {
    let names = dangerous_permissions_in(lang, role.permissions);
    match names.as_slice() {
        [] => None,
        [one] => Some(one.clone()),
        [first, second] => Some(t!(lang, "refusal.powers-two", first = first, second = second)),
        [first, ..] => Some(t!(lang, "refusal.powers-more", first = first, count = names.len() - 1)),
    }
}

/// Why `grantor` may not set `role` up to be handed out, or `None` when they may. A role with a
/// [dangerous permission](DANGEROUS_PERMISSIONS) needs the owner or an administrator; a role at or
/// above the grantor's own highest role needs the owner, as Discord's own role settings do.
/// `label` names the role in the sentence.
pub fn grant_refusal(role: RoleFacts, grantor: Grantor, label: &str) -> Option<String> {
    grant_refusal_in(Lang::En, role, grantor, label)
}

/// [`grant_refusal`] worded in `lang`.
pub fn grant_refusal_in(lang: Lang, role: RoleFacts, grantor: Grantor, label: &str) -> Option<String> {
    if !(grantor.owner || grantor.administrator)
        && let Some(powers) = powers(lang, &role)
    {
        return Some(t!(lang, "refusal.grant-powers", label = label, powers = powers));
    }
    hierarchy_refusal(lang, role, grantor, label)
}

fn hierarchy_refusal(lang: Lang, role: RoleFacts, grantor: Grantor, label: &str) -> Option<String> {
    if grantor.owner {
        return None;
    }
    match grantor.top_role {
        Some(top) if role.position < top => None,
        Some(_) => Some(t!(lang, "refusal.not-below", label = label)),
        None => Some(t!(lang, "refusal.roles-unchecked")),
    }
}

/// [`grant_refusal`] for a role a custom command or auto-responder hands out. Any member sets those off
/// whenever they like, so a role with a dangerous permission is refused whoever writes it, and the bot
/// checks it again every time it runs ([`run_time_refusal`]).
pub fn action_role_refusal(role: RoleFacts, grantor: Grantor, label: &str) -> Option<String> {
    action_role_refusal_in(Lang::En, role, grantor, label)
}

/// [`action_role_refusal`] worded in `lang`.
pub fn action_role_refusal_in(lang: Lang, role: RoleFacts, grantor: Grantor, label: &str) -> Option<String> {
    run_time_refusal_in(lang, role.permissions, label).or_else(|| hierarchy_refusal(lang, role, grantor, label))
}

/// Why a custom action may not hand out a role with these permissions now, or `None` when it may:
/// permissions can change after the action was written.
pub fn run_time_refusal(permissions: u64, label: &str) -> Option<String> {
    run_time_refusal_in(Lang::En, permissions, label)
}

/// [`run_time_refusal`] worded in `lang`.
pub fn run_time_refusal_in(lang: Lang, permissions: u64, label: &str) -> Option<String> {
    let role = RoleFacts {
        everyone: false,
        managed: false,
        position: 0,
        permissions,
    };
    powers(lang, &role).map(|powers| t!(lang, "refusal.run-time", label = label, powers = powers))
}

/// Why the bot cannot hand out a role, or `None` when it can. `bot_top` is the position of the
/// bot's highest role (`None` when it could not be looked up); `label` names the role in the
/// sentence (a mention in Discord, `@name` on the web).
pub fn role_refusal(role: RoleFacts, bot_top: Option<i64>, label: &str) -> Option<String> {
    role_refusal_in(Lang::En, role, bot_top, label)
}

/// [`role_refusal`] worded in `lang`.
pub fn role_refusal_in(lang: Lang, role: RoleFacts, bot_top: Option<i64>, label: &str) -> Option<String> {
    if role.everyone {
        return Some(t!(lang, "refusal.everyone"));
    }
    if role.managed {
        return Some(t!(lang, "refusal.managed", label = label));
    }
    match bot_top {
        Some(top) if role.position < top => None,
        Some(_) => Some(t!(lang, "refusal.above-bot", label = label)),
        None => Some(t!(lang, "refusal.bot-unchecked")),
    }
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
pub fn check_patch(
    mut patch: GuildSettingsPatch,
    points_now: (i64, i64),
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
    if patch.points_min.is_some() || patch.points_max.is_some() {
        let min = patch.points_min.unwrap_or(points_now.0);
        let max = patch.points_max.unwrap_or(points_now.1);
        note(check_points(min, max));
    }
    if errors.is_empty() { Ok(patch) } else { Err(errors) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every error with a catalog key says in English what its message says, and has Turkish too.
    #[test]
    fn keyed_errors_read_the_same_in_the_english_catalog() {
        let long = "x".repeat(MESSAGE_MAX + 1);
        let errors = [
            check_accent("zzz").unwrap_err(),
            check_message(MessageKind::Welcome, " ").unwrap_err(),
            check_message(MessageKind::Goodbye, &long).unwrap_err(),
            check_welcome_title(" ").unwrap_err(),
            check_welcome_title(&"y".repeat(WELCOME_TITLE_MAX + 1)).unwrap_err(),
            check_theme("neon").unwrap_err(),
            parse_levelup_mode("somewhere").unwrap_err(),
            check_points_name("").unwrap_err(),
            check_points_emoji("abc").unwrap_err(),
            check_points(-5, 5).unwrap_err(),
            check_points(9, 3).unwrap_err(),
            check_multiplier(9.0).unwrap_err(),
            check_voice_amount(0).unwrap_err(),
            check_level(0).unwrap_err(),
            check_reputation(0).unwrap_err(),
            check_price(0).unwrap_err(),
            check_stock(-1).unwrap_err(),
            crate::modules::check_channels(crate::modules::Module::Welcome, crate::modules::ChannelMode::Only, &[1])
                .unwrap_err(),
            crate::modules::check_channels(crate::modules::Module::Pets, crate::modules::ChannelMode::Only, &[0])
                .unwrap_err(),
            crate::modules::check_channels(crate::modules::Module::Pets, crate::modules::ChannelMode::Only, &[])
                .unwrap_err(),
        ];
        for error in errors {
            assert_eq!(
                error.catalog_text().as_deref(),
                Some(error.message.as_str()),
                "{error:?}"
            );
            let turkish = error.text(Lang::Tr);
            assert_ne!(turkish, error.message, "{error:?} has no Turkish yet");
            assert!(!turkish.contains("settings-error"), "{turkish}");
        }
        assert_eq!(
            FieldError::new("x", "Plain").text(Lang::Tr),
            "Plain",
            "no key: the English message"
        );
    }

    #[test]
    fn role_refusals_have_twin_words_in_turkish() {
        let managed = RoleFacts {
            everyone: false,
            managed: true,
            position: 1,
            permissions: 0,
        };
        assert_eq!(
            role_refusal_in(Lang::En, managed, Some(5), "<@&1>"),
            role_refusal(managed, Some(5), "<@&1>")
        );
        assert!(
            role_refusal_in(Lang::Tr, managed, Some(5), "<@&1>")
                .unwrap()
                .contains("yönetiliyor")
        );
        let admin = (1u64 << 3) | (1 << 5);
        let powers = dangerous_permissions_in(Lang::Tr, admin);
        assert_eq!(powers, ["Yönetici", "Sunucuyu Yönet"]);
        let refusal = run_time_refusal_in(Lang::Tr, admin, "<@&2>").unwrap();
        assert!(refusal.contains("Yönetici ve Sunucuyu Yönet"), "{refusal}");
    }

    #[test]
    fn colours_need_six_hex_digits() {
        assert_eq!(parse_hex_color("#ff6b4a"), Some(0xff6b4a));
        assert_eq!(parse_hex_color(" 06D6A0 "), Some(0x06d6a0));
        for bad in ["", "#fff", "#ff6b4a0", "+ff6b4", "#gg0000", "##ff6b4a"] {
            assert_eq!(parse_hex_color(bad), None, "{bad:?}");
        }
        assert_eq!(check_accent("nope").unwrap_err().field, "welcome_accent");
    }

    #[test]
    fn messages_are_bounded_and_not_blank() {
        assert!(check_message(MessageKind::Welcome, "Hi {user}").is_ok());
        assert!(check_message(MessageKind::Welcome, &"é".repeat(MESSAGE_MAX)).is_ok());
        let long = check_message(MessageKind::Goodbye, &"x".repeat(MESSAGE_MAX + 1)).unwrap_err();
        assert_eq!(long.field, "goodbye_message");
        assert!(long.message.contains("501"), "{}", long.message);
        assert_eq!(
            check_message(MessageKind::LevelUp, "  ").unwrap_err().field,
            "levelup_message"
        );
    }

    #[test]
    fn titles_names_and_emoji_are_trimmed_and_bounded() {
        assert_eq!(check_welcome_title("  HI THERE ").unwrap(), "HI THERE");
        assert!(check_welcome_title(&"W".repeat(WELCOME_TITLE_MAX + 1)).is_err());
        assert!(check_welcome_title(" ").is_err());
        assert_eq!(check_points_name(" coins ").unwrap(), "coins");
        assert!(check_points_name("").is_err());
        for good in [
            "⭐",
            " 🪙 ",
            "🇹🇷",
            "👨‍👩‍👧",
            "<:coin:123456789012345678>",
            "<a:spin:123456789012345678>",
        ] {
            assert!(check_points_emoji(good).is_ok(), "{good:?}");
        }
        for bad in ["", "coins", "⭐⭐", "<:c:1>", "<coin:123456789012345678>"] {
            assert!(check_points_emoji(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn prefixes_are_short_and_cannot_ping() {
        assert_eq!(check_prefix(" p! ").unwrap(), "p!");
        for good in ["?", "z!", "pin.", "!!!!!", "ğ$"] {
            assert!(check_prefix(good).is_ok(), "{good:?}");
        }
        for bad in ["", "  ", "toolong", "p !", "@", "<@1>", "#", "`p", "p\tq"] {
            assert_eq!(check_prefix(bad).unwrap_err().field, "prefix", "{bad:?}");
        }
    }

    #[test]
    fn points_ranges_and_order() {
        assert!(check_points(0, 1_000).is_ok());
        assert_eq!(check_points(5, 4).unwrap_err().field, "points_min");
        assert_eq!(check_points(0, 1_001).unwrap_err().field, "points_max");
        assert_eq!(check_points(-1, 3).unwrap_err().field, "points_min");
    }

    #[test]
    fn multipliers_round_and_stay_in_range() {
        assert_eq!(check_multiplier(1.234).unwrap(), 1.23);
        assert_eq!(check_multiplier(0.0).unwrap(), 0.0);
        assert_eq!(check_multiplier(5.004).unwrap(), 5.0);
        for bad in [5.01, -0.01, f64::NAN, f64::INFINITY] {
            assert!(check_multiplier(bad).is_err(), "{bad}");
        }
        assert_eq!(
            check_multiplier(9.0).unwrap_err().message,
            "A multiplier goes from 0 to 5."
        );
    }

    #[test]
    fn counts_have_ranges() {
        assert_eq!(check_voice_amount(10).unwrap(), 10);
        assert!(check_voice_amount(0).is_err() && check_voice_amount(101).is_err());
        assert!(check_level(1).is_ok() && check_level(1_001).is_err());
        assert!(check_reputation(100_000).is_ok() && check_reputation(0).is_err());
        assert!(check_price(1).is_ok() && check_price(0).is_err());
        assert!(check_stock(1_000_000).is_ok() && check_stock(0).is_err());
    }

    #[test]
    fn themes_and_modes_are_known_keys() {
        assert!(check_theme("midnight").is_ok() && check_theme("neon").is_err());
        assert_eq!(parse_levelup_mode("dm").unwrap(), LevelUpMode::Dm);
        assert!(parse_levelup_mode("everywhere").is_err());
        for mode in LEVELUP_MODES {
            assert_eq!(LevelUpMode::parse(mode.key()), mode);
        }
        for kind in MessageKind::ALL {
            assert_eq!(MessageKind::parse(kind.key()), Some(kind));
            assert!(kind.placeholders().contains(&"{user}"));
        }
    }

    #[test]
    fn dangerous_permissions_are_named() {
        assert!(dangerous_permissions(0).is_empty());
        assert!(dangerous_permissions((1 << 10) | (1 << 11)).is_empty(), "view and send");
        assert_eq!(dangerous_permissions(1 << 3), ["Administrator"]);
        assert_eq!(
            dangerous_permissions((1 << 40) | (1 << 1)),
            ["Kick Members", "Timeout Members"]
        );
        let all = DANGEROUS_PERMISSIONS.iter().fold(0, |bits, (bit, _)| bits | bit);
        assert_eq!(dangerous_permissions(all).len(), DANGEROUS_PERMISSIONS.len());
    }

    #[test]
    fn who_may_set_a_role_to_be_handed_out() {
        const ADMIN: u64 = 1 << 3;
        const MANAGE_MESSAGES: u64 = 1 << 13;
        const BAN: u64 = 1 << 2;
        let role = |position, permissions| RoleFacts {
            everyone: false,
            managed: false,
            position,
            permissions,
        };
        let owner = Grantor {
            owner: true,
            administrator: true,
            top_role: Some(0),
        };
        let admin = |top| Grantor {
            owner: false,
            administrator: true,
            top_role: top,
        };
        let manager = |top| Grantor {
            owner: false,
            administrator: false,
            top_role: top,
        };
        // (role, grantor, configured roles allowed, custom actions allowed)
        let cases = [
            (role(3, 0), manager(Some(5)), true, true),
            (role(5, 0), manager(Some(5)), false, false),
            (role(6, 0), manager(Some(5)), false, false),
            (role(3, 0), manager(None), false, false),
            (role(3, ADMIN), manager(Some(5)), false, false),
            (role(3, MANAGE_MESSAGES), manager(Some(5)), false, false),
            (role(3, BAN | ADMIN), admin(Some(5)), true, false),
            (role(6, ADMIN), admin(Some(5)), false, false),
            (role(6, 0), admin(Some(5)), false, false),
            (role(30, ADMIN), owner, true, false),
            (role(30, 0), owner, true, true),
            (
                role(30, 0),
                Grantor {
                    top_role: None,
                    ..owner
                },
                true,
                true,
            ),
        ];
        for (n, (role, grantor, configured, action)) in cases.into_iter().enumerate() {
            assert_eq!(
                grant_refusal(role, grantor, "@R").is_none(),
                configured,
                "case {n}: {role:?} {grantor:?}"
            );
            assert_eq!(
                action_role_refusal(role, grantor, "@R").is_none(),
                action,
                "case {n} as an action: {role:?} {grantor:?}"
            );
        }
        let why = grant_refusal(role(3, ADMIN | BAN | MANAGE_MESSAGES), manager(Some(5)), "@Mod").unwrap();
        assert_eq!(
            why,
            "@Mod has Administrator and 2 more, so only the server owner or an administrator can set it to be handed out."
        );
        assert!(
            grant_refusal(role(5, 0), manager(Some(5)), "@Top")
                .unwrap()
                .contains("not below your highest role")
        );
        assert_eq!(run_time_refusal(0, "@Fan"), None);
        assert_eq!(
            run_time_refusal(ADMIN | BAN, "@Fan").unwrap(),
            "@Fan has Administrator and Ban Members, so custom commands and auto-responders can't hand it out."
        );
    }

    #[test]
    fn role_refusals_explain_themselves() {
        let role = |everyone, managed, position| RoleFacts {
            everyone,
            managed,
            position,
            permissions: 0,
        };
        assert_eq!(role_refusal(role(false, false, 3), Some(5), "@Fan"), None);
        assert!(
            role_refusal(role(true, false, 0), Some(5), "@everyone")
                .unwrap()
                .contains("every member")
        );
        assert!(
            role_refusal(role(false, true, 1), Some(5), "@Bot")
                .unwrap()
                .starts_with("@Bot is managed")
        );
        assert!(
            role_refusal(role(false, false, 5), Some(5), "@Top")
                .unwrap()
                .contains("above my highest role")
        );
        assert!(
            role_refusal(role(false, false, 1), None, "@Fan")
                .unwrap()
                .contains("couldn't check")
        );
    }

    #[test]
    fn a_patch_is_cleaned_or_every_problem_is_listed() {
        let patch = GuildSettingsPatch {
            welcome_title: Some(Some("  HELLO ".into())),
            points_name: Some(" shells ".into()),
            points_max: Some(5),
            ..Default::default()
        };
        let clean = check_patch(patch, (1, 3)).unwrap();
        assert_eq!(clean.welcome_title, Some(Some("HELLO".into())));
        let prefix = |p: &str| GuildSettingsPatch {
            prefix: Some(Some(p.into())),
            ..Default::default()
        };
        assert_eq!(
            check_patch(prefix(" z! "), (1, 3)).unwrap().prefix,
            Some(Some("z!".into()))
        );
        assert_eq!(check_patch(prefix("p!"), (1, 3)).unwrap().prefix, Some(None));
        assert_eq!(check_patch(prefix("@"), (1, 3)).unwrap_err()[0].field, "prefix");
        assert_eq!(clean.points_name.as_deref(), Some("shells"));

        let bad = GuildSettingsPatch {
            welcome_message: Some(Some(" ".into())),
            welcome_theme: Some(Some("neon".into())),
            points_max: Some(2),
            voice_xp_amount: Some(0),
            ..Default::default()
        };
        let errors = check_patch(bad, (3, 5)).unwrap_err();
        let fields: Vec<_> = errors.iter().map(|e| e.field).collect();
        assert_eq!(
            fields,
            ["welcome_message", "voice_xp_amount", "welcome_theme", "points_min"]
        );
        // Clearing is always allowed.
        let clear = GuildSettingsPatch {
            welcome_message: Some(None),
            welcome_title: Some(None),
            ..Default::default()
        };
        assert!(check_patch(clear, (1, 3)).is_ok());
    }
}
