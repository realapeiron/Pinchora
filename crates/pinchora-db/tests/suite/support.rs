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

mod admission;

/// Field order closes the database session before returning its local admission slot.
#[must_use = "hold the guard until all fixture work and spawned tasks have finished"]
pub struct Guard {
    _connection: PgConnection,
    _admission: std::fs::File,
}

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

pub async fn shared() -> Option<Guard> {
    isolation(false).await
}

pub async fn exclusive() -> Option<Guard> {
    isolation(true).await
}

async fn isolation(exclusive: bool) -> Option<Guard> {
    let url = database_url()?;
    let (admission, lane) = admission::acquire().await;
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
    // The local permit bounds waiting sockets too. The PostgreSQL lane also bounds
    // active pools when independent runners on different hosts share a database.
    sqlx::query("SELECT pg_advisory_lock(724032, $1)")
        .bind(lane)
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
    Some(Guard {
        _connection: connection,
        _admission: admission,
    })
}

/// Allocate a disjoint fixture batch, reserving gaps for callers' related IDs.
pub fn spaced_ids<const N: usize>(spacing: u64) -> [u64; N] {
    assert!(
        spacing > 0 && N as u64 <= ID_STRIDE / spacing,
        "fixture ID batch is too large"
    );
    let base = id();
    std::array::from_fn(|i| base + i as u64 * spacing)
}

pub fn ids<const N: usize>() -> [u64; N] {
    spaced_ids(1)
}

pub async fn db(pool_size: u32) -> Option<Db> {
    let url = database_url()?;
    let db = Db::connect_waiting(&url, pool_size, std::time::Duration::from_secs(60))
        .await
        .expect("connect fixture database");
    db.migrate().await.expect("migrate fixture database");
    Some(db)
}

/// Only an absent URL skips database tests; malformed environment values must fail.
fn database_url() -> Option<String> {
    match std::env::var("PINCHORA_TEST_DATABASE_URL") {
        Ok(url) => Some(url),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => panic!("invalid test database URL: {error}"),
    }
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
