use crate::db::*;

#[tokio::test]
async fn place_and_pick_up_round_trip() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [user] = ids();
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    let (_, rooms, created) = db.house(user, &starter).await.unwrap();
    assert!(created && rooms.len() == 1);
    assert!(!db.house(user, &starter).await.unwrap().2, "second call finds it");

    db.grant_item(user, "chair", 1).await.unwrap();
    let placed = db.place_item(user, 0, "chair", 2, 3, false, |_| Ok(())).await.unwrap();
    let pinchora_db::PlaceOutcome::Placed { id } = placed else {
        panic!("should place")
    };
    assert!(
        !db.owns(user, "chair").await.unwrap(),
        "placed items leave the inventory"
    );
    assert!(matches!(
        db.place_item(user, 0, "chair", 4, 3, false, |_| Ok(())).await.unwrap(),
        pinchora_db::PlaceOutcome::NotOwned
    ));
    assert!(matches!(
        db.place_item(user, 0, "chair", 4, 3, false, |_| Err("blocked".into()))
            .await
            .unwrap(),
        pinchora_db::PlaceOutcome::Blocked(_)
    ));
    assert_eq!(db.pick_up(user, id).await.unwrap().as_deref(), Some("chair"));
    assert!(db.owns(user, "chair").await.unwrap());
    assert!(db.pick_up(user, id).await.unwrap().is_none());
}

#[tokio::test]
async fn a_trip_pays_once_and_freezes_the_pets_needs() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [owner, other] = ids();
    let pet = db.adopt(owner, "crumbclaw_crab", "Pip").await.unwrap();
    assert!(!pet.is_away(chrono::Utc::now()) && !pet.has_souvenir(chrono::Utc::now()));
    let now = pinchora_db::timestamp(chrono::Utc::now());
    let back = now + chrono::Duration::hours(4);

    // Only the owner can send it, and only once at a time.
    assert!(
        !db.start_trip(other, &pet, "tidepool_meadow", [10, 20, 30, 40], back)
            .await
            .unwrap()
    );
    assert!(
        db.start_trip(owner, &pet, "tidepool_meadow", [10, 20, 30, 40], back)
            .await
            .unwrap()
    );
    assert!(
        !db.start_trip(owner, &pet, "driftwood_cove", [1, 1, 1, 1], back)
            .await
            .unwrap()
    );
    let away = db.pets(owner).await.unwrap().remove(0);
    assert!(away.is_away(now) && !away.has_souvenir(now));
    assert_eq!(away.trip_dest.as_deref(), Some("tidepool_meadow"));
    assert_eq!(
        (away.hunger, away.fun, away.energy, away.hygiene, away.needs_at),
        (10, 20, 30, 40, back),
        "needs are frozen until the pet is back"
    );

    // Too early: nothing is paid, the trip stays.
    let early = db
        .claim_trip(owner, pet.id, now, |_, _| panic!("nothing to pay yet"))
        .await
        .unwrap();
    assert!(matches!(early, TripClaim::Away { ref trip, ends_at } if trip == "tidepool_meadow" && ends_at == back));
    // Somebody else's pet is nothing to claim.
    let theirs = db
        .claim_trip(other, pet.id, back, |_, _| panic!("not theirs"))
        .await
        .unwrap();
    assert!(matches!(theirs, TripClaim::Nothing));

    // Two claims race at the end of the trip; the souvenir is paid once.
    let after = back + chrono::Duration::minutes(1);
    assert!(db.pets(owner).await.unwrap()[0].has_souvenir(after));
    let pay = |_: &pinchora_db::PetRow, trip: &str| {
        assert_eq!(trip, "tidepool_meadow");
        TripPayout {
            shells: 120,
            item: Some("chair".to_owned()),
            level: 3,
            xp: 17,
        }
    };
    let (one, two) = tokio::join!(
        db.claim_trip(owner, pet.id, after, pay),
        db.claim_trip(owner, pet.id, after, pay),
    );
    let results = [one.unwrap(), two.unwrap()];
    let paid = results
        .iter()
        .filter(|r| matches!(r, TripClaim::Claimed { .. }))
        .count();
    assert_eq!(paid, 1, "{results:?}");
    assert_eq!(db.user(owner).await.unwrap().credits, 120);
    assert!(db.owns(owner, "chair").await.unwrap());
    let home = db.pets(owner).await.unwrap().remove(0);
    assert_eq!((home.level, home.xp, home.trip_dest.clone()), (3, 17, None));
    assert!(!home.is_away(after) && !home.has_souvenir(after));
    let again = db.claim_trip(owner, pet.id, after, pay).await.unwrap();
    assert!(matches!(again, TripClaim::Nothing));
    assert_eq!(
        db.user(owner).await.unwrap().credits,
        120,
        "a repeat claim pays nothing"
    );

    // Home again, it can go out again.
    assert!(
        db.start_trip(owner, &home, "driftwood_cove", [5, 5, 5, 5], after)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn care_cannot_write_over_a_trip() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [owner, other] = ids();
    let pet = db.adopt(owner, "crumbclaw_crab", "Pip").await.unwrap();
    let now = pinchora_db::timestamp(chrono::Utc::now());
    let back = now + chrono::Duration::hours(4);
    let frozen = [10, 20, 30, 40];

    // Home, a care write goes through, and only for the owner.
    assert!(!db.save_pet(other, &pet, [1, 1, 1, 1], 9, 9, now).await.unwrap());
    assert!(db.save_pet(owner, &pet, [90, 90, 90, 90], 2, 20, now).await.unwrap());
    // A click that read the pet before that write is stale now: refused, nothing written.
    assert!(!db.save_pet(owner, &pet, [5, 5, 5, 5], 3, 99, now).await.unwrap());
    let read = read_pet(&db, owner, pet.id).await;
    assert_eq!((read.hunger, read.level, read.xp), (90, 2, 20));
    assert!(
        db.start_trip(owner, &read, "tidepool_meadow", frozen, back)
            .await
            .unwrap()
    );

    // A care click that read the pet before it left: refused, and nothing is unfrozen.
    assert!(
        !db.save_pet(owner, &read, [100, 100, 100, 100], 3, 5, now)
            .await
            .unwrap()
    );
    let row = read_pet(&db, owner, pet.id).await;
    assert_eq!(
        (row.hunger, row.fun, row.energy, row.hygiene, row.needs_at),
        (10, 20, 30, 40, back),
        "needs stay frozen until it is back"
    );
    assert_eq!((row.level, row.xp), (2, 20));
    assert_eq!(row.trip_dest.as_deref(), Some("tidepool_meadow"));
    // Even a click that read the pet mid-trip cannot write until the trip is over.
    assert!(!db.save_pet(owner, &row, [100, 100, 100, 100], 3, 5, now).await.unwrap());

    // Back from the trip, the souvenir not yet claimed: it can be looked after.
    let after = back + chrono::Duration::minutes(1);
    assert!(db.save_pet(owner, &row, [60, 60, 60, 60], 2, 25, after).await.unwrap());
    // The claim pays from the row as it is, so the XP the care earned is not lost either.
    let pay = |pet: &pinchora_db::PetRow, _: &str| {
        assert_eq!((pet.level, pet.xp), (2, 25));
        TripPayout {
            shells: 50,
            item: None,
            level: 2,
            xp: 25 + 100,
        }
    };
    assert!(matches!(
        db.claim_trip(owner, pet.id, after, pay).await.unwrap(),
        TripClaim::Claimed { .. }
    ));
    assert_eq!(read_pet(&db, owner, pet.id).await.xp, 125);
}

#[tokio::test]
async fn a_care_write_that_read_the_pet_before_a_claim_cannot_undo_the_trip_xp() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [owner] = ids();
    let now = chrono::Utc::now();
    let after = now + chrono::Duration::hours(2);
    let pet = db.adopt(owner, "crumbclaw_crab", "Pip").await.unwrap();
    assert!(
        db.start_trip(owner, &pet, "tidepool_meadow", [50, 50, 50, 50], now)
            .await
            .unwrap()
    );
    // Back with the souvenir waiting: a care click reads the pet here ...
    let seen = read_pet(&db, owner, pet.id).await;
    assert!(seen.has_souvenir(after));
    // ... the claim pays 100 XP in between ...
    let pay = |pet: &pinchora_db::PetRow, _: &str| TripPayout {
        shells: 10,
        item: None,
        level: pet.level,
        xp: pet.xp + 100,
    };
    assert!(matches!(
        db.claim_trip(owner, pet.id, after, pay).await.unwrap(),
        TripClaim::Claimed { .. }
    ));
    // ... and the click's write, worked out from the old XP, is refused instead of undoing it.
    assert!(!db.save_pet(owner, &seen, [90, 90, 90, 90], 1, 5, after).await.unwrap());
    let row = read_pet(&db, owner, pet.id).await;
    assert_eq!((row.level, row.xp, row.trip_dest.clone()), (1, 100, None));
    // A click that reads the pet afresh goes through on top of the paid XP.
    assert!(db.save_pet(owner, &row, [90, 90, 90, 90], 1, 105, after).await.unwrap());
}

#[tokio::test]
async fn a_care_click_racing_a_claim_or_a_trip_never_loses_the_trips_xp_or_needs() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [owner] = ids();
    let now = pinchora_db::timestamp(chrono::Utc::now());
    let back = now + chrono::Duration::hours(2);
    let after = back + chrono::Duration::minutes(1);

    // A care click and the start of a trip, both worked out from the same read: one lands and the other is
    // refused, so the frozen needs never overwrite a care and a care never unfreezes a trip.
    let frozen = [11, 22, 33, 44];
    for round in 0..20 {
        let pet = db.adopt(owner, &format!("racer_{round}"), "Pip").await.unwrap();
        let (care, trip) = tokio::join!(
            db.save_pet(owner, &pet, [100, 100, 100, 100], 2, 10, now),
            db.start_trip(owner, &pet, "tidepool_meadow", frozen, back),
        );
        let (cared, left) = (care.unwrap(), trip.unwrap());
        assert!(
            cared != left,
            "round {round}: exactly one of them landed (care {cared}, trip {left})"
        );
        let row = read_pet(&db, owner, pet.id).await;
        if left {
            assert_eq!(
                (row.hunger, row.fun, row.energy, row.hygiene, row.needs_at),
                (11, 22, 33, 44, back),
                "round {round}"
            );
            assert_eq!(row.trip_dest.as_deref(), Some("tidepool_meadow"), "round {round}");
        } else {
            // The care landed first, so the trip, worked out from the old needs, was refused: the care is
            // not overwritten by the frozen needs.
            assert_eq!(
                (row.hunger, row.fun, row.energy, row.hygiene),
                (100, 100, 100, 100),
                "round {round}"
            );
            assert_eq!(row.trip_dest, None, "round {round}");
        }
    }

    // A care click and the claim of a finished trip: the 100 trip XP is always in the end result,
    // with the care's 5 on top only when the care landed first.
    let pay = |pet: &pinchora_db::PetRow, _: &str| TripPayout {
        shells: 10,
        item: None,
        level: pet.level,
        xp: pet.xp + 100,
    };
    for round in 0..20 {
        let pet = db.adopt(owner, &format!("homecomer_{round}"), "Pip").await.unwrap();
        assert!(
            db.start_trip(owner, &pet, "tidepool_meadow", [50, 50, 50, 50], back)
                .await
                .unwrap()
        );
        let seen = read_pet(&db, owner, pet.id).await;
        let (care, claim) = tokio::join!(
            db.save_pet(owner, &seen, [90, 90, 90, 90], 1, 5, after),
            db.claim_trip(owner, pet.id, after, pay),
        );
        let cared = care.unwrap();
        assert!(matches!(claim.unwrap(), TripClaim::Claimed { .. }), "round {round}");
        let row = read_pet(&db, owner, pet.id).await;
        assert_eq!(row.xp, if cared { 105 } else { 100 }, "round {round}: cared = {cared}");
        assert_eq!(row.trip_dest, None, "round {round}");
    }
}

#[tokio::test]
async fn waking_a_pet_is_free_once_a_day_and_costs_a_fee_after_that() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [owner] = ids();
    let (pet, second) = (
        db.adopt(owner, "crumbclaw_crab", "Pip").await.unwrap(),
        db.adopt(owner, "ribbonfin_guppy", "Bub").await.unwrap(),
    );
    let long_ago = pinchora_db::timestamp(chrono::Utc::now()) - chrono::Duration::days(5);
    for p in [&pet, &second] {
        db.save_pet(owner, p, [0, 0, 100, 0], 1, 0, long_ago).await.unwrap();
    }
    let now = pinchora_db::timestamp(chrono::Utc::now());
    assert!(db.free_wake_ready(owner, 20_000).await.unwrap());

    // A stale read changes nothing and charges nothing.
    let stale = db
        .wake_pet(owner, pet.id, now, [50, 50, 100, 50], now, 20_000, 75)
        .await
        .unwrap();
    assert_eq!(stale, Wake::Stale);
    assert!(db.free_wake_ready(owner, 20_000).await.unwrap());

    // The day's first wake-up is free.
    let first = db
        .wake_pet(owner, pet.id, long_ago, [50, 50, 100, 50], now, 20_000, 75)
        .await
        .unwrap();
    assert_eq!(first, Wake::Woke { free: true, balance: 0 });
    assert!(!db.free_wake_ready(owner, 20_000).await.unwrap());
    assert!(db.free_wake_ready(owner, 20_001).await.unwrap());
    let woke = db
        .pets(owner)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.id == pet.id)
        .unwrap();
    assert_eq!(
        (woke.hunger, woke.fun, woke.energy, woke.hygiene, woke.needs_at),
        (50, 50, 100, 50, now)
    );
    // Waking it twice is stale the second time.
    let twice = db
        .wake_pet(owner, pet.id, long_ago, [50, 50, 100, 50], now, 20_001, 75)
        .await
        .unwrap();
    assert_eq!(twice, Wake::Stale);

    // The second pet the same day costs the fee, which a broke player cannot pay.
    let broke = db
        .wake_pet(owner, second.id, long_ago, [50, 50, 100, 50], now, 20_000, 75)
        .await
        .unwrap();
    assert_eq!(broke, Wake::Poor { balance: 0 });
    let still = db
        .pets(owner)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.id == second.id)
        .unwrap();
    assert_eq!(still.needs_at, long_ago, "a refused wake-up changes nothing");
    db.earn_shells(owner, 100, "test").await.unwrap();
    let paid = db
        .wake_pet(owner, second.id, long_ago, [50, 50, 100, 50], now, 20_000, 75)
        .await
        .unwrap();
    assert_eq!(
        paid,
        Wake::Woke {
            free: false,
            balance: 25
        }
    );
    assert_eq!(db.user(owner).await.unwrap().credits, 25);
}

#[tokio::test]
async fn pet_timestamp_guards_accept_nanoseconds_but_reject_a_changed_microsecond() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [owner] = ids();
    let pet = db.adopt(owner, "crumbclaw_crab", "Pip").await.unwrap();
    // Explicit nanoseconds reproduce the Linux clock on macOS too. Keep the raw
    // value as the next guard, just as a caller retaining its last write would.
    let raw = chrono::DateTime::parse_from_rfc3339("2026-11-08T10:00:00.958061832Z")
        .unwrap()
        .to_utc();
    let stored = chrono::DateTime::parse_from_rfc3339("2026-11-08T10:00:00.958061Z")
        .unwrap()
        .to_utc();
    assert_ne!(raw, stored);
    assert!(db.save_pet(owner, &pet, [0, 0, 100, 0], 1, 0, raw).await.unwrap());
    assert_eq!(db.active_pet(owner).await.unwrap().unwrap().needs_at, stored);
    let now = raw + chrono::Duration::days(5);
    assert_eq!(
        db.wake_pet(
            owner,
            pet.id,
            raw + chrono::Duration::microseconds(1),
            [50, 50, 100, 50],
            now,
            20_000,
            75
        )
        .await
        .unwrap(),
        Wake::Stale
    );
    assert!(db.free_wake_ready(owner, 20_000).await.unwrap());
    assert_eq!(
        db.wake_pet(owner, pet.id, raw, [50, 50, 100, 50], now, 20_000, 75)
            .await
            .unwrap(),
        Wake::Woke { free: true, balance: 0 }
    );
    assert_eq!(
        db.active_pet(owner).await.unwrap().unwrap().needs_at,
        stored + chrono::Duration::days(5)
    );
    assert_eq!(
        db.wake_pet(owner, pet.id, raw, [50, 50, 100, 50], now, 20_001, 75)
            .await
            .unwrap(),
        Wake::Stale
    );
    assert!(db.free_wake_ready(owner, 20_001).await.unwrap());
    assert_eq!(db.user(owner).await.unwrap().credits, 0);

    // The SQL guards must retain the same precision semantics as the Rust guard.
    let mut seen = db.active_pet(owner).await.unwrap().unwrap();
    seen.needs_at = now;
    let ends = now + chrono::Duration::hours(4);
    assert!(
        db.start_trip(owner, &seen, "shore", [10, 20, 30, 40], ends)
            .await
            .unwrap()
    );
    assert!(!db.save_pet(owner, &seen, [1, 2, 3, 4], 1, 0, ends).await.unwrap());
    seen = db.active_pet(owner).await.unwrap().unwrap();
    seen.needs_at = ends;
    seen.trip_ends_at = Some(ends);
    assert!(db.save_pet(owner, &seen, [50, 50, 100, 50], 1, 0, ends).await.unwrap());
}

#[tokio::test]
async fn care_commits_payment_and_pet_together_under_races_and_refusals() {
    let _isolation = crate::support::shared().await;
    use pinchora_db::PetCare;
    let Some(db) = db(5).await else { return };
    let [owner] = ids();
    let pet = db.adopt(owner, "crumbclaw_crab", "Pip").await.unwrap();
    let now = chrono::Utc::now();
    assert_eq!(
        db.care_pet(owner, &pet, [90; 4], 1, 15, now, -10).await.unwrap(),
        PetCare::Poor
    );
    assert_eq!(db.reef_state(owner).await.unwrap().pet_streak, 0);
    assert_eq!(db.reef_state(owner).await.unwrap().seaweed, 0);
    let unchanged = db.pets(owner).await.unwrap().remove(0);
    assert_eq!(unchanged.xp, pet.xp);
    assert_eq!(unchanged.needs_at, pet.needs_at);
    db.earn_shells(owner, 100, "test").await.unwrap();
    let (first, second) = tokio::join!(
        db.care_pet(owner, &pet, [90; 4], 1, 15, now, -10),
        db.care_pet(owner, &pet, [90; 4], 1, 15, now, -10),
    );
    let outcomes = [first.unwrap(), second.unwrap()];
    assert_eq!(outcomes.iter().filter(|v| **v == PetCare::Saved).count(), 1);
    assert_eq!(outcomes.iter().filter(|v| **v == PetCare::Stale).count(), 1);
    assert_eq!(db.user(owner).await.unwrap().credits, 90);
    assert_eq!(db.pets(owner).await.unwrap()[0].xp, 15);
    assert_eq!(db.reef_state(owner).await.unwrap().pet_streak, 1);
    assert_eq!(db.reef_state(owner).await.unwrap().seaweed, 1);
    let ledger: Vec<(i64, String)> =
        sqlx::query_as("SELECT delta, reason FROM credit_ledger WHERE user_id=$1 AND reason LIKE 'pet_%'")
            .bind(owner as i64)
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert_eq!(ledger, vec![(-10, "pet_food".into())]);
    // A stale walk cannot pay a reward either.
    assert_eq!(
        db.care_pet(owner, &pet, [90; 4], 1, 40, now, 30).await.unwrap(),
        PetCare::Stale
    );
    assert_eq!(db.user(owner).await.unwrap().credits, 90);
    assert_eq!(db.reef_state(owner).await.unwrap().seaweed, 1);
}
