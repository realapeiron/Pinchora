//! When `/wrapped` is open and the fun title a year earns.

use chrono::{DateTime, Datelike, TimeZone, Utc};

/// The year whose Wrapped can be looked at on `now`: December of a year opens it, and it stays
/// open through January. `None` the rest of the year.
pub fn open_year(now: DateTime<Utc>) -> Option<i32> {
    match now.month() {
        12 => Some(now.year()),
        1 => Some(now.year() - 1),
        _ => None,
    }
}

/// When Wrapped opens next: the first of December. Only meaningful while it is closed.
pub fn opens_at(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(now.year(), 12, 1, 0, 0, 0).single().unwrap_or(now)
}

/// What a year came to, for [`title`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct YearTotals {
    pub messages: i64,
    pub shells: i64,
    pub fish: i64,
    pub visits: i64,
    pub likes: i64,
    pub pet_care: i64,
}

/// A line for the card: what the player did most, measured against what a busy year looks like.
pub fn title(t: YearTotals) -> &'static str {
    // A year of a keen player; a count is worth a title when it gets near its yardstick.
    let scores = [
        (t.messages as f64 / 3_000.0, "The Chatterbox"),
        (t.fish as f64 / 200.0, "Tide Whisperer"),
        (t.visits as f64 / 150.0, "Social Butterfly"),
        (t.likes as f64 / 200.0, "House Cheerleader"),
        (t.pet_care as f64 / 300.0, "Pet Whisperer"),
        (t.shells as f64 / 50_000.0, "Shell Tycoon"),
    ];
    let best = scores
        .iter()
        .fold((0.0, "Fresh Pearl"), |a, b| if b.0 > a.0 { *b } else { a });
    if best.0 < 0.05 { "Fresh Pearl" } else { best.1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    #[test]
    fn wrapped_is_open_from_december_to_the_end_of_january() {
        assert_eq!(open_year(at(2026, 12, 1)), Some(2026));
        assert_eq!(open_year(at(2026, 12, 31)), Some(2026));
        assert_eq!(open_year(at(2027, 1, 1)), Some(2026));
        assert_eq!(open_year(at(2027, 1, 31)), Some(2026));
        for month in 2..=11 {
            assert_eq!(open_year(at(2027, month, 15)), None, "month {month}");
        }
        assert_eq!(
            opens_at(at(2027, 2, 1)),
            Utc.with_ymd_and_hms(2027, 12, 1, 0, 0, 0).unwrap()
        );
        assert_eq!(
            opens_at(at(2027, 11, 30)),
            Utc.with_ymd_and_hms(2027, 12, 1, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn the_title_follows_what_was_done_most() {
        assert_eq!(title(YearTotals::default()), "Fresh Pearl");
        assert_eq!(
            title(YearTotals {
                messages: 6_000,
                fish: 30,
                ..Default::default()
            }),
            "The Chatterbox"
        );
        assert_eq!(
            title(YearTotals {
                fish: 400,
                messages: 100,
                ..Default::default()
            }),
            "Tide Whisperer"
        );
        assert_eq!(
            title(YearTotals {
                pet_care: 330,
                shells: 10_000,
                ..Default::default()
            }),
            "Pet Whisperer"
        );
        assert_eq!(
            title(YearTotals {
                messages: 2,
                ..Default::default()
            }),
            "Fresh Pearl"
        );
    }
}
