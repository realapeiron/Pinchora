use crate::collection::*;

#[tokio::test]
async fn glimmer_duplicate_recycles_and_crafts_bound_loot() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
    let [guild, user] = ids();
    let now = Utc::now();
    let first = critter_drop(&db, guild, &["g:x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert!(matches!(
        db.grab_card(first.id, 0, user, now, award("x_one"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Taken { .. }
    ));
    let second = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert!(matches!(
        db.grab_card(second.id, 0, user, now, award("x_one"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Taken { .. }
    ));
    let dex = db.dex(user).await.unwrap();
    assert_eq!((dex[0].caught, dex[0].kept, dex[0].glimmer), (2, 2, 1));
    assert_eq!(db.recycle_critter(user, "x_one", 12, "too-early").await.unwrap(), None);
    let third = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    db.grab_card(third.id, 0, user, now, award("x_one"), daily("critterdrop", 20))
        .await
        .unwrap();
    assert_eq!(
        db.recycle_critter(user, "x_one", 12, "recycle").await.unwrap(),
        Some(12)
    );
    assert_eq!(
        db.recycle_critter(user, "x_one", 12, "recycle").await.unwrap(),
        Some(12)
    );
    assert_eq!(
        db.recycle_critter(user, "x_one", 12, "recycle-again").await.unwrap(),
        None
    );
    assert_eq!(
        db.craft_shard_loot(user, 12, "sprigstep_planter_box__mint", "craft")
            .await
            .unwrap(),
        Some(0)
    );
    assert_eq!(db.shards(user).await.unwrap(), 0);
}

#[tokio::test]
async fn settings_start_off_and_change_one_field_at_a_time() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
    let [guild] = ids();
    let s = db.collection_settings(guild).await.unwrap();
    assert!(!s.enabled && s.channel_id.is_none());

    let s = db.set_collection_settings(guild, Some(true), None).await.unwrap();
    assert!(s.enabled && s.channel_id.is_none());
    let s = db.set_collection_settings(guild, None, Some(Some(55))).await.unwrap();
    assert!(s.enabled && s.channel_id == Some(55), "the channel did not turn it off");
    let s = db.set_collection_settings(guild, None, None).await.unwrap();
    assert!(s.enabled && s.channel_id == Some(55), "no change at all");
    let s = db
        .set_collection_settings(guild, Some(false), Some(None))
        .await
        .unwrap();
    assert!(!s.enabled && s.channel_id.is_none());
    assert_eq!(db.collection_settings(guild).await.unwrap(), s);
}

#[tokio::test]
async fn a_spare_is_recycled_and_the_day_has_a_limit() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
    let [guild, a] = ids();
    let now = Utc::now();
    let cap = daily("critterdrop", 2);

    let first = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    let Grab::Taken { granted, .. } = db.grab_card(first.id, 0, a, now, award("x_one"), cap).await.unwrap() else {
        panic!("taken");
    };
    assert!(granted.new && granted.shells == 15 && granted.fragments == 0);

    // The same critter again is a spare: it pays less and leaves a fragment.
    let second = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    let Grab::Taken { granted, paid, .. } = db.grab_card(second.id, 0, a, now, award("x_one"), cap).await.unwrap()
    else {
        panic!("taken");
    };
    assert!(!granted.new && paid && granted.caught == 2);
    assert_eq!((granted.shells, granted.fragments), (6, 1));
    assert_eq!(db.fragments(a).await.unwrap(), 1);

    // The third play of the day is past the limit: the critter is filed, nothing is paid.
    let third = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    let Grab::Taken { granted, paid, .. } = db.grab_card(third.id, 1, a, now, award("x_two"), cap).await.unwrap()
    else {
        panic!("taken");
    };
    assert!(!paid && granted.new && granted.shells == 0);
    assert_eq!(db.dex(a).await.unwrap().len(), 2);
    assert_eq!(shells(&db, a).await, 21);
    assert_eq!(ledger_total(&db, &[a]).await, 21);

    // Another day starts counting again.
    let fourth = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    let next_day = Daily { day: 20_001, ..cap };
    let Grab::Taken { paid, .. } = db
        .grab_card(fourth.id, 2, a, now, award("x_three"), next_day)
        .await
        .unwrap()
    else {
        panic!("taken");
    };
    assert!(paid);
}

#[tokio::test]
async fn a_beachcomb_waits_and_pays_once_per_wait() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
    let [a] = ids();
    let now = Utc::now();
    let wait = Duration::minutes(20);
    let cap = daily("beachcomb", 24);

    let Comb::Found { granted, paid } = db.beachcomb(a, now, wait, award("f_shell"), cap).await.unwrap() else {
        panic!("the first walk finds something");
    };
    assert!(paid && granted.new && granted.shells == 15);
    let Comb::Wait { ready_at } = db
        .beachcomb(a, now + Duration::minutes(5), wait, award("f_shell"), cap)
        .await
        .unwrap()
    else {
        panic!("too soon");
    };
    assert!(ready_at > now + Duration::minutes(19) && ready_at < now + Duration::minutes(21));
    assert_eq!(shells(&db, a).await, 15);
    // After the wait it is a spare of the same find.
    let Comb::Found { granted, .. } = db
        .beachcomb(a, now + Duration::minutes(21), wait, award("f_shell"), cap)
        .await
        .unwrap()
    else {
        panic!("the wait is over");
    };
    assert!(!granted.new && granted.shells == 6);
    assert_eq!(ledger_total(&db, &[a]).await, 21);
}

#[tokio::test]
async fn racing_beachcombs_pay_once() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
    let [a] = ids();
    let now = Utc::now();
    let mut tasks = Vec::new();
    for _ in 0..6 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.beachcomb(a, now, Duration::minutes(20), award("f_shell"), daily("beachcomb", 24))
                .await
                .unwrap()
        }));
    }
    let mut found = 0;
    for task in tasks {
        if matches!(task.await.unwrap(), Comb::Found { .. }) {
            found += 1;
        }
    }
    assert_eq!(found, 1);
    assert_eq!(ledger_total(&db, &[a]).await, 15);
}

#[tokio::test]
async fn a_pot_soaks_then_is_hauled_in_exactly_once() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
    let [a] = ids();
    let now = Utc::now();
    let cap = daily("crabpot", 3);
    let catches = |_level: i16, hours: i16, _ends: chrono::DateTime<Utc>| -> Vec<Award<'static>> {
        let mut all = vec![award("x_one"), award("x_one")];
        if hours >= 4 {
            all.push(award("x_two"));
        }
        all
    };

    assert!(matches!(
        db.haul_pot(a, now, cap, catches).await.unwrap(),
        PotHaul::Empty
    ));
    assert!(db.start_pot(a, 4, now).await.unwrap());
    assert!(!db.start_pot(a, 1, now).await.unwrap(), "one soak at a time");
    let pot = db.pot(a).await.unwrap();
    assert_eq!((pot.level, pot.hours), (1, Some(4)));
    assert!(matches!(
        db.haul_pot(a, now + Duration::hours(3), cap, catches).await.unwrap(),
        PotHaul::Soaking { .. }
    ));

    let done = now + Duration::hours(4) + Duration::seconds(1);
    let mut tasks = Vec::new();
    for _ in 0..5 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.haul_pot(a, done, cap, catches).await.unwrap()
        }));
    }
    let mut hauls = Vec::new();
    for task in tasks {
        if let PotHaul::Hauled {
            granted, paid, hours, ..
        } = task.await.unwrap()
        {
            hauls.push((granted, paid, hours));
        }
    }
    assert_eq!(hauls.len(), 1, "only one haul is paid");
    let (granted, paid, hours) = &hauls[0];
    assert!(*paid && *hours == 4 && granted.len() == 3);
    // x_one new (15), x_one again (6 + 1 fragment), x_two new (15).
    assert_eq!(granted.iter().map(|g| g.shells).sum::<i64>(), 36);
    assert_eq!(shells(&db, a).await, 36);
    assert_eq!(db.fragments(a).await.unwrap(), 1);
    assert_eq!(db.dex(a).await.unwrap().len(), 2);
    assert!(db.pot(a).await.unwrap().ends_at.is_none());
    assert_eq!(ledger_total(&db, &[a]).await, 36);
    // And the pot can be set again.
    assert!(db.start_pot(a, 1, done).await.unwrap());
}

#[tokio::test]
async fn upgrading_a_pot_costs_shells_once_per_level() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
    let [a] = ids();
    assert_eq!(
        db.upgrade_pot(a, 1, 400).await.unwrap(),
        PotUpgrade::Poor { balance: 0 }
    );
    db.earn_shells(a, 500, "test").await.unwrap();
    // Two upgrades from the same level at once: one wins.
    let (x, y) = tokio::join!(db.upgrade_pot(a, 1, 400), db.upgrade_pot(a, 1, 400));
    let results = [x.unwrap(), y.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, PotUpgrade::Upgraded { .. }))
            .count(),
        1
    );
    assert_eq!(results.iter().filter(|r| matches!(r, PotUpgrade::Stale)).count(), 1);
    assert_eq!(shells(&db, a).await, 100);
    assert_eq!(db.pot(a).await.unwrap().level, 2);
    assert_eq!(
        db.upgrade_pot(a, 2, 1_000).await.unwrap(),
        PotUpgrade::Poor { balance: 100 }
    );
    assert_eq!(ledger_total(&db, &[a]).await, 100);
    // The top level cannot be upgraded.
    sqlx::query("UPDATE collection_players SET pot_level = 5 WHERE user_id = $1")
        .bind(a as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(db.upgrade_pot(a, 5, 1).await.unwrap(), PotUpgrade::Stale);
}

#[tokio::test]
async fn fragments_buy_decor_once() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
    let [a] = ids();
    assert_eq!(db.exchange_fragments(a, 25, "dewtrail_print").await.unwrap(), None);
    // 30 fragments from spares, enough for one 25-fragment piece, not for two at once.
    let now = Utc::now();
    let bottle_row = bottle(&db, a, Duration::minutes(10)).await;
    db.open_bottle(bottle_row.id, a, now, BottlePrize::Fragments(30), daily("bottle", 10))
        .await
        .unwrap();
    assert_eq!(db.fragments(a).await.unwrap(), 30);
    let (x, y) = tokio::join!(
        db.exchange_fragments(a, 25, "dewtrail_print"),
        db.exchange_fragments(a, 25, "dewtrail_print")
    );
    let got: Vec<_> = [x.unwrap(), y.unwrap()].into_iter().flatten().collect();
    assert_eq!(got, vec![5], "one piece, five fragments left");
    // The piece is in the inventory as a soulbound one: its own row, never the plain piece.
    let have = db.inventory(a).await.unwrap();
    assert_eq!(
        have.iter()
            .find(|r| r.item_id == "dewtrail_print~bound")
            .map(|r| r.quantity),
        Some(1)
    );
    assert!(have.iter().all(|r| r.item_id != "dewtrail_print"));
}

#[tokio::test]
async fn soulbound_pieces_can_be_placed_but_never_leave_their_owner() {
    let _isolation = crate::support::shared().await;
    use pinchora_db::{PlaceOutcome, Refusal, RestoreOutcome, RoomChange, SOULBOUND_GUEST};

    let Some(db) = db(10).await else { return };
    let [a, b, editor] = ids();
    let (bound, plain) = ("dewtrail_print~bound", "dewtrail_print");
    house_of(&db, a).await;
    house_of(&db, b).await;
    let bottle_row = bottle(&db, a, Duration::minutes(10)).await;
    db.open_bottle(
        bottle_row.id,
        a,
        Utc::now(),
        BottlePrize::Fragments(100),
        daily("bottle", 10),
    )
    .await
    .unwrap();
    db.exchange_fragments(a, 25, plain).await.unwrap().expect("a piece");
    assert_eq!((pieces(&db, a, bound).await, pieces(&db, a, plain).await), (1, 0));

    // A gift or a plaza donation (the same move) is refused, and nothing leaves the inventory.
    assert!(!db.gift_item(a, b, bound).await.unwrap());
    assert!(!db.gift_item(a, b + 1, bound).await.unwrap());
    assert_eq!((pieces(&db, a, bound).await, pieces(&db, b, bound).await), (1, 0));

    // So is a listing on a stall.
    assert!(!db.list_item(a, bound, 500).await.unwrap());
    assert!(db.listings(a).await.unwrap().is_empty());
    assert_eq!(pieces(&db, a, bound).await, 1);

    // A partner's house is somebody else's: the piece cannot be placed there either.
    let blocked = db
        .place_item_for(b, a, 0, bound, 0, 0, false, |_| Ok(()))
        .await
        .unwrap();
    assert!(
        matches!(&blocked, PlaceOutcome::Blocked(why) if why == SOULBOUND_GUEST),
        "{blocked:?}"
    );
    assert!(db.placements(b, 0).await.unwrap().is_empty());
    assert_eq!(pieces(&db, a, bound).await, 1);

    // In their own house it goes, and it stays soulbound through picking up, placing again and clearing.
    let PlaceOutcome::Placed { id } = db.place_item(a, 0, bound, 1, 1, false, |_| Ok(())).await.unwrap() else {
        panic!("placed in their own house");
    };
    assert_eq!(pieces(&db, a, bound).await, 0);
    assert_eq!(db.pick_up(a, id).await.unwrap().as_deref(), Some(bound));
    assert_eq!((pieces(&db, a, bound).await, pieces(&db, a, plain).await), (1, 0));
    db.place_item(a, 0, bound, 1, 1, false, |_| Ok(())).await.unwrap();
    let cleared = db.clear_room(a, 0).await.unwrap();
    assert_eq!(cleared.len(), 1);
    assert_eq!((pieces(&db, a, bound).await, pieces(&db, a, plain).await), (1, 0));

    // Even an undo cannot hand a soulbound placement to an editor who is not the owner.
    let PlaceOutcome::Placed { id } = db.place_item(a, 0, bound, 1, 1, false, |_| Ok(())).await.unwrap() else {
        panic!("placed");
    };
    let placement = db
        .placements(a, 0)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap();
    let handed = db
        .restore_for(a, editor, 0, &[RoomChange::Remove(placement.clone())], |_, _| Ok(()))
        .await
        .unwrap();
    assert!(
        matches!(handed, RestoreOutcome::Refused(Refusal::Blocked(_))),
        "{handed:?}"
    );
    assert_eq!(db.placements(a, 0).await.unwrap().len(), 1);
    assert_eq!(pieces(&db, editor, bound).await, 0);
    // The owner's own undo takes it back as theirs.
    let back = db
        .restore(a, 0, &[RoomChange::Remove(placement)], |_, _| Ok(()))
        .await
        .unwrap();
    assert!(matches!(back, RestoreOutcome::Done { .. }));
    assert_eq!(pieces(&db, a, bound).await, 1);

    // The plain piece of the same design moves freely, so the marks are what tell them apart.
    db.grant_item(a, plain, 1).await.unwrap();
    assert!(db.gift_item(a, b, plain).await.unwrap());
    db.grant_item(a, plain, 1).await.unwrap();
    assert!(db.list_item(a, plain, 500).await.unwrap());
    db.grant_item(a, plain, 1).await.unwrap();
    let PlaceOutcome::Placed { .. } = db
        .place_item_for(b, a, 0, plain, 0, 0, false, |_| Ok(()))
        .await
        .unwrap()
    else {
        panic!("a plain piece goes into a partner's house");
    };
    assert_eq!(pieces(&db, a, bound).await, 1, "and the soulbound one never moved");
}
