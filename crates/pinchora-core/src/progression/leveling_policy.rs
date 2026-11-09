//! Server-specific XP policy. Defaults preserve the original leveling behaviour.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    rename = "LevelingParity",
    expecting = "struct LevelingParity",
    default,
    deny_unknown_fields
)]
pub struct LevelingPolicy {
    pub curve: Curve,
    pub message_min: i64,
    pub message_max: i64,
    pub message_cooldown_secs: u64,
    pub messages: bool,
    pub reactions: bool,
    pub voice: bool,
    pub effort: bool,
    pub votes: bool,
    pub separate_voice_level: bool,
    pub reset_on_leave: bool,
    pub leave_grace_secs: u64,
    pub reaction_xp: i64,
    pub effort_bonus: i64,
    pub vote_xp: i64,
    pub highlights_channel_id: Option<String>,
    pub vanity_slug: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    Classic,
    Linear { base: u64, per_level: u64 },
    Mee6,
    Polynomial { base: u64, linear: u64, quadratic: u64 },
}

impl Default for LevelingPolicy {
    fn default() -> Self {
        Self {
            curve: Curve::Classic,
            message_min: 15,
            message_max: 25,
            message_cooldown_secs: 60,
            messages: true,
            reactions: false,
            voice: true,
            effort: false,
            votes: false,
            separate_voice_level: false,
            reset_on_leave: false,
            leave_grace_secs: 300,
            reaction_xp: 5,
            effort_bonus: 5,
            vote_xp: 10,
            highlights_channel_id: None,
            vanity_slug: None,
        }
    }
}

impl LevelingPolicy {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.leave_grace_secs > 604_800 {
            return Err("Leave grace must be at most 604800 seconds.");
        }
        if !(1..=1000).contains(&self.message_min) || !(self.message_min..=1000).contains(&self.message_max) {
            return Err("Message XP must be between 1 and 1000, with minimum at most maximum.");
        }
        if !(5..=3600).contains(&self.message_cooldown_secs) {
            return Err("Message cooldown must be 5 to 3600 seconds.");
        }
        if !(1..=100).contains(&self.reaction_xp)
            || !(1..=100).contains(&self.effort_bonus)
            || !(1..=100).contains(&self.vote_xp)
        {
            return Err("Bonus XP must be 1 to 100.");
        }
        let coefficients = match self.curve {
            Curve::Classic | Curve::Mee6 => None,
            Curve::Linear { base, per_level } => Some((base, per_level, 0)),
            Curve::Polynomial {
                base,
                linear,
                quadratic,
            } => Some((base, linear, quadratic)),
        };
        if let Some((base, linear, quadratic)) = coefficients
            && (!(10..=100_000).contains(&base) || linear > 10_000 || quadratic > 1_000)
        {
            return Err("Curve coefficients are outside the supported range.");
        }
        if let Some(slug) = &self.vanity_slug
            && (slug.len() < 3
                || slug.len() > 32
                || !slug
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                || slug.starts_with('-')
                || slug.ends_with('-'))
        {
            return Err("Vanity URL must be 3 to 32 lowercase letters, digits or hyphens.");
        }
        if let Some(id) = &self.highlights_channel_id
            && !id.parse::<u64>().ok().is_some_and(|id| id > 0 && id <= i64::MAX as u64)
        {
            return Err("Invalid highlights channel.");
        }
        Ok(())
    }

    pub fn xp_to_next(&self, level: u32) -> u64 {
        let l = u64::from(level);
        match self.curve {
            Curve::Classic => 5 * l * l + 50 * l + 100,
            Curve::Mee6 => 5 * l * l + 50 * l + 100,
            Curve::Linear { base, per_level } => base.saturating_add(per_level.saturating_mul(l)),
            Curve::Polynomial {
                base,
                linear,
                quadratic,
            } => base
                .saturating_add(linear.saturating_mul(l))
                .saturating_add(quadratic.saturating_mul(l).saturating_mul(l)),
        }
    }

    pub fn progress(&self, xp: i64) -> crate::progression::leveling::LevelProgress {
        let total = xp.max(0) as u128;
        let mut low = 0_u64;
        let mut high = u64::from(u32::MAX);
        while low < high {
            let mid = low + (high - low).div_ceil(2);
            if self.xp_to_reach(mid) <= total {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        if low == u64::from(u32::MAX) {
            return crate::progression::leveling::LevelProgress {
                level: u32::MAX,
                current: 0,
                needed: 1,
            };
        }
        let level = low as u32;
        crate::progression::leveling::LevelProgress {
            level,
            current: (total - self.xp_to_reach(low)) as u64,
            needed: self.xp_to_next(level),
        }
    }

    fn xp_to_reach(&self, level: u64) -> u128 {
        let n = u128::from(level);
        let sum_levels = n * n.saturating_sub(1) / 2;
        let sum_squares = n * n.saturating_sub(1) * (2 * n).saturating_sub(1) / 6;
        let (base, linear, quadratic) = match self.curve {
            Curve::Classic | Curve::Mee6 => (100, 50, 5),
            Curve::Linear { base, per_level } => (base, per_level, 0),
            Curve::Polynomial {
                base,
                linear,
                quadratic,
            } => (base, linear, quadratic),
        };
        u128::from(base) * n + u128::from(linear) * sum_levels + u128::from(quadratic) * sum_squares
    }

    pub fn level_of(&self, xp: i64) -> u32 {
        self.progress(xp).level
    }
}

#[cfg(test)]
mod tests {
    use crate::progression::leveling_policy::*;

    #[test]
    fn invalid_json_keeps_the_public_policy_type_name() {
        let error = serde_json::from_str::<LevelingPolicy>("false").unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid type: boolean `false`, expected struct LevelingParity at line 1 column 5"
        );
    }

    #[test]
    fn defaults_match_classic_levels() {
        let config = LevelingPolicy::default();
        for xp in [0, 99, 100, 255, 10_000, 1_000_000] {
            assert_eq!(config.progress(xp), crate::progression::leveling::progress(xp));
        }
    }

    #[test]
    fn curves_and_limits() {
        let mut config = LevelingPolicy {
            curve: Curve::Linear {
                base: 100,
                per_level: 20,
            },
            ..Default::default()
        };
        assert_eq!(config.level_of(220), 2);
        config.curve = Curve::Polynomial {
            base: 100,
            linear: 0,
            quadratic: 10,
        };
        assert_eq!(config.xp_to_next(3), 190);
        config.curve = Curve::Linear { base: 10, per_level: 0 };
        assert_eq!(config.level_of(1_000_000_000), 100_000_000);
        config.message_min = 30;
        config.message_max = 20;
        assert!(config.validate().is_err());
        config.message_min = 15;
        config.message_max = 25;
        config.highlights_channel_id = Some("123456789012345678".into());
        assert!(config.validate().is_ok());
        assert_eq!(
            serde_json::to_value(&config).unwrap()["highlights_channel_id"],
            "123456789012345678"
        );
    }
}
