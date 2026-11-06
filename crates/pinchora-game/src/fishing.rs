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
mod tests {
    use super::*;

    #[test]
    fn day_parts() {
        assert_eq!(time_of_day(6), "dawn");
        assert_eq!(time_of_day(12), "day");
        assert_eq!(time_of_day(23), "night");
        assert_eq!(season(1), "winter");
        assert_eq!(season(7), "summer");
    }

    fn content() -> &'static Content {
        Content::get()
    }

    fn fish(id: &str) -> &'static Fish {
        content().fish(id).expect("a fish")
    }

    #[test]
    fn every_fish_lives_in_a_known_spot_with_a_weight() {
        let gear = &content().fishing;
        for f in &content().fish {
            assert!(!f.spots.is_empty(), "{} lives nowhere", f.id);
            for (spot, weight) in &f.spots {
                assert!(gear.spot(spot).is_some(), "{} lives in unknown spot {spot}", f.id);
                assert!(*weight > 0, "{} has no weight at {spot}", f.id);
            }
        }
    }

    #[test]
    fn every_fish_is_catchable_at_every_spot_it_lives_in_at_some_hour_and_month() {
        let gear = &content().fishing;
        for f in &content().fish {
            for spot in f.spots.keys() {
                let spot = gear.spot(spot).expect("spot");
                let mut caught = false;
                'search: for hour in 0..24 {
                    for month in 1..=12 {
                        if odds(content(), spot, hour, month, Tackle::PLAIN)
                            .iter()
                            .any(|(g, _)| g.id == f.id)
                        {
                            caught = true;
                            break 'search;
                        }
                    }
                }
                assert!(caught, "{} never bites at {}", f.id, spot.id);
            }
        }
    }

    #[test]
    fn every_fish_can_be_caught_by_a_player_who_has_reached_its_spot() {
        let gear = &content().fishing;
        for f in &content().fish {
            let first = f
                .spots
                .keys()
                .filter_map(|s| gear.spot(s))
                .map(|s| s.level)
                .min()
                .expect("a spot");
            // At the level that opens its shallowest spot, in some hour of some month, with plain bait.
            let open = gear.unlocked(first).any(|spot| {
                f.spots.contains_key(&spot.id)
                    && (0..24).any(|h| {
                        (1..=12).any(|m| {
                            odds(content(), spot, h, m, Tackle::PLAIN)
                                .iter()
                                .any(|(g, _)| g.id == f.id)
                        })
                    })
            });
            assert!(open, "{} is out of reach", f.id);
        }
    }

    #[test]
    fn spots_open_in_order_and_the_first_is_open_to_all() {
        let spots = &content().fishing.spots;
        assert_eq!(spots.len(), 5);
        assert_eq!(
            spots.iter().map(|s| s.level).collect::<Vec<_>>(),
            [1, 5, 12, 20, 30],
            "unlocked by global level"
        );
        assert_eq!(content().fishing.first_spot().level, 1);
    }

    /// Every hour (UTC) of every month: all the cells of the clock a cast can fall in.
    fn cells() -> impl Iterator<Item = (u32, u32)> {
        (0..24).flat_map(|hour| (1..=12).map(move |month| (hour, month)))
    }

    fn tier_name(tier: Tier) -> &'static str {
        ["junk", "common", "uncommon", "rare", "epic", "legendary"][tier as usize]
    }

    #[test]
    fn something_always_bites_at_every_spot() {
        for spot in &content().fishing.spots {
            for (hour, month) in cells() {
                let pool = odds(content(), spot, hour, month, Tackle::PLAIN);
                assert!(!pool.is_empty(), "{} is empty at hour {hour}, month {month}", spot.id);
                let total: f64 = pool.iter().map(|(_, p)| p).sum();
                assert!(
                    (total - 1.0).abs() < 1e-9,
                    "{} sums to {total} at {hour}:{month}",
                    spot.id
                );
            }
        }
    }

    #[test]
    fn shares_add_up_and_fall_with_rarity() {
        let content = content();
        for spot in &content.fishing.spots {
            let total: f64 = Tier::ALL.iter().map(|t| spot.shares.get(*t)).sum();
            assert!((total - 100.0).abs() < 1e-6, "{}: shares add up to {total}", spot.id);
            let lives = |tier: Tier| {
                content
                    .fish
                    .iter()
                    .any(|f| Tier::of(f) == tier && f.spots.contains_key(&spot.id))
            };
            for tier in Tier::ALL {
                // A kind is rolled exactly where something of it lives, so nothing is unreachable
                // and nothing is rolled that cannot be caught.
                assert_eq!(
                    spot.shares.get(tier) > 0.0,
                    lives(tier),
                    "{}: the {} share and its fish disagree",
                    spot.id,
                    tier_name(tier)
                );
            }
            // Rarer is rarer: each kind of fish comes up less often than the one below it, and junk is
            // never the bulk of a spot.
            for pair in Tier::ALL[1..].windows(2) {
                let (common, rarer) = (spot.shares.get(pair[0]), spot.shares.get(pair[1]));
                assert!(
                    rarer == 0.0 || rarer < common,
                    "{}: {} is not rarer than {}",
                    spot.id,
                    tier_name(pair[1]),
                    tier_name(pair[0])
                );
            }
            assert!(spot.shares.junk <= 12.0, "{}: too much junk", spot.id);
        }
        // Deeper is richer: from one spot to the next the share of rare, epic and legendary fish never falls.
        for pair in content.fishing.spots.windows(2) {
            for tier in [Tier::Rare, Tier::Epic, Tier::Legendary] {
                assert!(
                    pair[1].shares.get(tier) >= pair[0].shares.get(tier),
                    "{} has fewer {} fish than {}",
                    pair[1].id,
                    tier_name(tier),
                    pair[0].id
                );
            }
        }
    }

    #[test]
    fn every_kind_a_spot_rolls_has_a_fish_about_at_every_hour_and_month() {
        // Time and season are flavour inside a kind: no hour is ever empty, so a roll is never moved
        // to another kind.
        let content = content();
        for spot in &content.fishing.spots {
            for tier in Tier::ALL.into_iter().filter(|t| spot.shares.get(*t) > 0.0) {
                for (hour, month) in cells() {
                    let pools = pools(content, spot, hour, month, Tackle::PLAIN);
                    assert!(
                        !pools[tier as usize].is_empty(),
                        "{}: no {} fish at hour {hour}, month {month}",
                        spot.id,
                        tier_name(tier)
                    );
                }
            }
        }
    }

    #[test]
    fn the_clock_never_moves_the_tier_shares() {
        let (content, gear) = (content(), &content().fishing);
        let top = gear.rods.last().expect("a rod");
        for spot in &gear.spots {
            for bait in std::iter::once(None).chain(gear.baits.iter().map(Some)) {
                for luck in [1.0, top.luck] {
                    let tackle = Tackle { luck, bait };
                    let wanted = tier_chances(spot, tackle);
                    assert!((wanted.iter().sum::<f64>() - 1.0).abs() < 1e-9);
                    for (hour, month) in cells() {
                        let mut got = [0.0; 6];
                        for (fish, chance) in odds(content, spot, hour, month, tackle) {
                            got[Tier::of(fish) as usize] += chance;
                        }
                        for tier in Tier::ALL {
                            assert!(
                                (got[tier as usize] - wanted[tier as usize]).abs() < 1e-9,
                                "{} at {hour}:{month}: {} share is {} not {}",
                                spot.id,
                                tier_name(tier),
                                got[tier as usize],
                                wanted[tier as usize]
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn an_empty_tier_falls_back_to_the_nearest_one_never_to_junk() {
        let only = |tiers: &'static [Tier]| move |tier: Tier| tiers.contains(&tier);
        // Its own tier first.
        assert_eq!(settle(Tier::Epic, only(&Tier::ALL)), Some(Tier::Epic));
        assert_eq!(settle(Tier::Junk, only(&Tier::ALL)), Some(Tier::Junk));
        // The nearest one, either side.
        assert_eq!(settle(Tier::Epic, only(&[Tier::Common, Tier::Rare])), Some(Tier::Rare));
        assert_eq!(
            settle(Tier::Rare, only(&[Tier::Common, Tier::Legendary])),
            Some(Tier::Common)
        );
        assert_eq!(settle(Tier::Uncommon, only(&[Tier::Legendary])), Some(Tier::Legendary));
        // A tie goes to the lower, cheaper tier.
        assert_eq!(
            settle(Tier::Epic, only(&[Tier::Rare, Tier::Legendary])),
            Some(Tier::Rare)
        );
        assert_eq!(
            settle(Tier::Uncommon, only(&[Tier::Common, Tier::Rare])),
            Some(Tier::Common)
        );
        // Junk is rolled, never fallen back to; a junk roll with no junk about is a common fish.
        assert_eq!(settle(Tier::Common, only(&[Tier::Junk, Tier::Rare])), Some(Tier::Rare));
        assert_eq!(settle(Tier::Rare, only(&[Tier::Junk])), None);
        assert_eq!(
            settle(Tier::Junk, only(&[Tier::Uncommon, Tier::Rare])),
            Some(Tier::Uncommon)
        );
        assert_eq!(settle(Tier::Rare, |_| false), None);
    }

    #[test]
    fn a_tier_nothing_lives_in_moves_its_chance_to_the_nearest_tier() {
        // The shore has no epic fish; give it an epic share anyway. The epic rolls settle for rare fish.
        let content = content();
        let mut spot = content.fishing.spot("shore").expect("shore").clone();
        spot.shares = Shares {
            common: 50.0,
            epic: 50.0,
            ..Shares::default()
        };
        let pool = odds(content, &spot, 12, 6, Tackle::PLAIN);
        let rare: f64 = pool
            .iter()
            .filter(|(f, _)| f.rarity == Rarity::Rare)
            .map(|(_, p)| p)
            .sum();
        assert!((rare - 0.5).abs() < 1e-9, "the epic rolls became rare fish: {rare}");
        assert!((pool.iter().map(|(_, p)| p).sum::<f64>() - 1.0).abs() < 1e-9);
        let caught = catch_fish(content, &spot, 12, 6, Tackle::PLAIN, 0.99, 0.5).expect("a fish");
        assert_eq!(caught.rarity, Rarity::Rare);
        let caught = catch_fish(content, &spot, 12, 6, Tackle::PLAIN, 0.01, 0.5).expect("a fish");
        assert_eq!(caught.rarity, Rarity::Common);
        // A spot nobody lives at has nothing to fall back to.
        spot.id = "nowhere".into();
        assert!(odds(content, &spot, 12, 6, Tackle::PLAIN).is_empty());
        assert!(catch_fish(content, &spot, 12, 6, Tackle::PLAIN, 0.5, 0.5).is_none());
    }

    #[test]
    fn a_cast_follows_the_odds() {
        // Roll the tier at the middle of each tier's stretch of 0..1 and sweep the fish roll: the fish
        // come out in the proportions the odds table gives.
        let (content, gear) = (content(), &content().fishing);
        let abyss = gear.spot("abyss").expect("abyss");
        let tackle = Tackle {
            luck: 1.27,
            bait: gear.bait("tide_crumbs"),
        };
        let (hour, month) = (22, 12);
        let table = odds(content, abyss, hour, month, tackle);
        let chances = tier_chances(abyss, tackle);
        let mut counts = std::collections::HashMap::<&str, f64>::new();
        let mut start = 0.0;
        const FISH_ROLLS: u32 = 2_000;
        for chance in chances {
            for n in 0..FISH_ROLLS {
                let tier_roll = start + chance * 0.5;
                let fish_roll = (f64::from(n) + 0.5) / f64::from(FISH_ROLLS);
                if chance > 0.0 {
                    let fish = catch_fish(content, abyss, hour, month, tackle, tier_roll, fish_roll).expect("a fish");
                    *counts.entry(fish.id.as_str()).or_default() += chance / f64::from(FISH_ROLLS);
                }
            }
            start += chance;
        }
        assert_eq!(counts.len(), table.len());
        for (fish, chance) in &table {
            let got = counts[fish.id.as_str()];
            // The sweep of the fish roll is coarse to one part in FISH_ROLLS of the fish's tier.
            let slack = 2.0 * chances[Tier::of(fish) as usize] / f64::from(FISH_ROLLS);
            assert!((got - chance).abs() < slack, "{}: {got} against {chance}", fish.id);
        }
        // The ends of the roll land on the ends of the table.
        assert_eq!(pick_tier(&chances, 0.0), Some(Tier::Junk));
        assert_eq!(pick_tier(&chances, 1.0), Some(Tier::Legendary));
        assert_eq!(pick_tier(&[0.0; 6], 0.5), None);
    }

    #[test]
    fn odds_walk_the_whole_table() {
        let spot = content().fishing.spot("reef").expect("reef");
        let pool = odds(content(), spot, 12, 6, Tackle::PLAIN);
        let total: f64 = pool.iter().map(|(_, p)| p).sum();
        assert!((total - 1.0).abs() < 1e-9);
        // The roll walks the whole table: every fish in it is reachable and nothing else is.
        let mut seen: Vec<&str> = (0..2000)
            .filter_map(|n| pick(&pool, f64::from(n) / 2000.0))
            .map(|f| f.id.as_str())
            .collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), pool.len());
        assert!(pick(&[], 0.5).is_none());
    }

    fn share_of(pool: &[(&Fish, f64)], keep: impl Fn(&Fish) -> bool) -> f64 {
        let total: f64 = pool.iter().map(|(_, w)| w).sum();
        pool.iter().filter(|(f, _)| keep(f)).map(|(_, w)| w).sum::<f64>() / total
    }

    #[test]
    fn better_rods_raise_the_chance_of_rare_fish() {
        let (content, gear) = (content(), &content().fishing);
        let spot = gear.spot("deep_sea").expect("deep sea");
        let rare_share = |rod: &Rod| {
            let pool = odds(
                content,
                spot,
                22,
                12,
                Tackle {
                    luck: rod.luck,
                    bait: None,
                },
            );
            share_of(&pool, |f| f.rarity >= Rarity::Rare)
        };
        let shares: Vec<f64> = gear.rods.iter().map(rare_share).collect();
        assert!(shares.windows(2).all(|w| w[0] < w[1]), "{shares:?}");
        // Rods only ever get better: wider window, more luck, higher price.
        assert!(
            gear.rods
                .windows(2)
                .all(|w| { w[0].window_ms < w[1].window_ms && w[0].luck < w[1].luck && w[0].price < w[1].price })
        );
        assert_eq!(gear.starter_rod().price, 0);
        assert_eq!(
            gear.starter_rod().window_ms,
            2_500,
            "the window everyone had before rods"
        );
    }

    #[test]
    fn baits_boost_their_tier_or_the_spots_specials() {
        let (content, gear) = (content(), &content().fishing);
        let reef = gear.spot("reef").expect("reef");
        let with = |bait: Option<&str>, hour, month| {
            odds(
                content,
                reef,
                hour,
                month,
                Tackle {
                    luck: 1.0,
                    bait: bait.and_then(|id| gear.bait(id)),
                },
            )
        };
        let plain = with(None, 12, 7);
        let uncommon = |pool: &[(&Fish, f64)]| share_of(pool, |f| Tier::of(f) == Tier::Uncommon);
        let rare = |pool: &[(&Fish, f64)]| share_of(pool, |f| Tier::of(f) == Tier::Rare);
        let special = |pool: &[(&Fish, f64)]| share_of(pool, |f| reef.specials.contains(&f.id));
        assert!(uncommon(&with(Some("glow_worm"), 12, 7)) > 2.0 * uncommon(&plain));
        assert!(rare(&with(Some("silver_lure"), 12, 7)) > 2.0 * rare(&plain));
        // A bait leaves the other tiers' fish where they were: nothing drops out of the table.
        assert_eq!(with(Some("glow_worm"), 12, 7).len(), plain.len());
        // Tide Crumbs call the signature fish of the spot, whichever of them is about, and only inside
        // their own tier: the tier shares are the same.
        let crumbs = with(Some("tide_crumbs"), 12, 7);
        assert!(
            special(&crumbs) > 1.5 * special(&plain),
            "{}",
            special(&crumbs) / special(&plain)
        );
        assert!(
            (rare(&crumbs) - rare(&plain)).abs() < 1e-9,
            "the crumbs left the rare share alone"
        );
        // A special that is not about changes nothing: at the shore at noon in summer none of its three is.
        let shore = gear.spot("shore").expect("shore");
        let at_shore = |bait: Option<&str>, hour, month| {
            odds(
                content,
                shore,
                hour,
                month,
                Tackle {
                    luck: 1.0,
                    bait: bait.and_then(|id| gear.bait(id)),
                },
            )
        };
        let (quiet, calling) = (at_shore(None, 12, 7), at_shore(Some("tide_crumbs"), 12, 7));
        assert_eq!(quiet.len(), calling.len());
        assert!(
            quiet
                .iter()
                .zip(&calling)
                .all(|((f, p), (g, q))| f.id == g.id && (p - q).abs() < 1e-12)
        );
        // At dawn in spring two of them are, and they get likelier.
        let (quiet, calling) = (at_shore(None, 5, 4), at_shore(Some("tide_crumbs"), 5, 4));
        let shore_special = |pool: &[(&Fish, f64)]| share_of(pool, |f| shore.specials.contains(&f.id));
        assert!(shore_special(&calling) > 1.5 * shore_special(&quiet));
    }

    #[test]
    fn spots_name_specials_that_live_there() {
        for spot in &content().fishing.spots {
            assert!(spot.specials.len() >= 2, "{} needs signature fish", spot.id);
            for id in &spot.specials {
                let fish = fish(id);
                assert!(fish.spots.contains_key(&spot.id), "{id} is not a {} fish", spot.id);
                assert!(fish.rarity <= Rarity::Epic, "{id} is too grand for a bait");
            }
        }
    }

    #[test]
    fn size_rolls_stay_in_range_and_centre_on_the_mean() {
        for f in &content().fish {
            let (mut sum, mut n) = (0.0f64, 0u32);
            let (mut low, mut high) = (f32::MAX, f32::MIN);
            for a in 1..=40 {
                for b in 0..40 {
                    let size = roll_size(f, f64::from(a) / 40.0, f64::from(b) / 40.0);
                    assert!(size >= f.min_cm && size <= f.max_cm, "{}: {size}", f.id);
                    low = low.min(size);
                    high = high.max(size);
                    sum += f64::from(size);
                    n += 1;
                }
            }
            let mean = sum / f64::from(n);
            let spread = f64::from(f.max_cm - f.min_cm);
            assert!(
                (mean - f64::from(f.mean_cm())).abs() < spread * 0.05,
                "{}: mean {mean}",
                f.id
            );
            assert!(low < f.mean_cm() && high > f.mean_cm());
        }
    }

    #[test]
    fn extreme_rolls_reach_the_ends_of_the_range() {
        let koi = fish("pearlpatch_koi");
        // A tiny first roll is a huge number of deviations out: the biggest a koi can be.
        assert_eq!(roll_size(koi, 1e-9, 0.0), koi.max_cm);
        assert_eq!(roll_size(koi, 1e-9, 0.5), koi.min_cm);
        // A first roll of 1 is no deviation at all: the middle.
        assert!((roll_size(koi, 1.0, 0.1) - koi.mean_cm()).abs() < 0.1);
        // Normal, not flat: most sizes land near the middle.
        let near = (0..1000)
            .filter(|n| {
                let size = roll_size(koi, (f64::from(*n) + 0.5) / 1000.0, 0.25);
                (size - koi.mean_cm()).abs() < (koi.max_cm - koi.min_cm) / 6.0
            })
            .count();
        assert!(near > 300, "{near}");
    }

    #[test]
    fn a_bigger_catch_sells_for_more() {
        let koi = fish("pearlpatch_koi");
        let usual = sale_value(koi, koi.mean_cm());
        assert_eq!(usual, koi.base_value);
        assert!(sale_value(koi, koi.max_cm) > usual);
        assert!(sale_value(koi, koi.min_cm) < usual);
        // Never nothing, even for the smallest boot.
        assert!(content().fish.iter().all(|f| sale_value(f, f.min_cm) >= 1));
        assert!((size_ratio(koi, koi.mean_cm()) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn junk_is_the_six_odds_and_ends() {
        let junk: Vec<_> = content()
            .fish
            .iter()
            .filter(|f| f.junk)
            .map(|f| f.id.as_str())
            .collect();
        assert_eq!(
            junk,
            [
                "siltstep_boot",
                "rattlelip_tin",
                "frayloop_rope",
                "driftspoke_wheel",
                "chipwink_cup",
                "snaggletoe_sock"
            ]
        );
    }

    #[test]
    fn a_locked_spot_falls_back_to_the_deepest_open_one() {
        let gear = &content().fishing;
        assert_eq!(gear.spot_for("abyss", 1).id, "shore");
        assert_eq!(gear.spot_for("abyss", 14).id, "kelp_forest");
        assert_eq!(gear.spot_for("reef", 14).id, "reef");
        assert_eq!(gear.spot_for("abyss", 30).id, "abyss");
        assert_eq!(gear.spot_for("nowhere", 20).id, "deep_sea");
        assert_eq!(
            gear.unlocked(0).count(),
            1,
            "a new player at level 0 still has the shore"
        );
        assert_eq!(gear.spot_for("shore", 0).id, "shore");
    }

    #[test]
    fn milestones_pay_known_cosmetics_and_end_with_the_whole_book() {
        let (content, gear) = (content(), &content().fishing);
        let counts: Vec<i64> = gear.milestones.iter().map(|m| m.species).collect();
        assert_eq!(counts, [10, 25, 50, 72]);
        assert_eq!(
            counts.last().copied(),
            Some(content.fish.len() as i64),
            "the last is the full book"
        );
        assert!(counts.windows(2).all(|w| w[0] < w[1]));
        for m in &gear.milestones {
            assert!(m.shells > 0);
            match &m.reward {
                Reward::Badge { id } => {
                    let badge = content.badge(id).expect("a badge");
                    assert!(
                        matches!(badge.kind, crate::BadgeKind::Shop { .. }),
                        "{id} must be a shop badge: achievements are worked out from stats, not given"
                    );
                }
                Reward::Background { id } => assert!(content.background(id).is_some(), "no background {id}"),
            }
        }
        assert_eq!(gear.milestones_reached(9).count(), 0);
        assert_eq!(gear.milestones_reached(25).count(), 2);
        assert_eq!(gear.next_milestone(25).map(|m| m.species), Some(50));
        assert!(gear.next_milestone(72).is_none());
    }

    // /Economy

    /// The most a spot may pay an hour of constant casting with every reel caught, with any rod and bait,
    /// at any hour of any month.
    const CEILINGS: [(&str, f64); 5] = [
        ("shore", 1_200.0),
        ("reef", 1_700.0),
        ("kelp_forest", 2_300.0),
        ("deep_sea", 3_000.0),
        ("abyss", 4_000.0),
    ];

    /// What a fish is worth on average over the sizes it rolls.
    fn expected_sale(fish: &Fish) -> f64 {
        static WORTH: std::sync::OnceLock<std::collections::HashMap<String, f64>> = std::sync::OnceLock::new();
        const STEPS: u32 = 40;
        let worth = WORTH.get_or_init(|| {
            content()
                .fish
                .iter()
                .map(|fish| {
                    let mut sum = 0.0;
                    for a in 0..STEPS {
                        for b in 0..STEPS {
                            let size = roll_size(
                                fish,
                                (f64::from(a) + 0.5) / f64::from(STEPS),
                                (f64::from(b) + 0.5) / f64::from(STEPS),
                            );
                            sum += sale_value(fish, size) as f64;
                        }
                    }
                    (fish.id.clone(), sum / f64::from(STEPS * STEPS))
                })
                .collect()
        });
        worth[&fish.id]
    }

    /// Shells an hour at `spot` at this hour and month with `rod` and `bait` (plain bait for `None`):
    /// casting every [`CAST_COOLDOWN_SECS`], every reel caught, every fish sold, the bait paid for.
    fn shells_per_hour(spot: &Spot, hour: u32, month: u32, rod: &Rod, bait: Option<&Bait>) -> f64 {
        let cast: f64 = odds(content(), spot, hour, month, Tackle { luck: rod.luck, bait })
            .iter()
            .map(|(fish, chance)| chance * expected_sale(fish))
            .sum();
        let bait_cost = bait.map_or(BAIT_PRICE as f64, |b| b.price as f64 / b.pack as f64);
        (cast - bait_cost) * 3_600.0 / CAST_COOLDOWN_SECS as f64
    }

    /// The wage at every hour of every month, in [`cells`] order, for a spot, rod and bait.
    fn wages(spot: &Spot, rod: &Rod, bait: Option<&Bait>) -> Vec<f64> {
        type Key = (String, String, Option<String>);
        static TABLE: std::sync::Mutex<Option<std::collections::HashMap<Key, Vec<f64>>>> = std::sync::Mutex::new(None);
        let key = (spot.id.clone(), rod.id.clone(), bait.map(|b| b.id.clone()));
        if let Some(found) = TABLE.lock().expect("lock").get_or_insert_default().get(&key) {
            return found.clone();
        }
        let made: Vec<f64> = cells()
            .map(|(hour, month)| shells_per_hour(spot, hour, month, rod, bait))
            .collect();
        TABLE
            .lock()
            .expect("lock")
            .get_or_insert_default()
            .insert(key, made.clone());
        made
    }

    fn mean(values: &[f64]) -> f64 {
        values.iter().sum::<f64>() / values.len() as f64
    }

    fn lowest(values: &[f64]) -> f64 {
        values.iter().copied().fold(f64::MAX, f64::min)
    }

    fn highest(values: &[f64]) -> f64 {
        values.iter().copied().fold(f64::MIN, f64::max)
    }

    /// Every way to fit out a rod: each rod with plain bait and with each bait.
    fn all_tackle(gear: &Gear) -> Vec<(&Rod, Option<&Bait>)> {
        let baits = std::iter::once(None).chain(gear.baits.iter().map(Some));
        baits
            .flat_map(|bait| gear.rods.iter().map(move |rod| (rod, bait)))
            .collect()
    }

    /// The best mean wage a spot gives with any rod and bait.
    fn best_mean(gear: &Gear, spot: &Spot) -> f64 {
        all_tackle(gear)
            .into_iter()
            .map(|(rod, bait)| mean(&wages(spot, rod, bait)))
            .fold(f64::MIN, f64::max)
    }

    #[test]
    fn no_hour_of_any_month_pays_more_than_its_spot_ceiling_with_any_gear() {
        let gear = &content().fishing;
        for (id, ceiling) in CEILINGS {
            let spot = gear.spot(id).expect("spot");
            for (rod, bait) in all_tackle(gear) {
                for ((hour, month), pay) in cells().zip(wages(spot, rod, bait)) {
                    assert!(
                        pay <= ceiling,
                        "{id} pays {pay:.0} shells an hour at {hour}:00 in month {month} with {} and {:?}: the ceiling is {ceiling}",
                        rod.id,
                        bait.map(|b| &b.id)
                    );
                }
            }
        }
    }

    #[test]
    fn the_shore_with_the_starter_gear_pays_a_modest_wage() {
        let gear = &content().fishing;
        let pay = mean(&wages(gear.first_spot(), gear.starter_rod(), None));
        assert!((700.0..=900.0).contains(&pay), "{pay:.0}");
    }

    #[test]
    fn no_hour_is_dead_and_the_starter_never_loses_money() {
        let gear = &content().fishing;
        for spot in &gear.spots {
            for rod in &gear.rods {
                let pay = wages(spot, rod, None);
                let average = mean(&pay);
                for ((hour, month), cell) in cells().zip(&pay) {
                    assert!(
                        *cell >= 0.5 * average && *cell > 0.0,
                        "{} pays {cell:.0} at {hour}:00 in month {month} with {}: the mean is {average:.0}",
                        spot.id,
                        rod.id
                    );
                }
            }
            // The wage barely moves with the clock: the weakest hour is within a third of the best.
            let starter = wages(spot, gear.starter_rod(), None);
            assert!(
                lowest(&starter) > 0.6 * highest(&starter),
                "{}: {:.0} to {:.0} an hour",
                spot.id,
                lowest(&starter),
                highest(&starter)
            );
        }
    }

    #[test]
    fn deeper_spots_and_better_gear_each_pay_clearly_more() {
        let gear = &content().fishing;
        let starter = |spot: &Spot| wages(spot, gear.starter_rod(), None);
        for pair in gear.spots.windows(2) {
            let (up, down) = (&pair[0], &pair[1]);
            assert!(
                mean(&starter(down)) > 1.2 * mean(&starter(up)),
                "starter gear: {} vs {}",
                down.id,
                up.id
            );
            // At every hour, not only on average.
            assert!(
                lowest(&starter(down)) > highest(&starter(up)),
                "{}'s worst hour does not beat {}'s best",
                down.id,
                up.id
            );
            assert!(
                best_mean(gear, down) > 1.2 * best_mean(gear, up),
                "best gear: {} vs {}",
                down.id,
                up.id
            );
        }
        for spot in &gear.spots {
            assert!(
                best_mean(gear, spot) > 1.25 * mean(&starter(spot)),
                "{}: gear barely matters",
                spot.id
            );
        }
    }

    #[test]
    fn the_top_rod_takes_many_hours_in_the_abyss_to_pay_for_itself() {
        let gear = &content().fishing;
        let abyss = gear.spot("abyss").expect("abyss");
        let top = gear.rods.last().expect("a rod");
        for bait in std::iter::once(None).chain(gear.baits.iter().map(Some)) {
            let gain = mean(&wages(abyss, top, bait)) - mean(&wages(abyss, gear.starter_rod(), bait));
            assert!(gain > 0.0);
            let hours = top.price as f64 / gain;
            assert!(
                hours >= 15.0,
                "{} pays for itself in {hours:.1} h with {:?}",
                top.id,
                bait.map(|b| &b.id)
            );
        }
    }

    #[test]
    fn legendaries_stay_rare_at_every_hour_even_with_the_best_rod() {
        let (content, gear) = (content(), &content().fishing);
        let top = gear.rods.last().expect("a rod");
        for spot in &gear.spots {
            for bait in std::iter::once(None).chain(gear.baits.iter().map(Some)) {
                for (hour, month) in cells() {
                    let pool = odds(content, spot, hour, month, Tackle { luck: top.luck, bait });
                    let legendary = share_of(&pool, |f| Tier::of(f) == Tier::Legendary);
                    assert!(
                        legendary <= 0.003,
                        "{} at {hour}:00 in month {month}: {:.3}% legendary",
                        spot.id,
                        legendary * 100.0
                    );
                    if spot.id == "abyss" {
                        assert!(
                            legendary >= 0.001,
                            "abyss legendaries are out of reach at {hour}:00 in month {month}: {:.3}%",
                            legendary * 100.0
                        );
                    }
                }
            }
        }
    }

    // /Reel

    /// The share of casts a press at `press_ms` after the cast catches with a rod of `window_ms`, over
    /// the whole range of bite delays.
    fn blind_catch_share(press_ms: i64, window_ms: i64) -> f64 {
        const ROLLS: u32 = 3_000;
        let caught = (0..ROLLS)
            .filter(|n| {
                let bite = bite_delay_ms((f64::from(*n) + 0.5) / f64::from(ROLLS));
                reel_result(press_ms - bite, window_ms) == Reel::Caught
            })
            .count();
        caught as f64 / f64::from(ROLLS)
    }

    #[test]
    fn pressing_blind_at_any_fixed_moment_rarely_catches() {
        let gear = &content().fishing;
        for rod in &gear.rods {
            // Discord takes a moment each way, so even the starter rod leaves time to react.
            assert!(rod.window_ms >= 2_000, "{}: too short to react through Discord", rod.id);
            let best = (0..=BITE_MAX_MS + rod.window_ms)
                .step_by(50)
                .map(|press| blind_catch_share(press, rod.window_ms))
                .fold(0.0, f64::max);
            assert!(
                best <= 0.4,
                "{}: a fixed press catches {:.0}% of casts",
                rod.id,
                best * 100.0
            );
        }
        // The press that used to catch everything with a wide window: the last moment a bite could come.
        let top = gear.rods.last().expect("a rod");
        assert!(blind_catch_share(BITE_MAX_MS, top.window_ms) <= 0.4);
        assert!(
            blind_catch_share(0, top.window_ms) == 0.0,
            "before any bite is an early miss"
        );
    }

    #[test]
    fn answering_the_bite_always_catches_and_the_line_outlasts_it() {
        let gear = &content().fishing;
        for rod in &gear.rods {
            for n in 0..200 {
                let bite = bite_delay_ms(f64::from(n) / 200.0);
                assert!((BITE_MIN_MS..=BITE_MAX_MS).contains(&bite));
                // A second after the bite, well inside the shortest window.
                assert_eq!(reel_result(1_000, rod.window_ms), Reel::Caught);
                assert!(
                    (bite + rod.window_ms) as u64 <= LINE_TTL_SECS * 1_000,
                    "the line is gone before the {} window ends",
                    rod.id
                );
            }
            assert_eq!(reel_result(-1, rod.window_ms), Reel::Early);
            assert_eq!(reel_result(rod.window_ms, rod.window_ms), Reel::Caught);
            assert_eq!(reel_result(rod.window_ms + 1, rod.window_ms), Reel::Late);
        }
        assert_eq!(bite_delay_ms(0.0), BITE_MIN_MS);
        assert_eq!(bite_delay_ms(1.0), BITE_MAX_MS - 1);
        assert!(CAST_COOLDOWN_SECS * 1_000 > (BITE_MAX_MS + gear.rods.last().expect("a rod").window_ms) as u64);
    }

    #[test]
    fn gear_is_priced_and_named_consistently() {
        let gear = &content().fishing;
        assert_eq!(gear.rods.len(), 5);
        assert_eq!(gear.baits.len(), 3);
        for bait in &gear.baits {
            assert!(
                bait.pack > 0 && bait.price / bait.pack > BAIT_PRICE,
                "{} is cheaper than plain bait",
                bait.id
            );
        }
        let mut ids: Vec<&str> = gear
            .rods
            .iter()
            .map(|r| r.id.as_str())
            .chain(gear.baits.iter().map(|b| b.id.as_str()))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 8, "every piece of gear has its own id");
    }
}
