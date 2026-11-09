//! Rules of the server events: how long people can ask a giveaway or poll to run, and how
//! giveaway winners are drawn.

use chrono::Duration;

/// Shortest and longest giveaway.
pub const GIVEAWAY_MIN: Duration = Duration::minutes(1);
pub const GIVEAWAY_MAX: Duration = Duration::days(30);
/// Shortest and longest poll, and how long one runs when the author does not say.
pub const POLL_MIN: Duration = Duration::minutes(1);
pub const POLL_MAX: Duration = Duration::days(14);
pub const POLL_DEFAULT: Duration = Duration::hours(24);

/// Reads a duration such as `90s`, `30m`, `2h`, `1d`, `1w` or several together (`1d12h`, `1h 30m`).
/// Long unit names (`minutes`, `hours`) work too. `None` for anything else, zero, or a number too
/// big to count.
pub fn parse_duration(text: &str) -> Option<Duration> {
    let mut total = Duration::zero();
    let mut chars = text.trim().chars().peekable();
    let mut parts = 0;
    while chars.peek().is_some() {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut digits = String::new();
        while let Some(c) = chars.next_if(char::is_ascii_digit) {
            digits.push(c);
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut unit = String::new();
        while let Some(c) = chars.next_if(|c| c.is_ascii_alphabetic()) {
            unit.push(c.to_ascii_lowercase());
        }
        let seconds: i64 = match unit.as_str() {
            "s" | "sec" | "secs" | "second" | "seconds" => 1,
            "m" | "min" | "mins" | "minute" | "minutes" => 60,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600,
            "d" | "day" | "days" => 86_400,
            "w" | "week" | "weeks" => 604_800,
            _ => return None,
        };
        let count: i64 = digits.parse().ok()?;
        let part = count.checked_mul(seconds).and_then(Duration::try_seconds)?;
        total = total.checked_add(&part)?;
        parts += 1;
    }
    (parts > 0 && total > Duration::zero()).then_some(total)
}

/// Draws up to `count` different people from `pool`, every subset equally likely: a partial
/// Fisher-Yates shuffle. `below(n)` must return a uniform number in `0..n`.
pub fn pick_winners(pool: &[u64], count: usize, mut below: impl FnMut(usize) -> usize) -> Vec<u64> {
    let mut deck = pool.to_vec();
    let take = count.min(deck.len());
    for i in 0..take {
        let j = i + below(deck.len() - i).min(deck.len() - i - 1);
        deck.swap(i, j);
    }
    deck.truncate(take);
    deck
}

#[cfg(test)]
mod tests {
    use crate::admin::server_events::*;

    #[test]
    fn reads_single_and_combined_durations() {
        assert_eq!(parse_duration("90s"), Some(Duration::seconds(90)));
        assert_eq!(parse_duration("30m"), Some(Duration::minutes(30)));
        assert_eq!(parse_duration("2h"), Some(Duration::hours(2)));
        assert_eq!(parse_duration("1d"), Some(Duration::days(1)));
        assert_eq!(parse_duration("1w"), Some(Duration::weeks(1)));
        assert_eq!(parse_duration("1d12h"), Some(Duration::hours(36)));
        assert_eq!(parse_duration(" 1h 30m "), Some(Duration::minutes(90)));
        assert_eq!(parse_duration("2 Hours"), Some(Duration::hours(2)));
        assert_eq!(parse_duration("10 minutes"), Some(Duration::minutes(10)));
    }

    #[test]
    fn refuses_anything_else() {
        for text in [
            "",
            " ",
            "h",
            "5",
            "0m",
            "0d0h",
            "-5m",
            "1.5h",
            "5x",
            "1h2",
            "m5",
            "1h x",
            "99999999999999999999d",
            "9223372036854775807d",
        ] {
            assert_eq!(parse_duration(text), None, "{text:?} was accepted");
        }
    }

    /// A small deterministic generator, so the test needs no luck.
    fn rng(mut state: u64) -> impl FnMut(usize) -> usize {
        move |n| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % n as u64) as usize
        }
    }

    #[test]
    fn a_draw_has_no_repeats_and_never_more_than_asked_or_present() {
        let pool: Vec<u64> = (1..=10).collect();
        let mut below = rng(7);
        for count in [0, 1, 4, 10, 25] {
            let drawn = pick_winners(&pool, count, &mut below);
            assert_eq!(drawn.len(), count.min(pool.len()));
            let mut sorted = drawn.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), drawn.len(), "repeats in {drawn:?}");
            assert!(drawn.iter().all(|w| pool.contains(w)));
        }
        assert!(pick_winners(&[], 3, &mut below).is_empty());
    }

    #[test]
    fn every_entrant_is_equally_likely_to_win() {
        let pool: Vec<u64> = (0..5).collect();
        let (rounds, picks) = (50_000usize, 2usize);
        let mut below = rng(0x9e37_79b9_7f4a_7c15);
        let mut wins = [0usize; 5];
        let mut pairs = std::collections::HashMap::<[u64; 2], usize>::new();
        for _ in 0..rounds {
            let mut drawn = pick_winners(&pool, picks, &mut below);
            for w in &drawn {
                wins[*w as usize] += 1;
            }
            drawn.sort_unstable();
            *pairs.entry([drawn[0], drawn[1]]).or_default() += 1;
        }
        // Each entrant wins in 2 of 5 draws; each of the 10 pairs comes up in 1 of 10.
        let expected = (rounds * picks / pool.len()) as f64;
        for (entrant, won) in wins.iter().enumerate() {
            assert!(
                (*won as f64 - expected).abs() < expected * 0.04,
                "entrant {entrant} won {won} times, expected about {expected}"
            );
        }
        assert_eq!(pairs.len(), 10, "every pair should come up");
        let expected = rounds as f64 / 10.0;
        for (pair, seen) in &pairs {
            assert!(
                (*seen as f64 - expected).abs() < expected * 0.06,
                "pair {pair:?} came up {seen} times, expected about {expected}"
            );
        }
    }
}
