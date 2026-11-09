//! Event dates, RSVP options and recurrence rules shared by the bot and dashboard.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

pub const MAX_TITLE: usize = 100;
pub const MAX_DESCRIPTION: usize = 1000;
pub const MAX_OPTIONS: usize = 5;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RsvpOption {
    pub key: String,
    pub label: String,
    /// Zero means no capacity limit.
    pub capacity: i32,
}

pub fn default_options() -> Vec<RsvpOption> {
    vec![
        RsvpOption {
            key: "going".into(),
            label: "Going".into(),
            capacity: 0,
        },
        RsvpOption {
            key: "maybe".into(),
            label: "Maybe".into(),
            capacity: 0,
        },
        RsvpOption {
            key: "declined".into(),
            label: "Declined".into(),
            capacity: 0,
        },
    ]
}

pub fn check_options(options: &[RsvpOption]) -> bool {
    (3..=MAX_OPTIONS).contains(&options.len())
        && options.iter().enumerate().all(|(i, o)| {
            !o.key.is_empty()
                && o.key.len() <= 20
                && o.key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                && !o.label.trim().is_empty()
                && o.label.chars().count() <= 40
                && o.capacity >= 0
                && (i >= 3 || o.key == ["going", "maybe", "declined"][i])
                && options[..i].iter().all(|prior| prior.key != o.key)
        })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Repeat {
    Daily,
    Weekly,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Recurrence {
    pub repeat: Repeat,
    pub until: NaiveDate,
}

fn local(tz: Tz, date: NaiveDate, time: NaiveTime) -> Result<DateTime<Utc>, &'static str> {
    match tz.from_local_datetime(&date.and_time(time)) {
        LocalResult::Single(at) => Ok(at.with_timezone(&Utc)),
        _ => Err("events.ambiguous-time"),
    }
}

/// Understands an absolute local time, tomorrow/yarın, a weekday, or a relative duration.
pub fn parse_when(input: &str, now: DateTime<Utc>, tz: Tz) -> Result<DateTime<Utc>, &'static str> {
    let at = parse_local_when(input, now, tz)?;
    if at <= now {
        return Err("events.bad-time");
    }
    Ok(at)
}

fn parse_local_when(input: &str, now: DateTime<Utc>, tz: Tz) -> Result<DateTime<Utc>, &'static str> {
    let raw = input.trim().to_lowercase();
    if let Some((date, clock)) = raw.split_once(' ')
        && let Ok(date) = NaiveDate::parse_from_str(date, "%Y-%m-%d")
    {
        return local(tz, date, parse_clock(clock)?);
    }
    let words: Vec<&str> = raw.split_whitespace().collect();
    let relative = match words.as_slice() {
        ["in", hours] => hours.strip_suffix('h').and_then(|n| n.parse::<i64>().ok()),
        ["in", hours, "hour" | "hours" | "later"] | [hours, "saat", "sonra"] => hours.parse::<i64>().ok(),
        _ => None,
    };
    if let Some(hours) = relative.filter(|n| (1..=24 * 365).contains(n)) {
        return Ok(now + Duration::hours(hours));
    }
    if !(2..=3).contains(&words.len()) {
        return Err("events.bad-time");
    }
    let clock = words[1..].join(" ");
    let time = parse_clock(&clock)?;
    let today = now.with_timezone(&tz).date_naive();
    if matches!(words[0], "today" | "bugün" | "bugun") {
        return local(tz, today, time);
    }
    if matches!(words[0], "tomorrow" | "yarın" | "yarin") {
        return local(tz, today.succ_opt().ok_or("events.bad-time")?, time);
    }
    let weekday = match words[0] {
        "monday" | "pazartesi" => Weekday::Mon,
        "tuesday" | "salı" | "sali" => Weekday::Tue,
        "wednesday" | "çarşamba" | "carsamba" => Weekday::Wed,
        "thursday" | "perşembe" | "persembe" => Weekday::Thu,
        "friday" | "cuma" => Weekday::Fri,
        "saturday" | "cumartesi" => Weekday::Sat,
        "sunday" | "pazar" => Weekday::Sun,
        _ => return Err("events.bad-time"),
    };
    let days = (weekday.num_days_from_monday() as i64 - today.weekday().num_days_from_monday() as i64).rem_euclid(7);
    let mut date = today + Duration::days(days);
    let mut at = local(tz, date, time)?;
    if at <= now {
        date += Duration::days(7);
        at = local(tz, date, time)?;
    }
    Ok(at)
}

fn parse_clock(raw: &str) -> Result<NaiveTime, &'static str> {
    if let Ok(time) = NaiveTime::parse_from_str(raw, "%H:%M") {
        return Ok(time);
    }
    let (clock, afternoon) = if let Some(clock) = raw.strip_suffix("am") {
        (clock, false)
    } else if let Some(clock) = raw.strip_suffix("pm") {
        (clock, true)
    } else {
        return Err("events.bad-time");
    };
    let clock = clock.trim();
    let (hour, minute) = clock.split_once(':').unwrap_or((clock, "0"));
    let hour: u32 = hour.parse().map_err(|_| "events.bad-time")?;
    let minute: u32 = minute.parse().map_err(|_| "events.bad-time")?;
    if !(1..=12).contains(&hour) {
        return Err("events.bad-time");
    }
    NaiveTime::from_hms_opt(hour % 12 + if afternoon { 12 } else { 0 }, minute, 0).ok_or("events.bad-time")
}

/// The next local wall-clock occurrence, or none after the inclusive end date.
pub fn next_occurrence(starts_at: DateTime<Utc>, tz: Tz, recurrence: &Recurrence) -> Option<DateTime<Utc>> {
    let at = starts_at.with_timezone(&tz);
    let days = match recurrence.repeat {
        Repeat::Daily => 1,
        Repeat::Weekly => 7,
    };
    let mut date = at.date_naive();
    loop {
        date = date.checked_add_signed(Duration::days(days))?;
        if date > recurrence.until {
            return None;
        }
        // A missing or ambiguous wall clock skips this date, not the rest of the series.
        if let Ok(next) = local(tz, date, at.time()) {
            return Some(next);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::community::events::*;
    #[test]
    fn english_and_turkish_local_times() {
        let now = DateTime::parse_from_rfc3339("2026-11-05T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let tz: Tz = "Europe/Istanbul".parse().unwrap();
        let tomorrow = parse_when("yarın 20:00", now, tz).unwrap();
        assert_eq!(tomorrow, parse_when("tomorrow 20:00", now, tz).unwrap());
        assert_eq!(tomorrow, parse_when("2026-11-06 20:00", now, tz).unwrap());
        assert_eq!(parse_when("2 saat sonra", now, tz).unwrap(), now + Duration::hours(2));
        assert_eq!(parse_when("in 2h", now, tz).unwrap(), now + Duration::hours(2));
        assert_eq!(
            parse_when("cuma 21:00", now, tz).unwrap(),
            parse_when("friday 21:00", now, tz).unwrap()
        );
    }
    #[test]
    fn recurrence_preserves_local_time_and_stops() {
        let tz: Tz = "Europe/London".parse().unwrap();
        let start = DateTime::parse_from_rfc3339("2026-03-28T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let rule = Recurrence {
            repeat: Repeat::Daily,
            until: NaiveDate::from_ymd_opt(2026, 3, 29).unwrap(),
        };
        assert_eq!(
            next_occurrence(start, tz, &rule).unwrap().to_rfc3339(),
            "2026-03-29T19:00:00+00:00"
        );
        assert!(next_occurrence(next_occurrence(start, tz, &rule).unwrap(), tz, &rule).is_none());
    }
    #[test]
    fn option_keys_are_safe_for_component_ids() {
        let mut options = default_options();
        options.push(RsvpOption {
            key: "extra:1".into(),
            label: "Bringing snacks".into(),
            capacity: 2,
        });
        assert!(!check_options(&options));
        options[3].key = "extra_1".into();
        assert!(check_options(&options));
    }
}

/// Neutralise Discord mentions without expanding the stored character budget.
pub fn safe_text(input: &str) -> String {
    input.replace('@', "＠").replace("<#", "＜#")
}

#[cfg(test)]
mod review_tests {
    use crate::community::events::*;
    #[test]
    fn future_time_table_and_dst_rejection() {
        let now = DateTime::parse_from_rfc3339("2026-11-05T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let tz = "Europe/Istanbul".parse().unwrap();
        for text in ["bugün 20:00", "today 8:00 pm", "today 8pm", "2026-11-05 8:00 PM"] {
            assert_eq!(
                parse_when(text, now, tz).unwrap_or_else(|e| panic!("{text}: {e}")),
                now + Duration::hours(5),
                "{text}"
            );
        }
        for text in ["bugün 14:00", "2026-11-04 20:00", "in 0h", "-2 saat sonra"] {
            assert!(parse_when(text, now, tz).is_err(), "{text}");
        }
        let before = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let london = "Europe/London".parse().unwrap();
        for text in ["2026-03-29 01:30", "2026-10-25 01:30"] {
            assert_eq!(parse_when(text, before, london), Err("events.ambiguous-time"));
        }
    }
    #[test]
    fn recurrence_skips_dst_gap_and_fold_without_ending_the_series() {
        let zone = "Europe/London".parse().unwrap();
        for (start, expected) in [
            ("2026-03-28T01:30:00Z", "2026-03-30T00:30:00Z"),
            ("2026-10-24T00:30:00Z", "2026-10-26T01:30:00Z"),
        ] {
            let start = DateTime::parse_from_rfc3339(start).unwrap().with_timezone(&Utc);
            let rule = Recurrence {
                repeat: Repeat::Daily,
                until: (start + Duration::days(3)).date_naive(),
            };
            assert_eq!(
                next_occurrence(start, zone, &rule).unwrap(),
                DateTime::parse_from_rfc3339(expected).unwrap()
            );
            let expired = Recurrence {
                until: (start + Duration::days(1)).date_naive(),
                ..rule
            };
            assert!(next_occurrence(start, zone, &expired).is_none());
        }
    }
    #[test]
    fn neutralises_every_mention_without_exceeding_limits() {
        let text = "@everyone @here <@123> <@!123> <@&456> <#789>";
        let safe = safe_text(text);
        assert!(!safe.contains('@') && !safe.contains("<#"));
        assert_eq!(safe.chars().count(), text.chars().count());
    }
}
