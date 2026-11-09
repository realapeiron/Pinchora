use super::*;

#[tokio::test]
async fn wishlist_reserves_one_slot_then_releases_it() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
    let [guild, wisher, other] = ids();
    assert!(db.set_wish(wisher, "x_one", true).await.unwrap());
    sqlx::query("INSERT INTO members (guild_id, user_id) VALUES ($1, $2)")
        .bind(guild as i64)
        .bind(wisher as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let drop = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert_eq!(drop.protected_user_id, Some(wisher as i64));
    assert_eq!(drop.protected_slot, Some(0));
    let now = Utc::now();
    assert!(matches!(
        db.grab_card(drop.id, 0, other, now, award("x_one"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Protected
    ));
    assert!(matches!(
        db.grab_card(drop.id, 1, other, now, award("x_two"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Taken { .. }
    ));
    assert!(matches!(
        db.grab_card(drop.id, 0, wisher, now, award("x_one"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Taken { .. }
    ));
    assert!(db.remove_wish(wisher, "x_one").await.unwrap());
}

#[tokio::test]
async fn vote_reveals_one_card_and_only_that_card_can_be_claimed() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db(10).await else { return };
    // The exclusive guard makes old fixtures safe to clear before the bounded worker scan.
    sqlx::query("DELETE FROM collection_drops")
        .execute(db.pool())
        .await
        .unwrap();
    let [guild, a, b] = ids();
    let drop = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    db.set_drop_mode(drop.id, "vote", None).await.unwrap();
    let now = Utc::now();
    assert_eq!(db.vote_card(drop.id, 1, a, now).await.unwrap(), VoteCard::Cast);
    assert_eq!(db.vote_card(drop.id, 1, b, now).await.unwrap(), VoteCard::Cast);
    let revealed = db.reveal_due_votes(now + Duration::seconds(31), 20).await.unwrap();
    assert!(
        revealed
            .iter()
            .any(|row| row.id == drop.id && row.vote_winner == Some(1))
    );
    assert!(matches!(
        db.grab_card(
            drop.id,
            0,
            a,
            now + Duration::seconds(31),
            award("x_one"),
            daily("critterdrop", 20)
        )
        .await
        .unwrap(),
        Grab::Voting
    ));
    assert!(matches!(
        db.grab_card(
            drop.id,
            1,
            a,
            now + Duration::seconds(31),
            award("x_two"),
            daily("critterdrop", 20)
        )
        .await
        .unwrap(),
        Grab::Taken { .. }
    ));
}

#[tokio::test]
async fn hourly_roll_claims_exactly_once_and_waits_an_hour() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
    let [user] = ids();
    let now = Utc::now();
    assert!(matches!(
        db.start_collection_roll(user, "x_one", true, now).await.unwrap(),
        RollStart::Started(_)
    ));
    assert!(matches!(
        db.start_collection_roll(user, "x_two", false, now).await.unwrap(),
        RollStart::Wait(_)
    ));
    let (a, b) = tokio::join!(
        db.claim_collection_roll(user, now, award("x_one")),
        db.claim_collection_roll(user, now, award("x_one"))
    );
    assert_eq!(
        [a.unwrap(), b.unwrap()]
            .iter()
            .filter(|r| matches!(r, RollClaim::Claimed(_)))
            .count(),
        1
    );
    assert_eq!(db.dex(user).await.unwrap()[0].glimmer, 1);
    assert_eq!(shells(&db, user).await, 15);
}

#[tokio::test]
async fn eleventh_daily_drop_claim_is_refused_without_taking_a_slot() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
    let [guild, user] = ids();
    let now = Utc::now();
    for n in 0..11 {
        let drop = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
        let result = db
            .grab_card(drop.id, 0, user, now, award("x_one"), daily("critterdrop", 20))
            .await
            .unwrap();
        if n < 10 {
            assert!(matches!(result, Grab::Taken { .. }));
        } else {
            assert!(matches!(result, Grab::Limit));
            assert_eq!(db.drop_row(drop.id).await.unwrap().unwrap().takers[0], 0);
        }
    }
}

#[tokio::test]
async fn a_card_goes_to_one_player_and_each_player_takes_one() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
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
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
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
async fn a_bottle_opens_once_and_pays_its_prize() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
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
    let _isolation = crate::support::shared().await;
    let Some(db) = db(10).await else { return };
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
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db(10).await else { return };
    // The exclusive guard makes old fixtures safe to clear before the bounded worker scan.
    sqlx::query("DELETE FROM collection_drops")
        .execute(db.pool())
        .await
        .unwrap();
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
