//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.

use pinchora_db::{DailyOutcome, Db, RepOutcome, TransferOutcome};

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect(&url, 5).await.expect("connect");
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

#[tokio::test]
async fn daily_transfer_and_rep() {
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();

    let DailyOutcome::Claimed {
        amount,
        streak,
        balance,
    } = db.claim_daily(alice).await.unwrap()
    else {
        panic!("first daily must succeed");
    };
    assert_eq!((amount, streak, balance), (250, 1, 250));
    assert!(matches!(
        db.claim_daily(alice).await.unwrap(),
        DailyOutcome::Cooldown { .. }
    ));

    assert!(matches!(
        db.transfer_credits(alice, bob, 1_000).await.unwrap(),
        TransferOutcome::Insufficient { balance: 250 }
    ));
    assert!(matches!(
        db.transfer_credits(alice, bob, 100).await.unwrap(),
        TransferOutcome::Done { sender_balance: 150 }
    ));
    assert_eq!(db.user(bob).await.unwrap().credits, 100);
    assert!(db.transfer_credits(alice, alice, 1).await.is_err());

    assert!(matches!(
        db.give_rep(alice, bob).await.unwrap(),
        RepOutcome::Given { target_total: 1 }
    ));
    assert!(matches!(
        db.give_rep(alice, bob).await.unwrap(),
        RepOutcome::Cooldown { .. }
    ));
    assert!(db.give_rep(bob, bob).await.is_err());

    let (ledger,): (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM credit_ledger WHERE user_id = ANY($1)")
            .bind(vec![alice as i64, bob as i64])
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(ledger, 250, "ledger must balance: one daily, transfers net to zero");
}

#[tokio::test]
async fn concurrent_transfers_never_overdraw() {
    let Some(db) = db().await else { return };
    let [a, b] = ids();
    db.claim_daily(a).await.unwrap(); // a: 250
    db.claim_daily(b).await.unwrap(); // b: 250
    let mut tasks = Vec::new();
    for i in 0..20 {
        let db = db.clone();
        let (from, to) = if i % 2 == 0 { (a, b) } else { (b, a) };
        tasks.push(tokio::spawn(
            async move { db.transfer_credits(from, to, 40).await.unwrap() },
        ));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let total = db.user(a).await.unwrap().credits + db.user(b).await.unwrap().credits;
    assert_eq!(total, 500);
}

#[tokio::test]
async fn xp_ranks_and_settings() {
    let Some(db) = db().await else { return };
    let [guild, u1, u2, u3] = ids();

    db.add_message_xp(guild, u1, 20).await.unwrap();
    let gain = db.add_message_xp(guild, u1, 20).await.unwrap();
    assert_eq!((gain.old_xp, gain.new_xp), (20, 40));
    db.add_message_xp(guild, u2, 100).await.unwrap();
    db.add_message_xp(guild, u3, 5).await.unwrap();

    let r1 = db.member_rank(guild, u1).await.unwrap().unwrap();
    assert_eq!((r1.xp, r1.messages, r1.rank), (40, 2, 2));
    assert!(db.member_rank(guild, 1).await.unwrap().is_none());
    assert_eq!(db.guild_member_count(guild).await.unwrap(), 3);

    let board = db.guild_leaderboard(guild, 10, 0).await.unwrap();
    let order: Vec<i64> = board.iter().map(|r| r.user_id).collect();
    assert_eq!(order, vec![u2 as i64, u1 as i64, u3 as i64]);
    assert_eq!(db.user(u1).await.unwrap().xp, 40, "global XP follows guild XP");

    let s = db.guild_settings(guild).await.unwrap();
    assert!(s.leveling_enabled && !s.welcome_enabled);
    let s = db
        .update_welcome(guild, Some(true), Some(123), Some("hi {user}"))
        .await
        .unwrap();
    assert!(s.welcome_enabled);
    assert_eq!(s.welcome_channel_id, Some(123));
    let s = db.update_welcome(guild, None, None, None).await.unwrap();
    assert_eq!(
        s.welcome_message.as_deref(),
        Some("hi {user}"),
        "unset options keep old values"
    );
    let s = db.update_leveling(guild, Some(false), None, None).await.unwrap();
    assert!(!s.leveling_enabled);
}

#[tokio::test]
async fn player_stalls() {
    let Some(db) = db().await else { return };
    let [seller, buyer] = ids();
    db.grant_item(seller, "test_item", 1).await.unwrap();
    assert!(db.list_item(seller, "test_item", 1_000).await.unwrap());
    assert!(
        !db.list_item(seller, "test_item", 1_000).await.unwrap(),
        "only one was owned"
    );
    let listing = db.listings(seller).await.unwrap()[0].id;

    assert!(matches!(
        db.buy_listing(seller, listing).await.unwrap(),
        pinchora_db::StallBuy::OwnListing
    ));
    assert!(matches!(
        db.buy_listing(buyer, listing).await.unwrap(),
        pinchora_db::StallBuy::Insufficient { .. }
    ));
    assert_eq!(
        db.listings(seller).await.unwrap().len(),
        1,
        "failed purchase keeps the listing"
    );

    db.claim_daily(buyer).await.unwrap(); // 250
    db.transfer_credits(buyer, seller, 1).await.unwrap(); // seller 1, buyer 249
    assert!(db.unlist_item(seller, listing).await.unwrap().is_some());
    assert!(db.list_item(seller, "test_item", 200).await.unwrap());
    let listing = db.listings(seller).await.unwrap()[0].id;
    let pinchora_db::StallBuy::Bought { balance, .. } = db.buy_listing(buyer, listing).await.unwrap() else {
        panic!("purchase should succeed");
    };
    assert_eq!(balance, 49);
    assert_eq!(db.user(seller).await.unwrap().credits, 1 + 190, "5% fee taken");
    assert!(db.owns(buyer, "test_item").await.unwrap());
    assert!(matches!(
        db.buy_listing(buyer, listing).await.unwrap(),
        pinchora_db::StallBuy::Gone
    ));
}

#[tokio::test]
async fn market_never_oversells_under_contention() {
    let Some(db) = db().await else { return };
    let buyers: [u64; 20] = ids();
    let hour = -(buyers[0] as i64); // a unique fake hour for this run
    db.market(hour, &[("legendary_test".into(), 100, 3)]).await.unwrap();
    for &b in &buyers {
        db.claim_daily(b).await.unwrap(); // 250 shells each
    }
    let tasks: Vec<_> = buyers
        .iter()
        .map(|&b| {
            let db = db.clone();
            tokio::spawn(async move { db.market_buy(b, hour, "legendary_test").await.unwrap() })
        })
        .collect();
    let mut bought = 0;
    for t in tasks {
        if matches!(t.await.unwrap(), pinchora_db::BuyOutcome::Bought { .. }) {
            bought += 1;
        }
    }
    assert_eq!(bought, 3, "stock of 3 sells exactly 3");
    let row = &db.market(hour, &[]).await.unwrap()[0];
    assert_eq!((row.sold, row.stock), (3, 3));
}

#[tokio::test]
async fn place_and_pick_up_round_trip() {
    let Some(db) = db().await else { return };
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
async fn characters_are_saved_per_player() {
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();

    assert_eq!(db.character(alice).await.unwrap(), None, "unknown player");
    assert_eq!(
        db.user(alice).await.unwrap().character,
        None,
        "no character designed yet"
    );
    assert_eq!(db.character(alice).await.unwrap(), None);

    db.set_character(alice, "tidewalker.1.1.0.2").await.unwrap();
    db.set_character(bob, "bunhaven.7.11.5.9").await.unwrap();
    assert_eq!(
        db.character(alice).await.unwrap().as_deref(),
        Some("tidewalker.1.1.0.2")
    );
    assert_eq!(
        db.user(bob).await.unwrap().character.as_deref(),
        Some("bunhaven.7.11.5.9")
    );

    db.set_character(alice, "curlcove.0.0.0.0").await.unwrap();
    assert_eq!(db.character(alice).await.unwrap().as_deref(), Some("curlcove.0.0.0.0"));
    assert_eq!(
        db.character(bob).await.unwrap().as_deref(),
        Some("bunhaven.7.11.5.9"),
        "other players are untouched"
    );
}
