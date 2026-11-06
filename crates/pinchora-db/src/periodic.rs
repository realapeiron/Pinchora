//! `/weekly` and `/monthly`: shell rewards with a cooldown of their own. The daily reward lives in
//! [`crate::economy`].

use chrono::{DateTime, Utc};
use pinchora_core::economy::Period;

use crate::{Db, privacy::lock_users, sql_id};

/// What a claim of a [`Period`] reward came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodOutcome {
    Claimed {
        amount: i64,
        balance: i64,
        next_at: DateTime<Utc>,
    },
    /// Collected too recently; ready again at `next_at`.
    Cooldown { next_at: DateTime<Utc> },
}

impl Db {
    /// Pays the reward of `period` unless it was collected less than a cooldown before `now`.
    /// The player's row is locked while the claim is worked out, so two claims at once pay once.
    pub async fn claim_period(&self, user: u64, period: Period, now: DateTime<Utc>) -> anyhow::Result<PeriodOutcome> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        let last: Option<(DateTime<Utc>,)> =
            sqlx::query_as("SELECT claimed_at FROM reward_claims WHERE user_id = $1 AND kind = $2")
                .bind(id)
                .bind(period.key())
                .fetch_optional(&mut *tx)
                .await?;
        if let Some((last,)) = last
            && now - last < period.cooldown()
        {
            return Ok(PeriodOutcome::Cooldown {
                next_at: last + period.cooldown(),
            });
        }
        sqlx::query(
            "INSERT INTO reward_claims (user_id, kind, claimed_at) VALUES ($1, $2, $3)
             ON CONFLICT (user_id, kind) DO UPDATE SET claimed_at = EXCLUDED.claimed_at",
        )
        .bind(id)
        .bind(period.key())
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let amount = period.reward();
        let (balance,): (i64,) =
            sqlx::query_as("UPDATE users SET credits = credits + $2 WHERE id = $1 RETURNING credits")
                .bind(id)
                .bind(amount)
                .fetch_one(&mut *tx)
                .await?;
        crate::economy::ledger(&mut tx, id, amount, period.key()).await?;
        tx.commit().await?;
        Ok(PeriodOutcome::Claimed {
            amount,
            balance,
            next_at: now + period.cooldown(),
        })
    }

    /// When the player last claimed `period`: `None` when they never did.
    pub async fn period_claimed_at(&self, user: u64, period: Period) -> anyhow::Result<Option<DateTime<Utc>>> {
        let row: Option<(DateTime<Utc>,)> =
            sqlx::query_as("SELECT claimed_at FROM reward_claims WHERE user_id = $1 AND kind = $2")
                .bind(sql_id(user))
                .bind(period.key())
                .fetch_optional(self.pool())
                .await?;
        Ok(row.map(|(t,)| t))
    }
}
