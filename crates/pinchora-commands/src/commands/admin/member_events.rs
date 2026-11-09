//! `/config join-role`, `goodbye`, `log` and `log-events`: what the bot does as members come and go.

use pinchora_core::admin::logging::Group;
use pinchora_core::admin::logging::LogCategory;
use pinchora_core::admin::logging::LogConfig;
use pinchora_core::admin::logging::LogEvent;
use pinchora_core::admin::settings::MessageKind;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::GuildSettings;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{application::interaction::InteractionChannel, guild::Role};

use crate::Cx;
use crate::commands::moderation::setup::LogCategoryChoice;
use crate::commands::moderation::setup::ignore_summary;
use crate::community::events::default_message;
use crate::roles::roles;

fn on_off(lang: Lang, on: bool) -> String {
    t!(lang, if on { "cfg.on" } else { "cfg.off" })
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "join-role", desc = "Give every new member a role")]
pub struct ConfigJoinRole {
    /// The role new members get (bots are skipped)
    role: Option<Role>,
    /// Stop giving new members a role
    off: Option<bool>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "goodbye", desc = "Say goodbye when a member leaves")]
pub struct ConfigGoodbye {
    /// Channel for goodbye messages (this turns them on)
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Text. Placeholders: {user} {server} {members}
    #[command(max_length = 500)]
    message: Option<String>,
    /// Turn goodbye messages off
    off: Option<bool>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "log", desc = "The server log's channel, or one category's")]
pub struct ConfigLog {
    /// Channel for the server log (this turns it on), or for the category you pick
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Turn the server log off, or send the category back to the log channel
    off: Option<bool>,
    /// A category with a channel of its own
    category: Option<LogCategoryChoice>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "log-events", desc = "Pick what the server log reports")]
pub struct ConfigLogEvents {
    /// Members joining and leaving
    members: Option<bool>,
    /// Nickname and username changes
    names: Option<bool>,
    /// Bans and unbans
    bans: Option<bool>,
    /// Voice joins, leaves and moves
    voice: Option<bool>,
    /// Message edits and deletes
    messages: Option<bool>,
    /// Roles made, changed or deleted, and members' roles
    roles: Option<bool>,
    /// Channels and threads made, changed or deleted, permissions included
    channels: Option<bool>,
    /// Bans, kicks and timeouts, with who did them
    moderation: Option<bool>,
    /// Server settings changes
    server: Option<bool>,
    /// Invites made or deleted
    invites: Option<bool>,
    /// Emojis and stickers added, changed or removed
    emojis: Option<bool>,
}

/// How `/config show` and the commands word each setting.
pub(crate) fn join_role_summary(lang: Lang, s: &GuildSettings) -> String {
    match s.join_role_id {
        Some(role) => t!(lang, "cfg.join-role-on", role = format!("<@&{role}>")),
        None => t!(lang, "cfg.join-role-off"),
    }
}

pub(crate) fn goodbye_summary(lang: Lang, s: &GuildSettings) -> String {
    match s.goodbye_channel_id {
        Some(channel) => t!(
            lang,
            "cfg.goodbye-on",
            channel = format!("<#{channel}>"),
            message = s
                .goodbye_message
                .clone()
                .unwrap_or_else(|| default_message(lang, MessageKind::Goodbye))
        ),
        None => t!(lang, "cfg.goodbye-off"),
    }
}

pub(crate) fn log_summary(lang: Lang, s: &GuildSettings) -> String {
    let head = match s.log_channel_id {
        Some(channel) => t!(lang, "cfg.log-on", channel = format!("<#{channel}>")),
        None => t!(lang, "cfg.log-off"),
    };
    t!(
        lang,
        "cfg.log-events",
        head = head,
        members = on_off(lang, s.log_members),
        names = on_off(lang, s.log_names),
        bans = on_off(lang, s.log_bans),
        voice = on_off(lang, s.log_voice)
    )
}

/// The detailed log in a few lines: the channel of each category and how many of its events are on.
pub(crate) fn log_config_summary(lang: Lang, config: &LogConfig) -> String {
    let head = match config.default_channel {
        Some(channel) => t!(lang, "log.head-on", channel = format!("<#{channel}>")),
        None => t!(lang, "log.head-off"),
    };
    let mut lines = vec![head];
    for category in LogCategory::ALL {
        let events: Vec<LogEvent> = category.events().collect();
        let on = events.iter().filter(|e| config.enabled(**e)).count();
        let channel = match (config.channels.get(&category), config.default_channel) {
            (Some(own), _) => format!("<#{own}>"),
            (None, Some(_)) => t!(lang, "log.log-channel"),
            (None, None) => t!(lang, "log.no-channel"),
        };
        let name = t!(lang, &format!("log.category.{}", category.key()));
        lines.push(t!(
            lang,
            "log.category-line",
            category = name,
            on = on,
            total = events.len(),
            channel = channel
        ));
    }
    lines.push(ignore_summary(lang, config));
    lines.join("\n")
}

impl ConfigJoinRole {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        let text = match (self.role, self.off == Some(true)) {
            (Some(_), true) => t!(cx.lang(), "cfg.join-role-both"),
            (Some(role), false) => {
                if let Some(why) = roles::unassignable(cx, guild, &role).await {
                    return cx.reply_ephemeral(why).await;
                }
                db.set_join_role(guild, Some(role.id.get())).await?;
                cx.app.forget_guild_settings(guild).await;
                t!(cx.lang(), "cfg.join-role-set", role = format!("<@&{}>", role.id))
            }
            (None, true) => {
                db.set_join_role(guild, None).await?;
                cx.app.forget_guild_settings(guild).await;
                t!(cx.lang(), "cfg.join-role-cleared")
            }
            (None, false) => t!(
                cx.lang(),
                "cfg.join-role-current",
                summary = join_role_summary(cx.lang(), &db.guild_settings(guild).await?)
            ),
        };
        cx.reply_ephemeral(text).await
    }
}

impl ConfigGoodbye {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        let off = self.off == Some(true);
        let message = self.message.as_deref().map(str::trim).filter(|m| !m.is_empty());
        if off && (self.channel.is_some() || message.is_some()) {
            return cx.reply_ephemeral(t!(cx.lang(), "cfg.goodbye-alone")).await;
        }
        let settings = if off {
            db.clear_goodbye(guild).await?
        } else if self.channel.is_some() || message.is_some() {
            db.update_goodbye(guild, self.channel.map(|c| c.id.get()), message)
                .await?
        } else {
            db.guild_settings(guild).await?
        };
        cx.app.forget_guild_settings(guild).await;
        cx.reply_ephemeral(t!(
            cx.lang(),
            "cfg.goodbye-current",
            summary = goodbye_summary(cx.lang(), &settings)
        ))
        .await
    }
}

impl ConfigLog {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        let off = self.off == Some(true);
        if self.channel.is_some() && off {
            return cx.reply_ephemeral(t!(cx.lang(), "mod.setup.channel-or-off")).await;
        }
        let config = match (self.category.map(LogCategoryChoice::category), self.channel) {
            (Some(category), channel) if channel.is_some() || off => {
                db.update_log_config(guild, |config| match channel {
                    Some(channel) => {
                        config.channels.insert(category, channel.id.get());
                    }
                    None => {
                        config.channels.remove(&category);
                    }
                })
                .await?
            }
            (None, Some(channel)) => {
                db.set_log_channel(guild, Some(channel.id.get())).await?;
                db.log_config(guild).await?
            }
            (None, None) if off => {
                db.set_log_channel(guild, None).await?;
                db.log_config(guild).await?
            }
            _ => db.log_config(guild).await?,
        };
        cx.app.announce_guild_change(guild).await;
        let summary = log_config_summary(cx.lang(), &config);
        cx.reply_ephemeral(t!(cx.lang(), "log.prefix", summary = summary)).await
    }
}

impl ConfigLogEvents {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let groups = [
            (Group::Members, self.members),
            (Group::Names, self.names),
            (Group::Bans, self.bans),
            (Group::Voice, self.voice),
        ];
        let categories = [
            (LogCategory::Messages, self.messages),
            (LogCategory::Roles, self.roles),
            (LogCategory::Channels, self.channels),
            (LogCategory::Moderation, self.moderation),
            (LogCategory::Server, self.server),
            (LogCategory::Invites, self.invites),
            (LogCategory::Emojis, self.emojis),
        ];
        let config = cx
            .app
            .db
            .update_log_config(guild, |config| {
                for (group, on) in groups {
                    if let Some(on) = on {
                        config.set_group(group, on);
                    }
                }
                for (category, on) in categories {
                    if let Some(on) = on {
                        config.toggle_category(category, on);
                    }
                }
            })
            .await?;
        cx.app.announce_guild_change(guild).await;
        let summary = log_config_summary(cx.lang(), &config);
        cx.reply_ephemeral(t!(cx.lang(), "log.prefix", summary = summary)).await
    }
}
