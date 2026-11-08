//! Shared, bounded rules for the fish–house–pet loop. UTC clocks; ingredients cannot be traded.
use chrono::{Datelike, NaiveDate};

pub const SEED_COST: i64 = 40;
pub const GROW_SECS: i64 = 8 * 3600;
pub const WATER_SECS: i64 = 3600;
pub const HARVEST: i32 = 4;
pub const DIVE_COST: i64 = 300;
pub const DIVE_SECS: i64 = 90;
pub const REQUEST_SECS: i64 = 8 * 3600;
pub const INGREDIENT_CAP: i32 = 999;

#[derive(Debug, Clone, Copy)]
pub struct Recipe {
    pub id: &'static str,
    pub item: &'static str,
    pub seaweed: i32,
    pub fish: i64,
    pub pearls: i64,
    pub shells: i64,
}
pub const RECIPES: &[Recipe] = &[
    Recipe {
        id: "sprig",
        item: "sprigstep_planter_box__mint",
        seaweed: 8,
        fish: 1,
        pearls: 1,
        shells: 250,
    },
    Recipe {
        id: "glow",
        item: "reefbubble_lamp__sky",
        seaweed: 12,
        fish: 2,
        pearls: 2,
        shells: 400,
    },
    Recipe {
        id: "bloom",
        item: "blushpetal_planter__lilac",
        seaweed: 16,
        fish: 3,
        pearls: 3,
        shells: 600,
    },
];
pub fn recipe(id: &str) -> Option<&'static Recipe> {
    RECIPES.iter().find(|r| r.id == id)
}
/// Meteorological seasons match the fishing content: Dec–Feb, Mar–May, Jun–Aug, Sep–Nov.
pub fn season_start(day: NaiveDate) -> NaiveDate {
    let (year, month) = match day.month() {
        1 | 2 => (day.year() - 1, 12),
        3..=5 => (day.year(), 3),
        6..=8 => (day.year(), 6),
        9..=11 => (day.year(), 9),
        _ => (day.year(), 12),
    };
    NaiveDate::from_ymd_opt(year, month, 1).unwrap()
}
pub fn next_season(start: NaiveDate) -> NaiveDate {
    if start.month() == 12 {
        NaiveDate::from_ymd_opt(start.year() + 1, 3, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(start.year(), start.month() + 3, 1).unwrap()
    }
}
pub fn season_key(day: NaiveDate) -> &'static str {
    match season_start(day).month() {
        3 => "spring",
        6 => "summer",
        9 => "autumn",
        _ => "winter",
    }
}
/// One missed day is a grace day; a longer break starts again without losing possessions.
pub fn streak(last: Option<i32>, day: i32, value: i32) -> i32 {
    match last {
        Some(last) if day <= last => value,
        Some(last) if day - last <= 2 => value.saturating_add(1),
        _ => 1,
    }
}
/// Published chance of losing the held pearls on the next descent.
pub fn dive_risk(depth: i32) -> u32 {
    match depth {
        0 => 15,
        1 => 35,
        _ => 55,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn season_boundaries_and_grace() {
        let jan = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
        assert_eq!(season_start(jan).to_string(), "2026-12-01");
        assert_eq!(next_season(season_start(jan)).to_string(), "2027-03-01");
        assert_eq!(streak(Some(10), 10, 4), 4);
        assert_eq!(streak(Some(10), 12, 4), 5);
        assert_eq!(streak(Some(10), 13, 4), 1);
    }
    #[test]
    fn bounded_sources_have_costs_and_sinks() {
        assert_eq!([dive_risk(0), dive_risk(1), dive_risk(2)], [15, 35, 55]);
        assert!(
            RECIPES
                .iter()
                .all(|r| r.pearls > 0 && r.shells >= 250 && r.seaweed >= 8 && r.fish > 0)
        );
    }
}
