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
    let sum: i64 = sqlx::query_scalar("SELECT COALESCE(sum(delta),0)::bigint FROM credit_ledger WHERE user_id=$1")
        .bind(u as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(sum, db.user(u).await.unwrap().credits);
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
