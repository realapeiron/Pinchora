//! Which language the bot answers in, and the command names and descriptions Discord shows in
//! each language.
//!
//! An interaction is answered in, first to last: the language the player picked with `/language`,
//! their Discord client's language, the server's default (`/config language`), English. Messages
//! that answer nobody in particular (welcomes, level-ups, giveaways) use [`App::guild_lang`].

use std::time::Duration;

use moka::future::Cache as Moka;
use pinchora_core::i18n::{self, Lang};
use twilight_model::application::{
    command::{Command, CommandOption, CommandOptionType},
    interaction::Interaction,
};

use crate::App;

/// The language choices read on every interaction, kept a minute like the other settings.
pub(crate) struct Languages {
    users: Moka<u64, Option<Lang>>,
    guilds: Moka<u64, Option<Lang>>,
}

impl Languages {
    pub(crate) fn new() -> Self {
        let cache = || {
            Moka::builder()
                .max_capacity(100_000)
                .time_to_live(Duration::from_secs(60))
                .build()
        };
        Self {
            users: cache(),
            guilds: cache(),
        }
    }
}

impl App {
    /// The language `user` picked with `/language`, if any. A database error counts as none.
    pub async fn user_lang_choice(&self, user: u64) -> Option<Lang> {
        let read = self
            .languages
            .users
            .try_get_with(user, async { self.db.user_language(user).await });
        read.await.unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read a language choice");
            None
        })
    }

    pub async fn set_user_lang(&self, user: u64, lang: Option<Lang>) -> anyhow::Result<()> {
        self.db.set_user_language(user, lang).await?;
        self.languages.users.insert(user, lang).await;
        Ok(())
    }

    /// The server's default language as set with `/config language`, if any.
    pub async fn guild_lang_choice(&self, guild: u64) -> Option<Lang> {
        let read = self
            .languages
            .guilds
            .try_get_with(guild, async { self.db.guild_language(guild).await });
        read.await.unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read a server language");
            None
        })
    }

    /// The language for messages that answer nobody in particular on `guild`.
    pub async fn guild_lang(&self, guild: u64) -> Lang {
        self.guild_lang_choice(guild).await.unwrap_or_default()
    }

    pub async fn set_guild_lang(&self, guild: u64, lang: Option<Lang>) -> anyhow::Result<()> {
        self.db.set_guild_language(guild, lang).await?;
        self.languages.guilds.insert(guild, lang).await;
        Ok(())
    }
}

/// The language `interaction` is answered in (see the module docs for the order).
pub(crate) async fn resolve(app: &App, interaction: &Interaction) -> Lang {
    let user = match interaction.author_id() {
        Some(user) => app.user_lang_choice(user.get()).await,
        None => None,
    };
    let locale = interaction.locale.as_deref();
    // Discord sends a locale with nearly everything, so the server's default is rarely read here.
    let guild = match (user, locale, interaction.guild_id) {
        (None, None, Some(guild)) => app.guild_lang_choice(guild.get()).await,
        _ => None,
    };
    pick(user, locale, guild)
}

/// The language a text command is answered in. A message carries no Discord language, so it is the
/// member's `/language` pick, else the server's default, else English.
pub(crate) async fn resolve_text(app: &App, user: u64, guild: u64) -> Lang {
    let user = app.user_lang_choice(user).await;
    let guild = match user {
        None => app.guild_lang_choice(guild).await,
        Some(_) => None,
    };
    pick(user, None, guild)
}

/// The order of [`resolve`], on what it read.
fn pick(user: Option<Lang>, locale: Option<&str>, guild: Option<Lang>) -> Lang {
    user.or_else(|| locale.map(Lang::from_discord))
        .or(guild)
        .unwrap_or_default()
}

/// The catalog key for a command, subcommand or option description: `cmd.` and its path as typed,
/// e.g. `cmd.house edit room`. A choice adds `=` and its name: `cmd.language set language=English`.
pub(crate) fn command_key(path: &str) -> String {
    format!("cmd.{path}")
}

/// Fills every command's `description_localizations` (and its options' and choices' names) from
/// each language's `commands` catalog. Names stay English: they are what people type and share.
pub(crate) fn localize(commands: &mut [Command]) {
    for command in commands {
        let path = command.name.clone();
        command.description_localizations = descriptions(&path);
        for option in &mut command.options {
            localize_option(option, &path);
        }
    }
}

fn localize_option(option: &mut CommandOption, parent: &str) {
    let path = format!("{parent} {}", option.name);
    option.description_localizations = descriptions(&path);
    if let Some(choices) = option.choices.as_mut() {
        for choice in choices {
            choice.name_localizations = descriptions(&format!("{path}={}", choice.name));
        }
    }
    if matches!(
        option.kind,
        CommandOptionType::SubCommand | CommandOptionType::SubCommandGroup
    ) {
        for sub in option.options.iter_mut().flatten() {
            localize_option(sub, &path);
        }
    }
}

/// `path`'s text in every language that has it, by Discord locale; `None` when none has.
fn descriptions(path: &str) -> Option<std::collections::HashMap<String, String>> {
    let key = command_key(path);
    let found: std::collections::HashMap<String, String> = Lang::ALL
        .into_iter()
        .filter_map(|lang| i18n::exact(lang, &key).map(|text| (lang, text)))
        .flat_map(|(lang, text)| {
            lang.discord_locales()
                .iter()
                .map(move |l| ((*l).to_owned(), text.to_owned()))
        })
        .collect();
    (!found.is_empty()).then_some(found)
}

#[cfg(test)]
/// Every description path the registered commands have, with its English text: the commands,
/// options and choices a language's `commands` catalog translates.
pub(crate) fn command_paths(commands: &[Command]) -> Vec<(String, String)> {
    fn walk(option: &CommandOption, parent: &str, out: &mut Vec<(String, String)>) {
        let path = format!("{parent} {}", option.name);
        out.push((path.clone(), option.description.clone()));
        for twilight_model::application::command::CommandOptionChoice { name, .. } in option.choices.iter().flatten() {
            out.push((format!("{path}={name}"), name.clone()));
        }
        for sub in option.options.iter().flatten() {
            walk(sub, &path, out);
        }
    }
    let mut out = Vec::new();
    for command in commands {
        out.push((command.name.clone(), command.description.clone()));
        for option in &command.options {
            walk(option, &command.name, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn the_players_choice_beats_discord_which_beats_the_server() {
        assert_eq!(pick(Some(Lang::En), Some("tr"), Some(Lang::Tr)), Lang::En);
        assert_eq!(pick(None, Some("tr"), Some(Lang::En)), Lang::Tr);
        assert_eq!(pick(None, Some("en-US"), Some(Lang::Tr)), Lang::En);
        assert_eq!(pick(None, None, Some(Lang::Tr)), Lang::Tr);
        assert_eq!(pick(None, None, None), Lang::En);
    }

    /// The `commands` catalogs name only real paths, and every text fits Discord's 100 characters.
    #[test]
    fn command_translations_name_real_commands_and_fit() {
        let paths: BTreeSet<String> = command_paths(&crate::command_definitions())
            .into_iter()
            .map(|(path, _)| command_key(&path))
            .collect();
        let mut problems = Vec::new();
        for lang in Lang::ALL {
            for key in i18n::keys(lang).into_iter().filter(|k| k.starts_with("cmd.")) {
                if !paths.contains(key) {
                    problems.push(format!("{}: `{key}` is not a command, option or choice", lang.code()));
                }
                let text = i18n::exact(lang, key).unwrap_or_default();
                if text.is_empty() || text.chars().count() > 100 {
                    problems.push(format!("{}: `{key}` must be 1-100 characters", lang.code()));
                }
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    /// Every command, subcommand and option is described in every language. Choices may stay
    /// English: many are content names (fish spots, trip destinations) the content catalogs own.
    #[test]
    fn every_command_is_translated() {
        let mut missing = Vec::new();
        for lang in Lang::ALL.into_iter().filter(|l| *l != Lang::En) {
            for (path, english) in command_paths(&crate::command_definitions()) {
                if !path.contains('=') && i18n::exact(lang, &command_key(&path)).is_none() {
                    missing.push(format!("{}: \"{path}\" = \"{english}\"", lang.code()));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "translate these in locales/<code>/commands.toml:\n{}",
            missing.join("\n")
        );
    }

    /// What registration sends: Turkish descriptions next to the English ones.
    #[test]
    fn registration_json_carries_turkish_descriptions() {
        let json = serde_json::to_value(crate::command_definitions()).expect("commands serialize");
        let profile = json
            .as_array()
            .and_then(|all| all.iter().find(|c| c["name"] == "profile"))
            .expect("/profile is registered");
        assert_eq!(profile["description"], "Show a profile card");
        assert_eq!(
            profile["description_localizations"]["tr"],
            i18n::exact(Lang::Tr, "cmd.profile").expect("translated")
        );
        assert_eq!(
            profile["options"][0]["description_localizations"]["tr"],
            i18n::exact(Lang::Tr, "cmd.profile user").expect("translated")
        );
        assert!(
            profile.get("name_localizations").is_none_or(|n| n.is_null()),
            "names stay English"
        );
    }

    /// Lists every key a language has not translated yet, catalogs and commands alike. Run with
    /// `cargo test -p pinchora-commands untranslated -- --ignored --nocapture`.
    #[test]
    #[ignore = "a report, not a check"]
    fn untranslated() {
        let english: BTreeSet<&str> = i18n::keys(Lang::En).into_iter().collect();
        let commands = command_paths(&crate::command_definitions());
        for lang in Lang::ALL.into_iter().filter(|l| *l != Lang::En) {
            let theirs: BTreeSet<&str> = i18n::keys(lang).into_iter().collect();
            let missing: Vec<String> = english
                .difference(&theirs)
                .map(|k| (*k).to_owned())
                .chain(
                    commands
                        .iter()
                        .filter(|(path, _)| !theirs.contains(command_key(path).as_str()))
                        .map(|(path, text)| format!("\"{path}\" = \"{text}\"")),
                )
                .collect();
            println!("{}: {} untranslated", lang.code(), missing.len());
            for key in missing {
                println!("  {key}");
            }
        }
    }
}
