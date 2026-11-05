use crate::{Db, sql_id};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GuildSettings {
    pub id: i64,
    pub leveling_enabled: bool,
    pub levelup_channel_id: Option<i64>,
    pub levelup_message: Option<String>,
    pub welcome_enabled: bool,
    pub welcome_channel_id: Option<i64>,
    pub welcome_message: Option<String>,
}

macro_rules! columns {
    () => {
        "id, leveling_enabled, levelup_channel_id, levelup_message, \
         welcome_enabled, welcome_channel_id, welcome_message"
    };
}

impl Db {
    pub async fn guild_settings(&self, guild_id: u64) -> anyhow::Result<GuildSettings> {
        let sql = concat!(
            "WITH ins AS (INSERT INTO guilds (id) VALUES ($1) ON CONFLICT (id) DO NOTHING RETURNING ",
            columns!(),
            ") SELECT * FROM ins UNION ALL SELECT ",
            columns!(),
            " FROM guilds WHERE id = $1 LIMIT 1"
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .fetch_one(self.pool())
            .await?)
    }

    pub async fn update_welcome(
        &self,
        guild_id: u64,
        enabled: Option<bool>,
        channel_id: Option<u64>,
        message: Option<&str>,
    ) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET
                 welcome_enabled = COALESCE($2, welcome_enabled),
                 welcome_channel_id = COALESCE($3, welcome_channel_id),
                 welcome_message = COALESCE($4, welcome_message)
             WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(enabled)
            .bind(channel_id.map(sql_id))
            .bind(message)
            .fetch_one(self.pool())
            .await?)
    }

    pub async fn update_leveling(
        &self,
        guild_id: u64,
        enabled: Option<bool>,
        channel_id: Option<u64>,
        message: Option<&str>,
    ) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET
                 leveling_enabled = COALESCE($2, leveling_enabled),
                 levelup_channel_id = COALESCE($3, levelup_channel_id),
                 levelup_message = COALESCE($4, levelup_message)
             WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(enabled)
            .bind(channel_id.map(sql_id))
            .bind(message)
            .fetch_one(self.pool())
            .await?)
    }
}
