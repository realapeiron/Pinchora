//! AFK records, sticky posts and claimed scheduled messages.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::FromRow;

use crate::{Db, sql_id};

#[derive(Debug, Clone, FromRow)]
pub struct AfkRow {
    pub guild_id: i64,
    pub user_id: i64,
    pub message: String,
    pub since: DateTime<Utc>,
    pub nick_prefixed: bool,
}

#[derive(Debug, Clone, FromRow)]
pub struct UtilityStickyRow {
    pub channel_id: i64,
    pub guild_id: i64,
    pub content: String,
    pub embed: Option<Value>,
    pub last_message_id: Option<i64>,
    pub min_gap_secs: i32,
    pub min_messages: i32,
    pub message_count: i32,
    pub enabled: bool,
    pub lease_until: Option<DateTime<Utc>>,
    pub pause_reason: Option<String>,
    pub updated_by: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ScheduledRow {
    pub id: i64,
    pub guild_id: i64,
    pub channel_id: i64,
    pub content: String,
    pub embed: Option<Value>,
    pub schedule: Value,
    pub tz: String,
    pub next_run_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub runs: i32,
    pub enabled: bool,
    pub lease_until: Option<DateTime<Utc>>,
    pub pause_reason: Option<String>,
    pub created_by: Option<i64>,
    pub ping_allowed: bool,
}

const STICKY: &str = "channel_id, guild_id, content, embed, last_message_id, min_gap_secs, min_messages, message_count, enabled, updated_by, lease_until, pause_reason";
const SCHEDULED: &str = "id, guild_id, channel_id, content, embed, schedule, tz, next_run_at, last_run_at, runs, enabled, created_by, ping_allowed, lease_until, pause_reason";

impl Db {
    pub async fn queue_sticky_cleanup(&self, channel: u64, copy: u64) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO sticky_copy_cleanup (channel_id,message_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
            .bind(sql_id(channel))
            .bind(sql_id(copy))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn sticky_cleanups(&self, limit: i64) -> anyhow::Result<Vec<(i64, i64)>> {
        Ok(
            sqlx::query_as("SELECT channel_id,message_id FROM sticky_copy_cleanup ORDER BY channel_id LIMIT $1")
                .bind(limit)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn finish_sticky_cleanup(&self, channel: u64, copy: u64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sticky_copy_cleanup WHERE channel_id=$1 AND message_id=$2")
            .bind(sql_id(channel))
            .bind(sql_id(copy))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn utility_offset(&self, guild: u64) -> anyhow::Result<String> {
        Ok(
            sqlx::query_scalar::<_, String>("SELECT utc_offset FROM utility_settings WHERE guild_id=$1")
                .bind(sql_id(guild))
                .fetch_optional(&self.pool)
                .await?
                .unwrap_or_else(|| "+00:00".into()),
        )
    }

    pub async fn set_utility_offset(&self, guild: u64, offset: &str) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO utility_settings (guild_id,utc_offset) VALUES ($1,$2) ON CONFLICT (guild_id) DO UPDATE SET utc_offset=$2")
            .bind(sql_id(guild)).bind(offset).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn claim_afk_reply(&self, channel: u64) -> anyhow::Result<bool> {
        Ok(sqlx::query("INSERT INTO afk_reply_limits (channel_id,last_at) VALUES ($1,now()) ON CONFLICT (channel_id) DO UPDATE SET last_at=now() WHERE afk_reply_limits.last_at < now()-INTERVAL '5 minutes'")
            .bind(sql_id(channel)).execute(&self.pool).await?.rows_affected() > 0)
    }
    pub async fn set_afk(&self, guild: u64, user: u64, message: &str, nick_prefixed: bool) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO afk (guild_id,user_id,message,nick_prefixed) VALUES ($1,$2,$3,$4) ON CONFLICT (guild_id,user_id) DO UPDATE SET message=$3,nick_prefixed=afk.nick_prefixed OR $4,since=now()")
            .bind(sql_id(guild)).bind(sql_id(user)).bind(message).bind(nick_prefixed).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn clear_afk(&self, guild: u64, user: u64, before: DateTime<Utc>) -> anyhow::Result<Vec<AfkRow>> {
        Ok(sqlx::query_as::<_, AfkRow>("DELETE FROM afk WHERE user_id=$1 AND guild_id IN ($2,0) AND since < $3 RETURNING guild_id,user_id,message,since,nick_prefixed")
            .bind(sql_id(user)).bind(sql_id(guild)).bind(before).fetch_all(&self.pool).await?)
    }

    pub async fn afk_for(&self, guild: u64, users: &[i64]) -> anyhow::Result<Vec<AfkRow>> {
        Ok(sqlx::query_as::<_, AfkRow>("SELECT DISTINCT ON (user_id) guild_id,user_id,message,since,nick_prefixed FROM afk WHERE guild_id IN ($1,0) AND user_id=ANY($2) ORDER BY user_id,guild_id DESC")
            .bind(sql_id(guild)).bind(users).fetch_all(&self.pool).await?)
    }

    pub async fn upsert_sticky(&self, row: &UtilityStickyRow) -> anyhow::Result<()> {
        let changed = sqlx::query("INSERT INTO sticky_messages (channel_id,guild_id,content,embed,min_gap_secs,min_messages,enabled,updated_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (channel_id) DO UPDATE SET guild_id=$2,content=$3,embed=$4,min_gap_secs=$5,min_messages=$6,enabled=$7,updated_by=$8,message_count=0,updated_at=now(),pause_reason=NULL WHERE sticky_messages.lease_until IS NULL OR sticky_messages.lease_until<now()")
            .bind(row.channel_id).bind(row.guild_id).bind(&row.content).bind(&row.embed).bind(row.min_gap_secs).bind(row.min_messages).bind(row.enabled).bind(row.updated_by).execute(&self.pool).await?.rows_affected();
        anyhow::ensure!(changed > 0, "sticky delivery in progress; retry after it finishes");
        Ok(())
    }

    pub async fn stickies(&self, guild: u64) -> anyhow::Result<Vec<UtilityStickyRow>> {
        Ok(sqlx::query_as::<_, UtilityStickyRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {STICKY} FROM sticky_messages WHERE guild_id=$1 ORDER BY channel_id"
        )))
        .bind(sql_id(guild))
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn remove_sticky(&self, guild: u64, channel: u64) -> anyhow::Result<Option<UtilityStickyRow>> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, UtilityStickyRow>(sqlx::AssertSqlSafe(format!(
            "DELETE FROM sticky_messages WHERE guild_id=$1 AND channel_id=$2 AND (lease_until IS NULL OR lease_until<now()) RETURNING {STICKY}"
        ))).bind(sql_id(guild)).bind(sql_id(channel)).fetch_optional(&mut *tx).await?;
        if let Some(copy) = row.as_ref().and_then(|r| r.last_message_id) {
            sqlx::query("INSERT INTO sticky_copy_cleanup(channel_id,message_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
                .bind(sql_id(channel))
                .bind(copy)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(row)
    }

    pub async fn utility_sticky_enabled(&self, channel: u64) -> anyhow::Result<bool> {
        Ok(
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sticky_messages WHERE channel_id=$1 AND enabled)")
                .bind(sql_id(channel))
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn count_sticky_messages(&self, channel: u64, count: i32) -> anyhow::Result<()> {
        sqlx::query("UPDATE sticky_messages SET message_count=LEAST(message_count::bigint+$2,1000)::integer WHERE channel_id=$1 AND enabled")
            .bind(sql_id(channel)).bind(i64::from(count)).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn claim_stickies(&self, limit: i64) -> anyhow::Result<Vec<UtilityStickyRow>> {
        Ok(sqlx::query_as::<_, UtilityStickyRow>(sqlx::AssertSqlSafe(format!("WITH due AS (SELECT channel_id FROM sticky_messages WHERE enabled AND message_count>=min_messages AND COALESCE(last_post_at,updated_at)+min_gap_secs * INTERVAL '1 second'<=now() AND (lease_until IS NULL OR lease_until<now()) ORDER BY updated_at LIMIT $1 FOR UPDATE SKIP LOCKED) UPDATE sticky_messages s SET lease_until=now()+INTERVAL '45 seconds' FROM due WHERE s.channel_id=due.channel_id RETURNING {}", STICKY.split(", ").map(|s| format!("s.{s}")).collect::<Vec<_>>().join(", "))))
            .bind(limit).fetch_all(&self.pool).await?)
    }

    /// Persist intent before HTTP. A crash leaves a paused item, never an automatic duplicate.
    pub async fn begin_sticky(&self, row: &UtilityStickyRow) -> anyhow::Result<bool> {
        Ok(sqlx::query("UPDATE sticky_messages SET enabled=false,pause_reason='delivery_uncertain',message_count=0 WHERE channel_id=$1 AND enabled AND lease_until=$2 AND lease_until>now()")
            .bind(row.channel_id).bind(row.lease_until).execute(&self.pool).await?.rows_affected()>0)
    }

    pub async fn finish_sticky(&self, row: &UtilityStickyRow, copy: u64) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        let saved = sqlx::query("UPDATE sticky_messages SET last_message_id=$2,last_post_at=now(),lease_until=NULL,enabled=true,pause_reason=NULL WHERE channel_id=$1 AND lease_until=$3 AND pause_reason='delivery_uncertain'")
            .bind(row.channel_id).bind(sql_id(copy)).bind(row.lease_until).execute(&mut *tx).await?.rows_affected()>0;
        // If a manager removed/replaced the row while HTTP was in flight, discard our copy.
        let cleanup = if saved { row.last_message_id } else { Some(sql_id(copy)) };
        if let Some(old) = cleanup {
            sqlx::query("INSERT INTO sticky_copy_cleanup(channel_id,message_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
                .bind(row.channel_id)
                .bind(old)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn release_sticky(&self, row: &UtilityStickyRow) -> anyhow::Result<()> {
        sqlx::query("UPDATE sticky_messages SET lease_until=NULL WHERE channel_id=$1 AND lease_until=$2")
            .bind(row.channel_id)
            .bind(row.lease_until)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn pause_sticky(&self, row: &UtilityStickyRow, reason: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE sticky_messages SET enabled=false,pause_reason=$3,lease_until=NULL WHERE channel_id=$1 AND lease_until=$2")
            .bind(row.channel_id).bind(row.lease_until).bind(reason).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn schedules(&self, guild: u64) -> anyhow::Result<Vec<ScheduledRow>> {
        Ok(sqlx::query_as::<_, ScheduledRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {SCHEDULED} FROM scheduled_messages WHERE guild_id=$1 ORDER BY id"
        )))
        .bind(sql_id(guild))
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn schedule(&self, guild: u64, id: i64) -> anyhow::Result<Option<ScheduledRow>> {
        Ok(sqlx::query_as::<_, ScheduledRow>(sqlx::AssertSqlSafe(format!(
            "SELECT {SCHEDULED} FROM scheduled_messages WHERE guild_id=$1 AND id=$2"
        )))
        .bind(sql_id(guild))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn add_schedule(&self, row: &ScheduledRow) -> anyhow::Result<Option<i64>> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(row.guild_id)
            .execute(&mut *tx)
            .await?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM scheduled_messages WHERE guild_id=$1")
            .bind(row.guild_id)
            .fetch_one(&mut *tx)
            .await?;
        if count >= 20 {
            return Ok(None);
        }
        let id: i64 = sqlx::query_scalar("INSERT INTO scheduled_messages (guild_id,channel_id,content,embed,schedule,tz,next_run_at,created_by,ping_allowed) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id")
            .bind(row.guild_id).bind(row.channel_id).bind(&row.content).bind(&row.embed).bind(&row.schedule).bind(&row.tz).bind(row.next_run_at).bind(row.created_by).bind(row.ping_allowed).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(id))
    }

    pub async fn update_schedule(&self, row: &ScheduledRow) -> anyhow::Result<bool> {
        Ok(sqlx::query("UPDATE scheduled_messages SET channel_id=$3,content=$4,embed=$5,schedule=$6,tz=$7,next_run_at=$8,created_by=$9,ping_allowed=$10,enabled=true,lease_until=NULL,pause_reason=NULL WHERE guild_id=$1 AND id=$2 AND (lease_until IS NULL OR lease_until<now())")
            .bind(row.guild_id).bind(row.id).bind(row.channel_id).bind(&row.content).bind(&row.embed).bind(&row.schedule).bind(&row.tz).bind(row.next_run_at).bind(row.created_by).bind(row.ping_allowed).execute(&self.pool).await?.rows_affected()>0)
    }

    pub async fn remove_schedule(&self, guild: u64, id: i64) -> anyhow::Result<bool> {
        Ok(sqlx::query(
            "DELETE FROM scheduled_messages WHERE guild_id=$1 AND id=$2 AND (lease_until IS NULL OR lease_until<now())",
        )
        .bind(sql_id(guild))
        .bind(id)
        .execute(&self.pool)
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn enable_schedule(&self, guild: u64, id: i64, enabled: bool) -> anyhow::Result<bool> {
        Ok(
            sqlx::query("UPDATE scheduled_messages SET enabled=$3,lease_until=NULL,pause_reason=NULL WHERE guild_id=$1 AND id=$2 AND (lease_until IS NULL OR lease_until<now())")
                .bind(sql_id(guild))
                .bind(id)
                .bind(enabled)
                .execute(&self.pool)
                .await?
                .rows_affected()
                > 0,
        )
    }

    pub async fn claim_schedules(&self, limit: i64) -> anyhow::Result<Vec<ScheduledRow>> {
        Ok(sqlx::query_as::<_, ScheduledRow>(sqlx::AssertSqlSafe(format!("WITH due AS (SELECT id FROM scheduled_messages WHERE enabled AND next_run_at<=now() AND (lease_until IS NULL OR lease_until<now()) ORDER BY next_run_at LIMIT $1 FOR UPDATE SKIP LOCKED) UPDATE scheduled_messages s SET lease_until=now()+INTERVAL '90 seconds' FROM due WHERE s.id=due.id RETURNING {}", SCHEDULED.split(", ").map(|s| format!("s.{s}")).collect::<Vec<_>>().join(", "))))
            .bind(limit).fetch_all(&self.pool).await?)
    }

    pub async fn claim_schedule_now(&self, guild: u64, id: i64) -> anyhow::Result<Option<ScheduledRow>> {
        Ok(sqlx::query_as::<_, ScheduledRow>(sqlx::AssertSqlSafe(format!(
            "UPDATE scheduled_messages SET enabled=true,lease_until=now()+INTERVAL '90 seconds' WHERE guild_id=$1 AND id=$2 AND (lease_until IS NULL OR lease_until<now()) RETURNING {SCHEDULED}"
        ))).bind(sql_id(guild)).bind(id).fetch_optional(&self.pool).await?)
    }

    pub async fn begin_schedule(&self, row: &ScheduledRow) -> anyhow::Result<bool> {
        Ok(sqlx::query("UPDATE scheduled_messages SET enabled=false,pause_reason='delivery_uncertain' WHERE id=$1 AND enabled AND lease_until=$2 AND lease_until>now()")
            .bind(row.id).bind(row.lease_until).execute(&self.pool).await?.rows_affected()>0)
    }

    pub async fn finish_schedule(&self, row: &ScheduledRow, next: Option<DateTime<Utc>>) -> anyhow::Result<()> {
        sqlx::query("UPDATE scheduled_messages SET runs=runs+1,last_run_at=now(),next_run_at=COALESCE($2,next_run_at),enabled=$2 IS NOT NULL,lease_until=NULL,pause_reason=NULL WHERE id=$1 AND lease_until=$3 AND pause_reason='delivery_uncertain'")
            .bind(row.id).bind(next).bind(row.lease_until).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn release_schedule(&self, row: &ScheduledRow) -> anyhow::Result<()> {
        sqlx::query("UPDATE scheduled_messages SET lease_until=NULL WHERE id=$1 AND lease_until=$2")
            .bind(row.id)
            .bind(row.lease_until)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn pause_schedule(&self, row: &ScheduledRow, reason: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE scheduled_messages SET enabled=false,pause_reason=$3,lease_until=NULL WHERE id=$1 AND lease_until=$2")
            .bind(row.id).bind(row.lease_until).bind(reason).execute(&self.pool).await?;
        Ok(())
    }
}
