use chrono::{DateTime, Duration, Utc};

pub const DAILY_BASE: i64 = 250;
pub const DAILY_STREAK_BONUS: i64 = 25;
pub const DAILY_STREAK_CAP: i32 = 20;
pub const DAILY_COOLDOWN: Duration = Duration::hours(24);
/// A streak survives if the next claim lands within this window of the last one.
pub const DAILY_STREAK_WINDOW: Duration = Duration::hours(48);
pub const REP_COOLDOWN: Duration = Duration::hours(24);

/// A reward with a cooldown longer than the daily one: `/weekly` and `/monthly`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Weekly,
    Monthly,
}

impl Period {
    pub const ALL: [Self; 2] = [Self::Weekly, Self::Monthly];

    /// The name the database and the reminders know it by.
    pub fn key(self) -> &'static str {
        match self {
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }

    pub fn cooldown(self) -> Duration {
        match self {
            Self::Weekly => Duration::days(7),
            Self::Monthly => Duration::days(30),
        }
    }

    /// Shells a claim pays: about six dailies a week, about twenty-six a month.
    pub fn reward(self) -> i64 {
        match self {
            Self::Weekly => 1_500,
            Self::Monthly => 6_500,
        }
    }
}

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
    fn periods_pay_more_than_a_daily_and_less_often() {
        for period in Period::ALL {
            assert_eq!(Period::from_key(period.key()), Some(period));
            assert!(period.cooldown() > DAILY_COOLDOWN);
            assert!(period.reward() > daily_reward(1));
        }
        assert!(Period::Monthly.cooldown() > Period::Weekly.cooldown());
        assert_eq!(Period::from_key("daily"), None);
    }

    #[test]
    fn streaks() {
        let now = Utc::now();
        assert_eq!(next_streak(None, now, 7), 1);
        assert_eq!(next_streak(Some(now - Duration::hours(30)), now, 7), 8);
        assert_eq!(next_streak(Some(now - Duration::hours(49)), now, 7), 1);
    }
}
