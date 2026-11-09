//! Module switches per server and per channel (`guild_modules`), and the `guilds` columns that hold
//! the on/off switch of the modules that already had one.

use pinchora_core::admin::modules::ChannelMode;
use pinchora_core::admin::modules::GuildModules;
use pinchora_core::admin::modules::Module;
use pinchora_core::admin::modules::ModuleRule;

use crate::{Db, sql_id};

#[derive(sqlx::FromRow)]
struct ModuleRow {
    module: String,
    enabled: bool,
    channel_mode: String,
    channels: Vec<i64>,
}

impl ModuleRow {
    fn rule(self) -> Option<(Module, ModuleRule)> {
        let module = Module::parse(&self.module)?;
        Some((
            module,
            ModuleRule {
                enabled: self.enabled,
                mode: ChannelMode::parse(&self.channel_mode).unwrap_or_default(),
                channels: self.channels.into_iter().map(|id| id as u64).collect(),
            },
        ))
    }
}

/// The SQL that writes and reads the settings column holding `module`'s switch.
fn setting_sql(module: Module) -> Option<(&'static str, &'static str)> {
    Some(match module {
        Module::Leveling => (
            "UPDATE guilds SET leveling_enabled = $2 WHERE id = $1",
            "SELECT leveling_enabled FROM guilds WHERE id = $1",
        ),
        Module::Points => (
            "UPDATE guilds SET points_enabled = $2 WHERE id = $1",
            "SELECT points_enabled FROM guilds WHERE id = $1",
        ),
        Module::Welcome => (
            "UPDATE guilds SET welcome_enabled = $2 WHERE id = $1",
            "SELECT welcome_enabled FROM guilds WHERE id = $1",
        ),
        Module::Gambling => (
            "UPDATE guilds SET gambling_enabled = $2 WHERE id = $1",
            "SELECT gambling_enabled FROM guilds WHERE id = $1",
        ),
        _ => return None,
    })
}

impl Db {
    /// The rules a server set for its modules (what the bot enforces). Modules it never touched are
    /// on everywhere; the switches kept in `guilds` columns are not in here.
    pub async fn guild_modules(&self, guild: u64) -> anyhow::Result<GuildModules> {
        let rows = sqlx::query_as::<_, ModuleRow>(
            "SELECT module, enabled, channel_mode, channels FROM guild_modules WHERE guild_id = $1",
        )
        .bind(sql_id(guild))
        .fetch_all(self.pool())
        .await?;
        Ok(GuildModules::new(rows.into_iter().filter_map(ModuleRow::rule)))
    }

    /// Every module as the server has it, in [`Module::ALL`] order: the channel rules from
    /// `guild_modules`, and the on/off of leveling, points, welcome and gambling from `guilds`.
    pub async fn module_states(&self, guild: u64) -> anyhow::Result<Vec<(Module, ModuleRule)>> {
        let rules = self.guild_modules(guild).await?;
        let settings = self.guild_settings(guild).await?;
        let gambling = self.gambling_enabled(guild).await?;
        Ok(Module::ALL
            .into_iter()
            .map(|module| {
                let mut rule = rules.rule(module);
                match module {
                    Module::Leveling => rule.enabled = settings.leveling_enabled,
                    Module::Points => rule.enabled = settings.points_enabled,
                    Module::Welcome => rule.enabled = settings.welcome_enabled,
                    Module::Gambling => rule.enabled = gambling,
                    _ => {}
                }
                (module, rule)
            })
            .collect())
    }

    /// Changes one module: its switch when `enabled` is given, and where it works when `channels`
    /// is (a mode and the list, already checked by `modules::check_channels`). One transaction, so
    /// the switch in a `guilds` column and the channel row never disagree. A module back to "on
    /// everywhere" loses its row. Returns the module as it stands now.
    pub async fn set_module(
        &self,
        guild: u64,
        module: Module,
        enabled: Option<bool>,
        channels: Option<(ChannelMode, &[u64])>,
    ) -> anyhow::Result<ModuleRule> {
        if module == Module::CommandControl {
            return Ok(ModuleRule::default());
        }
        let id = sql_id(guild);
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO guilds (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let row: Option<ModuleRow> = sqlx::query_as(
            "SELECT module, enabled, channel_mode, channels FROM guild_modules
             WHERE guild_id = $1 AND module = $2 FOR UPDATE",
        )
        .bind(id)
        .bind(module.key())
        .fetch_optional(&mut *tx)
        .await?;
        let mut rule = row.and_then(ModuleRow::rule).map(|(_, rule)| rule).unwrap_or_default();
        if let Some((mode, list)) = channels {
            rule.mode = mode;
            rule.channels = list.to_vec();
        }
        let column = setting_sql(module);
        match (column, enabled) {
            (Some((set, _)), Some(on)) => {
                sqlx::query(set).bind(id).bind(on).execute(&mut *tx).await?;
            }
            (None, Some(on)) => rule.enabled = on,
            (_, None) => {}
        }
        if module == Module::Invites && enabled.is_some() {
            sqlx::query(
                "INSERT INTO invite_settings(guild_id,baseline_required) VALUES($1,TRUE) \
                ON CONFLICT(guild_id) DO UPDATE SET baseline_required=TRUE",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        // A switch in a `guilds` column is the only one: the row never says.
        let stored = if column.is_some() { true } else { rule.enabled };
        if stored && rule.mode == ChannelMode::All {
            sqlx::query("DELETE FROM guild_modules WHERE guild_id = $1 AND module = $2")
                .bind(id)
                .bind(module.key())
                .execute(&mut *tx)
                .await?;
        } else {
            let list: Vec<i64> = rule.channels.iter().map(|c| sql_id(*c)).collect();
            sqlx::query(
                "INSERT INTO guild_modules (guild_id, module, enabled, channel_mode, channels)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (guild_id, module) DO UPDATE
                 SET enabled = EXCLUDED.enabled, channel_mode = EXCLUDED.channel_mode, channels = EXCLUDED.channels",
            )
            .bind(id)
            .bind(module.key())
            .bind(stored)
            .bind(rule.mode.key())
            .bind(&list)
            .execute(&mut *tx)
            .await?;
        }
        if let Some((_, get)) = column {
            rule.enabled = sqlx::query_scalar(get).bind(id).fetch_one(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(rule)
    }
}
