//! Server stats: what a server's members do, counted without reading any message text, and the
//! counter channels whose names show a live number. Pure: the bot counts and renames, the dashboard
//! shows, and both work out periods, buckets and counter names with these rules.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Utc};

use crate::{i18n::Lang, settings::FieldError, t};

/// How long the hourly and daily numbers are kept.
pub const KEEP_DAYS: i64 = 90;
/// Counter channels a server may have.
pub const MAX_COUNTERS: usize = 10;
/// The least time between two renames of one counter channel: Discord allows two name changes per
/// channel every ten minutes, and the counters keep well inside that.
pub const RENAME_GAP: Duration = Duration::minutes(10);
/// The longest counter template, so the number always fits Discord's 100-character channel names.
pub const TEMPLATE_MAX: usize = 80;
/// Discord's longest channel name.
pub const CHANNEL_NAME_MAX: usize = 100;
/// Counters that count members one by one (humans, bots, a role's members) stop above this many:
/// Discord lists 1,000 members per request, so this is ten requests every ten minutes.
pub const MEMBER_LIST_MAX: u64 = 10_000;
/// What a template writes where the number goes.
pub const COUNT_PLACEHOLDER: &str = "{count}";

/// A stretch of days the stats look back over, ending today (UTC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Period {
    Week,
    #[default]
    Month,
    Quarter,
}

impl Period {
    pub const ALL: [Self; 3] = [Self::Week, Self::Month, Self::Quarter];

    pub const fn days(self) -> i64 {
        match self {
            Self::Week => 7,
            Self::Month => 30,
            Self::Quarter => 90,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Week => "7d",
            Self::Month => "30d",
            Self::Quarter => "90d",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

    /// The period of `days` days, for the dashboard's `?days=`.
    pub fn of_days(days: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.days() == days)
    }

    /// The first day of the period that ends on `today`, today included.
    pub fn first_day(self, today: NaiveDate) -> NaiveDate {
        today - Duration::days(self.days() - 1)
    }
}

/// The number a counter channel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CounterKind {
    /// Everyone in the server, bots included (Discord's approximate count).
    Members,
    Humans,
    Bots,
    /// Members online right now (Discord's approximate count).
    Online,
    /// Server boosts.
    Boosts,
    Roles,
    /// Channels, categories and threads left out.
    Channels,
    /// Members holding one role.
    RoleMembers,
    /// Houses built by the server's players.
    Houses,
    /// Fish caught by the server's players.
    Fish,
    /// The average server level of the players who earned XP here.
    LevelAvg,
}

impl CounterKind {
    pub const ALL: [Self; 11] = [
        Self::Members,
        Self::Humans,
        Self::Bots,
        Self::Online,
        Self::Boosts,
        Self::Roles,
        Self::Channels,
        Self::RoleMembers,
        Self::Houses,
        Self::Fish,
        Self::LevelAvg,
    ];

    /// The key stored in the database and used in the API.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Members => "members",
            Self::Humans => "humans",
            Self::Bots => "bots",
            Self::Online => "online",
            Self::Boosts => "boosts",
            Self::Roles => "roles",
            Self::Channels => "channels",
            Self::RoleMembers => "role_members",
            Self::Houses => "houses",
            Self::Fish => "fish",
            Self::LevelAvg => "level_avg",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// What the dashboard calls it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Members => "Members",
            Self::Humans => "Humans",
            Self::Bots => "Bots",
            Self::Online => "Online now",
            Self::Boosts => "Boosts",
            Self::Roles => "Roles",
            Self::Channels => "Channels",
            Self::RoleMembers => "Members with a role",
            Self::Houses => "Pinchora houses",
            Self::Fish => "Fish caught",
            Self::LevelAvg => "Average level",
        }
    }

    /// [`CounterKind::label`] in `lang`.
    pub fn name(self, lang: Lang) -> String {
        t!(lang, &format!("stats.kind.{}", self.key()))
    }

    /// The template a counter starts with, in the server's language.
    pub fn default_template(self, lang: Lang) -> String {
        t!(lang, &format!("stats.template.{}", self.key()))
    }

    /// Whether the counter needs a role picked.
    pub const fn needs_role(self) -> bool {
        matches!(self, Self::RoleMembers)
    }

    /// Whether the number is found by listing the members one by one (see [`MEMBER_LIST_MAX`]).
    pub const fn lists_members(self) -> bool {
        matches!(self, Self::Humans | Self::Bots | Self::RoleMembers)
    }
}

/// A counter template, trimmed, or why it cannot be used. It must say where the number goes and fit
/// a channel name with any number in it.
pub fn check_template(raw: &str) -> Result<String, FieldError> {
    let template = raw.trim();
    if !template.contains(COUNT_PLACEHOLDER) {
        return Err(FieldError::new(
            "template",
            "Put {count} where the number goes, e.g. Members: {count}.",
        ));
    }
    if template.chars().count() > TEMPLATE_MAX {
        return Err(FieldError::new(
            "template",
            format!("Keep it to {TEMPLATE_MAX} characters, so the number always fits."),
        ));
    }
    if template.chars().any(char::is_control) {
        return Err(FieldError::new("template", "A channel name is one line."));
    }
    Ok(template.to_owned())
}

/// `n` with a comma between thousands, the way counter channels show it.
pub fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if n < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The name a counter channel gets: the template with the number in it, cut to Discord's 100 characters.
pub fn counter_name(template: &str, value: i64) -> String {
    let name = template.replace(COUNT_PLACEHOLDER, &grouped(value));
    name.chars().take(CHANNEL_NAME_MAX).collect()
}

/// Whether a counter should be renamed now: its name would change, and it was not renamed in the last
/// [`RENAME_GAP`]. `last_name` is the name the bot last gave it; `None` for a counter never renamed.
pub fn rename_due(
    last_name: Option<&str>,
    last_rename_at: Option<DateTime<Utc>>,
    wanted: &str,
    now: DateTime<Utc>,
) -> bool {
    if last_name == Some(wanted) {
        return false;
    }
    last_rename_at.is_none_or(|at| now - at >= RENAME_GAP)
}

/// The hour `at` falls in, as hours since the Unix epoch: the key the counts are kept under.
pub fn hour_of(at: DateTime<Utc>) -> i64 {
    at.timestamp().div_euclid(3600)
}

/// The start of hour number `hour` (see [`hour_of`]).
pub fn hour_start(hour: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(hour * 3600, 0).single().unwrap_or_default()
}

/// The names of the counts kept for one server in one hour, as fields of one Redis hash.
pub mod field {
    pub const MESSAGES: &str = "m";
    pub const VOICE: &str = "v";
    pub const JOINS: &str = "j";
    pub const LEAVES: &str = "l";
    pub const COMMANDS: &str = "x";

    /// Messages in one channel.
    pub fn channel(id: u64) -> String {
        format!("c:{id}")
    }

    /// Voice minutes in one channel.
    pub fn channel_voice(id: u64) -> String {
        format!("cv:{id}")
    }

    /// Messages known to be in a channel visible to @everyone when counted.
    pub fn public_channel(id: u64) -> String {
        format!("pc:{id}")
    }

    /// Voice minutes known to be in a channel visible to @everyone when counted.
    pub fn public_channel_voice(id: u64) -> String {
        format!("pcv:{id}")
    }

    /// Messages of one member.
    pub fn member(id: u64) -> String {
        format!("u:{id}")
    }

    /// Voice minutes of one member.
    pub fn member_voice(id: u64) -> String {
        format!("uv:{id}")
    }
}

/// The counts for a message from `user` in `channel`.
pub fn message_fields(channel: u64, user: u64) -> Vec<(String, i64)> {
    vec![
        (field::MESSAGES.to_owned(), 1),
        (field::channel(channel), 1),
        (field::member(user), 1),
    ]
}

/// The counts for one minute of everyone in `present` (member, voice channel).
pub fn voice_fields(present: &[(u64, u64)]) -> Vec<(String, i64)> {
    if present.is_empty() {
        return Vec::new();
    }
    let mut channels: BTreeMap<u64, i64> = BTreeMap::new();
    let mut fields = vec![(field::VOICE.to_owned(), present.len() as i64)];
    for (user, channel) in present {
        *channels.entry(*channel).or_default() += 1;
        fields.push((field::member_voice(*user), 1));
    }
    fields.extend(
        channels
            .into_iter()
            .map(|(channel, n)| (field::channel_voice(channel), n)),
    );
    fields
}

/// Messages and voice minutes, of a channel or a member.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pair {
    pub messages: i64,
    pub voice_minutes: i64,
}

/// What one server did in one hour, read back from the fields of its hash.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HourTally {
    pub messages: i64,
    pub voice_minutes: i64,
    pub joins: i64,
    pub leaves: i64,
    pub commands: i64,
    pub channels: BTreeMap<u64, Pair>,
    pub public_channels: BTreeMap<u64, Pair>,
    pub members: BTreeMap<u64, Pair>,
}

impl HourTally {
    /// The tally of a hash's fields; fields it does not know (a later release's) are left out.
    pub fn from_fields(fields: &[(String, i64)]) -> Self {
        let mut tally = Self::default();
        for (name, value) in fields {
            let value = (*value).max(0);
            let id = |prefix: &str| name.strip_prefix(prefix).and_then(|id| id.parse::<u64>().ok());
            match name.as_str() {
                field::MESSAGES => tally.messages += value,
                field::VOICE => tally.voice_minutes += value,
                field::JOINS => tally.joins += value,
                field::LEAVES => tally.leaves += value,
                field::COMMANDS => tally.commands += value,
                _ => {
                    if let Some(channel) = id("c:") {
                        tally.channels.entry(channel).or_default().messages += value;
                    } else if let Some(channel) = id("cv:") {
                        tally.channels.entry(channel).or_default().voice_minutes += value;
                    } else if let Some(channel) = id("pc:") {
                        tally.public_channels.entry(channel).or_default().messages += value;
                    } else if let Some(channel) = id("pcv:") {
                        tally.public_channels.entry(channel).or_default().voice_minutes += value;
                    } else if let Some(user) = id("u:") {
                        tally.members.entry(user).or_default().messages += value;
                    } else if let Some(user) = id("uv:") {
                        tally.members.entry(user).or_default().voice_minutes += value;
                    }
                }
            }
        }
        tally
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One stored hour of a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HourRow {
    pub hour: DateTime<Utc>,
    pub messages: i64,
    pub voice_minutes: i64,
    pub joins: i64,
    pub leaves: i64,
    pub commands: i64,
}

/// One day of a server, added up from its hours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayTotals {
    pub day: NaiveDate,
    pub messages: i64,
    pub voice_minutes: i64,
    pub joins: i64,
    pub leaves: i64,
    pub commands: i64,
}

impl DayTotals {
    fn empty(day: NaiveDate) -> Self {
        Self {
            day,
            messages: 0,
            voice_minutes: 0,
            joins: 0,
            leaves: 0,
            commands: 0,
        }
    }
}

/// Every day from `first` for `days` days (UTC), each with what its hours add up to; days with nothing
/// are there with zeros, and hours outside the days are left out.
pub fn daily(rows: &[HourRow], first: NaiveDate, days: i64) -> Vec<DayTotals> {
    let mut out: Vec<DayTotals> = (0..days.max(0))
        .map(|i| DayTotals::empty(first + Duration::days(i)))
        .collect();
    for row in rows {
        let index = (row.hour.date_naive() - first).num_days();
        if let Some(day) = usize::try_from(index).ok().and_then(|i| out.get_mut(i)) {
            day.messages += row.messages;
            day.voice_minutes += row.voice_minutes;
            day.joins += row.joins;
            day.leaves += row.leaves;
            day.commands += row.commands;
        }
    }
    out
}

/// Messages by weekday and hour of the day, in UTC: `[weekday][hour]`, Monday first.
pub fn heatmap(rows: &[HourRow]) -> [[i64; 24]; 7] {
    let mut grid = [[0i64; 24]; 7];
    for row in rows {
        let weekday = row.hour.weekday().num_days_from_monday() as usize;
        grid[weekday][row.hour.hour() as usize] += row.messages;
    }
    grid
}

/// The days of `rows` added up into the totals of the whole stretch.
pub fn totals(days: &[DayTotals]) -> DayTotals {
    let first = days.first().map_or(NaiveDate::MIN, |d| d.day);
    days.iter().fold(DayTotals::empty(first), |mut sum, d| {
        sum.messages += d.messages;
        sum.voice_minutes += d.voice_minutes;
        sum.joins += d.joins;
        sum.leaves += d.leaves;
        sum.commands += d.commands;
        sum
    })
}

/// Minutes as a short duration: `45m`, `3h 20m`, `1,204h`.
pub fn minutes_text(minutes: i64, lang: Lang) -> String {
    let minutes = minutes.max(0);
    match minutes {
        0..60 => t!(lang, "stats.duration.minutes", minutes = minutes),
        60..6000 if minutes % 60 != 0 => t!(
            lang,
            "stats.duration.hours-minutes",
            hours = minutes / 60,
            minutes = minutes % 60
        ),
        _ => t!(lang, "stats.duration.hours", hours = grouped(minutes / 60)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
    }

    fn row(hour: DateTime<Utc>, messages: i64) -> HourRow {
        HourRow {
            hour,
            messages,
            voice_minutes: 0,
            joins: 0,
            leaves: 0,
            commands: 0,
        }
    }

    #[test]
    fn periods_parse_and_end_today() {
        for period in Period::ALL {
            assert_eq!(Period::parse(period.key()), Some(period));
            assert_eq!(Period::of_days(period.days()), Some(period));
        }
        let today = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        assert_eq!(
            Period::Week.first_day(today),
            NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
        );
        assert_eq!(Period::of_days(14), None);
    }

    #[test]
    fn counter_kinds_parse_back_and_have_text_in_every_language() {
        for kind in CounterKind::ALL {
            assert_eq!(CounterKind::parse(kind.key()), Some(kind));
            for lang in Lang::ALL {
                let template = kind.default_template(lang);
                assert!(template.contains(COUNT_PLACEHOLDER), "{lang:?} {kind:?}: {template}");
                assert!(check_template(&template).is_ok(), "{template}");
                assert!(!kind.name(lang).starts_with("stats."), "{lang:?} {kind:?} has no name");
            }
        }
    }

    #[test]
    fn templates_need_the_number_and_fit_a_channel_name() {
        assert_eq!(check_template("  Members: {count} ").unwrap(), "Members: {count}");
        assert_eq!(check_template("Members").unwrap_err().field, "template");
        assert!(check_template(&format!("{}{{count}}", "x".repeat(80))).is_err());
        assert!(check_template("a\n{count}").is_err());
        assert_eq!(counter_name("👥 Members: {count}", 12_345), "👥 Members: 12,345");
        assert_eq!(counter_name("{count}", -1_000), "-1,000");
        let longest = counter_name(&format!("{}{{count}}", "x".repeat(80)), i64::MAX);
        assert!(longest.chars().count() <= CHANNEL_NAME_MAX);
    }

    #[test]
    fn a_counter_is_renamed_when_its_name_changes_and_ten_minutes_have_passed() {
        let now = at(2026, 11, 6, 12);
        assert!(
            rename_due(None, None, "Members: 5", now),
            "a new counter is named at once"
        );
        assert!(!rename_due(
            Some("Members: 5"),
            Some(now - Duration::hours(3)),
            "Members: 5",
            now
        ));
        assert!(!rename_due(
            Some("Members: 5"),
            Some(now - Duration::minutes(9)),
            "Members: 6",
            now
        ));
        assert!(rename_due(
            Some("Members: 5"),
            Some(now - Duration::minutes(10)),
            "Members: 6",
            now
        ));
        assert!(rename_due(Some("Members: 5"), None, "Members: 6", now));
    }

    #[test]
    fn hours_round_trip_and_count_from_the_epoch() {
        let t = Utc.with_ymd_and_hms(2026, 11, 6, 13, 59, 59).unwrap();
        assert_eq!(hour_start(hour_of(t)), at(2026, 11, 6, 13));
        assert_eq!(hour_of(hour_start(500_000)), 500_000);
    }

    #[test]
    fn a_hash_reads_back_into_its_tally() {
        let mut fields = message_fields(7, 70);
        fields.extend(message_fields(7, 71));
        fields.extend(message_fields(8, 70));
        fields.extend(voice_fields(&[(70, 9), (72, 9), (73, 10)]));
        fields.push((field::JOINS.to_owned(), 2));
        fields.push((field::COMMANDS.to_owned(), 4));
        fields.push(("zz:later".to_owned(), 99));
        fields.push(("c:not-a-number".to_owned(), 99));
        let tally = HourTally::from_fields(&fields);
        assert_eq!(tally.messages, 3);
        assert_eq!(tally.voice_minutes, 3);
        assert_eq!((tally.joins, tally.leaves, tally.commands), (2, 0, 4));
        assert_eq!(
            tally.channels[&7],
            Pair {
                messages: 2,
                voice_minutes: 0
            }
        );
        assert_eq!(
            tally.channels[&9],
            Pair {
                messages: 0,
                voice_minutes: 2
            }
        );
        assert_eq!(
            tally.members[&70],
            Pair {
                messages: 2,
                voice_minutes: 1
            }
        );
        assert_eq!(tally.members.len(), 4);
        assert!(HourTally::from_fields(&[]).is_empty());
        assert!(voice_fields(&[]).is_empty());
    }

    #[test]
    fn heatmap_buckets_by_utc_weekday_and_hour() {
        // 2026-11-02 is a Monday; 2026-11-08 a Sunday.
        let rows = [
            row(at(2026, 11, 2, 0), 3),
            row(at(2026, 11, 2, 23), 1),
            row(at(2026, 11, 9, 0), 2),
            row(at(2026, 11, 8, 23), 5),
            row(at(2026, 11, 4, 12), 7),
        ];
        let grid = heatmap(&rows);
        assert_eq!(grid[0][0], 5, "both Mondays at midnight land in one cell");
        assert_eq!(grid[0][23], 1);
        assert_eq!(grid[6][23], 5, "Sunday is the last row");
        assert_eq!(grid[2][12], 7, "Wednesday noon");
        assert_eq!(grid.iter().flatten().sum::<i64>(), 18);
        // A time given with an offset is bucketed by its UTC hour.
        let offset = DateTime::parse_from_rfc3339("2026-11-03T01:30:00+03:00").unwrap();
        let grid = heatmap(&[row(offset.with_timezone(&Utc), 1)]);
        assert_eq!(grid[0][22], 1, "01:30 on Tuesday in UTC+3 is Monday 22:30 UTC");
    }

    #[test]
    fn days_fill_gaps_and_drop_hours_outside() {
        let first = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let rows = [
            row(at(2026, 10, 31, 23), 100),
            row(at(2026, 11, 1, 0), 2),
            row(at(2026, 11, 1, 23), 3),
            row(at(2026, 11, 3, 5), 4),
            row(at(2026, 11, 8, 0), 100),
        ];
        let days = daily(&rows, first, 7);
        assert_eq!(days.len(), 7);
        assert_eq!(
            days.iter().map(|d| d.messages).collect::<Vec<_>>(),
            vec![5, 0, 4, 0, 0, 0, 0]
        );
        assert_eq!(days[6].day, NaiveDate::from_ymd_opt(2026, 11, 7).unwrap());
        assert_eq!(totals(&days).messages, 9);
    }

    #[test]
    fn numbers_read_well() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
        assert_eq!(minutes_text(45, Lang::En), "45m");
        assert_eq!(minutes_text(200, Lang::En), "3h 20m");
        assert_eq!(minutes_text(120, Lang::En), "2h");
        assert_eq!(minutes_text(72_240, Lang::En), "1,204h");
        assert_eq!(minutes_text(200, Lang::Tr), "3 sa 20 dk");
    }

    #[test]
    fn public_channel_fields_stay_separate_from_mixed_history() {
        let tally = HourTally::from_fields(&[
            (field::channel(42), 9),
            (field::public_channel(42), 2),
            (field::public_channel_voice(42), 3),
        ]);
        assert_eq!(tally.channels[&42].messages, 9);
        assert_eq!(
            tally.public_channels[&42],
            Pair {
                messages: 2,
                voice_minutes: 3
            }
        );
    }
}
