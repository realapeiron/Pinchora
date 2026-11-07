//! Moderation rules the bot and the dashboard share: who may act on whom, the kinds of cases, the
//! limits Discord sets, and what a number of warns brings. Pure: no Discord, no database.

use std::ops::RangeInclusive;

use chrono::Duration;

use crate::settings::{FieldError, Grantor, RoleFacts, grant_refusal, role_refusal};

/// Longest reason a case keeps (Discord's audit log takes 512 characters).
pub const REASON_MAX: usize = 512;
/// Discord times a member out for at most 28 days.
pub const TIMEOUT_MAX: Duration = Duration::days(28);
pub const TIMEOUT_MIN: Duration = Duration::minutes(1);
/// A temporary ban lasts from a minute to a year.
pub const TEMP_BAN_MIN: Duration = Duration::minutes(1);
pub const TEMP_BAN_MAX: Duration = Duration::days(365);
/// How many days of a banned member's messages Discord can delete.
pub const DELETE_DAYS_MAX: i64 = 7;
/// `/purge` takes up to 100 messages, none older than 14 days (Discord's bulk delete rule).
pub const PURGE_MAX: i64 = 100;
pub const PURGE_AGE: Duration = Duration::days(14);
/// Slowmode runs from off to six hours.
pub const SLOWMODE_MAX: Duration = Duration::hours(6);
/// How many warn thresholds a server may set, and the warn counts they may name.
pub const MAX_THRESHOLDS: usize = 10;
pub const WARNS_RANGE: RangeInclusive<i64> = 1..=50;
/// Longest nickname Discord takes.
pub const NICK_MAX: usize = 32;

/// What a case records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaseKind {
    Note,
    Report,
    Appeal,
    Mute,
    Unmute,
    Jail,
    Unjail,
    Softban,
    Voice,
    RoleEdit,

    Ban,
    Unban,
    Kick,
    Timeout,
    Untimeout,
    Warn,
    Unwarn,
    Purge,
    Slowmode,
    Lock,
    Unlock,
    Nick,
    RoleAdd,
    RoleRemove,
}

impl CaseKind {
    pub const ALL: [Self; 24] = [
        Self::Note,
        Self::Report,
        Self::Appeal,
        Self::Mute,
        Self::Unmute,
        Self::Jail,
        Self::Unjail,
        Self::Softban,
        Self::Voice,
        Self::RoleEdit,
        Self::Ban,
        Self::Unban,
        Self::Kick,
        Self::Timeout,
        Self::Untimeout,
        Self::Warn,
        Self::Unwarn,
        Self::Purge,
        Self::Slowmode,
        Self::Lock,
        Self::Unlock,
        Self::Nick,
        Self::RoleAdd,
        Self::RoleRemove,
    ];

    /// The key stored in the database and used in the API.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Ban => "ban",
            Self::Note => "note",
            Self::Report => "report",
            Self::Appeal => "appeal",
            Self::Mute => "mute",
            Self::Unmute => "unmute",
            Self::Jail => "jail",
            Self::Unjail => "unjail",
            Self::Softban => "softban",
            Self::Voice => "voice",
            Self::RoleEdit => "role_edit",

            Self::Unban => "unban",
            Self::Kick => "kick",
            Self::Timeout => "timeout",
            Self::Untimeout => "untimeout",
            Self::Warn => "warn",
            Self::Unwarn => "unwarn",
            Self::Purge => "purge",
            Self::Slowmode => "slowmode",
            Self::Lock => "lock",
            Self::Unlock => "unlock",
            Self::Nick => "nick",
            Self::RoleAdd => "role_add",
            Self::RoleRemove => "role_remove",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ban => "Ban",
            Self::Note => "Note",
            Self::Report => "Report",
            Self::Appeal => "Appeal",
            Self::Mute => "Mute",
            Self::Unmute => "Unmute",
            Self::Jail => "Jail",
            Self::Unjail => "Unjail",
            Self::Softban => "Softban",
            Self::Voice => "Voice",
            Self::RoleEdit => "RoleEdit",

            Self::Unban => "Unban",
            Self::Kick => "Kick",
            Self::Timeout => "Timeout",
            Self::Untimeout => "Timeout removed",
            Self::Warn => "Warn",
            Self::Unwarn => "Warn removed",
            Self::Purge => "Purge",
            Self::Slowmode => "Slowmode",
            Self::Lock => "Lock",
            Self::Unlock => "Unlock",
            Self::Nick => "Nickname",
            Self::RoleAdd => "Role added",
            Self::RoleRemove => "Role removed",
        }
    }

    /// Whether the case is about a channel rather than a member.
    pub const fn on_channel(self) -> bool {
        matches!(self, Self::Purge | Self::Slowmode | Self::Lock | Self::Unlock)
    }

    /// Whether the member hears about it by direct message.
    pub const fn tells_member(self) -> bool {
        matches!(
            self,
            Self::Ban | Self::Kick | Self::Timeout | Self::Untimeout | Self::Warn
        )
    }

    /// Whether the direct message has to go before the action: once banned or kicked, a member shares
    /// no server with the bot and cannot be written to.
    pub const fn tells_first(self) -> bool {
        matches!(self, Self::Ban | Self::Kick)
    }
}

/// A member as the role hierarchy sees them: their id and the position of their highest role (0 when
/// they hold none but @everyone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rank {
    pub id: u64,
    pub top: i64,
}

/// Why someone may not act on a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Yourself,
    Me,
    Owner,
    /// The member's highest role is not below the moderator's.
    Above,
    /// The member's highest role is not below the bot's.
    AboveMe,
}

/// Whether `actor` may act on `target`, with the bot doing it: Discord's own rule. The owner is above
/// everyone and can be acted on by no one; anyone else must sit strictly higher than the member they
/// act on, and so must the bot.
pub fn may_act(owner: u64, actor: Rank, bot: Rank, target: Rank) -> Result<(), Refusal> {
    if target.id == actor.id {
        return Err(Refusal::Yourself);
    }
    if target.id == bot.id {
        return Err(Refusal::Me);
    }
    if target.id == owner {
        return Err(Refusal::Owner);
    }
    if actor.id != owner && actor.top <= target.top {
        return Err(Refusal::Above);
    }
    if bot.top <= target.top {
        return Err(Refusal::AboveMe);
    }
    Ok(())
}

/// Whether `actor` sits above `other` for what is done to a case after the fact (taking a warn back,
/// a new reason): the owner does, nobody is above the owner, and anyone else must sit strictly higher.
pub fn outranks(owner: u64, actor: Rank, other: Rank) -> bool {
    actor.id == owner || (other.id != owner && actor.top > other.top)
}

/// Why `grantor` may not hand out or take `role`, or `None` when they may: the same rule as every role
/// Pinchora hands out. The bot must be able to give it ([`role_refusal`]), and the grantor may only
/// move roles below their own highest one and, unless they own the server or are an administrator,
/// none with a dangerous permission ([`grant_refusal`]).
pub fn role_refusal_for(role: RoleFacts, grantor: Grantor, bot_top: Option<i64>, label: &str) -> Option<String> {
    role_refusal(role, bot_top, label).or_else(|| grant_refusal(role, grantor, label))
}

/// What a number of warns brings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Punishment {
    Timeout,
    Kick,
    Ban,
}

impl Punishment {
    pub const ALL: [Self; 3] = [Self::Timeout, Self::Kick, Self::Ban];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Kick => "kick",
            Self::Ban => "ban",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }
}

/// One rung of the warn ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Threshold {
    pub warns: i32,
    pub action: Punishment,
    /// How long a timeout or a ban lasts; a ban without one is permanent, a kick has none.
    pub duration: Option<Duration>,
}

impl Threshold {
    /// "3 warns → 1h timeout".
    pub fn describe(&self) -> String {
        let what = match (self.action, self.duration) {
            (Punishment::Timeout, Some(d)) => format!("{} timeout", format_duration(d)),
            (Punishment::Ban, Some(d)) => format!("{} ban", format_duration(d)),
            (Punishment::Ban, None) => "permanent ban".to_owned(),
            (Punishment::Kick, _) => "kick".to_owned(),
            (Punishment::Timeout, None) => "timeout".to_owned(),
        };
        let s = if self.warns == 1 { "" } else { "s" };
        format!("{} warn{s} → {what}", self.warns)
    }
}

/// Checks a threshold as a manager typed it: a warn count in [`WARNS_RANGE`], a known action, and a
/// duration that fits it (required for a timeout, optional for a ban, none for a kick).
pub fn check_threshold(warns: i64, action: &str, duration: Option<Duration>) -> Result<Threshold, FieldError> {
    if !WARNS_RANGE.contains(&warns) {
        return Err(FieldError::new(
            "warns",
            format!("Warn counts go from {} to {}.", WARNS_RANGE.start(), WARNS_RANGE.end()),
        ));
    }
    let Some(action) = Punishment::parse(action) else {
        return Err(FieldError::new("action", "Pick a timeout, a kick or a ban."));
    };
    let duration = match action {
        Punishment::Timeout => Some(check_timeout(duration)?),
        Punishment::Ban => duration.map(check_temp_ban).transpose()?,
        Punishment::Kick if duration.is_some() => {
            return Err(FieldError::new("duration", "A kick has no duration."));
        }
        Punishment::Kick => None,
    };
    Ok(Threshold {
        warns: warns as i32,
        action,
        duration,
    })
}

/// The threshold a member who now has `warns` active warns has just reached, if any. Only an exact
/// count fires, so each rung fires once on the way up.
pub fn reached(thresholds: &[Threshold], warns: i64) -> Option<Threshold> {
    thresholds.iter().find(|t| i64::from(t.warns) == warns).copied()
}

/// A timeout's length, checked: present and within Discord's limits.
pub fn check_timeout(duration: Option<Duration>) -> Result<Duration, FieldError> {
    match duration {
        Some(d) if (TIMEOUT_MIN..=TIMEOUT_MAX).contains(&d) => Ok(d),
        Some(_) => Err(FieldError::new(
            "duration",
            "A timeout lasts from a minute to 28 days, like `10m`, `2h` or `7d`.",
        )),
        None => Err(FieldError::new("duration", "Say how long, like `10m`, `2h` or `7d`.")),
    }
}

/// A temporary ban's length, checked.
pub fn check_temp_ban(duration: Duration) -> Result<Duration, FieldError> {
    if (TEMP_BAN_MIN..=TEMP_BAN_MAX).contains(&duration) {
        Ok(duration)
    } else {
        Err(FieldError::new(
            "duration",
            "A temporary ban lasts from a minute to a year, like `12h`, `7d` or `30d`.",
        ))
    }
}

/// A reason, trimmed; `None` when blank.
pub fn check_reason(text: Option<&str>) -> Result<Option<String>, FieldError> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > REASON_MAX {
        return Err(FieldError::new(
            "reason",
            format!("A reason is at most {REASON_MAX} characters."),
        ));
    }
    Ok(Some(text.to_owned()))
}

/// A slowmode as typed: `off` or `0` turns it off, anything else is a duration up to six hours.
/// Returns the seconds.
pub fn parse_slowmode(text: &str) -> Result<u16, FieldError> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("off") || text == "0" {
        return Ok(0);
    }
    let seconds = text
        .parse::<i64>()
        .ok()
        .map(Duration::seconds)
        .or_else(|| crate::server_events::parse_duration(text))
        .filter(|d| *d > Duration::zero() && *d <= SLOWMODE_MAX);
    match seconds {
        Some(d) => Ok(d.num_seconds() as u16),
        None => Err(FieldError::new(
            "duration",
            "Slowmode goes up to six hours, like `5s`, `30s`, `2m` or `1h`; `off` turns it off.",
        )),
    }
}

/// A duration the short way people write it: `45s`, `10m`, `2h 30m`, `7d`, `1w 2d`.
pub fn format_duration(duration: Duration) -> String {
    let mut left = duration.num_seconds().max(0);
    if left == 0 {
        return "0s".to_owned();
    }
    let mut parts = Vec::new();
    for (unit, size) in [("w", 604_800), ("d", 86_400), ("h", 3_600), ("m", 60), ("s", 1)] {
        if left >= size {
            parts.push(format!("{}{unit}", left / size));
            left %= size;
        }
        if parts.len() == 2 {
            break;
        }
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: u64 = 1;
    const BOT: u64 = 2;

    fn rank(id: u64, top: i64) -> Rank {
        Rank { id, top }
    }

    /// Discord's hierarchy, row by row: (actor, bot top, target, expected).
    #[test]
    fn the_hierarchy_follows_discord() {
        let bot = |top| rank(BOT, top);
        type Row = (&'static str, Rank, i64, Rank, Result<(), Refusal>);
        let rows: &[Row] = &[
            ("mod above member", rank(10, 5), 9, rank(11, 3), Ok(())),
            ("equal roles", rank(10, 5), 9, rank(11, 5), Err(Refusal::Above)),
            ("member above mod", rank(10, 5), 9, rank(11, 7), Err(Refusal::Above)),
            (
                "bot not above member",
                rank(10, 8),
                6,
                rank(11, 6),
                Err(Refusal::AboveMe),
            ),
            ("bot below member", rank(10, 8), 4, rank(11, 6), Err(Refusal::AboveMe)),
            ("yourself", rank(10, 5), 9, rank(10, 5), Err(Refusal::Yourself)),
            ("the bot", rank(10, 5), 9, rank(BOT, 9), Err(Refusal::Me)),
            ("the owner", rank(10, 99), 100, rank(OWNER, 0), Err(Refusal::Owner)),
            ("owner on anyone", rank(OWNER, 0), 9, rank(11, 8), Ok(())),
            (
                "owner, member above the bot",
                rank(OWNER, 0),
                9,
                rank(11, 9),
                Err(Refusal::AboveMe),
            ),
            ("no roles on no roles", rank(10, 0), 9, rank(11, 0), Err(Refusal::Above)),
            ("member with no roles", rank(10, 1), 9, rank(11, 0), Ok(())),
        ];
        for (label, actor, bot_top, target, expected) in rows {
            assert_eq!(may_act(OWNER, *actor, bot(*bot_top), *target), *expected, "{label}");
        }
    }

    #[test]
    fn only_the_owner_or_someone_higher_outranks() {
        type Row = (&'static str, Rank, Rank, bool);
        let rows: &[Row] = &[
            ("higher", rank(10, 5), rank(11, 3), true),
            ("equal", rank(10, 5), rank(11, 5), false),
            ("lower", rank(10, 3), rank(11, 5), false),
            ("no longer a member", rank(10, 0), rank(11, -1), true),
            ("the owner acts", rank(OWNER, 0), rank(11, 9), true),
            ("on the owner", rank(10, 99), rank(OWNER, 0), false),
        ];
        for (label, actor, other, expected) in rows {
            assert_eq!(outranks(OWNER, *actor, *other), *expected, "{label}");
        }
    }

    /// The shared role rule, row by row: (role position, permissions, grantor, bot top, refused with).
    #[test]
    fn roles_must_sit_below_the_moderator_and_the_bot() {
        const BAN: u64 = 1 << 2;
        let role = |position, permissions| RoleFacts {
            everyone: false,
            managed: false,
            position,
            permissions,
        };
        let moderator = Grantor {
            owner: false,
            administrator: false,
            top_role: Some(5),
        };
        let admin = Grantor {
            administrator: true,
            ..moderator
        };
        let owner = Grantor {
            owner: true,
            administrator: true,
            top_role: Some(0),
        };
        type Row = (&'static str, RoleFacts, Grantor, Option<&'static str>);
        let rows: &[Row] = &[
            ("below both", role(3, 0), moderator, None),
            ("equal to the moderator", role(5, 0), moderator, Some("not below your")),
            ("above the moderator", role(7, 0), moderator, Some("not below your")),
            (
                "a ban role for a moderator",
                role(3, BAN),
                moderator,
                Some("Ban Members"),
            ),
            ("a ban role for an administrator", role(3, BAN), admin, None),
            ("the owner, above their own roles", role(7, BAN), owner, None),
            ("not below the bot", role(9, 0), owner, Some("above my")),
            (
                "unknown roles",
                role(3, 0),
                Grantor {
                    top_role: None,
                    ..moderator
                },
                Some("couldn't check"),
            ),
        ];
        for (label, facts, grantor, refused) in rows {
            let why = role_refusal_for(*facts, *grantor, Some(9), "@R");
            match refused {
                None => assert_eq!(why, None, "{label}"),
                Some(part) => assert!(why.as_deref().is_some_and(|w| w.contains(part)), "{label}: {why:?}"),
            }
        }
        let managed = RoleFacts {
            managed: true,
            ..role(1, 0)
        };
        assert!(
            role_refusal_for(managed, owner, Some(9), "@R")
                .unwrap()
                .contains("managed")
        );
    }

    #[test]
    fn thresholds_need_a_fitting_duration() {
        let h = Duration::hours;
        let timeout = check_threshold(3, "timeout", Some(h(1))).unwrap();
        assert_eq!(timeout.describe(), "3 warns → 1h timeout");
        assert_eq!(check_threshold(3, "timeout", None).unwrap_err().field, "duration");
        assert_eq!(
            check_threshold(3, "timeout", Some(Duration::days(29)))
                .unwrap_err()
                .field,
            "duration"
        );
        assert_eq!(check_threshold(5, "kick", None).unwrap().describe(), "5 warns → kick");
        assert_eq!(check_threshold(5, "kick", Some(h(1))).unwrap_err().field, "duration");
        assert_eq!(
            check_threshold(7, "ban", None).unwrap().describe(),
            "7 warns → permanent ban"
        );
        assert_eq!(
            check_threshold(7, "ban", Some(h(48))).unwrap().describe(),
            "7 warns → 2d ban"
        );
        assert_eq!(check_threshold(0, "kick", None).unwrap_err().field, "warns");
        assert_eq!(check_threshold(51, "kick", None).unwrap_err().field, "warns");
        assert_eq!(check_threshold(2, "mute", None).unwrap_err().field, "action");
        assert_eq!(check_threshold(1, "kick", None).unwrap().describe(), "1 warn → kick");
    }

    #[test]
    fn only_the_exact_count_fires() {
        let ladder = [
            check_threshold(3, "timeout", Some(Duration::hours(1))).unwrap(),
            check_threshold(5, "kick", None).unwrap(),
        ];
        let fired: Vec<Option<Punishment>> = (1..=6).map(|n| reached(&ladder, n).map(|t| t.action)).collect();
        assert_eq!(
            fired,
            [
                None,
                None,
                Some(Punishment::Timeout),
                None,
                Some(Punishment::Kick),
                None
            ]
        );
    }

    #[test]
    fn reasons_are_trimmed_and_bounded() {
        assert_eq!(check_reason(Some("  spam ")).unwrap().as_deref(), Some("spam"));
        assert_eq!(check_reason(Some("   ")).unwrap(), None);
        assert_eq!(check_reason(None).unwrap(), None);
        assert!(check_reason(Some(&"é".repeat(REASON_MAX))).is_ok());
        assert!(check_reason(Some(&"x".repeat(REASON_MAX + 1))).is_err());
    }

    #[test]
    fn slowmode_reads_off_seconds_and_durations() {
        assert_eq!(parse_slowmode("off").unwrap(), 0);
        assert_eq!(parse_slowmode("0").unwrap(), 0);
        assert_eq!(parse_slowmode("30").unwrap(), 30);
        assert_eq!(parse_slowmode("2m").unwrap(), 120);
        assert_eq!(parse_slowmode("6h").unwrap(), 21_600);
        assert!(parse_slowmode("7h").is_err());
        assert!(parse_slowmode("soon").is_err());
    }

    #[test]
    fn durations_read_like_people_write_them() {
        assert_eq!(format_duration(Duration::seconds(45)), "45s");
        assert_eq!(format_duration(Duration::minutes(150)), "2h 30m");
        assert_eq!(format_duration(Duration::days(7)), "1w");
        assert_eq!(format_duration(Duration::days(9) + Duration::hours(5)), "1w 2d");
        assert_eq!(format_duration(Duration::zero()), "0s");
    }

    #[test]
    fn kinds_parse_back_and_know_their_target() {
        for kind in CaseKind::ALL {
            assert_eq!(CaseKind::parse(kind.key()), Some(kind));
        }
        assert!(CaseKind::Purge.on_channel() && !CaseKind::Ban.on_channel());
        assert!(CaseKind::Ban.tells_first() && !CaseKind::Warn.tells_first());
        assert!(CaseKind::Warn.tells_member() && !CaseKind::Nick.tells_member());
    }
}
