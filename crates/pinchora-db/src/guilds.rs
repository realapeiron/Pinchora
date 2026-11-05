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
    pub welcome_theme: Option<String>,
    pub welcome_title: Option<String>,
    pub welcome_accent: Option<i32>,
    /// Keep every earned level role (true) or only the highest one.
    pub level_roles_stack: bool,
    pub points_enabled: bool,
    pub points_name: String,
    pub points_emoji: String,
    pub points_min: i32,
    pub points_max: i32,
}

macro_rules! columns {
    () => {
        "id, leveling_enabled, levelup_channel_id, levelup_message, \
         welcome_enabled, welcome_channel_id, welcome_message, \
         welcome_theme, welcome_title, welcome_accent, \
         level_roles_stack, points_enabled, points_name, points_emoji, points_min, points_max"
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

    /// Stores the welcome card design; `None` keeps the current value.
    pub async fn update_welcome_card(
        &self,
        guild_id: u64,
        theme: Option<&str>,
        title: Option<&str>,
        accent: Option<i32>,
    ) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET
                 welcome_theme = COALESCE($2, welcome_theme),
                 welcome_title = COALESCE($3, welcome_title),
                 welcome_accent = COALESCE($4, welcome_accent)
             WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(theme)
            .bind(title)
            .bind(accent)
            .fetch_one(self.pool())
            .await?)
    }

    /// Stacking (true) or replacing (false) level roles.
    pub async fn set_level_roles_stack(&self, guild_id: u64, stack: bool) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET level_roles_stack = $2 WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(stack)
            .fetch_one(self.pool())
            .await?)
    }

    /// Stores the server points settings; `None` keeps the current value. The range must
    /// already be valid (`min <= max`), or the database refuses it.
    pub async fn update_points(
        &self,
        guild_id: u64,
        enabled: Option<bool>,
        name: Option<&str>,
        emoji: Option<&str>,
        min: Option<i32>,
        max: Option<i32>,
    ) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET
                 points_enabled = COALESCE($2, points_enabled),
                 points_name = COALESCE($3, points_name),
                 points_emoji = COALESCE($4, points_emoji),
                 points_min = COALESCE($5, points_min),
                 points_max = COALESCE($6, points_max)
             WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(enabled)
            .bind(name)
            .bind(emoji)
            .bind(min)
            .bind(max)
            .fetch_one(self.pool())
            .await?)
    }
}
