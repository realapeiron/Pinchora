use pinchora_db::GuildSettings;
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{Embed, MessageFlags},
    guild::Permissions,
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

use crate::{
    Cx,
    events::{DEFAULT_LEVELUP, DEFAULT_WELCOME},
    util::BRAND,
};

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "config",
    desc = "Pinchora settings for this server",
    default_permissions = "manage_guild",
    contexts = "guild"
)]
pub enum ConfigCommand {
    #[command(name = "welcome")]
    Welcome(ConfigWelcome),
    #[command(name = "leveling")]
    Leveling(ConfigLeveling),
    #[command(name = "show")]
    Show(ConfigShow),
}

impl ConfigCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral("Settings only exist inside a server.").await;
        };
        let settings = match self {
            Self::Welcome(c) => {
                cx.app
                    .db
                    .update_welcome(guild, c.enabled, c.channel.map(|ch| ch.id.get()), c.message.as_deref())
                    .await?
            }
            Self::Leveling(c) => {
                cx.app
                    .db
                    .update_leveling(guild, c.enabled, c.channel.map(|ch| ch.id.get()), c.message.as_deref())
                    .await?
            }
            Self::Show(_) => cx.app.db.guild_settings(guild).await?,
        };
        cx.app.forget_guild_settings(guild).await;
        cx.reply(InteractionResponseData {
            embeds: Some(vec![settings_embed(&settings)]),
            flags: Some(MessageFlags::EPHEMERAL),
            ..Default::default()
        })
        .await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "welcome", desc = "Welcome cards for new members")]
pub struct ConfigWelcome {
    /// Send a welcome card when someone joins
    enabled: Option<bool>,
    /// Channel for welcome cards
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Text above the card. Placeholders: {user} {server} {count}
    #[command(max_length = 500)]
    message: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "leveling", desc = "XP and level-up messages")]
pub struct ConfigLeveling {
    /// Give XP for chatting
    enabled: Option<bool>,
    /// Post level-ups here instead of where the member chatted
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Level-up text. Placeholders: {user} {level}
    #[command(max_length = 500)]
    message: Option<String>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "show", desc = "Show the current settings")]
pub struct ConfigShow;

fn settings_embed(s: &GuildSettings) -> Embed {
    let on_off = |b: bool| if b { "on" } else { "off" };
    let channel = |c: Option<i64>| c.map_or_else(|| "not set".to_owned(), |id| format!("<#{id}>"));
    EmbedBuilder::new()
        .color(BRAND)
        .title("Server settings")
        .field(EmbedFieldBuilder::new(
            "Welcome cards",
            format!(
                "**{}** · {}\n> {}",
                on_off(s.welcome_enabled),
                channel(s.welcome_channel_id),
                s.welcome_message.as_deref().unwrap_or(DEFAULT_WELCOME)
            ),
        ))
        .field(EmbedFieldBuilder::new(
            "Leveling",
            format!(
                "**{}** · level-ups in {}\n> {}",
                on_off(s.leveling_enabled),
                s.levelup_channel_id
                    .map_or_else(|| "the same channel".to_owned(), |id| format!("<#{id}>")),
                s.levelup_message.as_deref().unwrap_or(DEFAULT_LEVELUP)
            ),
        ))
        .build()
}
