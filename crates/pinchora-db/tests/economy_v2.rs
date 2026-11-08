use chrono::{Duration, TimeZone, Utc};
use pinchora_core::economy_v2::{EconomySettings, Offer, TradeItem};
use pinchora_db::{Db, economy_v2::Action as A};
use serde_json::Value;
async fn setup() -> Option<(Db, u64)> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 8, std::time::Duration::from_secs(60))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let u = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64)
        >> 4;
    for n in 0..4 {
        db.user(u + n).await.unwrap();
        db.earn_shells(u + n, 20000, "test").await.unwrap();
    }
    Some((db, u))
}
fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 8, 10, 0, 0).unwrap()
}
async fn act(db: &Db, u: u64, key: &str, a: A) -> Value {
    db.economy_action(u, key, &a, now()).await.unwrap()
}
async fn item(db: &Db, u: u64, name: &str, n: i32) {
    sqlx::query("INSERT INTO inventory(user_id,item_id,quantity) VALUES($1,$2,$3) ON CONFLICT(user_id,item_id) DO UPDATE SET quantity=EXCLUDED.quantity").bind(u as i64).bind(name).bind(n).execute(db.pool()).await.unwrap();
}
async fn qty(db: &Db, u: u64, name: &str) -> i32 {
    sqlx::query_scalar("SELECT COALESCE((SELECT quantity FROM inventory WHERE user_id=$1 AND item_id=$2),0)")
        .bind(u as i64)
        .bind(name)
        .fetch_one(db.pool())
        .await
        .unwrap()
}
fn offer(n: i64, item: &str) -> Offer {
    Offer {
        shells: n,
        items: if item.is_empty() {
            vec![]
        } else {
            vec![TradeItem {
                item_id: item.into(),
                quantity: 1,
            }]
        },
    }
}
async fn ledger_matches(db: &Db, u: u64) {
    let (credits, sum): (i64, i64) = sqlx::query_as("SELECT credits,(SELECT COALESCE(sum(delta),0)::bigint FROM credit_ledger WHERE user_id=$1) FROM users WHERE id=$1")
        .bind(u as i64).fetch_one(db.pool()).await.unwrap();
    assert_eq!(sum, credits);
}
#[tokio::test]
async fn shifts_are_ordered_capped_idempotent_and_promote() {
    let Some((db, u)) = setup().await else { return };
    for n in 0..3 {
        let time = now() + Duration::hours(n);
        let v = db
            .economy_action(
                u,
                &format!("start{n}"),
                &A::StartShift {
                    job: "kelp_keeper".into(),
                },
                time,
            )
            .await
            .unwrap();
        let token = v["token"].as_i64().unwrap();
        if n == 0 {
            sqlx::query("UPDATE economy_players SET shifts=9 WHERE user_id=$1")
                .bind(u as i64)
                .execute(db.pool())
                .await
                .unwrap();
        }
        assert!(
            db.economy_action(u, "skip", &A::Step { token, step: 2 }, time)
                .await
                .is_err()
        );
        for step in 0..3 {
            let action = A::Step { token, step };
            let key = format!("step{n}:{step}");
            let (a, b) = tokio::join!(
                db.economy_action(u, &key, &action, time),
                db.economy_action(u, &key, &action, time)
            );
            assert_eq!(a.unwrap(), b.unwrap());
        }
    }
    assert_eq!(db.user(u).await.unwrap().credits, 20350);
    assert!(
        db.economy_action(
            u,
            "cap",
            &A::StartShift {
                job: "beacon_tender".into()
            },
            now() + Duration::hours(3)
        )
        .await
        .is_err()
    );
    db.delete_user_data(u, now() + Duration::hours(3)).await.unwrap();
    assert!(
        db.economy_action(
            u,
            "deleted-cap",
            &A::StartShift {
                job: "kelp_keeper".into()
            },
            now() + Duration::hours(4)
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn provisions_chests_season_and_prestige_keep_budgets() {
    let Some((db, u)) = setup().await else { return };
    act(&db, u, "tend", A::Tend).await;
    assert!(
        db.economy_action(u, "early", &A::Provisions, now() + Duration::minutes(59))
            .await
            .is_err()
    );
    let action = A::Provisions;
    let t = now() + Duration::days(10);
    let (a, b) = tokio::join!(
        db.economy_action(u, "provisions-a", &action, t),
        db.economy_action(u, "provisions-b", &action, t)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(db.user(u).await.unwrap().credits, 20120);
    assert!(db.economy_action(u, "empty", &A::Chest, t).await.is_err());
    sqlx::query("UPDATE economy_players SET chests=1,shifts=40,season=202611,season_xp=500 WHERE user_id=$1")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        db.economy_action(u, "chest-a", &A::Chest, t),
        db.economy_action(u, "chest-b", &A::Chest, t)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    for tier in 1..=10 {
        act(&db, u, &format!("tier{tier}"), A::Season { tier }).await;
    }
    assert!(
        db.economy_action(u, "duplicate-tier", &A::Season { tier: 10 }, now())
            .await
            .is_err()
    );
    act(&db, u, "prestige", A::Prestige { expected: 0 }).await;
    let s = db.economy_snapshot(u, now()).await.unwrap();
    assert_eq!(s["player"]["shifts"], 0);
    assert_eq!(s["player"]["prestige"], 1);
    assert_eq!(s["season_claimed"], 10);
    ledger_matches(&db, u).await;
    db.delete_user_data(u, now()).await.unwrap();
    db.user(u).await.unwrap();
    sqlx::query("INSERT INTO economy_players(user_id,season,season_xp) VALUES($1,202611,500)")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        db.economy_action(u, "privacy-season", &A::Season { tier: 1 }, now())
            .await
            .is_err()
    );
    let dec = Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap();
    assert!(
        db.economy_action(u, "new-season", &A::Season { tier: 1 }, dec)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn auctions_escrow_refund_extend_and_settle_once() {
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
async fn overflow_rolls_back_claim_and_donations_are_idempotent_sinks() {
    let Some((db, u)) = setup().await else { return };
    let a = A::Donate {
        guild: (u + 10).to_string(),
        amount: 100,
    };
    let (a, b) = tokio::join!(
        db.economy_action(u, "donate", &a, now()),
        db.economy_action(u, "donate", &a, now())
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(db.economy_settings(u + 10).await.unwrap()["pool_total"], 100);
    ledger_matches(&db, u).await;
    act(&db, u, "tend", A::Tend).await;
    sqlx::query("UPDATE users SET credits=9223372036854775807 WHERE id=$1")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        db.economy_action(u, "overflow", &A::Provisions, now() + Duration::hours(12))
            .await
            .is_err()
    );
    assert_eq!(
        db.economy_snapshot(u, now()).await.unwrap()["player"]["provision_claimed"],
        0
    );
    // Restore the synthetic boundary balance so shared leaderboard fixtures stay valid.
    sqlx::query("UPDATE users SET credits=19900 WHERE id=$1")
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    ledger_matches(&db, u).await;
}
#[tokio::test]
async fn server_conditions_and_preview_reset_never_touch_shells() {
    let Some((db, u)) = setup().await else { return };
    let g = u + 10;
    sqlx::query("INSERT INTO members(guild_id,user_id,points) VALUES($1,$2,1001)")
        .bind(g as i64)
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.save_economy_settings(
        g,
        &EconomySettings {
            shop_enabled: false,
            ..Default::default()
        },
        0,
    )
    .await
    .unwrap();
    let it = db.add_store_item(g, u + 20, 1, None).await.unwrap().unwrap();
    assert_eq!(
        db.buy_store_item(g, u, it.id).await.unwrap(),
        pinchora_db::StoreBuy::Conditions
    );
    let preview = db.economy_points_preview(g).await.unwrap();
    db.economy_scale_points(g, 50, preview["version"].as_str().unwrap())
        .await
        .unwrap();
    assert!(
        db.economy_scale_points(g, 0, preview["version"].as_str().unwrap())
            .await
            .is_err()
    );
    assert_eq!(db.user(u).await.unwrap().credits, 20000);
    let fresh = db.economy_points_preview(g).await.unwrap();
    db.economy_scale_points(g, 0, fresh["version"].as_str().unwrap())
        .await
        .unwrap();
    assert_eq!(db.economy_points_preview(g).await.unwrap()["total"], "0");
}

async fn conserved(db: &Db, u: u64) {
    // The worker test may settle these auctions concurrently. Compare one MVCC snapshot,
    // never a ledger read before settlement with a balance read after it.
    let mut snapshot = db.pool().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *snapshot)
        .await
        .unwrap();
    let ids: Vec<i64> = (u..u + 4).map(|n| n as i64).collect();
    let total: i64 = sqlx::query_scalar("SELECT ((SELECT sum(credits)::numeric FROM users WHERE id=ANY($1)) + (SELECT COALESCE(sum(bid),0) FROM economy_auctions WHERE seller_id=ANY($1) AND status='open') + (SELECT COALESCE(sum((offer_a->>'shells')::bigint + CASE WHEN held_b THEN (offer_b->>'shells')::bigint ELSE 0 END),0) FROM economy_trades WHERE user_a=ANY($1) AND status='open') + (SELECT COALESCE(sum(delta),0) FROM economy_ledger WHERE user_id=ANY($1) AND asset='shells' AND account='sink'))::bigint")
        .bind(&ids).fetch_one(&mut *snapshot).await.unwrap();
    assert_eq!(total, 80_000, "wallets + escrow + sinks must conserve shells");
    for item in ["chair", "lamp"] {
        let total: i64 = sqlx::query_scalar("SELECT ((SELECT COALESCE(sum(quantity),0) FROM inventory WHERE user_id=ANY($1) AND item_id=$2) + (SELECT count(*) FROM economy_auctions WHERE seller_id=ANY($1) AND item_id=$2 AND status='open') + (SELECT COALESCE(sum((i->>'quantity')::bigint),0) FROM economy_trades t CROSS JOIN LATERAL jsonb_array_elements((offer_a->'items') || CASE WHEN held_b THEN offer_b->'items' ELSE '[]'::jsonb END) i WHERE user_a=ANY($1) AND status='open' AND i->>'item_id'=$2))::bigint")
            .bind(&ids).bind(item).fetch_one(&mut *snapshot).await.unwrap();
        assert_eq!(total, 40, "inventory + escrow must conserve {item}");
    }
    let broken: i64 = sqlx::query_scalar("SELECT count(*) FROM (SELECT movement FROM economy_ledger WHERE user_id=ANY($1) GROUP BY movement HAVING count(*)<>2 OR sum(delta)<>0 OR count(DISTINCT asset)<>1) p")
        .bind(&ids).fetch_one(&mut *snapshot).await.unwrap();
    assert_eq!(broken, 0, "every movement must have exactly one balanced pair");
    for n in u..u + 4 {
        let (credits, legacy): (i64, i64) = sqlx::query_as("SELECT credits,(SELECT COALESCE(sum(delta),0)::bigint FROM credit_ledger WHERE user_id=$1) FROM users WHERE id=$1")
            .bind(n as i64).fetch_one(&mut *snapshot).await.unwrap();
        assert_eq!(credits, legacy);
        let shells: i64 = sqlx::query_scalar("SELECT COALESCE(sum(delta),0)::bigint FROM economy_ledger WHERE user_id=$1 AND account='wallet' AND asset='shells'")
            .bind(n as i64).fetch_one(&mut *snapshot).await.unwrap();
        assert_eq!(shells + 20_000, credits);
        for item in ["chair", "lamp"] {
            let moved: i64 = sqlx::query_scalar("SELECT COALESCE(sum(delta),0)::bigint FROM economy_ledger WHERE user_id=$1 AND account='wallet' AND asset=$2")
                .bind(n as i64).bind(format!("item:{item}")).fetch_one(&mut *snapshot).await.unwrap();
            let quantity: i32 = sqlx::query_scalar(
                "SELECT COALESCE((SELECT quantity FROM inventory WHERE user_id=$1 AND item_id=$2),0)",
            )
            .bind(n as i64)
            .bind(item)
            .fetch_one(&mut *snapshot)
            .await
            .unwrap();
            assert_eq!(
                moved + 10,
                i64::from(quantity),
                "item movements must reconcile per wallet"
            );
        }
    }
    snapshot.commit().await.unwrap();
}

#[tokio::test]
async fn randomized_auction_trade_cancel_refund_sequences_conserve_assets() {
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
async fn worker_receipts_cannot_be_poisoned_and_leadership_releases_on_restart() {
    let Some((db, u)) = setup().await else { return };
    let id = act(
        &db,
        u,
        "create",
        A::Trade {
            to: (u + 1).to_string(),
            give: offer(100, ""),
            receive: offer(200, ""),
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    act(&db, u, &format!("expire:{id}"), A::Tend).await;
    assert!(
        db.economy_action(
            u,
            &format!("worker:expire:{id}"),
            &A::Provisions,
            now() + Duration::hours(12)
        )
        .await
        .is_err()
    );
    let mut leader = db.pool().begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('economy:sweep',0))")
        .execute(&mut *leader)
        .await
        .unwrap();
    db.sweep_economy(now() + Duration::hours(25)).await.unwrap();
    assert_eq!(db.user(u).await.unwrap().credits, 19900);
    leader.rollback().await.unwrap();
    let (a, b) = tokio::join!(
        db.sweep_economy(now() + Duration::hours(25)),
        db.sweep_economy(now() + Duration::hours(25))
    );
    a.unwrap();
    b.unwrap();
    db.sweep_economy(now() + Duration::hours(25)).await.unwrap();
    assert_eq!(db.user(u).await.unwrap().credits, 20000);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM credit_ledger WHERE user_id=$1 AND reason='trade_refund'")
        .bind(u as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn auction_net_proceeds_fit_even_when_gross_would_overflow() {
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
async fn invalid_amounts_and_inventory_overflow_leave_no_partial_moves() {
    let Some((db, u)) = setup().await else { return };
    for amount in [i64::MIN, -1, 0, i64::MAX] {
        for a in [
            A::Donate {
                guild: (u + 10).to_string(),
                amount,
            },
            A::Auction {
                item: "chair".into(),
                price: amount,
                hours: 1,
            },
        ] {
            assert!(db.economy_action(u, "invalid", &a, now()).await.is_err());
        }
    }
    item(&db, u, "chair", 1).await;
    item(&db, u + 1, "chair", i32::MAX).await;
    let id = act(
        &db,
        u,
        "trade",
        A::Trade {
            to: (u + 1).to_string(),
            give: offer(100, "chair"),
            receive: offer(200, ""),
        },
    )
    .await["id"]
        .as_i64()
        .unwrap();
    act(&db, u, "a", A::ConfirmTrade { id }).await;
    assert!(
        db.economy_action(u + 1, "b", &A::ConfirmTrade { id }, now())
            .await
            .is_err()
    );
    assert_eq!(db.user(u + 1).await.unwrap().credits, 20000);
    act(&db, u, "cancel", A::CancelTrade { id }).await;
    assert_eq!(db.user(u).await.unwrap().credits, 20000);
    assert_eq!(qty(&db, u, "chair").await, 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM economy_ledger WHERE user_id=$1")
        .bind((u + 1) as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn reset_preview_is_bound_to_the_guild() {
    let Some((db, u)) = setup().await else { return };
    for guild in [u + 10, u + 11] {
        sqlx::query("INSERT INTO members(guild_id,user_id,points) VALUES($1,$2,1000)")
            .bind(guild as i64)
            .bind(u as i64)
            .execute(db.pool())
            .await
            .unwrap();
    }
    let a = db.economy_points_preview(u + 10).await.unwrap();
    let b = db.economy_points_preview(u + 11).await.unwrap();
    assert_ne!(a["version"], b["version"]);
    assert!(
        db.economy_scale_points(u + 11, 0, a["version"].as_str().unwrap())
            .await
            .is_err()
    );
    assert_eq!(db.economy_points_preview(u + 11).await.unwrap()["total"], "1000");
}

#[tokio::test]
async fn anti_snipe_never_exceeds_hard_end_and_privacy_removes_ledger_pairs() {
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
        bid = pinchora_core::economy_v2::minimum_bid(bid);
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
async fn migration_0083_preserves_existing_balances_inventory_and_access_grants() {
    let Some((db, u)) = setup().await else { return };
    let mut tx = db.pool().begin().await.unwrap();
    let schema = format!("d8_migration_{u}");
    // A private transaction models the live pre-D8 tables; rollback removes the entire schema.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}; SET LOCAL search_path TO {schema}; CREATE TABLE users(id bigint PRIMARY KEY,credits bigint NOT NULL); CREATE TABLE members(guild_id bigint,user_id bigint,points bigint); CREATE TABLE inventory(user_id bigint,item_id text,quantity integer); INSERT INTO users VALUES(1,987654); INSERT INTO members VALUES(10,1,4321); INSERT INTO inventory VALUES(1,'chair',7);"))).execute(&mut *tx).await.unwrap();
    sqlx::raw_sql(include_str!("../../../migrations/0080_dashboard_managers.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dashboard_managers VALUES(10,20,ARRAY['configuration'])")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../../../migrations/0083_player_economy_v2.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let balances:(i64,i64,i32)=sqlx::query_as("SELECT credits,points,quantity FROM users JOIN members ON users.id=members.user_id JOIN inventory ON inventory.user_id=users.id").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(balances, (987654, 4321, 7));
    let areas: Vec<String> = sqlx::query_scalar("SELECT areas FROM dashboard_managers WHERE role_id=20")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(areas, vec!["configuration"]);
    sqlx::query("INSERT INTO dashboard_managers VALUES(10,21,ARRAY['economy'])")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn unchanged_settings_preserve_revision_and_active_market_survives_history() {
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

#[tokio::test]
async fn admin_audit_failure_rolls_back_settings_and_reset() {
    let Some((db, u)) = setup().await else { return };
    let actor = pinchora_db::economy_v2::EconomyAudit {
        actor: u,
        name: "invalid\0name",
        source: pinchora_db::AuditSource::Web,
    };
    assert!(
        db.save_economy_settings_audited(
            u + 10,
            &EconomySettings {
                shop_enabled: false,
                ..Default::default()
            },
            0,
            Some(&actor)
        )
        .await
        .is_err()
    );
    assert_eq!(db.economy_settings(u + 10).await.unwrap()["shop_enabled"], true);
    sqlx::query("INSERT INTO members(guild_id,user_id,points) VALUES($1,$2,1000)")
        .bind((u + 10) as i64)
        .bind(u as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let preview = db.economy_points_preview(u + 10).await.unwrap();
    assert!(
        db.economy_scale_points_audited(u + 10, 0, preview["version"].as_str().unwrap(), Some(&actor))
            .await
            .is_err()
    );
    assert_eq!(db.economy_points_preview(u + 10).await.unwrap()["total"], "1000");
    assert_eq!(db.user(u).await.unwrap().credits, 20000);
}
