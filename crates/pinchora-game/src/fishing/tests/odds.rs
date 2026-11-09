use super::*;

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
