//! Shared, transaction-bound system rewards. Daily receipts survive deletion for the UTC day.
use crate::{Db, economy::ledger, locks::lock_users, sql_id};
use sqlx::{Postgres, Transaction};

pub(crate) async fn game_credit(
    tx: &mut Transaction<'_, Postgres>,
    user: i64,
    amount: i64,
    reason: &str,
) -> anyhow::Result<()> {
    if amount <= 0 {
        return Ok(());
    }
    sqlx::query("INSERT INTO users(id,credits) VALUES($1,$2) ON CONFLICT(id) DO UPDATE SET credits=users.credits+EXCLUDED.credits")
        .bind(user).bind(amount).execute(&mut **tx).await?;
    ledger(tx, user, amount, reason).await
}
pub(crate) async fn capped_win(
    tx: &mut Transaction<'_, Postgres>,
    user: i64,
    game: &str,
    amount: i64,
    cap: i64,
    reason: &str,
) -> anyhow::Result<i64> {
    if amount <= 0 {
        return Ok(0);
    }
    // Legacy deletions may have removed the ledger before migration could seed this counter.
    // A missing counter after today's deletion stays closed until tomorrow; existing counters
    // retain their exact remaining allowance through the ON CONFLICT branch.
    let wins: i64 = sqlx::query_scalar("INSERT INTO game_reward_daily(user_id,day,game,wins) VALUES($1,floor(extract(epoch FROM now())/86400)::int,$2,CASE WHEN EXISTS(SELECT 1 FROM deleted_users WHERE user_id=$1 AND floor(extract(epoch FROM deleted_at)/86400)=floor(extract(epoch FROM now())/86400)) THEN $3+1 ELSE 1 END) ON CONFLICT(user_id,day,game) DO UPDATE SET wins=game_reward_daily.wins+1 RETURNING wins")
        .bind(user).bind(game).bind(cap).fetch_one(&mut **tx).await?;
    let paid = if wins <= cap { amount } else { 0 };
    game_credit(tx, user, paid, reason).await?;
    Ok(paid)
}
pub(crate) async fn game_stat(
    tx: &mut Transaction<'_, Postgres>,
    user: i64,
    game: &str,
    won: bool,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO game_stats(user_id,game,played,won) VALUES($1,$2,1,$3) ON CONFLICT(user_id,game) DO UPDATE SET played=game_stats.played+1,won=game_stats.won+EXCLUDED.won")
        .bind(user).bind(game).bind(i32::from(won)).execute(&mut **tx).await?;
    Ok(())
}
impl Db {
    /// Both transports use this same durable cap, never a Redis-only reward counter.
    pub async fn capped_game_win(
        &self,
        user: u64,
        game: &str,
        amount: i64,
        cap: i64,
        reason: &str,
    ) -> anyhow::Result<i64> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let paid = capped_win(&mut tx, sql_id(user), game, amount, cap, reason).await?;
        tx.commit().await?;
        Ok(paid)
    }
}
