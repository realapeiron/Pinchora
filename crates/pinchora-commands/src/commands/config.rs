use pinchora_core::{
    card_design::CardKind,
    i18n::Lang,
    leveling::LevelUpMode,
    settings::{self, MessageKind},
    t,
};
use pinchora_db::GuildSettings;
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
    card_images::{CardSubject, card_png, server_card},
    commands::{
        collection::settings::{ConfigCritters, critters_summary},
        language::ConfigLanguage,
        level_roles::ConfigLevelRoles,
        modules::ConfigModules,
        party::ConfigGambling,
        party_games::{ConfigPartyGames, party_summary},
        points::ConfigPoints,
        prefix::{ConfigPrefix, prefix_summary},
        rep_roles::ConfigRepRoles,
        store::ConfigStore,
        suggest::{ConfigSuggestions, suggestions_summary},
        surge::ConfigSurge,
        utilities::{
            ConfigGoodbye, ConfigJoinRole, ConfigLog, ConfigLogEvents, goodbye_summary, join_role_summary, log_summary,
        },
        xp::{ConfigXp, voice_summary},
    },
    events::default_message,
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
    #[command(name = "critters")]
    Critters(ConfigCritters),
    #[command(name = "gambling")]
    Gambling(ConfigGambling),
    #[command(name = "surge")]
    Surge(ConfigSurge),
    #[command(name = "partygames")]
    PartyGames(ConfigPartyGames),
    #[command(name = "modules")]
    Modules(ConfigModules),
    #[command(name = "language")]
    Language(ConfigLanguage),
    #[command(name = "prefix")]
    Prefix(ConfigPrefix),
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

impl ConfigWelcomeCard {
    /// With any option, the classic card in the chosen theme, headline and accent becomes the server's welcome
    /// image (a design made in the dashboard's editor is put aside); without one, the image as it is now.
    async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let accent = match self.color.as_deref().map(settings::check_accent) {
            Some(Ok(rgb)) => Some(rgb as i32),
            Some(Err(error)) => return cx.reply_ephemeral(error.text(cx.lang())).await,
            None => None,
        };
        let title = match self.title.as_deref().map(settings::check_welcome_title) {
            Some(Ok(title)) => Some(title),
            Some(Err(error)) => return cx.reply_ephemeral(error.text(cx.lang())).await,
            None => None,
        };
        let changed = self.theme.is_some() || title.is_some() || accent.is_some();
        let (stored, replaced) = if changed {
            let stored = cx
                .app
                .db
                .update_welcome_card(guild, self.theme.map(|t| t.value()), title.as_deref(), accent)
                .await?;
            let replaced = cx.app.db.clear_card_design(guild, CardKind::Welcome).await?;
            cx.app.forget_guild_settings(guild).await;
            (stored, replaced)
        } else {
            (cx.app.db.guild_settings(guild).await?, false)
        };
        let card = server_card(&cx.app, &stored, guild, CardKind::Welcome).await?;
        let author = cx.author()?.clone();
        // The card speaks the server's language, as it will for members.
        let card_lang = cx.app.guild_lang(guild).await;
        let subject = CardSubject {
            user: author.id,
            avatar: author.avatar,
            name: cx.author_display_name(),
            username: author.name.clone(),
            server: t!(card_lang, "cfg.your-server"),
            members: Some(PREVIEW_MEMBERS),
        };
        let png = card_png(&cx.app, guild, &card.design, &subject).await?;
        let lang = cx.lang();
        let mut about = t!(lang, "welcome.preview-about");
        if replaced {
            about.push_str("\n\n");
            about.push_str(&t!(lang, "welcome.preview-replaced"));
        } else if card.custom {
            about.push_str("\n\n");
            about.push_str(&t!(lang, "welcome.preview-custom"));
        }
        let embed = EmbedBuilder::new()
            .color(BRAND)
            .title(t!(lang, "welcome.preview-title"))
            .description(about);
        cx.reply(Cx::card(embed, "welcome.png", png, Vec::new(), true)).await
    }
}

/// The member number a preview shows.
const PREVIEW_MEMBERS: u64 = 1234;

impl ConfigCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return cx.reply_ephemeral(t!(cx.lang(), "config.guild-only")).await;
        };
        // A server can override who sees `/config` in Discord's integration settings, so the
        // permission is checked here too, for every subcommand.
        if !cx.can_manage_guild() {
            return cx.reply_ephemeral(t!(cx.lang(), "config.needs-manage-server")).await;
        }
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
            Self::Critters(c) => return c.run(cx, guild).await,
            Self::Gambling(c) => return c.run(cx, guild).await,
            Self::Surge(c) => return c.run(cx, guild).await,
            Self::PartyGames(c) => return c.run(cx, guild).await,
            Self::Modules(c) => return c.run(cx, guild).await,
            Self::Language(c) => return c.run(cx, guild).await,
            Self::Prefix(c) => return c.run(cx, guild).await,
            Self::Points(c) => match c.apply(&cx.app.db, guild, cx.lang()).await? {
                Ok(settings) => settings,
                Err(reason) => return cx.reply_ephemeral(reason).await,
            },
        };
        // Leveling on or off (and the rest of these settings) counts on every pod at once, voice XP included.
        cx.app.announce_guild_change(guild).await;
        let lang = cx.lang();
        let mut embed = settings_embed(lang, &settings);
        embed.fields.push(
            EmbedFieldBuilder::new(
                t!(lang, "cfg.suggestions"),
                suggestions_summary(lang, cx.app.db.suggestion_channel(guild).await?),
            )
            .build(),
        );
        embed.fields.push(
            EmbedFieldBuilder::new(
                t!(lang, "cfg.critter-drops"),
                critters_summary(&cx.app.db.collection_settings(guild).await?, lang),
            )
            .build(),
        );
        embed.fields.push(
            EmbedFieldBuilder::new(
                t!(lang, "cfg.party-games"),
                party_summary(lang, cx.app.db.party_strict(guild).await?),
            )
            .build(),
        );
        let language = match cx.app.guild_lang_choice(guild).await {
            Some(lang) => lang.native_name().to_owned(),
            None => t!(cx.lang(), "config.language-default"),
        };
        embed
            .fields
            .push(EmbedFieldBuilder::new(t!(cx.lang(), "config.language-field"), language).build());
        embed.fields.push(
            EmbedFieldBuilder::new(
                t!(cx.lang(), "text.config-field"),
                prefix_summary(settings.prefix(), cx.app.message_content, cx.lang()),
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
fn levelup_summary(lang: Lang, s: &GuildSettings) -> String {
    match LevelUpMode::parse(&s.levelup_mode) {
        LevelUpMode::Off => t!(lang, "cfg.levelup-off"),
        LevelUpMode::Dm => t!(lang, "cfg.levelup-dm"),
        LevelUpMode::Current => t!(lang, "cfg.levelup-current"),
        LevelUpMode::Channel => match s.levelup_channel_id {
            Some(id) => t!(lang, "cfg.levelup-channel", channel = format!("<#{id}>")),
            None => t!(lang, "cfg.levelup-current"),
        },
    }
}

fn settings_embed(lang: Lang, s: &GuildSettings) -> Embed {
    let on_off = |b: bool| t!(lang, if b { "cfg.on" } else { "cfg.off" });
    let channel = |c: Option<i64>| c.map_or_else(|| t!(lang, "cfg.not-set"), |id| format!("<#{id}>"));
    EmbedBuilder::new()
        .color(BRAND)
        .title(t!(lang, "cfg.title"))
        .field(EmbedFieldBuilder::new(
            t!(lang, "cfg.welcome-cards"),
            t!(
                lang,
                "cfg.welcome-value",
                state = on_off(s.welcome_enabled),
                channel = channel(s.welcome_channel_id),
                message = s
                    .welcome_message
                    .clone()
                    .unwrap_or_else(|| default_message(lang, MessageKind::Welcome))
            ),
        ))
        .field(EmbedFieldBuilder::new(
            t!(lang, "cfg.leveling"),
            t!(
                lang,
                "cfg.leveling-value",
                state = on_off(s.leveling_enabled),
                summary = levelup_summary(lang, s),
                message = s
                    .levelup_message
                    .clone()
                    .unwrap_or_else(|| default_message(lang, MessageKind::LevelUp))
            ),
        ))
        .field(EmbedFieldBuilder::new(
            t!(lang, "cfg.xp-rules"),
            t!(lang, "cfg.xp-value", voice = voice_summary(lang, s)),
        ))
        .field(EmbedFieldBuilder::new(
            t!(lang, "cfg.level-roles"),
            t!(
                lang,
                "cfg.level-roles-value",
                mode = t!(
                    lang,
                    if s.level_roles_stack {
                        "cfg.stack"
                    } else {
                        "cfg.replace"
                    }
                )
            ),
        ))
        .field(EmbedFieldBuilder::new(
            t!(lang, "cfg.server-points"),
            t!(
                lang,
                "cfg.points-value",
                state = on_off(s.points_enabled),
                emoji = s.points_emoji,
                name = s.points_name,
                min = s.points_min,
                max = s.points_max
            ),
        ))
        .field(EmbedFieldBuilder::new(
            t!(lang, "cfg.join-role"),
            join_role_summary(lang, s),
        ))
        .field(EmbedFieldBuilder::new(
            t!(lang, "cfg.goodbye"),
            goodbye_summary(lang, s),
        ))
        .field(EmbedFieldBuilder::new(t!(lang, "cfg.server-log"), log_summary(lang, s)))
        .build()
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use pinchora_core::leveling::{MAX_MULTIPLIER, VOICE_XP_RANGE};
    use pinchora_render::Theme;
    use twilight_model::application::command::{CommandOption, CommandOptionChoiceValue, CommandOptionValue};

    use super::*;

    fn find<'a>(options: &'a [CommandOption], path: &[&str]) -> &'a CommandOption {
        let (first, rest) = path.split_first().expect("a path");
        let found = options
            .iter()
            .find(|o| o.name == *first)
            .unwrap_or_else(|| panic!("no option {first}"));
        if rest.is_empty() {
            found
        } else {
            find(found.options.as_deref().unwrap_or_default(), rest)
        }
    }

    fn integer(value: Option<CommandOptionValue>) -> Option<i64> {
        match value {
            Some(CommandOptionValue::Integer(n)) => Some(n),
            _ => None,
        }
    }

    /// The `max_length`, `min_value` and `max_value` on the options only shape Discord's form; the
    /// rules themselves live in `pinchora_core::settings`, and the two must agree.
    #[test]
    fn discord_limits_match_the_shared_rules() {
        let command = ConfigCommand::create_command();
        let option = |path: &[&str]| find(&command.options, path).clone();
        for path in [["welcome", "message"], ["leveling", "message"], ["goodbye", "message"]] {
            assert_eq!(
                option(&path).max_length.map(usize::from),
                Some(settings::MESSAGE_MAX),
                "{path:?}"
            );
        }
        assert_eq!(
            option(&["welcome-card", "title"]).max_length.map(usize::from),
            Some(settings::WELCOME_TITLE_MAX)
        );
        assert_eq!(
            option(&["points", "name"]).max_length.map(usize::from),
            Some(settings::POINTS_NAME_MAX)
        );
        assert_eq!(
            option(&["points", "emoji"]).max_length.map(usize::from),
            Some(settings::POINTS_EMOJI_MAX)
        );
        let range = |path: &[&str], expected: &RangeInclusive<i64>| {
            let o = option(path);
            assert_eq!(
                (integer(o.min_value), integer(o.max_value)),
                (Some(*expected.start()), Some(*expected.end())),
                "{path:?}"
            );
        };
        range(&["points", "min"], &settings::POINTS_RANGE);
        range(&["points", "max"], &settings::POINTS_RANGE);
        range(&["level-roles", "add", "level"], &settings::LEVEL_RANGE);
        range(&["rep-roles", "add", "reputation"], &settings::REPUTATION_RANGE);
        range(&["store", "add", "price"], &settings::PRICE_RANGE);
        range(&["store", "add", "stock"], &settings::STOCK_RANGE);
        range(&["xp", "voice", "amount"], &VOICE_XP_RANGE);
        assert!(matches!(
            option(&["xp", "multiplier", "value"]).max_value,
            Some(CommandOptionValue::Number(m)) if m == MAX_MULTIPLIER
        ));
        let themes: Vec<(String, String)> = option(&["welcome-card", "theme"])
            .choices
            .unwrap_or_default()
            .into_iter()
            .map(|c| match c.value {
                CommandOptionChoiceValue::String(key) => (key, c.name),
                other => panic!("{other:?}"),
            })
            .collect();
        let shared: Vec<(String, String)> = settings::WELCOME_THEMES
            .iter()
            .map(|(k, n)| ((*k).to_owned(), (*n).to_owned()))
            .collect();
        assert_eq!(themes, shared);
        assert_eq!(
            Theme::PRESETS.to_vec(),
            settings::WELCOME_THEMES.map(|(k, _)| k).to_vec()
        );
    }
}
