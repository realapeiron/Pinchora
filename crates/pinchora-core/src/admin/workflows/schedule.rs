use crate::admin::workflows::*;
use chrono::{Datelike, Duration, Timelike};
impl Schedule {
    pub fn next(&self, after: DateTime<Utc>, tz: &str) -> Option<DateTime<Utc>> {
        let tz: chrono_tz::Tz = tz.parse().ok()?;
        match *self {
            Self::Interval { minutes } if (1..=525600).contains(&minutes) => {
                after.checked_add_signed(Duration::minutes(minutes.into()))
            }
            Self::Daily { hour, minute } | Self::Weekly { hour, minute, .. } if hour < 24 && minute < 60 => {
                // Iterate UTC minutes: nonexistent local times are skipped; a repeated local minute fires only once.
                let start = after.with_second(0)?.with_nanosecond(0)?;
                for offset in 1..=15 * 24 * 60 {
                    let next = start.checked_add_signed(Duration::minutes(offset))?;
                    let local = next.with_timezone(&tz);
                    let day = match self {
                        Self::Weekly { weekday, .. } => {
                            *weekday < 7 && local.weekday().num_days_from_monday() == *weekday
                        }
                        _ => true,
                    };
                    if day && local.hour() == hour && local.minute() == minute {
                        let naive = local.naive_local();
                        use chrono::TimeZone;
                        if tz.from_local_datetime(&naive).earliest()?.with_timezone(&Utc) == next {
                            return Some(next);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }
}
