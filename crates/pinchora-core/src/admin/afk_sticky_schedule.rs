//! Validated message text and recurrence math shared by commands and the dashboard.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

pub const MAX_SCHEDULES: usize = 20;

pub fn pause_text(lang: crate::infra::i18n::Lang, reason: &str) -> String {
    crate::t!(
        lang,
        match reason {
            "channel_deleted" => "utility.pause-channel",
            "permissions_lost" => "utility.pause-permissions",
            "invalid_schedule" => "utility.pause-invalid",
            _ => "utility.pause-uncertain",
        }
    )
}

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

/// Parse a stored IANA zone name; fixed numeric offsets are not accepted.
pub fn timezone(text: &str) -> Option<chrono_tz::Tz> {
    text.parse().ok()
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
            let zone = timezone(tz)?;
            let local = after.with_timezone(&zone);
            let start = local.date_naive();
            // A leap day can be eight years away across a non-leap century (2100).
            for d in 0..=2922 {
                let date: NaiveDate = start.checked_add_signed(Duration::days(d))?;
                let matches = |hour, minute| {
                    date.and_hms_opt(hour, minute, 0)
                        .and_then(|n| zone.from_local_datetime(&n).earliest())
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

/// Preview the same coalesced backlog the sender uses: one missed run, then future runs.
pub fn preview_runs(schedule: &Schedule, tz: &str, first: DateTime<Utc>, now: DateTime<Utc>) -> Vec<DateTime<Utc>> {
    let mut at = first.max(now);
    let mut runs = vec![at];
    for _ in 0..4 {
        let Some(next) = next_run(schedule, tz, at) else { break };
        at = next;
        runs.push(at);
    }
    runs
}

#[cfg(test)]
mod tests {
    use crate::admin::afk_sticky_schedule::*;
    #[test]
    fn preview_coalesces_outages_instead_of_showing_five_missed_runs() {
        let now = Utc.with_ymd_and_hms(2028, 3, 1, 9, 0, 0).unwrap();
        let past = now - Duration::days(10);
        let runs = preview_runs(&Schedule::Interval { seconds: 60 }, "Europe/Istanbul", past, now);
        assert_eq!(runs, (0..5).map(|n| now + Duration::minutes(n)).collect::<Vec<_>>());
        assert_eq!(preview_runs(&Schedule::Once, "Europe/Istanbul", past, now), vec![now]);
    }

    #[test]
    fn untrusted_zones_and_leap_century() {
        for bad in ["+💥0", "+0é:0", "+-1:00", "+00:-1", "+15:00", "Not/AZone"] {
            assert!(timezone(bad).is_none(), "{bad}");
        }
        let rule = Schedule::Cron {
            minute: "0".into(),
            hour: "0".into(),
            day: "29".into(),
            month: "2".into(),
            weekday: "*".into(),
        };
        assert_eq!(
            next_run(&rule, "UTC", Utc.with_ymd_and_hms(2096, 3, 1, 0, 0, 0).unwrap()),
            Some(Utc.with_ymd_and_hms(2104, 2, 29, 0, 0, 0).unwrap())
        );
        let daily = Schedule::Daily { hour: 9, minute: 0 };
        // Daily local time follows the spring DST transition.
        assert_eq!(
            next_run(
                &daily,
                "Europe/Berlin",
                Utc.with_ymd_and_hms(2026, 3, 29, 0, 0, 0).unwrap()
            ),
            Some(Utc.with_ymd_and_hms(2026, 3, 29, 7, 0, 0).unwrap())
        );
    }

    #[test]
    fn month_end_and_timezone() {
        let at = Utc.with_ymd_and_hms(2026, 1, 31, 22, 0, 0).unwrap();
        assert_eq!(
            next_run(&Schedule::Daily { hour: 9, minute: 0 }, "Europe/Istanbul", at).unwrap(),
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
                "Europe/Istanbul",
                at
            )
            .unwrap(),
            Utc.with_ymd_and_hms(2026, 2, 1, 6, 0, 0).unwrap()
        );
    }

    #[test]
    fn dst_gaps_skip_and_folds_run_once() {
        let daily = Schedule::Daily { hour: 2, minute: 30 };
        assert_eq!(
            next_run(
                &daily,
                "Europe/Berlin",
                Utc.with_ymd_and_hms(2026, 3, 29, 0, 0, 0).unwrap()
            ),
            Some(Utc.with_ymd_and_hms(2026, 3, 30, 0, 30, 0).unwrap())
        );
        let first = Utc.with_ymd_and_hms(2026, 10, 25, 0, 30, 0).unwrap();
        assert_eq!(
            next_run(&daily, "Europe/Berlin", first - Duration::minutes(1)),
            Some(first)
        );
        assert_eq!(
            next_run(&daily, "Europe/Berlin", first),
            Some(Utc.with_ymd_and_hms(2026, 10, 26, 1, 30, 0).unwrap())
        );
        assert!(timezone("+03:00").is_none());
    }

    #[test]
    fn recurrence_boundaries() {
        let at = Utc.with_ymd_and_hms(2026, 3, 29, 0, 59, 0).unwrap();
        assert_eq!(
            next_run(&Schedule::Interval { seconds: 60 }, "Europe/Istanbul", at),
            Some(at + Duration::minutes(1))
        );
        assert_eq!(
            next_run(
                &Schedule::Weekly {
                    weekday: 0,
                    hour: 4,
                    minute: 0
                },
                "Europe/Istanbul",
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
            next_run(&cron, "Europe/Istanbul", at),
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
                "UTC",
                Utc.with_ymd_and_hms(2026, 3, 1, 9, 0, 0).unwrap()
            ),
            Some(Utc.with_ymd_and_hms(2026, 3, 3, 9, 0, 0).unwrap())
        );
        assert!(timezone("+14:01").is_none());
        assert!(!validate(&Schedule::Interval { seconds: 30 }));
    }
}
