//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.

use pinchora_db::{
    CustomizeBuy, DailyOutcome, Db, LogKind, MAX_BADGES, MAX_PANEL_ROLES, PlayerStats, RepOutcome, TransferOutcome,
};

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

    db.add_message_xp(guild, u1, 20, 0).await.unwrap();
    let gain = db.add_message_xp(guild, u1, 20, 0).await.unwrap();
    assert_eq!((gain.old_xp, gain.new_xp), (20, 40));
    db.add_message_xp(guild, u2, 100, 0).await.unwrap();
    db.add_message_xp(guild, u3, 5, 0).await.unwrap();

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

#[tokio::test]
async fn server_points_accrue_give_and_take() {
    let Some(db) = db().await else { return };
    let [guild, chatter, helper, bystander] = ids();

    let s = db.guild_settings(guild).await.unwrap();
    assert!(s.points_enabled && s.level_roles_stack);
    assert_eq!(
        (
            s.points_name.as_str(),
            s.points_emoji.as_str(),
            s.points_min,
            s.points_max
        ),
        ("points", "⭐", 1, 3)
    );

    // Chat earns XP and points in one query; a second message adds to both.
    db.add_message_xp(guild, chatter, 20, 2).await.unwrap();
    let gain = db.add_message_xp(guild, chatter, 20, 3).await.unwrap();
    assert_eq!((gain.old_xp, gain.new_xp), (20, 40));
    assert_eq!(db.member_points(guild, chatter).await.unwrap(), (5, Some(1)));

    // Giving creates the member row; the points board ranks by points, the XP board by XP.
    assert_eq!(db.give_points(guild, helper, 9).await.unwrap(), 9);
    assert_eq!(db.give_points(guild, helper, 1).await.unwrap(), 10);
    assert_eq!(db.member_points(guild, chatter).await.unwrap(), (5, Some(2)));
    assert_eq!(db.member_points(guild, bystander).await.unwrap(), (0, None));
    let points: Vec<_> = db
        .guild_points_leaderboard(guild, 10, 0)
        .await
        .unwrap()
        .iter()
        .map(|r| (r.user_id, r.value))
        .collect();
    assert_eq!(points, vec![(helper as i64, 10), (chatter as i64, 5)]);
    let xp: Vec<i64> = db
        .guild_leaderboard(guild, 10, 0)
        .await
        .unwrap()
        .iter()
        .map(|r| r.user_id)
        .collect();
    assert_eq!(xp, vec![chatter as i64], "a member who only got points has no XP rank");
    assert_eq!(db.guild_member_count(guild).await.unwrap(), 1);
    let recent: Vec<i64> = db
        .recent_xp_members(guild, 10)
        .await
        .unwrap()
        .iter()
        .map(|r| r.user_id)
        .collect();
    assert_eq!(recent, vec![chatter as i64]);

    // Taking stops at zero and says how much it really took.
    assert_eq!(db.take_points(guild, helper, 4).await.unwrap(), (4, 6));
    assert_eq!(db.take_points(guild, helper, 100).await.unwrap(), (6, 0));
    assert_eq!(db.take_points(guild, helper, 1).await.unwrap(), (0, 0));
    assert_eq!(
        db.take_points(guild, bystander, 1).await.unwrap(),
        (0, 0),
        "no row at all"
    );
    assert_eq!(db.member_points(guild, helper).await.unwrap(), (0, None));
    assert!(db.give_points(guild, helper, 0).await.is_err());
    assert!(db.take_points(guild, helper, -1).await.is_err());

    // Settings keep unset options and refuse an upside-down range.
    let s = db
        .update_points(guild, Some(false), Some("coins"), Some("🪙"), Some(2), Some(5))
        .await
        .unwrap();
    assert_eq!(
        (
            s.points_enabled,
            s.points_name.as_str(),
            s.points_emoji.as_str(),
            s.points_min,
            s.points_max
        ),
        (false, "coins", "🪙", 2, 5)
    );
    let s = db.update_points(guild, None, None, None, None, Some(9)).await.unwrap();
    assert_eq!((s.points_name.as_str(), s.points_min, s.points_max), ("coins", 2, 9));
    assert!(db.update_points(guild, None, None, None, Some(10), None).await.is_err());
}

#[tokio::test]
async fn level_roles_are_capped_and_replaceable() {
    let Some(db) = db().await else { return };
    let [guild, other] = ids();

    assert!(db.level_roles(guild).await.unwrap().is_empty());
    assert!(db.add_level_role(guild, 10, 1_000).await.unwrap());
    assert!(db.add_level_role(guild, 5, 1_001).await.unwrap());
    assert!(
        db.add_level_role(guild, 5, 1_002).await.unwrap(),
        "roles can share a level"
    );
    let rows: Vec<_> = db
        .level_roles(guild)
        .await
        .unwrap()
        .iter()
        .map(|r| (r.level, r.role_id))
        .collect();
    assert_eq!(rows, vec![(5, 1_001), (5, 1_002), (10, 1_000)]);

    // Adding a role again moves it to the new level.
    assert!(db.add_level_role(guild, 20, 1_000).await.unwrap());
    assert_eq!(db.level_roles(guild).await.unwrap().last().unwrap().level, 20);

    // At the cap only new roles are refused; existing ones can still be moved.
    for role in 0..(pinchora_db::MAX_LEVEL_ROLES - 3) as u64 {
        assert!(db.add_level_role(guild, 1, 2_000 + role).await.unwrap());
    }
    assert_eq!(
        db.level_roles(guild).await.unwrap().len() as i64,
        pinchora_db::MAX_LEVEL_ROLES
    );
    assert!(!db.add_level_role(guild, 1, 9_999).await.unwrap());
    assert!(db.add_level_role(guild, 30, 1_000).await.unwrap());
    assert!(
        db.add_level_role(other, 1, 9_999).await.unwrap(),
        "the cap is per server"
    );

    assert!(db.remove_level_role(guild, 1_001).await.unwrap());
    assert!(!db.remove_level_role(guild, 1_001).await.unwrap());

    // A role deleted on Discord leaves both the rewards and the store.
    db.add_store_item(guild, 1_002, 50, None).await.unwrap().unwrap();
    db.forget_role(guild, 1_002).await.unwrap();
    assert!(db.level_roles(guild).await.unwrap().iter().all(|r| r.role_id != 1_002));
    assert!(db.store_items(guild).await.unwrap().is_empty());

    assert!(db.guild_settings(guild).await.unwrap().level_roles_stack);
    assert!(!db.set_level_roles_stack(guild, false).await.unwrap().level_roles_stack);
    assert!(db.set_level_roles_stack(guild, true).await.unwrap().level_roles_stack);
}

#[tokio::test]
async fn store_purchase_checks_balance_and_stock() {
    use pinchora_db::StoreBuy;

    let Some(db) = db().await else { return };
    let [guild, buyer, poor] = ids();
    db.give_points(guild, buyer, 100).await.unwrap();

    let unlimited = db.add_store_item(guild, 7_001, 30, None).await.unwrap().unwrap();
    let scarce = db.add_store_item(guild, 7_002, 40, Some(1)).await.unwrap().unwrap();
    let ids_by_price: Vec<i64> = db.store_items(guild).await.unwrap().iter().map(|i| i.id).collect();
    assert_eq!(ids_by_price, vec![unlimited.id, scarce.id], "cheapest first");

    // Too poor: nothing moves.
    assert_eq!(
        db.buy_store_item(guild, poor, unlimited.id).await.unwrap(),
        StoreBuy::Insufficient { price: 30, balance: 0 }
    );

    assert_eq!(
        db.buy_store_item(guild, buyer, unlimited.id).await.unwrap(),
        StoreBuy::Bought {
            role_id: 7_001,
            price: 30,
            balance: 70
        }
    );
    assert_eq!(db.store_item(guild, unlimited.id).await.unwrap().unwrap().stock, None);

    // The last unit sells once; the next buyer finds it sold out and keeps their points.
    assert!(matches!(
        db.buy_store_item(guild, buyer, scarce.id).await.unwrap(),
        StoreBuy::Bought { balance: 30, .. }
    ));
    assert_eq!(
        db.buy_store_item(guild, buyer, scarce.id).await.unwrap(),
        StoreBuy::SoldOut
    );
    assert_eq!(db.member_points(guild, buyer).await.unwrap().0, 30);

    // A refund gives back the points and the unit.
    assert_eq!(db.refund_store_purchase(guild, buyer, scarce.id, 40).await.unwrap(), 70);
    assert_eq!(db.store_item(guild, scarce.id).await.unwrap().unwrap().stock, Some(1));
    assert_eq!(
        db.refund_store_purchase(guild, buyer, unlimited.id, 30).await.unwrap(),
        100,
        "unlimited stock stays unlimited"
    );
    assert_eq!(db.store_item(guild, unlimited.id).await.unwrap().unwrap().stock, None);

    // Re-adding a role updates it in place and keeps its id.
    let again = db.add_store_item(guild, 7_002, 10, Some(5)).await.unwrap().unwrap();
    assert_eq!((again.id, again.price, again.stock), (scarce.id, 10, Some(5)));

    assert!(db.remove_store_item(guild, 7_002).await.unwrap());
    assert!(!db.remove_store_item(guild, 7_002).await.unwrap());
    assert_eq!(
        db.buy_store_item(guild, buyer, scarce.id).await.unwrap(),
        StoreBuy::Gone
    );
}

#[tokio::test]
async fn store_never_oversells_or_overspends_under_contention() {
    use pinchora_db::StoreBuy;

    let Some(db) = db().await else { return };
    let guild = ids::<1>()[0];
    let buyers: [u64; 20] = ids();
    for &b in &buyers {
        db.give_points(guild, b, 100).await.unwrap();
    }

    // Twenty buyers, three units.
    let scarce = db.add_store_item(guild, 8_001, 40, Some(3)).await.unwrap().unwrap();
    let tasks: Vec<_> = buyers
        .iter()
        .map(|&b| {
            let db = db.clone();
            tokio::spawn(async move { db.buy_store_item(guild, b, scarce.id).await.unwrap() })
        })
        .collect();
    let mut bought = 0;
    for t in tasks {
        if matches!(t.await.unwrap(), StoreBuy::Bought { .. }) {
            bought += 1;
        }
    }
    assert_eq!(bought, 3, "stock of 3 sells exactly 3");
    assert_eq!(db.store_item(guild, scarce.id).await.unwrap().unwrap().stock, Some(0));
    let spent: i64 = {
        let mut total = 0;
        for &b in &buyers {
            total += 100 - db.member_points(guild, b).await.unwrap().0;
        }
        total
    };
    assert_eq!(spent, 3 * 40, "only the three buyers paid");

    // One buyer with 100 points clicks a 60-point item ten times at once.
    let [solo]: [u64; 1] = ids();
    db.give_points(guild, solo, 100).await.unwrap();
    let plenty = db.add_store_item(guild, 8_002, 60, None).await.unwrap().unwrap();
    let tasks: Vec<_> = (0..10)
        .map(|_| {
            let db = db.clone();
            tokio::spawn(async move { db.buy_store_item(guild, solo, plenty.id).await.unwrap() })
        })
        .collect();
    let mut bought = 0;
    for t in tasks {
        if matches!(t.await.unwrap(), StoreBuy::Bought { .. }) {
            bought += 1;
        }
    }
    assert_eq!(bought, 1, "100 points afford one 60-point item");
    assert_eq!(db.member_points(guild, solo).await.unwrap().0, 40);
}

#[tokio::test]
async fn store_is_capped_per_server() {
    let Some(db) = db().await else { return };
    let [guild] = ids();
    for role in 0..pinchora_db::MAX_STORE_ITEMS as u64 {
        assert!(db.add_store_item(guild, 6_000 + role, 5, None).await.unwrap().is_some());
    }
    assert!(db.add_store_item(guild, 6_999, 5, None).await.unwrap().is_none());
    assert!(
        db.add_store_item(guild, 6_000, 99, Some(2)).await.unwrap().is_some(),
        "an existing role can still be updated"
    );
}

#[tokio::test]
async fn backgrounds_are_bought_once_and_equipped_only_when_owned() {
    let Some(db) = db().await else { return };
    let [alice] = ids();

    assert_eq!(
        db.buy_background(alice, "dune", 500).await.unwrap(),
        CustomizeBuy::Insufficient { balance: 0 },
        "a new player has no shells"
    );
    assert!(
        db.owned_backgrounds(alice).await.unwrap().is_empty(),
        "a refused buy owns nothing"
    );
    assert!(
        !db.equip_background(alice, "dune").await.unwrap(),
        "not owned, not worn"
    );
    assert!(
        db.buy_background(alice, "dune", 0).await.is_err(),
        "a purchase has a price"
    );

    db.earn_shells(alice, 1_000, "test").await.unwrap();
    assert_eq!(
        db.buy_background(alice, "dune", 500).await.unwrap(),
        CustomizeBuy::Bought { balance: 500 }
    );
    assert_eq!(
        db.buy_background(alice, "dune", 500).await.unwrap(),
        CustomizeBuy::AlreadyOwned
    );
    assert_eq!(db.user(alice).await.unwrap().credits, 500, "a duplicate costs nothing");
    assert_eq!(
        db.buy_background(alice, "reef", 600).await.unwrap(),
        CustomizeBuy::Insufficient { balance: 500 }
    );
    assert_eq!(db.owned_backgrounds(alice).await.unwrap(), vec!["dune"]);

    assert_eq!(db.user(alice).await.unwrap().background, None);
    assert!(db.equip_background(alice, "dune").await.unwrap());
    assert_eq!(db.user(alice).await.unwrap().background.as_deref(), Some("dune"));
    assert!(
        !db.equip_background(alice, "reef").await.unwrap(),
        "wearing one does not unlock another"
    );
    assert_eq!(db.user(alice).await.unwrap().background.as_deref(), Some("dune"));
    db.remove_background(alice).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().background, None);

    let (net, spent): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT, COALESCE(SUM(delta) FILTER (WHERE reason = 'background'), 0)::BIGINT
         FROM credit_ledger WHERE user_id = $1",
    )
    .bind(alice as i64)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!((net, spent), (500, -500), "the ledger shows the one purchase");
}

#[tokio::test]
async fn badges_are_bought_once_and_worn_in_order() {
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();
    db.earn_shells(alice, 1_000, "test").await.unwrap();

    // Ten clicks at once buy the badge exactly once.
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.buy_badge(alice, "tiny_tide", 100).await.unwrap()
        }));
    }
    let mut outcomes = Vec::new();
    for t in tasks {
        outcomes.push(t.await.unwrap());
    }
    let bought = outcomes
        .iter()
        .filter(|o| matches!(o, CustomizeBuy::Bought { .. }))
        .count();
    assert_eq!(bought, 1, "{outcomes:?}");
    assert!(outcomes.iter().all(|o| !matches!(o, CustomizeBuy::Insufficient { .. })));
    assert_eq!(db.user(alice).await.unwrap().credits, 900);
    assert_eq!(db.owned_badges(alice).await.unwrap(), vec!["tiny_tide"]);
    assert_eq!(
        db.buy_badge(bob, "tiny_tide", 100).await.unwrap(),
        CustomizeBuy::Insufficient { balance: 0 },
        "owning is per player"
    );
    assert!(db.owned_badges(bob).await.unwrap().is_empty());

    let worn = |ids: &[&str]| ids.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert!(db.user(alice).await.unwrap().badges.is_empty());
    db.set_badges(alice, &worn(&["tiny_tide", "fin_finder", "pawshake"]))
        .await
        .unwrap();
    assert_eq!(
        db.user(alice).await.unwrap().badges,
        worn(&["tiny_tide", "fin_finder", "pawshake"]),
        "the order is kept"
    );
    db.set_badges(alice, &worn(&["pawshake", "tiny_tide"])).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().badges, worn(&["pawshake", "tiny_tide"]));

    let seven = worn(&["a", "b", "c", "d", "e", "f", "g"]);
    assert_eq!(MAX_BADGES, 6);
    assert!(db.set_badges(alice, &seven).await.is_err(), "at most six");
    assert!(db.set_badges(alice, &worn(&["a", "a"])).await.is_err(), "no repeats");
    assert_eq!(
        db.user(alice).await.unwrap().badges,
        worn(&["pawshake", "tiny_tide"]),
        "refused saves change nothing"
    );
    db.set_badges(alice, &seven[..6]).await.unwrap();
    db.set_badges(alice, &[]).await.unwrap();
    assert!(db.user(alice).await.unwrap().badges.is_empty());
}

#[tokio::test]
async fn player_stats_collect_every_number_for_achievements() {
    let Some(db) = db().await else { return };
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

#[tokio::test]
async fn join_role_goodbye_and_log_settings() {
    let Some(db) = db().await else { return };
    let [guild, other] = ids();

    // Everything starts off, and a log channel alone switches every kind of event on.
    let fresh = db.guild_settings(guild).await.unwrap();
    assert!(fresh.join_role_id.is_none() && fresh.goodbye_channel_id.is_none() && fresh.log_channel_id.is_none());
    assert!(fresh.log_members && fresh.log_names && fresh.log_bans && fresh.log_voice);
    assert_eq!(fresh.log_channel(LogKind::Voice), None);

    assert_eq!(db.set_join_role(guild, Some(77)).await.unwrap().join_role_id, Some(77));
    assert_eq!(db.set_join_role(guild, None).await.unwrap().join_role_id, None);

    // The message survives turning goodbyes off.
    let on = db.update_goodbye(guild, Some(5), Some("bye {user}")).await.unwrap();
    assert_eq!(
        (on.goodbye_channel_id, on.goodbye_message.as_deref()),
        (Some(5), Some("bye {user}"))
    );
    let kept = db.update_goodbye(guild, None, None).await.unwrap();
    assert_eq!(kept.goodbye_channel_id, Some(5), "nothing given, nothing changed");
    let off = db.clear_goodbye(guild).await.unwrap();
    assert_eq!(
        (off.goodbye_channel_id, off.goodbye_message.as_deref()),
        (None, Some("bye {user}"))
    );

    let logged = db.set_log_channel(guild, Some(9)).await.unwrap();
    assert_eq!(logged.log_channel(LogKind::Bans), Some(9));
    let some = db
        .update_log_events(guild, Some(false), None, None, Some(false))
        .await
        .unwrap();
    assert_eq!(some.log_channel(LogKind::Members), None);
    assert_eq!(some.log_channel(LogKind::Names), Some(9));
    assert_eq!(some.log_channel(LogKind::Voice), None);
    assert_eq!(
        db.set_log_channel(guild, None)
            .await
            .unwrap()
            .log_channel(LogKind::Names),
        None
    );

    // Another server is untouched.
    assert!(db.guild_settings(other).await.unwrap().join_role_id.is_none());

    // A role deleted on Discord stops being the join role.
    db.set_join_role(guild, Some(88)).await.unwrap();
    db.forget_role(guild, 99).await.unwrap();
    assert_eq!(db.guild_settings(guild).await.unwrap().join_role_id, Some(88));
    db.forget_role(guild, 88).await.unwrap();
    assert_eq!(db.guild_settings(guild).await.unwrap().join_role_id, None);
}

#[tokio::test]
async fn role_panels_belong_to_their_server_and_keep_order() {
    let Some(db) = db().await else { return };
    let [guild, other] = ids();

    // A panel is found by title (any case) or number, only in its own server.
    let (panel, created) = db.save_panel(guild, "Colours", Some("Pick one")).await.unwrap();
    assert!(created && panel.channel_id.is_none() && panel.message_id.is_none());
    let (again, created) = db.save_panel(guild, "colours", None).await.unwrap();
    assert!(!created && again.id == panel.id, "same title, same panel");
    assert_eq!(
        again.description.as_deref(),
        Some("Pick one"),
        "no description given, none lost"
    );
    let (updated, _) = db.save_panel(guild, "COLOURS", Some("Pick two")).await.unwrap();
    assert_eq!(updated.description.as_deref(), Some("Pick two"));
    assert_eq!(updated.title, "Colours", "the first title stays");
    assert_eq!(db.find_panel(guild, "COLOURS").await.unwrap().unwrap().id, panel.id);
    assert_eq!(
        db.find_panel(guild, &format!("#{}", panel.id))
            .await
            .unwrap()
            .unwrap()
            .id,
        panel.id
    );
    assert!(db.find_panel(other, "Colours").await.unwrap().is_none());
    assert!(db.find_panel(other, &panel.id.to_string()).await.unwrap().is_none());
    let (theirs, created) = db.save_panel(other, "Colours", None).await.unwrap();
    assert!(created && theirs.id != panel.id, "titles only clash within a server");

    // Buttons keep the order they were added in; adding again changes the label in place.
    assert!(db.add_panel_role(panel.id, 301, "Red", Some("🔴")).await.unwrap());
    assert!(db.add_panel_role(panel.id, 300, "Blue", None).await.unwrap());
    assert!(db.add_panel_role(panel.id, 301, "Crimson", None).await.unwrap());
    let roles: Vec<_> = db
        .panel_roles(panel.id)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.role_id, r.label, r.emoji))
        .collect();
    assert_eq!(
        roles,
        vec![(301, "Crimson".to_owned(), None), (300, "Blue".to_owned(), None)]
    );

    // A click is only valid for a button of this server's panel.
    assert_eq!(
        db.panel_role(guild, panel.id, 300).await.unwrap().unwrap().label,
        "Blue"
    );
    assert!(
        db.panel_role(other, panel.id, 300).await.unwrap().is_none(),
        "another server"
    );
    assert!(
        db.panel_role(guild, panel.id, 999).await.unwrap().is_none(),
        "not on the panel"
    );
    assert!(
        db.panel_role(guild, theirs.id, 300).await.unwrap().is_none(),
        "not on that panel"
    );

    // Posting remembers the message.
    db.set_panel_message(panel.id, 11, 22).await.unwrap();
    let posted = db.find_panel(guild, "Colours").await.unwrap().unwrap();
    assert_eq!((posted.channel_id, posted.message_id), (Some(11), Some(22)));

    // Removing a button, and a role deleted on Discord, both take it off the panel.
    assert!(db.remove_panel_role(panel.id, 300).await.unwrap());
    assert!(!db.remove_panel_role(panel.id, 300).await.unwrap());
    db.forget_role(guild, 301).await.unwrap();
    assert!(db.panel_roles(panel.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn role_panels_hold_at_most_25_roles_even_when_filled_at_once() {
    let Some(db) = db().await else { return };
    let [guild] = ids();
    let (panel, _) = db.save_panel(guild, "Everything", None).await.unwrap();

    // 40 managers adding at once: exactly 25 get in.
    let mut tasks = Vec::new();
    for role in 0..40u64 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.add_panel_role(panel.id, 5_000 + role, &format!("Role {role}"), None)
                .await
                .unwrap()
        }));
    }
    let mut added = 0;
    for task in tasks {
        added += i64::from(task.await.unwrap());
    }
    assert_eq!(added, MAX_PANEL_ROLES);
    assert_eq!(db.panel_roles(panel.id).await.unwrap().len() as i64, MAX_PANEL_ROLES);

    // Existing roles can still be relabelled when full; new ones are refused.
    let first = db.panel_roles(panel.id).await.unwrap()[0].role_id as u64;
    assert!(db.add_panel_role(panel.id, first, "Renamed", Some("⭐")).await.unwrap());
    assert!(!db.add_panel_role(panel.id, 9_999, "One too many", None).await.unwrap());
    let renamed = db.panel_role(guild, panel.id, first).await.unwrap().unwrap();
    assert_eq!(
        (renamed.label.as_str(), renamed.emoji.as_deref()),
        ("Renamed", Some("⭐"))
    );
}
