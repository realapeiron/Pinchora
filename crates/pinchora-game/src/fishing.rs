//! Fishing: which fish bite now, and what they are worth.

use crate::{Content, Fish};

pub const BAIT_PRICE: i64 = 10;
pub const CAST_COOLDOWN_SECS: u64 = 30;
/// Time the player has to press Reel after the bite.
pub const REEL_WINDOW_MS: i64 = 2_500;

/// Part of the day for a UTC hour.
pub fn time_of_day(hour: u32) -> &'static str {
    match hour {
        5..=6 => "dawn",
        7..=17 => "day",
        18..=19 => "dusk",
        _ => "night",
    }
}

/// Season for a month 1..=12 (northern hemisphere).
pub fn season(month: u32) -> &'static str {
    match month {
        3..=5 => "spring",
        6..=8 => "summer",
        9..=11 => "autumn",
        _ => "winter",
    }
}

pub fn biting(content: &Content, hour: u32, month: u32) -> Vec<&Fish> {
    let (tod, season) = (time_of_day(hour), season(month));
    content
        .fish
        .iter()
        .filter(|f| (f.time == "any" || f.time == tod) && (f.season == "any" || f.season == season))
        .collect()
}

/// Picks a fish by weight and a size, from two 0..1 rolls.
pub fn catch<'a>(pool: &[&'a Fish], roll: f64, size_roll: f64) -> Option<(&'a Fish, f32, i64)> {
    let total: u64 = pool.iter().map(|f| u64::from(f.weight)).sum();
    if total == 0 {
        return None;
    }
    let mut target = (roll.clamp(0.0, 0.999_999) * total as f64) as u64;
    let fish = *pool.iter().find(|f| {
        let w = u64::from(f.weight);
        if target < w {
            true
        } else {
            target -= w;
            false
        }
    })?;
    let size = fish.min_cm + (fish.max_cm - fish.min_cm) * size_roll as f32;
    let avg = (fish.min_cm + fish.max_cm) / 2.0;
    let value = ((fish.base_value as f32) * (size / avg.max(0.1))).round().max(1.0) as i64;
    Some((fish, (size * 10.0).round() / 10.0, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_parts() {
        assert_eq!(time_of_day(6), "dawn");
        assert_eq!(time_of_day(12), "day");
        assert_eq!(time_of_day(23), "night");
        assert_eq!(season(1), "winter");
        assert_eq!(season(7), "summer");
    }
}
