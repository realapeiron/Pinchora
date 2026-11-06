//! `/config join-role`, `goodbye`, `log` and `log-events`: what the bot does as members come and go.

use pinchora_core::{i18n::Lang, settings::MessageKind, t};
use pinchora_db::GuildSettings;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{application::interaction::InteractionChannel, guild::Role};

use crate::{Cx, events::default_message, roles};

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
#[command(name = "log", desc = "Log joins, name changes, bans and voice in a channel")]
pub struct ConfigLog {
    /// Channel for the server log (this turns it on)
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Turn the server log off
    off: Option<bool>,
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
        let settings = match (self.channel, self.off == Some(true)) {
            (Some(_), true) => return cx.reply_ephemeral(t!(cx.lang(), "cfg.log-both")).await,
            (Some(channel), false) => db.set_log_channel(guild, Some(channel.id.get())).await?,
            (None, true) => db.set_log_channel(guild, None).await?,
            (None, false) => db.guild_settings(guild).await?,
        };
        cx.app.forget_guild_settings(guild).await;
        cx.reply_ephemeral(t!(
            cx.lang(),
            "cfg.log-current",
            summary = log_summary(cx.lang(), &settings)
        ))
        .await
    }
}

impl ConfigLogEvents {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        let settings = db
            .update_log_events(guild, self.members, self.names, self.bans, self.voice)
            .await?;
        cx.app.forget_guild_settings(guild).await;
        cx.reply_ephemeral(t!(
            cx.lang(),
            "cfg.log-current",
            summary = log_summary(cx.lang(), &settings)
        ))
        .await
    }
}
