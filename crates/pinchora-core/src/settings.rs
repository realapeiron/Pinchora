//! Server settings rules shared by `/config` and the web dashboard: limits, defaults, what a valid
//! value is, and the sentence that says why one is not. Pure: no Discord, no database.

use std::ops::RangeInclusive;

use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;

use crate::leveling::{LevelUpMode, MAX_MULTIPLIER, VOICE_XP_RANGE};

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
}

impl FieldError {
    pub fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
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
    parse_hex_color(s).ok_or_else(|| FieldError::new("welcome_accent", "Colours look like `#ff6b4a`."))
}

/// A welcome, goodbye or level-up message: something to say, at most [`MESSAGE_MAX`] characters.
pub fn check_message(kind: MessageKind, s: &str) -> Result<(), FieldError> {
    let field = kind.field();
    if s.trim().is_empty() {
        return Err(FieldError::new(
            field,
            "Write a message, or reset it to use the default.",
        ));
    }
    let length = s.chars().count();
    if length > MESSAGE_MAX {
        return Err(FieldError::new(
            field,
            format!("A message is at most {MESSAGE_MAX} characters; this one has {length}."),
        ));
    }
    Ok(())
}

/// The welcome card headline, trimmed.
pub fn check_welcome_title(s: &str) -> Result<String, FieldError> {
    let title = s.trim();
    if title.is_empty() {
        return Err(FieldError::new("welcome_title", "Write a headline, e.g. WELCOME."));
    }
    if title.chars().count() > WELCOME_TITLE_MAX {
        return Err(FieldError::new(
            "welcome_title",
            format!("A headline is at most {WELCOME_TITLE_MAX} characters."),
        ));
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
        ))
    }
}

pub fn parse_levelup_mode(key: &str) -> Result<LevelUpMode, FieldError> {
    LEVELUP_MODES.into_iter().find(|m| m.key() == key).ok_or_else(|| {
        FieldError::new(
            "levelup_mode",
            "Level-ups go to the level-up channel, the current channel, a DM, or nowhere.",
        )
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
        ));
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
        ));
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
            return Err(FieldError::new(
                field,
                format!("Points per message go from {low} to {high}."),
            ));
        }
    }
    if min > max {
        return Err(FieldError::new(
            "points_min",
            "The least points per message can't be above the most.",
        ));
    }
    Ok(())
}

/// An XP multiplier, rounded to hundredths: 0 (no XP) to [`MAX_MULTIPLIER`].
pub fn check_multiplier(m: f64) -> Result<f64, FieldError> {
    let value = (m * 100.0).round() / 100.0;
    if !value.is_finite() || !(0.0..=MAX_MULTIPLIER).contains(&value) {
        return Err(FieldError::new(
            "multiplier",
            format!("A multiplier goes from 0 to {MAX_MULTIPLIER}."),
        ));
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
        ));
    }
    Ok(n as i32)
}

fn in_range(field: &'static str, n: i64, range: &RangeInclusive<i64>, what: &str) -> Result<i64, FieldError> {
    if range.contains(&n) {
        Ok(n)
    } else {
        Err(FieldError::new(
            field,
            format!("{what} goes from {} to {}.", range.start(), range.end()),
        ))
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

/// What decides whether the bot can hand a role out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleFacts {
    /// `@everyone`, whose id is the server's.
    pub everyone: bool,
    /// Owned by an integration or a bot.
    pub managed: bool,
    pub position: i64,
}

/// Why the bot cannot hand out a role, or `None` when it can. `bot_top` is the position of the
/// bot's highest role (`None` when it could not be looked up); `label` names the role in the
/// sentence (a mention in Discord, `@name` on the web).
pub fn role_refusal(role: RoleFacts, bot_top: Option<i64>, label: &str) -> Option<String> {
    if role.everyone {
        return Some("@everyone can't be handed out: every member already has it.".to_owned());
    }
    if role.managed {
        return Some(format!(
            "{label} is managed by an integration or a bot, so it can't be handed out."
        ));
    }
    match bot_top {
        Some(top) if role.position < top => None,
        Some(_) => Some(format!(
            "{label} is above my highest role. Drag my role above it in Server Settings > Roles, then try again."
        )),
        None => Some("I couldn't check my own role position just now. Try again in a moment.".to_owned()),
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
    fn role_refusals_explain_themselves() {
        let role = |everyone, managed, position| RoleFacts {
            everyone,
            managed,
            position,
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
