//! Roblox identity links and per-guild bind configuration.
use crate::{Db, sql_id};
use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct RobloxLink {
    pub user_id: i64,
    pub roblox_id: i64,
    pub roblox_name: String,
    pub verified_at: DateTime<Utc>,
    pub method: String,
}
#[derive(Debug, Clone, FromRow)]
pub struct RobloxPending {
    pub user_id: i64,
    pub roblox_id: i64,
    pub roblox_name: String,
    pub code: String,
    pub expires_at: DateTime<Utc>,
}
#[derive(Debug, Clone, FromRow, Default)]
pub struct RobloxSettings {
    pub enabled: bool,
    pub verified_role_id: Option<i64>,
    pub unverified_role_id: Option<i64>,
    pub nickname_template: Option<String>,
    pub verify_channel_id: Option<i64>,
    pub require_verified: bool,
}
#[derive(Debug, Clone, FromRow)]
pub struct RobloxBind {
    pub id: i64,
    pub guild_id: i64,
    pub kind: String,
    pub group_id: Option<i64>,
    pub rank_min: Option<i16>,
    pub rank_max: Option<i16>,
    pub role_id: i64,
    pub remove_if_unmet: bool,
}

impl Db {
    pub async fn roblox_link(&self, user: u64) -> anyhow::Result<Option<RobloxLink>> {
        Ok(sqlx::query_as("SELECT * FROM roblox_links WHERE user_id=$1")
            .bind(sql_id(user))
            .fetch_optional(self.pool())
            .await?)
    }
    pub async fn roblox_refresh_name(&self, user: u64, roblox: u64, name: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE roblox_links SET roblox_name=$3 WHERE user_id=$1 AND roblox_id=$2")
            .bind(sql_id(user))
            .bind(sql_id(roblox))
            .bind(name)
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn roblox_link_by_id(&self, roblox: u64) -> anyhow::Result<Option<RobloxLink>> {
        Ok(sqlx::query_as("SELECT * FROM roblox_links WHERE roblox_id=$1")
            .bind(sql_id(roblox))
            .fetch_optional(self.pool())
            .await?)
    }
    pub async fn roblox_pending(&self, user: u64) -> anyhow::Result<Option<RobloxPending>> {
        Ok(
            sqlx::query_as("SELECT * FROM roblox_pending WHERE user_id=$1 AND expires_at>now()")
                .bind(sql_id(user))
                .fetch_optional(self.pool())
                .await?,
        )
    }
    pub async fn roblox_start(&self, user: u64, roblox: u64, name: &str, code: &str) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        crate::infra::locks::lock_users(&mut tx, &[sql_id(user)]).await?;
        sqlx::query("INSERT INTO roblox_pending(user_id,roblox_id,roblox_name,code,expires_at) VALUES($1,$2,$3,$4,now()+interval '10 minutes') ON CONFLICT(user_id) DO UPDATE SET roblox_id=$2,roblox_name=$3,code=$4,last_check_at=NULL,expires_at=now()+interval '10 minutes'")
            .bind(sql_id(user)).bind(sql_id(roblox)).bind(name).bind(code).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn roblox_check_attempt(&self, user: u64, code: &str) -> anyhow::Result<bool> {
        Ok(sqlx::query("UPDATE roblox_pending SET last_check_at=now() WHERE user_id=$1 AND code=$2 AND expires_at>now() AND (last_check_at IS NULL OR last_check_at<now()-interval '15 seconds')")
            .bind(sql_id(user)).bind(code).execute(self.pool()).await?.rows_affected()>0)
    }
    pub async fn roblox_complete(&self, user: u64, pending: &RobloxPending) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        crate::infra::locks::lock_users(&mut tx, &[sql_id(user)]).await?;
        if pending.user_id != sql_id(user) {
            return Ok(false);
        }
        let valid: Option<i64> = sqlx::query_scalar("SELECT roblox_id FROM roblox_pending WHERE user_id=$1 AND roblox_id=$2 AND code=$3 AND expires_at>now() FOR UPDATE")
            .bind(sql_id(user)).bind(pending.roblox_id).bind(&pending.code).fetch_optional(&mut *tx).await?;
        if valid.is_none() {
            tx.rollback().await?;
            return Ok(false);
        }
        let inserted = sqlx::query(
            "INSERT INTO roblox_links(user_id,roblox_id,roblox_name) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
        )
        .bind(sql_id(user))
        .bind(pending.roblox_id)
        .bind(&pending.roblox_name)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if inserted {
            sqlx::query("DELETE FROM roblox_pending WHERE user_id=$1")
                .bind(sql_id(user))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(inserted)
    }
    pub async fn roblox_unlink(&self, user: u64) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        crate::infra::locks::lock_users(&mut tx, &[sql_id(user)]).await?;
        sqlx::query("DELETE FROM roblox_pending WHERE user_id=$1")
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM roblox_cache WHERE key IN (SELECT 'groups:' || roblox_id::text FROM roblox_links WHERE user_id=$1)")
            .bind(sql_id(user)).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM roblox_links WHERE user_id=$1")
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?;
        // Retain already queued guilds even when the members table has no row.
        sqlx::query("UPDATE roblox_sync SET next_update_at=now() WHERE user_id=$1")
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn roblox_settings(&self, guild: u64) -> anyhow::Result<RobloxSettings> {
        Ok(sqlx::query_as("SELECT enabled,verified_role_id,unverified_role_id,nickname_template,verify_channel_id,require_verified FROM roblox_settings WHERE guild_id=$1")
            .bind(sql_id(guild)).fetch_optional(self.pool()).await?.unwrap_or_default())
    }
    pub async fn roblox_save_settings(&self, guild: u64, s: &RobloxSettings) -> anyhow::Result<()> {
        self.guild_settings(guild).await?;
        sqlx::query("INSERT INTO roblox_settings(guild_id,enabled,verified_role_id,unverified_role_id,nickname_template,verify_channel_id,require_verified) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(guild_id) DO UPDATE SET enabled=$2,verified_role_id=$3,unverified_role_id=$4,nickname_template=$5,verify_channel_id=$6,require_verified=$7")
            .bind(sql_id(guild)).bind(s.enabled).bind(s.verified_role_id).bind(s.unverified_role_id).bind(&s.nickname_template).bind(s.verify_channel_id).bind(s.require_verified).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn roblox_binds(&self, guild: u64) -> anyhow::Result<Vec<RobloxBind>> {
        Ok(
            sqlx::query_as("SELECT * FROM roblox_binds WHERE guild_id=$1 ORDER BY id")
                .bind(sql_id(guild))
                .fetch_all(self.pool())
                .await?,
        )
    }
    #[allow(clippy::too_many_arguments)] // A bind is one atomic row; callers validate every field.
    pub async fn roblox_add_bind(
        &self,
        guild: u64,
        kind: &str,
        group: Option<u64>,
        min: Option<i16>,
        max: Option<i16>,
        role: u64,
        remove: bool,
    ) -> anyhow::Result<i64> {
        let mut tx = self.pool().begin().await?;
        self.guild_settings(guild).await?;
        sqlx::query("SELECT id FROM guilds WHERE id=$1 FOR UPDATE")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM roblox_binds WHERE guild_id=$1")
            .bind(sql_id(guild))
            .fetch_one(&mut *tx)
            .await?;
        anyhow::ensure!(count < 25, "Roblox bind limit reached");
        let id:i64=sqlx::query_scalar("INSERT INTO roblox_binds(guild_id,kind,group_id,rank_min,rank_max,role_id,remove_if_unmet) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
            .bind(sql_id(guild)).bind(kind).bind(group.map(sql_id)).bind(min).bind(max).bind(sql_id(role)).bind(remove).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(id)
    }
    pub async fn roblox_remove_bind(&self, guild: u64, id: i64) -> anyhow::Result<bool> {
        Ok(sqlx::query("DELETE FROM roblox_binds WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .execute(self.pool())
            .await?
            .rows_affected()
            > 0)
    }
    pub async fn roblox_forget_member(&self, guild: u64, user: u64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM roblox_sync WHERE guild_id=$1 AND user_id=$2")
            .bind(sql_id(guild))
            .bind(sql_id(user))
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn roblox_queue(&self, guild: u64, user: u64) -> anyhow::Result<()> {
        self.guild_settings(guild).await?;
        sqlx::query("INSERT INTO roblox_sync(guild_id,user_id,next_update_at) VALUES($1,$2,now()) ON CONFLICT(guild_id,user_id) DO UPDATE SET next_update_at=now()")
            .bind(sql_id(guild)).bind(sql_id(user)).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn roblox_hold(&self, guild: u64, user: u64) -> anyhow::Result<()> {
        self.guild_settings(guild).await?;
        sqlx::query("INSERT INTO roblox_sync(guild_id,user_id,held) VALUES($1,$2,true) ON CONFLICT(guild_id,user_id) DO UPDATE SET held=true,next_update_at=now()")
            .bind(sql_id(guild)).bind(sql_id(user)).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn roblox_claim_hold(&self, guild: u64, user: u64) -> anyhow::Result<bool> {
        Ok(
            sqlx::query("UPDATE roblox_sync SET held=false WHERE guild_id=$1 AND user_id=$2 AND held=true")
                .bind(sql_id(guild))
                .bind(sql_id(user))
                .execute(self.pool())
                .await?
                .rows_affected()
                > 0,
        )
    }
    pub async fn roblox_user_guilds(&self, user: u64) -> anyhow::Result<Vec<i64>> {
        Ok(sqlx::query_scalar("SELECT guild_id FROM roblox_sync WHERE user_id=$1 UNION SELECT s.guild_id FROM roblox_settings s JOIN members m ON m.guild_id=s.guild_id WHERE m.user_id=$1")
            .bind(sql_id(user)).fetch_all(self.pool()).await?)
    }
    pub async fn roblox_queue_linked_guilds(&self, user: u64) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO roblox_sync(guild_id,user_id,next_update_at) SELECT s.guild_id,$1,now() FROM roblox_settings s JOIN members m ON m.guild_id=s.guild_id AND m.user_id=$1 WHERE s.enabled ON CONFLICT(guild_id,user_id) DO UPDATE SET next_update_at=now()")
            .bind(sql_id(user)).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn roblox_due(&self) -> anyhow::Result<Vec<(i64, i64)>> {
        Ok(sqlx::query_as("UPDATE roblox_sync s SET next_update_at=now()+interval '10 minutes' FROM (SELECT guild_id,user_id FROM roblox_sync WHERE next_update_at<=now() ORDER BY next_update_at LIMIT 20 FOR UPDATE SKIP LOCKED) due WHERE s.guild_id=due.guild_id AND s.user_id=due.user_id RETURNING s.guild_id,s.user_id")
            .fetch_all(self.pool()).await?)
    }
    pub async fn roblox_reschedule(&self, guild: u64, user: u64, minutes: i32) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE roblox_sync SET next_update_at=now()+($3 * interval '1 minute') WHERE guild_id=$1 AND user_id=$2",
        )
        .bind(sql_id(guild))
        .bind(sql_id(user))
        .bind(minutes)
        .execute(self.pool())
        .await?;
        Ok(())
    }
    pub async fn roblox_cache(&self, key: &str) -> anyhow::Result<Option<serde_json::Value>> {
        Ok(sqlx::query_scalar(
            "SELECT payload FROM roblox_cache WHERE key=$1 AND fetched_at>now()-interval '10 minutes'",
        )
        .bind(key)
        .fetch_optional(self.pool())
        .await?)
    }
    pub async fn roblox_cache_put(&self, key: &str, payload: &serde_json::Value) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO roblox_cache(key,payload) VALUES($1,$2) ON CONFLICT(key) DO UPDATE SET payload=$2,fetched_at=now()")
            .bind(key).bind(payload).execute(self.pool()).await?;
        Ok(())
    }
}
