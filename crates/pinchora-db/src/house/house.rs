use pinchora_core::games::items::is_soulbound;

use crate::Db;
use crate::infra::locks::Retry;
use crate::infra::locks::lock_users;
use crate::infra::locks::retry;
use crate::sql_id;

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

/// Why a soulbound piece cannot go into, or come out of, somebody else's house.
pub const SOULBOUND_GUEST: &str = "soulbound pieces only go in your own house";

#[derive(Debug, Clone)]
pub enum PlaceOutcome {
    Placed { id: i64 },
    NotOwned,
    Blocked(String),
}

/// One change [`Db::restore`] makes to a room.
#[derive(Debug, Clone)]
pub enum RoomChange {
    /// Takes this placement, exactly as given, back to the inventory.
    Remove(PlacementRow),
    /// Turns this placement, exactly as given, the other way.
    Flip(PlacementRow),
    /// Places one item from the inventory; the row's id is not used.
    Add(PlacementRow),
}

#[derive(Debug, Clone)]
pub enum RestoreOutcome {
    /// Everything changed; the new ids of the added rows, in order.
    Done { added: Vec<i64> },
    /// Nothing changed.
    Refused(Refusal),
}

/// Why [`Db::restore`] changed nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// This item is no longer where the change expected it.
    Moved(String),
    /// The inventory has none of this item left.
    NotOwned(String),
    /// The room check refused a change.
    Blocked(String),
}

#[derive(Debug, Clone, Copy)]
pub enum UpgradeOutcome {
    Upgraded { tier: i16, balance: i64 },
    Insufficient { balance: i64 },
    WrongTier { tier: i16 },
}

/// Holds an editor's cross-pod serialization lock until its Redis history has been saved.
/// A separate connection is reserved; the semaphore leaves capacity for the actual transactions.
pub struct HouseEditorGuard {
    _transaction: sqlx::Transaction<'static, sqlx::Postgres>,
    _slot: tokio::sync::OwnedSemaphorePermit,
}

impl Db {
    pub async fn house_editor(&self, owner: u64) -> anyhow::Result<HouseEditorGuard> {
        anyhow::ensure!(
            self.pool().options().get_max_connections() >= 2,
            "house editing requires two database connections"
        );
        let slot = self.editor_slots.clone().acquire_owned().await?;
        let mut tx = self.pool().begin().await?;
        // This lock never holds player rows. All mutation transactions still take lock_users first.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('house-editor:' || $1::text, 0))")
            .bind(owner.to_string())
            .execute(&mut *tx)
            .await?;
        Ok(HouseEditorGuard {
            _transaction: tx,
            _slot: slot,
        })
    }

    /// The player's house, created with `starter` as its first room when missing.
    pub async fn house(&self, user_id: u64, starter: &NewRoom) -> anyhow::Result<(HouseRow, Vec<RoomRow>, bool)> {
        retry("house", || self.house_once(user_id, starter)).await
    }

    /// One attempt of [`Db::house`].
    async fn house_once(&self, user_id: u64, starter: &NewRoom) -> anyhow::Result<(HouseRow, Vec<RoomRow>, bool)> {
        let id = sql_id(user_id);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
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
        retry("grant_item", || self.grant_item_once(user_id, item_id, quantity)).await
    }

    /// One attempt of [`Db::grant_item`].
    async fn grant_item_once(&self, user_id: u64, item_id: &str, quantity: i32) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user_id)]).await?;
        add_inventory(&mut tx, sql_id(user_id), item_id, quantity).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Moves one owned item into a room. Edits of one house are serialized by locking
    /// the house row; `check` validates the spot against the room's current placements (and is
    /// asked again if the edit loses a deadlock and runs a second time).
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
        F: FnMut(&[PlacementRow]) -> Result<(), String>,
    {
        self.place_item_for(user_id, user_id, room_idx, item_id, x, y, flipped, check)
            .await
    }

    /// Like [`Db::place_item`] for a piece that comes out of `stock`'s inventory and goes into
    /// `owner`'s house, where it is the owner's from then on (a partner decorating). A soulbound
    /// piece never goes into somebody else's house: that would hand it over.
    #[allow(clippy::too_many_arguments)]
    pub async fn place_item_for<F>(
        &self,
        owner: u64,
        stock: u64,
        room_idx: i16,
        item_id: &str,
        x: i16,
        y: i16,
        flipped: bool,
        mut check: F,
    ) -> anyhow::Result<PlaceOutcome>
    where
        F: FnMut(&[PlacementRow]) -> Result<(), String>,
    {
        let mut retry = Retry::new("place_item_for");
        loop {
            let result = self
                .place_item_for_once(owner, stock, room_idx, item_id, x, y, flipped, &mut check)
                .await;
            if !retry.again(&result).await {
                return result;
            }
        }
    }

    /// One attempt of [`Db::place_item_for`].
    #[allow(clippy::too_many_arguments)]
    async fn place_item_for_once<F>(
        &self,
        owner: u64,
        stock: u64,
        room_idx: i16,
        item_id: &str,
        x: i16,
        y: i16,
        flipped: bool,
        check: &mut F,
    ) -> anyhow::Result<PlaceOutcome>
    where
        F: FnMut(&[PlacementRow]) -> Result<(), String>,
    {
        if owner != stock && is_soulbound(item_id) {
            return Ok(PlaceOutcome::Blocked(SOULBOUND_GUEST.to_owned()));
        }
        let (id, stock) = (sql_id(owner), sql_id(stock));
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id, stock]).await?;
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
        .bind(stock)
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
        retry("pick_up", || self.pick_up_once(user_id, placement_id)).await
    }

    /// One attempt of [`Db::pick_up`].
    async fn pick_up_once(&self, user_id: u64, placement_id: i64) -> anyhow::Result<Option<String>> {
        let id = sql_id(user_id);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
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

    /// Takes every placement in a room back to the inventory at once; returns what stood there.
    pub async fn clear_room(&self, user_id: u64, room_idx: i16) -> anyhow::Result<Vec<PlacementRow>> {
        retry("clear_room", || self.clear_room_once(user_id, room_idx)).await
    }

    /// One attempt of [`Db::clear_room`].
    async fn clear_room_once(&self, user_id: u64, room_idx: i16) -> anyhow::Result<Vec<PlacementRow>> {
        let id = sql_id(user_id);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
        sqlx::query("SELECT 1 FROM houses WHERE user_id = $1 FOR UPDATE")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let mut removed = sqlx::query_as::<_, PlacementRow>(
            "DELETE FROM placements WHERE user_id = $1 AND room_idx = $2
             RETURNING id, room_idx, item_id, x, y, flipped",
        )
        .bind(id)
        .bind(room_idx)
        .fetch_all(&mut *tx)
        .await?;
        removed.sort_by_key(|p| p.id);
        let mut counts: Vec<(&str, i32)> = Vec::new();
        for p in &removed {
            match counts.iter_mut().find(|(item, _)| *item == p.item_id) {
                Some((_, n)) => *n += 1,
                None => counts.push((&p.item_id, 1)),
            }
        }
        for (item, n) in counts {
            add_inventory(&mut tx, id, item, n).await?;
        }
        tx.commit().await?;
        Ok(removed)
    }

    /// Applies `changes` to a room in order, all or nothing, under the house lock (the editor's
    /// undo). `check` sees the room as it is before each change and can refuse it.
    pub async fn restore<F>(
        &self,
        user_id: u64,
        room_idx: i16,
        changes: &[RoomChange],
        check: F,
    ) -> anyhow::Result<RestoreOutcome>
    where
        F: FnMut(&[PlacementRow], &RoomChange) -> Result<(), String>,
    {
        self.restore_for(user_id, user_id, room_idx, changes, check).await
    }

    /// Like [`Db::restore`] in `owner`'s house when `editor` is making the undo: a piece the
    /// undo takes out of the room (a placement being undone) goes back to the editor who placed
    /// it; pieces it puts back come from the owner's inventory, where taking them out left them.
    pub async fn restore_for<F>(
        &self,
        owner: u64,
        editor: u64,
        room_idx: i16,
        changes: &[RoomChange],
        mut check: F,
    ) -> anyhow::Result<RestoreOutcome>
    where
        F: FnMut(&[PlacementRow], &RoomChange) -> Result<(), String>,
    {
        let mut retry = Retry::new("restore_for");
        loop {
            let result = self
                .restore_for_once(
                    owner,
                    owner,
                    editor,
                    room_idx,
                    changes,
                    None,
                    None,
                    &mut |_| Ok(()),
                    &mut check,
                )
                .await;
            if !retry.again(&result).await {
                return result;
            }
        }
    }

    /// The shared restore transaction with an initial version check and explicit inventory routing.
    /// The initial check also runs for an empty change list, while holding the house lock.
    #[allow(clippy::too_many_arguments)]
    pub async fn edit_room<F, G>(
        &self,
        owner: u64,
        stock: u64,
        refund: u64,
        room: i16,
        changes: &[RoomChange],
        partnership: Option<(u64, i64)>,
        initial: G,
        check: F,
    ) -> anyhow::Result<RestoreOutcome>
    where
        F: FnMut(&[PlacementRow], &RoomChange) -> Result<(), String>,
        G: FnMut(&[PlacementRow]) -> Result<(), String>,
    {
        self.edit_room_receipted(owner, stock, refund, room, changes, partnership, None, initial, check)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn edit_room_receipted<F, G>(
        &self,
        owner: u64,
        stock: u64,
        refund: u64,
        room: i16,
        changes: &[RoomChange],
        partnership: Option<(u64, i64)>,
        receipt: Option<&crate::ReefEditRequest>,
        mut initial: G,
        mut check: F,
    ) -> anyhow::Result<RestoreOutcome>
    where
        F: FnMut(&[PlacementRow], &RoomChange) -> Result<(), String>,
        G: FnMut(&[PlacementRow]) -> Result<(), String>,
    {
        let mut retry = Retry::new("edit_room");
        loop {
            let result = self
                .restore_for_once(
                    owner,
                    stock,
                    refund,
                    room,
                    changes,
                    partnership,
                    receipt,
                    &mut initial,
                    &mut check,
                )
                .await;
            if !retry.again(&result).await {
                return result;
            }
        }
    }

    /// One attempt of [`Db::restore_for`].
    #[allow(clippy::too_many_arguments)]
    async fn restore_for_once<F, G>(
        &self,
        owner: u64,
        stock: u64,
        editor: u64,
        room_idx: i16,
        changes: &[RoomChange],
        partnership: Option<(u64, i64)>,
        receipt: Option<&crate::ReefEditRequest>,
        initial: &mut G,
        check: &mut F,
    ) -> anyhow::Result<RestoreOutcome>
    where
        F: FnMut(&[PlacementRow], &RoomChange) -> Result<(), String>,
        G: FnMut(&[PlacementRow]) -> Result<(), String>,
    {
        let (id, refund) = (sql_id(owner), sql_id(editor));
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id, refund, sql_id(stock)]).await?;
        sqlx::query("SELECT 1 FROM houses WHERE user_id = $1 FOR UPDATE")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if let Some(receipt) = receipt
            && !receipt.check(&mut tx, owner).await?
        {
            return Ok(RestoreOutcome::Refused(Refusal::Blocked("room_changed".into())));
        }
        if let Some((guest, bond)) = partnership {
            let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM guestbook_blocks WHERE (host_id=$1 AND blocked_id=$2) OR (host_id=$2 AND blocked_id=$1))")
                .bind(id).bind(sql_id(guest)).fetch_one(&mut *tx).await?;
            if blocked {
                return Ok(RestoreOutcome::Refused(Refusal::Blocked("access_denied".into())));
            }
            let pair: Option<(chrono::DateTime<chrono::Utc>,)> =
                sqlx::query_as("SELECT since FROM partners WHERE user_a=$1 AND user_b=$2 FOR SHARE")
                    .bind(sql_id(owner.min(guest)))
                    .bind(sql_id(owner.max(guest)))
                    .fetch_optional(&mut *tx)
                    .await?;
            if !pair.is_some_and(|(since,)| since.timestamp_micros() == bond) {
                return Ok(RestoreOutcome::Refused(Refusal::Blocked(
                    "You are no longer partners.".into(),
                )));
            }
        }
        let mut current = sqlx::query_as::<_, PlacementRow>(
            "SELECT id, room_idx, item_id, x, y, flipped FROM placements
             WHERE user_id = $1 AND room_idx = $2 ORDER BY id",
        )
        .bind(id)
        .bind(room_idx)
        .fetch_all(&mut *tx)
        .await?;
        if let Err(reason) = initial(&current) {
            return Ok(RestoreOutcome::Refused(Refusal::Blocked(reason)));
        }
        let mut added = Vec::new();
        for change in changes {
            if let RoomChange::Remove(p) | RoomChange::Flip(p) = change
                && !current
                    .iter()
                    .any(|c| (c.id, &c.item_id, c.x, c.y, c.flipped) == (p.id, &p.item_id, p.x, p.y, p.flipped))
            {
                return Ok(RestoreOutcome::Refused(Refusal::Moved(p.item_id.clone())));
            }
            if let Err(reason) = check(&current, change) {
                return Ok(RestoreOutcome::Refused(Refusal::Blocked(reason)));
            }
            match change {
                RoomChange::Remove(p) => {
                    if refund != id && is_soulbound(&p.item_id) {
                        return Ok(RestoreOutcome::Refused(Refusal::Blocked(SOULBOUND_GUEST.to_owned())));
                    }
                    sqlx::query("DELETE FROM placements WHERE id = $1 AND user_id = $2")
                        .bind(p.id)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    add_inventory(&mut tx, refund, &p.item_id, 1).await?;
                    current.retain(|c| c.id != p.id);
                }
                RoomChange::Flip(p) => {
                    sqlx::query("UPDATE placements SET flipped = NOT flipped WHERE id = $1 AND user_id = $2")
                        .bind(p.id)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    if let Some(c) = current.iter_mut().find(|c| c.id == p.id) {
                        c.flipped = !c.flipped;
                    }
                }
                RoomChange::Add(p) => {
                    if stock != owner && is_soulbound(&p.item_id) {
                        return Ok(RestoreOutcome::Refused(Refusal::Blocked(SOULBOUND_GUEST.to_owned())));
                    }
                    let taken = sqlx::query(
                        "UPDATE inventory SET quantity = quantity - 1
                         WHERE user_id = $1 AND item_id = $2 AND quantity > 0",
                    )
                    .bind(sql_id(stock))
                    .bind(&p.item_id)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
                    if taken == 0 {
                        return Ok(RestoreOutcome::Refused(Refusal::NotOwned(p.item_id.clone())));
                    }
                    let (new_id,): (i64,) = sqlx::query_as(
                        "INSERT INTO placements (user_id, room_idx, item_id, x, y, flipped)
                         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
                    )
                    .bind(id)
                    .bind(room_idx)
                    .bind(&p.item_id)
                    .bind(p.x)
                    .bind(p.y)
                    .bind(p.flipped)
                    .fetch_one(&mut *tx)
                    .await?;
                    current.push(PlacementRow {
                        id: new_id,
                        room_idx,
                        ..p.clone()
                    });
                    added.push(new_id);
                }
            }
        }
        if let Some(receipt) = receipt {
            receipt.save(&mut tx, owner, room_idx).await?;
        }
        tx.commit().await?;
        Ok(RestoreOutcome::Done { added })
    }

    /// Flips a placed item. The caller checks the turned footprint still fits first.
    pub async fn flip_placement(&self, user_id: u64, placement_id: i64) -> anyhow::Result<bool> {
        retry("flip_placement", || self.flip_placement_once(user_id, placement_id)).await
    }

    async fn flip_placement_once(&self, user_id: u64, placement_id: i64) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user_id)]).await?;
        sqlx::query("SELECT 1 FROM houses WHERE user_id = $1 FOR UPDATE")
            .bind(sql_id(user_id))
            .execute(&mut *tx)
            .await?;
        let n = sqlx::query("UPDATE placements SET flipped = NOT flipped WHERE id = $1 AND user_id = $2")
            .bind(placement_id)
            .bind(sql_id(user_id))
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(n == 1)
    }

    pub async fn set_room_surfaces(
        &self,
        user_id: u64,
        room_idx: i16,
        wallpaper: Option<&str>,
        floor: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user_id)]).await?;
        sqlx::query("SELECT 1 FROM houses WHERE user_id=$1 FOR UPDATE")
            .bind(sql_id(user_id))
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE rooms SET wallpaper = COALESCE($3, wallpaper), floor = COALESCE($4, floor)
             WHERE user_id = $1 AND idx = $2",
        )
        .bind(sql_id(user_id))
        .bind(room_idx)
        .bind(wallpaper)
        .bind(floor)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn set_house_locked(&self, user_id: u64, locked: bool) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user_id)]).await?;
        sqlx::query("UPDATE houses SET locked = $2 WHERE user_id = $1")
            .bind(sql_id(user_id))
            .bind(locked)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
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
        retry("upgrade_house", || {
            self.upgrade_house_once(user_id, from_tier, price, new_rooms)
        })
        .await
    }

    /// One attempt of [`Db::upgrade_house`].
    async fn upgrade_house_once(
        &self,
        user_id: u64,
        from_tier: i16,
        price: i64,
        new_rooms: &[NewRoom],
    ) -> anyhow::Result<UpgradeOutcome> {
        let id = sql_id(user_id);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
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
        crate::economy::legacy::ledger(&mut tx, id, -price, "house_upgrade").await?;
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
