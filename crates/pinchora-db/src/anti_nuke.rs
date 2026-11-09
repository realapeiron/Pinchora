//! Anti-nuke storage. Player locks precede feature locks; recoveries have durable claims.
use crate::{
    Db,
    locks::{lock_users, retry, try_lock_users},
    sql_id,
};
use chrono::{DateTime, Utc};
use pinchora_core::{
    anti_nuke::{Config, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOTS},
    i18n::Lang,
};
use serde_json::Value;
use sqlx::{Row, types::Json};
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AntiNukeEvent {
    pub id: i64,
    pub guild_id: i64,
    pub actor_id: i64,
    pub target_user_id: Option<i64>,
    pub target_id: Option<i64>,
    pub kind: String,
    pub count: i32,
    pub action_taken: String,
    pub details: Value,
    pub created_at: DateTime<Utc>,
    pub reverted: bool,
    pub recovery_state: String,
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AntiNukeBackup {
    pub user_id: i64,
    pub role_ids: Vec<i64>,
    pub quarantined: bool,
    pub restore_on_join: bool,
}
impl Db {
    pub async fn antinuke_config(&self, guild: u64) -> anyhow::Result<Config> {
        let Some(r) = sqlx::query("SELECT * FROM antinuke WHERE guild_id=$1")
            .bind(sql_id(guild))
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(Config::default());
        };
        let ids = |k| -> anyhow::Result<Vec<String>> {
            Ok(r.try_get::<Vec<i64>, _>(k)?.iter().map(ToString::to_string).collect())
        };
        let id = |k| -> anyhow::Result<Option<String>> { Ok(r.try_get::<Option<i64>, _>(k)?.map(|n| n.to_string())) };
        Ok(Config {
            enabled: r.try_get("enabled")?,
            limits: r.try_get::<Json<_>, _>("limits")?.0,
            response: r.try_get("response")?,
            log_channel_id: id("log_channel_id")?,
            quarantine_role_id: id("quarantine_role_id")?,
            vip_roles: ids("vip_roles")?,
            trusted_users: ids("trusted_users")?,
            owner_only_config: r.try_get("owner_only_config")?,
            auto_restore: r.try_get("auto_restore")?,
            revert_vip_bans: r.try_get("revert_vip_bans")?,
            remove_bots: r.try_get("remove_bots")?,
        })
    }
    pub async fn save_antinuke(&self, guild: u64, actor: u64, c: &Config) -> anyhow::Result<()> {
        pinchora_core::anti_nuke::validate(c, Lang::En).map_err(anyhow::Error::msg)?;
        let mut tx = self.pool.begin().await?;
        let mut users = c
            .trusted_users
            .iter()
            .map(|s| s.parse::<i64>())
            .collect::<Result<Vec<_>, _>>()?;
        users.push(sql_id(actor));
        lock_users(&mut tx, &users).await?;
        let ids = |v: &[String]| v.iter().map(|s| s.parse::<i64>()).collect::<Result<Vec<_>, _>>();
        sqlx::query("INSERT INTO antinuke(guild_id,enabled,limits,response,log_channel_id,quarantine_role_id,vip_roles,trusted_users,owner_only_config,auto_restore,revert_vip_bans,remove_bots) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(guild_id) DO UPDATE SET enabled=$2,limits=$3,response=$4,log_channel_id=$5,quarantine_role_id=$6,vip_roles=$7,trusted_users=$8,owner_only_config=$9,auto_restore=$10,revert_vip_bans=$11,remove_bots=$12")
            .bind(sql_id(guild)).bind(c.enabled).bind(Json(&c.limits)).bind(&c.response).bind(c.log_channel_id.as_deref().map(str::parse::<i64>).transpose()?).bind(c.quarantine_role_id.as_deref().map(str::parse::<i64>).transpose()?).bind(ids(&c.vip_roles)?).bind(ids(&c.trusted_users)?).bind(c.owner_only_config).bind(c.auto_restore).bind(c.revert_vip_bans).bind(c.remove_bots).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    /// Wait without occupying the pool: a burst must not strand the lock holder waiting for a
    /// second connection behind other tasks that are themselves waiting for its advisory lock.
    pub async fn lock_antinuke(&self, guild: u64) -> anyhow::Result<sqlx::Transaction<'_, sqlx::Postgres>> {
        loop {
            let mut tx = self.pool.begin().await?;
            let locked = sqlx::query_scalar::<_, bool>(
                "SELECT pg_try_advisory_xact_lock(hashtextextended('antinuke-runtime:' || $1::text,0))",
            )
            .bind(sql_id(guild))
            .fetch_one(&mut *tx)
            .await;
            if matches!(locked, Ok(true)) {
                return Ok(tx);
            }
            let released = tx.rollback().await;
            locked?;
            released?;
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }
    pub async fn antinuke_events(&self, guild: u64) -> anyhow::Result<Vec<AntiNukeEvent>> {
        Ok(
            sqlx::query_as(
                "SELECT * FROM antinuke_events WHERE guild_id=$1 ORDER BY created_at DESC,id DESC LIMIT 100",
            )
            .bind(sql_id(guild))
            .fetch_all(&self.pool)
            .await?,
        )
    }
    pub async fn antinuke_recovery_window(
        &self,
        guild: u64,
        actor: i64,
        kind: &str,
        at: DateTime<Utc>,
        window: u32,
    ) -> anyhow::Result<Vec<AntiNukeEvent>> {
        Ok(sqlx::query_as("SELECT * FROM antinuke_events WHERE guild_id=$1 AND actor_id=$2 AND kind=$3 AND created_at BETWEEN $4-$5*interval '1 second' AND $4 ORDER BY created_at,id LIMIT 1000").bind(sql_id(guild)).bind(actor).bind(kind).bind(at).bind(f64::from(window)).fetch_all(&self.pool).await?)
    }
    pub async fn antinuke_event(&self, guild: u64, id: i64) -> anyhow::Result<Option<AntiNukeEvent>> {
        Ok(
            sqlx::query_as("SELECT * FROM antinuke_events WHERE guild_id=$1 AND id=$2")
                .bind(sql_id(guild))
                .bind(id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }
    pub async fn open_antinuke_event(&self, e: &AntiNukeEvent) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;
        lock_users(&mut tx, &[e.actor_id, e.target_user_id.unwrap_or(e.actor_id)]).await?;
        let n=sqlx::query("INSERT INTO antinuke_events(id,guild_id,actor_id,target_user_id,target_id,kind,created_at,details) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING").bind(e.id).bind(e.guild_id).bind(e.actor_id).bind(e.target_user_id).bind(e.target_id).bind(&e.kind).bind(e.created_at).bind(&e.details).execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;
        Ok(n == 1)
    }
    pub async fn finish_antinuke_event(&self, guild: u64, id: i64, count: u32, action: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE antinuke_events SET count=$3,action_taken=$4 WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .bind(count as i32)
            .bind(action)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn claim_antinuke_restore(&self, guild: u64, id: i64) -> anyhow::Result<bool> {
        Ok(sqlx::query("UPDATE antinuke_events SET recovery_state='running' WHERE guild_id=$1 AND id=$2 AND recovery_state='ready' AND NOT reverted").bind(sql_id(guild)).bind(id).execute(&self.pool).await?.rows_affected()==1)
    }
    pub async fn reset_antinuke_restore(&self, guild: u64, id: i64) -> anyhow::Result<()> {
        sqlx::query("UPDATE antinuke_events SET recovery_state='ready' WHERE guild_id=$1 AND id=$2 AND recovery_state='running'").bind(sql_id(guild)).bind(id).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn finish_antinuke_restore(&self, guild: u64, id: i64, ok: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE antinuke_events SET reverted=$3,recovery_state=CASE WHEN $3 THEN 'done' ELSE 'failed' END WHERE guild_id=$1 AND id=$2").bind(sql_id(guild)).bind(id).bind(ok).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn backup_antinuke_roles(
        &self,
        guild: u64,
        user: u64,
        roles: &[i64],
        quarantined: bool,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(roles.len() <= 250, "too many roles");
        let mut tx = self.pool.begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        sqlx::query("INSERT INTO antinuke_role_backups(guild_id,user_id,role_ids,quarantined) VALUES($1,$2,$3,$4) ON CONFLICT(guild_id,user_id) DO UPDATE SET role_ids=EXCLUDED.role_ids,taken_at=now(),quarantined=$4 WHERE NOT antinuke_role_backups.quarantined AND NOT antinuke_role_backups.restore_on_join").bind(sql_id(guild)).bind(sql_id(user)).bind(roles).bind(quarantined).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM antinuke_role_backups WHERE (guild_id,user_id) IN (SELECT guild_id,user_id FROM antinuke_role_backups WHERE guild_id=$1 ORDER BY taken_at DESC OFFSET 5000 FOR UPDATE SKIP LOCKED)").bind(sql_id(guild)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn antinuke_backup(&self, guild: u64, user: u64) -> anyhow::Result<Option<AntiNukeBackup>> {
        Ok(sqlx::query_as("SELECT user_id,role_ids,quarantined,restore_on_join FROM antinuke_role_backups WHERE guild_id=$1 AND user_id=$2 AND taken_at>now()-interval '7 days'").bind(sql_id(guild)).bind(sql_id(user)).fetch_optional(&self.pool).await?)
    }
    pub async fn claim_antinuke_member_restore(&self, guild: u64, user: u64) -> anyhow::Result<bool> {
        Ok(sqlx::query("UPDATE antinuke_role_backups SET quarantined=FALSE,restore_on_join=FALSE WHERE guild_id=$1 AND user_id=$2 AND (quarantined OR restore_on_join)")
            .bind(sql_id(guild)).bind(sql_id(user)).execute(&self.pool).await?.rows_affected() == 1)
    }
    pub async fn antinuke_rejoin(&self, guild: u64, user: u64, pending: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE antinuke_role_backups SET restore_on_join=$3,quarantined=CASE WHEN $3 THEN quarantined ELSE FALSE END WHERE guild_id=$1 AND user_id=$2").bind(sql_id(guild)).bind(sql_id(user)).bind(pending).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn save_antinuke_snapshot(
        &self,
        guild: u64,
        kind: &str,
        id: u64,
        data: &Value,
        overwrites: &[(i64, String, String)],
    ) -> anyhow::Result<()> {
        retry("anti-nuke snapshot", || {
            self.save_antinuke_snapshot_once(guild, kind, id, data, overwrites)
        })
        .await
    }
    async fn save_antinuke_snapshot_once(
        &self,
        guild: u64,
        kind: &str,
        id: u64,
        data: &Value,
        overwrites: &[(i64, String, String)],
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            serde_json::to_vec(&(data, overwrites))?.len() <= MAX_SNAPSHOT_BYTES
                && overwrites.len() <= 100
                && overwrites.iter().all(|(user, allow, deny)| *user > 0
                    && allow.parse::<u64>().is_ok()
                    && deny.parse::<u64>().is_ok()),
            "snapshot too large"
        );
        let previous: Vec<i64> =
            sqlx::query_scalar("SELECT user_id FROM antinuke_overwrites WHERE guild_id=$1 AND channel_id=$2")
                .bind(sql_id(guild))
                .bind(sql_id(id))
                .fetch_all(&self.pool)
                .await?;
        let mut users = previous;
        users.extend(overwrites.iter().map(|o| o.0));
        let mut tx = self.pool.begin().await?;
        lock_users(&mut tx, &users).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('antinuke-snapshot:' || $1::text,0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        let mut data = data.clone();
        if kind == "channel" {
            data["member_overwrite_count"] = serde_json::json!(overwrites.len());
        }
        let saved = sqlx::query("INSERT INTO antinuke_snapshots(guild_id,kind,object_id,data) VALUES($1,$2,$3,$4) ON CONFLICT(guild_id,kind,object_id) DO UPDATE SET data=$4,taken_at=now() WHERE antinuke_snapshots.restored_id IS NULL").bind(sql_id(guild)).bind(kind).bind(sql_id(id)).bind(Json(&data)).execute(&mut *tx).await?.rows_affected();
        if kind == "channel" && saved > 0 {
            let actual: Vec<i64> =
                sqlx::query_scalar("SELECT user_id FROM antinuke_overwrites WHERE guild_id=$1 AND channel_id=$2")
                    .bind(sql_id(guild))
                    .bind(sql_id(id))
                    .fetch_all(&mut *tx)
                    .await?;
            try_lock_users(&mut tx, &actual).await?;
            sqlx::query("DELETE FROM antinuke_overwrites WHERE guild_id=$1 AND channel_id=$2")
                .bind(sql_id(guild))
                .bind(sql_id(id))
                .execute(&mut *tx)
                .await?;
            for (user, allow, deny) in overwrites {
                sqlx::query("INSERT INTO antinuke_overwrites(guild_id,channel_id,user_id,allow_bits,deny_bits) VALUES($1,$2,$3,$4,$5)").bind(sql_id(guild)).bind(sql_id(id)).bind(user).bind(allow).bind(deny).execute(&mut *tx).await?;
            }
        }
        sqlx::query("DELETE FROM antinuke_snapshots WHERE (guild_id,kind,object_id) IN (SELECT guild_id,kind,object_id FROM antinuke_snapshots WHERE guild_id=$1 ORDER BY taken_at DESC OFFSET $2 FOR UPDATE SKIP LOCKED)").bind(sql_id(guild)).bind(MAX_SNAPSHOTS).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn antinuke_snapshot(&self, guild: u64, kind: &str, id: u64) -> anyhow::Result<Option<Value>> {
        let row:Option<(Json<Value>,)>=sqlx::query_as("SELECT data FROM antinuke_snapshots WHERE guild_id=$1 AND kind=$2 AND object_id=$3 AND taken_at>now()-interval '7 days'").bind(sql_id(guild)).bind(kind).bind(sql_id(id)).fetch_optional(&self.pool).await?;
        let Some((Json(mut data),)) = row else { return Ok(None) };
        if kind == "channel" {
            let rows: Vec<(i64, String, String)> = sqlx::query_as(
                "SELECT user_id,allow_bits,deny_bits FROM antinuke_overwrites WHERE guild_id=$1 AND channel_id=$2",
            )
            .bind(sql_id(guild))
            .bind(sql_id(id))
            .fetch_all(&self.pool)
            .await?;
            if data["member_overwrite_count"].as_u64() != Some(rows.len() as u64) {
                return Ok(None);
            }
            let list = data["permission_overwrites"]
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("missing overwrites"))?;
            for (user, allow, deny) in rows {
                list.push(serde_json::json!({"id":user.to_string(),"type":1,"allow":allow,"deny":deny}));
            }
        }
        Ok(Some(data))
    }
    pub async fn antinuke_remap(&self, guild: u64, kind: &str, id: u64) -> anyhow::Result<u64> {
        let mapped: Option<i64> = sqlx::query_scalar(
            "SELECT restored_id FROM antinuke_snapshots WHERE guild_id=$1 AND kind=$2 AND object_id=$3",
        )
        .bind(sql_id(guild))
        .bind(kind)
        .bind(sql_id(id))
        .fetch_optional(&self.pool)
        .await?
        .flatten();
        Ok(mapped.map_or(id, |n| n as u64))
    }
    pub async fn antinuke_restored_object(&self, guild: u64, kind: &str, id: u64, new: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE antinuke_snapshots SET restored_id=$4 WHERE guild_id=$1 AND kind=$2 AND object_id=$3")
            .bind(sql_id(guild))
            .bind(kind)
            .bind(sql_id(id))
            .bind(sql_id(new))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn antinuke_guilds(&self) -> anyhow::Result<Vec<i64>> {
        Ok(
            sqlx::query_scalar("SELECT guild_id FROM antinuke WHERE enabled ORDER BY guild_id")
                .fetch_all(&self.pool)
                .await?,
        )
    }
    pub async fn sweep_antinuke(&self) -> anyhow::Result<()> {
        for query in [
            "DELETE FROM antinuke_events WHERE id IN (SELECT id FROM antinuke_events WHERE created_at<now()-interval '30 days' LIMIT 1000 FOR UPDATE SKIP LOCKED)",
            "DELETE FROM antinuke_role_backups WHERE (guild_id,user_id) IN (SELECT guild_id,user_id FROM antinuke_role_backups WHERE taken_at<now()-interval '7 days' LIMIT 1000 FOR UPDATE SKIP LOCKED)",
            "DELETE FROM antinuke_snapshots WHERE (guild_id,kind,object_id) IN (SELECT guild_id,kind,object_id FROM antinuke_snapshots WHERE taken_at<now()-interval '7 days' LIMIT 1000 FOR UPDATE SKIP LOCKED)",
        ] {
            retry("anti-nuke retention", || async {
                sqlx::query(query).execute(&self.pool).await?;
                Ok(())
            })
            .await?;
        }
        Ok(())
    }
}
