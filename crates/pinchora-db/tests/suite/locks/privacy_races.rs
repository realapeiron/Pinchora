use super::*;

#[tokio::test]
async fn guild_settings_reads_a_concurrent_first_insert_after_it_commits() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(3).await else { return };
    let [guild] = ids();
    let mut first = db.pool().begin().await.unwrap();
    sqlx::query("INSERT INTO guilds (id) VALUES ($1)")
        .bind(guild as i64)
        .execute(&mut *first)
        .await
        .unwrap();
    let (reader, pid) = side().await;
    let read = tokio::spawn(async move { reader.guild_settings(guild).await });
    queued(&db, pid, "guild settings lookup").await;
    first.commit().await.unwrap();
    assert_eq!(read.await.unwrap().unwrap().id, guild as i64);
}

#[tokio::test]
async fn accepting_a_duel_while_its_host_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [host, guest, guild] = ids();
    db.earn_shells(host, 100, "test").await.unwrap();
    db.earn_shells(guest, 100, "test").await.unwrap();
    let session = db.create_game(duel(host, guest, 10, guild)).await.unwrap();

    let (started, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM game_sessions WHERE id = $1 FOR UPDATE",
        session.id,
        host,
        move |db| async move { db.start_duel(session.id, guest, (8, 9), "go", 600).await },
    )
    .await;
    let started = started.expect("the accept fails nowhere");
    deleted.expect("the delete fails nowhere");
    assert!(matches!(started, DuelStart::Started(_)), "{started:?}");
    assert_eq!(
        credits(&db, guest).await,
        100,
        "the guest's bet came back with the delete"
    );
    assert_eq!(ledger(&db, guest).await, 100);
    db.delete_user_data(guest, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn settling_a_duel_while_its_winner_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [winner, loser, guild] = ids();
    db.earn_shells(winner, 100, "test").await.unwrap();
    db.earn_shells(loser, 100, "test").await.unwrap();
    let session = db.create_game(duel(winner, loser, 10, guild)).await.unwrap();
    assert!(matches!(
        db.start_duel(session.id, loser, (8, 9), "go", 600).await.unwrap(),
        DuelStart::Started(_)
    ));

    let (settled, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM game_sessions WHERE id = $1 FOR UPDATE",
        session.id,
        winner,
        move |db| async move {
            db.settle_game(session.id, None, "won", Payout::Winner(winner), false)
                .await
        },
    )
    .await;
    assert!(settled.expect("the settle fails nowhere").is_some());
    deleted.expect("the delete fails nowhere");
    assert_eq!((credits(&db, loser).await, ledger(&db, loser).await), (90, 90));
    assert_eq!(credits(&db, winner).await, 0, "the winner's pot went with their data");
    db.delete_user_data(loser, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn ending_a_giveaway_while_its_host_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [host, guest, guild] = ids();
    db.earn_shells(host, 100, "test").await.unwrap();
    let GiveawayOpen::Started(giveaway) = db
        .create_giveaway(NewGiveaway {
            guild_id: guild,
            channel_id: 1,
            host,
            prize: "Shells",
            shells: 10,
            winners: 2,
            required_role: None,
            ends_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("the giveaway starts");
    };
    db.toggle_giveaway_entry(giveaway.id, guest, Utc::now()).await.unwrap();

    let id = giveaway.id;
    let (ended, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM giveaways WHERE id = $1 FOR UPDATE",
        id,
        host,
        move |db| async move {
            db.end_giveaway(id, Utc::now(), true, |entrants, n| {
                entrants.iter().copied().take(n).collect()
            })
            .await
        },
    )
    .await;
    let ended = ended.expect("the end fails nowhere").expect("it was running");
    deleted.expect("the delete fails nowhere");
    assert_eq!((ended.winners, ended.refunded), (vec![guest], 10));
    assert_eq!((credits(&db, guest).await, ledger(&db, guest).await), (10, 10));
    assert_eq!(credits(&db, host).await, 0);
    db.delete_user_data(guest, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn chat_xp_while_the_player_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [user, guild] = ids();
    let grant = XpGrant {
        server_xp: 5,
        global_xp: 5,
        points: 1,
    };
    let week = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
    db.add_xp(guild, user, grant, week).await.unwrap();
    db.claim_daily(user).await.unwrap();

    let ((caller, caller_pid), (deleter, deleter_pid)) = (side().await, side().await);
    let mut blocker = db.pool().begin().await.unwrap();
    sqlx::query("SELECT 1 FROM members WHERE guild_id = $1 AND user_id = $2 FOR UPDATE")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let xp = tokio::spawn(async move { caller.add_xp(guild, user, grant, week).await });
    queued(&db, caller_pid, "the chat XP").await;
    let delete = tokio::spawn(async move { deleter.delete_user_data(user, Utc::now()).await });
    queued(&db, deleter_pid, "the delete").await;
    blocker.commit().await.unwrap();

    let gain = xp.await.unwrap().expect("chat XP fails nowhere");
    delete.await.unwrap().expect("the delete fails nowhere");
    assert_eq!((gain.old_xp, gain.new_xp), (5, 10));
    let (members,): (i64,) = sqlx::query_as("SELECT count(*) FROM members WHERE user_id = $1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(members, 0, "the delete ran after the XP and took it");
}

#[tokio::test]
async fn claiming_a_trip_while_the_owner_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [owner] = ids();
    let pet = db.adopt(owner, "cat", "Tom").await.unwrap();
    assert!(
        db.start_trip(
            owner,
            &pet,
            "beach",
            [50, 50, 50, 50],
            Utc::now() - Duration::minutes(1)
        )
        .await
        .unwrap()
    );

    let pet_id = pet.id;
    let (claimed, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM pets WHERE id = $1 FOR UPDATE",
        pet_id,
        owner,
        move |db| async move {
            db.claim_trip(owner, pet_id, Utc::now(), |p, _| TripPayout {
                shells: 5,
                item: None,
                level: p.level,
                xp: p.xp,
            })
            .await
        },
    )
    .await;
    let claimed = claimed.expect("the claim fails nowhere");
    deleted.expect("the delete fails nowhere");
    assert!(matches!(claimed, TripClaim::Claimed { balance: 5, .. }), "{claimed:?}");
    assert_eq!(credits(&db, owner).await, 0, "the souvenir went with their data");
}

#[tokio::test]
async fn waking_a_pet_while_the_owner_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [owner] = ids();
    db.earn_shells(owner, 100, "test").await.unwrap();
    let pet = db.adopt(owner, "cat", "Tom").await.unwrap();
    let day = (Utc::now().timestamp() / 86_400) as i32;

    let pet_id = pet.id;
    let (woke, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM pets WHERE id = $1 FOR UPDATE",
        pet_id,
        owner,
        move |db| async move {
            db.wake_pet(owner, pet_id, pet.needs_at, [90, 90, 90, 90], Utc::now(), day, 10)
                .await
        },
    )
    .await;
    let woke = woke.expect("the wake-up fails nowhere");
    deleted.expect("the delete fails nowhere");
    assert!(matches!(woke, Wake::Woke { free: true, .. }), "{woke:?}");
    assert!(
        !db.free_wake_ready(owner, day).await.unwrap(),
        "the day's free wake-up stays used after the delete"
    );
    // Back with a new row: the trigger gives the used wake-up back with it.
    assert_eq!(db.user(owner).await.unwrap().credits, 0);
    let (used,): (Option<i32>,) = sqlx::query_as("SELECT pet_wake_day FROM users WHERE id = $1")
        .bind(owner as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(used, Some(day));
    assert!(!db.free_wake_ready(owner, day).await.unwrap());
    db.delete_user_data(owner, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn buying_from_a_stall_while_the_seller_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [seller, buyer] = ids();
    db.earn_shells(seller, 5, "test").await.unwrap();
    db.earn_shells(buyer, 100, "test").await.unwrap();
    db.grant_item(seller, "chair", 1).await.unwrap();
    assert!(db.list_item(seller, "chair", 40).await.unwrap());
    let listing = db.listings(seller).await.unwrap()[0].id;

    let (bought, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM stall_listings WHERE id = $1 FOR UPDATE",
        listing,
        seller,
        move |db| async move { db.buy_listing(buyer, listing).await },
    )
    .await;
    let bought = bought.expect("the purchase fails nowhere");
    deleted.expect("the delete fails nowhere");
    assert!(matches!(bought, StallBuy::Bought { balance: 60, .. }), "{bought:?}");
    assert_eq!((credits(&db, buyer).await, ledger(&db, buyer).await), (60, 60));
    assert_eq!(credits(&db, seller).await, 0);
    db.delete_user_data(buyer, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn a_like_landing_while_the_delete_starts_cannot_cross_a_pair_call() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(6).await else { return };
    // `ids` count upwards: the host's lock comes before the deleter's in id order.
    let [host, deleter_id] = ids();
    let today = Utc::now().date_naive();
    sqlx::query("INSERT INTO houses (user_id) VALUES ($1)")
        .bind(host as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.earn_shells(host, 1, "test").await.unwrap();
    db.earn_shells(deleter_id, 1, "test").await.unwrap();

    // The like is given in a transaction that holds the deleter's lock, as a like does: the delete's
    // first read cannot see it, and its lock waits.
    let mut like = db.pool().begin().await.unwrap();
    sqlx::query("SELECT lock_user($1)")
        .bind(deleter_id as i64)
        .execute(&mut *like)
        .await
        .unwrap();
    sqlx::query("INSERT INTO house_likes (host_id, liker_id, day) VALUES ($1, $2, $3)")
        .bind(host as i64)
        .bind(deleter_id as i64)
        .bind(today)
        .execute(&mut *like)
        .await
        .unwrap();
    let ((deleter, deleter_pid), (friend, friend_pid)) = (side().await, side().await);
    let delete = tokio::spawn(async move { deleter.delete_user_data(deleter_id, Utc::now()).await });
    queued(&db, deleter_pid, "the delete").await;
    // A call about both players: takes the host's lock and waits for the deleter's.
    let befriend = tokio::spawn(async move { friend.befriend(host, deleter_id).await });
    queued(&db, friend_pid, "the pair call").await;
    let before = pinchora_db::conflict_retries();
    like.commit().await.unwrap();

    befriend.await.unwrap().expect("the pair call fails nowhere");
    delete.await.unwrap().expect("the delete fails nowhere");
    assert!(
        pinchora_db::conflict_retries() > before,
        "the delete found the host too late to wait for, and started over"
    );
    for user in [host, deleter_id] {
        db.delete_user_data(user, Utc::now()).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn friends_who_delete_their_data_at_once_both_get_through() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(8).await else { return };
    let day = Utc::now().date_naive();
    for _ in 0..4 {
        let [a, b] = ids();
        db.earn_shells(a, 10, "test").await.unwrap();
        db.earn_shells(b, 10, "test").await.unwrap();
        db.befriend(a, b).await.unwrap();
        // Rows both ways, so each delete removes what the other one removes from the other end.
        db.record_visit(a, b, day, 1, 3).await.unwrap();
        db.record_visit(b, a, day, 1, 3).await.unwrap();
        db.record_interaction("hug", a, b).await.unwrap();
        db.record_interaction("hug", b, a).await.unwrap();
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let deletes: Vec<_> = [a, b]
            .into_iter()
            .map(|user| {
                let (db, barrier) = (db.clone(), barrier.clone());
                tokio::spawn(async move {
                    barrier.wait().await;
                    db.delete_user_data(user, Utc::now()).await
                })
            })
            .collect();
        for delete in deletes {
            delete.await.unwrap().expect("a delete fails nowhere");
        }
        assert!(!db.are_friends(a, b).await.unwrap());
    }
}

#[tokio::test]
async fn rerolling_a_giveaway_while_an_entrant_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [host, first, leaving, last, guild] = ids();
    let GiveawayOpen::Started(giveaway) = db
        .create_giveaway(NewGiveaway {
            guild_id: guild,
            channel_id: 1,
            host,
            prize: "A role",
            shells: 0,
            winners: 1,
            required_role: None,
            ends_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("the giveaway starts");
    };
    let id = giveaway.id;
    for user in [first, leaving, last] {
        db.toggle_giveaway_entry(id, user, Utc::now()).await.unwrap();
    }
    db.end_giveaway(id, Utc::now(), true, |entrants, _| {
        entrants.iter().copied().filter(|e| *e == first).collect()
    })
    .await
    .unwrap()
    .expect("it was running");

    // One entrant's delete is under way: their entry is still there for anybody who reads it.
    let mut delete = db.pool().begin().await.unwrap();
    Db::delete_user_data_in(&mut delete, leaving, Utc::now()).await.unwrap();
    let (roller, roller_pid) = side().await;
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = asked.clone();
    let reroll = tokio::spawn(async move {
        roller
            .reroll_giveaway(guild, id, 1, move |entrants, _| {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                // Always the one who is leaving, while there is one.
                if entrants.contains(&leaving) {
                    vec![leaving]
                } else {
                    entrants.to_vec()
                }
            })
            .await
    });
    queued(&db, roller_pid, "the reroll").await;
    delete.commit().await.unwrap();

    let Reroll::Done(row, picked) = reroll.await.unwrap().expect("the reroll fails nowhere") else {
        panic!("somebody is left to win");
    };
    assert_eq!(picked, vec![last], "the pick started over without the deleted entrant");
    assert!(asked.load(std::sync::atomic::Ordering::SeqCst) >= 2);
    assert!(!row.winner_ids.contains(&(leaving as i64)));
    let (stored,): (Vec<i64>,) = sqlx::query_as("SELECT winner_ids FROM giveaways WHERE id = $1")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(
        stored,
        vec![first as i64, last as i64],
        "nothing of the deleted player is stored"
    );
    for user in [host, first, last] {
        db.delete_user_data(user, Utc::now()).await.unwrap();
    }
}

#[tokio::test]
async fn ending_a_giveaway_without_shells_while_an_entrant_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [host, first, leaving, guild] = ids();
    let GiveawayOpen::Started(giveaway) = db
        .create_giveaway(NewGiveaway {
            guild_id: guild,
            channel_id: 1,
            host,
            prize: "A role",
            shells: 0,
            winners: 1,
            required_role: None,
            ends_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("the giveaway starts");
    };
    let id = giveaway.id;
    for user in [first, leaving] {
        db.toggle_giveaway_entry(id, user, Utc::now()).await.unwrap();
    }

    let mut delete = db.pool().begin().await.unwrap();
    Db::delete_user_data_in(&mut delete, leaving, Utc::now()).await.unwrap();
    let (ender, ender_pid) = side().await;
    let end = tokio::spawn(async move {
        ender
            .end_giveaway(id, Utc::now(), true, move |entrants, _| {
                // Always the one who is leaving, while there is one.
                if entrants.contains(&leaving) {
                    vec![leaving]
                } else {
                    entrants.to_vec()
                }
            })
            .await
    });
    queued(&db, ender_pid, "the end").await;
    delete.commit().await.unwrap();

    let ended = end
        .await
        .unwrap()
        .expect("the end fails nowhere")
        .expect("it was running");
    assert_eq!(
        ended.winners,
        vec![first],
        "the pick started over without the deleted entrant"
    );
    let (stored,): (Vec<i64>,) = sqlx::query_as("SELECT winner_ids FROM giveaways WHERE id = $1")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(stored, vec![first as i64], "nothing of the deleted player is stored");
    for user in [host, first] {
        db.delete_user_data(user, Utc::now()).await.unwrap();
    }
}
