use crate::{Db, sql_id};

/// What a hug, pat, wave or high five adds up to once it is counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    /// How many of this kind the sender has sent this receiver, this one included.
    pub pair: i32,
    /// How many of this kind the sender has sent anyone.
    pub given: i64,
    /// How many of this kind the receiver has had from anyone.
    pub received: i64,
}

impl Db {
    /// Counts one interaction of `kind` from `from` to `to`.
    pub async fn record_interaction(&self, kind: &str, from: u64, to: u64) -> anyhow::Result<Tally> {
        let (from, to) = (sql_id(from), sql_id(to));
        let mut tx = self.pool().begin().await?;
        let (pair,): (i32,) = sqlx::query_as(
            "INSERT INTO interactions (kind, from_user, to_user, times) VALUES ($1, $2, $3, 1)
             ON CONFLICT (kind, from_user, to_user) DO UPDATE SET times = interactions.times + 1, last_at = now()
             RETURNING times",
        )
        .bind(kind)
        .bind(from)
        .bind(to)
        .fetch_one(&mut *tx)
        .await?;
        let (given,): (i64,) = sqlx::query_as(
            "SELECT COALESCE(sum(times), 0)::BIGINT FROM interactions WHERE kind = $1 AND from_user = $2",
        )
        .bind(kind)
        .bind(from)
        .fetch_one(&mut *tx)
        .await?;
        let (received,): (i64,) =
            sqlx::query_as("SELECT COALESCE(sum(times), 0)::BIGINT FROM interactions WHERE kind = $1 AND to_user = $2")
                .bind(kind)
                .bind(to)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(Tally { pair, given, received })
    }

    /// What two players have sent each other, per kind: `(kind, a to b, b to a)`.
    pub async fn exchanged(&self, a: u64, b: u64) -> anyhow::Result<Vec<(String, i64, i64)>> {
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT kind,
                    COALESCE(sum(times) FILTER (WHERE from_user = $1), 0)::BIGINT,
                    COALESCE(sum(times) FILTER (WHERE from_user = $2), 0)::BIGINT
             FROM interactions
             WHERE (from_user = $1 AND to_user = $2) OR (from_user = $2 AND to_user = $1)
             GROUP BY kind ORDER BY kind",
        )
        .bind(sql_id(a))
        .bind(sql_id(b))
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }
}
