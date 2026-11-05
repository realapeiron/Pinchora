use crate::{Db, sql_id};

#[derive(Debug, Clone, Default, sqlx::FromRow)]
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
    /// Role given to every new member (bots excepted); `None` is off.
    pub join_role_id: Option<i64>,
    /// Where goodbye messages go; `None` is off.
    pub goodbye_channel_id: Option<i64>,
    pub goodbye_message: Option<String>,
    /// Where the server log goes; `None` is off.
    pub log_channel_id: Option<i64>,
    pub log_members: bool,
    pub log_names: bool,
    pub log_bans: bool,
    pub log_voice: bool,
}

/// What the server log can report. Each kind has its own switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    /// Members joining and leaving.
    Members,
    /// Nickname and username changes.
    Names,
    /// Bans and unbans.
    Bans,
    /// Voice joins, leaves and moves.
    Voice,
}

impl GuildSettings {
    /// The channel the log of `kind` goes to: set, and switched on.
    pub fn log_channel(&self, kind: LogKind) -> Option<u64> {
        let on = match kind {
            LogKind::Members => self.log_members,
            LogKind::Names => self.log_names,
            LogKind::Bans => self.log_bans,
            LogKind::Voice => self.log_voice,
        };
        self.log_channel_id.filter(|_| on).map(|id| id as u64)
    }
}

macro_rules! columns {
    () => {
        "id, leveling_enabled, levelup_channel_id, levelup_message, \
         welcome_enabled, welcome_channel_id, welcome_message, \
         welcome_theme, welcome_title, welcome_accent, \
         level_roles_stack, points_enabled, points_name, points_emoji, points_min, points_max, \
         join_role_id, goodbye_channel_id, goodbye_message, \
         log_channel_id, log_members, log_names, log_bans, log_voice"
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

    /// Sets the role every new member gets, or turns it off with `None`.
    pub async fn set_join_role(&self, guild_id: u64, role_id: Option<u64>) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET join_role_id = $2 WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(role_id.map(sql_id))
            .fetch_one(self.pool())
            .await?)
    }

    /// Stores the goodbye channel and message, which turns goodbye messages on once a channel is set;
    /// `None` keeps the current value.
    pub async fn update_goodbye(
        &self,
        guild_id: u64,
        channel_id: Option<u64>,
        message: Option<&str>,
    ) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET
                 goodbye_channel_id = COALESCE($2, goodbye_channel_id),
                 goodbye_message = COALESCE($3, goodbye_message)
             WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(channel_id.map(sql_id))
            .bind(message)
            .fetch_one(self.pool())
            .await?)
    }

    /// Turns goodbye messages off; the message text is kept for next time.
    pub async fn clear_goodbye(&self, guild_id: u64) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET goodbye_channel_id = NULL WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .fetch_one(self.pool())
            .await?)
    }

    /// Sets the server log channel, or turns the log off with `None`.
    pub async fn set_log_channel(&self, guild_id: u64, channel_id: Option<u64>) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET log_channel_id = $2 WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(channel_id.map(sql_id))
            .fetch_one(self.pool())
            .await?)
    }

    /// Switches kinds of log events on or off; `None` keeps the current value.
    pub async fn update_log_events(
        &self,
        guild_id: u64,
        members: Option<bool>,
        names: Option<bool>,
        bans: Option<bool>,
        voice: Option<bool>,
    ) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET
                 log_members = COALESCE($2, log_members),
                 log_names = COALESCE($3, log_names),
                 log_bans = COALESCE($4, log_bans),
                 log_voice = COALESCE($5, log_voice)
             WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(members)
            .bind(names)
            .bind(bans)
            .bind(voice)
            .fetch_one(self.pool())
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(channel: Option<i64>, members: bool, names: bool, bans: bool, voice: bool) -> GuildSettings {
        GuildSettings {
            log_channel_id: channel,
            log_members: members,
            log_names: names,
            log_bans: bans,
            log_voice: voice,
            ..Default::default()
        }
    }

    #[test]
    fn log_kinds_follow_their_switch() {
        let s = settings(Some(7), true, false, true, false);
        assert_eq!(s.log_channel(LogKind::Members), Some(7));
        assert_eq!(s.log_channel(LogKind::Names), None);
        assert_eq!(s.log_channel(LogKind::Bans), Some(7));
        assert_eq!(s.log_channel(LogKind::Voice), None);
    }

    #[test]
    fn no_log_channel_means_nothing_is_logged() {
        let s = settings(None, true, true, true, true);
        for kind in [LogKind::Members, LogKind::Names, LogKind::Bans, LogKind::Voice] {
            assert_eq!(s.log_channel(kind), None, "{kind:?}");
        }
    }
}
