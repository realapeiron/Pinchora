//! Command configuration is cached by the app, invalidated with the server's other settings.
use crate::{Db, sql_id};
use pinchora_core::command_control::Config;
impl Db {
    pub async fn command_control(&self, guild: u64) -> anyhow::Result<Config> {
        let value: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT config FROM command_control WHERE guild_id=$1")
                .bind(sql_id(guild))
                .fetch_optional(self.pool())
                .await?;
        Ok(value.map(serde_json::from_value).transpose()?.unwrap_or_default())
    }
    pub async fn set_command_control(&self, guild: u64, config: &Config) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO command_control(guild_id,config) VALUES($1,$2) ON CONFLICT(guild_id) DO UPDATE SET config=EXCLUDED.config, revision=command_control.revision+1")
            .bind(sql_id(guild)).bind(serde_json::to_value(config)?).execute(self.pool()).await?;
        Ok(())
    }
}

impl Db {
    /// First snapshot wins, including on a retried registration after Discord already deleted IDs.
    pub async fn save_command_permissions(
        &self,
        guild: u64,
        application: u64,
        snapshot: &serde_json::Value,
    ) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO guilds(id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(sql_id(guild))
            .execute(self.pool())
            .await?;
        sqlx::query("INSERT INTO command_permission_snapshots(guild_id,application_id,snapshot) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(sql_id(guild)).bind(sql_id(application)).bind(snapshot).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn saved_command_permissions(
        &self,
        guild: u64,
        application: u64,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        Ok(sqlx::query_scalar(
            "SELECT snapshot FROM command_permission_snapshots WHERE guild_id=$1 AND application_id=$2",
        )
        .bind(sql_id(guild))
        .bind(sql_id(application))
        .fetch_optional(self.pool())
        .await?)
    }
}
