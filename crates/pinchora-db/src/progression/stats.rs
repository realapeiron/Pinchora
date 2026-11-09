//! Server stats: the hourly and daily numbers the bot flushes from Redis, the counter channels, and
//! the daily totals of the whole bot. Rules (periods, buckets, names) are `pinchora_core::admin::server_stats`.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use pinchora_core::admin::server_stats::HourRow;
use pinchora_core::admin::server_stats::HourTally;
use pinchora_core::admin::server_stats::MAX_COUNTERS;

use crate::Db;
use crate::infra::locks::lock_users;
use crate::infra::locks::retry;
use crate::moderation::audit::NewAudit;
use crate::sql_id;

/// A channel or member with what they did in a stretch of days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct ActivityRank {
    pub id: i64,
    pub messages: i64,
    pub voice_minutes: i64,
}

/// One day of a channel or a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct ActivityDay {
    pub day: NaiveDate,
    pub messages: i64,
    pub voice_minutes: i64,
}

/// A counter channel.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CounterRow {
    pub id: i64,
    pub guild_id: i64,
    pub channel_id: i64,
    /// A `CounterKind` key.
    pub kind: String,
    /// The role a `role_members` counter counts.
    pub role_id: Option<i64>,
    pub template: String,
    /// The number it showed last time it was worked out.
    pub last_value: Option<i64>,
    /// The name the bot last gave the channel.
    pub last_name: Option<String>,
    pub last_rename_at: Option<DateTime<Utc>>,
    /// Why it could not be renamed last time (a missing permission), in English.
    pub problem: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// What saving a counter came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CounterSave {
    /// Added, or the counter already on that channel changed.
    Saved(CounterRow),
    /// The server has [`MAX_COUNTERS`] already.
    Full,
}

/// The totals of the whole bot on one day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct GlobalDay {
    pub day: NaiveDate,
    pub servers: i64,
    pub players: i64,
    pub houses: i64,
    pub fish: i64,
    pub games: i64,
    pub messages: i64,
}

/// The totals of the whole bot now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GlobalTotals {
    pub servers: i64,
    pub players: i64,
    pub houses: i64,
    pub pets: i64,
    /// Fish caught, all players together.
    pub fish: i64,
    /// Games played, counted once per player in each.
    pub games: i64,
}

const COUNTER_COLUMNS: &str = "id, guild_id, channel_id, kind, role_id, template, last_value, last_name, last_rename_at, \
                               problem, created_at";

/// How many rows one sweep statement removes at most.
const SWEEP_BATCH: i64 = 5_000;
/// How long Postgres remembers the batches it took: past the time Redis keeps an unwritten one.
const FLUSHES_KEPT: Duration = Duration::days(4);

fn clamp(n: i64) -> i32 {
    n.clamp(0, i64::from(i32::MAX)) as i32
}

impl Db {
    /// Writes one sealed batch of counts: the server's hour, its channels' and members' days. Returns
    /// `false` when the batch was written before (a flush that stopped before Redis forgot it), in which
    /// case nothing changes. Takes the members' locks first (see the `locks` module).
    pub async fn write_stats_batch(
        &self,
        batch: &str,
        guild: u64,
        hour: DateTime<Utc>,
        tally: &HourTally,
    ) -> anyhow::Result<bool> {
        let users: Vec<i64> = tally.members.keys().map(|u| sql_id(*u)).collect();
        let user_messages: Vec<i32> = tally.members.values().map(|p| clamp(p.messages)).collect();
        let user_voice: Vec<i32> = tally.members.values().map(|p| clamp(p.voice_minutes)).collect();
        let channels: Vec<i64> = tally.channels.keys().map(|c| sql_id(*c)).collect();
        let channel_messages: Vec<i32> = tally.channels.values().map(|p| clamp(p.messages)).collect();
        let channel_voice: Vec<i32> = tally.channels.values().map(|p| clamp(p.voice_minutes)).collect();
        // A negative guild key keeps public-safe counts separate without mixing old parent/thread rows.
        let public_channels: Vec<i64> = tally.public_channels.keys().map(|c| sql_id(*c)).collect();
        let public_messages: Vec<i32> = tally.public_channels.values().map(|p| clamp(p.messages)).collect();
        let public_voice: Vec<i32> = tally.public_channels.values().map(|p| clamp(p.voice_minutes)).collect();
        let day = hour.date_naive();
        let (users, user_messages, user_voice) = (&users, &user_messages, &user_voice);
        let (channels, channel_messages, channel_voice) = (&channels, &channel_messages, &channel_voice);
        let (public_channels, public_messages, public_voice) = (&public_channels, &public_messages, &public_voice);
        retry("write_stats_batch", || async move {
            let mut tx = self.pool().begin().await?;
            lock_users(&mut tx, users).await?;
            let fresh = sqlx::query("INSERT INTO stats_flushes (batch) VALUES ($1) ON CONFLICT DO NOTHING")
                .bind(batch)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                == 1;
            if !fresh {
                return Ok(false);
            }
            sqlx::query(
                "WITH h AS (
                     INSERT INTO guild_activity_hourly (guild_id, hour, messages, voice_minutes, joins, leaves, commands)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)
                     ON CONFLICT (guild_id, hour) DO UPDATE SET
                         messages = guild_activity_hourly.messages + EXCLUDED.messages,
                         voice_minutes = guild_activity_hourly.voice_minutes + EXCLUDED.voice_minutes,
                         joins = guild_activity_hourly.joins + EXCLUDED.joins,
                         leaves = guild_activity_hourly.leaves + EXCLUDED.leaves,
                         commands = guild_activity_hourly.commands + EXCLUDED.commands
                 ), c AS (
                     INSERT INTO channel_activity_daily (guild_id, channel_id, day, messages, voice_minutes)
                     SELECT $1, channel_id, $8, messages, voice_minutes
                     FROM UNNEST($9::BIGINT[], $10::INT[], $11::INT[]) AS t(channel_id, messages, voice_minutes)
                     ON CONFLICT (guild_id, channel_id, day) DO UPDATE SET
                         messages = channel_activity_daily.messages + EXCLUDED.messages,
                         voice_minutes = channel_activity_daily.voice_minutes + EXCLUDED.voice_minutes
                 ), pc AS (
                     INSERT INTO channel_activity_daily (guild_id, channel_id, day, messages, voice_minutes)
                     SELECT -$1, channel_id, $8, messages, voice_minutes
                     FROM UNNEST($15::BIGINT[], $16::INT[], $17::INT[]) AS t(channel_id, messages, voice_minutes)
                     ON CONFLICT (guild_id, channel_id, day) DO UPDATE SET
                         messages = channel_activity_daily.messages + EXCLUDED.messages,
                         voice_minutes = channel_activity_daily.voice_minutes + EXCLUDED.voice_minutes
                 )
                 INSERT INTO member_activity_daily (guild_id, user_id, day, messages, voice_minutes)
                 SELECT $1, user_id, $8, messages, voice_minutes
                 FROM UNNEST($12::BIGINT[], $13::INT[], $14::INT[]) AS t(user_id, messages, voice_minutes)
                 WHERE NOT EXISTS (
                     SELECT 1 FROM deleted_users d WHERE d.user_id = t.user_id AND d.deleted_at >= $2
                 )
                 ON CONFLICT (guild_id, user_id, day) DO UPDATE SET
                     messages = member_activity_daily.messages + EXCLUDED.messages,
                     voice_minutes = member_activity_daily.voice_minutes + EXCLUDED.voice_minutes",
            )
            .bind(sql_id(guild))
            .bind(hour)
            .bind(clamp(tally.messages))
            .bind(clamp(tally.voice_minutes))
            .bind(clamp(tally.joins))
            .bind(clamp(tally.leaves))
            .bind(clamp(tally.commands))
            .bind(day)
            .bind(channels)
            .bind(channel_messages)
            .bind(channel_voice)
            .bind(users)
            .bind(user_messages)
            .bind(user_voice)
            .bind(public_channels)
            .bind(public_messages)
            .bind(public_voice)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(true)
        })
        .await
    }

    /// Forgets the numbers of days before `before` (and the record of batches taken long ago), up to a
    /// few thousand rows per table; returns how many rows went. Rows somebody holds are left for the
    /// next sweep (see the `locks` module).
    pub async fn sweep_stats(&self, before: NaiveDate) -> anyhow::Result<u64> {
        let since = before.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc();
        let mut gone = 0;
        gone += sqlx::query(
            "DELETE FROM guild_activity_hourly WHERE (guild_id, hour) IN (
                 SELECT guild_id, hour FROM guild_activity_hourly WHERE hour < $1 LIMIT $2 FOR UPDATE SKIP LOCKED)",
        )
        .bind(since)
        .bind(SWEEP_BATCH)
        .execute(self.pool())
        .await?
        .rows_affected();
        gone += sqlx::query(
            "DELETE FROM channel_activity_daily WHERE (guild_id, channel_id, day) IN (
                 SELECT guild_id, channel_id, day FROM channel_activity_daily WHERE day < $1 LIMIT $2
                 FOR UPDATE SKIP LOCKED)",
        )
        .bind(before)
        .bind(SWEEP_BATCH)
        .execute(self.pool())
        .await?
        .rows_affected();
        gone += sqlx::query(
            "DELETE FROM member_activity_daily WHERE (guild_id, user_id, day) IN (
                 SELECT guild_id, user_id, day FROM member_activity_daily WHERE day < $1 LIMIT $2
                 FOR UPDATE SKIP LOCKED)",
        )
        .bind(before)
        .bind(SWEEP_BATCH)
        .execute(self.pool())
        .await?
        .rows_affected();
        gone += sqlx::query(
            "DELETE FROM stats_flushes WHERE batch IN (
                 SELECT batch FROM stats_flushes WHERE flushed_at < now() - make_interval(secs => $1) LIMIT $2
                 FOR UPDATE SKIP LOCKED)",
        )
        .bind(FLUSHES_KEPT.num_seconds() as f64)
        .bind(SWEEP_BATCH)
        .execute(self.pool())
        .await?
        .rows_affected();
        Ok(gone)
    }

    /// A server's stored hours from `from` on, oldest first.
    pub async fn guild_hours(&self, guild: u64, from: DateTime<Utc>) -> anyhow::Result<Vec<HourRow>> {
        let rows: Vec<(DateTime<Utc>, i32, i32, i32, i32, i32)> = sqlx::query_as(
            "SELECT hour, messages, voice_minutes, joins, leaves, commands FROM guild_activity_hourly
             WHERE guild_id = $1 AND hour >= $2 ORDER BY hour",
        )
        .bind(sql_id(guild))
        .bind(from)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(hour, messages, voice_minutes, joins, leaves, commands)| HourRow {
                hour,
                messages: messages.into(),
                voice_minutes: voice_minutes.into(),
                joins: joins.into(),
                leaves: leaves.into(),
                commands: commands.into(),
            })
            .collect())
    }

    /// The busiest channels from `from` on: most messages first, then most voice minutes.
    pub async fn top_channels(&self, guild: u64, from: NaiveDate, limit: i64) -> anyhow::Result<Vec<ActivityRank>> {
        Ok(sqlx::query_as(
            "SELECT channel_id AS id, sum(messages)::BIGINT AS messages, sum(voice_minutes)::BIGINT AS voice_minutes
             FROM channel_activity_daily WHERE guild_id = $1 AND day >= $2
             GROUP BY channel_id ORDER BY messages DESC, voice_minutes DESC, channel_id LIMIT $3",
        )
        .bind(sql_id(guild))
        .bind(from)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Daily message and voice totals for channels the reader may see.
    pub async fn visible_channel_days(
        &self,
        guild: u64,
        from: NaiveDate,
        channels: &[i64],
    ) -> anyhow::Result<Vec<ActivityDay>> {
        Ok(sqlx::query_as(
            "SELECT day, sum(messages)::BIGINT AS messages, sum(voice_minutes)::BIGINT AS voice_minutes
             FROM channel_activity_daily WHERE guild_id = $1 AND day >= $2 AND channel_id = ANY($3)
             GROUP BY day ORDER BY day",
        )
        .bind(sql_id(guild))
        .bind(from)
        .bind(channels)
        .fetch_all(self.pool())
        .await?)
    }

    /// Only the separately counted public scope; older mixed thread rows are never read here.
    pub async fn public_visible_channel_days(
        &self,
        guild: u64,
        from: NaiveDate,
        channels: &[i64],
    ) -> anyhow::Result<Vec<ActivityDay>> {
        self.visible_channel_days_by_guild(-sql_id(guild), from, channels).await
    }

    async fn visible_channel_days_by_guild(
        &self,
        guild: i64,
        from: NaiveDate,
        channels: &[i64],
    ) -> anyhow::Result<Vec<ActivityDay>> {
        Ok(sqlx::query_as(
            "SELECT day, sum(messages)::BIGINT AS messages, sum(voice_minutes)::BIGINT AS voice_minutes
             FROM channel_activity_daily WHERE guild_id = $1 AND day >= $2 AND channel_id = ANY($3)
             GROUP BY day ORDER BY day",
        )
        .bind(guild)
        .bind(from)
        .bind(channels)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn public_top_channels(
        &self,
        guild: u64,
        from: NaiveDate,
        limit: i64,
    ) -> anyhow::Result<Vec<ActivityRank>> {
        Ok(sqlx::query_as(
            "SELECT channel_id AS id, sum(messages)::BIGINT AS messages, sum(voice_minutes)::BIGINT AS voice_minutes
             FROM channel_activity_daily WHERE guild_id = $1 AND day >= $2
             GROUP BY channel_id ORDER BY messages DESC, voice_minutes DESC, channel_id LIMIT $3",
        )
        .bind(-sql_id(guild))
        .bind(from)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// The most active members from `from` on, leaving out players who hide from the boards.
    pub async fn top_members(&self, guild: u64, from: NaiveDate, limit: i64) -> anyhow::Result<Vec<ActivityRank>> {
        Ok(sqlx::query_as(
            "SELECT m.user_id AS id, sum(m.messages)::BIGINT AS messages, sum(m.voice_minutes)::BIGINT AS voice_minutes
             FROM member_activity_daily m
             WHERE m.guild_id = $1 AND m.day >= $2
               AND NOT EXISTS (SELECT 1 FROM user_privacy p WHERE p.user_id = m.user_id AND p.hide_leaderboard)
             GROUP BY m.user_id ORDER BY messages DESC, voice_minutes DESC, m.user_id LIMIT $3",
        )
        .bind(sql_id(guild))
        .bind(from)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// How many members wrote or sat in voice from `from` on.
    pub async fn active_members(&self, guild: u64, from: NaiveDate) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT count(DISTINCT user_id) FROM member_activity_daily WHERE guild_id = $1 AND day >= $2",
        )
        .bind(sql_id(guild))
        .bind(from)
        .fetch_one(self.pool())
        .await?;
        Ok(n)
    }

    /// One member's days from `from` on, oldest first; days with nothing are left out.
    pub async fn member_days(&self, guild: u64, user: u64, from: NaiveDate) -> anyhow::Result<Vec<ActivityDay>> {
        Ok(sqlx::query_as(
            "SELECT day, messages::BIGINT AS messages, voice_minutes::BIGINT AS voice_minutes FROM member_activity_daily
             WHERE guild_id = $1 AND user_id = $2 AND day >= $3 ORDER BY day",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .bind(from)
        .fetch_all(self.pool())
        .await?)
    }

    /// One channel's days from `from` on, oldest first; days with nothing are left out.
    pub async fn channel_days(&self, guild: u64, channel: u64, from: NaiveDate) -> anyhow::Result<Vec<ActivityDay>> {
        Ok(sqlx::query_as(
            "SELECT day, messages::BIGINT AS messages, voice_minutes::BIGINT AS voice_minutes FROM channel_activity_daily
             WHERE guild_id = $1 AND channel_id = $2 AND day >= $3 ORDER BY day",
        )
        .bind(sql_id(guild))
        .bind(sql_id(channel))
        .bind(from)
        .fetch_all(self.pool())
        .await?)
    }

    /// Where a member stands by messages from `from` on, as (place, out of): `None` when they wrote nothing.
    pub async fn member_place(&self, guild: u64, user: u64, from: NaiveDate) -> anyhow::Result<Option<(i64, i64)>> {
        self.place(
            "WITH t AS (SELECT user_id AS id, sum(messages) AS n FROM member_activity_daily
                        WHERE guild_id = $1 AND day >= $3 GROUP BY user_id HAVING sum(messages) > 0)
             SELECT (SELECT count(*) FROM t WHERE n > me.n) + 1, (SELECT count(*) FROM t) FROM t me WHERE me.id = $2",
            guild,
            user,
            from,
        )
        .await
    }

    /// Where a channel stands by messages from `from` on, as (place, out of): `None` when nobody wrote there.
    pub async fn channel_place(&self, guild: u64, channel: u64, from: NaiveDate) -> anyhow::Result<Option<(i64, i64)>> {
        self.place(
            "WITH t AS (SELECT channel_id AS id, sum(messages) AS n FROM channel_activity_daily
                        WHERE guild_id = $1 AND day >= $3 GROUP BY channel_id HAVING sum(messages) > 0)
             SELECT (SELECT count(*) FROM t WHERE n > me.n) + 1, (SELECT count(*) FROM t) FROM t me WHERE me.id = $2",
            guild,
            channel,
            from,
        )
        .await
    }

    async fn place(
        &self,
        sql: &'static str,
        guild: u64,
        id: u64,
        from: NaiveDate,
    ) -> anyhow::Result<Option<(i64, i64)>> {
        Ok(sqlx::query_as(sql)
            .bind(sql_id(guild))
            .bind(sql_id(id))
            .bind(from)
            .fetch_optional(self.pool())
            .await?)
    }

    /// Whether the server's stats page is open to anyone with the link.
    pub async fn stats_public(&self, guild: u64) -> anyhow::Result<bool> {
        let found: Option<(bool,)> = sqlx::query_as("SELECT stats_public FROM guilds WHERE id = $1")
            .bind(sql_id(guild))
            .fetch_optional(self.pool())
            .await?;
        Ok(found.is_some_and(|(on,)| on))
    }

    /// Opens or closes the server's stats page to anyone with the link.
    pub async fn set_stats_public(&self, guild: u64, public: bool) -> anyhow::Result<()> {
        self.guild_settings(guild).await?;
        sqlx::query("UPDATE guilds SET stats_public = $2 WHERE id = $1")
            .bind(sql_id(guild))
            .bind(public)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Change publication and record who did it in the same transaction.
    pub async fn set_stats_public_audited(
        &self,
        guild: u64,
        public: bool,
        actor: u64,
        actor_name: &str,
    ) -> anyhow::Result<bool> {
        self.guild_settings(guild).await?;
        let mut tx = self.pool().begin().await?;
        let (was,): (bool,) = sqlx::query_as("SELECT stats_public FROM guilds WHERE id = $1 FOR UPDATE")
            .bind(sql_id(guild))
            .fetch_one(&mut *tx)
            .await?;
        if was != public {
            sqlx::query("UPDATE guilds SET stats_public = $2 WHERE id = $1")
                .bind(sql_id(guild))
                .bind(public)
                .execute(&mut *tx)
                .await?;
            let before = serde_json::json!({"public": was}).to_string();
            let after = serde_json::json!({"public": public}).to_string();
            let audit = NewAudit {
                guild,
                actor,
                actor_name,
                source: crate::AuditSource::Web,
                action: "stats.public",
                target: None,
                before: Some(&before),
                after: Some(&after),
            };
            crate::moderation::audit::insert_in(&mut tx, &audit).await?;
        }
        tx.commit().await?;
        Ok(was != public)
    }

    /// The server's counter channels, oldest first.
    pub async fn counters(&self, guild: u64) -> anyhow::Result<Vec<CounterRow>> {
        let sql = format!("SELECT {COUNTER_COLUMNS} FROM counter_channels WHERE guild_id = $1 ORDER BY created_at, id");
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(guild))
            .fetch_all(self.pool())
            .await?)
    }

    /// Adds a counter on `channel`, or changes the one already there; at most [`MAX_COUNTERS`] per
    /// server, counted under a per-server lock so two adds at once cannot pass it.
    pub async fn save_counter(
        &self,
        guild: u64,
        channel: u64,
        kind: &str,
        role: Option<u64>,
        template: &str,
    ) -> anyhow::Result<CounterSave> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('counters:' || $1::TEXT, 0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        let (count, here): (i64, bool) = sqlx::query_as(
            "SELECT count(*), coalesce(bool_or(channel_id = $2), false) FROM counter_channels WHERE guild_id = $1",
        )
        .bind(sql_id(guild))
        .bind(sql_id(channel))
        .fetch_one(&mut *tx)
        .await?;
        if !here && count >= MAX_COUNTERS as i64 {
            return Ok(CounterSave::Full);
        }
        let sql = format!(
            "INSERT INTO counter_channels (guild_id, channel_id, kind, role_id, template) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (guild_id, channel_id) DO UPDATE SET kind = EXCLUDED.kind, role_id = EXCLUDED.role_id,
                 template = EXCLUDED.template, last_value = NULL, problem = NULL, checked_at = NULL
             RETURNING {COUNTER_COLUMNS}"
        );
        let row: CounterRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(sql_id(guild))
            .bind(sql_id(channel))
            .bind(kind)
            .bind(role.map(sql_id))
            .bind(template)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(CounterSave::Saved(row))
    }

    /// Removes the counter on `channel`; `false` when there was none. The channel keeps its last name.
    pub async fn remove_counter(&self, guild: u64, channel: u64) -> anyhow::Result<bool> {
        Ok(
            sqlx::query("DELETE FROM counter_channels WHERE guild_id = $1 AND channel_id = $2")
                .bind(sql_id(guild))
                .bind(sql_id(channel))
                .execute(self.pool())
                .await?
                .rows_affected()
                > 0,
        )
    }

    /// Takes up to `limit` servers whose counters were last worked out [`RENAME_GAP`] ago or more (or
    /// never) and returns all their counters, marked as checked at `now`: another pod skips them until
    /// they are due again.
    ///
    /// [`RENAME_GAP`]: pinchora_core::admin::server_stats::RENAME_GAP
    pub async fn claim_counters(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<CounterRow>> {
        let due = now - pinchora_core::admin::server_stats::RENAME_GAP;
        let sql = format!(
            "UPDATE counter_channels SET checked_at = $1
             WHERE id IN (
                 SELECT id FROM counter_channels
                 WHERE guild_id IN (SELECT guild_id FROM counter_channels
                                    WHERE checked_at IS NULL OR checked_at <= $2
                                    GROUP BY guild_id ORDER BY min(checked_at) NULLS FIRST LIMIT $3)
                 FOR UPDATE SKIP LOCKED)
             RETURNING {COUNTER_COLUMNS}"
        );
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(now)
            .bind(due)
            .bind(limit)
            .fetch_all(self.pool())
            .await?)
    }

    /// Notes what a counter came to: its number, the name it was given (and when) if it was renamed,
    /// and what kept it from being renamed (`None` clears an old problem).
    pub async fn record_counter(
        &self,
        id: i64,
        value: Option<i64>,
        renamed: Option<(&str, DateTime<Utc>)>,
        problem: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE counter_channels SET last_value = coalesce($2, last_value), problem = $5,
                 last_name = coalesce($3, last_name), last_rename_at = coalesce($4, last_rename_at)
             WHERE id = $1",
        )
        .bind(id)
        .bind(value)
        .bind(renamed.map(|(name, _)| name))
        .bind(renamed.map(|(_, at)| at))
        .bind(problem)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Houses built by the server's players (anyone with XP or messages counted here).
    pub async fn guild_houses(&self, guild: u64) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM houses h JOIN members m ON m.user_id = h.user_id WHERE m.guild_id = $1",
        )
        .bind(sql_id(guild))
        .fetch_one(self.pool())
        .await?;
        Ok(n)
    }

    /// Fish caught by the server's players.
    pub async fn guild_fish(&self, guild: u64) -> anyhow::Result<i64> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT coalesce(sum(f.caught), 0)::BIGINT FROM fish_book f JOIN members m ON m.user_id = f.user_id
             WHERE m.guild_id = $1",
        )
        .bind(sql_id(guild))
        .fetch_one(self.pool())
        .await?;
        Ok(n)
    }

    /// The server XP of every player who earned some here, for their average level.
    pub async fn guild_xp(&self, guild: u64) -> anyhow::Result<Vec<i64>> {
        let rows: Vec<(i64,)> = sqlx::query_as("SELECT xp FROM members WHERE guild_id = $1 AND xp > 0")
            .bind(sql_id(guild))
            .fetch_all(self.pool())
            .await?;
        Ok(rows.into_iter().map(|(xp,)| xp).collect())
    }

    /// The totals of the whole bot now; scans whole tables, so callers keep the answer for a while.
    pub async fn global_totals(&self) -> anyhow::Result<GlobalTotals> {
        let (servers, players, houses, pets, fish, games): (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM guilds), (SELECT count(*) FROM users), (SELECT count(*) FROM houses),
                    (SELECT count(*) FROM pets), (SELECT coalesce(sum(caught), 0)::BIGINT FROM fish_book),
                    (SELECT coalesce(sum(played), 0)::BIGINT FROM game_stats)",
        )
        .fetch_one(self.pool())
        .await?;
        Ok(GlobalTotals {
            servers,
            players,
            houses,
            pets,
            fish,
            games,
        })
    }

    /// Writes down the bot's totals for `day`, with the messages counted that day so far; run again
    /// later the same day, it brings them up to date.
    pub async fn snapshot_global_stats(&self, day: NaiveDate) -> anyhow::Result<()> {
        let totals = self.global_totals().await?;
        let start = day.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc();
        sqlx::query(
            "INSERT INTO global_stats_daily (day, servers, players, houses, fish, games, messages)
             SELECT $1, $2, $3, $4, $5, $6, coalesce(sum(messages), 0)::BIGINT FROM guild_activity_hourly
             WHERE hour >= $7 AND hour < $7 + interval '1 day'
             ON CONFLICT (day) DO UPDATE SET servers = EXCLUDED.servers, players = EXCLUDED.players,
                 houses = EXCLUDED.houses, fish = EXCLUDED.fish, games = EXCLUDED.games, messages = EXCLUDED.messages",
        )
        .bind(day)
        .bind(totals.servers)
        .bind(totals.players)
        .bind(totals.houses)
        .bind(totals.fish)
        .bind(totals.games)
        .bind(start)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Recompute messages in retained snapshots after late or retried hourly batches.
    pub async fn refresh_global_message_history(&self, from: NaiveDate) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE global_stats_daily d SET messages = (
                 SELECT coalesce(sum(h.messages), 0)::BIGINT FROM guild_activity_hourly h
                 WHERE h.hour >= d.day::timestamptz AND h.hour < (d.day + 1)::timestamptz
             ) WHERE d.day >= $1",
        )
        .bind(from)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The bot's daily totals from `from` on, oldest first.
    pub async fn global_days(&self, from: NaiveDate) -> anyhow::Result<Vec<GlobalDay>> {
        Ok(sqlx::query_as(
            "SELECT day, servers, players, houses, fish, games, messages FROM global_stats_daily
             WHERE day >= $1 ORDER BY day",
        )
        .bind(from)
        .fetch_all(self.pool())
        .await?)
    }
}
