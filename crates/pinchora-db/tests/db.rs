//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.

use pinchora_db::{
    BefriendOutcome, CustomizeBuy, DailyOutcome, Db, LogKind, MAX_BADGES, MAX_FRIENDS, MAX_PANEL_ROLES, MAX_REP_ROLES,
    MAX_XP_RULES, PairOutcome, PlayerStats, RepOutcome, Tally, TransferOutcome, TripClaim, TripPayout, Wake, XpGrant,
    XpTarget,
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
async fn badges_stay_automatic_until_the_player_chooses() {
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();

    // A new player has not chosen: the card picks their badges.
    let row = db.user(alice).await.unwrap();
    assert!(!row.badges_set && row.badges.is_empty());
    assert_eq!(
        row.badges_seen,
        Some(Vec::new()),
        "nothing seen yet, so what they earn first is new"
    );
    assert!(row.title.is_none());

    // Any save is a choice, an empty one included: it now means "no badges".
    db.set_badges(alice, &[]).await.unwrap();
    assert!(db.user(alice).await.unwrap().badges_set);
    db.set_badges(alice, &["pawshake".to_owned()]).await.unwrap();
    assert!(db.user(alice).await.unwrap().badges_set);

    // Resetting hands the choice back to the card.
    db.reset_badges(alice).await.unwrap();
    let row = db.user(alice).await.unwrap();
    assert!(!row.badges_set && row.badges.is_empty());

    // Saving for a player the database has never seen creates them with the choice made.
    db.set_badges(bob, &[]).await.unwrap();
    assert!(db.user(bob).await.unwrap().badges_set);
}

#[tokio::test]
async fn badge_notices_are_claimed_once_and_old_news_is_not_announced() {
    let Some(db) = db().await else { return };
    let [alice, old, nobody] = ids();
    let list = |ids: &[&str]| ids.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();

    // A new player starts with nothing announced.
    assert_eq!(db.user(alice).await.unwrap().credits, 0);
    let (stats, notified) = db.badge_progress(alice).await.unwrap();
    assert_eq!(stats, PlayerStats::default());
    assert_eq!(notified, Some(Vec::new()));
    assert_eq!(
        db.claim_badge_notices(alice, &list(&["little_beacon"])).await.unwrap(),
        list(&["little_beacon"])
    );
    assert_eq!(
        db.claim_badge_notices(alice, &list(&["little_beacon"])).await.unwrap(),
        Vec::<String>::new(),
        "never twice"
    );
    assert_eq!(
        db.claim_badge_notices(alice, &list(&["little_beacon", "pawshake"]))
            .await
            .unwrap(),
        list(&["pawshake"]),
        "only what is new"
    );
    assert_eq!(
        db.badge_progress(alice).await.unwrap().1,
        Some(list(&["little_beacon", "pawshake"]))
    );

    // Commands finishing together announce a badge exactly once between them.
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.claim_badge_notices(alice, &list(&["fin_finder"])).await.unwrap()
        }));
    }
    let mut claimed = 0;
    for t in tasks {
        claimed += t.await.unwrap().len();
    }
    assert_eq!(claimed, 1);

    // A player from before notices existed has no list. The first claim fills it with what
    // they already have and announces nothing; the next one announces what comes after.
    db.user(old).await.unwrap();
    sqlx::query("UPDATE users SET badges_notified = NULL, badges_seen = NULL WHERE id = $1")
        .bind(old as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(db.badge_progress(old).await.unwrap().1, None);
    assert_eq!(db.user(old).await.unwrap().badges_seen, None);
    assert!(
        db.claim_badge_notices(old, &list(&["little_beacon", "fin_finder"]))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.claim_badge_notices(old, &list(&["little_beacon", "fin_finder", "pawshake"]))
            .await
            .unwrap(),
        list(&["pawshake"])
    );

    // Nobody the database does not know has nothing yet; a pet alone is enough to count, and
    // announcing it gives them their row.
    assert_eq!(
        db.badge_progress(nobody).await.unwrap(),
        (PlayerStats::default(), Some(Vec::new()))
    );
    db.adopt(nobody, "crumbclaw_crab", "Sir Clicks").await.unwrap();
    let (stats, notified) = db.badge_progress(nobody).await.unwrap();
    assert_eq!((stats.pets, notified), (1, Some(Vec::new())));
    assert_eq!(
        db.claim_badge_notices(nobody, &list(&["pawshake"])).await.unwrap(),
        list(&["pawshake"])
    );
    assert!(
        db.claim_badge_notices(nobody, &list(&["pawshake"]))
            .await
            .unwrap()
            .is_empty()
    );

    // What the player has seen in the collection is kept for its NEW markers.
    db.mark_badges_seen(old, &list(&["little_beacon"])).await.unwrap();
    assert_eq!(db.user(old).await.unwrap().badges_seen, Some(list(&["little_beacon"])));
}

#[tokio::test]
async fn titles_are_worn_and_granted() {
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();

    assert!(db.granted_titles(alice).await.unwrap().is_empty());
    assert!(db.grant_title(alice, "house_of_the_week").await.unwrap());
    assert!(
        !db.grant_title(alice, "house_of_the_week").await.unwrap(),
        "granted once"
    );
    assert!(db.grant_title(alice, "tide_angler").await.unwrap());
    assert_eq!(
        db.granted_titles(alice).await.unwrap(),
        ["house_of_the_week", "tide_angler"],
        "oldest first"
    );
    assert!(
        db.granted_titles(bob).await.unwrap().is_empty(),
        "grants are per player"
    );

    assert_eq!(db.user(alice).await.unwrap().title, None);
    db.set_title(alice, Some("tide_angler")).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().title.as_deref(), Some("tide_angler"));
    db.set_title(alice, None).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().title, None);
    db.set_title(bob, Some("early_riser")).await.unwrap();
    assert_eq!(
        db.user(bob).await.unwrap().title.as_deref(),
        Some("early_riser"),
        "saving creates the player"
    );
}

#[tokio::test]
async fn card_style_and_showcase_are_saved_per_player() {
    let Some(db) = db().await else { return };
    let [alice, bob] = ids();

    // A new player has the look every player starts with and an empty showcase.
    let row = db.user(alice).await.unwrap();
    assert_eq!(
        (row.accent.as_deref(), row.panel_opacity, row.frame.as_str()),
        (None, 1, "plain")
    );
    assert_eq!(
        (
            row.showcase_fish,
            row.showcase_pet,
            row.showcase_item,
            row.showcase_house
        ),
        (None, None, None, false)
    );

    // Style: each part is saved on its own; saving creates the player; the database refuses nonsense.
    db.set_accent(alice, Some("#3a9bdc")).await.unwrap();
    db.set_panel_opacity(alice, 0).await.unwrap();
    db.set_frame(alice, "rope").await.unwrap();
    let row = db.user(alice).await.unwrap();
    assert_eq!(
        (row.accent.as_deref(), row.panel_opacity, row.frame.as_str()),
        (Some("#3a9bdc"), 0, "rope")
    );
    for bad in ["3a9bdc", "#3A9BDC", "#fff", "#3a9bd", "#3a9bdcc", "#gggggg", ""] {
        assert!(
            db.set_accent(alice, Some(bad)).await.is_err(),
            "{bad:?} is not an accent"
        );
    }
    assert!(db.set_panel_opacity(alice, 3).await.is_err() && db.set_panel_opacity(alice, -1).await.is_err());
    assert_eq!(
        db.user(alice).await.unwrap().accent.as_deref(),
        Some("#3a9bdc"),
        "refusals change nothing"
    );
    db.set_frame(bob, "gold").await.unwrap();
    assert_eq!(db.user(bob).await.unwrap().frame, "gold", "saving creates the player");
    assert_eq!(db.user(alice).await.unwrap().frame, "rope", "styles are per player");
    db.set_accent(alice, None).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().accent, None);
    db.set_accent(alice, Some("#06d6a0")).await.unwrap();
    db.reset_card_style(alice).await.unwrap();
    let row = db.user(alice).await.unwrap();
    assert_eq!(
        (row.accent.as_deref(), row.panel_opacity, row.frame.as_str()),
        (None, 1, "plain")
    );

    // What can be put on the showcase: the fish book, first catch first.
    assert!(db.fish_book(alice).await.unwrap().is_empty());
    db.log_catch(alice, "trout", 20.0).await.unwrap();
    db.log_catch(alice, "carp", 31.5).await.unwrap();
    db.log_catch(alice, "trout", 25.0).await.unwrap();
    let book = db.fish_book(alice).await.unwrap();
    assert_eq!(
        book.iter().map(|b| b.fish_id.as_str()).collect::<Vec<_>>(),
        ["trout", "carp"]
    );
    assert_eq!((book[0].caught, book[0].best_cm), (2, 25.0));
    assert!(db.fish_book(bob).await.unwrap().is_empty(), "books are per player");

    // Furniture they own: in the inventory or standing in a room, each once, by id.
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    assert!(db.house_preview(alice).await.unwrap().is_none(), "no house yet");
    db.house(alice, &starter).await.unwrap();
    db.grant_item(alice, "chair", 2).await.unwrap();
    db.grant_item(alice, "lamp", 1).await.unwrap();
    db.place_item(alice, 0, "lamp", 1, 1, false, |_| Ok(())).await.unwrap();
    db.place_item(alice, 0, "chair", 2, 2, false, |_| Ok(())).await.unwrap();
    assert_eq!(
        db.owned_item_ids(alice).await.unwrap(),
        ["chair", "lamp"],
        "a chair left in the inventory and a placed one count once; a lamp that is only placed counts too"
    );
    assert!(db.owned_item_ids(bob).await.unwrap().is_empty());
    let preview = db.house_preview(alice).await.unwrap().expect("a house");
    assert_eq!((preview.room.idx, preview.room.wallpaper.as_str()), (0, "w"));
    assert_eq!(preview.placements.len(), 2);
    assert_eq!(preview.house.tier, 1);

    // The picks themselves, and clearing them.
    db.set_showcase_fish(alice, Some("carp")).await.unwrap();
    db.set_showcase_pet(alice, Some(42)).await.unwrap();
    db.set_showcase_item(alice, Some("lamp")).await.unwrap();
    db.set_showcase_house(alice, true).await.unwrap();
    let row = db.user(alice).await.unwrap();
    assert_eq!(
        (
            row.showcase_fish.as_deref(),
            row.showcase_pet,
            row.showcase_item.as_deref(),
            row.showcase_house
        ),
        (Some("carp"), Some(42), Some("lamp"), true)
    );
    db.set_showcase_fish(alice, None).await.unwrap();
    assert_eq!(db.user(alice).await.unwrap().showcase_fish, None);
    assert_eq!(
        db.user(alice).await.unwrap().showcase_pet,
        Some(42),
        "the other picks stay"
    );
    db.clear_showcase(alice).await.unwrap();
    let row = db.user(alice).await.unwrap();
    assert_eq!(
        (
            row.showcase_fish,
            row.showcase_pet,
            row.showcase_item,
            row.showcase_house
        ),
        (None, None, None, false)
    );
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

#[tokio::test]
async fn restore_is_all_or_nothing() {
    let Some(db) = db().await else { return };
    let [user] = ids();
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    db.house(user, &starter).await.unwrap();
    db.grant_item(user, "chair", 2).await.unwrap();
    db.place_item(user, 0, "chair", 1, 1, false, |_| Ok(())).await.unwrap();
    db.place_item(user, 0, "chair", 2, 2, true, |_| Ok(())).await.unwrap();
    let removed = db.clear_room(user, 0).await.unwrap();
    assert_eq!(removed.len(), 2);
    assert!(db.placements(user, 0).await.unwrap().is_empty());
    assert_eq!(db.inventory(user).await.unwrap()[0].quantity, 2);

    // Putting back three chairs with two in the inventory changes nothing.
    let mut changes: Vec<pinchora_db::RoomChange> = removed.iter().cloned().map(pinchora_db::RoomChange::Add).collect();
    let mut extra = removed[0].clone();
    extra.x = 5;
    changes.push(pinchora_db::RoomChange::Add(extra));
    let refused = db.restore(user, 0, &changes, |_, _| Ok(())).await.unwrap();
    assert!(matches!(
        refused,
        pinchora_db::RestoreOutcome::Refused(pinchora_db::Refusal::NotOwned(ref item)) if item == "chair"
    ));
    assert!(db.placements(user, 0).await.unwrap().is_empty(), "rolled back");

    // The room check can refuse too; then the real two go back exactly.
    changes.pop();
    let blocked = db.restore(user, 0, &changes, |_, _| Err("no".into())).await.unwrap();
    assert!(matches!(
        blocked,
        pinchora_db::RestoreOutcome::Refused(pinchora_db::Refusal::Blocked(_))
    ));
    let pinchora_db::RestoreOutcome::Done { added } = db.restore(user, 0, &changes, |_, _| Ok(())).await.unwrap()
    else {
        panic!("restores")
    };
    let back = db.placements(user, 0).await.unwrap();
    assert_eq!(back.iter().map(|p| p.id).collect::<Vec<_>>(), added);
    assert_eq!(
        back.iter().map(|p| (p.x, p.y, p.flipped)).collect::<Vec<_>>(),
        [(1, 1, false), (2, 2, true)]
    );

    // A remove or flip must find the row exactly as given.
    let mut moved = back[0].clone();
    moved.x = 7;
    let stale = db
        .restore(user, 0, &[pinchora_db::RoomChange::Flip(moved)], |_, _| Ok(()))
        .await
        .unwrap();
    assert!(matches!(
        stale,
        pinchora_db::RestoreOutcome::Refused(pinchora_db::Refusal::Moved(_))
    ));
    db.restore(
        user,
        0,
        &[
            pinchora_db::RoomChange::Flip(back[0].clone()),
            pinchora_db::RoomChange::Remove(back[1].clone()),
        ],
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    let now = db.placements(user, 0).await.unwrap();
    assert_eq!(now.len(), 1);
    assert!(now[0].flipped);
}

#[tokio::test]
async fn contest_week_closes_once() {
    let Some(db) = db().await else { return };
    let [guild, a, b, voter] = ids();
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    for user in [a, b] {
        db.house(user, &starter).await.unwrap();
        db.grant_item(user, "chair", 1).await.unwrap();
        db.place_item(user, 0, "chair", 1, 1, false, |_| Ok(())).await.unwrap();
        assert_eq!(db.enter_contest(guild, 100, user, "Name", 0).await.unwrap(), 1);
    }
    assert!(db.contest_vote(guild, 100, a, voter).await.unwrap());
    assert!(
        !db.contest_vote(guild, 100, a, voter).await.unwrap(),
        "one vote per entry"
    );
    assert!(!db.contest_vote(guild, 100, a, a).await.unwrap(), "not for your own");
    assert!(db.contest_vote(guild, 100, a, b).await.unwrap());
    assert!(db.contest_vote(guild, 100, b, voter).await.unwrap());
    assert_eq!(db.contest_pieces(guild, 100, a).await.unwrap().len(), 1);

    let before = db.user(a).await.unwrap().credits;
    // Two calls race to close the week; only one pays.
    let (one, two) = tokio::join!(
        db.close_contest_weeks(guild, 101, &[500, 250, 100], "house_of_the_week"),
        db.close_contest_weeks(guild, 101, &[500, 250, 100], "house_of_the_week"),
    );
    let closed = [one.unwrap(), two.unwrap()];
    assert_eq!(closed.iter().map(Vec::len).sum::<usize>(), 1, "{closed:?}");
    assert!(
        db.close_contest_weeks(guild, 101, &[500], "t")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(db.user(a).await.unwrap().credits, before + 500);
    let result = db.last_contest_result(guild, 101).await.unwrap().unwrap();
    assert_eq!(result.winner_ids, vec![a as i64, b as i64]);
    assert_eq!(result.winner_votes, vec![2, 1]);
    assert_eq!(result.entries, 2);
}

#[tokio::test]
async fn one_partner_each_even_when_pairings_race() {
    let Some(db) = db().await else { return };
    let [x, y, z, w, v] = ids();

    let PairOutcome::Paired { since } = db.pair_partners(y, z).await.unwrap() else {
        panic!("two free players pair up");
    };
    let partner = db.partner(y).await.unwrap().expect("y has a partner");
    assert_eq!((partner.partner_id, partner.since), (z, since));
    assert_eq!(db.partner(z).await.unwrap().map(|p| p.partner_id), Some(y));

    // `y` is the smaller id of the first pair and the larger of the second: only the trigger
    // sees that this is a second partner. Either order of the arguments is the same pair.
    assert_eq!(db.pair_partners(x, y).await.unwrap(), PairOutcome::Taken { user: y });
    assert_eq!(db.pair_partners(y, x).await.unwrap(), PairOutcome::Taken { user: y });
    assert_eq!(db.pair_partners(z, w).await.unwrap(), PairOutcome::Taken { user: z });
    assert!(db.partner(x).await.unwrap().is_none() && db.partner(w).await.unwrap().is_none());
    assert!(db.pair_partners(x, x).await.is_err());

    // Names ride along for the profile card.
    assert_eq!(db.partner_name(y).await.unwrap(), None, "no name remembered yet");
    db.remember_name(z, "Zed").await.unwrap();
    db.remember_name(z, "Zed the Second").await.unwrap();
    assert_eq!(db.partner_name(y).await.unwrap().as_deref(), Some("Zed the Second"));
    assert_eq!(db.partner_name(x).await.unwrap(), None);

    // Leaving frees both, and tells who the partner was.
    let left = db.leave_partner(z).await.unwrap().expect("z had a partner");
    assert_eq!((left.partner_id, left.since), (y, since));
    assert!(db.leave_partner(z).await.unwrap().is_none());
    assert!(db.partner(y).await.unwrap().is_none());
    assert!(matches!(
        db.pair_partners(x, y).await.unwrap(),
        PairOutcome::Paired { .. }
    ));

    // Pairings that share a player race: exactly one wins.
    let outcomes = tokio::join!(
        db.pair_partners(w, v),
        db.pair_partners(v, z),
        db.pair_partners(z, w),
        db.pair_partners(w, z),
    );
    let outcomes = [outcomes.0, outcomes.1, outcomes.2, outcomes.3].map(|o| o.unwrap());
    let paired = outcomes
        .iter()
        .filter(|o| matches!(o, PairOutcome::Paired { .. }))
        .count();
    // (w,v) and (v,z) and (z,w) all share players: at most one pairing among w, v, z survives.
    assert_eq!(paired, 1, "{outcomes:?}");
    let in_pairs = [w, v, z].into_iter().map(|u| db.partner(u)).collect::<Vec<_>>();
    let mut partnered = 0;
    for p in in_pairs {
        partnered += usize::from(p.await.unwrap().is_some());
    }
    assert_eq!(partnered, 2, "one pair, two partnered players");
}

#[tokio::test]
async fn friends_are_mutual_and_capped() {
    let Some(db) = db().await else { return };
    let [a, b, c] = ids();

    assert_eq!(db.befriend(a, b).await.unwrap(), BefriendOutcome::Added);
    assert_eq!(db.befriend(b, a).await.unwrap(), BefriendOutcome::AlreadyFriends);
    assert!(db.are_friends(a, b).await.unwrap() && db.are_friends(b, a).await.unwrap());
    assert!(!db.are_friends(a, c).await.unwrap());
    assert!(db.befriend(a, a).await.is_err());

    db.remember_name(b, "Bea").await.unwrap();
    db.grant_item(b, "chair", 1).await.unwrap();
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    db.house(b, &starter).await.unwrap();
    db.like_house(b, c, chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap())
        .await
        .unwrap();
    assert_eq!(db.befriend(a, c).await.unwrap(), BefriendOutcome::Added);
    let (page, total) = db.friends(a, 0, 10).await.unwrap();
    assert_eq!(total, 2);
    assert_eq!(
        page.iter().map(|f| f.friend_id).collect::<Vec<_>>(),
        vec![c, b],
        "newest first"
    );
    let bea = page.iter().find(|f| f.friend_id == b).unwrap();
    assert_eq!((bea.name.as_str(), bea.likes, bea.locked), ("Bea", 1, false));
    assert_eq!(db.friends(a, 1, 1).await.unwrap().0.len(), 1, "paging");

    // The shortlist leaves out locked houses and puts the most liked first.
    let shortlist = db.visitable_friends(a, 25).await.unwrap();
    assert_eq!(shortlist.iter().map(|f| f.friend_id).collect::<Vec<_>>(), vec![b, c]);
    db.set_house_locked(b, true).await.unwrap();
    let shortlist = db.visitable_friends(a, 25).await.unwrap();
    assert_eq!(shortlist.iter().map(|f| f.friend_id).collect::<Vec<_>>(), vec![c]);

    // Removing a friend removes both sides.
    assert!(db.unfriend(b, a).await.unwrap());
    assert!(!db.unfriend(a, b).await.unwrap());
    assert!(!db.are_friends(a, b).await.unwrap() && !db.are_friends(b, a).await.unwrap());
    assert_eq!(db.friend_count(b).await.unwrap(), 0);

    // 100 friends is the most; the other side's cap counts too.
    let [full, last] = ids();
    let crowd = (0..MAX_FRIENDS as u64)
        .map(|n| full + 1_000_000 + n)
        .collect::<Vec<_>>();
    for friend in &crowd {
        assert_eq!(db.befriend(full, *friend).await.unwrap(), BefriendOutcome::Added);
    }
    assert_eq!(
        db.befriend(full, last).await.unwrap(),
        BefriendOutcome::Full { user: full }
    );
    assert_eq!(
        db.befriend(last, full).await.unwrap(),
        BefriendOutcome::Full { user: full }
    );
    assert_eq!(db.friend_count(full).await.unwrap(), MAX_FRIENDS);
    assert!(db.unfriend(full, crowd[0]).await.unwrap());
    assert_eq!(db.befriend(last, full).await.unwrap(), BefriendOutcome::Added);
}

#[tokio::test]
async fn interactions_count_per_pair_and_per_user() {
    let Some(db) = db().await else { return };
    let [a, b, c] = ids();

    assert_eq!(
        db.record_interaction("hug", a, b).await.unwrap(),
        Tally {
            pair: 1,
            given: 1,
            received: 1
        }
    );
    assert_eq!(
        db.record_interaction("hug", a, b).await.unwrap(),
        Tally {
            pair: 2,
            given: 2,
            received: 2
        }
    );
    // Another receiver: the pair starts over, the sender's total goes on.
    assert_eq!(
        db.record_interaction("hug", a, c).await.unwrap(),
        Tally {
            pair: 1,
            given: 3,
            received: 1
        }
    );
    // The other direction is its own pair; a different kind has its own counters.
    assert_eq!(
        db.record_interaction("hug", b, a).await.unwrap(),
        Tally {
            pair: 1,
            given: 1,
            received: 1
        }
    );
    assert_eq!(
        db.record_interaction("pat", a, b).await.unwrap(),
        Tally {
            pair: 1,
            given: 1,
            received: 1
        }
    );
    assert_eq!(
        db.exchanged(a, b).await.unwrap(),
        vec![("hug".to_owned(), 2, 1), ("pat".to_owned(), 1, 0)]
    );
    assert_eq!(
        db.exchanged(b, a).await.unwrap(),
        vec![("hug".to_owned(), 1, 2), ("pat".to_owned(), 0, 1)]
    );
    assert!(db.exchanged(b, c).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_repeat_visit_moves_the_time_but_pays_once() {
    let Some(db) = db().await else { return };
    let [host, guest] = ids();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();

    let first = db.record_visit(host, guest, day, 25, 10).await.unwrap();
    assert!(
        matches!(first, pinchora_db::VisitOutcome::Rewarded { reward: 25 }),
        "{first:?}"
    );
    // Pretend the first visit was long ago; the next visit of the day must bring it up to date.
    sqlx::query("UPDATE visits SET visited_at = now() - interval '30 hours' WHERE host_id = $1 AND guest_id = $2")
        .bind(host as i64)
        .bind(guest as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        db.recent_visitors(host, 10).await.unwrap()[0].visited_at < chrono::Utc::now() - chrono::Duration::hours(24)
    );
    let again = db.record_visit(host, guest, day, 25, 10).await.unwrap();
    assert!(matches!(again, pinchora_db::VisitOutcome::AlreadyToday), "{again:?}");
    let visitors = db.recent_visitors(host, 10).await.unwrap();
    assert!(
        visitors[0].visited_at > chrono::Utc::now() - chrono::Duration::minutes(1),
        "{visitors:?}"
    );
    assert_eq!(db.user(guest).await.unwrap().credits, 25, "paid once");
    let (rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM visits WHERE host_id = $1 AND guest_id = $2")
        .bind(host as i64)
        .bind(guest as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[tokio::test]
async fn weekly_xp_is_kept_per_week_and_chat_splits_server_and_global_xp() {
    let Some(db) = db().await else { return };
    let [guild, a, b, c] = ids();
    let this_week = chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
    let last_week = this_week - chrono::Duration::days(7);
    let grant = |server_xp, global_xp, points| XpGrant {
        server_xp,
        global_xp,
        points,
    };

    // A boosted chat tick: the server gets 40 XP and 6 points, the global total only the base 20.
    let gain = db.add_xp(guild, a, grant(40, 20, 6), last_week).await.unwrap();
    assert_eq!((gain.old_xp, gain.new_xp), (0, 40));
    assert_eq!(db.user(a).await.unwrap().xp, 20, "global XP is not multiplied");
    assert_eq!(db.member_points(guild, a).await.unwrap().0, 6);
    // A no-XP tick: nothing for the server, the member row and the weekly board stay clean.
    let gain = db.add_xp(guild, b, grant(0, 20, 0), this_week).await.unwrap();
    assert_eq!((gain.old_xp, gain.new_xp), (0, 0));
    assert_eq!(db.user(b).await.unwrap().xp, 20);
    assert!(db.weekly_leaderboard(guild, this_week, 10, 0).await.unwrap().is_empty());

    // Voice minutes are batched: one statement, every member counted, level-ups visible in the gains.
    db.add_xp(guild, a, grant(10, 10, 0), this_week).await.unwrap();
    let gains = db
        .add_voice_xp(guild, &[(a, 10), (b, 30), (c, 20)], this_week)
        .await
        .unwrap();
    let gain_of = |user: u64| gains.iter().find(|(u, _)| *u == user).unwrap().1;
    assert_eq!((gain_of(a).old_xp, gain_of(a).new_xp), (50, 60));
    assert_eq!((gain_of(b).old_xp, gain_of(b).new_xp), (0, 30));
    assert_eq!((gain_of(c).old_xp, gain_of(c).new_xp), (0, 20));
    assert!(db.add_voice_xp(guild, &[], this_week).await.unwrap().is_empty());
    assert_eq!(db.user(c).await.unwrap().xp, 0, "voice earns server XP only");

    // The weekly boards: this week counts only this week's XP, in order, and last week keeps its own.
    let board =
        |rows: Vec<pinchora_db::LeaderboardRow>| rows.iter().map(|r| (r.user_id as u64, r.value)).collect::<Vec<_>>();
    assert_eq!(
        board(db.weekly_leaderboard(guild, this_week, 10, 0).await.unwrap()),
        vec![(b, 30), (a, 20), (c, 20)]
    );
    assert_eq!(
        board(db.weekly_leaderboard(guild, this_week, 1, 1).await.unwrap()),
        vec![(a, 20)],
        "paging, ties by user id"
    );
    assert_eq!(
        board(db.weekly_leaderboard(guild, last_week, 10, 0).await.unwrap()),
        vec![(a, 40)]
    );
    assert!(
        db.weekly_leaderboard(guild + 1, this_week, 10, 0)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.guild_leaderboard(guild, 10, 0).await.unwrap()[0].value,
        60,
        "all-time XP is untouched"
    );

    // Old weeks can be pruned without touching the current one.
    let ancient = this_week - chrono::Duration::days(70);
    db.add_voice_xp(guild, &[(a, 5)], ancient).await.unwrap();
    assert!(
        db.forget_weeks_before(this_week - chrono::Duration::days(35))
            .await
            .unwrap()
            >= 1
    );
    assert!(db.weekly_leaderboard(guild, ancient, 10, 0).await.unwrap().is_empty());
    assert_eq!(db.weekly_leaderboard(guild, this_week, 10, 0).await.unwrap().len(), 3);
}

#[tokio::test]
async fn xp_rules_are_capped_replaced_and_cleared() {
    let Some(db) = db().await else { return };
    let [guild, other] = ids();

    assert!(db.xp_rules(guild).await.unwrap().is_empty());
    assert!(db.set_xp_rule(guild, XpTarget::Channel, 100, 2.0).await.unwrap());
    assert!(
        db.set_xp_rule(guild, XpTarget::Role, 100, 0.0).await.unwrap(),
        "same id, other kind"
    );
    assert!(
        db.set_xp_rule(guild, XpTarget::Channel, 100, 1.5).await.unwrap(),
        "replaced"
    );
    let rules = db.xp_rules(guild).await.unwrap();
    assert_eq!(rules.guild, guild);
    assert_eq!((rules.channels[&100], rules.roles[&100]), (1.5, 0.0));

    // The cap counts channels and roles together; existing rules can still be changed.
    for n in 0..(MAX_XP_RULES - 2) as u64 {
        assert!(db.set_xp_rule(guild, XpTarget::Channel, 1_000 + n, 3.0).await.unwrap());
    }
    assert!(
        !db.set_xp_rule(guild, XpTarget::Role, 999, 2.0).await.unwrap(),
        "at the limit"
    );
    assert!(db.set_xp_rule(guild, XpTarget::Role, 100, 4.0).await.unwrap());
    assert!(
        db.set_xp_rule(other, XpTarget::Role, 999, 2.0).await.unwrap(),
        "the cap is per server"
    );

    assert!(db.remove_xp_rule(guild, XpTarget::Channel, 100).await.unwrap());
    assert!(!db.remove_xp_rule(guild, XpTarget::Channel, 100).await.unwrap());
    assert!(
        db.set_xp_rule(guild, XpTarget::Role, 999, 2.0).await.unwrap(),
        "room again"
    );

    // A role deleted on Discord takes its rule with it.
    db.forget_role(guild, 999).await.unwrap();
    assert!(!db.xp_rules(guild).await.unwrap().roles.contains_key(&999));
    assert_eq!(db.clear_xp_rules(guild).await.unwrap(), MAX_XP_RULES as u64 - 1);
    assert!(db.xp_rules(guild).await.unwrap().is_empty());
    assert_eq!(db.xp_rules(other).await.unwrap().roles.len(), 1);

    // The database refuses a multiplier out of range.
    assert!(db.set_xp_rule(guild, XpTarget::Role, 1, 5.5).await.is_err());
    assert!(db.set_xp_rule(guild, XpTarget::Role, 1, -1.0).await.is_err());
}

#[tokio::test]
async fn rep_roles_are_capped_and_replaceable() {
    let Some(db) = db().await else { return };
    let [guild, other] = ids();

    assert!(db.rep_roles(guild).await.unwrap().is_empty());
    assert!(db.add_rep_role(guild, 10, 1_000).await.unwrap());
    assert!(db.add_rep_role(guild, 3, 1_001).await.unwrap());
    assert!(
        db.add_rep_role(guild, 3, 1_002).await.unwrap(),
        "roles can share a count"
    );
    let rows: Vec<_> = db
        .rep_roles(guild)
        .await
        .unwrap()
        .iter()
        .map(|r| (r.reputation, r.role_id))
        .collect();
    assert_eq!(rows, vec![(3, 1_001), (3, 1_002), (10, 1_000)]);
    assert!(
        db.add_rep_role(guild, 25, 1_000).await.unwrap(),
        "adding again moves it"
    );
    assert_eq!(db.rep_roles(guild).await.unwrap().last().unwrap().reputation, 25);

    for role in 0..(MAX_REP_ROLES - 3) as u64 {
        assert!(db.add_rep_role(guild, 1, 2_000 + role).await.unwrap());
    }
    assert!(!db.add_rep_role(guild, 1, 9_999).await.unwrap(), "at the limit");
    assert!(
        db.add_rep_role(guild, 30, 1_000).await.unwrap(),
        "existing ones can still move"
    );
    assert!(db.add_rep_role(other, 1, 9_999).await.unwrap(), "the cap is per server");

    assert!(db.remove_rep_role(guild, 1_001).await.unwrap());
    assert!(!db.remove_rep_role(guild, 1_001).await.unwrap());
    db.forget_role(guild, 1_002).await.unwrap();
    assert!(db.rep_roles(guild).await.unwrap().iter().all(|r| r.role_id != 1_002));
}

#[tokio::test]
async fn leveling_toolkit_settings_have_safe_defaults() {
    let Some(db) = db().await else { return };
    let [guild] = ids();

    let s = db.guild_settings(guild).await.unwrap();
    assert_eq!(s.levelup_mode, "channel", "level-ups keep working as before");
    assert!(!s.voice_xp_enabled, "voice XP starts off");
    assert_eq!(s.voice_xp_amount, 10);

    assert_eq!(db.set_levelup_mode(guild, "dm").await.unwrap().levelup_mode, "dm");
    assert!(
        db.set_levelup_mode(guild, "nowhere").await.is_err(),
        "unknown modes are refused"
    );
    let s = db.update_voice_xp(guild, Some(true), None).await.unwrap();
    assert_eq!((s.voice_xp_enabled, s.voice_xp_amount), (true, 10));
    let s = db.update_voice_xp(guild, None, Some(25)).await.unwrap();
    assert_eq!(
        (s.voice_xp_enabled, s.voice_xp_amount),
        (true, 25),
        "unset options keep old values"
    );
    assert!(db.update_voice_xp(guild, None, Some(0)).await.is_err());
    assert!(db.update_voice_xp(guild, None, Some(101)).await.is_err());
    assert_eq!(db.guild_settings(guild).await.unwrap().levelup_mode, "dm");
}

#[tokio::test]
async fn a_trip_pays_once_and_freezes_the_pets_needs() {
    let Some(db) = db().await else { return };
    let [owner, other] = ids();
    let pet = db.adopt(owner, "crumbclaw_crab", "Pip").await.unwrap();
    assert!(!pet.is_away(chrono::Utc::now()) && !pet.has_souvenir(chrono::Utc::now()));
    let now = chrono::Utc::now();
    let back = now + chrono::Duration::hours(4);

    // Only the owner can send it, and only once at a time.
    assert!(
        !db.start_trip(other, pet.id, "tidepool_meadow", [10, 20, 30, 40], back)
            .await
            .unwrap()
    );
    assert!(
        db.start_trip(owner, pet.id, "tidepool_meadow", [10, 20, 30, 40], back)
            .await
            .unwrap()
    );
    assert!(
        !db.start_trip(owner, pet.id, "driftwood_cove", [1, 1, 1, 1], back)
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
        db.start_trip(owner, pet.id, "driftwood_cove", [5, 5, 5, 5], after)
            .await
            .unwrap()
    );
}

/// The pet as stored now, as a care click reads it before it writes.
async fn read_pet(db: &Db, owner: u64, id: i64) -> pinchora_db::PetRow {
    db.pets(owner)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.id == id)
        .expect("the pet")
}

#[tokio::test]
async fn care_cannot_write_over_a_trip() {
    let Some(db) = db().await else { return };
    let [owner, other] = ids();
    let pet = db.adopt(owner, "crumbclaw_crab", "Pip").await.unwrap();
    let now = chrono::Utc::now();
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
        db.start_trip(owner, pet.id, "tidepool_meadow", frozen, back)
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
    let Some(db) = db().await else { return };
    let [owner] = ids();
    let now = chrono::Utc::now();
    let after = now + chrono::Duration::hours(2);
    let pet = db.adopt(owner, "crumbclaw_crab", "Pip").await.unwrap();
    assert!(
        db.start_trip(owner, pet.id, "tidepool_meadow", [50, 50, 50, 50], now)
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
    let Some(db) = db().await else { return };
    let [owner] = ids();
    let now = chrono::Utc::now();
    let back = now + chrono::Duration::hours(2);
    let after = back + chrono::Duration::minutes(1);

    // A care click and the start of a trip: whichever lands first, the pet ends up frozen.
    let frozen = [11, 22, 33, 44];
    for round in 0..20 {
        let pet = db.adopt(owner, &format!("racer_{round}"), "Pip").await.unwrap();
        let (care, trip) = tokio::join!(
            db.save_pet(owner, &pet, [100, 100, 100, 100], 2, 10, now),
            db.start_trip(owner, pet.id, "tidepool_meadow", frozen, back),
        );
        care.unwrap();
        assert!(trip.unwrap(), "round {round}: the trip started");
        let row = read_pet(&db, owner, pet.id).await;
        assert_eq!(
            (row.hunger, row.fun, row.energy, row.hygiene, row.needs_at),
            (11, 22, 33, 44, back),
            "round {round}"
        );
        assert_eq!(row.trip_dest.as_deref(), Some("tidepool_meadow"), "round {round}");
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
            db.start_trip(owner, pet.id, "tidepool_meadow", [50, 50, 50, 50], back)
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
    let Some(db) = db().await else { return };
    let [owner] = ids();
    let (pet, second) = (
        db.adopt(owner, "crumbclaw_crab", "Pip").await.unwrap(),
        db.adopt(owner, "ribbonfin_guppy", "Bub").await.unwrap(),
    );
    let long_ago = chrono::Utc::now() - chrono::Duration::days(5);
    for p in [&pet, &second] {
        db.save_pet(owner, p, [0, 0, 100, 0], 1, 0, long_ago).await.unwrap();
    }
    let now = chrono::Utc::now();
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
