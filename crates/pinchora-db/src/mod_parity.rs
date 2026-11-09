//! Moderation policy and private queues. All lookups include the guild boundary.
use crate::Db;
use pinchora_core::mod_parity::Config;
use serde_json::Value;
impl Db {
    /// Non-expiring fleet exclusion; the caller must await commit or rollback before returning.
    pub async fn claim_mod_parity_sweep(&self) -> anyhow::Result<Option<sqlx::Transaction<'_, sqlx::Postgres>>> {
        let mut tx = self.pool().begin().await?;
        let claimed = sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_xact_lock(74, 0)")
            .fetch_one(&mut *tx)
            .await;
        match claimed {
            Ok(true) => Ok(Some(tx)),
            result => {
                tx.rollback().await?;
                result?;
                Ok(None)
            }
        }
    }

    /// Personal D11 export. Staff-only evidence and third-party identities stay private.
    pub async fn export_mod_parity_data(&self, user: u64) -> anyhow::Result<Value> {
        let mut data = serde_json::Map::new();
        // Fixed statements only: no table/column names or target identities come from the client.
        for (table, query) in [
            (
                "mod_notes",
                "SELECT jsonb_build_object('id',id::text,'guild_id',guild_id::text,'created_at',created_at,'body',CASE WHEN author_id=$1 THEN body ELSE NULL END) FROM mod_notes WHERE user_id=$1 OR author_id=$1",
            ),
            (
                "mod_reports",
                "SELECT jsonb_build_object('id',id::text,'guild_id',guild_id::text,'status',status,'created_at',created_at,'body',CASE WHEN reporter_id=$1 THEN body ELSE NULL END) FROM mod_reports WHERE user_id=$1 OR reporter_id=$1 OR reviewer_id=$1",
            ),
            (
                "mod_appeals",
                "SELECT jsonb_build_object('id',id::text,'guild_id',guild_id::text,'case_number',case_number,'body',body,'status',status,'decision',decision,'created_at',created_at) FROM mod_appeals WHERE user_id=$1 OR reviewer_id=$1",
            ),
            (
                "mod_restrictions",
                "SELECT jsonb_build_object('guild_id',guild_id::text,'role_id',role_id::text,'role_ids',ARRAY(SELECT r::text FROM unnest(role_ids) r),'kind',kind,'created_at',created_at) FROM mod_restrictions WHERE user_id=$1",
            ),
            (
                "mod_voice_bans",
                "SELECT jsonb_build_object('guild_id',guild_id::text) FROM mod_voice_bans WHERE user_id=$1",
            ),
            (
                "mod_report_blocks",
                "SELECT jsonb_build_object('guild_id',guild_id::text) FROM mod_report_blocks WHERE user_id=$1",
            ),
            (
                "mod_booster_roles",
                "SELECT jsonb_build_object('guild_id',guild_id::text,'role_id',role_id::text) FROM mod_booster_roles WHERE user_id=$1",
            ),
        ] {
            let rows: Vec<Value> = sqlx::query_scalar(query)
                .bind(user as i64)
                .fetch_all(self.pool())
                .await?;
            data.insert(table.into(), Value::Array(rows));
        }
        Ok(Value::Object(data))
    }

    pub async fn member_appealable_cases(&self, user: u64) -> anyhow::Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('guild',c.guild_id::text,'number',c.number,'kind',c.kind,'reason',c.reason) FROM mod_cases c JOIN mod_parity_settings s ON s.guild_id=c.guild_id WHERE c.target_id=$1 AND c.kind IN ('ban','warn') AND c.active AND s.config->'appeals'='true'::jsonb ORDER BY c.created_at DESC LIMIT 100")
            .bind(user as i64).fetch_all(self.pool()).await?)
    }

    pub async fn member_mod_appeal(&self, guild: u64, case: i32, user: u64) -> anyhow::Result<Option<Value>> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('status',status,'decision',decision) FROM mod_appeals WHERE guild_id=$1 AND case_number=$2 AND user_id=$3")
            .bind(guild as i64).bind(case).bind(user as i64).fetch_optional(self.pool()).await?)
    }

    pub async fn mod_parity_config(&self, guild: u64) -> anyhow::Result<Config> {
        let v: Option<Value> = sqlx::query_scalar("SELECT config FROM mod_parity_settings WHERE guild_id=$1")
            .bind(guild as i64)
            .fetch_optional(self.pool())
            .await?;
        Ok(v.map(serde_json::from_value).transpose()?.unwrap_or_default())
    }
    pub async fn save_mod_parity_config(&self, guild: u64, c: &Config) -> anyhow::Result<()> {
        anyhow::ensure!(c.valid(), "invalid moderation policy");
        sqlx::query("INSERT INTO mod_parity_settings(guild_id,config) VALUES($1,$2) ON CONFLICT(guild_id) DO UPDATE SET config=$2")
            .bind(guild as i64).bind(serde_json::to_value(c)?).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn mod_parity_queues(&self, guild: u64) -> anyhow::Result<Value> {
        let reports: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(r) || jsonb_build_object('id',id::text,'user_id',user_id::text,'reporter_id',reporter_id::text,'reviewer_id',reviewer_id::text,'channel_id',channel_id::text,'message_id',message_id::text,'guild_id',guild_id::text) FROM mod_reports r WHERE guild_id=$1 ORDER BY (status='pending') DESC,id DESC LIMIT 100").bind(guild as i64).fetch_all(self.pool()).await?;
        let appeals: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(r) || jsonb_build_object('id',id::text,'user_id',user_id::text,'reviewer_id',reviewer_id::text,'guild_id',guild_id::text) FROM mod_appeals r WHERE guild_id=$1 ORDER BY (status='pending') DESC,id DESC LIMIT 100").bind(guild as i64).fetch_all(self.pool()).await?;
        Ok(serde_json::json!({"reports":reports,"appeals":appeals}))
    }
    pub async fn submit_mod_report(
        &self,
        guild: u64,
        user: u64,
        reporter: u64,
        channel: Option<u64>,
        message: Option<u64>,
        body: &str,
    ) -> anyhow::Result<Option<i64>> {
        if user == reporter || !(1..=1000).contains(&body.trim().chars().count()) {
            return Ok(None);
        }
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1,$2)")
            .bind(74_i32)
            .bind((guild % i32::MAX as u64) as i32)
            .execute(&mut *tx)
            .await?;
        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mod_report_blocks WHERE guild_id=$1 AND user_id=$2) OR EXISTS(SELECT 1 FROM mod_reports WHERE guild_id=$1 AND reporter_id=$2 AND created_at>now()-interval '1 minute') OR (SELECT count(*) FROM mod_reports WHERE guild_id=$1 AND reporter_id=$2 AND created_at>now()-interval '1 hour') >= 5 OR (SELECT count(*) FROM mod_reports WHERE guild_id=$1 AND created_at>now()-interval '1 minute') >= 30").bind(guild as i64).bind(reporter as i64).fetch_one(&mut *tx).await?;
        if blocked {
            return Ok(None);
        }
        let id=sqlx::query_scalar("INSERT INTO mod_reports(guild_id,user_id,reporter_id,channel_id,message_id,body) VALUES($1,$2,$3,$4,$5,$6) RETURNING id").bind(guild as i64).bind(user as i64).bind(reporter as i64).bind(channel.map(|v|v as i64)).bind(message.map(|v|v as i64)).bind(body).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(id))
    }
    pub async fn submit_mod_appeal(&self, guild: u64, user: u64, case: i32, body: &str) -> anyhow::Result<bool> {
        if !(10..=2000).contains(&body.trim().chars().count()) {
            return Ok(false);
        }
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("mod-appeal:{guild}:{user}"))
            .execute(&mut *tx)
            .await?;
        let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mod_appeals WHERE guild_id=$1 AND user_id=$2 AND (status='pending' OR created_at>now()-interval '10 minutes'))")
            .bind(guild as i64).bind(user as i64).fetch_one(&mut *tx).await?;
        if blocked {
            return Ok(false);
        }
        let result=sqlx::query("INSERT INTO mod_appeals(guild_id,user_id,case_number,body) SELECT $1,$2,$3,$4 WHERE EXISTS(SELECT 1 FROM mod_cases WHERE guild_id=$1 AND number=$3 AND target_id=$2 AND kind IN ('ban','warn') AND active) ON CONFLICT DO NOTHING")
            .bind(guild as i64).bind(user as i64).bind(case).bind(body.trim()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result.rows_affected() == 1)
    }
}
