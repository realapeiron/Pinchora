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
    /// The spot's signature fish, the ones Tide Crumbs call.
    pub specials: Vec<String>,
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

/// The fish that can bite now at `spot`, each with its draw weight after rod and bait.
pub fn odds<'a>(content: &'a Content, spot: &Spot, hour: u32, month: u32, tackle: Tackle<'_>) -> Vec<(&'a Fish, f64)> {
    content
        .fish
        .iter()
        .filter(|f| f.bites_at(hour, month))
        .filter_map(|f| {
            let base = f64::from(*f.spots.get(&spot.id)?);
            let mut weight = base;
            if f.rarity >= Rarity::Rare {
                weight *= tackle.luck;
            }
            match tackle.bait.map(|b| b.boost) {
                Some(Boost::Rarity { rarity, factor }) if f.rarity == rarity => weight *= factor,
                Some(Boost::Specials { factor }) if spot.specials.contains(&f.id) => weight *= factor,
                _ => {}
            }
            (weight > 0.0).then_some((f, weight))
        })
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

    #[test]
    fn something_always_bites_at_every_spot() {
        for spot in &content().fishing.spots {
            for hour in 0..24 {
                for month in 1..=12 {
                    let pool = odds(content(), spot, hour, month, Tackle::PLAIN);
                    assert!(!pool.is_empty(), "{} is empty at hour {hour}, month {month}", spot.id);
                    assert!(pool.iter().map(|(_, w)| w).sum::<f64>() > 0.0);
                }
            }
        }
    }

    #[test]
    fn weights_follow_rarity_within_a_spot() {
        let content = content();
        for spot in &content.fishing.spots {
            for (commoner, rarer) in Rarity::ALL.iter().zip(&Rarity::ALL[1..]) {
                let weights = |rarity: Rarity| {
                    content
                        .fish
                        .iter()
                        .filter(move |f| f.rarity == rarity)
                        .filter_map(|f| f.spots.get(&spot.id).copied())
                        .collect::<Vec<_>>()
                };
                let (common, rare) = (weights(*commoner), weights(*rarer));
                if let (Some(low), Some(high)) = (common.iter().min(), rare.iter().max()) {
                    assert!(high < low, "{}: {rarer:?} fish outweigh {commoner:?} ones", spot.id);
                }
            }
        }
    }

    #[test]
    fn odds_sum_to_one_once_normalised() {
        let spot = content().fishing.spot("reef").expect("reef");
        let pool = odds(content(), spot, 12, 6, Tackle::PLAIN);
        let total: f64 = pool.iter().map(|(_, w)| w).sum();
        let shares: f64 = pool.iter().map(|(_, w)| w / total).sum();
        assert!((shares - 1.0).abs() < 1e-9);
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
        let plain = odds(content, reef, 12, 7, Tackle::PLAIN);
        let with = |bait: &str| {
            odds(
                content,
                reef,
                12,
                7,
                Tackle {
                    luck: 1.0,
                    bait: gear.bait(bait),
                },
            )
        };
        let uncommon = |pool: &[(&Fish, f64)]| share_of(pool, |f| f.rarity == Rarity::Uncommon);
        let rare = |pool: &[(&Fish, f64)]| share_of(pool, |f| f.rarity == Rarity::Rare);
        let special = |pool: &[(&Fish, f64)]| share_of(pool, |f| reef.specials.contains(&f.id));
        assert!(uncommon(&with("glow_worm")) > 2.0 * uncommon(&plain));
        assert!(rare(&with("silver_lure")) > 1.8 * rare(&plain));
        assert!(special(&with("tide_crumbs")) > 2.5 * special(&plain));
        // A bait leaves the other tiers' fish where they were: nothing drops out of the table.
        assert_eq!(with("glow_worm").len(), plain.len());
        // A special of one spot means nothing at another.
        let shore = gear.spot("shore").expect("shore");
        let at_shore = odds(
            content,
            shore,
            12,
            7,
            Tackle {
                luck: 1.0,
                bait: gear.bait("tide_crumbs"),
            },
        );
        assert!(
            at_shore
                .iter()
                .all(|(f, w)| reef.specials.contains(&f.id).not_or(*w == f64::from(f.spots["shore"])))
        );
    }

    trait NotOr {
        fn not_or(self, other: bool) -> bool;
    }
    impl NotOr for bool {
        fn not_or(self, other: bool) -> bool {
            !self || other
        }
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

    /// The most a spot may pay an hour of constant casting with every reel caught, with any rod and bait.
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

    /// What one cast at `spot` brings in on average over every hour of the day and month of the year,
    /// before the bait is paid, and the share of casts that bring in a fish of each rarity.
    fn average_cast(spot: &Spot, tackle: Tackle<'_>) -> (f64, [f64; 5]) {
        let (mut shells, mut shares, mut cells) = (0.0, [0.0; 5], 0.0);
        for hour in 0..24 {
            for month in 1..=12 {
                let pool = odds(content(), spot, hour, month, tackle);
                let total: f64 = pool.iter().map(|(_, w)| w).sum();
                for (fish, weight) in &pool {
                    shells += weight / total * expected_sale(fish);
                    shares[fish.rarity as usize] += weight / total;
                }
                cells += 1.0;
            }
        }
        (shells / cells, shares.map(|share| share / cells))
    }

    /// Shells an hour at `spot` with `rod` and `bait` (plain bait for `None`): casting every
    /// [`CAST_COOLDOWN_SECS`], every reel caught, every fish sold, the bait paid for.
    fn shells_per_hour(spot: &Spot, rod: &Rod, bait: Option<&Bait>) -> f64 {
        let (shells, _) = average_cast(spot, Tackle { luck: rod.luck, bait });
        let bait_cost = bait.map_or(BAIT_PRICE as f64, |b| b.price as f64 / b.pack as f64);
        (shells - bait_cost) * 3_600.0 / CAST_COOLDOWN_SECS as f64
    }

    /// Every way to fit out a rod: each rod with plain bait and with each bait.
    fn all_tackle(gear: &Gear) -> Vec<(&Rod, Option<&Bait>)> {
        let baits = std::iter::once(None).chain(gear.baits.iter().map(Some));
        baits
            .flat_map(|bait| gear.rods.iter().map(move |rod| (rod, bait)))
            .collect()
    }

    #[test]
    fn no_spot_pays_more_than_its_ceiling_with_any_gear() {
        let gear = &content().fishing;
        for (id, ceiling) in CEILINGS {
            let spot = gear.spot(id).expect("spot");
            for (rod, bait) in all_tackle(gear) {
                let pay = shells_per_hour(spot, rod, bait);
                assert!(
                    pay <= ceiling,
                    "{id} pays {pay:.0} shells an hour with {} and {:?}: the ceiling is {ceiling}",
                    rod.id,
                    bait.map(|b| &b.id)
                );
            }
        }
    }

    #[test]
    fn the_shore_with_the_starter_gear_pays_a_modest_wage() {
        let gear = &content().fishing;
        let pay = shells_per_hour(gear.first_spot(), gear.starter_rod(), None);
        assert!((700.0..=900.0).contains(&pay), "{pay:.0}");
    }

    #[test]
    fn deeper_spots_and_better_gear_each_pay_clearly_more() {
        let gear = &content().fishing;
        let starter = |spot: &Spot| shells_per_hour(spot, gear.starter_rod(), None);
        let best = |spot: &Spot| {
            all_tackle(gear)
                .into_iter()
                .map(|(rod, bait)| shells_per_hour(spot, rod, bait))
                .fold(f64::MIN, f64::max)
        };
        for pair in gear.spots.windows(2) {
            let (up, down) = (&pair[0], &pair[1]);
            assert!(
                starter(down) > 1.2 * starter(up),
                "starter gear: {} vs {}",
                down.id,
                up.id
            );
            assert!(best(down) > 1.2 * best(up), "best gear: {} vs {}", down.id, up.id);
        }
        for spot in &gear.spots {
            assert!(best(spot) > 1.25 * starter(spot), "{}: gear barely matters", spot.id);
        }
    }

    #[test]
    fn the_top_rod_takes_many_hours_in_the_abyss_to_pay_for_itself() {
        let gear = &content().fishing;
        let abyss = gear.spot("abyss").expect("abyss");
        let top = gear.rods.last().expect("a rod");
        for bait in std::iter::once(None).chain(gear.baits.iter().map(Some)) {
            let gain = shells_per_hour(abyss, top, bait) - shells_per_hour(abyss, gear.starter_rod(), bait);
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
    fn legendaries_stay_rare_even_with_the_best_rod() {
        let gear = &content().fishing;
        let top = gear.rods.last().expect("a rod");
        for spot in &gear.spots {
            for bait in std::iter::once(None).chain(gear.baits.iter().map(Some)) {
                let (_, shares) = average_cast(spot, Tackle { luck: top.luck, bait });
                let legendary = shares[Rarity::Legendary as usize];
                assert!(legendary <= 0.003, "{}: {:.3}% legendary", spot.id, legendary * 100.0);
                if spot.id == "abyss" {
                    assert!(
                        legendary >= 0.001,
                        "abyss legendaries are out of reach: {:.3}%",
                        legendary * 100.0
                    );
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
