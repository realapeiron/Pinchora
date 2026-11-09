//! Player studio persistence. Uses existing tables and the economy's user lock order.
use crate::Db;
use crate::UserRow;
use crate::infra::locks::lock_users;
use crate::sql_id;
use serde::{Deserialize, Serialize};

/// Only editable profile fields; balances, identity and achievement progress are never accepted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlayerProfile {
    pub bio: String,
    pub background: Option<String>,
    pub badges: Vec<String>,
    pub badges_set: bool,
    pub title: Option<String>,
    pub accent: Option<String>,
    pub panel_opacity: i16,
    pub frame: String,
    pub showcase_fish: Option<String>,
    pub showcase_pet: Option<String>,
    pub showcase_item: Option<String>,
    pub showcase_house: bool,
}
impl PlayerProfile {
    pub fn from_row(row: &UserRow) -> Self {
        Self {
            bio: row.bio.clone(),
            background: row.background.clone(),
            badges: row.badges.clone(),
            badges_set: row.badges_set,
            title: row.title.clone(),
            accent: row.accent.clone(),
            panel_opacity: row.panel_opacity,
            frame: row.frame.clone(),
            showcase_fish: row.showcase_fish.clone(),
            showcase_pet: row.showcase_pet.map(|id| id.to_string()),
            showcase_item: row.showcase_item.clone(),
            showcase_house: row.showcase_house,
        }
    }
    /// Apply a validated draft to a render snapshot, preserving all read-only player facts.
    pub fn apply(&self, row: &mut UserRow) {
        row.bio.clone_from(&self.bio);
        row.background.clone_from(&self.background);
        row.badges.clone_from(&self.badges);
        row.badges_set = self.badges_set;
        row.title.clone_from(&self.title);
        row.accent.clone_from(&self.accent);
        row.panel_opacity = self.panel_opacity;
        row.frame.clone_from(&self.frame);
        row.showcase_fish.clone_from(&self.showcase_fish);
        row.showcase_pet = self.showcase_pet.as_deref().and_then(|v| v.parse().ok());
        row.showcase_item.clone_from(&self.showcase_item);
        row.showcase_house = self.showcase_house;
    }
}

#[derive(sqlx::FromRow, Serialize)]
pub struct PlayerLedgerRow {
    pub delta: i64,
    pub reason: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
impl Db {
    /// Atomically save a validated studio draft. Purchases remain in the existing buy functions.
    pub async fn save_player_profile(&self, user: u64, p: &PlayerProfile) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE users SET bio=$2, background=$3, badges=$4, badges_set=$5, title=$6,
            accent=$7, panel_opacity=$8, frame=$9, showcase_fish=$10, showcase_pet=$11,
            showcase_item=$12, showcase_house=$13 WHERE id=$1",
        )
        .bind(sql_id(user))
        .bind(&p.bio)
        .bind(&p.background)
        .bind(&p.badges)
        .bind(p.badges_set)
        .bind(&p.title)
        .bind(&p.accent)
        .bind(p.panel_opacity)
        .bind(&p.frame)
        .bind(&p.showcase_fish)
        .bind(p.showcase_pet.as_deref().map(str::parse::<i64>).transpose()?)
        .bind(&p.showcase_item)
        .bind(p.showcase_house)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn player_ledger(&self, user: u64) -> anyhow::Result<Vec<PlayerLedgerRow>> {
        Ok(sqlx::query_as(
            "SELECT delta, reason, created_at FROM credit_ledger WHERE user_id=$1 ORDER BY id DESC LIMIT 50",
        )
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?)
    }
    /// Store the slot's name and character together, keeping an existing name when omitted.
    pub async fn save_player_outfit(&self, user: u64, slot: i16, code: &str, name: Option<&str>) -> anyhow::Result<()> {
        anyhow::ensure!((1..=3).contains(&slot), "outfit slots are 1 to 3");
        let name = name.map(str::trim);
        anyhow::ensure!(
            name.is_none_or(|n| (1..=32).contains(&n.chars().count())),
            "outfit name is 1 to 32 characters"
        );
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        sqlx::query("INSERT INTO character_outfits (user_id, slot, name, code) VALUES ($1, $2, COALESCE($4, 'Outfit ' || $2), $3)
            ON CONFLICT (user_id, slot) DO UPDATE SET code=EXCLUDED.code, name=COALESCE($4, character_outfits.name), saved_at=now()")
            .bind(sql_id(user)).bind(slot).bind(code).bind(name).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn delete_player_outfit(&self, user: u64, slot: i16) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM character_outfits WHERE user_id=$1 AND slot=$2")
            .bind(sql_id(user))
            .bind(slot)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
