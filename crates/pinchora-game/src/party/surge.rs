//! The storm surge: a whole server stacks sandbags against a rising sea. The wall has to reach its
//! goal before time runs out, while the water wears it down.

/// Seconds a storm lasts.
pub const DURATION_SECS: i64 = 600;
/// One sandbag is washed away this often.
pub const ERODE_SECS: i64 = 30;
/// Sandbags the wall needs with nobody there, and for each helper who has joined (up to 20).
pub const BASE_GOAL: i64 = 24;
pub const PER_HELPER: i64 = 8;
pub const MAX_COUNTED_HELPERS: i64 = 20;
/// Seconds between one helper's sandbags, and the most one helper can stack in a storm.
pub const HELP_COOLDOWN_SECS: u64 = 6;
pub const MAX_HELPS: i64 = 25;
/// A manager can summon a storm this often per server, in seconds; a timed storm is at least
/// [`MIN_EVERY_HOURS`] apart.
pub const SUMMON_COOLDOWN_SECS: u64 = 1_800;
pub const MIN_EVERY_HOURS: i32 = 6;
/// Shells the system pays one player per day for storms.
pub const DAILY_SHELLS: i64 = 120;
/// What a storm does when it ends.
pub const WIN_BASE: i64 = 20;
pub const WIN_PER_HELP: i64 = 2;
pub const WIN_HELPS_COUNTED: i64 = 15;
pub const LOSE_BASE: i64 = 6;
pub const LOSE_PER_HELP: i64 = 1;
pub const LOSE_HELPS_COUNTED: i64 = 14;

/// Sandbags the wall needs for this many helpers.
pub fn goal(helpers: i64) -> i64 {
    BASE_GOAL + PER_HELPER * helpers.clamp(0, MAX_COUNTED_HELPERS)
}

/// Sandbags the water has washed away after `elapsed_secs`.
pub fn erosion(elapsed_secs: i64) -> i64 {
    elapsed_secs.max(0) / ERODE_SECS
}

/// How tall the wall stands: the sandbags stacked less the ones washed away.
pub fn wall(stacked: i64, elapsed_secs: i64) -> i64 {
    (stacked - erosion(elapsed_secs)).max(0)
}

/// Shells a helper who stacked `helps` sandbags gets when the storm ends.
pub fn reward(held: bool, helps: i64) -> i64 {
    let helps = helps.max(0);
    if held {
        WIN_BASE + WIN_PER_HELP * helps.min(WIN_HELPS_COUNTED)
    } else {
        LOSE_BASE + LOSE_PER_HELP * helps.min(LOSE_HELPS_COUNTED)
    }
}

/// The intervals a server can pick for timed storms, in hours.
pub const EVERY_CHOICES: [i32; 5] = [6, 12, 24, 48, 168];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_goal_grows_with_the_crowd_up_to_a_limit() {
        assert_eq!(goal(0), BASE_GOAL);
        assert_eq!(goal(1), 32);
        assert_eq!(goal(5), 64);
        assert_eq!(goal(20), goal(500));
        assert_eq!(goal(-3), BASE_GOAL);
    }

    #[test]
    fn the_sea_wears_the_wall_down_but_never_below_the_sand() {
        assert_eq!(erosion(0), 0);
        assert_eq!(erosion(29), 0);
        assert_eq!(erosion(30), 1);
        assert_eq!(erosion(DURATION_SECS), 20);
        assert_eq!(wall(10, 60), 8);
        assert_eq!(wall(1, 600), 0);
        assert_eq!(erosion(-5), 0);
    }

    #[test]
    fn a_crowd_can_win_and_one_player_cannot() {
        // One helper is held to 25 bags, so even stacking one every cooldown, the sea takes its share back
        // before the wall can reach the goal of a crowd of one.
        assert!(wall(MAX_HELPS, DURATION_SECS) < goal(1));
        assert!(wall(MAX_HELPS, MAX_HELPS * HELP_COOLDOWN_SECS as i64) < goal(1));
        // Two helpers at full speed can just do it.
        assert!(wall(2 * MAX_HELPS, MAX_HELPS * HELP_COOLDOWN_SECS as i64) >= goal(2));
        // Five helpers each stacking 18 hold it with minutes to spare.
        assert!(wall(5 * 18, 400) >= goal(5));
        // Ten who stack five each do not.
        assert!(wall(10 * 5, 590) < goal(10));
    }

    #[test]
    fn rewards_are_small_and_a_win_pays_more_than_a_loss() {
        for helps in [0, 1, 7, 15, 25] {
            assert!(reward(true, helps) > reward(false, helps));
        }
        assert_eq!(reward(true, 15), 50);
        assert_eq!(reward(true, 100), 50);
        assert_eq!(reward(false, 3), 9);
        assert!(reward(true, MAX_HELPS) <= DAILY_SHELLS);
    }

    #[test]
    fn timed_storms_are_never_closer_than_the_minimum() {
        assert!(EVERY_CHOICES.iter().all(|h| *h >= MIN_EVERY_HOURS));
    }
}
