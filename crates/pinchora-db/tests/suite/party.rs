//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! The money paths of the party games: stakes held and paid out once, refunds, daily limits, and
//! endings that race each other.

use chrono::{Duration, Utc};
use pinchora_db::{
    ACTIVE, CrewJoin, DONE, Db, DerbyJoin, DerbyLeave, DerbyOutcome, HeistSettled, NewGame, PENDING, ShellLimits,
    ShellPick, ShellStart, SurgeHelp,
};

/// The daily derby loss limit the tests pass to joins that are not about the limit.
const LOSS: i64 = 100_000;

use crate::support::db;

/// Ids unique per run, so the suite can run repeatedly against one database.
use crate::support::ids;

use crate::support::shells;

/// What the ledger says the given players hold in total, which must match their balances.
use crate::support::ledger_total;

async fn lobby(db: &Db, kind: &'static str, guild: u64, channel: u64, host: u64, bet: i64) -> i64 {
    db.open_party_game(NewGame {
        kind,
        guild_id: guild,
        channel_id: channel,
        host,
        guest: None,
        bet,
        status: PENDING,
        state: "7",
        ttl_secs: 600,
    })
    .await
    .unwrap()
    .expect("the channel is free")
    .id
}

async fn run_out(db: &Db, session: i64) {
    sqlx::query("UPDATE game_sessions SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(session)
        .execute(db.pool())
        .await
        .unwrap();
}

/// Runs the futures at the same time and collects what they return.
async fn futures_join<T, F: std::future::Future<Output = anyhow::Result<T>> + Send + 'static>(
    futures: impl IntoIterator<Item = F>,
) -> Vec<T>
where
    T: Send + 'static,
{
    let handles: Vec<_> = futures.into_iter().map(tokio::spawn).collect();
    let mut out = Vec::new();
    for handle in handles {
        out.push(handle.await.expect("task").expect("query"));
    }
    out
}

/// A player's rows in `party_daily` for `game` as (days ago, plays, net), newest first.
async fn daily_rows(db: &Db, user: u64, game: &str) -> Vec<(i32, i32, i64)> {
    let today = Utc::now().timestamp().div_euclid(86_400) as i32;
    let rows: Vec<(i32, i32, i64)> =
        sqlx::query_as("SELECT day, plays, net FROM party_daily WHERE user_id = $1 AND game = $2 ORDER BY day DESC")
            .bind(user as i64)
            .bind(game)
            .fetch_all(db.pool())
            .await
            .unwrap();
    rows.into_iter()
        .map(|(day, plays, net)| (today - day, plays, net))
        .collect()
}

/// Pretends the player put their stake in a day ago: their join, and the day's row it was counted on.
async fn joined_yesterday(db: &Db, session: i64, user: u64, game: &str) {
    sqlx::query("UPDATE party_members SET joined_at = joined_at - interval '1 day' WHERE session_id = $1")
        .bind(session)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE party_daily SET day = day - 1 WHERE user_id = $1 AND game = $2")
        .bind(user as i64)
        .bind(game)
        .execute(db.pool())
        .await
        .unwrap();
}

mod cooperative;
mod derby;
mod rewards;
