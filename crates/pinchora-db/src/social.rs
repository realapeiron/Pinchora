use chrono::NaiveDate;
use pinchora_core::items::is_soulbound;

use crate::{
    Db,
    house::add_inventory,
    locks::{lock_users, retry},
    sql_id,
};

impl Db {
    /// Moves one item from `from`'s inventory to `to`'s (a gift, or a donation to a plaza). False if `from`
    /// has none, or it is soulbound: those never change hands.
    pub async fn gift_item(&self, from: u64, to: u64, item_id: &str) -> anyhow::Result<bool> {
        if is_soulbound(item_id) {
            return Ok(false);
        }
        retry("gift_item", || self.gift_item_once(from, to, item_id)).await
    }

    /// One attempt of [`Db::gift_item`].
    async fn gift_item_once(&self, from: u64, to: u64, item_id: &str) -> anyhow::Result<bool> {
        let (from, to) = (sql_id(from), sql_id(to));
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[from, to]).await?;
        let taken = sqlx::query(
            "UPDATE inventory SET quantity = quantity - 1 WHERE user_id = $1 AND item_id = $2 AND quantity > 0",
        )
        .bind(from)
        .bind(item_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if taken == 0 {
            return Ok(false);
        }
        add_inventory(&mut tx, to, item_id, 1).await?;
        sqlx::query("INSERT INTO gifts (from_id, to_id, item_id) VALUES ($1, $2, $3)")
            .bind(from)
            .bind(to)
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// One like per host per liker per day; returns the host's new total, or `None`
    /// if already liked today (or the host has no house any more).
    pub async fn like_house(&self, host: u64, liker: u64, day: NaiveDate) -> anyhow::Result<Option<i32>> {
        retry("like_house", || self.like_house_once(host, liker, day)).await
    }

    /// One attempt of [`Db::like_house`].
    async fn like_house_once(&self, host: u64, liker: u64, day: NaiveDate) -> anyhow::Result<Option<i32>> {
        let (host, liker) = (sql_id(host), sql_id(liker));
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[host, liker]).await?;
        // A like given before the player deleted their data is still a like given today.
        let (before,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM deleted_users WHERE user_id = $1 AND day = $3 AND $2 = ANY(liked))",
        )
        .bind(liker)
        .bind(host)
        .bind(day)
        .fetch_one(&mut *tx)
        .await?;
        if before {
            return Ok(None);
        }
        let fresh =
            sqlx::query("INSERT INTO house_likes (host_id, liker_id, day) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
                .bind(host)
                .bind(liker)
                .bind(day)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                == 1;
        if !fresh {
            return Ok(None);
        }
        let likes: Option<(i32,)> =
            sqlx::query_as("UPDATE houses SET likes = likes + 1 WHERE user_id = $1 RETURNING likes")
                .bind(host)
                .fetch_optional(&mut *tx)
                .await?;
        // The host deleted their data since the house was looked up: dropping the transaction takes
        // the like back.
        let Some((likes,)) = likes else {
            return Ok(None);
        };
        // In id order, so two players liking each other's houses at once cannot deadlock.
        let mut counts = [
            (liker, crate::Counter::LikesGiven),
            (host, crate::Counter::LikesReceived),
        ];
        counts.sort_by_key(|(user, _)| *user);
        for (user, counter) in counts {
            crate::activity::bump(&mut tx, user, counter, 1, None).await?;
        }
        tx.commit().await?;
        Ok(Some(likes))
    }
}
