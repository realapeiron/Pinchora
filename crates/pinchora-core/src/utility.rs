//! Validated message text and recurrence math shared by commands and the dashboard.

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

pub const MAX_SCHEDULES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    Once,
    Interval {
        seconds: i64,
    },
    Daily {
        hour: u32,
        minute: u32,
    },
    Weekly {
        weekday: u32,
        hour: u32,
        minute: u32,
    },
    /// Five fields: minute hour day-of-month month weekday; `*`, a number or `*/step`.
    Cron {
        minute: String,
        hour: String,
        day: String,
        month: String,
        weekday: String,
    },
}

pub fn offset(text: &str) -> Option<FixedOffset> {
    let bytes = text.as_bytes();
    if bytes.len() != 6 || bytes[3] != b':' || !matches!(bytes[0], b'+' | b'-') {
        return None;
    }
    let h: i32 = text[1..3].parse().ok()?;
    let m: i32 = text[4..6].parse().ok()?;
    if h > 14 || m > 59 || (h == 14 && m != 0) {
        return None;
    }
    FixedOffset::east_opt((if bytes[0] == b'-' { -1 } else { 1 }) * (h * 3600 + m * 60))
}

pub fn clean(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

pub fn validate(schedule: &Schedule) -> bool {
    match schedule {
        Schedule::Once => true,
        Schedule::Interval { seconds } => (60..=31_536_000).contains(seconds),
        Schedule::Daily { hour, minute } => *hour < 24 && *minute < 60,
        Schedule::Weekly { weekday, hour, minute } => *weekday < 7 && *hour < 24 && *minute < 60,
        Schedule::Cron {
            minute,
            hour,
            day,
            month,
            weekday,
        } => [
            (&**minute, 0, 59),
            (&**hour, 0, 23),
            (&**day, 1, 31),
            (&**month, 1, 12),
            (&**weekday, 0, 6),
        ]
        .iter()
        .all(|(s, lo, hi)| {
            *s == "*"
                || s.parse::<u32>().is_ok_and(|n| (*lo..=*hi).contains(&n))
                || s.strip_prefix("*/")
                    .and_then(|v| v.parse::<u32>().ok())
                    .is_some_and(|step| step > 0 && step <= hi - lo + 1)
        }),
    }
}

fn field(value: &str, n: u32, start: u32) -> bool {
    value == "*"
        || value.parse::<u32>() == Ok(n)
        || value
            .strip_prefix("*/")
            .and_then(|v| v.parse::<u32>().ok())
            .is_some_and(|step| step > 0 && (n - start).is_multiple_of(step))
}

pub fn next_run(schedule: &Schedule, tz: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    if !validate(schedule) {
        return None;
    }
    match schedule {
        Schedule::Once => None,
        Schedule::Interval { seconds } => after.checked_add_signed(Duration::seconds(*seconds)),
        _ => {
            let zone = offset(tz)?;
            let local = after.with_timezone(&zone);
            let start = local.date_naive();
            for d in 0..=1462 {
                let date: NaiveDate = start.checked_add_signed(Duration::days(d))?;
                let matches = |hour, minute| {
                    date.and_hms_opt(hour, minute, 0)
                        .and_then(|n| zone.from_local_datetime(&n).single())
                        .map(|v| v.with_timezone(&Utc))
                        .filter(|v| *v > after)
                };
                let found = match schedule {
                    Schedule::Daily { hour, minute } => matches(*hour, *minute),
                    Schedule::Weekly { weekday, hour, minute } if date.weekday().num_days_from_sunday() == *weekday => {
                        matches(*hour, *minute)
                    }
                    Schedule::Cron {
                        minute,
                        hour,
                        day,
                        month,
                        weekday,
                    } if field(day, date.day(), 1)
                        && field(month, date.month(), 1)
                        && field(weekday, date.weekday().num_days_from_sunday(), 0) =>
                    {
                        let mut result = None;
                        for h in 0..24 {
                            for m in 0..60 {
                                if field(hour, h, 0)
                                    && field(minute, m, 0)
                                    && let Some(v) = matches(h, m)
                                {
                                    result = Some(v);
                                    break;
                                }
                            }
                            if result.is_some() {
                                break;
                            }
                        }
                        result
                    }
                    _ => None,
                };
                if found.is_some() {
                    return found;
                }
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn month_end_and_offset() {
        let at = Utc.with_ymd_and_hms(2026, 1, 31, 22, 0, 0).unwrap();
        assert_eq!(
            next_run(&Schedule::Daily { hour: 9, minute: 0 }, "+03:00", at).unwrap(),
            Utc.with_ymd_and_hms(2026, 2, 1, 6, 0, 0).unwrap()
        );
        assert_eq!(
            next_run(
                &Schedule::Cron {
                    minute: "0".into(),
                    hour: "9".into(),
                    day: "1".into(),
                    month: "*".into(),
                    weekday: "*".into()
                },
                "+03:00",
                at
            )
            .unwrap(),
            Utc.with_ymd_and_hms(2026, 2, 1, 6, 0, 0).unwrap()
        );
    }

    #[test]
    fn recurrence_boundaries() {
        let at = Utc.with_ymd_and_hms(2026, 3, 29, 0, 59, 0).unwrap();
        assert_eq!(
            next_run(&Schedule::Interval { seconds: 60 }, "+03:00", at),
            Some(at + Duration::minutes(1))
        );
        assert_eq!(
            next_run(
                &Schedule::Weekly {
                    weekday: 0,
                    hour: 4,
                    minute: 0
                },
                "+03:00",
                at
            ),
            Some(Utc.with_ymd_and_hms(2026, 3, 29, 1, 0, 0).unwrap())
        );
        let cron = Schedule::Cron {
            minute: "*/15".into(),
            hour: "*".into(),
            day: "*".into(),
            month: "*".into(),
            weekday: "*".into(),
        };
        assert_eq!(
            next_run(&cron, "+03:00", at),
            Some(Utc.with_ymd_and_hms(2026, 3, 29, 1, 0, 0).unwrap())
        );
        let alternate_days = Schedule::Cron {
            minute: "0".into(),
            hour: "9".into(),
            day: "*/2".into(),
            month: "*".into(),
            weekday: "*".into(),
        };
        assert_eq!(
            next_run(
                &alternate_days,
                "+00:00",
                Utc.with_ymd_and_hms(2026, 3, 1, 9, 0, 0).unwrap()
            ),
            Some(Utc.with_ymd_and_hms(2026, 3, 3, 9, 0, 0).unwrap())
        );
        assert!(offset("+14:01").is_none());
        assert!(!validate(&Schedule::Interval { seconds: 30 }));
    }
}
