use chrono::{DateTime, Utc};

use crate::{Db, sql_id};

/// A player's partner and when they paired up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartnerRow {
    pub partner_id: u64,
    pub since: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairOutcome {
    Paired {
        since: DateTime<Utc>,
    },
    /// This user already has a partner.
    Taken {
        user: u64,
    },
}

impl Db {
    /// Remembers the name `user` had just now, for lists and cards that show them.
    pub async fn remember_name(&self, user: u64, name: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO social_names (user_id, name) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE SET name = EXCLUDED.name",
        )
        .bind(sql_id(user))
        .bind(name)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The name last remembered for `user`.
    pub async fn remembered_name(&self, user: u64) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as("SELECT name FROM social_names WHERE user_id = $1")
            .bind(sql_id(user))
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map(|(name,)| name))
    }

    /// The player's partner, if they have one.
    pub async fn partner(&self, user: u64) -> anyhow::Result<Option<PartnerRow>> {
        let row: Option<(i64, DateTime<Utc>)> = sqlx::query_as(
            "SELECT CASE WHEN user_a = $1 THEN user_b ELSE user_a END, since
             FROM partners WHERE user_a = $1 OR user_b = $1",
        )
        .bind(sql_id(user))
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|(partner, since)| PartnerRow {
            partner_id: partner as u64,
            since,
        }))
    }

    /// The remembered name of the player's partner, for the profile card.
    pub async fn partner_name(&self, user: u64) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT n.name FROM partners p
             JOIN social_names n ON n.user_id = CASE WHEN p.user_a = $1 THEN p.user_b ELSE p.user_a END
             WHERE p.user_a = $1 OR p.user_b = $1",
        )
        .bind(sql_id(user))
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|(name,)| name))
    }

    /// Pairs two players up. The database refuses a second partner for either of them, even when
    /// two pairings race.
    pub async fn pair_partners(&self, a: u64, b: u64) -> anyhow::Result<PairOutcome> {
        anyhow::ensure!(a != b, "nobody partners themselves");
        let (low, high) = (a.min(b), a.max(b));
        let inserted: Result<(DateTime<Utc>,), sqlx::Error> =
            sqlx::query_as("INSERT INTO partners (user_a, user_b) VALUES ($1, $2) RETURNING since")
                .bind(sql_id(low))
                .bind(sql_id(high))
                .fetch_one(self.pool())
                .await;
        match inserted {
            Ok((since,)) => Ok(PairOutcome::Paired { since }),
            Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("23505") => {
                let user = if self.partner(a).await?.is_some() { a } else { b };
                Ok(PairOutcome::Taken { user })
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Ends the player's partnership; returns the partner they had, or `None` if they had none.
    pub async fn leave_partner(&self, user: u64) -> anyhow::Result<Option<PartnerRow>> {
        let row: Option<(i64, DateTime<Utc>)> = sqlx::query_as(
            "DELETE FROM partners WHERE user_a = $1 OR user_b = $1
             RETURNING CASE WHEN user_a = $1 THEN user_b ELSE user_a END, since",
        )
        .bind(sql_id(user))
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|(partner, since)| PartnerRow {
            partner_id: partner as u64,
            since,
        }))
    }

    /// What a pair's houses add up to: the likes on both, and the pieces standing in both.
    pub async fn pair_house_totals(&self, a: u64, b: u64) -> anyhow::Result<(i64, i64)> {
        let both = vec![sql_id(a), sql_id(b)];
        let (likes,): (i64,) =
            sqlx::query_as("SELECT COALESCE(sum(likes), 0)::BIGINT FROM houses WHERE user_id = ANY($1)")
                .bind(&both)
                .fetch_one(self.pool())
                .await?;
        let (pieces,): (i64,) = sqlx::query_as("SELECT count(*) FROM placements WHERE user_id = ANY($1)")
            .bind(&both)
            .fetch_one(self.pool())
            .await?;
        Ok((likes, pieces))
    }
}
