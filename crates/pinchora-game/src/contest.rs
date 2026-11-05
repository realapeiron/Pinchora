//! The weekly room contest: themes and week arithmetic. Weeks start Monday 00:00 UTC.

use std::sync::LazyLock;

use serde::Deserialize;

const DAY: i64 = 86_400;
const WEEK: i64 = 7 * DAY;
/// 1970-01-05, the first Monday after the Unix epoch: week 0 starts here.
const FIRST_MONDAY: i64 = 4 * DAY;

/// Shells for first, second and third place.
pub const PRIZES: [i64; 3] = [500, 250, 100];
/// The title the week's winner earns.
pub const WINNER_TITLE: &str = "house_of_the_week";

/// What a week's rooms should be about.
#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub description: String,
}

static THEMES: LazyLock<Vec<Theme>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../content/contest_themes.json"))
        .expect("content/contest_themes.json must parse; run `cargo test -p pinchora-game`")
});

pub fn themes() -> &'static [Theme] {
    &THEMES
}

/// The contest week a Unix time falls in.
pub fn week_of(unix: i64) -> i64 {
    (unix - FIRST_MONDAY).div_euclid(WEEK)
}

/// When a week starts, in Unix seconds; it ends when the next one starts.
pub fn week_start(week: i64) -> i64 {
    FIRST_MONDAY + week * WEEK
}

/// The week's theme, the same in every server; the list repeats in order.
pub fn theme(week: i64) -> &'static Theme {
    let themes = themes();
    &themes[week.rem_euclid(themes.len() as i64) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-02 00:00:00 UTC, a Monday.
    const MONDAY: i64 = 1_793_577_600;

    #[test]
    fn weeks_start_on_monday_midnight_utc() {
        let week = week_of(MONDAY);
        assert_eq!(week_start(week), MONDAY);
        assert_eq!(week_of(MONDAY - 1), week - 1, "Sunday 23:59:59 is the week before");
        assert_eq!(week_of(MONDAY + WEEK - 1), week, "until the next Sunday ends");
        assert_eq!(week_of(MONDAY + WEEK), week + 1);
        assert_eq!(week_of(FIRST_MONDAY), 0);
        assert_eq!(week_of(0), -1, "the epoch was a Thursday, before week 0");
    }

    #[test]
    fn themes_are_plenty_and_distinct() {
        let themes = themes();
        assert!(themes.len() >= 40, "{} themes", themes.len());
        let mut ids: Vec<&str> = themes.iter().map(|t| t.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), themes.len(), "theme ids are unique");
        for t in themes {
            assert!(!t.name.is_empty() && t.name.chars().count() <= 40, "{}", t.name);
            assert!(
                !t.description.is_empty() && t.description.chars().count() <= 100,
                "{}",
                t.id
            );
        }
        // Consecutive weeks get different themes, and the pick only depends on the week.
        assert_ne!(theme(10).id, theme(11).id);
        assert_eq!(theme(10).id, theme(10 + themes.len() as i64).id);
        assert_eq!(theme(-1).id, themes.last().unwrap().id);
    }
}
