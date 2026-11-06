use chrono::{DateTime, Utc};
use pinchora_core::economy::{DAILY_COOLDOWN, REP_COOLDOWN, daily_reward, next_streak};

use crate::{
    Db,
    locks::{lock_users, retry},
    sql_id,
};

#[derive(Debug, Clone, Copy)]
pub enum DailyOutcome {
    Claimed { amount: i64, streak: i32, balance: i64 },
    Cooldown { next_at: DateTime<Utc> },
}

#[derive(Debug, Clone, Copy)]
pub enum TransferOutcome {
    Done { sender_balance: i64 },
    Insufficient { balance: i64 },
}

#[derive(Debug, Clone, Copy)]
pub enum RepOutcome {
    Given { target_total: i32 },
    Cooldown { next_at: DateTime<Utc> },
}

impl Db {
    pub async fn claim_daily(&self, user_id: u64) -> anyhow::Result<DailyOutcome> {
        retry("claim_daily", || self.claim_daily_once(user_id)).await
    }

    /// One attempt of [`Db::claim_daily`].
    async fn claim_daily_once(&self, user_id: u64) -> anyhow::Result<DailyOutcome> {
        let id = sql_id(user_id);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let (last, streak): (Option<DateTime<Utc>>, i32) =
            sqlx::query_as("SELECT last_daily_at, daily_streak FROM users WHERE id = $1 FOR UPDATE")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;

        let now = Utc::now();
        if let Some(last) = last
            && now - last < DAILY_COOLDOWN
        {
            return Ok(DailyOutcome::Cooldown {
                next_at: last + DAILY_COOLDOWN,
            });
        }

        let streak = next_streak(last, now, streak);
        let amount = daily_reward(streak);
        let (balance,): (i64,) = sqlx::query_as(
            "UPDATE users SET credits = credits + $2, daily_streak = $3, last_daily_at = $4
             WHERE id = $1 RETURNING credits",
        )
        .bind(id)
        .bind(amount)
        .bind(streak)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        ledger(&mut tx, id, amount, "daily").await?;
        tx.commit().await?;
        Ok(DailyOutcome::Claimed {
            amount,
            streak,
            balance,
        })
    }

    pub async fn transfer_credits(&self, from: u64, to: u64, amount: i64) -> anyhow::Result<TransferOutcome> {
        anyhow::ensure!(amount > 0 && from != to, "invalid transfer");
        retry("transfer_credits", || self.transfer_credits_once(from, to, amount)).await
    }

    /// One attempt of [`Db::transfer_credits`].
    async fn transfer_credits_once(&self, from: u64, to: u64, amount: i64) -> anyhow::Result<TransferOutcome> {
        let (from, to) = (sql_id(from), sql_id(to));
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[from, to]).await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1), ($2) ON CONFLICT (id) DO NOTHING")
            .bind(from)
            .bind(to)
            .execute(&mut *tx)
            .await?;
        // Lock both rows in id order so concurrent opposite transfers cannot deadlock.
        let rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT id, credits FROM users WHERE id = ANY($1) ORDER BY id FOR UPDATE")
                .bind(vec![from, to])
                .fetch_all(&mut *tx)
                .await?;
        let balance = rows.iter().find(|(id, _)| *id == from).map_or(0, |(_, c)| *c);
        if balance < amount {
            return Ok(TransferOutcome::Insufficient { balance });
        }
        sqlx::query("UPDATE users SET credits = credits - $2 WHERE id = $1")
            .bind(from)
            .bind(amount)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE users SET credits = credits + $2 WHERE id = $1")
            .bind(to)
            .bind(amount)
            .execute(&mut *tx)
            .await?;
        ledger(&mut tx, from, -amount, "transfer_out").await?;
        ledger(&mut tx, to, amount, "transfer_in").await?;
        tx.commit().await?;
        Ok(TransferOutcome::Done {
            sender_balance: balance - amount,
        })
    }

    pub async fn give_rep(&self, from: u64, to: u64) -> anyhow::Result<RepOutcome> {
        anyhow::ensure!(from != to, "cannot rep yourself");
        retry("give_rep", || self.give_rep_once(from, to)).await
    }

    /// One attempt of [`Db::give_rep`].
    async fn give_rep_once(&self, from: u64, to: u64) -> anyhow::Result<RepOutcome> {
        let (from, to) = (sql_id(from), sql_id(to));
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[from, to]).await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1), ($2) ON CONFLICT (id) DO NOTHING")
            .bind(from)
            .bind(to)
            .execute(&mut *tx)
            .await?;
        let (last,): (Option<DateTime<Utc>>,) =
            sqlx::query_as("SELECT last_rep_at FROM users WHERE id = $1 FOR UPDATE")
                .bind(from)
                .fetch_one(&mut *tx)
                .await?;
        let now = Utc::now();
        if let Some(last) = last
            && now - last < REP_COOLDOWN
        {
            return Ok(RepOutcome::Cooldown {
                next_at: last + REP_COOLDOWN,
            });
        }
        sqlx::query("UPDATE users SET last_rep_at = $2 WHERE id = $1")
            .bind(from)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        let (target_total,): (i32,) =
            sqlx::query_as("UPDATE users SET reputation = reputation + 1 WHERE id = $1 RETURNING reputation")
                .bind(to)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(RepOutcome::Given { target_total })
    }
}

pub(crate) async fn ledger(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
    delta: i64,
    reason: &str,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO credit_ledger (user_id, delta, reason) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(delta)
        .bind(reason)
        .execute(&mut **tx)
        .await?;
    let earned = crate::activity::earned(delta, reason);
    if earned > 0 {
        crate::activity::bump(tx, user_id, crate::activity::Counter::Shells, earned, None).await?;
    }
    Ok(())
}
