//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! The lock order of `pinchora_db`'s transactions (player locks first, then the object, then the
//! writes): the interleavings that used to end in a deadlock, and a mixed load with deletes that
//! must give no player an error and lose or make no shells.

use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use chrono::{Duration, NaiveDate, Utc};
use pinchora_core::economy::legacy::Period;
use pinchora_db::{
    ACTIVE, Award, BottlePrize, Commit, DROP_BOTTLE, DROP_CRITTERS, Daily, Db, DerbyJoin, DuelStart, GiveawayOpen,
    Grab, NewDrop, NewGame, NewGiveaway, NewParty, NewRoom, PENDING, Pay, Payout, PlayerChange, Reroll, ShellLimits,
    ShellPick, ShellStart, StallBuy, TripClaim, TripPayout, Wake, XpGrant,
};

use crate::support::db as db_with;

/// Ids unique per run, and larger than any earlier run's, so the suite can run repeatedly
/// against one database.
use crate::support::spaced_ids as ids;

fn duel(host: u64, guest: u64, bet: i64, guild: u64) -> NewGame<'static> {
    NewGame {
        kind: "connect4",
        guild_id: guild,
        channel_id: crate::support::id(),
        host,
        guest: Some(guest),
        bet,
        status: PENDING,
        state: "start",
        ttl_secs: 600,
    }
}

/// A connection of its own whose backend can be watched, so a test knows when the call it makes
/// is queued on a lock (other tests waiting on locks in the same database do not count).
async fn side() -> (Db, i32) {
    let db = db_with(1).await.expect("a test database");
    let (pid,): (i32,) = sqlx::query_as("SELECT pg_backend_pid()")
        .fetch_one(db.pool())
        .await
        .unwrap();
    (db, pid)
}

/// Waits until backend `pid` is waiting on a lock; panics after ten seconds.
async fn queued(db: &Db, pid: i32, what: &str) {
    for _ in 0..200 {
        let (waiting,): (bool,) = sqlx::query_as(
            "SELECT COALESCE(bool_or(wait_event_type = 'Lock'), FALSE) FROM pg_stat_activity WHERE pid = $1",
        )
        .bind(pid)
        .fetch_one(db.pool())
        .await
        .unwrap();
        if waiting {
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(50)).await;
    }
    panic!("{what} never queued on a lock");
}

async fn credits(db: &Db, user: u64) -> i64 {
    let row: Option<(i64,)> = sqlx::query_as("SELECT credits FROM users WHERE id = $1")
        .bind(user as i64)
        .fetch_optional(db.pool())
        .await
        .unwrap();
    row.map_or(0, |(c,)| c)
}

/// What the ledger says the player holds, which must be their balance.
async fn ledger(db: &Db, user: u64) -> i64 {
    let (sum,): (i64,) = sqlx::query_as("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM credit_ledger WHERE user_id = $1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    sum
}

/// Locks the row `sql` selects (with `id` bound) in a transaction of its own, left open.
async fn hold(db: &Db, sql: &'static str, id: i64) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = db.pool().begin().await.unwrap();
    sqlx::query(sql).bind(id).execute(&mut *tx).await.unwrap();
    tx
}

/// Runs `call` on its own connection while a blocker holds the row of `held` (`id` bound), then a
/// delete of `deleted` on another, waits until both queue, lets the blocker go and returns both
/// results. Before the rule this was a deadlock for every caller below.
async fn race_a_delete<T, F, Fut>(
    db: &Db,
    held: &'static str,
    id: i64,
    deleted: u64,
    call: F,
) -> (anyhow::Result<T>, anyhow::Result<pinchora_db::Deleted>)
where
    T: Send + 'static,
    F: FnOnce(Db) -> Fut,
    Fut: Future<Output = anyhow::Result<T>> + Send + 'static,
{
    let ((caller, caller_pid), (deleter, deleter_pid)) = (side().await, side().await);
    let blocker = hold(db, held, id).await;
    let called = tokio::spawn(call(caller));
    queued(db, caller_pid, "the call").await;
    let delete = tokio::spawn(async move { deleter.delete_user_data(deleted, Utc::now()).await });
    queued(db, deleter_pid, "the delete").await;
    blocker.commit().await.unwrap();
    (called.await.unwrap(), delete.await.unwrap())
}

// ---------------------------------------------------------------- the games

/// The daily limit of the tests: nothing here is about it.
const LOSS: i64 = 1_000_000;

fn lobby_of(
    kind: &'static str,
    guild: u64,
    channel: u64,
    host: u64,
    bet: i64,
    status: &'static str,
) -> NewGame<'static> {
    NewGame {
        kind,
        guild_id: guild,
        channel_id: channel,
        host,
        guest: None,
        bet,
        status,
        state: "7",
        ttl_secs: 600,
    }
}

// ---------------------------------------------------------------- mixed load

#[derive(Default, Clone)]
struct Tally {
    ok: u64,
    failed: u64,
    samples: Vec<String>,
}

const OPS: [&str; 24] = [
    "daily", "weekly", "rep", "transfer", "like", "visit", "xp", "duel", "giveaway", "delete", "wallet", "earn",
    "stall", "trip", "friend", "house", "sweep", "derby", "heist", "surge", "shells", "collect", "party", "contest",
];

mod mixed_load;
mod privacy_races;
mod sessions;
