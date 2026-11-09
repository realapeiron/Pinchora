//! `/config moderation` (the mod-log and direct messages), `/config warn-ladder` and `/config log-ignore`.

use pinchora_core::admin::logging::LogCategory;
use pinchora_core::admin::logging::MAX_IGNORED;
use pinchora_core::admin::server_events::parse_duration;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::moderation::moderation::MAX_THRESHOLDS;
use pinchora_core::moderation::moderation::Threshold;
use pinchora_core::moderation::moderation::check_threshold;
use pinchora_core::t;
use pinchora_db::ModSettings;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption, ResolvedUser};
use twilight_model::{application::interaction::InteractionChannel, guild::Role};

use crate::Cx;
use crate::commands::moderation::words;

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
pub(crate) fn moderation_summary(lang: Lang, settings: &ModSettings, ladder: &[Threshold]) -> String {
    let channel = match settings.log_channel_id {
        Some(c) => format!("<#{c}>"),
        None => t!(lang, "mod.setup.log-moderation-channel"),
    };
    let dm = t!(
        lang,
        if settings.dm_members {
            "mod.setup.on"
        } else {
            "mod.setup.off"
        }
    );
    let ladder = if ladder.is_empty() {
        t!(lang, "mod.setup.none")
    } else {
        ladder
            .iter()
            .map(|r| words::rung(lang, r))
            .collect::<Vec<_>>()
            .join(" · ")
    };
    t!(lang, "mod.setup.summary", channel = channel, dm = dm, ladder = ladder)
}

impl ConfigModeration {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let (db, lang) = (&cx.app.db, cx.lang());
        let channel = match (self.channel, self.off == Some(true)) {
            (Some(_), true) => return cx.reply_ephemeral(t!(lang, "mod.setup.channel-or-off")).await,
            (Some(channel), false) => Some(Some(channel.id.get())),
            (None, true) => Some(None),
            (None, false) => None,
        };
        let settings = if channel.is_some() || self.dm.is_some() {
            db.set_mod_settings(guild, channel, self.dm).await?
        } else {
            db.mod_settings(guild).await?
        };
        let text = moderation_summary(lang, &settings, &db.warn_thresholds(guild).await?);
        cx.reply_ephemeral(text).await
    }
}

impl ConfigWarnLadder {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let (db, lang) = (&cx.app.db, cx.lang());
        let summary = async || -> anyhow::Result<String> {
            Ok(moderation_summary(
                lang,
                &db.mod_settings(guild).await?,
                &db.warn_thresholds(guild).await?,
            ))
        };
        let text = match (self.warns, self.action, self.remove == Some(true)) {
            (Some(warns), None, true) => {
                let gone = db.delete_warn_threshold(guild, warns as i32).await?;
                let what = t!(
                    lang,
                    if gone {
                        "mod.setup.rung-removed"
                    } else {
                        "mod.setup.no-rung"
                    },
                    count = warns
                );
                format!("{what}\n{}", summary().await?)
            }
            (Some(warns), Some(action), false) => {
                let duration = match self.duration.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
                    Some(text) => match parse_duration(text) {
                        Some(d) => Some(d),
                        None => return cx.reply_ephemeral(t!(lang, "mod.setup.duration-format")).await,
                    },
                    None => None,
                };
                let rung = match check_threshold(warns, action.value(), duration) {
                    Ok(rung) => rung,
                    Err(error) => return cx.reply_ephemeral(error.text(lang)).await,
                };
                match db.put_warn_threshold(guild, rung).await? {
                    Ok(_) => {
                        let added = t!(lang, "mod.setup.rung-added", rung = words::rung(lang, &rung));
                        format!("{added}\n{}", summary().await?)
                    }
                    Err(_) => t!(lang, "mod.setup.ladder-full", max = MAX_THRESHOLDS),
                }
            }
            (None, None, false) => summary().await?,
            _ => t!(lang, "mod.setup.ladder-usage"),
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
pub(crate) fn ignore_summary(lang: Lang, config: &pinchora_core::admin::logging::LogConfig) -> String {
    let list = |ids: &[u64], f: fn(u64) -> String| {
        if ids.is_empty() {
            t!(lang, "mod.setup.none")
        } else {
            ids.iter().map(|id| f(*id)).collect::<Vec<_>>().join(" ")
        }
    };
    let bots = t!(
        lang,
        if config.ignore_bots {
            "mod.setup.bots-ignored"
        } else {
            "mod.setup.bots-logged"
        }
    );
    t!(
        lang,
        "mod.setup.ignore-summary",
        channels = list(&config.ignored_channels, |id| format!("<#{id}>")),
        roles = list(&config.ignored_roles, |id| format!("<@&{id}>")),
        members = list(&config.ignored_users, |id| format!("<@{id}>")),
        bots = bots
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
        let mut text = ignore_summary(cx.lang(), &config);
        if full {
            text = format!("{}\n{text}", t!(cx.lang(), "mod.setup.ignore-full", max = MAX_IGNORED));
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
    use crate::commands::moderation::setup::*;

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
