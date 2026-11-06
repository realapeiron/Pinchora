//! The detailed server log's settings: the channel of each category, the events switched off one by
//! one, and the ignore lists. The server log's own channel and its four older switches stay in `guilds`.

use std::collections::HashMap;

use pinchora_core::logging::{Groups, LogCategory, LogConfig, keys, parse_events};

use crate::{Db, sql_id};

#[derive(sqlx::FromRow)]
struct ConfigRow {
    log_channel_id: Option<i64>,
    log_members: bool,
    log_names: bool,
    log_bans: bool,
    log_voice: bool,
    disabled_events: Option<Vec<String>>,
    ignored_channels: Option<Vec<i64>>,
    ignored_roles: Option<Vec<i64>>,
    ignored_users: Option<Vec<i64>>,
    ignore_bots: Option<bool>,
}

fn ids(list: Option<Vec<i64>>) -> Vec<u64> {
    list.unwrap_or_default().into_iter().map(|id| id as u64).collect()
}

fn sql_ids(list: &[u64]) -> Vec<i64> {
    list.iter().map(|id| sql_id(*id)).collect()
}

impl Db {
    /// Everything that decides whether and where the server log reports an event.
    pub async fn log_config(&self, guild: u64) -> anyhow::Result<LogConfig> {
        self.guild_settings(guild).await?;
        let mut conn = self.pool().acquire().await?;
        read_config(&mut conn, guild).await
    }

    /// Changes the log's settings with `change`, under the server's row lock so two edits at once
    /// never undo each other, and returns them as stored.
    pub async fn update_log_config(
        &self,
        guild: u64,
        change: impl FnOnce(&mut LogConfig),
    ) -> anyhow::Result<LogConfig> {
        self.guild_settings(guild).await?;
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT 1 FROM guilds WHERE id = $1 FOR UPDATE")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        let mut config = read_config(&mut tx, guild).await?;
        change(&mut config);
        let g = &config.groups;
        sqlx::query(
            "UPDATE guilds SET log_channel_id = $2, log_members = $3, log_names = $4, log_bans = $5, log_voice = $6
             WHERE id = $1",
        )
        .bind(sql_id(guild))
        .bind(config.default_channel.map(sql_id))
        .bind(g.members)
        .bind(g.names)
        .bind(g.bans)
        .bind(g.voice)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO log_settings (guild_id, disabled_events, ignored_channels, ignored_roles, ignored_users, ignore_bots)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (guild_id) DO UPDATE SET
                 disabled_events = EXCLUDED.disabled_events, ignored_channels = EXCLUDED.ignored_channels,
                 ignored_roles = EXCLUDED.ignored_roles, ignored_users = EXCLUDED.ignored_users,
                 ignore_bots = EXCLUDED.ignore_bots",
        )
        .bind(sql_id(guild))
        .bind(keys(&config.disabled))
        .bind(sql_ids(&config.ignored_channels))
        .bind(sql_ids(&config.ignored_roles))
        .bind(sql_ids(&config.ignored_users))
        .bind(config.ignore_bots)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM log_channels WHERE guild_id = $1")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        for (category, channel) in &config.channels {
            sqlx::query("INSERT INTO log_channels (guild_id, category, channel_id) VALUES ($1, $2, $3)")
                .bind(sql_id(guild))
                .bind(category.key())
                .bind(sql_id(*channel))
                .execute(&mut *tx)
                .await?;
        }
        let stored = read_config(&mut tx, guild).await?;
        tx.commit().await?;
        Ok(stored)
    }
}

async fn read_config(conn: &mut sqlx::PgConnection, guild: u64) -> anyhow::Result<LogConfig> {
    let row = sqlx::query_as::<_, ConfigRow>(
        "SELECT g.log_channel_id, g.log_members, g.log_names, g.log_bans, g.log_voice, s.disabled_events,
                s.ignored_channels, s.ignored_roles, s.ignored_users, s.ignore_bots
         FROM guilds g LEFT JOIN log_settings s ON s.guild_id = g.id WHERE g.id = $1",
    )
    .bind(sql_id(guild))
    .fetch_one(&mut *conn)
    .await?;
    let channels: Vec<(String, i64)> =
        sqlx::query_as("SELECT category, channel_id FROM log_channels WHERE guild_id = $1")
            .bind(sql_id(guild))
            .fetch_all(&mut *conn)
            .await?;
    let defaults = LogConfig::default();
    Ok(LogConfig {
        default_channel: row.log_channel_id.map(|id| id as u64),
        groups: Groups {
            members: row.log_members,
            names: row.log_names,
            bans: row.log_bans,
            voice: row.log_voice,
        },
        channels: channels
            .into_iter()
            .filter_map(|(key, id)| Some((LogCategory::parse(&key)?, id as u64)))
            .collect::<HashMap<_, _>>(),
        disabled: row
            .disabled_events
            .map_or(defaults.disabled, |keys| parse_events(&keys)),
        ignored_channels: ids(row.ignored_channels),
        ignored_roles: ids(row.ignored_roles),
        ignored_users: ids(row.ignored_users),
        ignore_bots: row.ignore_bots.unwrap_or(false),
    })
}
