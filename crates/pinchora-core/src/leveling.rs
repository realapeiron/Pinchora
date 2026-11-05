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
}
