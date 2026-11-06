//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.

use chrono::{Duration, Utc};
use pinchora_db::{
    Award, BottleOpen, BottlePrize, Comb, DROP_BOTTLE, DROP_CRITTERS, Daily, Db, Grab, NewDrop, PotHaul, PotUpgrade,
};

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect(&url, 10).await.expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}

/// Ids unique per run, so the suite can run repeatedly against one database.
fn ids<const N: usize>() -> [u64; N] {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        >> 4;
    std::array::from_fn(|i| base + i as u64)
}

fn award(entry: &str) -> Award<'_> {
    Award {
        entry,
        new_shells: 15,
        dup_shells: 6,
        dup_fragments: 1,
        bonus_fragments: 0,
    }
}

fn daily(game: &'static str, cap: i32) -> Daily<'static> {
    Daily { game, day: 20_000, cap }
}

async fn shells(db: &Db, user: u64) -> i64 {
    db.user(user).await.unwrap().credits
}

/// What the ledger says the given players hold in total, which must match their balances.
async fn ledger_total(db: &Db, users: &[u64]) -> i64 {
    let ids: Vec<i64> = users.iter().map(|u| *u as i64).collect();
    let (sum,): (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM credit_ledger WHERE user_id = ANY($1)")
            .bind(ids)
            .fetch_one(db.pool())
            .await
            .unwrap();
    sum
}

async fn critter_drop(db: &Db, guild: u64, cards: &[&str], lasts: Duration) -> pinchora_db::DropRow {
    let cards: Vec<String> = cards.iter().map(|c| (*c).to_owned()).collect();
    db.create_drop(NewDrop {
        kind: DROP_CRITTERS,
        guild_id: guild,
        channel_id: guild + 1,
        host: None,
        cards: &cards,
        expires_at: Utc::now() + lasts,
    })
    .await
    .unwrap()
}

async fn bottle(db: &Db, guild: u64, lasts: Duration) -> pinchora_db::DropRow {
    db.create_drop(NewDrop {
        kind: DROP_BOTTLE,
        guild_id: guild,
        channel_id: guild + 1,
        host: None,
        cards: &["bottle_01".to_owned()],
        expires_at: Utc::now() + lasts,
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn settings_start_off_and_change_one_field_at_a_time() {
    let Some(db) = db().await else { return };
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
async fn a_card_goes_to_one_player_and_each_player_takes_one() {
    let Some(db) = db().await else { return };
    let [guild, a, b, c] = ids();
    let drop = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    let now = Utc::now();

    let Grab::Taken { granted, paid, .. } = db
        .grab_card(drop.id, 0, a, now, award("x_one"), daily("critterdrop", 20))
        .await
        .unwrap()
    else {
        panic!("a free card is taken");
    };
    assert!(granted.new && paid && granted.shells == 15 && granted.caught == 1);
    assert_eq!(shells(&db, a).await, 15);

    // The same card for somebody else, and a second card for the same player.
    assert!(matches!(
        db.grab_card(drop.id, 0, b, now, award("x_one"), daily("critterdrop", 20)).await.unwrap(),
        Grab::Lost { by } if by == a
    ));
    assert!(matches!(
        db.grab_card(drop.id, 1, a, now, award("x_two"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Have { slot: 0 }
    ));
    // A message that showed another card in that slot is out of date.
    assert!(matches!(
        db.grab_card(drop.id, 1, b, now, award("x_three"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Gone
    ));
    assert!(matches!(
        db.grab_card(drop.id, 1, b, now, award("x_two"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Taken { .. }
    ));
    // After the drop's time, nothing is taken.
    assert!(matches!(
        db.grab_card(
            drop.id,
            2,
            c,
            now + Duration::seconds(91),
            award("x_three"),
            daily("critterdrop", 20)
        )
        .await
        .unwrap(),
        Grab::Gone
    ));
    assert_eq!(shells(&db, c).await, 0);
    assert_eq!(db.dex(c).await.unwrap().len(), 0);
    assert_eq!(ledger_total(&db, &[a, b, c]).await, 30);
}

#[tokio::test]
async fn racing_clicks_on_one_card_pay_one_player() {
    let Some(db) = db().await else { return };
    let [guild, u0, u1, u2, u3, u4, u5, u6, u7] = ids();
    let players = [u0, u1, u2, u3, u4, u5, u6, u7];
    let drop = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    let now = Utc::now();

    let mut tasks = Vec::new();
    for user in players {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.grab_card(drop.id, 0, user, now, award("x_one"), daily("critterdrop", 20))
                .await
                .unwrap()
        }));
    }
    let mut taken = 0;
    for task in tasks {
        match task.await.unwrap() {
            Grab::Taken { .. } => taken += 1,
            Grab::Lost { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(taken, 1, "exactly one click takes the card");
    assert_eq!(ledger_total(&db, &players).await, 15, "and only one is paid");
    let row = db.drop_row(drop.id).await.unwrap().unwrap();
    assert_eq!(row.takers.iter().filter(|t| **t != 0).count(), 1);
}

#[tokio::test]
async fn a_spare_is_recycled_and_the_day_has_a_limit() {
    let Some(db) = db().await else { return };
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
async fn a_bottle_opens_once_and_pays_its_prize() {
    let Some(db) = db().await else { return };
    let [guild, a, b] = ids();
    let now = Utc::now();
    let cap = daily("bottle", 10);

    let first = bottle(&db, guild, Duration::minutes(10)).await;
    let BottleOpen::Opened {
        shells: got,
        drop,
        paid,
        ..
    } = db
        .open_bottle(first.id, a, now, BottlePrize::Shells(40), cap)
        .await
        .unwrap()
    else {
        panic!("opened");
    };
    assert!(paid && got == 40 && !drop.is_open());
    // The loser is told who was quicker, even though opening closed the bottle.
    assert!(matches!(
        db.open_bottle(first.id, b, now, BottlePrize::Shells(40), cap)
            .await
            .unwrap(),
        BottleOpen::Lost { by } if by == a
    ));
    // That stays true once its time has passed.
    assert!(matches!(
        db.open_bottle(first.id, b, now + Duration::minutes(11), BottlePrize::Shells(40), cap)
            .await
            .unwrap(),
        BottleOpen::Lost { by } if by == a
    ));
    assert_eq!(shells(&db, a).await, 40);
    assert_eq!(shells(&db, b).await, 0);

    // A find goes into the dex and pays like a card; fragments and treasure pay as they say.
    let second = bottle(&db, guild, Duration::minutes(10)).await;
    let BottleOpen::Opened { granted, .. } = db
        .open_bottle(second.id, a, now, BottlePrize::Find(award("f_pearl")), cap)
        .await
        .unwrap()
    else {
        panic!("opened");
    };
    assert!(granted.is_some_and(|g| g.new && g.shells == 15));
    let third = bottle(&db, guild, Duration::minutes(10)).await;
    db.open_bottle(third.id, a, now, BottlePrize::Fragments(7), cap)
        .await
        .unwrap();
    assert_eq!(db.fragments(a).await.unwrap(), 7);
    let fourth = bottle(&db, guild, Duration::minutes(10)).await;
    db.open_bottle(fourth.id, a, now, BottlePrize::Treasure(200), cap)
        .await
        .unwrap();
    assert_eq!(shells(&db, a).await, 40 + 15 + 200);
    assert_eq!(ledger_total(&db, &[a, b]).await, 40 + 15 + 200);

    // A bottle whose time ran out, or one that is not a bottle, cannot be opened.
    let late = bottle(&db, guild, Duration::minutes(10)).await;
    assert!(matches!(
        db.open_bottle(late.id, b, now + Duration::minutes(11), BottlePrize::Shells(40), cap)
            .await
            .unwrap(),
        BottleOpen::Gone
    ));
    let cards = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert!(matches!(
        db.open_bottle(cards.id, b, now, BottlePrize::Shells(40), cap)
            .await
            .unwrap(),
        BottleOpen::Gone
    ));
}

#[tokio::test]
async fn racing_openers_pay_one_player_and_a_spent_day_pays_nothing() {
    let Some(db) = db().await else { return };
    let [guild, u0, u1, u2, u3, u4, u5] = ids();
    let players = [u0, u1, u2, u3, u4, u5];
    let now = Utc::now();
    let drop = bottle(&db, guild, Duration::minutes(10)).await;
    let mut tasks = Vec::new();
    for user in players {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.open_bottle(drop.id, user, now, BottlePrize::Shells(40), daily("bottle", 10))
                .await
                .unwrap()
        }));
    }
    let mut opened = 0;
    for task in tasks {
        if matches!(task.await.unwrap(), BottleOpen::Opened { .. }) {
            opened += 1;
        }
    }
    assert_eq!(opened, 1);
    assert_eq!(ledger_total(&db, &players).await, 40);

    // With the day's limit used up the bottle still opens, and holds only its message.
    let [late] = ids();
    let limit = daily("bottle", 0);
    let again = bottle(&db, guild, Duration::minutes(10)).await;
    let BottleOpen::Opened { shells, paid, .. } = db
        .open_bottle(again.id, late, now, BottlePrize::Shells(40), limit)
        .await
        .unwrap()
    else {
        panic!("opened");
    };
    assert!(!paid && shells == 0);
    assert_eq!(ledger_total(&db, &[late]).await, 0);
}

#[tokio::test]
async fn due_drops_close_once() {
    let Some(db) = db().await else { return };
    let [guild, other] = ids();
    // One that ran out an hour ago and one still floating, in channels of their own.
    let late = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::hours(-1)).await;
    let live = critter_drop(&db, other, &["x_one", "x_two", "x_three"], Duration::minutes(5)).await;
    let now = Utc::now();
    assert!(
        !db.drop_waiting_in(guild + 1, now).await.unwrap(),
        "past its time nothing waits"
    );
    assert!(db.drop_waiting_in(other + 1, now).await.unwrap());
    let (a, b) = tokio::join!(db.close_due_drops(now, 500), db.close_due_drops(now, 500));
    let count = |rows: Vec<pinchora_db::DropRow>, id: i64| rows.iter().filter(|d| d.id == id).count();
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(
        count(a.clone(), late.id) + count(b.clone(), late.id),
        1,
        "one pod closes it"
    );
    assert_eq!(
        count(a, live.id) + count(b, live.id),
        0,
        "a drop still floating is left alone"
    );
    assert!(!db.drop_row(late.id).await.unwrap().unwrap().is_open());
    assert!(db.drop_row(live.id).await.unwrap().unwrap().is_open());
}

#[tokio::test]
async fn a_beachcomb_waits_and_pays_once_per_wait() {
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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

/// A starter room for `user`, so there is a house to place pieces in.
async fn house_of(db: &Db, user: u64) {
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    db.house(user, &starter).await.unwrap();
}

async fn pieces(db: &Db, user: u64, item: &str) -> i32 {
    db.inventory(user)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.item_id == item)
        .map_or(0, |r| r.quantity)
}

#[tokio::test]
async fn soulbound_pieces_can_be_placed_but_never_leave_their_owner() {
    use pinchora_db::{PlaceOutcome, Refusal, RestoreOutcome, RoomChange, SOULBOUND_GUEST};

    let Some(db) = db().await else { return };
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
