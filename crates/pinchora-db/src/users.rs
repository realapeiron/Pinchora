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
    /// The id of the background on their cards (`users.background`); `None` is the plain look.
    pub background: Option<String>,
    /// The badge ids on their profile card, in order (`users.badges`).
    pub badges: Vec<String>,
    /// Whether they chose their badges. While false the card shows their best earned ones.
    pub badges_set: bool,
    /// Badges they had earned when they last opened the collection; `None` before the first time.
    pub badges_seen: Option<Vec<String>>,
    /// The id of the title under their name (`users.title`).
    pub title: Option<String>,
    /// The accent colour they typed or picked, `#rrggbb` (`users.accent`); `None` follows the background.
    pub accent: Option<String>,
    /// How see-through their card panels are: 0 light, 1 normal, 2 solid.
    pub panel_opacity: i16,
    /// The id of the frame around their cards (`users.frame`).
    pub frame: String,
    /// The fish, pet (its id) and furniture item (its id) on their profile card, and whether it
    /// shows their house.
    pub showcase_fish: Option<String>,
    pub showcase_pet: Option<i64>,
    pub showcase_item: Option<String>,
    pub showcase_house: bool,
}

impl Db {
    /// The player's row, created when missing.
    ///
    /// The insert waits for whoever holds the player's lock (see the `locks` module). When that was
    /// a transaction creating or re-creating the row, the insert finds it but the statement's
    /// snapshot, taken before the wait, does not show it: a statement of its own reads it then. A
    /// row a delete takes away in between is made again on the next go.
    pub async fn user(&self, id: u64) -> anyhow::Result<UserRow> {
        for _ in 0..3 {
            let row = sqlx::query_as::<_, UserRow>(
                "WITH ins AS (
                     INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING RETURNING *
                 )
                 SELECT * FROM ins UNION ALL SELECT * FROM users WHERE id = $1 LIMIT 1",
            )
            .bind(sql_id(id))
            .fetch_optional(self.pool())
            .await?;
            if let Some(row) = row {
                return Ok(row);
            }
            let row = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = $1")
                .bind(sql_id(id))
                .fetch_optional(self.pool())
                .await?;
            if let Some(row) = row {
                return Ok(row);
            }
        }
        anyhow::bail!("the row of player {id} kept disappearing")
    }

    /// 1-based position on the global XP leaderboard, which leaves out players who hide from it.
    pub async fn global_xp_rank(&self, xp: i64) -> anyhow::Result<i64> {
        let (above,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM users u WHERE u.xp > $1
             AND NOT EXISTS (SELECT 1 FROM user_privacy p WHERE p.user_id = u.id AND p.hide_leaderboard)",
        )
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
