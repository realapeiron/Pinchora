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
}

impl Care {
    pub fn cooldown_secs(self) -> u64 {
        match self {
            Self::Feed => 30 * 60,
            Self::Play => 20 * 60,
            Self::Clean => 2 * 3600,
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
        }
    }

    pub fn xp(self) -> i32 {
        match self {
            Self::Feed => 8,
            Self::Play => 15,
            Self::Clean => 10,
        }
    }
}

/// XP needed to go from `level` to `level + 1`.
pub fn xp_to_next(level: i32) -> i32 {
    60 + level * 40
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
    use super::*;

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
}
