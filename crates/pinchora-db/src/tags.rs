//! Custom commands and auto-responders (`custom_commands`, `custom_responders`). The rows hold what a
//! server wrote as `pinchora_core::tags` types; the checks that matter are made there, before a write.

use chrono::{DateTime, Utc};
use pinchora_core::tags::{
    Action, CommandDef, Destination, EmbedSpec, MAX_COMMANDS, MAX_RESPONDERS, MatchKind, ReplyMode, ResponderDef,
    TriggerKind,
};

use crate::{Db, sql_id};

/// What a write to one of the tables came to.
#[derive(Debug, Clone, PartialEq)]
pub enum TagWrite<T> {
    Saved(T),
    /// Another command of the server already has that name.
    NameTaken,
    /// The server has as many as it may.
    Full,
    /// The row is not there (any more).
    Gone,
}

/// A custom command with what the database adds to what the server wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandRow {
    pub id: i64,
    pub guild_id: i64,
    pub def: CommandDef,
    pub uses: i64,
    pub created_by: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An auto-responder with what the database adds to what the server wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponderRow {
    pub id: i64,
    pub guild_id: i64,
    pub def: ResponderDef,
    pub created_by: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Everything a server wrote, for the bot to keep for a minute.
#[derive(Debug, Clone, Default)]
pub struct TagSet {
    pub commands: Vec<CommandRow>,
    pub responders: Vec<ResponderRow>,
}

macro_rules! command_columns {
    () => {
        "id, guild_id, name, description, responses, embed::text AS embed, reply_mode, actions::text AS actions,
         allow_roles, deny_roles, allow_channels, deny_channels, cooldown_secs, enabled, allow_mentions, uses,
         created_by, created_at, updated_at"
    };
}

macro_rules! responder_columns {
    () => {
        "id, guild_id, name, trigger_kind, trigger_value, match_kind, responses, embed::text AS embed, destination,
         channel_id, actions::text AS actions, chance, cooldown_secs, allow_channels, deny_channels, enabled,
         allow_mentions, created_by, created_at, updated_at"
    };
}

#[derive(sqlx::FromRow)]
struct RawCommand {
    id: i64,
    guild_id: i64,
    name: String,
    description: String,
    responses: Vec<String>,
    embed: Option<String>,
    reply_mode: String,
    actions: String,
    allow_roles: Vec<i64>,
    deny_roles: Vec<i64>,
    allow_channels: Vec<i64>,
    deny_channels: Vec<i64>,
    cooldown_secs: i32,
    enabled: bool,
    allow_mentions: bool,
    uses: i64,
    created_by: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct RawResponder {
    id: i64,
    guild_id: i64,
    name: String,
    trigger_kind: String,
    trigger_value: Option<String>,
    match_kind: Option<String>,
    responses: Vec<String>,
    embed: Option<String>,
    destination: String,
    channel_id: Option<i64>,
    actions: String,
    chance: i16,
    cooldown_secs: i32,
    allow_channels: Vec<i64>,
    deny_channels: Vec<i64>,
    enabled: bool,
    allow_mentions: bool,
    created_by: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn ids(list: Vec<i64>) -> Vec<u64> {
    list.into_iter().map(|id| id as u64).collect()
}

fn sql_ids(list: &[u64]) -> Vec<i64> {
    list.iter().map(|id| sql_id(*id)).collect()
}

fn embed_of(raw: Option<String>) -> anyhow::Result<Option<EmbedSpec>> {
    Ok(raw.map(|text| serde_json::from_str(&text)).transpose()?)
}

impl RawCommand {
    fn row(self) -> anyhow::Result<CommandRow> {
        Ok(CommandRow {
            id: self.id,
            guild_id: self.guild_id,
            def: CommandDef {
                name: self.name,
                description: self.description,
                responses: self.responses,
                embed: embed_of(self.embed)?,
                reply_mode: ReplyMode::parse(&self.reply_mode).unwrap_or_default(),
                actions: serde_json::from_str::<Vec<Action>>(&self.actions)?,
                allow_roles: ids(self.allow_roles),
                deny_roles: ids(self.deny_roles),
                allow_channels: ids(self.allow_channels),
                deny_channels: ids(self.deny_channels),
                cooldown_secs: i64::from(self.cooldown_secs),
                enabled: self.enabled,
                allow_mentions: self.allow_mentions,
            },
            uses: self.uses,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

impl RawResponder {
    fn row(self) -> anyhow::Result<ResponderRow> {
        Ok(ResponderRow {
            id: self.id,
            guild_id: self.guild_id,
            def: ResponderDef {
                name: self.name,
                trigger_kind: TriggerKind::parse(&self.trigger_kind)
                    .ok_or_else(|| anyhow::anyhow!("unknown trigger `{}`", self.trigger_kind))?,
                trigger_value: self.trigger_value,
                match_kind: self.match_kind.as_deref().and_then(MatchKind::parse),
                responses: self.responses,
                embed: embed_of(self.embed)?,
                destination: Destination::parse(&self.destination).unwrap_or_default(),
                channel_id: self.channel_id.map(|id| id as u64),
                actions: serde_json::from_str::<Vec<Action>>(&self.actions)?,
                chance: i64::from(self.chance),
                cooldown_secs: i64::from(self.cooldown_secs),
                allow_channels: ids(self.allow_channels),
                deny_channels: ids(self.deny_channels),
                enabled: self.enabled,
                allow_mentions: self.allow_mentions,
            },
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

/// Rows that parse; one that does not (written by hand, or by a newer bot) is skipped and logged rather
/// than hiding the server's other commands.
fn parsed<R, T>(raw: Vec<R>, row: impl Fn(R) -> anyhow::Result<T>, what: &str) -> Vec<T> {
    raw.into_iter()
        .filter_map(|r| {
            row(r)
                .map_err(|error| tracing::warn!(%error, what, "a stored row could not be read"))
                .ok()
        })
        .collect()
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error.as_database_error().is_some_and(|e| e.is_unique_violation())
}

/// Takes the server's write lock for the transaction, so two writes cannot both slip under a limit.
async fn lock_guild(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, guild: u64) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('tags:' || $1::text, 0))")
        .bind(sql_id(guild))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn json_or_null(embed: &Option<EmbedSpec>) -> anyhow::Result<Option<String>> {
    Ok(embed.as_ref().map(serde_json::to_string).transpose()?)
}

impl Db {
    /// Everything a server wrote, for the bot's cache: the commands by name, the responders oldest first.
    pub async fn tag_set(&self, guild: u64) -> anyhow::Result<TagSet> {
        Ok(TagSet {
            commands: self.custom_commands(guild).await?,
            responders: self.custom_responders(guild).await?,
        })
    }

    // /Commands

    /// A server's custom commands, by name.
    pub async fn custom_commands(&self, guild: u64) -> anyhow::Result<Vec<CommandRow>> {
        let raw = sqlx::query_as::<_, RawCommand>(concat!(
            "SELECT ",
            command_columns!(),
            " FROM custom_commands WHERE guild_id = $1 ORDER BY name"
        ))
        .bind(sql_id(guild))
        .fetch_all(self.pool())
        .await?;
        Ok(parsed(raw, RawCommand::row, "custom command"))
    }

    pub async fn custom_command(&self, guild: u64, id: i64) -> anyhow::Result<Option<CommandRow>> {
        let raw = sqlx::query_as::<_, RawCommand>(concat!(
            "SELECT ",
            command_columns!(),
            " FROM custom_commands WHERE guild_id = $1 AND id = $2"
        ))
        .bind(sql_id(guild))
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        raw.map(RawCommand::row).transpose()
    }

    /// Adds a command, unless the name is taken or the server has [`MAX_COMMANDS`].
    pub async fn add_custom_command(
        &self,
        guild: u64,
        def: &CommandDef,
        author: u64,
    ) -> anyhow::Result<TagWrite<CommandRow>> {
        let mut tx = self.pool().begin().await?;
        lock_guild(&mut tx, guild).await?;
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM custom_commands WHERE guild_id = $1")
            .bind(sql_id(guild))
            .fetch_one(&mut *tx)
            .await?;
        if count as usize >= MAX_COMMANDS {
            return Ok(TagWrite::Full);
        }
        let inserted = sqlx::query_as::<_, RawCommand>(concat!(
            "INSERT INTO custom_commands (guild_id, name, description, responses, embed, reply_mode, actions,
                 allow_roles, deny_roles, allow_channels, deny_channels, cooldown_secs, enabled, allow_mentions, created_by)
             VALUES ($1, $2, $3, $4, $5::jsonb, $6, $7::jsonb, $8, $9, $10, $11, $12, $13, $14, $15)
             RETURNING ",
            command_columns!()
        ))
        .bind(sql_id(guild))
        .bind(&def.name)
        .bind(&def.description)
        .bind(&def.responses)
        .bind(json_or_null(&def.embed)?)
        .bind(def.reply_mode.key())
        .bind(serde_json::to_string(&def.actions)?)
        .bind(sql_ids(&def.allow_roles))
        .bind(sql_ids(&def.deny_roles))
        .bind(sql_ids(&def.allow_channels))
        .bind(sql_ids(&def.deny_channels))
        .bind(def.cooldown_secs as i32)
        .bind(def.enabled)
        .bind(def.allow_mentions)
        .bind(sql_id(author))
        .fetch_one(&mut *tx)
        .await;
        match inserted {
            Ok(raw) => {
                tx.commit().await?;
                Ok(TagWrite::Saved(raw.row()?))
            }
            Err(error) if is_unique_violation(&error) => Ok(TagWrite::NameTaken),
            Err(error) => Err(error.into()),
        }
    }

    /// Replaces what a command says, keeping its id, use count and author.
    pub async fn replace_custom_command(
        &self,
        guild: u64,
        id: i64,
        def: &CommandDef,
    ) -> anyhow::Result<TagWrite<CommandRow>> {
        let updated = sqlx::query_as::<_, RawCommand>(concat!(
            "UPDATE custom_commands SET name = $3, description = $4, responses = $5, embed = $6::jsonb,
                 reply_mode = $7, actions = $8::jsonb, allow_roles = $9, deny_roles = $10, allow_channels = $11,
                 deny_channels = $12, cooldown_secs = $13, enabled = $14, allow_mentions = $15, updated_at = now()
             WHERE guild_id = $1 AND id = $2
             RETURNING ",
            command_columns!()
        ))
        .bind(sql_id(guild))
        .bind(id)
        .bind(&def.name)
        .bind(&def.description)
        .bind(&def.responses)
        .bind(json_or_null(&def.embed)?)
        .bind(def.reply_mode.key())
        .bind(serde_json::to_string(&def.actions)?)
        .bind(sql_ids(&def.allow_roles))
        .bind(sql_ids(&def.deny_roles))
        .bind(sql_ids(&def.allow_channels))
        .bind(sql_ids(&def.deny_channels))
        .bind(def.cooldown_secs as i32)
        .bind(def.enabled)
        .bind(def.allow_mentions)
        .fetch_optional(self.pool())
        .await;
        match updated {
            Ok(Some(raw)) => Ok(TagWrite::Saved(raw.row()?)),
            Ok(None) => Ok(TagWrite::Gone),
            Err(error) if is_unique_violation(&error) => Ok(TagWrite::NameTaken),
            Err(error) => Err(error.into()),
        }
    }

    /// Switches a command on or off.
    pub async fn set_custom_command_enabled(
        &self,
        guild: u64,
        id: i64,
        enabled: bool,
    ) -> anyhow::Result<Option<CommandRow>> {
        let raw = sqlx::query_as::<_, RawCommand>(concat!(
            "UPDATE custom_commands SET enabled = $3, updated_at = now() WHERE guild_id = $1 AND id = $2 RETURNING ",
            command_columns!()
        ))
        .bind(sql_id(guild))
        .bind(id)
        .bind(enabled)
        .fetch_optional(self.pool())
        .await?;
        raw.map(RawCommand::row).transpose()
    }

    /// Deletes a command; returns it as it was.
    pub async fn delete_custom_command(&self, guild: u64, id: i64) -> anyhow::Result<Option<CommandRow>> {
        let raw = sqlx::query_as::<_, RawCommand>(concat!(
            "DELETE FROM custom_commands WHERE guild_id = $1 AND id = $2 RETURNING ",
            command_columns!()
        ))
        .bind(sql_id(guild))
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        raw.map(RawCommand::row).transpose()
    }

    /// Counts one run of a command.
    pub async fn count_custom_command_use(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("UPDATE custom_commands SET uses = uses + 1 WHERE id = $1")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    // /Responders

    /// A server's auto-responders, oldest first.
    pub async fn custom_responders(&self, guild: u64) -> anyhow::Result<Vec<ResponderRow>> {
        let raw = sqlx::query_as::<_, RawResponder>(concat!(
            "SELECT ",
            responder_columns!(),
            " FROM custom_responders WHERE guild_id = $1 ORDER BY id"
        ))
        .bind(sql_id(guild))
        .fetch_all(self.pool())
        .await?;
        Ok(parsed(raw, RawResponder::row, "auto-responder"))
    }

    pub async fn custom_responder(&self, guild: u64, id: i64) -> anyhow::Result<Option<ResponderRow>> {
        let raw = sqlx::query_as::<_, RawResponder>(concat!(
            "SELECT ",
            responder_columns!(),
            " FROM custom_responders WHERE guild_id = $1 AND id = $2"
        ))
        .bind(sql_id(guild))
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        raw.map(RawResponder::row).transpose()
    }

    /// Adds a responder, unless the server has [`MAX_RESPONDERS`].
    pub async fn add_custom_responder(
        &self,
        guild: u64,
        def: &ResponderDef,
        author: u64,
    ) -> anyhow::Result<TagWrite<ResponderRow>> {
        let mut tx = self.pool().begin().await?;
        lock_guild(&mut tx, guild).await?;
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM custom_responders WHERE guild_id = $1")
            .bind(sql_id(guild))
            .fetch_one(&mut *tx)
            .await?;
        if count as usize >= MAX_RESPONDERS {
            return Ok(TagWrite::Full);
        }
        let raw = sqlx::query_as::<_, RawResponder>(concat!(
            "INSERT INTO custom_responders (guild_id, name, trigger_kind, trigger_value, match_kind, responses, embed,
                 destination, channel_id, actions, chance, cooldown_secs, allow_channels, deny_channels, enabled,
                 allow_mentions, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9, $10::jsonb, $11, $12, $13, $14, $15, $16, $17)
             RETURNING ",
            responder_columns!()
        ))
        .bind(sql_id(guild))
        .bind(&def.name)
        .bind(def.trigger_kind.key())
        .bind(&def.trigger_value)
        .bind(def.match_kind.map(MatchKind::key))
        .bind(&def.responses)
        .bind(json_or_null(&def.embed)?)
        .bind(def.destination.key())
        .bind(def.channel_id.map(sql_id))
        .bind(serde_json::to_string(&def.actions)?)
        .bind(def.chance as i16)
        .bind(def.cooldown_secs as i32)
        .bind(sql_ids(&def.allow_channels))
        .bind(sql_ids(&def.deny_channels))
        .bind(def.enabled)
        .bind(def.allow_mentions)
        .bind(sql_id(author))
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(TagWrite::Saved(raw.row()?))
    }

    /// Replaces what a responder says and when it fires, keeping its id and author.
    pub async fn replace_custom_responder(
        &self,
        guild: u64,
        id: i64,
        def: &ResponderDef,
    ) -> anyhow::Result<TagWrite<ResponderRow>> {
        let raw = sqlx::query_as::<_, RawResponder>(concat!(
            "UPDATE custom_responders SET name = $3, trigger_kind = $4, trigger_value = $5, match_kind = $6,
                 responses = $7, embed = $8::jsonb, destination = $9, channel_id = $10, actions = $11::jsonb,
                 chance = $12, cooldown_secs = $13, allow_channels = $14, deny_channels = $15, enabled = $16,
                 allow_mentions = $17, updated_at = now()
             WHERE guild_id = $1 AND id = $2
             RETURNING ",
            responder_columns!()
        ))
        .bind(sql_id(guild))
        .bind(id)
        .bind(&def.name)
        .bind(def.trigger_kind.key())
        .bind(&def.trigger_value)
        .bind(def.match_kind.map(MatchKind::key))
        .bind(&def.responses)
        .bind(json_or_null(&def.embed)?)
        .bind(def.destination.key())
        .bind(def.channel_id.map(sql_id))
        .bind(serde_json::to_string(&def.actions)?)
        .bind(def.chance as i16)
        .bind(def.cooldown_secs as i32)
        .bind(sql_ids(&def.allow_channels))
        .bind(sql_ids(&def.deny_channels))
        .bind(def.enabled)
        .bind(def.allow_mentions)
        .fetch_optional(self.pool())
        .await?;
        Ok(match raw {
            Some(raw) => TagWrite::Saved(raw.row()?),
            None => TagWrite::Gone,
        })
    }

    /// Switches a responder on or off.
    pub async fn set_custom_responder_enabled(
        &self,
        guild: u64,
        id: i64,
        enabled: bool,
    ) -> anyhow::Result<Option<ResponderRow>> {
        let raw = sqlx::query_as::<_, RawResponder>(concat!(
            "UPDATE custom_responders SET enabled = $3, updated_at = now() WHERE guild_id = $1 AND id = $2 RETURNING ",
            responder_columns!()
        ))
        .bind(sql_id(guild))
        .bind(id)
        .bind(enabled)
        .fetch_optional(self.pool())
        .await?;
        raw.map(RawResponder::row).transpose()
    }

    /// Deletes a responder; returns it as it was.
    pub async fn delete_custom_responder(&self, guild: u64, id: i64) -> anyhow::Result<Option<ResponderRow>> {
        let raw = sqlx::query_as::<_, RawResponder>(concat!(
            "DELETE FROM custom_responders WHERE guild_id = $1 AND id = $2 RETURNING ",
            responder_columns!()
        ))
        .bind(sql_id(guild))
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        raw.map(RawResponder::row).transpose()
    }
}
