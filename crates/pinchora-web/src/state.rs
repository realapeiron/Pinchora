use std::sync::Arc;

use pinchora_commands::App;

use crate::{config::WebSettings, discord::DiscordApi};

/// What every handler gets: the bot's [`App`], the settings and a Discord client for user tokens.
#[derive(Clone)]
pub struct WebState {
    pub app: Arc<App>,
    pub(crate) reef: Arc<crate::reef::Runtime>,
    pub settings: Arc<WebSettings>,
    pub(crate) discord: Arc<DiscordApi>,
}

impl WebState {
    pub fn new(app: Arc<App>, settings: WebSettings) -> Self {
        let discord = Arc::new(DiscordApi::new(&settings.discord_api_base));
        Self {
            app,
            reef: Arc::default(),
            settings: Arc::new(settings),
            discord,
        }
    }
}
