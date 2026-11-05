use chrono::{DateTime, Utc};

use crate::{Db, sql_id};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: i64,
    pub credits: i64,
    pub xp: i64,
    pub reputation: i32,
    pub daily_streak: i32,
    pub last_daily_at: Option<DateTime<Utc>>,
    pub last_rep_at: Option<DateTime<Utc>>,
    pub bio: String,
    pub accent_color: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub pearls: i64,
    /// The saved character code (`users.avatar`); `None` until they design one.
    #[sqlx(rename = "avatar")]
    pub character: Option<String>,
}

impl Db {
    pub async fn user(&self, id: u64) -> anyhow::Result<UserRow> {
        let row = sqlx::query_as::<_, UserRow>(
            "WITH ins AS (
                 INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING RETURNING *
             )
             SELECT * FROM ins UNION ALL SELECT * FROM users WHERE id = $1 LIMIT 1",
        )
        .bind(sql_id(id))
        .fetch_one(self.pool())
        .await?;
        Ok(row)
    }

    /// 1-based position on the global XP leaderboard.
    pub async fn global_xp_rank(&self, xp: i64) -> anyhow::Result<i64> {
        let (above,): (i64,) = sqlx::query_as("SELECT count(*) FROM users WHERE xp > $1")
            .bind(xp)
            .fetch_one(self.pool())
            .await?;
        Ok(above + 1)
    }

    pub async fn set_bio(&self, id: u64, bio: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO users (id, bio) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET bio = EXCLUDED.bio",
        )
        .bind(sql_id(id))
        .bind(bio)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The saved character code, or `None` if the player never designed one (or is unknown).
    pub async fn character(&self, id: u64) -> anyhow::Result<Option<String>> {
        let row: Option<(Option<String>,)> = sqlx::query_as("SELECT avatar FROM users WHERE id = $1")
            .bind(sql_id(id))
            .fetch_optional(self.pool())
            .await?;
        Ok(row.and_then(|(code,)| code))
    }

    pub async fn set_character(&self, id: u64, code: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO users (id, avatar) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET avatar = EXCLUDED.avatar",
        )
        .bind(sql_id(id))
        .bind(code)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn set_accent_color(&self, id: u64, color: Option<i32>) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO users (id, accent_color) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET accent_color = EXCLUDED.accent_color",
        )
        .bind(sql_id(id))
        .bind(color)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
