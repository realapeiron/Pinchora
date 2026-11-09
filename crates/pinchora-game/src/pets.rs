//! Pet needs, care and capsules.

use crate::{Content, PetSpecies};

/// Points lost per hour.
pub const HUNGER_DECAY: f32 = 4.0;
pub const FUN_DECAY: f32 = 5.0;
pub const HYGIENE_DECAY: f32 = 2.5;
/// Energy refills while resting.
pub const ENERGY_REGEN: f32 = 8.0;

pub const CAPSULE_PRICE: i64 = 1_500;
pub const FOOD_PRICE: i64 = 20;
/// Shells refunded when a capsule hatches a species you already own.
pub const DUPLICATE_REFUND: i64 = 400;

/// The highest pet level; a pet at the top keeps playing but stops earning XP.
pub const MAX_LEVEL: i32 = 30;
/// Hours with hunger, fun and hygiene all at zero before a pet falls asleep.
pub const SLEEP_AFTER_HOURS: f32 = 24.0;
/// Hunger, fun or hygiene at or below this and a pet needs looking after: what the pet reminder tells.
pub const LOW_AT: i16 = 30;
/// Shells a wake-up costs once the free one of the day is used.
pub const WAKE_FEE: i64 = 75;
/// How a pet feels when it wakes up: rested and half full.
pub const WAKE_NEEDS: Needs = Needs {
    hunger: 50,
    fun: 50,
    energy: 100,
    hygiene: 50,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Needs {
    pub hunger: i16,
    pub fun: i16,
    pub energy: i16,
    pub hygiene: i16,
}

impl Needs {
    /// Needs `hours` after they were stored.
    pub fn after(self, hours: f32) -> Self {
        let dec = |v: i16, rate: f32| (f32::from(v) - rate * hours).clamp(0.0, 100.0).round() as i16;
        Self {
            hunger: dec(self.hunger, HUNGER_DECAY),
            fun: dec(self.fun, FUN_DECAY),
            energy: dec(self.energy, -ENERGY_REGEN),
            hygiene: dec(self.hygiene, HYGIENE_DECAY),
        }
    }

    /// Hours from now until hunger, fun and hygiene have all run out. Energy is left out: it
    /// refills by itself, so it never sits at zero.
    pub fn hours_until_drained(self) -> f32 {
        [
            f32::from(self.hunger) / HUNGER_DECAY,
            f32::from(self.fun) / FUN_DECAY,
            f32::from(self.hygiene) / HYGIENE_DECAY,
        ]
        .into_iter()
        .fold(0.0, f32::max)
    }

    /// Hours from now until the first of hunger, fun and hygiene is down to [`LOW_AT`]: 0 when one
    /// already is. Energy is left out, it refills by itself.
    pub fn hours_until_low(self) -> f32 {
        [
            (f32::from(self.hunger) - f32::from(LOW_AT)) / HUNGER_DECAY,
            (f32::from(self.fun) - f32::from(LOW_AT)) / FUN_DECAY,
            (f32::from(self.hygiene) - f32::from(LOW_AT)) / HYGIENE_DECAY,
        ]
        .into_iter()
        .fold(f32::INFINITY, f32::min)
        .max(0.0)
    }

    /// What the pet wants most: the lowest of hunger, fun and hygiene, as a short phrase.
    pub fn wants(self) -> &'static str {
        let lowest = [
            (self.hunger, "something to eat"),
            (self.fun, "someone to play with"),
            (self.hygiene, "a bath"),
        ]
        .into_iter()
        .min_by_key(|(value, _)| *value);
        lowest.map_or("some care", |(_, phrase)| phrase)
    }

    /// What the pet wants most (see [`Needs::wants`]) as a catalog key: `food`, `play`, `bath` or `care`.
    pub fn want_key(self) -> &'static str {
        let lowest = [(self.hunger, "food"), (self.fun, "play"), (self.hygiene, "bath")]
            .into_iter()
            .min_by_key(|(value, _)| *value);
        lowest.map_or("care", |(_, key)| key)
    }

    /// Hours after these needs were stored when a pet nobody looked after falls asleep.
    pub fn sleeps_after_hours(self) -> f32 {
        self.hours_until_drained() + SLEEP_AFTER_HOURS
    }

    pub fn mood(self) -> &'static str {
        let low = [self.hunger, self.fun, self.energy, self.hygiene]
            .into_iter()
            .min()
            .unwrap_or(0);
        match low {
            0..=15 => "miserable",
            16..=40 => "grumpy",
            41..=70 => "content",
            _ => "delighted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Care {
    Feed,
    Play,
    Clean,
    Walk,
}

impl Care {
    pub fn cooldown_secs(self) -> u64 {
        match self {
            Self::Feed => 30 * 60,
            Self::Play => 20 * 60,
            Self::Clean => 2 * 3600,
            Self::Walk => 45 * 60,
        }
    }

    /// Applies the action; `None` when the pet is too tired to play.
    pub fn apply(self, n: Needs) -> Option<Needs> {
        let up = |v: i16, by: i16| (v + by).min(100);
        match self {
            Self::Feed => Some(Needs {
                hunger: up(n.hunger, 35),
                ..n
            }),
            Self::Play if n.energy < 15 => None,
            Self::Play => Some(Needs {
                fun: up(n.fun, 30),
                energy: (n.energy - 15).max(0),
                hunger: (n.hunger - 5).max(0),
                ..n
            }),
            Self::Clean => Some(Needs { hygiene: 100, ..n }),
            Self::Walk if n.energy < 10 => None,
            Self::Walk => Some(Needs {
                fun: up(n.fun, 20),
                energy: (n.energy - 10).max(0),
                hunger: (n.hunger - 8).max(0),
                hygiene: (n.hygiene - 10).max(0),
            }),
        }
    }

    pub fn xp(self) -> i32 {
        match self {
            Self::Feed => 8,
            Self::Play => 15,
            Self::Clean => 10,
            Self::Walk => 12,
        }
    }
}

/// XP needed to go from `level` to `level + 1`; 0 at [`MAX_LEVEL`], where there is nothing to earn.
pub fn xp_to_next(level: i32) -> i32 {
    if level >= MAX_LEVEL { 0 } else { 60 + level * 40 }
}

/// A pet's level and the XP into it, after earning some.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub level: i32,
    pub xp: i32,
    /// How many levels this gained.
    pub gained: i32,
}

/// `gain` XP on top of `xp` at `level`: level-ups carry over; the top level holds no XP.
pub fn add_xp(level: i32, xp: i32, gain: i32) -> Progress {
    let (mut level, mut xp) = (level.clamp(1, MAX_LEVEL), xp.max(0) + gain.max(0));
    let from = level;
    while level < MAX_LEVEL && xp >= xp_to_next(level) {
        xp -= xp_to_next(level);
        level += 1;
    }
    if level >= MAX_LEVEL {
        xp = 0;
    }
    Progress {
        level,
        xp,
        gained: level - from,
    }
}

/// The shells a walk can turn up at `level`: both ends grow with the level.
pub fn walk_range(level: i32) -> (i64, i64) {
    let level = i64::from(level.clamp(1, MAX_LEVEL));
    (5 + level, 30 + 2 * level)
}

/// Shells found on a walk, from a 0..1 roll.
pub fn walk_shells(level: i32, roll: f64) -> i64 {
    let (low, high) = walk_range(level);
    low + (roll.clamp(0.0, 0.999_999) * (high - low + 1) as f64) as i64
}

/// How much better a pet of `level` finds things than one of level 1: 1.0 there, 3% more a level.
pub fn find_bonus(level: i32) -> f64 {
    1.0 + 0.03 * f64::from(level.clamp(1, MAX_LEVEL) - 1)
}

/// Draws a species weighted by `capsule_weight`, from a 0..1 roll.
pub fn hatch(content: &Content, roll: f64) -> Option<&PetSpecies> {
    let total: u64 = content.pets.iter().map(|p| u64::from(p.capsule_weight)).sum();
    if total == 0 {
        return None;
    }
    let mut target = (roll.clamp(0.0, 0.999_999) * total as f64) as u64;
    content.pets.iter().find(|p| {
        let w = u64::from(p.capsule_weight);
        if target < w {
            true
        } else {
            target -= w;
            false
        }
    })
}

/// Published odds per rarity, for the capsule screen.
pub fn odds(content: &Content) -> Vec<(crate::Rarity, f64)> {
    let total: f64 = content.pets.iter().map(|p| f64::from(p.capsule_weight)).sum();
    crate::Rarity::ALL
        .into_iter()
        .map(|r| {
            let w: f64 = content
                .pets
                .iter()
                .filter(|p| p.rarity == r)
                .map(|p| f64::from(p.capsule_weight))
                .sum();
            (r, if total > 0.0 { w / total } else { 0.0 })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::pets::*;

    #[test]
    fn needs_decay_and_care() {
        let n = Needs {
            hunger: 80,
            fun: 80,
            energy: 20,
            hygiene: 80,
        };
        let later = n.after(10.0);
        assert_eq!(
            (later.hunger, later.fun, later.energy, later.hygiene),
            (40, 30, 100, 55)
        );
        assert_eq!(n.after(100.0).hunger, 0);
        assert_eq!(Care::Feed.apply(n).unwrap().hunger, 100);
        assert!(Care::Play.apply(Needs { energy: 10, ..n }).is_none());
        assert_eq!(Care::Play.apply(n).unwrap().energy, 5);
        assert_eq!(Needs { hunger: 10, ..n }.mood(), "miserable");
    }

    #[test]
    fn a_pet_gets_low_when_the_first_of_its_needs_does() {
        let n = Needs {
            hunger: 80,
            fun: 80,
            energy: 100,
            hygiene: 80,
        };
        // Hunger loses 4 an hour and reaches 30 after 12.5 h, fun loses 5 and gets there after 10 h.
        assert!((n.hours_until_low() - 10.0).abs() < 0.01);
        assert_eq!(n.after(10.0).fun, 30);
        assert_eq!(Needs { fun: 20, ..n }.hours_until_low(), 0.0);
        assert_eq!(Needs { hunger: 10, ..n }.wants(), "something to eat");
        assert_eq!(Needs { fun: 10, ..n }.wants(), "someone to play with");
        assert_eq!(Needs { hygiene: 10, ..n }.wants(), "a bath");
    }

    #[test]
    fn xp_levels_up_carries_over_and_stops_at_the_top() {
        // Level 1 needs 100 XP, level 2 needs 140.
        assert_eq!(xp_to_next(1), 100);
        assert_eq!(xp_to_next(2), 140);
        assert_eq!(
            add_xp(1, 90, 8),
            Progress {
                level: 1,
                xp: 98,
                gained: 0
            }
        );
        assert_eq!(
            add_xp(1, 90, 15),
            Progress {
                level: 2,
                xp: 5,
                gained: 1
            }
        );
        // One big gain climbs several levels (100 + 140 + 180 = 420).
        assert_eq!(
            add_xp(1, 0, 430),
            Progress {
                level: 4,
                xp: 10,
                gained: 3
            }
        );
        // The top level holds no XP; nothing is earned past it.
        assert_eq!(xp_to_next(MAX_LEVEL), 0);
        let top = add_xp(MAX_LEVEL - 1, xp_to_next(MAX_LEVEL - 1) - 1, 50);
        assert_eq!(
            top,
            Progress {
                level: MAX_LEVEL,
                xp: 0,
                gained: 1
            }
        );
        assert_eq!(
            add_xp(MAX_LEVEL, 0, 500),
            Progress {
                level: MAX_LEVEL,
                xp: 0,
                gained: 0
            }
        );
        // A level read from an old row beyond the curve counts as the top.
        assert_eq!(add_xp(99, 7, 10).level, MAX_LEVEL);
        // The whole curve costs a believable amount: a few weeks of daily care.
        let total: i32 = (1..MAX_LEVEL).map(xp_to_next).sum();
        assert_eq!(total, 19_140);
    }

    #[test]
    fn walks_find_more_at_higher_levels() {
        assert_eq!(walk_range(1), (6, 32));
        assert_eq!(walk_range(30), (35, 90));
        let mut last = walk_range(1);
        for level in 2..=MAX_LEVEL {
            let now = walk_range(level);
            assert!(now.0 > last.0 && now.1 > last.1, "level {level}");
            last = now;
        }
        assert_eq!(walk_shells(1, 0.0), 6);
        assert_eq!(walk_shells(1, 1.0), 32);
        assert_eq!(walk_shells(30, 1.0), 90);
        assert!((find_bonus(1) - 1.0).abs() < 1e-9 && find_bonus(30) > 1.8);
    }

    #[test]
    fn a_pet_nobody_looked_after_falls_asleep_a_day_after_running_dry() {
        // Hunger runs out after 80/4 = 20 h, fun after 16 h, hygiene after 32 h: all dry at 32 h.
        let n = Needs {
            hunger: 80,
            fun: 80,
            energy: 80,
            hygiene: 80,
        };
        assert!((n.hours_until_drained() - 32.0).abs() < 1e-4);
        assert!((n.sleeps_after_hours() - 56.0).abs() < 1e-4);
        // Already dry: 24 h from then.
        let dry = Needs {
            hunger: 0,
            fun: 0,
            energy: 100,
            hygiene: 0,
        };
        assert_eq!(dry.sleeps_after_hours(), SLEEP_AFTER_HOURS);
        // Energy refills by itself and does not hold sleep off.
        assert_eq!(dry.after(SLEEP_AFTER_HOURS).energy, 100);
        // Waking leaves nothing dry, so the clock starts over.
        assert!(WAKE_NEEDS.sleeps_after_hours() > 40.0);
    }
}
