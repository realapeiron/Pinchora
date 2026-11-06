//! `/config moderation` (the mod-log and direct messages), `/config warn-ladder` and `/config log-ignore`.

use pinchora_core::{
    logging::{LogCategory, MAX_IGNORED},
    moderation::{MAX_THRESHOLDS, Threshold, check_threshold},
    server_events::parse_duration,
};
use pinchora_db::ModSettings;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption, ResolvedUser};
use twilight_model::{application::interaction::InteractionChannel, guild::Role};

use crate::Cx;

#[derive(CommandModel, CreateCommand)]
#[command(name = "moderation", desc = "Where cases go and whether members are told")]
pub struct ConfigModeration {
    /// The mod-log channel for every case
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// No mod-log channel of its own: cases go to the server log's moderation channel
    off: Option<bool>,
    /// Tell members by direct message when they are warned, timed out, kicked or banned
    dm: Option<bool>,
}

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum LadderAction {
    #[option(name = "Timeout", value = "timeout")]
    Timeout,
    #[option(name = "Kick", value = "kick")]
    Kick,
    #[option(name = "Ban", value = "ban")]
    Ban,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "warn-ladder", desc = "What a number of warns brings")]
pub struct ConfigWarnLadder {
    /// How many active warns
    #[command(min_value = 1, max_value = 50)]
    warns: Option<i64>,
    /// What happens (the same warn count again replaces it)
    action: Option<LadderAction>,
    /// How long, like 1h or 7d (needed for a timeout; a ban without one is for good)
    #[command(max_length = 32)]
    duration: Option<String>,
    /// Take the rung at this warn count off instead
    remove: Option<bool>,
}

/// The settings in a few lines.
pub(crate) fn moderation_summary(settings: &ModSettings, ladder: &[Threshold]) -> String {
    let channel = settings.log_channel_id.map_or_else(
        || "the server log's moderation channel".to_owned(),
        |c| format!("<#{c}>"),
    );
    let dm = if settings.dm_members { "on" } else { "off" };
    let ladder = if ladder.is_empty() {
        "none".to_owned()
    } else {
        ladder.iter().map(Threshold::describe).collect::<Vec<_>>().join(" · ")
    };
    format!("Cases go to {channel} · direct messages **{dm}**\n> Warn ladder: {ladder}")
}

impl ConfigModeration {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        let channel = match (self.channel, self.off == Some(true)) {
            (Some(_), true) => return cx.reply_ephemeral("Pick a channel or `off`, not both.").await,
            (Some(channel), false) => Some(Some(channel.id.get())),
            (None, true) => Some(None),
            (None, false) => None,
        };
        let settings = if channel.is_some() || self.dm.is_some() {
            db.set_mod_settings(guild, channel, self.dm).await?
        } else {
            db.mod_settings(guild).await?
        };
        let text = moderation_summary(&settings, &db.warn_thresholds(guild).await?);
        cx.reply_ephemeral(text).await
    }
}

impl ConfigWarnLadder {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        let summary = async || -> anyhow::Result<String> {
            Ok(moderation_summary(
                &db.mod_settings(guild).await?,
                &db.warn_thresholds(guild).await?,
            ))
        };
        let text = match (self.warns, self.action, self.remove == Some(true)) {
            (Some(warns), None, true) => {
                let gone = db.delete_warn_threshold(guild, warns as i32).await?;
                let what = if gone { "Removed the rung at" } else { "There is no rung at" };
                format!("{what} {warns} warns.\n{}", summary().await?)
            }
            (Some(warns), Some(action), false) => {
                let duration = match self.duration.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
                    Some(text) => match parse_duration(text) {
                        Some(d) => Some(d),
                        None => return cx.reply_ephemeral("Write the duration like `1h`, `12h` or `7d`.").await,
                    },
                    None => None,
                };
                let rung = match check_threshold(warns, action.value(), duration) {
                    Ok(rung) => rung,
                    Err(error) => return cx.reply_ephemeral(error.message).await,
                };
                match db.put_warn_threshold(guild, rung).await? {
                    Ok(_) => format!("Added **{}**.\n{}", rung.describe(), summary().await?),
                    Err(_) => format!("The warn ladder holds at most {MAX_THRESHOLDS} rungs. Remove one first."),
                }
            }
            (None, None, false) => summary().await?,
            _ => {
                "Give `warns` and `action` to add a rung, `warns` and `remove` to take one off, or nothing to see the ladder."
                    .to_owned()
            }
        };
        cx.reply_ephemeral(text).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "log-ignore", desc = "Who and where the server log leaves alone")]
pub struct ConfigLogIgnore {
    /// A channel (or category) to leave alone
    channel: Option<InteractionChannel>,
    /// Members with this role are left alone
    role: Option<Role>,
    /// A member to leave alone
    user: Option<ResolvedUser>,
    /// Leave every bot alone
    bots: Option<bool>,
    /// Take the channel, role or member off the list instead
    remove: Option<bool>,
}

/// The ignore lists in a line.
pub(crate) fn ignore_summary(config: &pinchora_core::logging::LogConfig) -> String {
    let list = |ids: &[u64], f: fn(u64) -> String| {
        if ids.is_empty() {
            "none".to_owned()
        } else {
            ids.iter().map(|id| f(*id)).collect::<Vec<_>>().join(" ")
        }
    };
    format!(
        "Ignored channels: {} · roles: {} · members: {} · bots: **{}**",
        list(&config.ignored_channels, |id| format!("<#{id}>")),
        list(&config.ignored_roles, |id| format!("<@&{id}>")),
        list(&config.ignored_users, |id| format!("<@{id}>")),
        if config.ignore_bots { "ignored" } else { "logged" }
    )
}

impl ConfigLogIgnore {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let remove = self.remove == Some(true);
        let entries = [
            self.channel.map(|c| (0, c.id.get())),
            self.role.map(|r| (1, r.id.get())),
            self.user.map(|u| (2, u.resolved.id.get())),
        ];
        let mut full = false;
        let config = cx
            .app
            .db
            .update_log_config(guild, |config| {
                for (list, id) in entries.into_iter().flatten() {
                    let list = match list {
                        0 => &mut config.ignored_channels,
                        1 => &mut config.ignored_roles,
                        _ => &mut config.ignored_users,
                    };
                    if remove {
                        list.retain(|x| *x != id);
                    } else if list.len() >= MAX_IGNORED {
                        full = true;
                    } else if !list.contains(&id) {
                        list.push(id);
                    }
                }
                if let Some(bots) = self.bots {
                    config.ignore_bots = bots;
                }
            })
            .await?;
        cx.app.announce_guild_change(guild).await;
        let mut text = ignore_summary(&config);
        if full {
            text = format!("An ignore list holds at most {MAX_IGNORED} entries.\n{text}");
        }
        cx.reply_ephemeral(text).await
    }
}

/// A category of the server log, for `/config log`.
#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum LogCategoryChoice {
    #[option(name = "Members", value = "members")]
    Members,
    #[option(name = "Messages", value = "messages")]
    Messages,
    #[option(name = "Roles", value = "roles")]
    Roles,
    #[option(name = "Channels and threads", value = "channels")]
    Channels,
    #[option(name = "Voice", value = "voice")]
    Voice,
    #[option(name = "Moderation", value = "moderation")]
    Moderation,
    #[option(name = "Server", value = "server")]
    Server,
    #[option(name = "Invites", value = "invites")]
    Invites,
    #[option(name = "Emojis and stickers", value = "emojis")]
    Emojis,
}

impl LogCategoryChoice {
    pub(crate) fn category(self) -> LogCategory {
        LogCategory::parse(self.value()).unwrap_or(LogCategory::Members)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_can_be_picked() {
        use LogCategoryChoice as C;
        let picked: Vec<LogCategory> = [
            C::Members,
            C::Messages,
            C::Roles,
            C::Channels,
            C::Voice,
            C::Moderation,
            C::Server,
            C::Invites,
            C::Emojis,
        ]
        .into_iter()
        .map(|c| {
            assert_eq!(LogCategory::parse(c.value()).map(LogCategory::key), Some(c.value()));
            c.category()
        })
        .collect();
        assert_eq!(picked, LogCategory::ALL);
    }
}
