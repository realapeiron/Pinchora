use std::sync::Arc;

use pinchora_commands::App;

use crate::{config::WebSettings, discord::DiscordApi};

/// What every handler gets: the bot's [`App`], the settings and a Discord client for user tokens.
#[derive(Clone)]
pub struct WebState {
    pub app: Arc<App>,
    pub(crate) reef: Arc<crate::reef::Runtime>,
    pub settings: Arc<WebSettings>,
    pub(crate) csp: axum::http::HeaderValue,
    pub(crate) discord: Arc<DiscordApi>,
}

impl WebState {
    pub fn services(&self) -> &Arc<pinchora_services::Services> {
        &self.app.services
    }

    /// Stop room actors and sockets when a pod drains. Remaining pods acquire a fresh fencing epoch.
    pub fn drain_reef(&self) {
        self.reef.shutdown.send_replace(true);
        // Cancel immediately. Leases expire rather than being transferred by a potentially stalled pod.
        for task in self.reef.tasks.lock().expect("room task registry").drain(..) {
            task.abort();
        }
    }

    pub fn new(app: Arc<App>, settings: WebSettings) -> Self {
        let discord = Arc::new(DiscordApi::new(
            &settings.discord_api_base,
            app.services.infrastructure.oauth_http.clone(),
        ));
        Self {
            app,
            reef: Arc::default(),
            csp: crate::routes::content_security_policy(&settings.origin),
            settings: Arc::new(settings),
            discord,
        }
    }
}
