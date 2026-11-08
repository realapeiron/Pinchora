//! Version-checked house settings and payment in the same transaction as the mutation.
use crate::{Currency, Db, NewRoom, PlacementRow, house::add_inventory, locks::lock_users, sql_id};

pub enum HouseUpdate<'a> {
    Surface {
        wallpaper: Option<&'a str>,
        floor: Option<&'a str>,
        key: &'a str,
        price: i64,
        currency: Currency,
        confirmed: bool,
    },
    Upgrade {
        from_tier: i16,
        price: i64,
        rooms: &'a [NewRoom],
    },
    Lock(bool),
}

#[derive(Debug, PartialEq, Eq)]
pub enum HouseUpdateOutcome {
    Done,
    Conflict,
    Confirm { balance: i64 },
    Insufficient { balance: i64 },
}

impl Db {
    pub async fn update_house_checked<F>(
        &self,
        owner: u64,
        room: i16,
        update: HouseUpdate<'_>,
        current_version: F,
    ) -> anyhow::Result<HouseUpdateOutcome>
    where
        F: FnMut(&[PlacementRow]) -> bool,
    {
        self.update_house_receipted(owner, room, update, None, current_version)
            .await
    }
    pub async fn update_house_receipted<F>(
        &self,
        owner: u64,
        room: i16,
        update: HouseUpdate<'_>,
        receipt: Option<&crate::ReefEditRequest>,
        mut current_version: F,
    ) -> anyhow::Result<HouseUpdateOutcome>
    where
        F: FnMut(&[PlacementRow]) -> bool,
    {
        let mut tx = self.pool().begin().await?;
        let id = sql_id(owner);
        lock_users(&mut tx, &[id]).await?;
        let tier: Option<i16> = sqlx::query_scalar("SELECT tier FROM houses WHERE user_id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(receipt) = receipt
            && !receipt.check(&mut tx, owner).await?
        {
            return Ok(HouseUpdateOutcome::Conflict);
        }
        let exists: Option<i16> = sqlx::query_scalar("SELECT idx FROM rooms WHERE user_id=$1 AND idx=$2 FOR UPDATE")
            .bind(id)
            .bind(room)
            .fetch_optional(&mut *tx)
            .await?;
        let placements = sqlx::query_as::<_, PlacementRow>(
            "SELECT id,room_idx,item_id,x,y,flipped FROM placements WHERE user_id=$1 AND room_idx=$2 ORDER BY id",
        )
        .bind(id)
        .bind(room)
        .fetch_all(&mut *tx)
        .await?;
        if tier.is_none() || exists.is_none() || !current_version(&placements) {
            return Ok(HouseUpdateOutcome::Conflict);
        }
        if !matches!(&update, HouseUpdate::Lock(_)) {
            // A house can exist before the player has ever opened their wallet.
            sqlx::query("INSERT INTO users(id) VALUES($1) ON CONFLICT(id) DO NOTHING")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        match update {
            HouseUpdate::Surface {
                wallpaper,
                floor,
                key,
                price,
                currency,
                confirmed,
            } => {
                let owned: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM inventory WHERE user_id=$1 AND item_id=$2 AND quantity>0)",
                )
                .bind(id)
                .bind(key)
                .fetch_one(&mut *tx)
                .await?;
                if !owned {
                    let balance = crate::shop::balance_of(&mut tx, id, currency).await?;
                    if !confirmed {
                        return Ok(HouseUpdateOutcome::Confirm { balance });
                    }
                    if crate::shop::charge(&mut tx, id, price, currency).await?.is_none() {
                        return Ok(HouseUpdateOutcome::Insufficient { balance });
                    }
                    add_inventory(&mut tx, id, key, 1).await?;
                    if currency == Currency::Shells {
                        crate::economy::ledger(&mut tx, id, -price, "shop").await?;
                    }
                }
                sqlx::query("UPDATE rooms SET wallpaper=COALESCE($3,wallpaper),floor=COALESCE($4,floor) WHERE user_id=$1 AND idx=$2")
                    .bind(id).bind(room).bind(wallpaper).bind(floor).execute(&mut *tx).await?;
            }
            HouseUpdate::Upgrade {
                from_tier,
                price,
                rooms,
            } => {
                if tier != Some(from_tier) {
                    return Ok(HouseUpdateOutcome::Conflict);
                }
                let Some(_) = crate::shop::charge(&mut tx, id, price, Currency::Shells).await? else {
                    return Ok(HouseUpdateOutcome::Insufficient {
                        balance: crate::shop::balance_of(&mut tx, id, Currency::Shells).await?,
                    });
                };
                crate::economy::ledger(&mut tx, id, -price, "house_upgrade").await?;
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM rooms WHERE user_id=$1")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
                for (offset, room) in rooms.iter().enumerate() {
                    sqlx::query("INSERT INTO rooms(user_id,idx,room_type,wallpaper,floor) VALUES($1,$2,$3,$4,$5)")
                        .bind(id)
                        .bind((count as usize + offset) as i16)
                        .bind(&room.room_type)
                        .bind(&room.wallpaper)
                        .bind(&room.floor)
                        .execute(&mut *tx)
                        .await?;
                }
                sqlx::query("UPDATE houses SET tier=tier+1 WHERE user_id=$1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            HouseUpdate::Lock(locked) => {
                sqlx::query("UPDATE houses SET locked=$2 WHERE user_id=$1")
                    .bind(id)
                    .bind(locked)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        if let Some(receipt) = receipt {
            receipt.save(&mut tx, owner, room).await?;
        }
        tx.commit().await?;
        Ok(HouseUpdateOutcome::Done)
    }
}
