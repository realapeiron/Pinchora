use std::{collections::HashMap, ops::RangeInclusive};

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

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

/// Largest XP multiplier a channel or role can have.
pub const MAX_MULTIPLIER: f64 = 5.0;
/// Server XP a minute in voice earns by default, and the range a server can pick from.
pub const VOICE_XP_DEFAULT: i64 = 10;
pub const VOICE_XP_RANGE: RangeInclusive<i64> = 1..=100;

/// Where a level-up is announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelUpMode {
    /// The level-up channel; where the member chatted when the server set none.
    Channel,
    /// Always where the member chatted.
    Current,
    /// A direct message to the member.
    Dm,
    /// Nowhere: the roles are still given.
    Off,
}

impl LevelUpMode {
    /// What the database stores. Anything unknown reads as [`LevelUpMode::Channel`].
    pub fn parse(raw: &str) -> Self {
        match raw {
            "current" => Self::Current,
            "dm" => Self::Dm,
            "off" => Self::Off,
            _ => Self::Channel,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Current => "current",
            Self::Dm => "dm",
            Self::Off => "off",
        }
    }
}

/// The XP multipliers a server set up for its channels and roles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XpRules {
    /// The server's id, which is also the id of its `@everyone` role.
    pub guild: u64,
    pub channels: HashMap<u64, f64>,
    pub roles: HashMap<u64, f64>,
}

impl XpRules {
    /// What a member holding `held` earns, times, for chatting in or sitting in `channel`: the
    /// channel's multiplier times the highest multiplier among their roles (1 where nothing is set).
    /// A role set to 0 always wins, so a no-XP role cannot be outvoted by a booster role. `@everyone`
    /// counts for every member.
    pub fn multiplier(&self, channel: u64, held: &[u64]) -> f64 {
        self.multiplier_in(&[channel], held)
    }

    /// [`XpRules::multiplier`] for a channel that sits under others: `lineage` is the channel first,
    /// then what it is in, nearest first (a thread or forum post, its channel, that channel's
    /// category). The first one with a rule decides, so a thread's own rule beats its channel's, and a
    /// channel's beats its category's.
    pub fn multiplier_in(&self, lineage: &[u64], held: &[u64]) -> f64 {
        let channel = lineage
            .iter()
            .find_map(|id| self.channels.get(id))
            .copied()
            .unwrap_or(1.0);
        let mut roles = self
            .roles
            .iter()
            .filter(|(id, _)| **id == self.guild || held.contains(id))
            .map(|(_, m)| *m);
        let role = match roles.next() {
            None => 1.0,
            Some(first) => roles.fold(first, |best, m| if best == 0.0 || m == 0.0 { 0.0 } else { best.max(m) }),
        };
        channel * role
    }

    pub fn is_empty(&self) -> bool {
        self.channels.is_empty() && self.roles.is_empty()
    }
}

/// `base` XP or points times `multiplier`, rounded. Only a multiplier of 0 gives nothing: a small
/// multiplier still pays at least 1.
pub fn scale(base: i64, multiplier: f64) -> i64 {
    if base <= 0 || multiplier <= 0.0 {
        return 0;
    }
    ((base as f64 * multiplier).round() as i64).max(1)
}

/// The Monday (UTC) of the week `at` falls in: the key weekly XP is stored under. A week runs from
/// Monday 00:00 UTC to the next Monday.
pub fn week_start(at: DateTime<Utc>) -> NaiveDate {
    let day = at.date_naive();
    day - Duration::days(i64::from(day.weekday().num_days_from_monday()))
}

/// A member sitting in a voice channel. Bots are never listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoicePresence {
    pub user: u64,
    pub channel: u64,
    /// Muted by themselves or by a moderator.
    pub muted: bool,
    /// Deafened by themselves or by a moderator.
    pub deafened: bool,
}

/// Who earns voice XP this minute among the people in voice (bots excluded). Nobody earns in the
/// server's AFK channel, alone in a channel, or while muted or deafened.
pub fn voice_earners(present: &[VoicePresence], afk: Option<u64>) -> Vec<u64> {
    let mut company: HashMap<u64, usize> = HashMap::new();
    for member in present {
        *company.entry(member.channel).or_default() += 1;
    }
    present
        .iter()
        .filter(|m| Some(m.channel) != afk && company[&m.channel] >= 2 && !m.muted && !m.deafened)
        .map(|m| m.user)
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::progression::leveling::*;

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
    fn rules(channels: &[(u64, f64)], roles: &[(u64, f64)]) -> XpRules {
        XpRules {
            guild: 1,
            channels: channels.iter().copied().collect(),
            roles: roles.iter().copied().collect(),
        }
    }

    #[test]
    fn nothing_set_means_normal_xp() {
        assert_eq!(XpRules::default().multiplier(5, &[7]), 1.0);
        assert!(XpRules::default().is_empty());
        assert_eq!(rules(&[(9, 2.0)], &[]).multiplier(5, &[7]), 1.0);
    }

    #[test]
    fn channel_and_highest_role_multiply() {
        let r = rules(&[(5, 2.0)], &[(10, 1.5), (11, 3.0), (12, 0.5)]);
        assert_eq!(r.multiplier(5, &[]), 2.0, "no role of theirs has a rule");
        assert_eq!(r.multiplier(5, &[10]), 3.0);
        assert_eq!(r.multiplier(5, &[10, 11, 12]), 6.0, "channel 2 x highest role 3");
        assert_eq!(r.multiplier(6, &[12]), 0.5, "a lone small role still counts");
        assert_eq!(r.multiplier(6, &[10, 12]), 1.5);
    }

    #[test]
    fn the_nearest_channel_with_a_rule_decides() {
        // 5 is a thread in channel 6, which sits in category 7.
        let lineage = [5, 6, 7];
        let r = rules(&[(7, 2.0)], &[]);
        assert_eq!(r.multiplier_in(&lineage, &[]), 2.0, "only the category has a rule");
        assert_eq!(r.multiplier_in(&lineage[1..], &[]), 2.0, "a channel in the category");
        assert_eq!(r.multiplier_in(&[8], &[]), 1.0, "a channel somewhere else");
        let r = rules(&[(7, 2.0), (6, 3.0)], &[]);
        assert_eq!(r.multiplier_in(&lineage, &[]), 3.0, "the channel beats its category");
        assert_eq!(
            r.multiplier_in(&[8, 7], &[]),
            2.0,
            "a channel without a rule uses the category"
        );
        let r = rules(&[(7, 2.0), (6, 3.0), (5, 4.0)], &[]);
        assert_eq!(r.multiplier_in(&lineage, &[]), 4.0, "the thread beats its channel");
        assert_eq!(r.multiplier_in(&[], &[]), 1.0);
    }

    #[test]
    fn a_rule_of_zero_on_a_parent_reaches_its_threads_and_roles_still_multiply() {
        let r = rules(&[(6, 0.0), (7, 2.0)], &[(10, 3.0)]);
        assert_eq!(
            r.multiplier_in(&[5, 6, 7], &[10]),
            0.0,
            "no XP in the channel, no XP in its threads"
        );
        let r = rules(&[(7, 2.0)], &[(10, 3.0)]);
        assert_eq!(r.multiplier_in(&[5, 6, 7], &[10]), 6.0, "category 2 x role 3");
        assert_eq!(r.multiplier(5, &[10]), 3.0, "the plain call looks at one channel only");
    }

    #[test]
    fn zero_means_no_xp_and_a_no_xp_role_cannot_be_outvoted() {
        let r = rules(&[(5, 0.0)], &[(10, 0.0), (11, 3.0)]);
        assert_eq!(r.multiplier(5, &[]), 0.0, "a no-XP channel");
        assert_eq!(r.multiplier(6, &[10]), 0.0, "a no-XP role");
        assert_eq!(r.multiplier(6, &[10, 11]), 0.0, "even next to a booster role");
        assert_eq!(r.multiplier(6, &[11]), 3.0);
        assert_eq!(r.multiplier(6, &[]), 1.0);
    }

    #[test]
    fn everyone_applies_to_every_member() {
        let r = rules(&[], &[(1, 2.0), (10, 1.5)]);
        assert_eq!(r.multiplier(5, &[]), 2.0, "@everyone is the server id");
        assert_eq!(r.multiplier(5, &[10]), 2.0, "highest of @everyone and the role");
    }

    #[test]
    fn scaling_rounds_and_only_zero_pays_nothing() {
        assert_eq!(scale(20, 1.0), 20);
        assert_eq!(scale(20, 2.5), 50);
        assert_eq!(scale(15, 0.5), 8, "7.5 rounds up");
        assert_eq!(scale(3, 0.1), 1, "a small multiplier still pays 1");
        assert_eq!(scale(20, 0.0), 0);
        assert_eq!(scale(0, 5.0), 0);
        assert_eq!(scale(-4, 5.0), 0);
        assert_eq!(scale(25, MAX_MULTIPLIER), 125);
    }

    #[test]
    fn weeks_start_on_monday_utc() {
        let at = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        assert_eq!(week_start(at("2026-11-02T00:00:00Z")), monday);
        assert_eq!(week_start(at("2026-11-05T13:45:00Z")), monday);
        assert_eq!(
            week_start(at("2026-11-08T23:59:59Z")),
            monday,
            "Sunday still belongs to it"
        );
        assert_eq!(
            week_start(at("2026-11-09T00:00:00Z")),
            NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
        );
        assert_eq!(
            week_start(at("2026-11-01T23:59:59Z")),
            NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
        );
    }

    #[test]
    fn level_up_modes_round_trip_and_unknown_means_channel() {
        for mode in [
            LevelUpMode::Channel,
            LevelUpMode::Current,
            LevelUpMode::Dm,
            LevelUpMode::Off,
        ] {
            assert_eq!(LevelUpMode::parse(mode.key()), mode);
        }
        assert_eq!(LevelUpMode::parse(""), LevelUpMode::Channel);
        assert_eq!(LevelUpMode::parse("whatever"), LevelUpMode::Channel);
    }

    fn here(user: u64, channel: u64, muted: bool, deafened: bool) -> VoicePresence {
        VoicePresence {
            user,
            channel,
            muted,
            deafened,
        }
    }

    #[test]
    fn voice_needs_company_and_skips_the_afk_channel() {
        let present = [
            here(1, 10, false, false),
            here(2, 10, false, false),
            here(3, 20, false, false),
        ];
        assert_eq!(voice_earners(&present, None), vec![1, 2], "3 is alone");
        assert_eq!(
            voice_earners(&present, Some(10)),
            Vec::<u64>::new(),
            "10 is the AFK channel"
        );
        assert_eq!(voice_earners(&[], None), Vec::<u64>::new());
    }

    #[test]
    fn voice_skips_either_muted_or_deafened() {
        let present = [
            here(1, 10, true, true),
            here(2, 10, true, false),
            here(3, 10, false, true),
            here(4, 10, false, false),
        ];
        assert_eq!(voice_earners(&present, None), vec![4], "either flag prevents XP");
        let pair = [here(1, 10, true, true), here(2, 10, false, false)];
        assert_eq!(
            voice_earners(&pair, None),
            vec![2],
            "an idle member still counts as company"
        );
    }
}
