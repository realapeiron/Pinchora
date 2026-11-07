//! F10 persistence. Player advisory locks precede pending/join rows; Redis leases serialize REST transitions.
use crate::{Db, locks::lock_users, sql_id};
use chrono::{DateTime, Utc};
use pinchora_core::{
    anti_raid::RaidSettings,
    verification::{Config, Verification},
};
use sqlx::types::Json;
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VerificationPending {
    pub guild_id: i64,
    pub user_id: i64,
    pub joined_at: DateTime<Utc>,
    pub attempts: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub verified_at: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RaidEvent {
    pub id: i64,
    pub guild_id: i64,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub joins: i32,
    pub action: String,
    pub ended_by: Option<i64>,
    pub expires_at: DateTime<Utc>,
    pub snapshot: serde_json::Value,
    pub error: Option<String>,
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RaidJoin {
    pub guild_id: i64,
    pub user_id: i64,
    pub joined_at: DateTime<Utc>,
    pub name_family: String,
    pub avatar: Option<String>,
    pub young: bool,
    pub status: String,
}
impl Db {
    pub async fn safety_config(&self, guild: u64) -> anyhow::Result<Config> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let v:Option<Json<Verification>>=sqlx::query_scalar("SELECT (to_jsonb(v)-'guild_id') || jsonb_build_object('exempt_users', ARRAY(SELECT x::text FROM unnest(v.exempt_users) x)) FROM verification v WHERE guild_id=$1").bind(sql_id(guild)).fetch_optional(&mut *tx).await?;
        let r: Option<Json<RaidSettings>> =
            sqlx::query_scalar("SELECT to_jsonb(r)-'guild_id' FROM raid_settings r WHERE guild_id=$1")
                .bind(sql_id(guild))
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(Config {
            verification: v.map(|v| v.0).unwrap_or_default(),
            raid: r.map(|r| r.0).unwrap_or_default(),
        })
    }
    pub async fn save_safety(&self, guild: u64, c: &Config) -> anyhow::Result<()> {
        pinchora_core::verification::validate(c, pinchora_core::i18n::Lang::En).map_err(anyhow::Error::msg)?;
        let mut tx = self.pool.begin().await?;
        let users: Vec<i64> = c
            .verification
            .exempt_users
            .iter()
            .map(|id| id.parse())
            .collect::<Result<_, _>>()?;
        lock_users(&mut tx, &users).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('safety-config:' || $1::text,0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO verification (guild_id,enabled,mode,verified_role_id,unverified_role_id,channel_id,message_id,kick_after_mins,min_account_age_hours,block_default_avatar,block_name_patterns,dm_on_block,log_channel_id,message,rule_action,exempt_roles,exempt_users) SELECT $1,x.enabled,x.mode,x.verified_role_id,x.unverified_role_id,x.channel_id,x.message_id,x.kick_after_mins,x.min_account_age_hours,x.block_default_avatar,x.block_name_patterns,x.dm_on_block,x.log_channel_id,x.message,x.rule_action,x.exempt_roles,x.exempt_users FROM jsonb_populate_record(NULL::verification,$2) x ON CONFLICT(guild_id) DO UPDATE SET enabled=EXCLUDED.enabled,mode=EXCLUDED.mode,verified_role_id=EXCLUDED.verified_role_id,unverified_role_id=EXCLUDED.unverified_role_id,channel_id=EXCLUDED.channel_id,kick_after_mins=EXCLUDED.kick_after_mins,min_account_age_hours=EXCLUDED.min_account_age_hours,block_default_avatar=EXCLUDED.block_default_avatar,block_name_patterns=EXCLUDED.block_name_patterns,dm_on_block=EXCLUDED.dm_on_block,log_channel_id=EXCLUDED.log_channel_id,message=EXCLUDED.message,rule_action=EXCLUDED.rule_action,exempt_roles=EXCLUDED.exempt_roles,exempt_users=EXCLUDED.exempt_users").bind(sql_id(guild)).bind(Json(&c.verification)).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO raid_settings (guild_id,enabled,join_threshold,window_secs,action,lockdown_minutes,alert_channel_id,similar_threshold,similar_names,similar_avatars,new_account_hours,channels,slowmode_secs,lock_channels,pause_invites) SELECT $1,x.enabled,x.join_threshold,x.window_secs,x.action,x.lockdown_minutes,x.alert_channel_id,x.similar_threshold,x.similar_names,x.similar_avatars,x.new_account_hours,x.channels,x.slowmode_secs,x.lock_channels,x.pause_invites FROM jsonb_populate_record(NULL::raid_settings,$2) x ON CONFLICT(guild_id) DO UPDATE SET enabled=EXCLUDED.enabled,join_threshold=EXCLUDED.join_threshold,window_secs=EXCLUDED.window_secs,action=EXCLUDED.action,lockdown_minutes=EXCLUDED.lockdown_minutes,alert_channel_id=EXCLUDED.alert_channel_id,similar_threshold=EXCLUDED.similar_threshold,similar_names=EXCLUDED.similar_names,similar_avatars=EXCLUDED.similar_avatars,new_account_hours=EXCLUDED.new_account_hours,channels=EXCLUDED.channels,slowmode_secs=EXCLUDED.slowmode_secs,lock_channels=EXCLUDED.lock_channels,pause_invites=EXCLUDED.pause_invites").bind(sql_id(guild)).bind(Json(&c.raid)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn verification_posted(&self, guild: u64, message: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE verification SET message_id=$2 WHERE guild_id=$1")
            .bind(sql_id(guild))
            .bind(message.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn pending_verification(&self, guild: u64, user: u64) -> anyhow::Result<Option<VerificationPending>> {
        Ok(
            sqlx::query_as("SELECT * FROM verification_pending WHERE guild_id=$1 AND user_id=$2")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .fetch_optional(&self.pool)
                .await?,
        )
    }
    pub async fn hold_verification(
        &self,
        guild: u64,
        user: u64,
        joined: DateTime<Utc>,
        expires: Option<DateTime<Utc>>,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        sqlx::query("INSERT INTO verification_pending(guild_id,user_id,joined_at,expires_at) VALUES($1,$2,$3,$4) ON CONFLICT(guild_id,user_id) DO UPDATE SET joined_at=$3,expires_at=$4,verified_at=NULL,attempts=0 WHERE verification_pending.joined_at<$3")
        .bind(sql_id(guild)).bind(sql_id(user)).bind(joined).bind(expires).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn finish_verification(&self, guild: u64, user: u64, now: DateTime<Utc>) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let n=sqlx::query("UPDATE verification_pending SET verified_at=$3,expires_at=NULL WHERE guild_id=$1 AND user_id=$2 AND verified_at IS NULL").bind(sql_id(guild)).bind(sql_id(user)).bind(now).execute(&mut *tx).await?.rows_affected();
        sqlx::query("UPDATE raid_joins SET status='verified' WHERE guild_id=$1 AND user_id=$2")
            .bind(sql_id(guild))
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(n > 0)
    }
    pub async fn forget_verification(&self, guild: u64, user: u64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM verification_pending WHERE guild_id=$1 AND user_id=$2")
            .bind(sql_id(guild))
            .bind(sql_id(user))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn verification_attempt(&self, guild: u64, user: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE verification_pending SET attempts=attempts+1 WHERE guild_id=$1 AND user_id=$2 AND verified_at IS NULL").bind(sql_id(guild)).bind(sql_id(user)).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn due_verifications(&self, now: DateTime<Utc>) -> anyhow::Result<Vec<VerificationPending>> {
        Ok(sqlx::query_as("SELECT * FROM verification_pending WHERE verified_at IS NULL AND expires_at<=$1 ORDER BY expires_at LIMIT 100").bind(now).fetch_all(&self.pool).await?)
    }
    pub async fn note_raid_join(&self, j: &RaidJoin) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;
        lock_users(&mut tx, &[j.user_id]).await?;
        let n=sqlx::query("INSERT INTO raid_joins(guild_id,user_id,joined_at,name_family,avatar,young,status) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(guild_id,user_id) DO UPDATE SET joined_at=$3,name_family=$4,avatar=$5,young=$6,status=$7 WHERE raid_joins.joined_at<$3")
        .bind(j.guild_id).bind(j.user_id).bind(j.joined_at).bind(&j.name_family).bind(&j.avatar).bind(j.young).bind(&j.status).execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;
        Ok(n > 0)
    }
    pub async fn raid_joins_since(&self, guild: u64, since: DateTime<Utc>) -> anyhow::Result<Vec<RaidJoin>> {
        Ok(sqlx::query_as(
            "SELECT * FROM raid_joins WHERE guild_id=$1 AND joined_at>=$2 ORDER BY joined_at DESC LIMIT 1000",
        )
        .bind(sql_id(guild))
        .bind(since)
        .fetch_all(&self.pool)
        .await?)
    }
    pub async fn raid_join_status(&self, guild: u64, user: u64, status: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE raid_joins SET status=$3 WHERE guild_id=$1 AND user_id=$2")
            .bind(sql_id(guild))
            .bind(sql_id(user))
            .bind(status)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn active_raid(&self, guild: u64) -> anyhow::Result<Option<RaidEvent>> {
        Ok(
            sqlx::query_as("SELECT * FROM raid_events WHERE guild_id=$1 AND ended_at IS NULL")
                .bind(sql_id(guild))
                .fetch_optional(&self.pool)
                .await?,
        )
    }
    pub async fn raid_event(&self, guild: u64, id: i64) -> anyhow::Result<Option<RaidEvent>> {
        Ok(sqlx::query_as("SELECT * FROM raid_events WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }
    pub async fn raid_events(&self, guild: u64) -> anyhow::Result<Vec<RaidEvent>> {
        Ok(
            sqlx::query_as("SELECT * FROM raid_events WHERE guild_id=$1 ORDER BY id DESC LIMIT 50")
                .bind(sql_id(guild))
                .fetch_all(&self.pool)
                .await?,
        )
    }
    pub async fn open_raid(
        &self,
        guild: u64,
        joins: i32,
        action: &str,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
        snapshot: &serde_json::Value,
    ) -> anyhow::Result<Option<RaidEvent>> {
        Ok(sqlx::query_as("INSERT INTO raid_events(guild_id,joins,action,started_at,expires_at,snapshot) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING RETURNING *")
        .bind(sql_id(guild)).bind(joins).bind(action).bind(now).bind(until).bind(snapshot).fetch_optional(&self.pool).await?)
    }
    pub async fn raid_error(&self, id: i64, error: Option<&str>) -> anyhow::Result<()> {
        sqlx::query("UPDATE raid_events SET error=$2 WHERE id=$1")
            .bind(id)
            .bind(error)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn end_raid(&self, id: i64, actor: u64, now: DateTime<Utc>) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        lock_users(&mut tx, &[sql_id(actor)]).await?;
        sqlx::query("UPDATE raid_events SET ended_at=$3,ended_by=$2,error=NULL WHERE id=$1 AND ended_at IS NULL")
            .bind(id)
            .bind(sql_id(actor))
            .bind(now)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn due_raids(&self, now: DateTime<Utc>) -> anyhow::Result<Vec<RaidEvent>> {
        Ok(sqlx::query_as(
            "SELECT * FROM raid_events WHERE ended_at IS NULL AND expires_at<=$1 ORDER BY expires_at LIMIT 100",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await?)
    }
    pub async fn sweep_safety(&self, cutoff: DateTime<Utc>) -> anyhow::Result<()> {
        for query in [
            "DELETE FROM raid_joins WHERE (guild_id,user_id) IN (SELECT guild_id,user_id FROM raid_joins WHERE joined_at<$1 FOR UPDATE SKIP LOCKED LIMIT 1000)",
            "DELETE FROM verification_pending WHERE (guild_id,user_id) IN (SELECT guild_id,user_id FROM verification_pending WHERE verified_at<$1 FOR UPDATE SKIP LOCKED LIMIT 1000)",
            "DELETE FROM raid_events WHERE id IN (SELECT id FROM raid_events WHERE ended_at<$1 FOR UPDATE SKIP LOCKED LIMIT 1000)",
        ] {
            sqlx::query(query).bind(cutoff).execute(&self.pool).await?;
        }
        Ok(())
    }
}
