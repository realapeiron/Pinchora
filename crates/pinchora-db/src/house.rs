use crate::{Db, sql_id};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HouseRow {
    pub user_id: i64,
    pub tier: i16,
    pub name: String,
    pub locked: bool,
    pub likes: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoomRow {
    pub idx: i16,
    pub room_type: String,
    pub wallpaper: String,
    pub floor: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlacementRow {
    pub id: i64,
    pub room_idx: i16,
    pub item_id: String,
    pub x: i16,
    pub y: i16,
    pub flipped: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InventoryRow {
    pub item_id: String,
    pub quantity: i32,
}

/// A new room's look.
#[derive(Debug, Clone)]
pub struct NewRoom {
    pub room_type: String,
    pub wallpaper: String,
    pub floor: String,
}

#[derive(Debug, Clone)]
pub enum PlaceOutcome {
    Placed { id: i64 },
    NotOwned,
    Blocked(String),
}

#[derive(Debug, Clone, Copy)]
pub enum UpgradeOutcome {
    Upgraded { tier: i16, balance: i64 },
    Insufficient { balance: i64 },
    WrongTier { tier: i16 },
}

impl Db {
    /// The player's house, created with `starter` as its first room when missing.
    pub async fn house(&self, user_id: u64, starter: &NewRoom) -> anyhow::Result<(HouseRow, Vec<RoomRow>, bool)> {
        let id = sql_id(user_id);
        let mut tx = self.pool().begin().await?;
        let created = sqlx::query("INSERT INTO houses (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if created {
            sqlx::query("INSERT INTO rooms (user_id, idx, room_type, wallpaper, floor) VALUES ($1, 0, $2, $3, $4)")
                .bind(id)
                .bind(&starter.room_type)
                .bind(&starter.wallpaper)
                .bind(&starter.floor)
                .execute(&mut *tx)
                .await?;
        }
        let house =
            sqlx::query_as::<_, HouseRow>("SELECT user_id, tier, name, locked, likes FROM houses WHERE user_id = $1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        let rooms = sqlx::query_as::<_, RoomRow>(
            "SELECT idx, room_type, wallpaper, floor FROM rooms WHERE user_id = $1 ORDER BY idx",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok((house, rooms, created))
    }

    pub async fn placements(&self, user_id: u64, room_idx: i16) -> anyhow::Result<Vec<PlacementRow>> {
        Ok(sqlx::query_as::<_, PlacementRow>(
            "SELECT id, room_idx, item_id, x, y, flipped FROM placements
             WHERE user_id = $1 AND room_idx = $2 ORDER BY id",
        )
        .bind(sql_id(user_id))
        .bind(room_idx)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn inventory(&self, user_id: u64) -> anyhow::Result<Vec<InventoryRow>> {
        Ok(sqlx::query_as::<_, InventoryRow>(
            "SELECT item_id, quantity FROM inventory WHERE user_id = $1 AND quantity > 0 ORDER BY acquired_at DESC, item_id",
        )
        .bind(sql_id(user_id))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn owns(&self, user_id: u64, item_id: &str) -> anyhow::Result<bool> {
        let row: Option<(i32,)> = sqlx::query_as("SELECT quantity FROM inventory WHERE user_id = $1 AND item_id = $2")
            .bind(sql_id(user_id))
            .bind(item_id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.is_some_and(|(q,)| q > 0))
    }

    pub async fn grant_item(&self, user_id: u64, item_id: &str, quantity: i32) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        add_inventory(&mut tx, sql_id(user_id), item_id, quantity).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Moves one owned item into a room. Edits of one house are serialized by locking
    /// the house row; `check` validates the spot against the room's current placements.
    #[allow(clippy::too_many_arguments)]
    pub async fn place_item<F>(
        &self,
        user_id: u64,
        room_idx: i16,
        item_id: &str,
        x: i16,
        y: i16,
        flipped: bool,
        check: F,
    ) -> anyhow::Result<PlaceOutcome>
    where
        F: FnOnce(&[PlacementRow]) -> Result<(), String>,
    {
        let id = sql_id(user_id);
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT 1 FROM houses WHERE user_id = $1 FOR UPDATE")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let current = sqlx::query_as::<_, PlacementRow>(
            "SELECT id, room_idx, item_id, x, y, flipped FROM placements WHERE user_id = $1 AND room_idx = $2",
        )
        .bind(id)
        .bind(room_idx)
        .fetch_all(&mut *tx)
        .await?;
        if let Err(reason) = check(&current) {
            return Ok(PlaceOutcome::Blocked(reason));
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
            return Ok(PlaceOutcome::NotOwned);
        }
        let (placement,): (i64,) = sqlx::query_as(
            "INSERT INTO placements (user_id, room_idx, item_id, x, y, flipped)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(id)
        .bind(room_idx)
        .bind(item_id)
        .bind(x)
        .bind(y)
        .bind(flipped)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(PlaceOutcome::Placed { id: placement })
    }

    /// Returns a placed item to the inventory. `None` if it is not the player's.
    pub async fn pick_up(&self, user_id: u64, placement_id: i64) -> anyhow::Result<Option<String>> {
        let id = sql_id(user_id);
        let mut tx = self.pool().begin().await?;
        let item: Option<(String,)> =
            sqlx::query_as("DELETE FROM placements WHERE id = $1 AND user_id = $2 RETURNING item_id")
                .bind(placement_id)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((item_id,)) = item else {
            return Ok(None);
        };
        add_inventory(&mut tx, id, &item_id, 1).await?;
        tx.commit().await?;
        Ok(Some(item_id))
    }

    /// Flips a placed item. The footprint is symmetric, so no collision check is needed.
    pub async fn flip_placement(&self, user_id: u64, placement_id: i64) -> anyhow::Result<bool> {
        let n = sqlx::query("UPDATE placements SET flipped = NOT flipped WHERE id = $1 AND user_id = $2")
            .bind(placement_id)
            .bind(sql_id(user_id))
            .execute(self.pool())
            .await?
            .rows_affected();
        Ok(n == 1)
    }

    pub async fn set_room_surfaces(
        &self,
        user_id: u64,
        room_idx: i16,
        wallpaper: Option<&str>,
        floor: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE rooms SET wallpaper = COALESCE($3, wallpaper), floor = COALESCE($4, floor)
             WHERE user_id = $1 AND idx = $2",
        )
        .bind(sql_id(user_id))
        .bind(room_idx)
        .bind(wallpaper)
        .bind(floor)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn set_house_locked(&self, user_id: u64, locked: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE houses SET locked = $2 WHERE user_id = $1")
            .bind(sql_id(user_id))
            .bind(locked)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Pays for the next tier and adds its rooms. Placed furniture stays where it is.
    pub async fn upgrade_house(
        &self,
        user_id: u64,
        from_tier: i16,
        price: i64,
        new_rooms: &[NewRoom],
    ) -> anyhow::Result<UpgradeOutcome> {
        let id = sql_id(user_id);
        let mut tx = self.pool().begin().await?;
        let (tier,): (i16,) = sqlx::query_as("SELECT tier FROM houses WHERE user_id = $1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if tier != from_tier {
            return Ok(UpgradeOutcome::WrongTier { tier });
        }
        let (balance,): (i64,) = sqlx::query_as("SELECT credits FROM users WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if balance < price {
            return Ok(UpgradeOutcome::Insufficient { balance });
        }
        sqlx::query("UPDATE users SET credits = credits - $2 WHERE id = $1")
            .bind(id)
            .bind(price)
            .execute(&mut *tx)
            .await?;
        crate::economy::ledger(&mut tx, id, -price, "house_upgrade").await?;
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM rooms WHERE user_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        for (offset, room) in new_rooms.iter().enumerate() {
            sqlx::query("INSERT INTO rooms (user_id, idx, room_type, wallpaper, floor) VALUES ($1, $2, $3, $4, $5)")
                .bind(id)
                .bind((count as usize + offset) as i16)
                .bind(&room.room_type)
                .bind(&room.wallpaper)
                .bind(&room.floor)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE houses SET tier = tier + 1 WHERE user_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(UpgradeOutcome::Upgraded {
            tier: from_tier + 1,
            balance: balance - price,
        })
    }
}

pub(crate) async fn add_inventory(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
    item_id: &str,
    quantity: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO inventory (user_id, item_id, quantity) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, item_id) DO UPDATE
         SET quantity = inventory.quantity + EXCLUDED.quantity, acquired_at = now()",
    )
    .bind(user_id)
    .bind(item_id)
    .bind(quantity)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
