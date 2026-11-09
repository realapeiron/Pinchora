//! Fishing: spots, rods and baits, which fish bite, how big they are and what they sell for.

use std::f64::consts::TAU;

use serde::Deserialize;

use crate::{Content, Fish, Rarity};

/// What plain bait costs for a cast, paid when no other bait is on the hook.
pub const BAIT_PRICE: i64 = 10;
pub const CAST_COOLDOWN_SECS: u64 = 30;
/// Fish the bucket holds; a full one stops the next cast until something is sold.
pub const BUCKET_CAPACITY: i64 = 60;
/// How long a cast stays valid, in seconds: the bite and the longest Reel! window must fit inside.
pub const LINE_TTL_SECS: u64 = 30;
/// The earliest a bite comes after the cast, in milliseconds.
pub const BITE_MIN_MS: i64 = 2_000;
/// The latest a bite comes after the cast. The span from the earliest is well over twice the longest
/// Reel! window, so pressing the button blind at one fixed moment lands inside a window for under two
/// casts in five, whatever the rod.
pub const BITE_MAX_MS: i64 = 11_000;

// /Content

/// A place to fish. The deeper ones open at a higher level.
#[derive(Debug, Clone, Deserialize)]
pub struct Spot {
    pub id: String,
    pub name: String,
    /// Global level that opens the spot.
    pub level: u32,
    pub blurb: String,
    /// How often a cast here comes up as each kind of catch, whatever the hour or season.
    pub shares: Shares,
    /// The spot's signature fish, the ones Tide Crumbs call.
    pub specials: Vec<String>,
}

/// The kind of catch a cast rolls first. Junk is its own kind; the rest are the fish rarities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Junk,
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl Tier {
    pub const ALL: [Tier; 6] = [
        Self::Junk,
        Self::Common,
        Self::Uncommon,
        Self::Rare,
        Self::Epic,
        Self::Legendary,
    ];

    /// The kind a fish is rolled as: boots and tins are junk, the others go by rarity.
    pub fn of(fish: &Fish) -> Tier {
        if fish.junk { Self::Junk } else { fish.rarity.into() }
    }
}

impl From<Rarity> for Tier {
    fn from(rarity: Rarity) -> Self {
        match rarity {
            Rarity::Common => Self::Common,
            Rarity::Uncommon => Self::Uncommon,
            Rarity::Rare => Self::Rare,
            Rarity::Epic => Self::Epic,
            Rarity::Legendary => Self::Legendary,
        }
    }
}

/// A spot's share of casts for each [`Tier`], in percent; a spot's shares add up to 100. A kind left out
/// is never rolled there.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default)]
pub struct Shares {
    pub junk: f64,
    pub common: f64,
    pub uncommon: f64,
    pub rare: f64,
    pub epic: f64,
    pub legendary: f64,
}

impl Shares {
    pub fn get(&self, tier: Tier) -> f64 {
        match tier {
            Tier::Junk => self.junk,
            Tier::Common => self.common,
            Tier::Uncommon => self.uncommon,
            Tier::Rare => self.rare,
            Tier::Epic => self.epic,
            Tier::Legendary => self.legendary,
        }
    }
}

/// A rod: it widens the Reel! window and tips the odds toward rare fish.
#[derive(Debug, Clone, Deserialize)]
pub struct Rod {
    pub id: String,
    pub name: String,
    /// Shells; 0 for the rod everyone starts with.
    pub price: i64,
    /// How long after the bite Reel! still lands.
    pub window_ms: i64,
    /// Multiplies the draw weight of rare, epic and legendary fish.
    pub luck: f64,
    pub blurb: String,
}

/// What a bait does to the odds while it is on the hook.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Boost {
    /// Every fish of one rarity is `factor` times as likely.
    Rarity { rarity: Rarity, factor: f64 },
    /// The spot's signature fish are `factor` times as likely.
    Specials { factor: f64 },
}

/// A bait, bought in packs and used up one per cast, instead of plain bait.
#[derive(Debug, Clone, Deserialize)]
pub struct Bait {
    pub id: String,
    pub name: String,
    /// Pieces in a pack.
    pub pack: i64,
    /// Shells for a whole pack.
    pub price: i64,
    pub boost: Boost,
    pub blurb: String,
}

/// A cosmetic that already exists in `backgrounds.json` or `badges.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reward {
    /// A shop badge, given instead of bought.
    Badge {
        id: String,
    },
    Background {
        id: String,
    },
}

/// A fish-book count that pays out once: shells and a cosmetic.
#[derive(Debug, Clone, Deserialize)]
pub struct Milestone {
    pub species: i64,
    pub shells: i64,
    pub reward: Reward,
}

/// Everything `fishing_gear.json` holds.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Gear {
    pub spots: Vec<Spot>,
    pub rods: Vec<Rod>,
    pub baits: Vec<Bait>,
    pub milestones: Vec<Milestone>,
}

impl Gear {
    pub fn spot(&self, id: &str) -> Option<&Spot> {
        self.spots.iter().find(|s| s.id == id)
    }

    pub fn rod(&self, id: &str) -> Option<&Rod> {
        self.rods.iter().find(|r| r.id == id)
    }

    pub fn bait(&self, id: &str) -> Option<&Bait> {
        self.baits.iter().find(|b| b.id == id)
    }

    /// The first spot, which everyone can fish.
    pub fn first_spot(&self) -> &Spot {
        &self.spots[0]
    }

    /// The rod everyone starts with: the free one.
    pub fn starter_rod(&self) -> &Rod {
        self.rods.iter().find(|r| r.price == 0).unwrap_or(&self.rods[0])
    }

    /// Whether a player of `level` may fish `spot`. The first spot is open to all, even to a player
    /// still at level 0.
    pub fn is_open(&self, spot: &Spot, level: u32) -> bool {
        spot.level <= level || spot.id == self.first_spot().id
    }

    /// The spots a player of `level` may fish, shallowest first.
    pub fn unlocked(&self, level: u32) -> impl Iterator<Item = &Spot> {
        self.spots.iter().filter(move |s| self.is_open(s, level))
    }

    /// The spot to use for `wanted`: itself if the player may fish it, else the deepest one they may.
    pub fn spot_for(&self, wanted: &str, level: u32) -> &Spot {
        self.spot(wanted)
            .filter(|s| self.is_open(s, level))
            .or_else(|| self.unlocked(level).last())
            .unwrap_or_else(|| self.first_spot())
    }

    /// Milestones a book of `species` has reached, smallest first.
    pub fn milestones_reached(&self, species: i64) -> impl Iterator<Item = &Milestone> {
        self.milestones.iter().filter(move |m| m.species <= species)
    }

    /// The first milestone a book of `species` has not reached.
    pub fn next_milestone(&self, species: i64) -> Option<&Milestone> {
        self.milestones.iter().find(|m| m.species > species)
    }
}

// /Fish

impl Fish {
    /// The middle of the size range: what a fish of this kind usually measures.
    pub fn mean_cm(&self) -> f32 {
        (self.min_cm + self.max_cm) / 2.0
    }

    /// Whether the fish is about at this hour (UTC) and month.
    pub fn bites_at(&self, hour: u32, month: u32) -> bool {
        (self.time == "any" || self.time == time_of_day(hour)) && (self.season == "any" || self.season == season(month))
    }
}

/// Part of the day for a UTC hour.
pub fn time_of_day(hour: u32) -> &'static str {
    match hour {
        5..=6 => "dawn",
        7..=17 => "day",
        18..=19 => "dusk",
        _ => "night",
    }
}

/// Season for a month 1..=12 (northern hemisphere).
pub fn season(month: u32) -> &'static str {
    match month {
        3..=5 => "spring",
        6..=8 => "summer",
        9..=11 => "autumn",
        _ => "winter",
    }
}

// /Odds

/// What shapes a cast's odds besides the spot and the clock.
#[derive(Debug, Clone, Copy)]
pub struct Tackle<'a> {
    /// The rod's [`Rod::luck`].
    pub luck: f64,
    pub bait: Option<&'a Bait>,
}

impl Tackle<'_> {
    /// Plain bait on the starter rod.
    pub const PLAIN: Tackle<'static> = Tackle { luck: 1.0, bait: None };
}

/// The chance of rolling each [`Tier`] at `spot`, in [`Tier::ALL`] order, adding up to 1. The spot's shares
/// are fixed; the clock does not move them. The rod multiplies the rare, epic and legendary shares and a
/// rarity bait the share of its own tier, and what is left over is shared out again.
pub fn tier_chances(spot: &Spot, tackle: Tackle<'_>) -> [f64; 6] {
    let mut chances = Tier::ALL.map(|tier| {
        let mut share = spot.shares.get(tier);
        if tier >= Tier::Rare {
            share *= tackle.luck;
        }
        if let Some(Boost::Rarity { rarity, factor }) = tackle.bait.map(|b| b.boost)
            && Tier::from(rarity) == tier
        {
            share *= factor;
        }
        share
    });
    let total: f64 = chances.iter().sum();
    if total > 0.0 {
        chances.iter_mut().for_each(|chance| *chance /= total);
    }
    chances
}

/// The fish of each [`Tier`] that can bite now at `spot`, each with its weight within the tier: the weight
/// in `fish.json`, times a specials bait's factor for the spot's signature fish.
fn pools<'a>(
    content: &'a Content,
    spot: &Spot,
    hour: u32,
    month: u32,
    tackle: Tackle<'_>,
) -> [Vec<(&'a Fish, f64)>; 6] {
    let mut pools: [Vec<(&Fish, f64)>; 6] = Default::default();
    for fish in content.fish.iter().filter(|f| f.bites_at(hour, month)) {
        let Some(base) = fish.spots.get(&spot.id) else {
            continue;
        };
        let mut weight = f64::from(*base);
        if let Some(Boost::Specials { factor }) = tackle.bait.map(|b| b.boost)
            && spot.specials.contains(&fish.id)
        {
            weight *= factor;
        }
        if weight > 0.0 {
            pools[Tier::of(fish) as usize].push((fish, weight));
        }
    }
    pools
}

/// Where a roll of `rolled` ends up when `has` says which tiers have a fish about right now: the tier
/// itself, else the nearest other one that has one, the lower one on a tie. Junk is only ever rolled, never
/// fallen back to, and a junk roll with no junk about goes to the common fish.
pub fn settle(rolled: Tier, has: impl Fn(Tier) -> bool) -> Option<Tier> {
    if rolled == Tier::Junk && has(Tier::Junk) {
        return Some(Tier::Junk);
    }
    let ladder = &Tier::ALL[1..];
    let start = ladder.iter().position(|t| *t == rolled).unwrap_or(0);
    (0..ladder.len()).find_map(|distance| {
        let below = start.checked_sub(distance).map(|i| ladder[i]);
        let above = ladder.get(start + distance).copied();
        [below, above].into_iter().flatten().find(|t| has(*t))
    })
}

/// The tier a 0..1 roll lands on, by the chances of [`tier_chances`].
pub fn pick_tier(chances: &[f64; 6], roll: f64) -> Option<Tier> {
    let total: f64 = chances.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let mut target = roll.clamp(0.0, 0.999_999) * total;
    let mut last = None;
    for (tier, chance) in Tier::ALL.into_iter().zip(chances) {
        if *chance <= 0.0 {
            continue;
        }
        if target < *chance {
            return Some(tier);
        }
        target -= chance;
        last = Some(tier);
    }
    last
}

/// The fish that can bite now at `spot`, each with its chance of being the catch of a cast (they add up to
/// 1): the chance of its tier, after any fallback, times its weight among the tier's fish about now.
pub fn odds<'a>(content: &'a Content, spot: &Spot, hour: u32, month: u32, tackle: Tackle<'_>) -> Vec<(&'a Fish, f64)> {
    let pools = pools(content, spot, hour, month, tackle);
    let mut landed = [0.0; 6];
    for (tier, chance) in Tier::ALL.into_iter().zip(tier_chances(spot, tackle)) {
        if chance > 0.0
            && let Some(at) = settle(tier, |t| !pools[t as usize].is_empty())
        {
            landed[at as usize] += chance;
        }
    }
    pools
        .into_iter()
        .zip(landed)
        .flat_map(|(pool, chance)| {
            let total: f64 = pool.iter().map(|(_, w)| w).sum();
            pool.into_iter()
                .map(move |(fish, weight)| (fish, chance * weight / total))
        })
        .filter(|(_, chance)| *chance > 0.0)
        .collect()
}

/// Picks a fish from `odds` by a 0..1 roll.
pub fn pick<'a>(odds: &[(&'a Fish, f64)], roll: f64) -> Option<&'a Fish> {
    let total: f64 = odds.iter().map(|(_, w)| w).sum();
    if odds.is_empty() || total <= 0.0 {
        return None;
    }
    let mut target = roll.clamp(0.0, 0.999_999) * total;
    for (fish, weight) in odds {
        if target < *weight {
            return Some(fish);
        }
        target -= weight;
    }
    odds.last().map(|(fish, _)| *fish)
}

/// The catch of one cast: roll the tier first (`tier_roll`, 0..1), settle on the nearest tier with a fish
/// about now if the roll's own has none, then pick among that tier's fish (`fish_roll`, 0..1).
pub fn catch_fish<'a>(
    content: &'a Content,
    spot: &Spot,
    hour: u32,
    month: u32,
    tackle: Tackle<'_>,
    tier_roll: f64,
    fish_roll: f64,
) -> Option<&'a Fish> {
    let pools = pools(content, spot, hour, month, tackle);
    let rolled = pick_tier(&tier_chances(spot, tackle), tier_roll)?;
    let tier = settle(rolled, |t| !pools[t as usize].is_empty())?;
    pick(&pools[tier as usize], fish_roll)
}

// /Reel

/// The delay between a cast and its bite from a 0..1 roll, spread evenly over
/// [`BITE_MIN_MS`]..[`BITE_MAX_MS`].
pub fn bite_delay_ms(roll: f64) -> i64 {
    let span = (BITE_MAX_MS - BITE_MIN_MS) as f64;
    BITE_MIN_MS + (roll.clamp(0.0, 0.999_999) * span) as i64
}

/// How a press of Reel! turned out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reel {
    /// Pressed before the bite: the splash scares the fish away.
    Early,
    /// Pressed inside the rod's window after the bite.
    Caught,
    /// Pressed after the window closed.
    Late,
}

/// The outcome of pressing Reel! `late_by_ms` after the bite (negative before it) with a rod whose
/// window is `window_ms`.
pub fn reel_result(late_by_ms: i64, window_ms: i64) -> Reel {
    if late_by_ms < 0 {
        Reel::Early
    } else if late_by_ms > window_ms {
        Reel::Late
    } else {
        Reel::Caught
    }
}

// /Sizes

/// A size in cm from two 0..1 rolls: normal around the middle of the species' range, three standard
/// deviations to either end of it, to a tenth of a centimetre.
pub fn roll_size(fish: &Fish, first: f64, second: f64) -> f32 {
    let mean = f64::from(fish.mean_cm());
    let deviation = f64::from(fish.max_cm - fish.min_cm) / 6.0;
    // Box-Muller; the first roll must not be 0.
    let z = (-2.0 * first.clamp(1e-12, 1.0).ln()).sqrt() * (TAU * second).cos();
    let size = (mean + deviation * z.clamp(-3.0, 3.0)) as f32;
    ((size * 10.0).round() / 10.0).clamp(fish.min_cm, fish.max_cm)
}

/// How big a catch is for its species: 1.0 is the usual size.
pub fn size_ratio(fish: &Fish, size_cm: f32) -> f32 {
    size_cm / fish.mean_cm().max(0.1)
}

/// What the catch sells for: the species' worth, scaled by its size.
pub fn sale_value(fish: &Fish, size_cm: f32) -> i64 {
    ((fish.base_value as f32) * size_ratio(fish, size_cm)).round().max(1.0) as i64
}

#[cfg(test)]
#[cfg(test)]
#[path = "fishing/tests.rs"]
mod tests;
