//! F1 persistence. Configuration changes serialize per guild; player locks precede hit/strike rows.
use crate::{Db, locks::lock_users, sql_id};
use chrono::{DateTime, Utc};
use pinchora_core::{
    automod::{Config, Rule},
    i18n::Lang,
};
use sqlx::{Row, types::Json};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AutoModHit {
    pub id: i64,
    pub guild_id: i64,
    pub rule_id: Option<i64>,
    pub user_id: i64,
    pub channel_id: i64,
    pub message_id: Option<i64>,
    pub action: String,
    pub snippet: Option<String>,
    pub created_at: DateTime<Utc>,
    pub reviewed: bool,
}
#[derive(Debug, sqlx::FromRow)]
pub struct AutoModSync {
    pub revision: i64,
    pub generation: i64,
    pub sync_error: Option<String>,
    pub synced_at: Option<DateTime<Utc>>,
}

impl Db {
    pub async fn automod_config(&self, guild: u64) -> anyhow::Result<Config> {
        let mut tx = self.pool.begin().await?;
        // One consistent snapshot of settings and cards even while another manager saves.
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let settings = sqlx::query("SELECT profile, escalation, dm_on_action, alert_channel_id, observe_until FROM automod_settings WHERE guild_id=$1")
            .bind(sql_id(guild)).fetch_optional(&mut *tx).await?;
        let Some(s) = settings else {
            return Ok(Config::default());
        };
        let rows = sqlx::query("SELECT id, name, kind, enabled, log_only, native, discord_rule_id, params, actions, exempt_roles, exempt_channels, only_channels FROM automod_rules WHERE guild_id=$1 ORDER BY id")
            .bind(sql_id(guild)).fetch_all(&mut *tx).await?;
        let ids = |r: &sqlx::postgres::PgRow, key| -> anyhow::Result<Vec<String>> {
            Ok(r.try_get::<Vec<i64>, _>(key)?.iter().map(i64::to_string).collect())
        };
        let rules = rows
            .iter()
            .map(|r| {
                anyhow::Ok(Rule {
                    id: Some(r.try_get::<i64, _>("id")?.to_string()),
                    name: r.try_get("name")?,
                    kind: r.try_get("kind")?,
                    enabled: r.try_get("enabled")?,
                    log_only: r.try_get("log_only")?,
                    native: r.try_get("native")?,
                    discord_rule_id: r.try_get::<Option<i64>, _>("discord_rule_id")?.map(|id| id.to_string()),
                    params: r.try_get::<Json<_>, _>("params")?.0,
                    actions: r.try_get::<Json<_>, _>("actions")?.0,
                    exempt_roles: ids(r, "exempt_roles")?,
                    exempt_channels: ids(r, "exempt_channels")?,
                    only_channels: ids(r, "only_channels")?,
                })
            })
            .collect::<anyhow::Result<_>>()?;
        tx.commit().await?;
        Ok(Config {
            profile: s.try_get("profile")?,
            escalation: s.try_get::<Json<_>, _>("escalation")?.0,
            dm_on_action: s.try_get("dm_on_action")?,
            alert_channel_id: s.try_get::<Option<i64>, _>("alert_channel_id")?.map(|i| i.to_string()),
            observe_until: s.try_get("observe_until")?,
            rules,
        })
    }

    pub async fn save_automod(&self, guild: u64, actor: u64, config: &Config) -> anyhow::Result<()> {
        pinchora_core::automod::validate(config, Lang::En).map_err(anyhow::Error::msg)?;
        let mut tx = self.pool.begin().await?;
        lock_users(&mut tx, &[sql_id(actor)]).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('automod:' || $1::text, 0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO automod_settings (guild_id,profile,escalation,dm_on_action,alert_channel_id,observe_until,revision,sync_generation,sync_after) VALUES ($1,$2,$3,$4,$5,$6,1,1,now()+interval '5 seconds') ON CONFLICT (guild_id) DO UPDATE SET profile=$2, escalation=$3, dm_on_action=$4, alert_channel_id=$5, observe_until=$6, revision=automod_settings.revision+1, sync_generation=automod_settings.sync_generation+1, sync_after=now()+interval '5 seconds', sync_error=NULL")
            .bind(sql_id(guild)).bind(&config.profile).bind(Json(&config.escalation)).bind(config.dm_on_action)
            .bind(config.alert_channel_id.as_deref().map(str::parse::<i64>).transpose()?).bind(config.observe_until).execute(&mut *tx).await?;
        let parse_ids = |ids: &[String]| ids.iter().map(|s| s.parse::<i64>()).collect::<Result<Vec<_>, _>>();
        let mut kept = Vec::new();
        for r in &config.rules {
            // Server-owned native ids and creator ids cannot be supplied by a client. Names are the
            // profile's stable keys; profile reapplication preserves native ids and existing hits.
            let id: i64 = sqlx::query_scalar("INSERT INTO automod_rules (guild_id,name,kind,enabled,log_only,native,params,actions,exempt_roles,exempt_channels,only_channels,created_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT (guild_id,name) DO UPDATE SET kind=$3,enabled=$4,log_only=$5,native=$6,params=$7,actions=$8,exempt_roles=$9,exempt_channels=$10,only_channels=$11,updated_at=now() RETURNING id")
                .bind(sql_id(guild)).bind(&r.name).bind(&r.kind).bind(r.enabled).bind(r.log_only).bind(r.native).bind(Json(&r.params)).bind(Json(&r.actions))
                .bind(parse_ids(&r.exempt_roles)?).bind(parse_ids(&r.exempt_channels)?).bind(parse_ids(&r.only_channels)?).bind(sql_id(actor)).fetch_one(&mut *tx).await?;
            kept.push(id);
        }
        // Hits retain their historical rule id; reconciliation removes orphaned native mirrors.
        sqlx::query("DELETE FROM automod_rules WHERE guild_id=$1 AND NOT(id=ANY($2))")
            .bind(sql_id(guild))
            .bind(&kept)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn automod_hits(&self, guild: u64) -> anyhow::Result<Vec<AutoModHit>> {
        Ok(
            sqlx::query_as("SELECT * FROM automod_hits WHERE guild_id=$1 ORDER BY created_at DESC,id DESC LIMIT 50")
                .bind(sql_id(guild))
                .fetch_all(&self.pool)
                .await?,
        )
    }
    pub async fn automod_hit(&self, guild: u64, id: i64) -> anyhow::Result<Option<AutoModHit>> {
        Ok(sqlx::query_as("SELECT * FROM automod_hits WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }
    pub async fn open_automod_hit(&self, hit: &AutoModHit) -> anyhow::Result<Option<AutoModHit>> {
        let mut tx = self.pool.begin().await?;
        lock_users(&mut tx, &[hit.user_id]).await?;
        let row = sqlx::query_as("INSERT INTO automod_hits (guild_id,rule_id,user_id,channel_id,message_id,action,snippet,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING RETURNING *")
            .bind(hit.guild_id).bind(hit.rule_id).bind(hit.user_id).bind(hit.channel_id).bind(hit.message_id).bind(&hit.action).bind(&hit.snippet).bind(hit.created_at).fetch_optional(&mut *tx).await?;
        tx.commit().await?;
        Ok(row)
    }
    pub async fn finish_automod_hit(&self, guild: u64, id: i64, action: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE automod_hits SET action=$3 WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .bind(action)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn review_automod_hit(&self, guild: u64, id: i64) -> anyhow::Result<bool> {
        Ok(
            sqlx::query("UPDATE automod_hits SET reviewed=TRUE WHERE guild_id=$1 AND id=$2 AND NOT reviewed")
                .bind(sql_id(guild))
                .bind(id)
                .execute(&self.pool)
                .await?
                .rows_affected()
                > 0,
        )
    }
    /// A moderator action has one durable claim, including across process restarts.
    pub async fn claim_automod_action(&self, guild: u64, hit: i64, kind: &str) -> anyhow::Result<bool> {
        let changed = sqlx::query("INSERT INTO automod_hit_actions(hit_id,kind) SELECT id,$3 FROM automod_hits WHERE guild_id=$1 AND id=$2 ON CONFLICT DO NOTHING")
            .bind(sql_id(guild)).bind(hit).bind(kind).execute(&self.pool).await?;
        Ok(changed.rows_affected() == 1)
    }
    pub async fn append_automod_action(&self, guild: u64, id: i64, kind: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE automod_hits SET action=action || ' · ' || $3 WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .bind(kind)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn automod_strike(&self, guild: u64, user: u64, now: DateTime<Utc>, window: u32) -> anyhow::Result<i32> {
        let mut tx = self.pool.begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let count = sqlx::query_scalar("INSERT INTO automod_strikes(guild_id,user_id,window_start,count) VALUES($1,$2,$3,1) ON CONFLICT(guild_id,user_id) DO UPDATE SET count=CASE WHEN automod_strikes.window_start <= $3 - $4 * interval '1 second' THEN 1 ELSE automod_strikes.count+1 END, window_start=CASE WHEN automod_strikes.window_start <= $3 - $4 * interval '1 second' THEN $3 ELSE automod_strikes.window_start END RETURNING count")
            .bind(sql_id(guild)).bind(sql_id(user)).bind(now).bind(f64::from(window)).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(count)
    }
    pub async fn reset_automod_strikes(&self, guild: u64, user: u64) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        sqlx::query("DELETE FROM automod_strikes WHERE guild_id=$1 AND user_id=$2")
            .bind(sql_id(guild))
            .bind(sql_id(user))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn automod_sync_status(&self, guild: u64) -> anyhow::Result<Option<AutoModSync>> {
        Ok(
            sqlx::query_as("SELECT revision,sync_generation AS generation,sync_error,synced_at FROM automod_settings WHERE guild_id=$1")
                .bind(sql_id(guild))
                .fetch_optional(&self.pool)
                .await?,
        )
    }
    pub async fn automod_sync_due(&self) -> anyhow::Result<Vec<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT guild_id FROM automod_settings WHERE sync_after <= now() ORDER BY sync_after LIMIT 20",
        )
        .fetch_all(&self.pool)
        .await?)
    }
    pub async fn queue_automod_sync(&self, guild: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE automod_settings SET sync_generation=sync_generation+1, sync_after=LEAST(COALESCE(sync_after,now()+interval '5 seconds'),now()+interval '5 seconds') WHERE guild_id=$1").bind(sql_id(guild)).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn finish_automod_sync(
        &self,
        guild: u64,
        revision: i64,
        generation: i64,
        error: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query("UPDATE automod_settings SET sync_after=CASE WHEN $4::text IS NULL THEN NULL ELSE now()+interval '60 seconds' END, sync_error=$4, synced_at=now() WHERE guild_id=$1 AND revision=$2 AND sync_generation=$3")
            .bind(sql_id(guild)).bind(revision).bind(generation).bind(error).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn set_automod_native(&self, guild: u64, rule: i64, native: Option<u64>) -> anyhow::Result<()> {
        sqlx::query("UPDATE automod_rules SET discord_rule_id=$3 WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(rule)
            .bind(native.map(sql_id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn sweep_automod(&self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM automod_hits WHERE id IN (SELECT id FROM automod_hits WHERE created_at < now()-interval '30 days' LIMIT 1000 FOR UPDATE SKIP LOCKED)").execute(&self.pool).await?;
        sqlx::query("DELETE FROM automod_strikes WHERE (guild_id,user_id) IN (SELECT guild_id,user_id FROM automod_strikes WHERE window_start < now()-interval '30 days' LIMIT 1000 FOR UPDATE SKIP LOCKED)").execute(&self.pool).await?;
        Ok(())
    }
}
