//! Checks the shipped content (content/*.json) against the rules the code relies on.

use std::collections::HashSet;

use pinchora_game::{
    Availability, BadgeKind, BadgeStat, Content, Currency, Layer, Rarity, TitleKind, house, market, pets,
};

#[test]
fn shipped_content_is_consistent() {
    let c = Content::get();
    assert!(
        c.furniture.len() >= 250,
        "expected the full catalog, got {}",
        c.furniture.len()
    );
    assert!(c.items().count() >= 1000, "need 1000+ playable items");

    let mut ids = HashSet::new();
    for f in &c.furniture {
        assert!(ids.insert(f.id.as_str()), "duplicate furniture id {}", f.id);
        assert!(!f.id.contains("__"), "{}: `__` is reserved for variants", f.id);
        let [w, d] = f.footprint;
        match f.layer {
            Layer::Surface => assert_eq!([w, d], [1, 1], "{}", f.id),
            Layer::Wall => assert!(
                (1..=4).contains(&w) && (1..=house::WALL_LEVELS).contains(&d),
                "{}",
                f.id
            ),
            _ => assert!((1..=4).contains(&w) && (1..=4).contains(&d), "{}", f.id),
        }
        if let Some(iw) = f.iso_width {
            assert!((8..=(w + d) * 32).contains(&iw), "{}: iso_width {iw}", f.id);
        }
        assert!(f.price > 0, "{}", f.id);
        if let Some(set) = &f.set {
            assert!(c.sets.iter().any(|s| &s.id == set), "{}: unknown set {set}", f.id);
        }
    }
    for item in c.items() {
        assert!(c.item(&item.id()).is_some(), "{} must resolve", item.id());
    }

    for room in &c.rooms {
        assert!(c.wallpaper(&room.default_wallpaper).is_some(), "room {}", room.id);
        assert!(c.floor(&room.default_floor).is_some(), "room {}", room.id);
    }
    for t in house::ROOM_ORDER {
        assert!(c.room_type(t).is_some(), "room type {t} used by house tiers");
    }

    assert_eq!(c.pets.iter().filter(|p| p.starter).count(), 3);
    assert!(c.pets.iter().filter(|p| p.starter).all(|p| p.rarity == Rarity::Common));
    let odds: f64 = pets::odds(c).iter().map(|(_, p)| p).sum();
    assert!((odds - 1.0).abs() < 1e-9);
    assert!(c.fish.len() >= 60);
    assert!(c.fish.iter().all(|f| f.min_cm <= f.max_cm && f.weight > 0));

    // Look ids end up in saved character codes, so they must stay plain words.
    assert!(
        c.looks.len() >= 24,
        "expected the full set of looks, got {}",
        c.looks.len()
    );
    let mut looks = HashSet::new();
    for l in &c.looks {
        assert!(looks.insert(l.id.as_str()), "duplicate look id {}", l.id);
        assert!(
            !l.id.is_empty() && l.id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
            "look id {:?}",
            l.id
        );
        assert!(!l.name.is_empty() && !l.sprite_brief.is_empty(), "{}", l.id);
    }
}

#[test]
fn backgrounds_and_badges_are_consistent() {
    let c = Content::get();

    // Background ids name atlas sprites (`bg_<id>`), so they must stay plain words.
    assert!(
        c.backgrounds.len() >= 36,
        "expected every background, got {}",
        c.backgrounds.len()
    );
    let mut ids = HashSet::new();
    for b in &c.backgrounds {
        assert!(ids.insert(b.id.as_str()), "duplicate background id {}", b.id);
        assert!(
            !b.id.is_empty()
                && b.id
                    .bytes()
                    .all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == b'_'),
            "background id {:?}",
            b.id
        );
        assert!(b.price > 0, "{}", b.id);
        assert!(
            b.accent_rgb().is_some(),
            "{}: accent {:?} is not #rrggbb",
            b.id,
            b.accent
        );
        assert!(!b.name.is_empty() && !b.scene_brief.is_empty(), "{}", b.id);
        assert_eq!(c.background(&b.id).map(|x| x.id.as_str()), Some(b.id.as_str()));
    }
    assert!(Rarity::ALL.iter().all(|r| c.backgrounds.iter().any(|b| b.rarity == *r)));

    // Badge ids name atlas sprites (`b_<id>`) and end up in saved lists.
    assert!(c.badges.len() >= 40, "expected every badge, got {}", c.badges.len());
    let mut ids = HashSet::new();
    let (mut achievements, mut shop) = (0, 0);
    for b in &c.badges {
        assert!(ids.insert(b.id.as_str()), "duplicate badge id {}", b.id);
        assert!(
            !b.id.is_empty()
                && b.id
                    .bytes()
                    .all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == b'_'),
            "badge id {:?}",
            b.id
        );
        assert!(!b.name.is_empty() && !b.description.is_empty(), "{}", b.id);
        match b.kind {
            BadgeKind::Achievement { stat, at } => {
                achievements += 1;
                assert!(at > 0, "{}: threshold {at}", b.id);
                if stat == BadgeStat::FishSpecies {
                    assert!(
                        at as usize <= c.fish.len(),
                        "{}: more species than the fish book has",
                        b.id
                    );
                }
            }
            BadgeKind::Shop { price } => {
                shop += 1;
                assert!(price > 0, "{}", b.id);
            }
        }
        assert_eq!(c.badge(&b.id).map(|x| x.id.as_str()), Some(b.id.as_str()));
    }
    assert!(achievements > 0 && shop > 0);
    for stat in BadgeStat::ALL {
        assert!(
            c.badges
                .iter()
                .any(|b| matches!(b.kind, BadgeKind::Achievement { stat: s, .. } if s == stat)),
            "no badge counts {stat:?}"
        );
    }
}

#[test]
fn badge_rarity_follows_difficulty() {
    let c = Content::get();
    assert!(
        Rarity::ALL.iter().all(|r| c.badges.iter().any(|b| b.rarity == *r)),
        "every rarity has a badge"
    );
    // Within one stat a higher threshold is never less rare.
    for stat in BadgeStat::ALL {
        let mut ladder: Vec<_> = c
            .badges
            .iter()
            .filter_map(|b| match b.kind {
                BadgeKind::Achievement { stat: s, at } if s == stat => Some((at, b.rarity)),
                _ => None,
            })
            .collect();
        ladder.sort();
        assert!(
            ladder.windows(2).all(|w| w[0].1 <= w[1].1),
            "{stat:?} gets less rare as it gets harder: {ladder:?}"
        );
    }
    // Shop badges: the dearer, the rarer.
    let mut shop: Vec<_> = c
        .badges
        .iter()
        .filter_map(|b| match b.kind {
            BadgeKind::Shop { price } => Some((price, b.rarity)),
            _ => None,
        })
        .collect();
    shop.sort();
    assert!(shop.windows(2).all(|w| w[0].1 <= w[1].1), "{shop:?}");
    assert_eq!(shop.first().map(|s| s.1), Some(Rarity::Common));
    assert_eq!(shop.last().map(|s| s.1), Some(Rarity::Legendary));
}

#[test]
fn titles_are_consistent() {
    let c = Content::get();
    assert!(c.titles.len() >= 30, "expected every title, got {}", c.titles.len());
    let (mut ids, mut names) = (HashSet::new(), HashSet::new());
    for t in &c.titles {
        assert!(ids.insert(t.id.as_str()), "duplicate title id {}", t.id);
        assert!(names.insert(t.name.to_lowercase()), "duplicate title name {}", t.name);
        assert!(
            !t.id.is_empty()
                && t.id
                    .bytes()
                    .all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == b'_'),
            "title id {:?}",
            t.id
        );
        assert!(
            !t.name.is_empty() && t.name.chars().count() <= 24 && !t.description.is_empty(),
            "{}",
            t.id
        );
        match &t.kind {
            TitleKind::Stat { stat, at } => {
                assert!(*at > 0, "{}: threshold {at}", t.id);
                if *stat == BadgeStat::FishSpecies {
                    assert!(
                        *at as usize <= c.fish.len(),
                        "{}: more species than the fish book has",
                        t.id
                    );
                }
            }
            TitleKind::Badge { badge } => assert!(c.badge(badge).is_some(), "{}: unknown badge {badge}", t.id),
            TitleKind::Special => {}
        }
        assert_eq!(c.title(&t.id).map(|x| x.id.as_str()), Some(t.id.as_str()));
    }
    for stat in BadgeStat::ALL {
        assert!(
            c.titles
                .iter()
                .any(|t| matches!(t.kind, TitleKind::Stat { stat: s, .. } if s == stat)),
            "no title counts {stat:?}"
        );
    }
    let special = c.title("house_of_the_week").expect("the contest title");
    assert_eq!(special.kind, TitleKind::Special);
    assert_eq!(
        c.titles.iter().filter(|t| t.kind == TitleKind::Special).count(),
        1,
        "nothing else is special yet"
    );
}

#[test]
fn badge_kinds_reject_impossible_combinations() {
    let parse = |json: &str| {
        Content::from_json("[]", "[]", r#"{"wallpapers":[],"floors":[]}"#, "[]", "[]", "[]", "[]")
            .and_then(|c| c.with_cosmetics("[]", json, "[]"))
    };
    let achievement = r#"{"id":"a","name":"A","description":"d","rarity":"common","kind":"achievement","stat":"level","at":5,"icon_brief":"i"}"#;
    assert!(parse(&format!("[{achievement}]")).is_ok());
    for bad in [
        r#"{"id":"a","name":"A","description":"d","rarity":"common","kind":"achievement","stat":"xp","at":5,"icon_brief":"i"}"#,
        r#"{"id":"a","name":"A","description":"d","rarity":"common","kind":"achievement","stat":"level","icon_brief":"i"}"#,
        r#"{"id":"a","name":"A","description":"d","rarity":"common","kind":"shop","icon_brief":"i"}"#,
        r#"{"id":"a","name":"A","description":"d","rarity":"common","kind":"gift","price":5,"icon_brief":"i"}"#,
        // A badge without a rarity, or with one that does not exist.
        r#"{"id":"a","name":"A","description":"d","kind":"achievement","stat":"level","at":5,"icon_brief":"i"}"#,
        r#"{"id":"a","name":"A","description":"d","rarity":"mythic","kind":"achievement","stat":"level","at":5,"icon_brief":"i"}"#,
    ] {
        assert!(parse(&format!("[{bad}]")).is_err(), "{bad}");
    }
}

#[test]
fn title_kinds_reject_impossible_combinations() {
    let parse = |json: &str| {
        Content::from_json("[]", "[]", r#"{"wallpapers":[],"floors":[]}"#, "[]", "[]", "[]", "[]")
            .and_then(|c| c.with_cosmetics("[]", "[]", json))
    };
    for good in [
        r#"{"id":"a","name":"A","description":"d","kind":"stat","stat":"level","at":5}"#,
        r#"{"id":"a","name":"A","description":"d","kind":"badge","badge":"little_beacon"}"#,
        r#"{"id":"a","name":"A","description":"d","kind":"special"}"#,
    ] {
        assert!(parse(&format!("[{good}]")).is_ok(), "{good}");
    }
    for bad in [
        r#"{"id":"a","name":"A","description":"d","kind":"stat","stat":"xp","at":5}"#,
        r#"{"id":"a","name":"A","description":"d","kind":"stat","stat":"level"}"#,
        r#"{"id":"a","name":"A","description":"d","kind":"badge"}"#,
        r#"{"id":"a","name":"A","description":"d","kind":"gift"}"#,
    ] {
        assert!(parse(&format!("[{bad}]")).is_err(), "{bad}");
    }
}

#[test]
fn markets_and_starter_kits_work_with_real_content() {
    let c = Content::get();
    let rotating = c
        .furniture
        .iter()
        .filter(|f| f.availability == Availability::Rotating && f.currency == Currency::Shells)
        .count();
    assert!(rotating >= market::OFFERS_PER_HOUR);
    for hour in [0, 1, 487_000, 487_001] {
        let offers = market::offers(c, hour);
        assert_eq!(offers.len(), market::OFFERS_PER_HOUR, "hour {hour}");
        let bases: HashSet<_> = offers.iter().map(|(id, _, _)| id.split("__").next().unwrap()).collect();
        assert_eq!(bases.len(), offers.len(), "distinct designs per hour");
        assert_eq!(offers, market::offers(c, hour), "same hour, same market on every pod");
    }
    assert_ne!(market::offers(c, 10), market::offers(c, 11));
}
