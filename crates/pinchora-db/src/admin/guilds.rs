use pinchora_core::admin::settings;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::admin::settings::GuildSettingsPatch;
use sqlx::{Postgres, QueryBuilder};

use crate::{Db, sql_id};

/// A patch refused because of what the row holds at the moment of writing, e.g. points that would
/// end up with the least above the most. Found inside `patch_guild_settings` with `downcast_ref`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchRefused(pub FieldError);

impl std::fmt::Display for PatchRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.0.field, self.0.message)
    }
}

impl std::error::Error for PatchRefused {}

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
    /// Where level-ups are announced: `channel`, `current`, `dm` or `off`.
    pub levelup_mode: String,
    /// Whether members in voice earn server XP.
    pub voice_xp_enabled: bool,
    /// Server XP a minute in voice earns.
    pub voice_xp_amount: i32,
    /// What text commands start with; `None` is [`settings::DEFAULT_PREFIX`].
    pub prefix: Option<String>,
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

impl GuildSettings {
    /// The prefix text commands start with here.
    pub fn prefix(&self) -> &str {
        self.prefix.as_deref().unwrap_or(settings::DEFAULT_PREFIX)
    }

    /// These settings with `patch` applied, as [`Db::patch_guild_settings`] would store them.
    pub fn apply(&self, patch: &GuildSettingsPatch) -> GuildSettings {
        let mut s = self.clone();
        let id = |v: Option<u64>| v.map(sql_id);
        let p = patch;
        macro_rules! set {
            ($field:ident) => {
                if let Some(v) = &p.$field {
                    s.$field = v.clone();
                }
            };
            ($field:ident, $map:expr) => {
                if let Some(v) = &p.$field {
                    s.$field = $map(v.clone());
                }
            };
        }
        set!(leveling_enabled);
        set!(levelup_channel_id, id);
        set!(levelup_message);
        set!(levelup_mode, |m: pinchora_core::progression::leveling::LevelUpMode| m
            .key()
            .to_owned());
        set!(level_roles_stack);
        set!(voice_xp_enabled);
        set!(voice_xp_amount, |n: i64| n as i32);
        set!(welcome_enabled);
        set!(welcome_channel_id, id);
        set!(welcome_message);
        set!(welcome_theme);
        set!(welcome_title);
        set!(welcome_accent, |c: Option<u32>| c.map(|c| c as i32));
        set!(join_role_id, id);
        set!(goodbye_channel_id, id);
        set!(goodbye_message);
        set!(points_enabled);
        set!(points_name);
        set!(points_emoji);
        set!(points_min, |n: i64| n as i32);
        set!(points_max, |n: i64| n as i32);
        set!(log_channel_id, id);
        set!(log_members);
        set!(log_names);
        set!(log_bans);
        set!(log_voice);
        set!(prefix);
        s
    }
}

macro_rules! columns {
    () => {
        "id, leveling_enabled, levelup_channel_id, levelup_message, \
         welcome_enabled, welcome_channel_id, welcome_message, \
         welcome_theme, welcome_title, welcome_accent, \
         level_roles_stack, points_enabled, points_name, points_emoji, points_min, points_max, \
         join_role_id, goodbye_channel_id, goodbye_message, \
         log_channel_id, log_members, log_names, log_bans, log_voice, \
         levelup_mode, voice_xp_enabled, voice_xp_amount, prefix"
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
        if let Some(settings) = sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .fetch_optional(self.pool())
            .await?
        {
            return Ok(settings);
        }
        // A concurrent insert can win the conflict after this statement's snapshot was taken.
        // Read again with a fresh snapshot so the committed winner is visible.
        Ok(
            sqlx::query_as::<_, GuildSettings>(concat!("SELECT ", columns!(), " FROM guilds WHERE id = $1"))
                .bind(sql_id(guild_id))
                .fetch_one(self.pool())
                .await?,
        )
    }

    /// Writes exactly the columns `patch` names (clearing those set to `Some(None)`) and returns the
    /// row. The patch must have passed `settings::check_patch`. The points range is checked again
    /// against the row as it is locked for the write, so two patches that each move one end cannot
    /// leave the least above the most: the second fails with [`PatchRefused`].
    pub async fn patch_guild_settings(
        &self,
        guild_id: u64,
        patch: &GuildSettingsPatch,
    ) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let mut tx = self.pool().begin().await?;
        let (min, max): (i32, i32) =
            sqlx::query_as("SELECT points_min, points_max FROM guilds WHERE id = $1 FOR UPDATE")
                .bind(sql_id(guild_id))
                .fetch_one(&mut *tx)
                .await?;
        if patch.points_min.is_some() || patch.points_max.is_some() {
            let min = patch.points_min.unwrap_or(min.into());
            let max = patch.points_max.unwrap_or(max.into());
            if let Err(error) = settings::check_points(min, max) {
                return Err(PatchRefused(error).into());
            }
        }
        let mut sql: QueryBuilder<Postgres> = QueryBuilder::new("UPDATE guilds SET id = id");
        let p = patch;
        let id = |v: Option<u64>| v.map(sql_id);
        macro_rules! bind {
            ($field:ident) => {
                if let Some(v) = &p.$field {
                    sql.push(concat!(", ", stringify!($field), " = "))
                        .push_bind(v.clone());
                }
            };
            ($field:ident, $map:expr) => {
                if let Some(v) = &p.$field {
                    sql.push(concat!(", ", stringify!($field), " = "))
                        .push_bind($map(v.clone()));
                }
            };
        }
        bind!(leveling_enabled);
        bind!(levelup_channel_id, id);
        bind!(levelup_message);
        bind!(levelup_mode, |m: pinchora_core::progression::leveling::LevelUpMode| m
            .key());
        bind!(level_roles_stack);
        bind!(voice_xp_enabled);
        bind!(voice_xp_amount, |n: i64| n as i32);
        bind!(welcome_enabled);
        bind!(welcome_channel_id, id);
        bind!(welcome_message);
        bind!(welcome_theme);
        bind!(welcome_title);
        bind!(welcome_accent, |c: Option<u32>| c.map(|c| c as i32));
        bind!(join_role_id, id);
        bind!(goodbye_channel_id, id);
        bind!(goodbye_message);
        bind!(points_enabled);
        bind!(points_name);
        bind!(points_emoji);
        bind!(points_min, |n: i64| n as i32);
        bind!(points_max, |n: i64| n as i32);
        bind!(log_channel_id, id);
        bind!(log_members);
        bind!(log_names);
        bind!(log_bans);
        bind!(log_voice);
        bind!(prefix);
        sql.push(" WHERE id = ")
            .push_bind(sql_id(guild_id))
            .push(concat!(" RETURNING ", columns!()));
        let row = sql.build_query_as::<GuildSettings>().fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(row)
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

    /// Sets the text command prefix; `None` goes back to the default. It must have passed
    /// `settings::check_prefix`.
    pub async fn set_prefix(&self, guild_id: u64, prefix: Option<&str>) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!("UPDATE guilds SET prefix = $2 WHERE id = $1 RETURNING ", columns!());
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(prefix)
            .fetch_one(self.pool())
            .await?)
    }

    /// Sets where level-ups are announced (`channel`, `current`, `dm` or `off`).
    pub async fn set_levelup_mode(&self, guild_id: u64, mode: &str) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET levelup_mode = $2 WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(mode)
            .fetch_one(self.pool())
            .await?)
    }

    /// Turns voice XP on or off and sets what a minute earns; `None` keeps the current value. The
    /// amount must already be in range, or the database refuses it.
    pub async fn update_voice_xp(
        &self,
        guild_id: u64,
        enabled: Option<bool>,
        amount: Option<i32>,
    ) -> anyhow::Result<GuildSettings> {
        self.guild_settings(guild_id).await?;
        let sql = concat!(
            "UPDATE guilds SET
                 voice_xp_enabled = COALESCE($2, voice_xp_enabled),
                 voice_xp_amount = COALESCE($3, voice_xp_amount)
             WHERE id = $1 RETURNING ",
            columns!()
        );
        Ok(sqlx::query_as::<_, GuildSettings>(sql)
            .bind(sql_id(guild_id))
            .bind(enabled)
            .bind(amount)
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
    use crate::admin::guilds::*;

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
