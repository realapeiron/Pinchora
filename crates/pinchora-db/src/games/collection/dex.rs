use super::*;

/// A server's chat-drop settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct CollectionSettings {
    pub enabled: bool,
    pub channel_id: Option<i64>,
}

/// One entry of a player's dex.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct DexRow {
    pub entry_id: String,
    pub caught: i32,
    pub kept: i32,
    pub glimmer: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct WishlistRow {
    pub entry_id: String,
    pub ping_on_spawn: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CollectionRoll {
    pub entry_id: String,
    pub glimmer: bool,
    pub rolled_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
}

pub enum RollStart {
    Started(CollectionRoll),
    Wait(CollectionRoll),
    Cooldown,
    Limit,
}

pub enum RollClaim {
    Claimed(Granted),
    Gone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteCard {
    Cast,
    Gone,
    Full,
}

impl Db {
    // /Settings

    pub async fn collection_settings(&self, guild: u64) -> anyhow::Result<CollectionSettings> {
        let row: Option<CollectionSettings> =
            sqlx::query_as("SELECT enabled, channel_id FROM collection_settings WHERE guild_id = $1")
                .bind(sql_id(guild))
                .fetch_optional(self.pool())
                .await?;
        Ok(row.unwrap_or(CollectionSettings {
            enabled: false,
            channel_id: None,
        }))
    }

    /// Changes a server's settings: `enabled` when given, and the channel when `channel` is `Some`
    /// (`Some(None)` clears it).
    pub async fn set_collection_settings(
        &self,
        guild: u64,
        enabled: Option<bool>,
        channel: Option<Option<u64>>,
    ) -> anyhow::Result<CollectionSettings> {
        Ok(sqlx::query_as(
            "INSERT INTO collection_settings (guild_id, enabled, channel_id)
             VALUES ($1, COALESCE($2, FALSE), CASE WHEN $4 THEN $3 END)
             ON CONFLICT (guild_id) DO UPDATE
             SET enabled = COALESCE($2, collection_settings.enabled),
                 channel_id = CASE WHEN $4 THEN $3 ELSE collection_settings.channel_id END
             RETURNING enabled, channel_id",
        )
        .bind(sql_id(guild))
        .bind(enabled)
        .bind(channel.flatten().map(sql_id))
        .bind(channel.is_some())
        .fetch_one(self.pool())
        .await?)
    }

    // /Dex

    /// Every entry the player has, in the order they first got them.
    pub async fn dex(&self, user: u64) -> anyhow::Result<Vec<DexRow>> {
        Ok(sqlx::query_as::<_, DexRow>(
            "SELECT entry_id, caught, kept, glimmer FROM dex_entries WHERE user_id = $1 ORDER BY first_at, entry_id",
        )
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn fragments(&self, user: u64) -> anyhow::Result<i64> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT fragments FROM collection_players WHERE user_id = $1")
            .bind(sql_id(user))
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map_or(0, |(n,)| n))
    }

    pub async fn shards(&self, user: u64) -> anyhow::Result<i64> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT shards FROM collection_players WHERE user_id = $1")
            .bind(sql_id(user))
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map_or(0, |(n,)| n))
    }

    pub async fn collection_roll(&self, user: u64) -> anyhow::Result<Option<CollectionRoll>> {
        Ok(
            sqlx::query_as("SELECT entry_id, glimmer, rolled_at, claimed_at FROM collection_rolls WHERE user_id = $1")
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub async fn start_collection_roll(
        &self,
        user: u64,
        entry: &str,
        glimmer: bool,
        now: DateTime<Utc>,
    ) -> anyhow::Result<RollStart> {
        retry("start_collection_roll", || async {
            let mut tx = self.pool().begin().await?;
            lock_users(&mut tx, &[sql_id(user)]).await?;
            let cooling: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM deleted_users WHERE user_id=$1 AND collection_roll_until>$2)")
                .bind(sql_id(user)).bind(now).fetch_one(&mut *tx).await?;
            if cooling { return Ok(RollStart::Cooldown); }
            let previous: Option<CollectionRoll> = sqlx::query_as("SELECT entry_id, glimmer, rolled_at, claimed_at FROM collection_rolls WHERE user_id = $1 FOR UPDATE")
                .bind(sql_id(user)).fetch_optional(&mut *tx).await?;
            if let Some(old) = previous.filter(|r| r.rolled_at + Duration::hours(1) > now) {
                return Ok(RollStart::Wait(old));
            }
            let day = now.timestamp().div_euclid(86_400) as i32;
            let (plays,): (i32,) = sqlx::query_as("INSERT INTO collection_daily (user_id, day, game, plays) VALUES ($1, $2, 'critter_roll_start', 1)
                ON CONFLICT (user_id, day, game) DO UPDATE SET plays = collection_daily.plays + 1 RETURNING plays")
                .bind(sql_id(user)).bind(day).fetch_one(&mut *tx).await?;
            if plays > 6 { return Ok(RollStart::Limit); }
            let row: CollectionRoll = sqlx::query_as("INSERT INTO collection_rolls (user_id, entry_id, glimmer, rolled_at, claimed_at) VALUES ($1, $2, $3, $4, NULL)
                ON CONFLICT (user_id) DO UPDATE SET entry_id = EXCLUDED.entry_id, glimmer = EXCLUDED.glimmer, rolled_at = EXCLUDED.rolled_at, claimed_at = NULL
                RETURNING entry_id, glimmer, rolled_at, claimed_at")
                .bind(sql_id(user)).bind(entry).bind(glimmer).bind(now).fetch_one(&mut *tx).await?;
            tx.commit().await?;
            Ok(RollStart::Started(row))
        }).await
    }

    pub async fn claim_collection_roll(
        &self,
        user: u64,
        now: DateTime<Utc>,
        award: Award<'_>,
    ) -> anyhow::Result<RollClaim> {
        retry("claim_collection_roll", || async {
            let mut tx = self.pool().begin().await?;
            lock_users(&mut tx, &[sql_id(user)]).await?;
            let roll: Option<CollectionRoll> = sqlx::query_as(
                "SELECT entry_id, glimmer, rolled_at, claimed_at FROM collection_rolls WHERE user_id = $1 FOR UPDATE",
            )
            .bind(sql_id(user))
            .fetch_optional(&mut *tx)
            .await?;
            let Some(roll) = roll.filter(|r| {
                r.entry_id == award.entry && r.claimed_at.is_none() && r.rolled_at + Duration::minutes(10) > now
            }) else {
                return Ok(RollClaim::Gone);
            };
            sqlx::query("UPDATE collection_rolls SET claimed_at = $2 WHERE user_id = $1")
                .bind(sql_id(user))
                .bind(now)
                .execute(&mut *tx)
                .await?;
            let granted = grant(&mut tx, user, award, true, "critter_roll").await?;
            if roll.glimmer {
                sqlx::query("UPDATE dex_entries SET glimmer = glimmer + 1 WHERE user_id = $1 AND entry_id = $2")
                    .bind(sql_id(user))
                    .bind(award.entry)
                    .execute(&mut *tx)
                    .await?;
            }
            tx.commit().await?;
            Ok(RollClaim::Claimed(granted))
        })
        .await
    }

    pub async fn wishlist(&self, user: u64) -> anyhow::Result<Vec<WishlistRow>> {
        Ok(sqlx::query_as(
            "SELECT entry_id, ping_on_spawn FROM collection_wishlist WHERE user_id = $1 ORDER BY created_at, entry_id",
        )
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?)
    }

    /// At most ten wishes; a ping is explicit opt-in and may be switched off without removing the wish.
    pub async fn set_wish(&self, user: u64, entry: &str, ping: bool) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM collection_wishlist WHERE user_id = $1")
            .bind(sql_id(user))
            .fetch_one(&mut *tx)
            .await?;
        let exists: Option<(i64,)> =
            sqlx::query_as("SELECT 1::bigint FROM collection_wishlist WHERE user_id = $1 AND entry_id = $2")
                .bind(sql_id(user))
                .bind(entry)
                .fetch_optional(&mut *tx)
                .await?;
        if count >= 10 && exists.is_none() {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO collection_wishlist (user_id, entry_id, ping_on_spawn) VALUES ($1, $2, $3)
                     ON CONFLICT (user_id, entry_id) DO UPDATE SET ping_on_spawn = EXCLUDED.ping_on_spawn",
        )
        .bind(sql_id(user))
        .bind(entry)
        .bind(ping)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn remove_wish(&self, user: u64, entry: &str) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let removed = sqlx::query("DELETE FROM collection_wishlist WHERE user_id = $1 AND entry_id = $2")
            .bind(sql_id(user))
            .bind(entry)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            != 0;
        tx.commit().await?;
        Ok(removed)
    }

    /// Recycle one held duplicate, keeping the first copy in the book. Returns the shard balance.
    pub async fn recycle_critter(
        &self,
        user: u64,
        entry: &str,
        shards: i64,
        request: &str,
    ) -> anyhow::Result<Option<i64>> {
        anyhow::ensure!(
            shards > 0 && !request.is_empty() && request.len() <= 100,
            "invalid shard operation"
        );
        retry("recycle_critter", || async {
            let mut tx = self.pool().begin().await?;
            lock_users(&mut tx, &[sql_id(user)]).await?;
            if let Some((balance,)) = sqlx::query_as::<_, (i64,)>(
                "SELECT balance FROM collection_shard_ledger WHERE user_id = $1 AND request_id = $2 AND kind = $3")
                .bind(sql_id(user)).bind(request).bind("recycle").fetch_optional(&mut *tx).await? {
                return Ok(Some(balance));
            }
            let kept: Option<(i32,)> = sqlx::query_as("UPDATE dex_entries SET kept = kept - 1
                WHERE user_id = $1 AND entry_id = $2 AND kept > glimmer + 1 RETURNING kept")
                .bind(sql_id(user)).bind(entry).fetch_optional(&mut *tx).await?;
            if kept.is_none() { return Ok(None); }
            let (left,): (i64,) = sqlx::query_as("INSERT INTO collection_players (user_id, shards) VALUES ($1, $2)
                ON CONFLICT (user_id) DO UPDATE SET shards = collection_players.shards + EXCLUDED.shards RETURNING shards")
                .bind(sql_id(user)).bind(shards).fetch_one(&mut *tx).await?;
            sqlx::query("INSERT INTO collection_shard_ledger (user_id, request_id, kind, entry_id, delta, balance) VALUES ($1,$2,$3,$4,$5,$6)")
                .bind(sql_id(user)).bind(request).bind("recycle").bind(entry).bind(shards).bind(left)
                .execute(&mut *tx).await?;
            movement(&mut tx, sql_id(user), &format!("collection:{entry}"), -1, "collection_recycle").await?;
            movement(&mut tx, sql_id(user), "shards", shards, "collection_recycle").await?;
            tx.commit().await?;
            Ok(Some(left))
        }).await
    }

    /// The committed result used to replay a craft response without rerolling its displayed reward.
    pub async fn shard_craft_result(&self, user: u64, request: &str) -> anyhow::Result<Option<(String, i64, i64)>> {
        Ok(sqlx::query_as("SELECT entry_id, balance, -delta FROM collection_shard_ledger WHERE user_id=$1 AND request_id=$2 AND kind='craft'")
            .bind(sql_id(user)).bind(request).fetch_optional(self.pool()).await?)
    }

    /// Spend shards on one bound loot item. The caller chooses only from the collection recipe pool.
    pub async fn craft_shard_loot(
        &self,
        user: u64,
        cost: i64,
        item: &str,
        request: &str,
    ) -> anyhow::Result<Option<i64>> {
        anyhow::ensure!(
            cost > 0 && !request.is_empty() && request.len() <= 100,
            "invalid shard operation"
        );
        retry("craft_shard_loot", || async {
            let mut tx = self.pool().begin().await?;
            lock_users(&mut tx, &[sql_id(user)]).await?;
            if let Some((balance,)) = sqlx::query_as::<_, (i64,)>(
                "SELECT balance FROM collection_shard_ledger WHERE user_id = $1 AND request_id = $2 AND kind = $3")
                .bind(sql_id(user)).bind(request).bind("craft").fetch_optional(&mut *tx).await? {
                return Ok(Some(balance));
            }
            let left: Option<(i64,)> = sqlx::query_as(
                "UPDATE collection_players SET shards = shards - $2
                WHERE user_id = $1 AND shards >= $2 RETURNING shards",
            )
            .bind(sql_id(user))
            .bind(cost)
            .fetch_optional(&mut *tx)
            .await?;
            let Some((left,)) = left else {
                return Ok(None);
            };
            add_inventory(&mut tx, sql_id(user), &soulbound(item), 1).await?;
            movement(&mut tx, sql_id(user), "shards", -cost, "collection_craft").await?;
            movement(&mut tx, sql_id(user), &format!("item:{}", soulbound(item)), 1, "collection_craft").await?;
            sqlx::query("INSERT INTO collection_shard_ledger (user_id, request_id, kind, entry_id, delta, balance) VALUES ($1,$2,$3,$4,$5,$6)")
                .bind(sql_id(user)).bind(request).bind("craft").bind(item).bind(-cost).bind(left)
                .execute(&mut *tx).await?;
            tx.commit().await?;
            Ok(Some(left))
        })
        .await
    }

    /// Spends `cost` fragments on the decor piece `item` (an item id) and puts it in the inventory as a
    /// soulbound piece: it can be placed in their own house, but never sold, gifted or donated.
    /// Returns the fragments left, or `None` when the player cannot cover it.
    pub async fn exchange_fragments(&self, user: u64, cost: i64, item: &str) -> anyhow::Result<Option<i64>> {
        retry("exchange_fragments", || self.exchange_fragments_once(user, cost, item)).await
    }

    /// One attempt of [`Db::exchange_fragments`]. Like everything below that writes a player's rows, it
    /// takes the player's lock first (see the `locks` module).
    async fn exchange_fragments_once(&self, user: u64, cost: i64, item: &str) -> anyhow::Result<Option<i64>> {
        let id = sql_id(user);
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[id]).await?;
        let left: Option<(i64,)> = sqlx::query_as(
            "UPDATE collection_players SET fragments = fragments - $2
             WHERE user_id = $1 AND fragments >= $2 RETURNING fragments",
        )
        .bind(id)
        .bind(cost)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((left,)) = left else {
            return Ok(None);
        };
        add_inventory(&mut tx, id, &soulbound(item), 1).await?;
        movement(&mut tx, id, "fragments", -cost, "collection_exchange").await?;
        movement(
            &mut tx,
            id,
            &format!("item:{}", soulbound(item)),
            1,
            "collection_exchange",
        )
        .await?;
        tx.commit().await?;
        Ok(Some(left))
    }
}
