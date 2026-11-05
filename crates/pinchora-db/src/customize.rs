use chrono::{DateTime, Utc};
use pinchora_core::{economy::DAILY_STREAK_WINDOW, leveling};

use crate::{
    Db,
    shop::{Currency, balance_of, charge},
    sql_id,
};

/// Most badges a player wears on their profile card.
pub const MAX_BADGES: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomizeBuy {
    Bought { balance: i64 },
    AlreadyOwned,
    Insufficient { balance: i64 },
}

/// The numbers achievement badges are earned from. All zero for a player never seen before.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerStats {
    /// From their total XP.
    pub level: i64,
    /// Different fish in the fish book.
    pub fish_species: i64,
    /// Likes their house has received.
    pub house_likes: i64,
    pub pets: i64,
    /// The daily streak as it stands now: 0 once they missed the window to extend it.
    pub streak: i64,
    /// Furniture standing in their rooms.
    pub placed: i64,
    pub reputation: i64,
    pub shells: i64,
}

#[derive(sqlx::FromRow)]
struct StatsRow {
    xp: i64,
    reputation: i32,
    credits: i64,
    daily_streak: i32,
    last_daily_at: Option<DateTime<Utc>>,
    fish_species: i64,
    house_likes: i64,
    pets: i64,
    placed: i64,
}

impl Db {
    /// Every number achievement badges need, in one query.
    pub async fn player_stats(&self, user: u64) -> anyhow::Result<PlayerStats> {
        let row = sqlx::query_as::<_, StatsRow>(
            "SELECT u.xp, u.reputation, u.credits, u.daily_streak, u.last_daily_at,
                    (SELECT count(*) FROM fish_book WHERE user_id = u.id) AS fish_species,
                    COALESCE((SELECT likes FROM houses WHERE user_id = u.id), 0)::BIGINT AS house_likes,
                    (SELECT count(*) FROM pets WHERE owner_id = u.id) AS pets,
                    (SELECT count(*) FROM placements WHERE user_id = u.id) AS placed
             FROM users u WHERE u.id = $1",
        )
        .bind(sql_id(user))
        .fetch_optional(self.pool())
        .await?;
        let Some(row) = row else {
            return Ok(PlayerStats::default());
        };
        let streak_alive = row.last_daily_at.is_some_and(|t| Utc::now() - t <= DAILY_STREAK_WINDOW);
        Ok(PlayerStats {
            level: i64::from(leveling::level_of(row.xp)),
            fish_species: row.fish_species,
            house_likes: row.house_likes,
            pets: row.pets,
            streak: if streak_alive { i64::from(row.daily_streak) } else { 0 },
            placed: row.placed,
            reputation: i64::from(row.reputation),
            shells: row.credits,
        })
    }

    /// Background ids the player bought, oldest first.
    pub async fn owned_backgrounds(&self, user: u64) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT background_id FROM user_backgrounds WHERE user_id = $1 ORDER BY bought_at, background_id",
        )
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Shop badge ids the player bought, oldest first.
    pub async fn owned_badges(&self, user: u64) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT badge_id FROM user_badges WHERE user_id = $1 ORDER BY bought_at, badge_id")
                .bind(sql_id(user))
                .fetch_all(self.pool())
                .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Buys a background at `price` shells: the shells, the ledger entry and the new
    /// ownership happen together, or not at all.
    pub async fn buy_background(&self, user: u64, background_id: &str, price: i64) -> anyhow::Result<CustomizeBuy> {
        self.buy_cosmetic(
            "INSERT INTO user_backgrounds (user_id, background_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            user,
            background_id,
            price,
            "background",
        )
        .await
    }

    /// Buys a shop badge at `price` shells, like [`Db::buy_background`].
    pub async fn buy_badge(&self, user: u64, badge_id: &str, price: i64) -> anyhow::Result<CustomizeBuy> {
        self.buy_cosmetic(
            "INSERT INTO user_badges (user_id, badge_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            user,
            badge_id,
            price,
            "badge",
        )
        .await
    }

    async fn buy_cosmetic(
        &self,
        insert: &'static str,
        user: u64,
        id: &str,
        price: i64,
        reason: &str,
    ) -> anyhow::Result<CustomizeBuy> {
        anyhow::ensure!(price > 0, "a purchase needs a price");
        let user = sql_id(user);
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(user)
            .execute(&mut *tx)
            .await?;
        // The ownership row goes first: of two clicks at once, the second waits here and finds it taken.
        let fresh = sqlx::query(insert)
            .bind(user)
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if !fresh {
            return Ok(CustomizeBuy::AlreadyOwned);
        }
        let Some(balance) = charge(&mut tx, user, price, Currency::Shells).await? else {
            // Dropping the transaction takes the ownership row back.
            let balance = balance_of(&mut tx, user, Currency::Shells).await?;
            return Ok(CustomizeBuy::Insufficient { balance });
        };
        crate::economy::ledger(&mut tx, user, -price, reason).await?;
        tx.commit().await?;
        Ok(CustomizeBuy::Bought { balance })
    }

    /// Puts an owned background on the player's cards. False when they do not own it.
    pub async fn equip_background(&self, user: u64, background_id: &str) -> anyhow::Result<bool> {
        let equipped = sqlx::query(
            "UPDATE users SET background = $2
             WHERE id = $1 AND EXISTS (SELECT 1 FROM user_backgrounds WHERE user_id = $1 AND background_id = $2)",
        )
        .bind(sql_id(user))
        .bind(background_id)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(equipped == 1)
    }

    /// Goes back to the plain card look.
    pub async fn remove_background(&self, user: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE users SET background = NULL WHERE id = $1")
            .bind(sql_id(user))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Saves the badges on the profile card, in order. The caller has checked that the
    /// player may wear each of them; this keeps the list short and free of repeats.
    pub async fn set_badges(&self, user: u64, badges: &[String]) -> anyhow::Result<()> {
        anyhow::ensure!(badges.len() <= MAX_BADGES, "at most {MAX_BADGES} badges");
        anyhow::ensure!(
            badges.iter().enumerate().all(|(i, b)| !badges[..i].contains(b)),
            "a badge can be worn once"
        );
        sqlx::query(
            "INSERT INTO users (id, badges) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET badges = EXCLUDED.badges",
        )
        .bind(sql_id(user))
        .bind(badges)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
