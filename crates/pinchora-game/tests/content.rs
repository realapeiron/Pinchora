//! Checks the shipped content (content/*.json) against the rules the code relies on.

use std::collections::HashSet;

use pinchora_game::{Availability, Content, Currency, Layer, Rarity, house, market, pets};

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
            Layer::Wall => assert!((1..=3).contains(&w) && (1..=house::WALL_ROWS).contains(&d), "{}", f.id),
            _ => assert!((1..=4).contains(&w) && (1..=3).contains(&d), "{}", f.id),
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
