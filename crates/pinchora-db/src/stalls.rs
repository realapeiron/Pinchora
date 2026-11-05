use crate::{Db, house::add_inventory, sql_id};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ListingRow {
    pub id: i64,
    pub seller_id: i64,
    pub item_id: String,
    pub price: i64,
}

#[derive(Debug, Clone)]
pub enum StallBuy {
    Bought {
        item_id: String,
        price: i64,
        seller: i64,
        balance: i64,
    },
    Gone,
    OwnListing,
    Insufficient {
        price: i64,
        balance: i64,
    },
}

/// Percent of each sale that leaves the economy.
pub const STALL_FEE_PERCENT: i64 = 5;
pub const MAX_LISTINGS: i64 = 8;

impl Db {
    pub async fn listings(&self, seller: u64) -> anyhow::Result<Vec<ListingRow>> {
        Ok(sqlx::query_as::<_, ListingRow>(
            "SELECT id, seller_id, item_id, price FROM stall_listings WHERE seller_id = $1 ORDER BY listed_at",
        )
        .bind(sql_id(seller))
        .fetch_all(self.pool())
        .await?)
    }

    /// Moves one item from inventory onto the seller's stall. `Ok(false)` if not owned or the stall is full.
    pub async fn list_item(&self, seller: u64, item_id: &str, price: i64) -> anyhow::Result<bool> {
        let id = sql_id(seller);
        let mut tx = self.pool().begin().await?;
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM stall_listings WHERE seller_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if count >= MAX_LISTINGS {
            return Ok(false);
        }
        let taken = sqlx::query(
            "UPDATE inventory SET quantity = quantity - 1 WHERE user_id = $1 AND item_id = $2 AND quantity > 0",
        )
        .bind(id)
        .bind(item_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if taken == 0 {
            return Ok(false);
        }
        sqlx::query("INSERT INTO stall_listings (seller_id, item_id, price) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(item_id)
            .bind(price)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Takes a listing back into the seller's inventory.
    pub async fn unlist_item(&self, seller: u64, listing: i64) -> anyhow::Result<Option<String>> {
        let id = sql_id(seller);
        let mut tx = self.pool().begin().await?;
        let row: Option<(String,)> =
            sqlx::query_as("DELETE FROM stall_listings WHERE id = $1 AND seller_id = $2 RETURNING item_id")
                .bind(listing)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((item_id,)) = row else {
            return Ok(None);
        };
        add_inventory(&mut tx, id, &item_id, 1).await?;
        tx.commit().await?;
        Ok(Some(item_id))
    }

    /// Buys a listing: the listing row is deleted first, so two buyers can never both get it.
    pub async fn buy_listing(&self, buyer: u64, listing: i64) -> anyhow::Result<StallBuy> {
        let buyer = sql_id(buyer);
        let mut tx = self.pool().begin().await?;
        let row: Option<ListingRow> =
            sqlx::query_as("DELETE FROM stall_listings WHERE id = $1 RETURNING id, seller_id, item_id, price")
                .bind(listing)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(row) = row else {
            return Ok(StallBuy::Gone);
        };
        if row.seller_id == buyer {
            return Ok(StallBuy::OwnListing);
        }
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(buyer)
            .execute(&mut *tx)
            .await?;
        let paid: Option<(i64,)> =
            sqlx::query_as("UPDATE users SET credits = credits - $2 WHERE id = $1 AND credits >= $2 RETURNING credits")
                .bind(buyer)
                .bind(row.price)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((balance,)) = paid else {
            let (balance,): (i64,) = sqlx::query_as("SELECT credits FROM users WHERE id = $1")
                .bind(buyer)
                .fetch_one(&mut *tx)
                .await?;
            // Rolling back restores the listing.
            return Ok(StallBuy::Insufficient {
                price: row.price,
                balance,
            });
        };
        let payout = row.price - row.price * STALL_FEE_PERCENT / 100;
        sqlx::query("UPDATE users SET credits = credits + $2 WHERE id = $1")
            .bind(row.seller_id)
            .bind(payout)
            .execute(&mut *tx)
            .await?;
        crate::economy::ledger(&mut tx, buyer, -row.price, "stall_buy").await?;
        crate::economy::ledger(&mut tx, row.seller_id, payout, "stall_sale").await?;
        add_inventory(&mut tx, buyer, &row.item_id, 1).await?;
        tx.commit().await?;
        Ok(StallBuy::Bought {
            item_id: row.item_id,
            price: row.price,
            seller: row.seller_id,
            balance,
        })
    }
}
