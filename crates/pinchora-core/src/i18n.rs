/// A language the bot speaks. `En` is the default and the fallback for missing keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lang {
    #[default]
    En,
    Tr,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Tr];

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Tr => "tr",
        }
    }

    /// Discord's locale string ("tr", "en-US", "en-GB") → our language.
    pub fn from_discord(locale: &str) -> Lang {
        if locale.starts_with("tr") { Lang::Tr } else { Lang::En }
    }
}
