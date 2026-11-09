use crate::economy_player::*;

#[tokio::test]
async fn auctions_escrow_refund_extend_and_settle_once() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    item(&db, u, "test-chair", 1).await;
    let id = act(
        &db,
        u,
        "list",
        A::Auction {
            item: "test-chair".into(),
            price: 100,
            hours: 1,
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    assert_eq!(qty(&db, u, "test-chair").await, 0);
    assert!(
        db.economy_action(u, "self", &A::Bid { id, amount: 100 }, now())
            .await
            .is_err()
    );
    act(&db, u + 1, "bid", A::Bid { id, amount: 100 }).await;
    assert!(
        db.economy_action(u, "cancel", &A::CancelAuction { id }, now())
            .await
            .is_err()
    );
    assert!(
        db.economy_action(u + 2, "low", &A::Bid { id, amount: 104 }, now())
            .await
            .is_err()
    );
    let late = now() + Duration::minutes(59);
    let bid = A::Bid { id, amount: 105 };
    let (a, b) = tokio::join!(
        db.economy_action(u + 2, "race-a", &bid, late),
        db.economy_action(u + 3, "race-b", &bid, late)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(db.user(u + 1).await.unwrap().credits, 20000);
    let end: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT ends_at FROM economy_auctions WHERE id=$1")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(end, now() + Duration::minutes(61));
    let settle = A::SettleAuction { id };
    let (a, b) = tokio::join!(
        db.economy_action(u, "settle-a", &settle, end),
        db.economy_action(u + 1, "settle-b", &settle, end)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(
        qty(&db, u + 2, "test-chair").await + qty(&db, u + 3, "test-chair").await,
        1
    );
    assert_eq!(db.user(u).await.unwrap().credits, 20099);
    for n in 0..4 {
        ledger_matches(&db, u + n).await;
    }
}

#[tokio::test]
async fn trades_need_both_confirmations_and_swap_atomically() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    item(&db, u, "chair", 1).await;
    item(&db, u + 1, "lamp", 1).await;
    let id = act(
        &db,
        u,
        "offer",
        A::Trade {
            to: (u + 1).to_string(),
            give: offer(100, "chair"),
            receive: offer(200, "lamp"),
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    assert_eq!(qty(&db, u, "chair").await, 0);
    assert!(
        db.economy_action(
            u,
            "double-sell",
            &A::Auction {
                item: "chair".into(),
                price: 10,
                hours: 1
            },
            now()
        )
        .await
        .is_err()
    );
    assert!(
        db.economy_action(u + 2, "intruder", &A::ConfirmTrade { id }, now())
            .await
            .is_err()
    );
    act(&db, u, "confirm-a", A::ConfirmTrade { id }).await;
    assert_eq!(qty(&db, u, "lamp").await, 0);
    let confirm = A::ConfirmTrade { id };
    let (a, b) = tokio::join!(
        db.economy_action(u + 1, "confirm-b", &confirm, now()),
        db.economy_action(u + 1, "confirm-b", &confirm, now())
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(qty(&db, u, "lamp").await, 1);
    assert_eq!(qty(&db, u + 1, "chair").await, 1);
    assert_eq!(db.user(u).await.unwrap().credits, 20100);
    assert_eq!(db.user(u + 1).await.unwrap().credits, 19900);
    for n in 0..2 {
        ledger_matches(&db, u + n).await;
    }
}

#[tokio::test]
async fn trade_failure_expiry_and_privacy_return_counterparty_assets() {
    let _isolation = crate::support::exclusive().await;
    let Some((db, u)) = setup().await else { return };
    let id = act(
        &db,
        u,
        "offer",
        A::Trade {
            to: (u + 1).to_string(),
            give: offer(100, ""),
            receive: offer(200, "missing"),
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    assert!(
        db.economy_action(u + 1, "bad-confirm", &A::ConfirmTrade { id }, now())
            .await
            .is_err()
    );
    assert_eq!(db.user(u + 1).await.unwrap().credits, 20000);
    db.sweep_economy(now() + Duration::hours(25)).await.unwrap();
    assert_eq!(db.user(u).await.unwrap().credits, 20000);
    let id = act(
        &db,
        u,
        "offer2",
        A::Trade {
            to: (u + 1).to_string(),
            give: offer(100, ""),
            receive: offer(200, ""),
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    act(&db, u + 1, "hold-b", A::ConfirmTrade { id }).await;
    assert_eq!(db.user(u + 1).await.unwrap().credits, 19800);
    db.delete_user_data(u, now()).await.unwrap();
    assert_eq!(db.user(u + 1).await.unwrap().credits, 20000);
    ledger_matches(&db, u + 1).await;
}

#[tokio::test]
async fn auction_privacy_refunds_bids_and_returns_items() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    item(&db, u, "chair", 1).await;
    let id = act(
        &db,
        u,
        "list",
        A::Auction {
            item: "chair".into(),
            price: 100,
            hours: 1,
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    act(&db, u + 1, "bid", A::Bid { id, amount: 100 }).await;
    db.delete_user_data(u + 1, now()).await.unwrap();
    assert_eq!(qty(&db, u, "chair").await, 1);
    let id = act(
        &db,
        u,
        "relist",
        A::Auction {
            item: "chair".into(),
            price: 100,
            hours: 1,
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    act(&db, u + 2, "bid2", A::Bid { id, amount: 100 }).await;
    db.delete_user_data(u, now()).await.unwrap();
    assert_eq!(db.user(u + 2).await.unwrap().credits, 20000);
    ledger_matches(&db, u + 2).await;
}

#[tokio::test]
async fn randomized_auction_trade_cancel_refund_sequences_conserve_assets() {
    let _isolation = crate::support::exclusive().await;
    let Some((db, u)) = setup().await else { return };
    for n in u..u + 4 {
        for name in ["chair", "lamp"] {
            item(&db, n, name, 10).await;
        }
    }
    // Reproducible generated sequences exercise successes, stale actions, retries and rejections.
    let mut rng = 0xd883_6762_u64;
    let mut auctions = Vec::new();
    let mut trades = Vec::new();
    for step in 0..240 {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let user = u + (rng >> 32) % 4;
        let other = u + (user - u + 1 + (rng >> 40) % 3) % 4;
        let a = match (rng >> 48) % 8 {
            0 => A::Auction {
                item: if rng & 256 == 0 { "chair" } else { "lamp" }.into(),
                price: 100,
                hours: 1,
            },
            1 if !auctions.is_empty() => A::Bid {
                id: auctions[(rng as usize >> 16) % auctions.len()],
                amount: 100 + step * 10,
            },
            2 if !auctions.is_empty() => A::CancelAuction {
                id: auctions[(rng as usize >> 16) % auctions.len()],
            },
            3 => A::Trade {
                to: other.to_string(),
                give: offer(100, "chair"),
                receive: offer(200, "lamp"),
            },
            4 if !trades.is_empty() => A::ConfirmTrade {
                id: trades[(rng as usize >> 16) % trades.len()],
            },
            5 if !trades.is_empty() => A::CancelTrade {
                id: trades[(rng as usize >> 16) % trades.len()],
            },
            6 => A::Donate {
                guild: (u + 10).to_string(),
                amount: 17,
            },
            _ => A::Trade {
                to: other.to_string(),
                give: offer(-1, ""),
                receive: offer(0, ""),
            },
        };
        let key = format!("random:{step}");
        if let Ok(v) = db.economy_action(user, &key, &a, now()).await {
            if matches!(a, A::Auction { .. }) {
                auctions.push(v["id"].as_i64().unwrap());
            }
            if matches!(a, A::Trade { .. }) {
                trades.push(v["id"].as_i64().unwrap());
            }
            assert_eq!(v, db.economy_action(user, &key, &a, now()).await.unwrap());
        }
        conserved(&db, u).await;
        if step % 40 == 39 {
            db.sweep_economy(now() + Duration::hours(25)).await.unwrap();
            conserved(&db, u).await;
        }
    }
    let counts: (i64,i64) = sqlx::query_as("SELECT count(*) FILTER (WHERE reason='auction_refund'),count(*) FILTER (WHERE reason='trade_refund') FROM economy_ledger WHERE user_id BETWEEN $1 AND $1+3")
        .bind(u as i64).fetch_one(db.pool()).await.unwrap();
    assert!(
        counts.0 > 0 && counts.1 > 0,
        "generated sequences must exercise both refunds: {counts:?}"
    );
}

#[tokio::test]
async fn auction_net_proceeds_fit_even_when_gross_would_overflow() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    item(&db, u, "chair", 1).await;
    let id = act(
        &db,
        u,
        "create",
        A::Auction {
            item: "chair".into(),
            price: 100,
            hours: 1,
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    act(&db, u + 1, "bid", A::Bid { id, amount: 100 }).await;
    sqlx::query("UPDATE users SET credits=$2 WHERE id=$1")
        .bind(u as i64)
        .bind(i64::MAX - 95)
        .execute(db.pool())
        .await
        .unwrap();
    let result = db
        .economy_action(u, "settle", &A::SettleAuction { id }, now() + Duration::hours(2))
        .await;
    let balance = db.user(u).await.unwrap().credits;
    // Restore before assertions so shared leaderboards remain within fixture budgets.
    sqlx::query("UPDATE users SET credits=20095 WHERE id=$1")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    result.unwrap();
    assert_eq!(balance, i64::MAX);
    assert_eq!(qty(&db, u + 1, "chair").await, 1);
    ledger_matches(&db, u).await;
}

#[tokio::test]
async fn anti_snipe_never_exceeds_hard_end_and_privacy_removes_ledger_pairs() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    item(&db, u, "chair", 1).await;
    let id = act(
        &db,
        u,
        "create",
        A::Auction {
            item: "chair".into(),
            price: 100,
            hours: 1,
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let hard = now() + Duration::minutes(90);
    let mut end = now() + Duration::hours(1);
    let mut bid = 100;
    for n in 0..33 {
        let result = db
            .economy_action(
                u + 1 + (n % 2),
                &format!("late:{n}"),
                &A::Bid { id, amount: bid },
                end - Duration::seconds(30),
            )
            .await
            .unwrap();
        end = serde_json::from_value(result["ends_at"].clone()).unwrap();
        assert!(end <= hard);
        bid = pinchora_core::economy::player::minimum_bid(bid);
    }
    assert_eq!(end, hard);
    assert!(
        db.economy_action(u + 3, "too-late", &A::Bid { id, amount: bid }, hard)
            .await
            .is_err()
    );
    db.delete_user_data(u, hard).await.unwrap();
    for bidder in [u + 1, u + 2] {
        assert_eq!(db.user(bidder).await.unwrap().credits, 20000);
        ledger_matches(&db, bidder).await;
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM economy_ledger WHERE user_id=$1")
        .bind(u as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn unchanged_settings_preserve_revision_and_active_market_survives_history() {
    let _isolation = crate::support::shared().await;
    let Some((db, u)) = setup().await else { return };
    assert!(
        !db.save_economy_settings(u + 10, &EconomySettings::default(), 0)
            .await
            .unwrap()
    );
    assert_eq!(db.economy_settings(u + 10).await.unwrap()["revision"], 0);
    assert!(
        db.save_economy_settings(
            u + 10,
            &EconomySettings {
                shop_enabled: false,
                ..Default::default()
            },
            0
        )
        .await
        .unwrap()
    );
    assert!(
        !db.save_economy_settings(
            u + 10,
            &EconomySettings {
                shop_enabled: false,
                ..Default::default()
            },
            1
        )
        .await
        .unwrap()
    );
    assert_eq!(db.economy_settings(u + 10).await.unwrap()["revision"], 1);
    item(&db, u + 1, "chair", 1).await;
    let active = act(
        &db,
        u + 1,
        "list",
        A::Auction {
            item: "chair".into(),
            price: 100,
            hours: 1,
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    sqlx::query("INSERT INTO economy_auctions(seller_id,item_id,start_price,ends_at,hard_end,status) SELECT $1,'chair',100,now(),now(),'expired' FROM generate_series(1,101)").bind(u as i64).execute(db.pool()).await.unwrap();
    let snapshot = db.economy_snapshot(u, now()).await.unwrap();
    assert!(
        snapshot["auctions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == active)
    );
}
