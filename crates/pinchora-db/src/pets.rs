use chrono::{DateTime, Utc};

use crate::{Db, sql_id};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PetRow {
    pub id: i64,
    pub owner_id: i64,
    pub species: String,
    pub name: String,
    pub level: i32,
    pub xp: i32,
    pub hunger: i16,
    pub fun: i16,
    pub energy: i16,
    pub hygiene: i16,
    pub needs_at: DateTime<Utc>,
    pub active: bool,
}

macro_rules! pet_columns {
    () => {
        "id, owner_id, species, name, level, xp, hunger, fun, energy, hygiene, needs_at, active"
    };
}

impl Db {
    pub async fn pets(&self, owner: u64) -> anyhow::Result<Vec<PetRow>> {
        Ok(sqlx::query_as::<_, PetRow>(concat!(
            "SELECT ",
            pet_columns!(),
            " FROM pets WHERE owner_id = $1 ORDER BY active DESC, adopted_at"
        ))
        .bind(sql_id(owner))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn active_pet(&self, owner: u64) -> anyhow::Result<Option<PetRow>> {
        Ok(sqlx::query_as::<_, PetRow>(concat!(
            "SELECT ",
            pet_columns!(),
            " FROM pets WHERE owner_id = $1 AND active"
        ))
        .bind(sql_id(owner))
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn has_species(&self, owner: u64, species: &str) -> anyhow::Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM pets WHERE owner_id = $1 AND species = $2 LIMIT 1")
            .bind(sql_id(owner))
            .bind(species)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.is_some())
    }

    /// Adds a pet; the first pet a player gets becomes their active one.
    pub async fn adopt(&self, owner: u64, species: &str, name: &str) -> anyhow::Result<PetRow> {
        let id = sql_id(owner);
        let mut tx = self.pool().begin().await?;
        let (has_active,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pets WHERE owner_id = $1 AND active)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        let pet = sqlx::query_as::<_, PetRow>(concat!(
            "INSERT INTO pets (owner_id, species, name, active) VALUES ($1, $2, $3, $4) RETURNING ",
            pet_columns!()
        ))
        .bind(id)
        .bind(species)
        .bind(name)
        .bind(!has_active)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(pet)
    }

    pub async fn set_active_pet(&self, owner: u64, pet_id: i64) -> anyhow::Result<bool> {
        let id = sql_id(owner);
        let mut tx = self.pool().begin().await?;
        let owned: Option<(i64,)> = sqlx::query_as("SELECT id FROM pets WHERE id = $1 AND owner_id = $2")
            .bind(pet_id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        if owned.is_none() {
            return Ok(false);
        }
        sqlx::query("UPDATE pets SET active = FALSE WHERE owner_id = $1 AND active")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE pets SET active = TRUE WHERE id = $1")
            .bind(pet_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Stores needs as of now, plus new XP and level.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_pet(&self, owner: u64, pet_id: i64, needs: [i16; 4], level: i32, xp: i32) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE pets SET hunger = $3, fun = $4, energy = $5, hygiene = $6, level = $7, xp = $8, needs_at = now()
             WHERE id = $1 AND owner_id = $2",
        )
        .bind(pet_id)
        .bind(sql_id(owner))
        .bind(needs[0])
        .bind(needs[1])
        .bind(needs[2])
        .bind(needs[3])
        .bind(level)
        .bind(xp)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn rename_pet(&self, owner: u64, pet_id: i64, name: &str) -> anyhow::Result<bool> {
        let n = sqlx::query("UPDATE pets SET name = $3 WHERE id = $1 AND owner_id = $2")
            .bind(pet_id)
            .bind(sql_id(owner))
            .bind(name)
            .execute(self.pool())
            .await?
            .rows_affected();
        Ok(n == 1)
    }

    /// Takes shells for a purchase or fee; `None` when the player cannot afford it.
    pub async fn spend_shells(&self, user: u64, amount: i64, reason: &str) -> anyhow::Result<Option<i64>> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let row: Option<(i64,)> =
            sqlx::query_as("UPDATE users SET credits = credits - $2 WHERE id = $1 AND credits >= $2 RETURNING credits")
                .bind(id)
                .bind(amount)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((balance,)) = row else {
            return Ok(None);
        };
        crate::economy::ledger(&mut tx, id, -amount, reason).await?;
        tx.commit().await?;
        Ok(Some(balance))
    }

    /// Pays shells out (rewards, refunds); returns the new balance.
    pub async fn earn_shells(&self, user: u64, amount: i64, reason: &str) -> anyhow::Result<i64> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        let (balance,): (i64,) = sqlx::query_as(
            "INSERT INTO users (id, credits) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET credits = users.credits + EXCLUDED.credits RETURNING credits",
        )
        .bind(id)
        .bind(amount)
        .fetch_one(&mut *tx)
        .await?;
        crate::economy::ledger(&mut tx, id, amount, reason).await?;
        tx.commit().await?;
        Ok(balance)
    }

    /// Records a catch in the fish book; true when it is the player's first of that fish.
    pub async fn log_catch(&self, user: u64, fish_id: &str, size_cm: f32) -> anyhow::Result<bool> {
        let (caught,): (i32,) = sqlx::query_as(
            "INSERT INTO fish_book (user_id, fish_id, caught, best_cm) VALUES ($1, $2, 1, $3)
             ON CONFLICT (user_id, fish_id) DO UPDATE
             SET caught = fish_book.caught + 1, best_cm = GREATEST(fish_book.best_cm, EXCLUDED.best_cm)
             RETURNING caught",
        )
        .bind(sql_id(user))
        .bind(fish_id)
        .bind(size_cm)
        .fetch_one(self.pool())
        .await?;
        Ok(caught == 1)
    }

    pub async fn fish_book_count(&self, user: u64) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM fish_book WHERE user_id = $1")
            .bind(sql_id(user))
            .fetch_one(self.pool())
            .await?;
        Ok(n)
    }
}
