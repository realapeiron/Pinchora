//! The shell game: a pearl under one of three shells, a guess, and a payout that favours the sea.

use super::Rng;

/// Shells on the table.
pub const SHELLS: usize = 3;
/// Smallest and largest stake, in shells of currency.
pub const MIN_STAKE: i64 = 5;
pub const MAX_STAKE: i64 = 100;
/// Seconds to make the guess before the stake is given back.
pub const GUESS_SECS: i64 = 45;
/// A player can start this many games a day.
pub const DAILY_PLAYS: i32 = 20;
/// The table closes for a player who is this far down, or this far up, on the day.
pub const DAILY_LOSS_LIMIT: i64 = 300;
pub const DAILY_WIN_LIMIT: i64 = 600;
/// Seconds between one player's games.
pub const COOLDOWN_SECS: u64 = 8;

/// What a stake comes back as on a right guess: two and a half times, rounded down. With the pearl
/// under one of three shells a player gets back five sixths of what they put in, on average.
pub fn payout(stake: i64) -> i64 {
    stake.saturating_mul(5) / 2
}

/// The shell (0, 1 or 2) the pearl is under for `seed`.
pub fn pearl(seed: u64) -> usize {
    Rng::new(seed).below(SHELLS as u32) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sea_keeps_a_sixth_of_every_stake_on_average() {
        for stake in [MIN_STAKE, 7, 50, 99, MAX_STAKE] {
            let expected = payout(stake) as f64 / SHELLS as f64;
            assert!(expected < stake as f64, "stake {stake} pays back {expected}");
            assert!(payout(stake) > stake, "a win is a win");
        }
        assert_eq!(payout(10), 25);
        assert_eq!(payout(5), 12);
    }

    #[test]
    fn the_pearl_is_under_each_shell_about_a_third_of_the_time() {
        let mut counts = [0u32; SHELLS];
        for seed in 0..3_000u64 {
            counts[pearl(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15))] += 1;
        }
        for (shell, n) in counts.iter().enumerate() {
            assert!((850..1150).contains(n), "shell {shell}: {counts:?}");
        }
        assert_eq!(pearl(5), pearl(5));
    }
}
