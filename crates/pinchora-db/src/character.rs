//! Character 2.0: the accessories a player bought and their saved outfits. The character code
//! itself is `users.avatar` (see [`Db::character`]).

use pinchora_core::stats::WardrobeFacts;

use crate::{CustomizeBuy, Db, sql_id};

/// One saved outfit.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OutfitRow {
    /// 1 to 3.
    pub slot: i16,
    pub name: String,
    /// A character code, as `users.avatar` holds it.
    pub code: String,
}

impl Db {
    /// The facts of one player, also for one with no `users` row yet (all zeros, nothing bought).
    pub async fn wardrobe_facts(&self, user: u64) -> anyhow::Result<WardrobeFacts> {
        Ok(WardrobeFacts {
            stats: self.player_stats(user).await?,
            bought_badges: self.owned_badges(user).await?,
            bought_accessories: self.owned_accessories(user).await?,
        })
    }

    /// Accessory ids the player bought, oldest first.
    pub async fn owned_accessories(&self, user: u64) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT accessory_id FROM user_accessories WHERE user_id = $1 ORDER BY bought_at, accessory_id",
        )
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Buys an accessory at `price` shells: the shells, the ledger entry and the ownership
    /// happen together, or not at all.
    pub async fn buy_accessory(&self, user: u64, accessory_id: &str, price: i64) -> anyhow::Result<CustomizeBuy> {
        self.buy_cosmetic(
            "INSERT INTO user_accessories (user_id, accessory_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            user,
            accessory_id,
            price,
            "accessory",
        )
        .await
    }

    /// The player's saved outfits, by slot; empty slots are missing.
    pub async fn outfits(&self, user: u64) -> anyhow::Result<Vec<OutfitRow>> {
        Ok(sqlx::query_as::<_, OutfitRow>(
            "SELECT slot, name, code FROM character_outfits WHERE user_id = $1 ORDER BY slot",
        )
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?)
    }

    /// Saves `code` in outfit `slot` (1 to 3), keeping the slot's name, or calling a new one
    /// "Outfit N".
    pub async fn save_outfit(&self, user: u64, slot: i16, code: &str) -> anyhow::Result<()> {
        anyhow::ensure!((1..=3).contains(&slot), "outfit slots are 1 to 3");
        sqlx::query(
            "INSERT INTO character_outfits (user_id, slot, name, code) VALUES ($1, $2, 'Outfit ' || $2, $3)
             ON CONFLICT (user_id, slot) DO UPDATE SET code = EXCLUDED.code, saved_at = now()",
        )
        .bind(sql_id(user))
        .bind(slot)
        .bind(code)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Renames a saved outfit. False when the slot is empty.
    pub async fn rename_outfit(&self, user: u64, slot: i16, name: &str) -> anyhow::Result<bool> {
        let name = name.trim();
        anyhow::ensure!(
            (1..=32).contains(&name.chars().count()),
            "an outfit name is 1 to 32 characters"
        );
        Ok(
            sqlx::query("UPDATE character_outfits SET name = $3 WHERE user_id = $1 AND slot = $2")
                .bind(sql_id(user))
                .bind(slot)
                .bind(name)
                .execute(self.pool())
                .await?
                .rows_affected()
                == 1,
        )
    }
}
