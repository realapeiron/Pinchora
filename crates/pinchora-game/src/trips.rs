//! Pet field trips: where a pet can go, how long it takes and what it brings home.

use std::sync::LazyLock;

use pinchora_core::i18n::Lang;
use serde::Deserialize;

use crate::{Availability, Category, Content, Currency, Furniture, Rarity, pets};

/// One destination, from `content/pet_trips.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct Trip {
    pub id: String,
    pub name: String,
    pub blurb: String,
    /// How long the pet is away.
    pub hours: u32,
    /// The pet level that opens this trip.
    pub unlock_level: i32,
    /// The least and the most shells a level-1 pet brings home.
    pub shells: [i64; 2],
    pub xp: i32,
    /// Chance that a level-1 pet also brings a piece of furniture.
    pub furniture_chance: f64,
    /// The rarest furniture this trip can turn up.
    pub max_rarity: Rarity,
    /// Furniture categories that fit the place. An empty draw falls back to any fitting rarity.
    pub categories: Vec<Category>,
    /// Story lines; `{pet}` stands for the pet's name.
    pub stories: Vec<String>,
}

static TRIPS: LazyLock<Vec<Trip>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../content/pet_trips.json"))
        .expect("embedded pet trips must parse; run `cargo test -p pinchora-game`")
});

/// Every destination, shortest and lowest level first.
pub fn all() -> &'static [Trip] {
    &TRIPS
}

pub fn get(id: &str) -> Option<&'static Trip> {
    TRIPS.iter().find(|t| t.id == id)
}

/// Dice for one claim, each in `0..1`, so the draw can be checked without a random source.
#[derive(Debug, Clone, Copy)]
pub struct Rolls {
    pub shells: f64,
    pub furniture: f64,
    pub pick: f64,
    pub story: f64,
}

/// What a pet brings home.
#[derive(Debug, Clone, PartialEq)]
pub struct Haul {
    pub shells: i64,
    pub xp: i32,
    /// An item id (`sofa` or `sofa__mint`) from the catalog.
    pub item: Option<String>,
    pub story: String,
}

/// How likely a find of this rarity is, before the pet's level shifts it towards the rare ones.
fn weight(rarity: Rarity, level: i32) -> f64 {
    let (base, rare_side) = match rarity {
        Rarity::Common => (50.0, false),
        Rarity::Uncommon => (28.0, false),
        Rarity::Rare => (14.0, true),
        Rarity::Epic => (6.0, true),
        Rarity::Legendary => (2.0, true),
    };
    if rare_side {
        base * (1.0 + 0.05 * f64::from(level.clamp(1, pets::MAX_LEVEL) - 1))
    } else {
        base
    }
}

impl Trip {
    /// "1 h", "4 h".
    pub fn length(&self) -> String {
        format!("{} h", self.hours)
    }

    /// What the place can turn up, as a short phrase.
    pub fn finds(&self) -> String {
        let places: Vec<String> = self.categories.iter().map(|c| c.label().to_lowercase()).collect();
        format!("shells, sometimes {}", places.join(", "))
    }

    /// The most and least shells a pet of `level` brings home.
    pub fn shells_range(&self, level: i32) -> (i64, i64) {
        let bonus = pets::find_bonus(level);
        (
            (self.shells[0] as f64 * bonus).round() as i64,
            (self.shells[1] as f64 * bonus).round() as i64,
        )
    }

    /// The chance of a furniture find for a pet of `level`.
    pub fn furniture_chance_at(&self, level: i32) -> f64 {
        (self.furniture_chance * pets::find_bonus(level)).min(0.9)
    }

    /// The pieces this trip can bring home: free-to-earn catalog furniture (`reward` items,
    /// never what the shop sells, never premium) up to the trip's rarity, in the place's
    /// categories; any category when none of those is left.
    pub fn pool<'a>(&self, content: &'a Content) -> Vec<&'a Furniture> {
        let fits = |f: &&Furniture| {
            f.availability == Availability::Reward && f.currency == Currency::Shells && f.rarity <= self.max_rarity
        };
        let themed: Vec<&Furniture> = content
            .furniture
            .iter()
            .filter(fits)
            .filter(|f| self.categories.contains(&f.category))
            .collect();
        if themed.is_empty() {
            content.furniture.iter().filter(fits).collect()
        } else {
            themed
        }
    }

    /// The haul of one trip claimed by a pet called `pet_name` at `level`.
    pub fn haul(&self, content: &Content, pet_name: &str, level: i32, rolls: Rolls) -> Haul {
        self.haul_in(content, pet_name, level, rolls, Lang::En)
    }

    /// [`Trip::haul`] with the story told in `lang`.
    pub fn haul_in(&self, content: &Content, pet_name: &str, level: i32, rolls: Rolls, lang: Lang) -> Haul {
        let (low, high) = self.shells_range(level);
        let shells = low + (rolls.shells.clamp(0.0, 0.999_999) * (high - low + 1) as f64) as i64;
        let item = if rolls.furniture < self.furniture_chance_at(level) {
            self.pick(content, level, rolls.pick)
        } else {
            None
        };
        let stories = self.stories_in(lang);
        let story = match stories.len() {
            0 => String::new(),
            n => {
                let at = (rolls.story.clamp(0.0, 0.999_999) * n as f64) as usize;
                stories[at].replace("{pet}", pet_name)
            }
        };
        Haul {
            shells,
            xp: self.xp,
            item,
            story,
        }
    }

    /// One piece of the pool, weighted by rarity; `roll` also chooses the colour of a variant.
    fn pick(&self, content: &Content, level: i32, roll: f64) -> Option<String> {
        let pool = self.pool(content);
        let total: f64 = pool.iter().map(|f| weight(f.rarity, level)).sum();
        let mut target = roll.clamp(0.0, 0.999_999) * total;
        for furniture in pool {
            let w = weight(furniture.rarity, level);
            if target < w {
                return Some(match furniture.variants.len() {
                    0 => furniture.id.clone(),
                    n => {
                        let at = ((target / w) * n as f64) as usize;
                        format!("{}__{}", furniture.id, furniture.variants[at.min(n - 1)])
                    }
                });
            }
            target -= w;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rolls(shells: f64, furniture: f64, pick: f64, story: f64) -> Rolls {
        Rolls {
            shells,
            furniture,
            pick,
            story,
        }
    }

    #[test]
    fn the_shipped_trips_are_well_formed() {
        let content = Content::get();
        assert_eq!(all().len(), 8);
        let mut seen = std::collections::HashSet::new();
        for t in all() {
            assert!(seen.insert(t.id.as_str()), "duplicate trip {}", t.id);
            assert!(matches!(t.hours, 1 | 4 | 8), "{}: {} h", t.id, t.hours);
            assert!((1..=pets::MAX_LEVEL).contains(&t.unlock_level), "{}", t.id);
            assert!(t.shells[0] > 0 && t.shells[0] <= t.shells[1], "{}", t.id);
            assert!(t.xp > 0 && (0.0..=1.0).contains(&t.furniture_chance), "{}", t.id);
            assert!(
                !t.stories.is_empty() && t.stories.iter().all(|s| s.contains("{pet}")),
                "{}",
                t.id
            );
            assert!(t.stories.iter().all(|s| s.len() <= 100), "{}: keep stories short", t.id);
            // Every place has furniture it can bring, and every id it can bring is real.
            assert!(!t.pool(content).is_empty(), "{} has nothing to find", t.id);
            for roll in [0.0, 0.3, 0.6, 0.99] {
                let id = t.pick(content, 30, roll).expect("a find");
                assert!(content.item(&id).is_some(), "{}: {id} is not an item", t.id);
            }
        }
        // Longer trips unlock later and pay more.
        let by_len = |h: u32| all().iter().filter(move |t| t.hours == h);
        for (short, long) in [(1, 4), (4, 8)] {
            assert!(by_len(short).map(|t| t.unlock_level).max() < by_len(long).map(|t| t.unlock_level).min());
            assert!(by_len(short).map(|t| t.shells[1]).max() < by_len(long).map(|t| t.shells[0]).max());
        }
        assert_eq!(all()[0].unlock_level, 1, "the first trip is open to everyone");
    }

    #[test]
    fn finds_are_free_to_earn_catalog_pieces_within_the_trips_rarity() {
        let content = Content::get();
        for t in all() {
            for f in t.pool(content) {
                assert_eq!(f.availability, Availability::Reward, "{}: {}", t.id, f.id);
                assert_eq!(f.currency, Currency::Shells, "{}: {}", t.id, f.id);
                assert!(f.rarity <= t.max_rarity, "{}: {}", t.id, f.id);
            }
        }
    }

    #[test]
    fn higher_levels_find_more() {
        let t = get("kelp_forest_library").expect("a trip");
        let (low1, high1) = t.shells_range(1);
        let (low30, high30) = t.shells_range(30);
        assert_eq!((low1, high1), (110, 190));
        assert!(low30 > low1 * 3 / 2 && high30 > high1 * 3 / 2, "{low30}..{high30}");
        assert!(t.furniture_chance_at(30) > t.furniture_chance_at(1) * 1.8);
        // The same dice buy more shells at level 30.
        let at1 = t.haul(Content::get(), "Pip", 1, rolls(0.99, 0.99, 0.0, 0.0)).shells;
        let at30 = t.haul(Content::get(), "Pip", 30, rolls(0.99, 0.99, 0.0, 0.0)).shells;
        assert!(at30 > at1);
        // The rare tiers get likelier with level.
        assert!(weight(Rarity::Epic, 30) > weight(Rarity::Epic, 1));
        assert_eq!(weight(Rarity::Common, 30), weight(Rarity::Common, 1));
    }

    #[test]
    fn a_haul_follows_the_dice() {
        let content = Content::get();
        let t = get("tidepool_meadow").expect("a trip");
        let none = t.haul(content, "Pip", 1, rolls(0.0, 0.99, 0.5, 0.0));
        assert_eq!(none.shells, 25);
        assert_eq!(none.xp, 25);
        assert!(none.item.is_none(), "a high furniture roll finds nothing");
        assert!(none.story.starts_with("Pip counted"), "{}", none.story);
        let top = t.haul(content, "Pip", 1, rolls(0.999, 0.0, 0.0, 0.999));
        assert_eq!(top.shells, 45);
        let item = top.item.expect("a low furniture roll finds something");
        assert!(content.item(&item).is_some(), "{item}");
        assert!(top.story.contains("Pip"));
        // Different pick dice reach different pieces.
        let picks: std::collections::HashSet<_> = (0..40)
            .filter_map(|n| t.pick(content, 1, f64::from(n) / 40.0))
            .collect();
        assert!(picks.len() >= 3, "{picks:?}");
    }
}
