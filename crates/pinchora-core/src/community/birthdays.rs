//! Calendar and privacy rules for opt-in birthdays.

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

pub const MAX_MESSAGE: usize = 500;
pub const MAX_GIFT: i16 = 500;
pub const DEFAULT_MESSAGE: &str = "Happy birthday, {user}! 🎂";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BirthdaySettings {
    pub enabled: bool,
    pub channel_id: Option<u64>,
    pub role_id: Option<u64>,
    pub message: String,
    pub announce_hour: i16,
    pub server_tz: String,
    pub shells_gift: i16,
    pub allow_age: bool,
    pub badge: bool,
}

impl Default for BirthdaySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            channel_id: None,
            role_id: None,
            message: DEFAULT_MESSAGE.into(),
            announce_hour: 0,
            server_tz: "UTC".into(),
            shells_gift: 0,
            allow_age: false,
            badge: false,
        }
    }
}

pub fn valid_date(month: u32, day: u32, year: Option<i32>) -> bool {
    NaiveDate::from_ymd_opt(year.unwrap_or(2000), month, day).is_some()
}

pub fn timezone(text: &str) -> Option<Tz> {
    text.parse().ok()
}

pub fn birthday_this_year(month: u32, day: u32, year: i32) -> Option<NaiveDate> {
    let actual_day = if month == 2 && day == 29 && NaiveDate::from_ymd_opt(year, 2, 29).is_none() {
        28
    } else {
        day
    };
    NaiveDate::from_ymd_opt(year, month, actual_day)
}

pub fn local_date(now: DateTime<Utc>, tz: Tz) -> NaiveDate {
    now.with_timezone(&tz).date_naive()
}

/// The first local hour of a day, including zones that jump over 00:00 at a DST change.
pub fn local_midnight_window(now: DateTime<Utc>, tz: Tz) -> bool {
    local_date(now - chrono::Duration::hours(1), tz) != local_date(now, tz)
}

pub fn next_date(month: u32, day: u32, now: DateTime<Utc>, tz: Tz) -> Option<NaiveDate> {
    let today = local_date(now, tz);
    let this = birthday_this_year(month, day, today.year())?;
    if this >= today {
        Some(this)
    } else {
        birthday_this_year(month, day, today.year() + 1)
    }
}

pub fn visible_age(birth_year: Option<i32>, show_age: bool, allow_age: bool, current_year: i32) -> Option<i32> {
    if show_age && allow_age {
        birth_year.and_then(|year| (year <= current_year).then_some(current_year - year))
    } else {
        None
    }
}

pub fn check_settings(s: &BirthdaySettings) -> Result<(), &'static str> {
    if timezone(&s.server_tz).is_none() {
        return Err("timezone");
    }
    if s.announce_hour != 0 {
        return Err("hour");
    }
    if !(0..=MAX_GIFT).contains(&s.shells_gift) {
        return Err("gift");
    }
    if s.message.chars().count() > MAX_MESSAGE
        || !s.message.contains("{user}")
        || s.message.contains("@everyone")
        || s.message.contains("@here")
    {
        return Err("message");
    }
    if s.channel_id.is_some_and(|id| id == 0 || id > i64::MAX as u64) {
        return Err("channel_id");
    }
    if s.role_id.is_some_and(|id| id == 0 || id > i64::MAX as u64) {
        return Err("role_id");
    }
    if s.enabled && s.channel_id.is_none() {
        return Err("channel");
    }
    Ok(())
}

pub fn format_message(template: &str, user: u64, age: Option<i32>) -> String {
    let message = template
        .replace("{user}", &format!("<@{user}>"))
        .replace("{age}", &age.map_or_else(|| "".into(), |n| n.to_string()));
    truncate_message(&message, 1800)
}

/// Discord limits content by UTF-16 units; template expansion can exceed its input limit.
pub fn truncate_message(message: &str, limit: usize) -> String {
    if message.encode_utf16().count() <= limit {
        return message.to_owned();
    }
    let mut units = 0;
    let mut text: String = message
        .chars()
        .take_while(|c| {
            units += c.len_utf16();
            units < limit
        })
        .collect();
    text.push('…');
    text
}

#[cfg(test)]
mod tests {
    use crate::community::birthdays::*;
    #[test]
    fn odd_offsets_and_skipped_midnight() {
        for (zone, instant) in [
            ("Asia/Kathmandu", "2026-01-01T18:20:00Z"),
            ("Pacific/Tongatapu", "2026-01-01T11:05:00Z"),
            ("America/Santiago", "2026-09-06T04:05:00Z"),
        ] {
            let now = DateTime::parse_from_rfc3339(instant).unwrap().with_timezone(&Utc);
            let tz = timezone(zone).unwrap();
            assert!(local_midnight_window(now, tz), "{zone}");
            assert_eq!(
                next_date(local_date(now, tz).month(), local_date(now, tz).day(), now, tz),
                Some(local_date(now, tz))
            );
        }
    }
    #[test]
    fn expanded_messages_and_ids_are_bounded() {
        let message = format_message(&"{user}".repeat(83), u64::MAX, None);
        assert!(message.encode_utf16().count() <= 1800);
        assert!(truncate_message(&"🎂".repeat(1000), 1900).encode_utf16().count() <= 1900);
        for id in [0, u64::MAX] {
            assert!(
                check_settings(&BirthdaySettings {
                    role_id: Some(id),
                    ..Default::default()
                })
                .is_err()
            );
            assert!(
                check_settings(&BirthdaySettings {
                    channel_id: Some(id),
                    ..Default::default()
                })
                .is_err()
            );
        }
    }
    #[test]
    fn leap_day_and_privacy() {
        assert_eq!(birthday_this_year(2, 29, 2025), NaiveDate::from_ymd_opt(2025, 2, 28));
        assert_eq!(birthday_this_year(2, 29, 2024), NaiveDate::from_ymd_opt(2024, 2, 29));
        assert!(valid_date(2, 29, None));
        assert!(!valid_date(2, 30, None));
        assert_eq!(visible_age(Some(2000), true, false, 2026), None);
        assert_eq!(visible_age(Some(2000), true, true, 2026), Some(26));
    }
    #[test]
    fn local_calendar_survives_dst() {
        let ny: Tz = "America/New_York".parse().unwrap();
        let before = DateTime::parse_from_rfc3339("2026-03-08T04:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let after = DateTime::parse_from_rfc3339("2026-03-08T05:01:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_ne!(local_date(before, ny), local_date(after, ny));
        assert!(local_midnight_window(after, ny));
        assert!(!local_midnight_window(after + chrono::Duration::hours(1), ny));
    }
}
