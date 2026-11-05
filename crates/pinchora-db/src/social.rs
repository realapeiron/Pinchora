use chrono::NaiveDate;

use crate::{Db, house::add_inventory, sql_id};

impl Db {
    /// Moves one item from `from`'s inventory to `to`'s. False if `from` has none.
    pub async fn gift_item(&self, from: u64, to: u64, item_id: &str) -> anyhow::Result<bool> {
        let (from, to) = (sql_id(from), sql_id(to));
        let mut tx = self.pool().begin().await?;
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
    /// if already liked today.
    pub async fn like_house(&self, host: u64, liker: u64, day: NaiveDate) -> anyhow::Result<Option<i32>> {
        let (host, liker) = (sql_id(host), sql_id(liker));
        let mut tx = self.pool().begin().await?;
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
        let (likes,): (i32,) = sqlx::query_as("UPDATE houses SET likes = likes + 1 WHERE user_id = $1 RETURNING likes")
            .bind(host)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(likes))
    }
}
