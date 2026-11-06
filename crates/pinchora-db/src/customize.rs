use chrono::{DateTime, Utc};
use pinchora_core::{economy::DAILY_STREAK_WINDOW, leveling, stats::PlayerStats};

use crate::{
    Db,
    locks::{lock_users, retry},
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
    badges_notified: Option<Vec<String>>,
}

impl StatsRow {
    fn stats(&self) -> PlayerStats {
        let streak_alive = self
            .last_daily_at
            .is_some_and(|t| Utc::now() - t <= DAILY_STREAK_WINDOW);
        PlayerStats {
            level: i64::from(leveling::level_of(self.xp)),
            fish_species: self.fish_species,
            house_likes: self.house_likes,
            pets: self.pets,
            streak: if streak_alive { i64::from(self.daily_streak) } else { 0 },
            placed: self.placed,
            reputation: i64::from(self.reputation),
            shells: self.credits,
        }
    }
}

impl Db {
    /// One row of numbers, also for a player with no `users` row yet (a first pet needs none):
    /// theirs are zeros, and nothing has been announced to them.
    async fn stats_row(&self, user: u64) -> anyhow::Result<StatsRow> {
        Ok(sqlx::query_as::<_, StatsRow>(
            "SELECT COALESCE(u.xp, 0) AS xp, COALESCE(u.reputation, 0) AS reputation,
                    COALESCE(u.credits, 0) AS credits, COALESCE(u.daily_streak, 0) AS daily_streak,
                    u.last_daily_at,
                    CASE WHEN u.id IS NULL THEN '{}'::TEXT[] ELSE u.badges_notified END AS badges_notified,
                    (SELECT count(*) FROM fish_book WHERE user_id = q.id) AS fish_species,
                    COALESCE((SELECT likes FROM houses WHERE user_id = q.id), 0)::BIGINT AS house_likes,
                    (SELECT count(*) FROM pets WHERE owner_id = q.id) AS pets,
                    (SELECT count(*) FROM placements WHERE user_id = q.id) AS placed
             FROM (SELECT $1::BIGINT AS id) q LEFT JOIN users u ON u.id = q.id",
        )
        .bind(sql_id(user))
        .fetch_one(self.pool())
        .await?)
    }

    /// Every number achievement badges need, in one query.
    pub async fn player_stats(&self, user: u64) -> anyhow::Result<PlayerStats> {
        Ok(self.stats_row(user).await?.stats())
    }

    /// The same numbers with the badges already announced to the player (`None`: the list was
    /// never filled in, see [`Db::claim_badge_notices`]), still in one query.
    pub async fn badge_progress(&self, user: u64) -> anyhow::Result<(PlayerStats, Option<Vec<String>>)> {
        let row = self.stats_row(user).await?;
        Ok((row.stats(), row.badges_notified))
    }

    /// Records the badges in `earned` as announced and returns the ones that had not been, so
    /// two commands finishing together announce a badge once. A player whose list was never
    /// filled in (they existed before notices did) gets it filled with `earned` and hears
    /// nothing: those badges are old news.
    pub async fn claim_badge_notices(&self, user: u64, earned: &[String]) -> anyhow::Result<Vec<String>> {
        retry("claim_badge_notices", || self.claim_badge_notices_once(user, earned)).await
    }

    /// One attempt of [`Db::claim_badge_notices`].
    async fn claim_badge_notices_once(&self, user: u64, earned: &[String]) -> anyhow::Result<Vec<String>> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        // A player may have things to announce before they have a row (a first pet needs none).
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?;
        let (notified,): (Option<Vec<String>>,) =
            sqlx::query_as("SELECT badges_notified FROM users WHERE id = $1 FOR UPDATE")
                .bind(sql_id(user))
                .fetch_one(&mut *tx)
                .await?;
        let (fresh, all) = match notified {
            None => (Vec::new(), earned.to_vec()),
            Some(mut announced) => {
                let fresh: Vec<String> = earned.iter().filter(|b| !announced.contains(b)).cloned().collect();
                announced.extend(fresh.iter().cloned());
                (fresh, announced)
            }
        };
        sqlx::query("UPDATE users SET badges_notified = $2 WHERE id = $1")
            .bind(sql_id(user))
            .bind(&all)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(fresh)
    }

    /// Remembers which badges the player had when they opened the collection, for its NEW markers.
    pub async fn mark_badges_seen(&self, user: u64, earned: &[String]) -> anyhow::Result<()> {
        sqlx::query("UPDATE users SET badges_seen = $2 WHERE id = $1")
            .bind(sql_id(user))
            .bind(earned)
            .execute(self.pool())
            .await?;
        Ok(())
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

    pub(crate) async fn buy_cosmetic(
        &self,
        insert: &'static str,
        user: u64,
        id: &str,
        price: i64,
        reason: &str,
    ) -> anyhow::Result<CustomizeBuy> {
        anyhow::ensure!(price > 0, "a purchase needs a price");
        retry("buy_cosmetic", || {
            self.buy_cosmetic_once(insert, user, id, price, reason)
        })
        .await
    }

    /// One attempt of [`Db::buy_cosmetic`].
    async fn buy_cosmetic_once(
        &self,
        insert: &'static str,
        user: u64,
        id: &str,
        price: i64,
        reason: &str,
    ) -> anyhow::Result<CustomizeBuy> {
        let user = sql_id(user);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[user]).await?;
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

    /// Saves the badges on the profile card, in order, as the player's own choice: an empty
    /// list now means "no badges". The caller has checked that the player may wear each of
    /// them; this keeps the list short and free of repeats.
    pub async fn set_badges(&self, user: u64, badges: &[String]) -> anyhow::Result<()> {
        anyhow::ensure!(badges.len() <= MAX_BADGES, "at most {MAX_BADGES} badges");
        anyhow::ensure!(
            badges.iter().enumerate().all(|(i, b)| !badges[..i].contains(b)),
            "a badge can be worn once"
        );
        sqlx::query(
            "INSERT INTO users (id, badges, badges_set) VALUES ($1, $2, TRUE)
             ON CONFLICT (id) DO UPDATE SET badges = EXCLUDED.badges, badges_set = TRUE",
        )
        .bind(sql_id(user))
        .bind(badges)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Goes back to showing the player's best earned badges on their own.
    pub async fn reset_badges(&self, user: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE users SET badges = '{}', badges_set = FALSE WHERE id = $1")
            .bind(sql_id(user))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Puts a title under the player's name, or none. The caller has checked that it is theirs.
    pub async fn set_title(&self, user: u64, title: Option<&str>) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO users (id, title) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title",
        )
        .bind(sql_id(user))
        .bind(title)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Title ids granted to the player outside the stats (`user_titles`), oldest first.
    pub async fn granted_titles(&self, user: u64) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT title_id FROM user_titles WHERE user_id = $1 ORDER BY granted_at, title_id")
                .bind(sql_id(user))
                .fetch_all(self.pool())
                .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Grants a title the stats cannot give, e.g. the house of the week. False if they had it.
    pub async fn grant_title(&self, user: u64, title: &str) -> anyhow::Result<bool> {
        Ok(
            sqlx::query("INSERT INTO user_titles (user_id, title_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                .bind(sql_id(user))
                .bind(title)
                .execute(self.pool())
                .await?
                .rows_affected()
                == 1,
        )
    }
}
