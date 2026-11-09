use super::*;

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
