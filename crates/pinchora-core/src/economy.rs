use chrono::{DateTime, Duration, Utc};

pub const DAILY_BASE: i64 = 250;
pub const DAILY_STREAK_BONUS: i64 = 25;
pub const DAILY_STREAK_CAP: i32 = 20;
pub const DAILY_COOLDOWN: Duration = Duration::hours(24);
/// A streak survives if the next claim lands within this window of the last one.
pub const DAILY_STREAK_WINDOW: Duration = Duration::hours(48);
pub const REP_COOLDOWN: Duration = Duration::hours(24);

pub fn daily_reward(streak: i32) -> i64 {
    DAILY_BASE + DAILY_STREAK_BONUS * i64::from((streak - 1).clamp(0, DAILY_STREAK_CAP))
}

pub fn next_streak(last_claim: Option<DateTime<Utc>>, now: DateTime<Utc>, streak: i32) -> i32 {
    match last_claim {
        Some(last) if now - last <= DAILY_STREAK_WINDOW => streak + 1,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewards() {
        assert_eq!(daily_reward(1), 250);
        assert_eq!(daily_reward(2), 275);
        assert_eq!(daily_reward(100), 250 + 25 * 20);
    }

    #[test]
    fn streaks() {
        let now = Utc::now();
        assert_eq!(next_streak(None, now, 7), 1);
        assert_eq!(next_streak(Some(now - Duration::hours(30)), now, 7), 8);
        assert_eq!(next_streak(Some(now - Duration::hours(49)), now, 7), 1);
    }
}
