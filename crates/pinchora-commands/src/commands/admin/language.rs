//! `/language set`: the language Pinchora answers a player in, and `/config language`: a server's
//! default for messages that answer nobody in particular.

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use twilight_interactions::command::{CommandModel, CommandOption, CreateCommand, CreateOption};

use crate::Cx;

/// A language to pick, or none: follow Discord (for a player) or English (for a server). A new
/// language gets a variant here too.
#[derive(CommandOption, CreateOption, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LangChoice {
    #[option(name = "Automatic", value = "auto")]
    Auto,
    #[option(name = "English", value = "en")]
    En,
    #[option(name = "Türkçe", value = "tr")]
    Tr,
}

impl LangChoice {
    fn lang(self) -> Option<Lang> {
        match self {
            Self::Auto => None,
            Self::En => Some(Lang::En),
            Self::Tr => Some(Lang::Tr),
        }
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "language", desc = "The language Pinchora speaks with you")]
pub enum LanguageCommand {
    #[command(name = "set")]
    Set(LanguageSet),
}

#[derive(CommandModel, CreateCommand)]
#[command(name = "set", desc = "Pick your language, or follow your Discord language")]
pub struct LanguageSet {
    /// The language to use everywhere
    language: LangChoice,
}

impl LanguageCommand {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Self::Set(set) = self;
        let picked = set.language.lang();
        cx.app.set_user_lang(cx.author()?.id.get(), picked).await?;
        // Answered in the language just picked; "automatic" goes back to the client's.
        let now = picked.unwrap_or_else(|| {
            cx.interaction
                .locale
                .as_deref()
                .map(Lang::from_discord)
                .unwrap_or_default()
        });
        let text = match picked {
            Some(lang) => t!(lang, "language.set", language = lang.native_name()),
            None => t!(now, "language.auto", language = now.native_name()),
        };
        cx.reply_ephemeral(text).await
    }
}

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "language",
    desc = "The default language for welcomes, level-ups and other server messages"
)]
pub struct ConfigLanguage {
    /// The server's language (Automatic: English)
    language: LangChoice,
}

impl ConfigLanguage {
    pub async fn run(self, cx: &Cx, guild: u64) -> anyhow::Result<()> {
        let picked = self.language.lang();
        cx.app.set_guild_lang(guild, picked).await?;
        let text = match picked {
            Some(lang) => t!(cx.lang(), "language.server-set", language = lang.native_name()),
            None => t!(cx.lang(), "language.server-auto"),
        };
        cx.reply_ephemeral(text).await
    }
}
