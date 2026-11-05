use std::ops::RangeInclusive;

/// XP granted per eligible message.
pub const MESSAGE_XP: RangeInclusive<i64> = 15..=25;
/// One XP grant per member per guild within this window.
pub const MESSAGE_XP_COOLDOWN_SECS: u64 = 60;

/// XP needed to go from `level` to `level + 1`.
pub const fn xp_to_next(level: u32) -> u64 {
    let l = level as u64;
    5 * l * l + 50 * l + 100
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelProgress {
    pub level: u32,
    /// XP earned inside the current level.
    pub current: u64,
    /// XP the current level takes in total.
    pub needed: u64,
}

impl LevelProgress {
    pub fn fraction(&self) -> f32 {
        self.current as f32 / self.needed as f32
    }
}

pub fn progress(total_xp: i64) -> LevelProgress {
    let mut remaining = total_xp.max(0) as u64;
    let mut level = 0;
    loop {
        let needed = xp_to_next(level);
        if remaining < needed {
            return LevelProgress {
                level,
                current: remaining,
                needed,
            };
        }
        remaining -= needed;
        level += 1;
    }
}

pub fn level_of(total_xp: i64) -> u32 {
    progress(total_xp).level
}

/// A role given to members who reach `level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelReward {
    pub level: u32,
    pub role_id: u64,
}

/// The reward roles a member at `level` should gain and lose, as `(add, remove)`, given the
/// roles they hold now. Stacking keeps every earned reward; otherwise only the roles of the
/// highest level reached stay and the lower reward roles are taken away. Rewards above
/// `level` are never touched.
pub fn role_changes(rewards: &[LevelReward], level: u32, stack: bool, current: &[u64]) -> (Vec<u64>, Vec<u64>) {
    let earned = || rewards.iter().filter(|r| r.level <= level);
    let top = earned().map(|r| r.level).max();
    let kept = |r: &LevelReward| stack || Some(r.level) == top;
    let add = earned()
        .filter(|r| kept(r) && !current.contains(&r.role_id))
        .map(|r| r.role_id)
        .collect();
    let remove = earned()
        .filter(|r| !kept(r) && current.contains(&r.role_id))
        .map(|r| r.role_id)
        .collect();
    (add, remove)
}

/// The nearest reward above `level`; the lowest role id wins a tie.
pub fn next_reward(rewards: &[LevelReward], level: u32) -> Option<LevelReward> {
    rewards
        .iter()
        .filter(|r| r.level > level)
        .min_by_key(|r| (r.level, r.role_id))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels() {
        assert_eq!(
            progress(0),
            LevelProgress {
                level: 0,
                current: 0,
                needed: 100
            }
        );
        assert_eq!(progress(99).level, 0);
        assert_eq!(
            progress(100),
            LevelProgress {
                level: 1,
                current: 0,
                needed: 155
            }
        );
        assert_eq!(progress(254).current, 154);
        assert_eq!(progress(255).level, 2);
        assert_eq!(progress(-5).level, 0);
    }

    fn reward(level: u32, role_id: u64) -> LevelReward {
        LevelReward { level, role_id }
    }

    #[test]
    fn stacking_adds_every_missing_earned_role() {
        let rewards = [reward(5, 50), reward(10, 100), reward(20, 200)];
        assert_eq!(role_changes(&rewards, 4, true, &[]), (vec![], vec![]));
        assert_eq!(role_changes(&rewards, 10, true, &[]), (vec![50, 100], vec![]));
        assert_eq!(role_changes(&rewards, 10, true, &[50, 7]), (vec![100], vec![]));
        assert_eq!(role_changes(&rewards, 25, true, &[50, 100, 200]), (vec![], vec![]));
    }

    #[test]
    fn replacing_keeps_only_the_highest_reward() {
        let rewards = [reward(5, 50), reward(10, 100), reward(20, 200)];
        assert_eq!(role_changes(&rewards, 4, false, &[]), (vec![], vec![]));
        assert_eq!(role_changes(&rewards, 5, false, &[]), (vec![50], vec![]));
        assert_eq!(role_changes(&rewards, 10, false, &[50]), (vec![100], vec![50]));
        assert_eq!(role_changes(&rewards, 12, false, &[50, 100, 9]), (vec![], vec![50]));
        assert_eq!(role_changes(&rewards, 12, false, &[100]), (vec![], vec![]));
    }

    #[test]
    fn replacing_leaves_rewards_above_the_level_alone() {
        let rewards = [reward(5, 50), reward(20, 200)];
        assert_eq!(role_changes(&rewards, 8, false, &[50, 200]), (vec![], vec![]));
    }

    #[test]
    fn roles_sharing_the_top_level_are_all_kept() {
        let rewards = [reward(5, 50), reward(10, 100), reward(10, 101)];
        assert_eq!(role_changes(&rewards, 10, false, &[50]), (vec![100, 101], vec![50]));
        assert_eq!(role_changes(&[], 10, false, &[50]), (vec![], vec![]));
    }

    #[test]
    fn next_reward_is_the_nearest_one_above() {
        let rewards = [reward(10, 100), reward(5, 51), reward(5, 50), reward(20, 200)];
        assert_eq!(next_reward(&rewards, 0), Some(reward(5, 50)));
        assert_eq!(next_reward(&rewards, 5), Some(reward(10, 100)));
        assert_eq!(next_reward(&rewards, 19), Some(reward(20, 200)));
        assert_eq!(next_reward(&rewards, 20), None);
        assert_eq!(next_reward(&[], 0), None);
    }
}
