//! Shared fixture connections and cross-process isolation.
//!
//! Every database test holds a shared lock for its entire lifetime. Tests that scan,
//! claim or delete global state hold the exclusive form of the same lock instead.
//! Keep the guard in the test, before fixture creation, so spawned races stay concurrent
//! and additional pools do not reacquire it. A dedicated connection releases the lock
//! even on panic and never takes a slot from a deliberately small application pool.
use std::sync::atomic::{AtomicU64, Ordering};

use pinchora_db::Db;
use sqlx::PgConnection;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
const ID_STRIDE: u64 = 65_536;
const ID_RANGE: u64 = 1_u64 << 32;

/// Reserve room for local fixture offsets, including thousand-element ID batches.
/// Database tests first lease a disjoint range from PostgreSQL; the initial range
/// is only used by the synchronous allocator test, which writes no database rows.
pub fn id() -> u64 {
    NEXT_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            // Leave the range boundary unused so exhaustion cannot silently cross
            // into a range leased to a different nextest process.
            (next % ID_RANGE < ID_RANGE - ID_STRIDE).then_some(next + ID_STRIDE)
        })
        .expect("fixture ID range exhausted")
}

pub async fn shared() -> Option<PgConnection> {
    isolation(false).await
}

pub async fn exclusive() -> Option<PgConnection> {
    isolation(true).await
}

async fn isolation(exclusive: bool) -> Option<PgConnection> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    // Use the same bounded acquisition as the application fixtures: PostgreSQL may
    // still be reclaiming sockets from a test whose runtime just shut down. Detach
    // the connection so session locks can never leak back into a reusable pool.
    let lock_db = Db::connect_waiting(&url, 1, std::time::Duration::from_secs(60))
        .await
        .expect("fixture lock connection");
    let mut connection = lock_db.pool().acquire().await.expect("fixture lock socket").detach();
    lock_db.pool().close().await;
    // DDL is serialized independently of the fixture lock. Each lease has space for
    // 65,534 allocations; fetching a newer lease cannot overlap an older allocation.
    sqlx::raw_sql(
        "BEGIN; SELECT pg_advisory_xact_lock(724031, 2); CREATE SEQUENCE IF NOT EXISTS pinchora_test_fixture_ids",
    )
    .execute(&mut connection)
    .await
    .expect("fixture sequence");
    let lease: i64 = sqlx::query_scalar("SELECT nextval('pinchora_test_fixture_ids')")
        .fetch_one(&mut connection)
        .await
        .expect("fixture ID lease");
    sqlx::query("COMMIT")
        .execute(&mut connection)
        .await
        .expect("commit ID lease");
    NEXT_ID.fetch_max((1_u64 << 60) + (lease as u64) * ID_RANGE + ID_STRIDE, Ordering::Relaxed);
    // Nextest starts a process per test. Bound active fixture pools before opening
    // them, while preserving all concurrency inside each test. PostgreSQL queues
    // these two lanes across processes; no runner configuration or retry is needed.
    sqlx::query("SELECT pg_advisory_lock(724032, $1)")
        .bind((lease % 2) as i32)
        .execute(&mut connection)
        .await
        .expect("fixture connection budget");
    let query = if exclusive {
        "SELECT pg_advisory_lock(724031, 1)"
    } else {
        "SELECT pg_advisory_lock_shared(724031, 1)"
    };
    sqlx::query(query)
        .execute(&mut connection)
        .await
        .expect("fixture isolation lock");
    Some(connection)
}

pub fn ids_seven<const N: usize>() -> [u64; N] {
    assert!(N as u64 <= ID_STRIDE / 7, "fixture ID batch is too large");
    let base = crate::support::id();
    std::array::from_fn(|i| base + i as u64 * 7)
}

pub async fn db_eight() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 8, std::time::Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}

pub async fn db_five() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 5, std::time::Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}

pub fn ids<const N: usize>() -> [u64; N] {
    assert!(N as u64 <= ID_STRIDE, "fixture ID batch is too large");
    let base = crate::support::id();
    std::array::from_fn(|i| base + i as u64)
}

pub async fn db_ten() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 10, std::time::Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}

pub async fn db_eight_unwrap() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 8, std::time::Duration::from_secs(60))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    Some(db)
}
pub async fn shells(db: &Db, user: u64) -> i64 {
    db.user(user).await.unwrap().credits
}

pub async fn ledger_total(db: &Db, users: &[u64]) -> i64 {
    let ids: Vec<i64> = users.iter().map(|u| *u as i64).collect();
    let (sum,): (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM credit_ledger WHERE user_id = ANY($1)")
            .bind(ids)
            .fetch_one(db.pool())
            .await
            .unwrap();
    sum
}
