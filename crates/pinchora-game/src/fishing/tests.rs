use super::*;

#[test]
fn day_parts() {
    assert_eq!(time_of_day(6), "dawn");
    assert_eq!(time_of_day(12), "day");
    assert_eq!(time_of_day(23), "night");
    assert_eq!(season(1), "winter");
    assert_eq!(season(7), "summer");
}

fn content() -> &'static Content {
    Content::get()
}

fn fish(id: &str) -> &'static Fish {
    content().fish(id).expect("a fish")
}

/// Every hour (UTC) of every month: all the cells of the clock a cast can fall in.
fn cells() -> impl Iterator<Item = (u32, u32)> {
    (0..24).flat_map(|hour| (1..=12).map(move |month| (hour, month)))
}

fn share_of(pool: &[(&Fish, f64)], keep: impl Fn(&Fish) -> bool) -> f64 {
    let total: f64 = pool.iter().map(|(_, w)| w).sum();
    pool.iter().filter(|(f, _)| keep(f)).map(|(_, w)| w).sum::<f64>() / total
}

#[path = "tests/economy.rs"]
mod economy;
#[path = "tests/odds.rs"]
mod odds;
