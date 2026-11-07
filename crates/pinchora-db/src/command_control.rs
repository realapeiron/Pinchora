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
