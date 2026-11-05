//! `/config join-role`, `goodbye`, `log` and `log-events`: what the bot does as members come and go.

use pinchora_db::GuildSettings;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{application::interaction::InteractionChannel, guild::Role};

use crate::{Cx, events::DEFAULT_GOODBYE, roles};

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
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
pub(crate) fn join_role_summary(s: &GuildSettings) -> String {
    match s.join_role_id {
        Some(role) => format!("**on** · <@&{role}>"),
        None => "**off** · pick a role with `/config join-role`".to_owned(),
    }
}

pub(crate) fn goodbye_summary(s: &GuildSettings) -> String {
    match s.goodbye_channel_id {
        Some(channel) => format!(
            "**on** · <#{channel}>\n> {}",
            s.goodbye_message.as_deref().unwrap_or(DEFAULT_GOODBYE)
        ),
        None => "**off** · pick a channel with `/config goodbye`".to_owned(),
    }
}

pub(crate) fn log_summary(s: &GuildSettings) -> String {
    let head = match s.log_channel_id {
        Some(channel) => format!("**on** · <#{channel}>"),
        None => "**off** · pick a channel with `/config log`".to_owned(),
    };
    format!(
        "{head}\n> joins and leaves {} · name changes {} · bans {} · voice {}",
        on_off(s.log_members),
        on_off(s.log_names),
        on_off(s.log_bans),
        on_off(s.log_voice)
    )
}

impl ConfigJoinRole {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        let text = match (self.role, self.off == Some(true)) {
            (Some(_), true) => "Pick a role or `off`, not both.".to_owned(),
            (Some(role), false) => {
                if let Some(why) = roles::unassignable(&cx.app, guild, &role).await {
                    return cx.reply_ephemeral(why).await;
                }
                db.set_join_role(guild, Some(role.id.get())).await?;
                cx.app.forget_guild_settings(guild).await;
                format!("New members now get <@&{}> when they join. Bots are skipped.", role.id)
            }
            (None, true) => {
                db.set_join_role(guild, None).await?;
                cx.app.forget_guild_settings(guild).await;
                "New members no longer get a role.".to_owned()
            }
            (None, false) => format!("Join role: {}", join_role_summary(&db.guild_settings(guild).await?)),
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
            return cx.reply_ephemeral("Use `off` on its own.").await;
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
        cx.reply_ephemeral(format!("Goodbye messages: {}", goodbye_summary(&settings)))
            .await
    }
}

impl ConfigLog {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let db = &cx.app.db;
        let settings = match (self.channel, self.off == Some(true)) {
            (Some(_), true) => return cx.reply_ephemeral("Pick a channel or `off`, not both.").await,
            (Some(channel), false) => db.set_log_channel(guild, Some(channel.id.get())).await?,
            (None, true) => db.set_log_channel(guild, None).await?,
            (None, false) => db.guild_settings(guild).await?,
        };
        cx.app.forget_guild_settings(guild).await;
        cx.reply_ephemeral(format!("Server log: {}", log_summary(&settings)))
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
        cx.reply_ephemeral(format!("Server log: {}", log_summary(&settings)))
            .await
    }
}
