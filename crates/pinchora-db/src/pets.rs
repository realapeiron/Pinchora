use chrono::{DateTime, Utc};

use crate::{
    Db,
    locks::{Retry, lock_users, retry},
    sql_id,
};

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
    /// The field trip it is on or just came back from and has not been claimed (an id from
    /// `content/pet_trips.json`), and when it is back.
    pub trip_dest: Option<String>,
    pub trip_ends_at: Option<DateTime<Utc>>,
}

impl PetRow {
    /// Out on a trip that is not over yet: not at home, so not in the house and not to be cared for.
    pub fn is_away(&self, now: DateTime<Utc>) -> bool {
        self.trip_ends_at.is_some_and(|ends| ends > now)
    }

    /// Back from a trip whose souvenir nobody claimed yet.
    pub fn has_souvenir(&self, now: DateTime<Utc>) -> bool {
        self.trip_dest.is_some() && !self.is_away(now)
    }
}

macro_rules! pet_columns {
    () => {
        "id, owner_id, species, name, level, xp, hunger, fun, energy, hygiene, needs_at, active, trip_dest, trip_ends_at"
    };
}

/// What a claimed trip pays, worked out from the pet as the claim finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TripPayout {
    pub shells: i64,
    /// A catalog item id for the player's inventory.
    pub item: Option<String>,
    /// The pet's level and XP after the trip's XP.
    pub level: i32,
    pub xp: i32,
}

#[derive(Debug, Clone)]
pub enum TripClaim {
    /// Paid. `pet` is the pet as it was before the claim; `balance` is the player's shells now.
    Claimed { pet: PetRow, balance: i64 },
    /// The pet is still out there until `ends_at`.
    Away { trip: String, ends_at: DateTime<Utc> },
    /// No trip waits: the pet never went, or it was claimed already.
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    /// Awake. `free` when the day's free wake-up paid for it, otherwise `balance` is what the
    /// fee left.
    Woke { free: bool, balance: i64 },
    /// The pet changed since it was read (somebody woke it already): nothing was charged.
    Stale,
    /// Not enough shells for the fee, and the free wake-up is used.
    Poor { balance: i64 },
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
        retry("adopt", || self.adopt_once(owner, species, name)).await
    }

    /// One attempt of [`Db::adopt`].
    async fn adopt_once(&self, owner: u64, species: &str, name: &str) -> anyhow::Result<PetRow> {
        let id = sql_id(owner);
        let mut tx = self.pool().begin().await?;
        // Two adoptions at once take turns, so only one of them becomes the active pet.
        lock_users(&mut tx, &[id]).await?;
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
        retry("set_active_pet", || self.set_active_pet_once(owner, pet_id)).await
    }

    /// One attempt of [`Db::set_active_pet`].
    async fn set_active_pet_once(&self, owner: u64, pet_id: i64) -> anyhow::Result<bool> {
        let id = sql_id(owner);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
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

    /// Stores needs as of `at`, plus new XP and level, for a pet as the caller read it (`seen`):
    /// the write only lands if the pet is still in that state (same needs time, level, XP and
    /// trip), so a care click that raced a trip start, a claim, a wake-up or another care click
    /// cannot unfreeze the needs a trip froze or write over the XP a claim paid. Refused (false,
    /// nothing written) when the pet changed since, is out on a trip that is not over by `at`, or
    /// is not the player's.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_pet(
        &self,
        owner: u64,
        seen: &PetRow,
        needs: [i16; 4],
        level: i32,
        xp: i32,
        at: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        let n = sqlx::query(
            "UPDATE pets SET hunger = $3, fun = $4, energy = $5, hygiene = $6, level = $7, xp = $8, needs_at = $9
             WHERE id = $1 AND owner_id = $2
               AND needs_at = $10 AND level = $11 AND xp = $12
               AND trip_dest IS NOT DISTINCT FROM $13 AND trip_ends_at IS NOT DISTINCT FROM $14
               AND (trip_ends_at IS NULL OR trip_ends_at <= $9)",
        )
        .bind(seen.id)
        .bind(sql_id(owner))
        .bind(needs[0])
        .bind(needs[1])
        .bind(needs[2])
        .bind(needs[3])
        .bind(level)
        .bind(xp)
        .bind(at)
        .bind(seen.needs_at)
        .bind(seen.level)
        .bind(seen.xp)
        .bind(seen.trip_dest.as_deref())
        .bind(seen.trip_ends_at)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(n == 1)
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

    /// Sends a pet on a trip that ends at `ends_at`, with its needs frozen as `needs` until
    /// then. False when the pet is not the player's or is already out or waiting to be claimed.
    pub async fn start_trip(
        &self,
        owner: u64,
        pet_id: i64,
        trip: &str,
        needs: [i16; 4],
        ends_at: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        let n = sqlx::query(
            "UPDATE pets SET trip_dest = $3, trip_ends_at = $4, needs_at = $4,
                    hunger = $5, fun = $6, energy = $7, hygiene = $8
             WHERE id = $1 AND owner_id = $2 AND trip_dest IS NULL",
        )
        .bind(pet_id)
        .bind(sql_id(owner))
        .bind(trip)
        .bind(ends_at)
        .bind(needs[0])
        .bind(needs[1])
        .bind(needs[2])
        .bind(needs[3])
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(n == 1)
    }

    /// Pays the souvenir of a finished trip: shells, an item, the pet's XP. The pet is locked
    /// while `payout` works out the haul from it, and the trip is cleared in the same
    /// transaction as the payment, so a repeated claim finds [`TripClaim::Nothing`]. `payout` is
    /// asked again if the claim loses a deadlock and runs a second time; the last answer is paid.
    pub async fn claim_trip<F>(
        &self,
        owner: u64,
        pet_id: i64,
        now: DateTime<Utc>,
        mut payout: F,
    ) -> anyhow::Result<TripClaim>
    where
        F: FnMut(&PetRow, &str) -> TripPayout,
    {
        let mut retry = Retry::new("claim_trip");
        loop {
            let result = self.claim_trip_once(owner, pet_id, now, &mut payout).await;
            if !retry.again(&result).await {
                return result;
            }
        }
    }

    /// One attempt of [`Db::claim_trip`].
    async fn claim_trip_once<F>(
        &self,
        owner: u64,
        pet_id: i64,
        now: DateTime<Utc>,
        payout: &mut F,
    ) -> anyhow::Result<TripClaim>
    where
        F: FnMut(&PetRow, &str) -> TripPayout,
    {
        let id = sql_id(owner);
        let mut tx = self.pool().begin().await?;
        // The owner's lock comes before the pet's row (see the `locks` module).
        lock_users(&mut tx, &[id]).await?;
        let pet = sqlx::query_as::<_, PetRow>(concat!(
            "SELECT ",
            pet_columns!(),
            " FROM pets WHERE id = $1 AND owner_id = $2 FOR UPDATE"
        ))
        .bind(pet_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(pet) = pet else {
            return Ok(TripClaim::Nothing);
        };
        let (Some(trip), Some(ends_at)) = (pet.trip_dest.clone(), pet.trip_ends_at) else {
            return Ok(TripClaim::Nothing);
        };
        if ends_at > now {
            return Ok(TripClaim::Away { trip, ends_at });
        }
        let paid = payout(&pet, &trip);
        sqlx::query("UPDATE pets SET trip_dest = NULL, trip_ends_at = NULL, level = $2, xp = $3 WHERE id = $1")
            .bind(pet_id)
            .bind(paid.level)
            .bind(paid.xp)
            .execute(&mut *tx)
            .await?;
        let (balance,): (i64,) = sqlx::query_as(
            "INSERT INTO users (id, credits) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET credits = users.credits + EXCLUDED.credits RETURNING credits",
        )
        .bind(id)
        .bind(paid.shells)
        .fetch_one(&mut *tx)
        .await?;
        crate::economy::ledger(&mut tx, id, paid.shells, "pet_trip").await?;
        if let Some(item) = &paid.item {
            crate::house::add_inventory(&mut tx, id, item, 1).await?;
        }
        tx.commit().await?;
        Ok(TripClaim::Claimed { pet, balance })
    }

    /// Whether the player's free wake-up of the UTC day `day` (days since 1970) is still there. One
    /// used before deleting their data counts.
    pub async fn free_wake_ready(&self, user: u64, day: i32) -> anyhow::Result<bool> {
        let (used,): (Option<i32>,) = sqlx::query_as(
            "SELECT GREATEST((SELECT pet_wake_day FROM users WHERE id = $1),
                             (SELECT pet_wake_day FROM deleted_users WHERE user_id = $1))",
        )
        .bind(sql_id(user))
        .fetch_one(self.pool())
        .await?;
        Ok(used.is_none_or(|d| d < day))
    }

    /// Wakes a sleeping pet: its needs become `needs` as of `at`. The player's free wake-up of
    /// the day `day` pays for it, else `fee` shells. `seen_at` is the pet's `needs_at` as the
    /// caller read it; a pet that changed since is left alone ([`Wake::Stale`]).
    #[allow(clippy::too_many_arguments)]
    pub async fn wake_pet(
        &self,
        owner: u64,
        pet_id: i64,
        seen_at: DateTime<Utc>,
        needs: [i16; 4],
        at: DateTime<Utc>,
        day: i32,
        fee: i64,
    ) -> anyhow::Result<Wake> {
        retry("wake_pet", || {
            self.wake_pet_once(owner, pet_id, seen_at, needs, at, day, fee)
        })
        .await
    }

    /// One attempt of [`Db::wake_pet`].
    #[allow(clippy::too_many_arguments)]
    async fn wake_pet_once(
        &self,
        owner: u64,
        pet_id: i64,
        seen_at: DateTime<Utc>,
        needs: [i16; 4],
        at: DateTime<Utc>,
        day: i32,
        fee: i64,
    ) -> anyhow::Result<Wake> {
        let id = sql_id(owner);
        let mut tx = self.pool().begin().await?;
        // The owner's lock comes before the pet's row (see the `locks` module).
        lock_users(&mut tx, &[id]).await?;
        let current: Option<(DateTime<Utc>,)> =
            sqlx::query_as("SELECT needs_at FROM pets WHERE id = $1 AND owner_id = $2 FOR UPDATE")
                .bind(pet_id)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        if current.is_none_or(|(t,)| t != seen_at) {
            return Ok(Wake::Stale);
        }
        sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let free = sqlx::query(
            "UPDATE users SET pet_wake_day = $2 WHERE id = $1 AND (pet_wake_day IS NULL OR pet_wake_day < $2)",
        )
        .bind(id)
        .bind(day)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        let balance = if free {
            let (credits,): (i64,) = sqlx::query_as("SELECT credits FROM users WHERE id = $1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            credits
        } else {
            let paid: Option<(i64,)> = sqlx::query_as(
                "UPDATE users SET credits = credits - $2 WHERE id = $1 AND credits >= $2 RETURNING credits",
            )
            .bind(id)
            .bind(fee)
            .fetch_optional(&mut *tx)
            .await?;
            let Some((credits,)) = paid else {
                let (credits,): (i64,) = sqlx::query_as("SELECT credits FROM users WHERE id = $1")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
                return Ok(Wake::Poor { balance: credits });
            };
            crate::economy::ledger(&mut tx, id, -fee, "pet_wake").await?;
            credits
        };
        sqlx::query("UPDATE pets SET hunger = $2, fun = $3, energy = $4, hygiene = $5, needs_at = $6 WHERE id = $1")
            .bind(pet_id)
            .bind(needs[0])
            .bind(needs[1])
            .bind(needs[2])
            .bind(needs[3])
            .bind(at)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Wake::Woke { free, balance })
    }

    /// Takes shells for a purchase or fee; `None` when the player cannot afford it.
    pub async fn spend_shells(&self, user: u64, amount: i64, reason: &str) -> anyhow::Result<Option<i64>> {
        retry("spend_shells", || self.spend_shells_once(user, amount, reason)).await
    }

    /// One attempt of [`Db::spend_shells`].
    async fn spend_shells_once(&self, user: u64, amount: i64, reason: &str) -> anyhow::Result<Option<i64>> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
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
        retry("earn_shells", || self.earn_shells_once(user, amount, reason)).await
    }

    /// One attempt of [`Db::earn_shells`].
    async fn earn_shells_once(&self, user: u64, amount: i64, reason: &str) -> anyhow::Result<i64> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
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
        // The counts are for /wrapped; a catch that is logged is not undone by them.
        if let Err(error) = self.bump_activity(user, crate::Counter::Fish, 1).await {
            tracing::debug!(%error, user, "catch not counted");
        }
        if let Err(error) = self
            .add_catch_in(user, chrono::Datelike::year(&chrono::Utc::now()), fish_id)
            .await
        {
            tracing::debug!(%error, user, "catch not counted for the year");
        }
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

impl Db {
    /// Pays premium pearls (quest rewards, events).
    pub async fn earn_pearls(&self, user: u64, amount: i64) -> anyhow::Result<i64> {
        let (balance,): (i64,) = sqlx::query_as(
            "INSERT INTO users (id, pearls) VALUES ($1, $2)
             ON CONFLICT (id) DO UPDATE SET pearls = users.pearls + EXCLUDED.pearls RETURNING pearls",
        )
        .bind(sql_id(user))
        .bind(amount)
        .fetch_one(self.pool())
        .await?;
        Ok(balance)
    }
}
