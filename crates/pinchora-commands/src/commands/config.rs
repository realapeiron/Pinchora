use pinchora_core::leveling::LevelUpMode;
use pinchora_db::GuildSettings;
use pinchora_render::{Theme, WelcomeCard};
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};
use twilight_model::{
    application::interaction::InteractionChannel,
    channel::message::{Embed, MessageFlags},
    guild::Permissions,
    http::interaction::InteractionResponseData,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder};

use crate::{
    Cx,
    commands::{
        level_roles::ConfigLevelRoles,
        points::ConfigPoints,
        rep_roles::ConfigRepRoles,
        store::ConfigStore,
        suggest::{ConfigSuggestions, suggestions_summary},
        utilities::{
            ConfigGoodbye, ConfigJoinRole, ConfigLog, ConfigLogEvents, goodbye_summary, join_role_summary, log_summary,
        },
        xp::{ConfigXp, voice_summary},
    },
    events::{DEFAULT_LEVELUP, DEFAULT_WELCOME},
    util::BRAND,
};

fn manage_guild() -> Permissions {
    Permissions::MANAGE_GUILD
}

#[derive(CommandModel, CreateCommand)]
#[allow(clippy::large_enum_variant)] // parsed once per interaction, never stored
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
    #[command(name = "welcome-card")]
    WelcomeCard(ConfigWelcomeCard),
    #[command(name = "level-roles")]
    LevelRoles(ConfigLevelRoles),
    #[command(name = "xp")]
    Xp(ConfigXp),
    #[command(name = "rep-roles")]
    RepRoles(ConfigRepRoles),
    #[command(name = "points")]
    Points(ConfigPoints),
    #[command(name = "store")]
    Store(ConfigStore),
    #[command(name = "join-role")]
    JoinRole(ConfigJoinRole),
    #[command(name = "goodbye")]
    Goodbye(ConfigGoodbye),
    #[command(name = "log")]
    Log(ConfigLog),
    #[command(name = "log-events")]
    LogEvents(ConfigLogEvents),
    #[command(name = "suggestions")]
    Suggestions(ConfigSuggestions),
}

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum CardTheme {
    #[option(name = "Reef (navy & coral)", value = "reef")]
    Reef,
    #[option(name = "Sunset", value = "sunset")]
    Sunset,
    #[option(name = "Forest", value = "forest")]
    Forest,
    #[option(name = "Candy", value = "candy")]
    Candy,
    #[option(name = "Midnight", value = "midnight")]
    Midnight,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "welcome-card", desc = "Design the welcome card and see a preview")]
pub struct ConfigWelcomeCard {
    /// Colour theme
    theme: Option<CardTheme>,
    /// Big headline on the card, e.g. WELCOME or HI THERE
    #[command(max_length = 18)]
    title: Option<String>,
    /// Accent colour as hex, e.g. #ff6b4a
    #[command(max_length = 7)]
    color: Option<String>,
}

fn parse_hex(s: &str) -> Option<i32> {
    let s = s.trim().trim_start_matches('#');
    (s.len() == 6).then(|| i32::from_str_radix(s, 16).ok()).flatten()
}

impl ConfigWelcomeCard {
    async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let accent = match self.color.as_deref() {
            Some(c) => match parse_hex(c) {
                Some(v) => Some(v),
                None => return cx.reply_ephemeral("Colours look like `#ff6b4a`.").await,
            },
            None => None,
        };
        let settings = cx
            .app
            .db
            .update_welcome_card(guild, self.theme.map(|t| t.value()), self.title.as_deref(), accent)
            .await?;
        cx.app.forget_guild_settings(guild).await;
        let author = cx.author()?.clone();
        let name = cx.author_display_name();
        let avatar = cx.app.avatar(&author, WelcomeCard::AVATAR_SIZE, Some(44)).await;
        let title = settings.welcome_title.clone().unwrap_or_else(|| "Welcome".to_owned());
        let theme = Theme::preset(settings.welcome_theme.as_deref().unwrap_or("reef"))
            .with_accent(settings.welcome_accent.map(|c| c as u32));
        let png = cx
            .app
            .render(move |r| {
                r.welcome_card(&WelcomeCard {
                    display_name: &name,
                    avatar_png: &avatar,
                    server_name: "your server",
                    member_number: Some(1234),
                    title: &title,
                    theme,
                })
            })
            .await?;
        let embed = EmbedBuilder::new()
            .color(BRAND)
            .title("Welcome card preview")
            .description("New members will see this card. Turn it on and pick a channel with `/config welcome`.");
        cx.reply(Cx::card(embed, "welcome.png", png, Vec::new(), true)).await
    }
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
                let db = &cx.app.db;
                let mut settings = db
                    .update_leveling(guild, c.enabled, c.channel.map(|ch| ch.id.get()), c.message.as_deref())
                    .await?;
                if let Some(mode) = c.mode {
                    settings = db.set_levelup_mode(guild, mode.value()).await?;
                }
                settings
            }
            Self::Show(_) => cx.app.db.guild_settings(guild).await?,
            Self::WelcomeCard(c) => return c.run(cx, guild).await,
            Self::LevelRoles(c) => return c.run(cx, guild).await,
            Self::Xp(c) => return c.run(cx, guild).await,
            Self::RepRoles(c) => return c.run(cx, guild).await,
            Self::Store(c) => return c.run(cx, guild).await,
            Self::JoinRole(c) => return c.run(cx, guild).await,
            Self::Goodbye(c) => return c.run(cx, guild).await,
            Self::Log(c) => return c.run(cx, guild).await,
            Self::LogEvents(c) => return c.run(cx, guild).await,
            Self::Suggestions(c) => return c.run(cx, guild).await,
            Self::Points(c) => match c.apply(&cx.app.db, guild).await? {
                Ok(settings) => settings,
                Err(reason) => return cx.reply_ephemeral(reason).await,
            },
        };
        cx.app.forget_guild_settings(guild).await;
        let mut embed = settings_embed(&settings);
        embed.fields.push(
            EmbedFieldBuilder::new(
                "Suggestions",
                suggestions_summary(cx.app.db.suggestion_channel(guild).await?),
            )
            .build(),
        );
        cx.reply(InteractionResponseData {
            embeds: Some(vec![embed]),
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

#[derive(CommandOption, CreateOption, Clone, Copy)]
pub enum LevelUpChoice {
    #[option(name = "Channel: the level-up channel, or where they chatted", value = "channel")]
    Channel,
    #[option(name = "Current: always where they chatted", value = "current")]
    Current,
    #[option(name = "DM: a direct message to them", value = "dm")]
    Dm,
    #[option(name = "Off: no level-up message (roles are still given)", value = "off")]
    Off,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "leveling", desc = "XP and level-up messages")]
pub struct ConfigLeveling {
    /// Give XP for chatting
    enabled: Option<bool>,
    /// Post level-ups here instead of where the member chatted
    #[command(channel_types = "guild_text guild_announcement")]
    channel: Option<InteractionChannel>,
    /// Level-up text. {user} {level} {server} {role} (reward role just earned, empty if none)
    #[command(max_length = 500)]
    message: Option<String>,
    /// Where level-ups go (default: channel)
    mode: Option<LevelUpChoice>,
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "show", desc = "Show the current settings")]
pub struct ConfigShow;

/// Where level-ups go, worded to follow "level-ups".
fn levelup_summary(s: &GuildSettings) -> String {
    match LevelUpMode::parse(&s.levelup_mode) {
        LevelUpMode::Off => "are off".to_owned(),
        LevelUpMode::Dm => "arrive by direct message".to_owned(),
        LevelUpMode::Current => "in the channel the member chatted in".to_owned(),
        LevelUpMode::Channel => format!(
            "in {}",
            s.levelup_channel_id.map_or_else(
                || "the channel the member chatted in".to_owned(),
                |id| format!("<#{id}>")
            )
        ),
    }
}

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
                "**{}** · level-ups {}\n> {}",
                on_off(s.leveling_enabled),
                levelup_summary(s),
                s.levelup_message.as_deref().unwrap_or(DEFAULT_LEVELUP)
            ),
        ))
        .field(EmbedFieldBuilder::new(
            "XP rules and voice XP",
            format!("{}\n> Multipliers: `/config xp show`", voice_summary(s)),
        ))
        .field(EmbedFieldBuilder::new(
            "Level roles",
            format!(
                "**{}** · set up with `/config level-roles`",
                if s.level_roles_stack { "stack" } else { "replace" }
            ),
        ))
        .field(EmbedFieldBuilder::new(
            "Server points",
            format!(
                "**{}** · {} **{}** per message, {} to {}\n> Spend them in `/store`; add roles with `/config store`",
                on_off(s.points_enabled),
                s.points_emoji,
                s.points_name,
                s.points_min,
                s.points_max
            ),
        ))
        .field(EmbedFieldBuilder::new("Join role", join_role_summary(s)))
        .field(EmbedFieldBuilder::new("Goodbye", goodbye_summary(s)))
        .field(EmbedFieldBuilder::new("Server log", log_summary(s)))
        .build()
}
