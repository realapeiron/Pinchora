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
    let u = crate::support::id();
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

mod market;
mod progression;
