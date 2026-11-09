use super::*;

impl Db {
    // /Beachcomb

    /// A walk along the shore: unless the player walked less than `wait` ago, files `award`'s find
    /// in their dex, pays it and starts the wait.
    pub async fn beachcomb(
        &self,
        user: u64,
        now: DateTime<Utc>,
        wait: Duration,
        award: Award<'_>,
        daily: Daily<'_>,
    ) -> anyhow::Result<Comb> {
        let mut tx = self.pool().begin().await?;
        // The wait can sit in a tombstone (see `Db::delete_user_data`): the player's lock first, so a
        // delete still under way is waited for and its tombstone is the one read.
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let ready: Option<DateTime<Utc>> = lock_player(&mut tx, user)
            .await?
            .and_then(|row: (Option<DateTime<Utc>>,)| row.0);
        if let Some(ready) = ready.filter(|r| *r > now) {
            return Ok(Comb::Wait { ready_at: ready });
        }
        sqlx::query("UPDATE collection_players SET comb_ready_at = $2 WHERE user_id = $1")
            .bind(sql_id(user))
            .bind(now + wait)
            .execute(&mut *tx)
            .await?;
        let paid = paid_play(&mut tx, user, daily).await?;
        let granted = grant(&mut tx, user, award, paid, "beachcomb").await?;
        tx.commit().await?;
        Ok(Comb::Found { granted, paid })
    }

    // /Crab pot

    pub async fn pot(&self, user: u64) -> anyhow::Result<PotRow> {
        let row: Option<(i16, Option<i16>, Option<DateTime<Utc>>)> =
            sqlx::query_as("SELECT pot_level, pot_hours, pot_ends_at FROM collection_players WHERE user_id = $1")
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?;
        Ok(row.map_or(
            PotRow {
                level: 1,
                hours: None,
                ends_at: None,
            },
            |(level, hours, ends_at)| PotRow { level, hours, ends_at },
        ))
    }

    /// Sets the pot for `hours`. False when it already has a soak running or waiting to be hauled.
    pub async fn start_pot(&self, user: u64, hours: i16, now: DateTime<Utc>) -> anyhow::Result<bool> {
        sqlx::query("INSERT INTO collection_players (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
            .bind(sql_id(user))
            .execute(self.pool())
            .await?;
        let set = sqlx::query(
            "UPDATE collection_players SET pot_hours = $2, pot_ends_at = $3
             WHERE user_id = $1 AND pot_ends_at IS NULL",
        )
        .bind(sql_id(user))
        .bind(hours)
        .bind(now + Duration::hours(i64::from(hours)))
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(set == 1)
    }

    /// Hauls the pot in once its soak is done: `catches` works out what it brought from the pot's level, the
    /// soak's hours and the time the soak ended (the pot is locked meanwhile), and the haul is filed, paid and
    /// cleared in one transaction, so a second haul finds [`PotHaul::Empty`].
    pub async fn haul_pot<'a, F>(
        &self,
        user: u64,
        now: DateTime<Utc>,
        daily: Daily<'_>,
        mut catches: F,
    ) -> anyhow::Result<PotHaul>
    where
        F: FnMut(i16, i16, DateTime<Utc>) -> Vec<Award<'a>>,
    {
        let mut retry = Retry::new("haul_pot");
        loop {
            let result = self.haul_pot_once(user, now, daily, &mut catches).await;
            if !retry.again(&result).await {
                return result;
            }
        }
    }

    /// One attempt of [`Db::haul_pot`]; `catches` is asked again if it loses a deadlock and runs twice.
    async fn haul_pot_once<'a, F>(
        &self,
        user: u64,
        now: DateTime<Utc>,
        daily: Daily<'_>,
        catches: &mut F,
    ) -> anyhow::Result<PotHaul>
    where
        F: FnMut(i16, i16, DateTime<Utc>) -> Vec<Award<'a>>,
    {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let row: Option<(i16, Option<i16>, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT pot_level, pot_hours, pot_ends_at FROM collection_players WHERE user_id = $1 FOR UPDATE",
        )
        .bind(sql_id(user))
        .fetch_optional(&mut *tx)
        .await?;
        let Some((level, Some(hours), Some(ends_at))) = row else {
            return Ok(PotHaul::Empty);
        };
        if ends_at > now {
            return Ok(PotHaul::Soaking { ends_at });
        }
        sqlx::query("UPDATE collection_players SET pot_hours = NULL, pot_ends_at = NULL WHERE user_id = $1")
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?;
        let paid = paid_play(&mut tx, user, daily).await?;
        let mut granted = Vec::new();
        for award in catches(level, hours, ends_at) {
            granted.push(grant(&mut tx, user, award, paid, "crab_pot").await?);
        }
        tx.commit().await?;
        Ok(PotHaul::Hauled {
            granted,
            paid,
            level,
            hours,
        })
    }

    /// Takes the pot from level `from` to the next for `cost` shells.
    pub async fn upgrade_pot(&self, user: u64, from: i16, cost: i64) -> anyhow::Result<PotUpgrade> {
        retry("upgrade_pot", || self.upgrade_pot_once(user, from, cost)).await
    }

    /// One attempt of [`Db::upgrade_pot`].
    async fn upgrade_pot_once(&self, user: u64, from: i16, cost: i64) -> anyhow::Result<PotUpgrade> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
        sqlx::query("INSERT INTO collection_players (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let (level,): (i16,) = sqlx::query_as("SELECT pot_level FROM collection_players WHERE user_id = $1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if level != from || level >= 5 {
            return Ok(PotUpgrade::Stale);
        }
        let paid: Option<(i64,)> =
            sqlx::query_as("UPDATE users SET credits = credits - $2 WHERE id = $1 AND credits >= $2 RETURNING credits")
                .bind(id)
                .bind(cost)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((balance,)) = paid else {
            let (balance,): (i64,) = sqlx::query_as("SELECT COALESCE((SELECT credits FROM users WHERE id = $1), 0)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            return Ok(PotUpgrade::Poor { balance });
        };
        ledger(&mut tx, id, -cost, "crab_pot_upgrade").await?;
        movement(&mut tx, id, "shells", -cost, "collection_upgrade").await?;
        sqlx::query("UPDATE collection_players SET pot_level = pot_level + 1 WHERE user_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(PotUpgrade::Upgraded {
            level: level + 1,
            balance,
        })
    }
}
