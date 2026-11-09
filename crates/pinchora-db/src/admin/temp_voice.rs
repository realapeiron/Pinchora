//! Temp voice: creator channels, the rooms that exist right now, and who an owner kicked out of one.

use chrono::{DateTime, Utc};
use pinchora_core::admin::temp_voice::MAX_CREATORS;
use pinchora_core::admin::temp_voice::MAX_ROOMS;

use crate::Db;
use crate::infra::locks::lock_users;
use crate::sql_id;

/// A voice channel a server set up as a creator.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct VoiceCreator {
    pub guild_id: i64,
    pub channel_id: i64,
    /// Where the rooms go; `None`: at the top of the channel list.
    pub category_id: Option<i64>,
    pub name_template: String,
    /// 0: no limit.
    pub default_limit: i32,
    pub default_locked: bool,
    pub max_per_user: i32,
    /// Whether each room gets the control buttons.
    pub panel: bool,
    /// A text channel whose private thread holds the room's panel and chat; `None`: the room's own chat.
    pub panel_channel_id: Option<i64>,
    pub enabled: bool,
}

/// A room that exists right now.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TempRoom {
    pub channel_id: i64,
    pub guild_id: i64,
    /// The creator that made it.
    pub source_id: i64,
    /// `None`: nobody owns it (the owner deleted their data); whoever is in it may claim it.
    pub owner_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub locked: bool,
    pub hidden: bool,
    pub user_limit: i32,
    pub last_rename_at: Option<DateTime<Utc>>,
    pub text_channel_id: Option<i64>,
    pub panel_channel_id: Option<i64>,
    pub panel_message_id: Option<i64>,
}

/// A room about to be recorded.
#[derive(Debug, Clone, Copy)]
pub struct NewTempRoom {
    pub channel_id: u64,
    pub guild_id: u64,
    pub source_id: u64,
    pub owner_id: u64,
    pub locked: bool,
    pub user_limit: i32,
}

/// What saving a creator did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatorSave {
    /// Saved; `true` when the creator is new.
    Saved(bool),
    /// The server has [`MAX_CREATORS`] already, and this one is new.
    Full,
    /// The channel is a room itself.
    Taken,
}

/// What recording a room did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomMake {
    Made(TempRoom),
    /// The server has [`MAX_ROOMS`] rooms alive already.
    Full,
    PerUserFull,
}

macro_rules! creator_columns {
    () => {
        "guild_id, channel_id, category_id, name_template, default_limit, default_locked, max_per_user, panel, \
         panel_channel_id, enabled"
    };
}

macro_rules! room_columns {
    () => {
        "channel_id, guild_id, source_id, owner_id, created_at, locked, hidden, user_limit, last_rename_at, \
         text_channel_id, panel_channel_id, panel_message_id"
    };
}

impl Db {
    /// Serializes voice events and destructive checks for the affected rooms across pods.
    pub async fn lock_temp_voice_rooms(
        &self,
        channels: &[u64],
    ) -> anyhow::Result<sqlx::Transaction<'_, sqlx::Postgres>> {
        let mut tx = self.pool().begin().await?;
        let mut channels = channels.to_vec();
        channels.sort_unstable();
        channels.dedup();
        for channel in channels {
            sqlx::query("SELECT pg_advisory_xact_lock($1)")
                .bind(sql_id(channel) ^ 0x7465_6d70_726f_6f6d)
                .execute(&mut *tx)
                .await?;
        }
        Ok(tx)
    }

    /// A transaction-scoped sweep fence; a slow Discord call cannot let a second pod enter.
    pub async fn lock_temp_voice_sweep(&self) -> anyhow::Result<sqlx::Transaction<'_, sqlx::Postgres>> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(0x7465_6d70_7377_6565_i64)
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }

    /// Holds the room's Discord overwrite mutation lock until the returned transaction is committed or dropped.
    pub async fn lock_temp_room_mutation(
        &self,
        channel_id: u64,
    ) -> anyhow::Result<sqlx::Transaction<'_, sqlx::Postgres>> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(sql_id(channel_id) ^ 0x7465_6d70_6f76_6572)
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
    /// The server's creators, oldest first.
    pub async fn voice_creators(&self, guild_id: u64) -> anyhow::Result<Vec<VoiceCreator>> {
        Ok(sqlx::query_as::<_, VoiceCreator>(concat!(
            "SELECT ",
            creator_columns!(),
            " FROM voice_creators WHERE guild_id = $1 ORDER BY created_at, channel_id"
        ))
        .bind(sql_id(guild_id))
        .fetch_all(self.pool())
        .await?)
    }

    /// One creator, by its channel.
    pub async fn voice_creator(&self, guild_id: u64, channel_id: u64) -> anyhow::Result<Option<VoiceCreator>> {
        Ok(sqlx::query_as::<_, VoiceCreator>(concat!(
            "SELECT ",
            creator_columns!(),
            " FROM voice_creators WHERE guild_id = $1 AND channel_id = $2"
        ))
        .bind(sql_id(guild_id))
        .bind(sql_id(channel_id))
        .fetch_optional(self.pool())
        .await?)
    }

    /// Whether the server has anything to do with temp voice: a creator, or a room alive.
    pub async fn voice_active(&self, guild_id: u64) -> anyhow::Result<bool> {
        let (active,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM voice_creators WHERE guild_id = $1)
                 OR EXISTS (SELECT 1 FROM temp_voice_channels WHERE guild_id = $1)",
        )
        .bind(sql_id(guild_id))
        .fetch_one(self.pool())
        .await?;
        Ok(active)
    }

    /// Saves a creator, or changes the one on this channel. The count of creators is checked under a lock on
    /// the server's rows, so two managers adding at once cannot go past the limit.
    pub async fn save_voice_creator(&self, creator: &VoiceCreator) -> anyhow::Result<CreatorSave> {
        let mut tx = self.pool().begin().await?;
        // Serialises the managers of one server; harmless when there is nothing to lock yet.
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(creator.guild_id ^ 0x7465_6d70_766f_6963)
            .execute(&mut *tx)
            .await?;
        let owner: Option<(i64,)> = sqlx::query_as("SELECT guild_id FROM voice_creators WHERE channel_id = $1")
            .bind(creator.channel_id)
            .fetch_optional(&mut *tx)
            .await?;
        let is_room: Option<(i64,)> =
            sqlx::query_as("SELECT channel_id FROM temp_voice_channels WHERE channel_id = $1")
                .bind(creator.channel_id)
                .fetch_optional(&mut *tx)
                .await?;
        if is_room.is_some() {
            return Ok(CreatorSave::Taken);
        }
        // Channel ids are unique across Discord, so a creator row for this channel in another server is
        // left over from one that is gone: this server takes the channel over, and counts it as new.
        let is_new = owner.is_none_or(|(guild,)| guild != creator.guild_id);
        if is_new {
            let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM voice_creators WHERE guild_id = $1")
                .bind(creator.guild_id)
                .fetch_one(&mut *tx)
                .await?;
            if count >= MAX_CREATORS as i64 {
                return Ok(CreatorSave::Full);
            }
        }
        sqlx::query(
            "INSERT INTO voice_creators (guild_id, channel_id, category_id, name_template, default_limit,
                                         default_locked, max_per_user, panel, panel_channel_id, enabled)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (channel_id) DO UPDATE SET
                 guild_id = EXCLUDED.guild_id,
                 category_id = EXCLUDED.category_id,
                 name_template = EXCLUDED.name_template,
                 default_limit = EXCLUDED.default_limit,
                 default_locked = EXCLUDED.default_locked,
                 max_per_user = EXCLUDED.max_per_user,
                 panel = EXCLUDED.panel,
                 panel_channel_id = EXCLUDED.panel_channel_id,
                 enabled = EXCLUDED.enabled",
        )
        .bind(creator.guild_id)
        .bind(creator.channel_id)
        .bind(creator.category_id)
        .bind(&creator.name_template)
        .bind(creator.default_limit)
        .bind(creator.default_locked)
        .bind(creator.max_per_user)
        .bind(creator.panel)
        .bind(creator.panel_channel_id)
        .bind(creator.enabled)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CreatorSave::Saved(is_new))
    }

    /// Stops a channel being a creator. Rooms it made stay until they empty.
    pub async fn remove_voice_creator(&self, guild_id: u64, channel_id: u64) -> anyhow::Result<bool> {
        let done = sqlx::query("DELETE FROM voice_creators WHERE guild_id = $1 AND channel_id = $2")
            .bind(sql_id(guild_id))
            .bind(sql_id(channel_id))
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The rooms alive in a server, oldest first.
    pub async fn temp_rooms(&self, guild_id: u64) -> anyhow::Result<Vec<TempRoom>> {
        Ok(sqlx::query_as::<_, TempRoom>(concat!(
            "SELECT ",
            room_columns!(),
            " FROM temp_voice_channels WHERE guild_id = $1 ORDER BY created_at, channel_id"
        ))
        .bind(sql_id(guild_id))
        .fetch_all(self.pool())
        .await?)
    }

    /// One room, by its voice channel.
    pub async fn temp_room(&self, channel_id: u64) -> anyhow::Result<Option<TempRoom>> {
        Ok(sqlx::query_as::<_, TempRoom>(concat!(
            "SELECT ",
            room_columns!(),
            " FROM temp_voice_channels WHERE channel_id = $1"
        ))
        .bind(sql_id(channel_id))
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn tracked_temp_voice_thread(&self, thread_id: u64) -> anyhow::Result<bool> {
        let (found,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM temp_voice_channels WHERE text_channel_id = $1)")
                .bind(sql_id(thread_id))
                .fetch_one(self.pool())
                .await?;
        Ok(found)
    }

    /// The room a channel belongs to: the voice channel itself, or the thread paired with it.
    pub async fn temp_room_of(&self, guild_id: u64, channel_id: u64) -> anyhow::Result<Option<TempRoom>> {
        Ok(sqlx::query_as::<_, TempRoom>(concat!(
            "SELECT ",
            room_columns!(),
            " FROM temp_voice_channels WHERE guild_id = $1 AND (channel_id = $2 OR text_channel_id = $2)"
        ))
        .bind(sql_id(guild_id))
        .bind(sql_id(channel_id))
        .fetch_optional(self.pool())
        .await?)
    }

    /// The rooms a member owns in a server, oldest first.
    pub async fn rooms_owned_by(&self, guild_id: u64, user: u64) -> anyhow::Result<Vec<TempRoom>> {
        Ok(sqlx::query_as::<_, TempRoom>(concat!(
            "SELECT ",
            room_columns!(),
            " FROM temp_voice_channels WHERE guild_id = $1 AND owner_id = $2 ORDER BY created_at, channel_id"
        ))
        .bind(sql_id(guild_id))
        .bind(sql_id(user))
        .fetch_all(self.pool())
        .await?)
    }

    /// How many rooms a creator has made that are still alive.
    pub async fn rooms_of_creator(&self, source_id: u64) -> anyhow::Result<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM temp_voice_channels WHERE source_id = $1")
            .bind(sql_id(source_id))
            .fetch_one(self.pool())
            .await?;
        Ok(count)
    }

    /// Records a room just made, unless the server has [`MAX_ROOMS`] already.
    pub async fn create_temp_room(&self, room: NewTempRoom) -> anyhow::Result<RoomMake> {
        let mut tx = self.reserve_temp_room(room.guild_id, room.owner_id).await?;
        let result = Self::create_temp_room_in(&mut tx, room).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Reserve guild capacity and this owner's slot before calling Discord.
    pub async fn reserve_temp_room(
        &self,
        guild_id: u64,
        owner_id: u64,
    ) -> anyhow::Result<sqlx::Transaction<'_, sqlx::Postgres>> {
        let had_user: Option<(DateTime<Utc>,)> = sqlx::query_as("SELECT created_at FROM users WHERE id = $1")
            .bind(sql_id(owner_id))
            .fetch_optional(self.pool())
            .await?;
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(owner_id)]).await?;
        let has_user: Option<(DateTime<Utc>,)> = sqlx::query_as("SELECT created_at FROM users WHERE id = $1")
            .bind(sql_id(owner_id))
            .fetch_optional(&mut *tx)
            .await?;
        if had_user.is_some() && had_user != has_user {
            anyhow::bail!("room creation overlapped player data deletion");
        }
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(sql_id(guild_id) ^ 0x7465_6d70_726f_6f6d)
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }

    /// Records a channel while its creation reservation is still held.
    pub async fn create_temp_room_in(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        room: NewTempRoom,
    ) -> anyhow::Result<RoomMake> {
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM temp_voice_channels WHERE guild_id = $1")
            .bind(sql_id(room.guild_id))
            .fetch_one(&mut **tx)
            .await?;
        if count >= MAX_ROOMS {
            return Ok(RoomMake::Full);
        }
        let (owned,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM temp_voice_channels WHERE source_id = $1 AND owner_id = $2")
                .bind(sql_id(room.source_id))
                .bind(sql_id(room.owner_id))
                .fetch_one(&mut **tx)
                .await?;
        let (max_per_user,): (i32,) =
            sqlx::query_as("SELECT COALESCE((SELECT max_per_user FROM voice_creators WHERE channel_id = $1), 1)")
                .bind(sql_id(room.source_id))
                .fetch_one(&mut **tx)
                .await?;
        if owned >= i64::from(max_per_user) {
            return Ok(RoomMake::PerUserFull);
        }
        let made = sqlx::query_as::<_, TempRoom>(concat!(
            "INSERT INTO temp_voice_channels (channel_id, guild_id, source_id, owner_id, locked, user_limit)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (channel_id) DO UPDATE SET owner_id = EXCLUDED.owner_id
             RETURNING ",
            room_columns!()
        ))
        .bind(sql_id(room.channel_id))
        .bind(sql_id(room.guild_id))
        .bind(sql_id(room.source_id))
        .bind(sql_id(room.owner_id))
        .bind(room.locked)
        .bind(room.user_limit)
        .fetch_one(&mut **tx)
        .await?;
        Ok(RoomMake::Made(made))
    }

    /// Forgets a room (and, with it, who was kicked from it). `true` when it was there.
    pub async fn delete_temp_room(&self, channel_id: u64) -> anyhow::Result<bool> {
        let done = sqlx::query("DELETE FROM temp_voice_channels WHERE channel_id = $1")
            .bind(sql_id(channel_id))
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Hands a room to `to`, but only if it is still `from`'s (`None`: nobody's). `false` when somebody got
    /// there first.
    pub async fn move_room_owner(&self, channel_id: u64, from: Option<u64>, to: u64) -> anyhow::Result<bool> {
        let had_user: Option<(DateTime<Utc>,)> = sqlx::query_as("SELECT created_at FROM users WHERE id = $1")
            .bind(sql_id(to))
            .fetch_optional(self.pool())
            .await?;
        let mut tx = self.pool().begin().await?;
        let mut users = vec![sql_id(to)];
        if let Some(from) = from {
            users.push(sql_id(from));
        }
        lock_users(&mut tx, &users).await?;
        let has_user: Option<(DateTime<Utc>,)> = sqlx::query_as("SELECT created_at FROM users WHERE id = $1")
            .bind(sql_id(to))
            .fetch_optional(&mut *tx)
            .await?;
        if had_user.is_some() && had_user != has_user {
            return Ok(false);
        }
        let room: Option<(i64, i64)> = sqlx::query_as(
            "SELECT guild_id, source_id FROM temp_voice_channels WHERE channel_id = $1 AND owner_id IS NOT DISTINCT FROM $2",
        )
        .bind(sql_id(channel_id))
        .bind(from.map(sql_id))
        .fetch_optional(&mut *tx)
        .await?;
        let Some((guild, source)) = room else {
            return Ok(false);
        };
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(guild ^ 0x7465_6d70_726f_6f6d)
            .execute(&mut *tx)
            .await?;
        let (owned,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM temp_voice_channels WHERE source_id = $1 AND owner_id = $2")
                .bind(source)
                .bind(sql_id(to))
                .fetch_one(&mut *tx)
                .await?;
        let (max_per_user,): (i32,) =
            sqlx::query_as("SELECT COALESCE((SELECT max_per_user FROM voice_creators WHERE channel_id = $1), 1)")
                .bind(source)
                .fetch_one(&mut *tx)
                .await?;
        if owned >= i64::from(max_per_user) {
            return Ok(false);
        }
        let done = sqlx::query(
            "UPDATE temp_voice_channels SET owner_id = $3 WHERE channel_id = $1 AND owner_id IS NOT DISTINCT FROM $2",
        )
        .bind(sql_id(channel_id))
        .bind(from.map(sql_id))
        .bind(sql_id(to))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(done.rows_affected() > 0)
    }

    /// Changes what the dashboard and the panel show of a room; `None` leaves a field as it is.
    pub async fn set_room_state(
        &self,
        channel_id: u64,
        locked: Option<bool>,
        hidden: Option<bool>,
        user_limit: Option<i32>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE temp_voice_channels SET
                 locked = COALESCE($2, locked),
                 hidden = COALESCE($3, hidden),
                 user_limit = COALESCE($4, user_limit)
             WHERE channel_id = $1",
        )
        .bind(sql_id(channel_id))
        .bind(locked)
        .bind(hidden)
        .bind(user_limit)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Notes that the room was renamed just now.
    pub async fn mark_room_renamed(&self, channel_id: u64, at: DateTime<Utc>) -> anyhow::Result<()> {
        sqlx::query("UPDATE temp_voice_channels SET last_rename_at = $2 WHERE channel_id = $1")
            .bind(sql_id(channel_id))
            .bind(at)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Where a room's panel message sits, and its paired thread.
    pub async fn set_room_panel(
        &self,
        channel_id: u64,
        text_channel: Option<u64>,
        panel_channel: Option<u64>,
        panel_message: Option<u64>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE temp_voice_channels SET text_channel_id = $2, panel_channel_id = $3, panel_message_id = $4
             WHERE channel_id = $1",
        )
        .bind(sql_id(channel_id))
        .bind(text_channel.map(sql_id))
        .bind(panel_channel.map(sql_id))
        .bind(panel_message.map(sql_id))
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Keeps `user` out of a room.
    pub async fn ban_from_room(&self, channel_id: u64, user: u64) -> anyhow::Result<()> {
        let had_user: Option<(DateTime<Utc>,)> = sqlx::query_as("SELECT created_at FROM users WHERE id = $1")
            .bind(sql_id(user))
            .fetch_optional(self.pool())
            .await?;
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let has_user: Option<(DateTime<Utc>,)> = sqlx::query_as("SELECT created_at FROM users WHERE id = $1")
            .bind(sql_id(user))
            .fetch_optional(&mut *tx)
            .await?;
        if had_user.is_some() && had_user != has_user {
            anyhow::bail!("room ban overlapped player data deletion");
        }
        sqlx::query(
            "INSERT INTO temp_voice_bans (channel_id, user_id) VALUES ($1, $2)
             ON CONFLICT DO NOTHING",
        )
        .bind(sql_id(channel_id))
        .bind(sql_id(user))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Lets `user` back into a room. `true` when they were kept out.
    pub async fn unban_from_room(&self, channel_id: u64, user: u64) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let done = sqlx::query("DELETE FROM temp_voice_bans WHERE channel_id = $1 AND user_id = $2")
            .bind(sql_id(channel_id))
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(done.rows_affected() > 0)
    }

    pub async fn is_banned_from_room(&self, channel_id: u64, user: u64) -> anyhow::Result<bool> {
        let (banned,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM temp_voice_bans WHERE channel_id = $1 AND user_id = $2)")
                .bind(sql_id(channel_id))
                .bind(sql_id(user))
                .fetch_one(self.pool())
                .await?;
        Ok(banned)
    }

    /// Everyone kept out of a room.
    pub async fn room_bans(&self, channel_id: u64) -> anyhow::Result<Vec<u64>> {
        let rows: Vec<(i64,)> =
            sqlx::query_as("SELECT user_id FROM temp_voice_bans WHERE channel_id = $1 ORDER BY user_id")
                .bind(sql_id(channel_id))
                .fetch_all(self.pool())
                .await?;
        Ok(rows.into_iter().map(|(id,)| id as u64).collect())
    }

    /// Rooms made before `before`, oldest first, from `after_channel` on: the sweep walks them in batches.
    pub async fn rooms_to_sweep(
        &self,
        before: DateTime<Utc>,
        after_channel: u64,
        limit: i64,
    ) -> anyhow::Result<Vec<TempRoom>> {
        Ok(sqlx::query_as::<_, TempRoom>(concat!(
            "SELECT ",
            room_columns!(),
            " FROM temp_voice_channels WHERE created_at < $1 AND channel_id > $2 ORDER BY channel_id LIMIT $3"
        ))
        .bind(before)
        .bind(sql_id(after_channel))
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }
}
