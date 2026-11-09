use super::*;

use std::collections::HashSet;

use crate::play::Game;

#[test]
fn shard_rates_match_game_design() {
    assert_eq!(
        [
            Rarity::Common,
            Rarity::Uncommon,
            Rarity::Rare,
            Rarity::Epic,
            Rarity::Legendary
        ]
        .map(recycle_shards),
        [2, 4, 9, 20, 45]
    );
    assert_eq!(GLIMMER_SHARE, 0.02);
}

#[test]
fn habitat_sets_discount_crafting_only_when_complete() {
    let mut owned = HashSet::new();
    assert_eq!(shard_craft_cost(&owned), 12);
    for entry in Collection::get()
        .critters
        .iter()
        .filter(|entry| entry.habitat == Some(Habitat::Kelp))
    {
        owned.insert(entry.id.clone());
    }
    let kelp = habitat_sets(&owned)
        .into_iter()
        .find(|set| set.habitat == Habitat::Kelp)
        .unwrap();
    assert!(kelp.complete());
    assert_eq!(shard_craft_cost(&owned), 11);
    for entry in &Collection::get().critters {
        owned.insert(entry.id.clone());
    }
    assert_eq!(shard_craft_cost(&owned), 7);
}

/// A fixed list of dice, repeated.
fn dice(rolls: &'static [f64]) -> impl FnMut() -> f64 {
    let mut n = 0;
    move || {
        let roll = rolls[n % rolls.len()];
        n += 1;
        roll
    }
}

/// A deterministic spread of dice, for odds checks.
fn spread() -> impl FnMut() -> f64 {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[test]
fn the_shipped_catalogue_is_well_formed() {
    let c = Collection::get();
    let content = Content::get();
    assert_eq!(c.critters.len(), 48);
    assert_eq!(c.finds.len(), 36);
    let mut ids = HashSet::new();
    for e in c.critters.iter().chain(&c.finds) {
        assert!(ids.insert(e.id.as_str()), "duplicate id {}", e.id);
        assert!(
            !e.id.is_empty() && e.id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{}",
            e.id
        );
        assert!(!e.name.is_empty() && e.name.len() <= 24, "{}: name", e.id);
        assert!(e.flavor.len() <= 72 && e.flavor.ends_with('.'), "{}: flavor", e.id);
        assert!(
            matches!(e.time.as_str(), "any" | "dawn" | "day" | "dusk" | "night"),
            "{}",
            e.id
        );
        assert!(
            matches!(e.season.as_str(), "any" | "spring" | "summer" | "autumn" | "winter"),
            "{}",
            e.id
        );
        // Every entry can be pictured: a borrowed sprite that exists, or a look the renderer draws.
        match e.art().unwrap_or_else(|| panic!("{} has no art", e.id)) {
            Art::Fish(id) => assert!(content.fish(id).is_some(), "{}: fish {id}", e.id),
            Art::Pet(id) => assert!(content.pet(id).is_some(), "{}: pet {id}", e.id),
            Art::Furniture(id) => assert!(content.furniture(id).is_some(), "{}: furniture {id}", e.id),
            Art::Drawn {
                shape,
                body,
                accent,
                pattern,
            } => {
                assert!(
                    pinchora_render::critters::draw(shape, body, accent, pattern).is_some(),
                    "{}: cannot draw {shape}/{body}/{accent}",
                    e.id
                );
                assert!(pinchora_render::critters::PATTERNS.contains(&pattern), "{}", e.id);
            }
        }
    }
    for e in &c.critters {
        assert!(
            e.habitat.is_some() && e.salvage == 0,
            "{}: a critter lives somewhere",
            e.id
        );
    }
    for e in &c.finds {
        assert!(e.habitat.is_none(), "{}: a find has no habitat", e.id);
    }
    // Every rarity has something in both halves, the common ones the most.
    for kind in [Kind::Critter, Kind::Find] {
        let count = |r: Rarity| c.list(kind).iter().filter(|e| e.rarity == r).count();
        for r in Rarity::ALL {
            assert!(count(r) >= 2, "{kind:?} {r:?}");
        }
        assert!(count(Rarity::Common) > count(Rarity::Uncommon) && count(Rarity::Uncommon) > count(Rarity::Rare));
    }
    for b in &c.bottles {
        assert!(b.text.len() <= 120 && !b.text.is_empty(), "{}", b.id);
    }
    assert!(c.bottles.len() >= 40);
    let texts: HashSet<_> = c.bottles.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts.len(), c.bottles.len(), "no message twice");
    // Each message has a Turkish text, written for Turkish readers and clean for a strict server.
    let mut turkish = HashSet::new();
    for b in &c.bottles {
        let tr = c.bottle_text(&b.id, Lang::Tr).expect("a Turkish text");
        assert_ne!(tr, b.text, "{}: not translated", b.id);
        assert!(tr.chars().count() <= 95 && turkish.insert(tr), "{}: {tr}", b.id);
        assert!(
            !crate::party::filter::is_profane_for(tr, true, Lang::Tr),
            "{}: {tr}",
            b.id
        );
        assert_eq!(c.bottle_text(&b.id, Lang::En), Some(b.text.as_str()));
    }
    assert_eq!(c.bottle_text("nope", Lang::Tr), None);
    assert_eq!(c.entry("pebblesnap_crab").map(|e| e.kind), Some(Kind::Critter));
    assert_eq!(c.entry("sand_dollar").map(|e| e.kind), Some(Kind::Find));
    assert!(c.entry("nonsense").is_none());
}

#[test]
fn rarer_things_pay_more_and_cost_more() {
    let rarities = Rarity::ALL;
    for pair in rarities.windows(2) {
        let (a, b) = (pay(pair[0]), pay(pair[1]));
        assert!(a.new_shells < b.new_shells && a.dup_shells < b.dup_shells && a.dup_fragments < b.dup_fragments);
        assert!(exchange_cost(pair[0]) < exchange_cost(pair[1]));
    }
    for r in rarities {
        let p = pay(r);
        assert!(p.dup_shells < p.new_shells, "a discovery pays more than a spare");
    }
    let total: f64 = rarities.iter().map(|r| odds(*r)).sum();
    assert!((total - 100.0).abs() < 1e-9);
    assert!(odds(Rarity::Common) > odds(Rarity::Legendary) * 30.0);
}

#[test]
fn a_draw_follows_the_table() {
    let pool: Vec<&Entry> = Collection::get().critters.iter().collect();
    // The lowest dice give a common critter, the highest a legendary one.
    let low = pick(&pool, 0.0, &mut dice(&[0.0, 0.0])).expect("a critter");
    assert_eq!(low.rarity, Rarity::Common);
    let high = pick(&pool, 0.0, &mut dice(&[0.999, 0.5])).expect("a critter");
    assert_eq!(high.rarity, Rarity::Legendary);
    // Many draws land close to the table, whatever the number of entries in a tier.
    let mut rng = spread();
    let mut seen = std::collections::BTreeMap::new();
    let n = 20_000;
    for _ in 0..n {
        *seen
            .entry(pick(&pool, 0.0, &mut rng).expect("a critter").rarity)
            .or_insert(0usize) += 1;
    }
    for r in Rarity::ALL {
        let share = seen[&r] as f64 * 100.0 / n as f64;
        assert!(
            (share - odds(r)).abs() < 1.5,
            "{r:?}: {share:.1}% against {:.1}%",
            odds(r)
        );
    }
    // Luck lifts the rare side only.
    let mut rng = spread();
    let lucky = (0..n)
        .filter(|_| pick(&pool, 1.0, &mut rng).expect("a critter").rarity >= Rarity::Rare)
        .count();
    assert!(lucky as f64 / n as f64 > 0.17, "{lucky}");
    assert!(pick(&[], 0.0, &mut spread()).is_none());
}

#[test]
fn a_drop_has_three_different_critters() {
    let mut rng = spread();
    for _ in 0..500 {
        let cards = drop_cards(12, 6, &mut rng);
        assert_eq!(cards.len(), DROP_CARDS);
        let ids: HashSet<_> = cards.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids.len(), DROP_CARDS);
        assert!(cards.iter().all(|c| c.kind == Kind::Critter));
    }
    // Even dice that keep asking for the same legendary critter end up with three cards.
    assert_eq!(drop_cards(12, 6, &mut dice(&[0.999, 0.0])).len(), DROP_CARDS);
}

#[test]
fn a_beachcomb_follows_the_clock_and_always_finds_something() {
    let mut rng = spread();
    for hour in 0..24 {
        for month in 1..=12 {
            for _ in 0..40 {
                let e = comb(hour, month, &mut rng).expect("something washes up");
                let (tod, season) = (fishing::time_of_day(hour), fishing::season(month));
                assert!(e.fits(tod, season), "{} at hour {hour} in month {month}", e.id);
                if e.kind == Kind::Critter {
                    assert!(matches!(e.habitat, Some(Habitat::Shore | Habitat::Tidepool)));
                }
            }
        }
    }
    // A critter roll below the share, then dice for a common one; the moon pearl needs the night.
    let critter = comb(12, 6, &mut dice(&[0.1, 0.0, 0.0])).expect("a critter");
    assert_eq!(critter.kind, Kind::Critter);
    let find = comb(12, 6, &mut dice(&[0.9, 0.999, 0.99])).expect("a find");
    assert_ne!(find.id, "moon_pearl", "no moon pearl at noon");
    let mut rng = spread();
    let pearl = (0..4_000).any(|_| comb(23, 3, &mut rng).is_some_and(|e| e.id == "moon_pearl"));
    assert!(pearl, "the moon pearl turns up at night");
}

#[test]
fn crab_pots_catch_more_with_level_and_longer_soaks() {
    for level in 1..POT_MAX_LEVEL {
        assert!(pot_rate(level) < pot_rate(level + 1));
        assert!(pot_upgrade_cost(level).is_some());
        assert!(pot_upgrade_cost(level) < pot_upgrade_cost(level + 1).or(Some(i64::MAX)));
    }
    assert_eq!(pot_upgrade_cost(POT_MAX_LEVEL), None);
    assert_eq!(pot_catches(1, 1), 1);
    assert_eq!(pot_catches(1, 4), 4);
    assert_eq!(pot_catches(1, 8), 10);
    assert_eq!(pot_catches(5, 8), 18);
    for level in 1..=POT_MAX_LEVEL {
        // Per hour, a longer soak is worth a little more.
        let per_hour = |h: i16| f64::from(pot_catches(level, h)) / f64::from(h);
        assert!(per_hour(8) >= per_hour(4) && per_hour(4) >= per_hour(1));
    }
    // The deep stays out of a pot's reach, however lucky it is.
    let mut rng = spread();
    for _ in 0..3_000 {
        let e = pot_catch(5, 23, 12, &mut rng).expect("a catch");
        assert_ne!(e.habitat, Some(Habitat::Deep), "{}", e.id);
    }
    // A better pot finds rare things more often.
    let rares = |level: i16| {
        let mut rng = spread();
        (0..6_000)
            .filter(|_| pot_catch(level, 12, 6, &mut rng).is_some_and(|e| e.rarity >= Rarity::Rare))
            .count()
    };
    assert!(rares(5) > rares(1), "{} against {}", rares(5), rares(1));
}

#[test]
fn a_bottle_holds_a_prize_by_the_dice() {
    assert_eq!(open_bottle(&mut dice(&[0.0, 0.0])), Prize::Shells(20));
    assert_eq!(open_bottle(&mut dice(&[0.61, 0.999])), Prize::Shells(60));
    assert_eq!(open_bottle(&mut dice(&[0.7, 0.0])), Prize::Fragments(4));
    assert!(matches!(open_bottle(&mut dice(&[0.9, 0.5, 0.5])), Prize::Find(_)));
    assert_eq!(open_bottle(&mut dice(&[0.99, 0.999])), Prize::Treasure(300));
    // On average a bottle pays well under a hundred shells.
    let mut rng = spread();
    let n = 10_000;
    let worth: i64 = (0..n)
        .map(|_| match open_bottle(&mut rng) {
            Prize::Shells(n) | Prize::Treasure(n) => n,
            Prize::Find(e) => e.pay().dup_shells,
            Prize::Fragments(_) => 0,
        })
        .sum();
    assert!((20..60).contains(&(worth / n)), "{}", worth / n);
}

#[test]
fn no_collection_game_comes_near_the_hourly_limit() {
    // What a spare pays on average: the table's odds against what each rarity pays.
    let spare: f64 = Rarity::ALL
        .iter()
        .map(|r| odds(*r) / 100.0 * pay(*r).dup_shells as f64)
        .sum();
    assert!((14.0..20.0).contains(&spare), "{spare}");
    // The most plays an hour each game can have, and what they pay on average.
    let beachcomb = 60.0 / COMB_COOLDOWN_MINUTES as f64 * spare;
    // A server gets one spawn per cooldown, and a player one hand-called drop per half hour.
    let spawns_per_hour = 3600.0 / SPAWN_COOLDOWN_SECS as f64;
    let critter_drops = (spawns_per_hour + 3600.0 / MANUAL_DROP_COOLDOWN_SECS as f64) * spare;
    let bottles = spawns_per_hour * 34.0;
    let pot = f64::from(pot_catches(POT_MAX_LEVEL, 8)) * spare * 1.2 / 8.0;
    for (game, per_hour) in [
        ("beachcomb", beachcomb),
        ("critter drop", critter_drops),
        ("bottle", bottles),
        ("crab pot", pot),
    ] {
        assert!(per_hour < 200.0, "{game} pays {per_hour:.0} shells an hour");
    }
    // All four at once, a player who never sleeps, still under a tenth of the 1,500 an hour the games may pay.
    assert!(beachcomb + critter_drops + bottles + pot < 150.0 * 4.0);
    // Only so many plays a day pay at all.
    for game in [Game::CritterDrop, Game::Bottle, Game::Beachcomb, Game::CrabPot] {
        assert!((3..=24).contains(&game.daily_cap()), "{game:?}");
    }
}

#[test]
fn fragments_buy_free_to_earn_decor_at_every_rarity() {
    let content = Content::get();
    for r in Rarity::ALL {
        let pool = exchange_pool(content, r);
        assert!(!pool.is_empty(), "{r:?} has nothing to buy");
        for f in &pool {
            assert_eq!(f.availability, Availability::Reward);
            assert_eq!(f.currency, Currency::Shells);
            assert_eq!(f.rarity, r);
        }
        for roll in [0.0, 0.3, 0.6, 0.999] {
            let id = exchange_pick(content, r, roll).expect("a piece");
            assert!(content.item(&id).is_some(), "{id} is not an item");
        }
    }
    // Different dice reach different pieces.
    let picks: HashSet<_> = (0..40)
        .filter_map(|n| exchange_pick(content, Rarity::Common, f64::from(n) / 40.0))
        .collect();
    assert!(picks.len() >= 3, "{picks:?}");
}
/// An hour and a month in which `e` can turn up.
fn when(e: &Entry) -> (u32, u32) {
    let hour = match e.time.as_str() {
        "dawn" => 5,
        "dusk" => 18,
        "night" => 23,
        _ => 12,
    };
    let month = match e.season.as_str() {
        "spring" => 4,
        "summer" => 7,
        "autumn" => 10,
        "winter" => 1,
        _ => 6,
    };
    (hour, month)
}

#[test]
fn what_is_tied_to_a_time_or_season_turns_up_in_drops_and_pots_only_then() {
    let c = Collection::get();
    let tied: Vec<&Entry> = c
        .critters
        .iter()
        .filter(|e| e.time != "any" || e.season != "any")
        .collect();
    assert!(tied.len() >= 8, "{} critters are tied to a time", tied.len());
    // Away from their time none of them is ever dealt, in a drop or a pot, whatever the dice say.
    let mut rng = spread();
    for hour in 0..24 {
        for month in 1..=12 {
            let (tod, season) = (fishing::time_of_day(hour), fishing::season(month));
            for _ in 0..30 {
                for card in drop_cards(hour, month, &mut rng) {
                    assert!(card.fits(tod, season), "{} at hour {hour} in month {month}", card.id);
                }
                let e = pot_catch(3, hour, month, &mut rng).expect("a catch");
                assert!(e.fits(tod, season), "{} at hour {hour} in month {month}", e.id);
            }
        }
    }
    // At their time each can be dealt in a drop (the deep ones only come that way), and the ones a
    // pot reaches in a pot.
    for e in &tied {
        let (hour, month) = when(e);
        assert!(e.fits(fishing::time_of_day(hour), fishing::season(month)), "{}", e.id);
        let mut rng = spread();
        let dealt = (0..60_000).any(|_| drop_cards(hour, month, &mut rng).iter().any(|c| c.id == e.id));
        assert!(
            dealt,
            "{} never washes up in a drop at hour {hour} in month {month}",
            e.id
        );
        if e.habitat != Some(Habitat::Deep) {
            let caught = (0..60_000).any(|_| pot_catch(5, hour, month, &mut rng).is_some_and(|c| c.id == e.id));
            assert!(
                caught,
                "{} is never caught in a pot at hour {hour} in month {month}",
                e.id
            );
        }
    }
    // The moon pearl is a night find: a pot brings it up at night and never at noon.
    let mut rng = spread();
    assert!((0..30_000).any(|_| pot_catch(5, 23, 6, &mut rng).is_some_and(|e| e.id == "moon_pearl")));
    assert!(!(0..30_000).any(|_| pot_catch(5, 12, 6, &mut rng).is_some_and(|e| e.id == "moon_pearl")));
}

#[test]
fn fragment_prices_follow_the_catalogue() {
    let content = Content::get();
    // A spare pays this many shells and fragments on average, so a fragment is worth about this many shells.
    let (mut shells, mut fragments) = (0.0, 0.0);
    for r in Rarity::ALL {
        shells += odds(r) / 100.0 * pay(r).dup_shells as f64;
        fragments += odds(r) / 100.0 * pay(r).dup_fragments as f64;
    }
    assert!(
        (16.6..16.8).contains(&shells) && (2.3..2.5).contains(&fragments),
        "{shells} {fragments}"
    );
    assert!(
        (shells / fragments - FRAGMENT_SHELLS).abs() < 0.2,
        "a fragment is worth {:.2} shells beside a spare's shells",
        shells / fragments
    );
    // And a piece costs what its tier is worth in catalogue price at that rate, give or take a tenth.
    for r in Rarity::ALL {
        let pool = exchange_pool(content, r);
        let mean = pool.iter().map(|f| f.price as f64).sum::<f64>() / pool.len() as f64;
        let worth = mean / FRAGMENT_SHELLS / exchange_cost(r) as f64;
        assert!(
            (0.9..1.1).contains(&worth),
            "{r:?}: pieces average {mean:.0} shells, {} fragments is {:.0}% of that at {FRAGMENT_SHELLS} a fragment",
            exchange_cost(r),
            worth * 100.0
        );
    }
    // Common pieces are within a day's play, the legendary ones are a long goal.
    assert!(exchange_cost(Rarity::Common) < 20 && exchange_cost(Rarity::Legendary) > 5_000);
}
