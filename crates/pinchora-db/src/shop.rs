use chrono::NaiveDate;

use crate::{Db, house::add_inventory, sql_id};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    Shells,
    Pearls,
}

#[derive(Debug, Clone, Copy)]
pub enum BuyOutcome {
    Bought { balance: i64 },
    Insufficient { balance: i64 },
    SoldOut,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MarketRow {
    pub item_id: String,
    pub price: i64,
    pub stock: i32,
    pub sold: i32,
}

#[derive(Debug, Clone, Copy)]
pub enum VisitOutcome {
    /// First visit to this host today; `reward` shells were paid (0 once the daily cap is hit).
    Rewarded {
        reward: i64,
    },
    AlreadyToday,
}

impl Db {
    /// Buys `quantity` of a catalog item at a fixed unit price.
    pub async fn buy_item(
        &self,
        user_id: u64,
        item_id: &str,
        unit_price: i64,
        currency: Currency,
        quantity: i32,
    ) -> anyhow::Result<BuyOutcome> {
        let id = sql_id(user_id);
        let total = unit_price * i64::from(quantity);
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let Some(balance) = charge(&mut tx, id, total, currency).await? else {
            let balance = balance_of(&mut tx, id, currency).await?;
            return Ok(BuyOutcome::Insufficient { balance });
        };
        add_inventory(&mut tx, id, item_id, quantity).await?;
        if currency == Currency::Shells {
            crate::economy::ledger(&mut tx, id, -total, "shop").await?;
        }
        tx.commit().await?;
        Ok(BuyOutcome::Bought { balance })
    }

    /// Opens an hour's market: creates its rows on first call, then returns them.
    pub async fn market(&self, hour: i64, offers: &[(String, i64, i32)]) -> anyhow::Result<Vec<MarketRow>> {
        for (item, price, stock) in offers {
            sqlx::query(
                "INSERT INTO market_stock (hour, item_id, price, stock) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (hour, item_id) DO NOTHING",
            )
            .bind(hour)
            .bind(item)
            .bind(price)
            .bind(stock)
            .execute(self.pool())
            .await?;
        }
        Ok(sqlx::query_as::<_, MarketRow>(
            "SELECT item_id, price, stock, sold FROM market_stock WHERE hour = $1 ORDER BY price DESC, item_id",
        )
        .bind(hour)
        .fetch_all(self.pool())
        .await?)
    }

    /// Buys one unit from the hourly market; stock and money move in one transaction.
    pub async fn market_buy(&self, user_id: u64, hour: i64, item_id: &str) -> anyhow::Result<BuyOutcome> {
        let id = sql_id(user_id);
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let price: Option<(i64,)> = sqlx::query_as(
            "UPDATE market_stock SET sold = sold + 1
             WHERE hour = $1 AND item_id = $2 AND sold < stock RETURNING price",
        )
        .bind(hour)
        .bind(item_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((price,)) = price else {
            return Ok(BuyOutcome::SoldOut);
        };
        let Some(balance) = charge(&mut tx, id, price, Currency::Shells).await? else {
            // Dropping the transaction rolls the stock back.
            let balance = balance_of(&mut tx, id, Currency::Shells).await?;
            return Ok(BuyOutcome::Insufficient { balance });
        };
        add_inventory(&mut tx, id, item_id, 1).await?;
        crate::economy::ledger(&mut tx, id, -price, "market").await?;
        tx.commit().await?;
        Ok(BuyOutcome::Bought { balance })
    }

    /// Logs a house visit and pays the guest `reward` for the first visit to this host
    /// today, up to `daily_cap` paid visits per day. Every visit stamps `visited_at` with
    /// the time of the latest one (the guestbook's "visited in the last day" and the recent
    /// visitors read it); only the first of the day pays.
    pub async fn record_visit(
        &self,
        host: u64,
        guest: u64,
        day: NaiveDate,
        reward: i64,
        daily_cap: i64,
    ) -> anyhow::Result<VisitOutcome> {
        let (host, guest) = (sql_id(host), sql_id(guest));
        let mut tx = self.pool().begin().await?;
        // `xmax` is 0 on a row this statement inserted and set on one it updated.
        let (fresh,): (bool,) = sqlx::query_as(
            "INSERT INTO visits (host_id, guest_id, day) VALUES ($1, $2, $3)
             ON CONFLICT (host_id, guest_id, day) DO UPDATE SET visited_at = now()
             RETURNING (xmax = 0)",
        )
        .bind(host)
        .bind(guest)
        .bind(day)
        .fetch_one(&mut *tx)
        .await?;
        if !fresh {
            tx.commit().await?;
            return Ok(VisitOutcome::AlreadyToday);
        }
        crate::activity::bump(&mut tx, guest, crate::Counter::Visits, 1, None).await?;
        let (today,): (i64,) = sqlx::query_as("SELECT count(*) FROM visits WHERE guest_id = $1 AND day = $2")
            .bind(guest)
            .bind(day)
            .fetch_one(&mut *tx)
            .await?;
        let paid = if today <= daily_cap { reward } else { 0 };
        if paid > 0 {
            sqlx::query("INSERT INTO users (id, credits) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET credits = users.credits + EXCLUDED.credits")
                .bind(guest)
                .bind(paid)
                .execute(&mut *tx)
                .await?;
            crate::economy::ledger(&mut tx, guest, paid, "visit").await?;
        }
        tx.commit().await?;
        Ok(VisitOutcome::Rewarded { reward: paid })
    }
}

/// Deducts `amount`; returns the new balance, or `None` if the player cannot afford it.
pub(crate) async fn charge(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
    amount: i64,
    currency: Currency,
) -> anyhow::Result<Option<i64>> {
    let sql = match currency {
        Currency::Shells => "UPDATE users SET credits = credits - $2 WHERE id = $1 AND credits >= $2 RETURNING credits",
        Currency::Pearls => "UPDATE users SET pearls = pearls - $2 WHERE id = $1 AND pearls >= $2 RETURNING pearls",
    };
    let row: Option<(i64,)> = sqlx::query_as(sql)
        .bind(user_id)
        .bind(amount)
        .fetch_optional(&mut **tx)
        .await?;
    Ok(row.map(|(b,)| b))
}

pub(crate) async fn balance_of(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
    currency: Currency,
) -> anyhow::Result<i64> {
    let sql = match currency {
        Currency::Shells => "SELECT credits FROM users WHERE id = $1",
        Currency::Pearls => "SELECT pearls FROM users WHERE id = $1",
    };
    let (b,): (i64,) = sqlx::query_as(sql).bind(user_id).fetch_one(&mut **tx).await?;
    Ok(b)
}
