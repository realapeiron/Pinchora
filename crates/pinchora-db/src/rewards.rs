use crate::{
    Db,
    locks::{lock_users, retry},
    sql_id,
};

/// Most level roles a server can set up.
pub const MAX_LEVEL_ROLES: i64 = 25;
/// Most roles a server can sell.
pub const MAX_STORE_ITEMS: i64 = 25;
/// Most reputation roles a server can set up.
pub const MAX_REP_ROLES: i64 = 25;

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct LevelRoleRow {
    pub level: i32,
    pub role_id: i64,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct RepRoleRow {
    pub reputation: i32,
    pub role_id: i64,
}

#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct StoreItem {
    pub id: i64,
    pub role_id: i64,
    pub price: i64,
    /// `None` is unlimited.
    pub stock: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreBuy {
    Conditions,
    /// Points were taken and one unit of stock claimed; the caller now gives the role.
    Bought {
        role_id: i64,
        price: i64,
        balance: i64,
    },
    Gone,
    SoldOut,
    Insufficient {
        price: i64,
        balance: i64,
    },
}

impl Db {
    pub async fn level_roles(&self, guild_id: u64) -> anyhow::Result<Vec<LevelRoleRow>> {
        Ok(sqlx::query_as::<_, LevelRoleRow>(
            "SELECT level, role_id FROM level_roles WHERE guild_id = $1 ORDER BY level, role_id",
        )
        .bind(sql_id(guild_id))
        .fetch_all(self.pool())
        .await?)
    }

    /// Sets the level a role is given at. `Ok(false)` when the server is at its limit.
    pub async fn add_level_role(&self, guild_id: u64, level: i32, role_id: u64) -> anyhow::Result<bool> {
        let added = sqlx::query(
            "INSERT INTO level_roles (guild_id, level, role_id)
             SELECT $1, $2, $3
             WHERE (SELECT count(*) FROM level_roles WHERE guild_id = $1) < $4
                OR EXISTS (SELECT 1 FROM level_roles WHERE guild_id = $1 AND role_id = $3)
             ON CONFLICT (guild_id, role_id) DO UPDATE SET level = EXCLUDED.level",
        )
        .bind(sql_id(guild_id))
        .bind(level)
        .bind(sql_id(role_id))
        .bind(MAX_LEVEL_ROLES)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(added > 0)
    }

    pub async fn remove_level_role(&self, guild_id: u64, role_id: u64) -> anyhow::Result<bool> {
        let removed = sqlx::query("DELETE FROM level_roles WHERE guild_id = $1 AND role_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(role_id))
            .execute(self.pool())
            .await?
            .rows_affected();
        Ok(removed > 0)
    }

    pub async fn rep_roles(&self, guild_id: u64) -> anyhow::Result<Vec<RepRoleRow>> {
        Ok(sqlx::query_as::<_, RepRoleRow>(
            "SELECT reputation, role_id FROM rep_roles WHERE guild_id = $1 ORDER BY reputation, role_id",
        )
        .bind(sql_id(guild_id))
        .fetch_all(self.pool())
        .await?)
    }

    /// Sets the reputation a role is given at. `Ok(false)` when the server is at its limit.
    pub async fn add_rep_role(&self, guild_id: u64, reputation: i32, role_id: u64) -> anyhow::Result<bool> {
        let added = sqlx::query(
            "INSERT INTO rep_roles (guild_id, reputation, role_id)
             SELECT $1, $2, $3
             WHERE (SELECT count(*) FROM rep_roles WHERE guild_id = $1) < $4
                OR EXISTS (SELECT 1 FROM rep_roles WHERE guild_id = $1 AND role_id = $3)
             ON CONFLICT (guild_id, role_id) DO UPDATE SET reputation = EXCLUDED.reputation",
        )
        .bind(sql_id(guild_id))
        .bind(reputation)
        .bind(sql_id(role_id))
        .bind(MAX_REP_ROLES)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(added > 0)
    }

    pub async fn remove_rep_role(&self, guild_id: u64, role_id: u64) -> anyhow::Result<bool> {
        let removed = sqlx::query("DELETE FROM rep_roles WHERE guild_id = $1 AND role_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(role_id))
            .execute(self.pool())
            .await?
            .rows_affected();
        Ok(removed > 0)
    }

    /// Drops a role that no longer exists on Discord from the rewards, the store, the role panels,
    /// the XP rules and the join role.
    pub async fn forget_role(&self, guild_id: u64, role_id: u64) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM level_roles WHERE guild_id = $1 AND role_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(role_id))
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM voice_level_roles WHERE guild_id = $1 AND role_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(role_id))
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM rep_roles WHERE guild_id = $1 AND role_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(role_id))
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM xp_rules WHERE guild_id = $1 AND kind = 'role' AND target_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(role_id))
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM store_items WHERE guild_id = $1 AND role_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(role_id))
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "DELETE FROM role_panel_roles
             WHERE role_id = $2 AND panel_id IN (SELECT id FROM role_panels WHERE guild_id = $1)",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(role_id))
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE guilds SET join_role_id = NULL WHERE id = $1 AND join_role_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(role_id))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Cheapest first.
    pub async fn store_items(&self, guild_id: u64) -> anyhow::Result<Vec<StoreItem>> {
        Ok(sqlx::query_as::<_, StoreItem>(
            "SELECT id, role_id, price, stock FROM store_items WHERE guild_id = $1 ORDER BY price, id",
        )
        .bind(sql_id(guild_id))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn store_item(&self, guild_id: u64, item_id: i64) -> anyhow::Result<Option<StoreItem>> {
        Ok(sqlx::query_as::<_, StoreItem>(
            "SELECT id, role_id, price, stock FROM store_items WHERE guild_id = $1 AND id = $2",
        )
        .bind(sql_id(guild_id))
        .bind(item_id)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Puts a role on sale, or updates its price and stock when it already is.
    /// `Ok(None)` when the server is at its limit.
    pub async fn add_store_item(
        &self,
        guild_id: u64,
        role_id: u64,
        price: i64,
        stock: Option<i32>,
    ) -> anyhow::Result<Option<StoreItem>> {
        Ok(sqlx::query_as::<_, StoreItem>(
            "INSERT INTO store_items (guild_id, role_id, price, stock)
             SELECT $1, $2, $3, $4
             WHERE (SELECT count(*) FROM store_items WHERE guild_id = $1) < $5
                OR EXISTS (SELECT 1 FROM store_items WHERE guild_id = $1 AND role_id = $2)
             ON CONFLICT (guild_id, role_id) DO UPDATE SET price = EXCLUDED.price, stock = EXCLUDED.stock
             RETURNING id, role_id, price, stock",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(role_id))
        .bind(price)
        .bind(stock)
        .bind(MAX_STORE_ITEMS)
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn remove_store_item(&self, guild_id: u64, role_id: u64) -> anyhow::Result<bool> {
        let removed = sqlx::query("DELETE FROM store_items WHERE guild_id = $1 AND role_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(role_id))
            .execute(self.pool())
            .await?
            .rows_affected();
        Ok(removed > 0)
    }

    /// Takes the price and one unit of stock in a single transaction. The item row is locked
    /// first, so two buyers can neither take the last unit nor spend the same points twice.
    pub async fn buy_store_item(&self, guild_id: u64, user_id: u64, item_id: i64) -> anyhow::Result<StoreBuy> {
        retry("buy_store_item", || {
            self.buy_store_item_once(guild_id, user_id, item_id)
        })
        .await
    }

    /// One attempt of [`Db::buy_store_item`].
    async fn buy_store_item_once(&self, guild_id: u64, user_id: u64, item_id: i64) -> anyhow::Result<StoreBuy> {
        let (guild, user) = (sql_id(guild_id), sql_id(user_id));
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[user]).await?;
        if !crate::economy_v2::store_allowed(&mut tx, guild, user).await? {
            return Ok(StoreBuy::Conditions);
        }
        let item: Option<StoreItem> = sqlx::query_as(
            "SELECT id, role_id, price, stock FROM store_items WHERE guild_id = $1 AND id = $2 FOR UPDATE",
        )
        .bind(guild)
        .bind(item_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(item) = item else {
            return Ok(StoreBuy::Gone);
        };
        if item.stock == Some(0) {
            return Ok(StoreBuy::SoldOut);
        }
        let paid: Option<(i64,)> = sqlx::query_as(
            "UPDATE members SET points = points - $3
             WHERE guild_id = $1 AND user_id = $2 AND points >= $3
             RETURNING points",
        )
        .bind(guild)
        .bind(user)
        .bind(item.price)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((balance,)) = paid else {
            let balance: Option<(i64,)> =
                sqlx::query_as("SELECT points FROM members WHERE guild_id = $1 AND user_id = $2")
                    .bind(guild)
                    .bind(user)
                    .fetch_optional(&mut *tx)
                    .await?;
            return Ok(StoreBuy::Insufficient {
                price: item.price,
                balance: balance.map_or(0, |(p,)| p),
            });
        };
        sqlx::query("UPDATE store_items SET stock = stock - 1 WHERE id = $1 AND stock IS NOT NULL")
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(StoreBuy::Bought {
            role_id: item.role_id,
            price: item.price,
            balance,
        })
    }

    /// Undoes a purchase whose role Discord refused to give: the price comes back, and so does the stock.
    /// Returns the new balance.
    pub async fn refund_store_purchase(
        &self,
        guild_id: u64,
        user_id: u64,
        item_id: i64,
        price: i64,
    ) -> anyhow::Result<i64> {
        retry("refund_store_purchase", || {
            self.refund_store_purchase_once(guild_id, user_id, item_id, price)
        })
        .await
    }

    /// One attempt of [`Db::refund_store_purchase`].
    async fn refund_store_purchase_once(
        &self,
        guild_id: u64,
        user_id: u64,
        item_id: i64,
        price: i64,
    ) -> anyhow::Result<i64> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user_id)]).await?;
        sqlx::query("UPDATE store_items SET stock = stock + 1 WHERE guild_id = $1 AND id = $2 AND stock IS NOT NULL")
            .bind(sql_id(guild_id))
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        let (balance,): (i64,) = sqlx::query_as(
            "INSERT INTO members (guild_id, user_id, points) VALUES ($1, $2, $3)
             ON CONFLICT (guild_id, user_id) DO UPDATE SET points = members.points + EXCLUDED.points
             RETURNING points",
        )
        .bind(sql_id(guild_id))
        .bind(sql_id(user_id))
        .bind(price)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(balance)
    }
}
