use crate::db::*;

#[tokio::test]
async fn daily_transfer_and_rep() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
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
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
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
async fn player_stalls() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
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
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
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
async fn player_stats_collect_every_number_for_achievements() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let [nobody, host, fan_a, fan_b] = ids();
    assert_eq!(db.player_stats(nobody).await.unwrap(), PlayerStats::default());

    // 255 XP is exactly level 2; the streak is alive after a daily.
    db.claim_daily(host).await.unwrap();
    sqlx::query("UPDATE users SET xp = 255, reputation = 7 WHERE id = $1")
        .bind(host as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.log_catch(host, "minnow", 10.0).await.unwrap();
    db.log_catch(host, "minnow", 12.0).await.unwrap();
    db.log_catch(host, "trout", 30.0).await.unwrap();
    db.adopt(host, "crumbclaw_crab", "Sir Clicks").await.unwrap();
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    db.house(host, &starter).await.unwrap();
    db.grant_item(host, "chair", 2).await.unwrap();
    for x in [1, 3] {
        db.place_item(host, 0, "chair", x, 1, false, |_| Ok(())).await.unwrap();
    }
    let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
    for fan in [fan_a, fan_b] {
        db.like_house(host, fan, day).await.unwrap();
    }

    assert_eq!(
        db.player_stats(host).await.unwrap(),
        PlayerStats {
            level: 2,
            fish_species: 2,
            house_likes: 2,
            pets: 1,
            streak: 1,
            placed: 2,
            reputation: 7,
            shells: 250,
        }
    );

    // A streak nobody extended in time counts as none, as on the profile card.
    sqlx::query("UPDATE users SET last_daily_at = now() - interval '3 days' WHERE id = $1")
        .bind(host as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(db.player_stats(host).await.unwrap().streak, 0);
}
