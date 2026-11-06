//! Talking to Discord: OAuth with the user's token, presence checks with the bot's. Both cached in
//! Redis, because Discord rate-limits the user-token routes per user.

use std::time::Duration;

use anyhow::Context as _;
use futures_util::{StreamExt, stream};
use pinchora_core::config::Secret;
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use twilight_http::error::ErrorType;
use twilight_model::id::Id;

use crate::{
    auth::session::{self, Session},
    config::WebSettings,
    error::{ApiError, ApiResult},
    extract::CurrentSession,
    state::WebState,
};

/// Permission bits as Discord sends them in `permissions`.
pub const ADMINISTRATOR: u64 = 1 << 3;
pub const MANAGE_GUILD: u64 = 1 << 5;
/// What the invite link asks for: add reactions, view channels, send messages, embed links, attach
/// files, read message history, use external emojis and manage roles.
const BOT_PERMISSIONS: u64 =
    (1 << 6) | (1 << 10) | (1 << 11) | (1 << 14) | (1 << 15) | (1 << 16) | (1 << 18) | (1 << 28);
/// How long a user's server list and the bot's presence in a server are trusted.
const LIST_TTL: Duration = Duration::from_secs(60);
/// A token this close to expiring is refreshed before use.
const REFRESH_MARGIN_SECS: i64 = 300;
/// One request per session refreshes its token at a time; the lock outlives a slow Discord call.
const REFRESH_LOCK: Duration = Duration::from_secs(15);
/// How long a request waits for another one's refresh before giving up.
const REFRESH_WAIT: Duration = Duration::from_secs(12);

/// Tokens print as `Secret(***)` in any `Debug` output.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: Secret,
    pub refresh_token: Option<Secret>,
    pub expires_in: i64,
}

#[derive(Debug, Deserialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
}

/// A server on the user's list, as `GET /users/@me/guilds` has it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserGuild {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub owner: bool,
    /// The user's permission bits there, as a decimal string.
    #[serde(default)]
    pub permissions: String,
}

impl UserGuild {
    pub fn id(&self) -> Option<u64> {
        self.id.parse().ok()
    }

    /// Owner, Manage Server or Administrator: who may use the admin side.
    pub fn can_manage(&self) -> bool {
        let bits: u64 = self.permissions.parse().unwrap_or(0);
        self.owner || bits & (MANAGE_GUILD | ADMINISTRATOR) != 0
    }
}

/// A failed user-token call: the token is no good, or something else broke.
pub enum CallError {
    Unauthorized,
    Other(anyhow::Error),
}

impl From<reqwest::Error> for CallError {
    fn from(error: reqwest::Error) -> Self {
        Self::Other(error.into())
    }
}

/// Discord's REST API with OAuth tokens (the bot's own calls go through `App::http`).
pub struct DiscordApi {
    http: reqwest::Client,
    base: String,
}

impl DiscordApi {
    pub fn new(base: &str) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("Pinchora dashboard (https://github.com/realapeiron/Pinchora)")
                .build()
                .expect("reqwest client"),
            base: base.to_owned(),
        }
    }

    async fn token(&self, settings: &WebSettings, form: &[(&str, &str)]) -> Result<TokenResponse, CallError> {
        let secret = settings.client_secret.as_ref().map_or("", Secret::expose);
        let client_id = settings.client_id.to_string();
        let mut body = vec![("client_id", client_id.as_str()), ("client_secret", secret)];
        body.extend_from_slice(form);
        let response = self
            .http
            .post(format!("{}/oauth2/token", self.base))
            .form(&body)
            .send()
            .await?;
        match response.status() {
            status if status.is_success() => Ok(response.json().await?),
            StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED => Err(CallError::Unauthorized),
            status => Err(CallError::Other(anyhow::anyhow!("token endpoint answered {status}"))),
        }
    }

    /// Trades the callback's `code` for tokens.
    pub async fn exchange_code(
        &self,
        settings: &WebSettings,
        code: &str,
        verifier: &str,
    ) -> Result<TokenResponse, CallError> {
        self.token(
            settings,
            &[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", &settings.redirect_uri),
                ("code_verifier", verifier),
            ],
        )
        .await
    }

    pub async fn refresh(&self, settings: &WebSettings, refresh_token: &Secret) -> Result<TokenResponse, CallError> {
        self.token(
            settings,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.expose()),
            ],
        )
        .await
    }

    /// Revokes a token on logout. Best effort: a failure is only logged.
    pub async fn revoke(&self, settings: &WebSettings, token: &Secret) {
        let secret = settings.client_secret.as_ref().map_or("", Secret::expose);
        let client_id = settings.client_id.to_string();
        let result = self
            .http
            .post(format!("{}/oauth2/token/revoke", self.base))
            .form(&[
                ("client_id", client_id.as_str()),
                ("client_secret", secret),
                ("token", token.expose()),
                ("token_type_hint", "access_token"),
            ])
            .send()
            .await;
        if let Err(error) = result.and_then(reqwest::Response::error_for_status) {
            tracing::debug!(%error, "token revoke failed");
        }
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, token: &Secret, path: &str) -> Result<T, CallError> {
        let response = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token.expose())
            .send()
            .await?;
        match response.status() {
            status if status.is_success() => Ok(response.json().await?),
            StatusCode::UNAUTHORIZED => Err(CallError::Unauthorized),
            status => Err(CallError::Other(anyhow::anyhow!("GET {path} answered {status}"))),
        }
    }

    pub async fn me(&self, token: &Secret) -> Result<DiscordUser, CallError> {
        self.get(token, "/users/@me").await
    }

    pub async fn guilds(&self, token: &Secret) -> Result<Vec<UserGuild>, CallError> {
        self.get(token, "/users/@me/guilds").await
    }
}

/// The servers the user is in, cached for a minute (`force` asks Discord again). Refreshes the
/// access token first when it is about to run out; a token Discord refuses ends the session.
pub async fn user_guilds(state: &WebState, current: &mut CurrentSession, force: bool) -> ApiResult<Vec<UserGuild>> {
    if let Some(guilds) = &current.session.dev_guilds {
        return Ok(guilds.clone());
    }
    let key = format!("web:guilds:{}", current.session.user_id);
    let cache = &state.app.cache;
    if !force
        && let Some(raw) = cache.get(&key).await?
        && let Ok(guilds) = serde_json::from_str(&raw)
    {
        return Ok(guilds);
    }
    let token = fresh_token(state, current).await?;
    let guilds = match state.discord.guilds(&token).await {
        Ok(guilds) => guilds,
        Err(CallError::Unauthorized) => return Err(end_session(state, current).await),
        Err(CallError::Other(error)) => return Err(error.context("server list").into()),
    };
    let raw = serde_json::to_string(&guilds).map_err(anyhow::Error::from)?;
    cache.put(&key, &raw, LIST_TTL).await?;
    Ok(guilds)
}

/// The session's access token, refreshed first when it is about to expire.
///
/// Discord hands out a new refresh token with every refresh and refuses the old one, so two
/// requests must never refresh one session at once: a Redis lock lets one through, and the others
/// wait for it and then use the token it stored.
async fn fresh_token(state: &WebState, current: &mut CurrentSession) -> ApiResult<Secret> {
    if !expiring(&current.session) {
        return Ok(current.session.access_token.clone());
    }
    let cache = &state.app.cache;
    let lock = format!("web:refresh:{}", current.sid);
    let started = std::time::Instant::now();
    while cache.try_cooldown(&lock, REFRESH_LOCK).await?.is_some() {
        if started.elapsed() > REFRESH_WAIT {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "busy",
                "Discord is slow to answer right now. Try again in a moment.",
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let refreshed = refresh_locked(state, current).await;
    if let Err(error) = cache.release_cooldown(&lock).await {
        tracing::warn!(%error, "token refresh lock not released; it runs out by itself");
    }
    refreshed
}

/// The refresh itself, under the session's lock: whoever held it before may have done it already.
async fn refresh_locked(state: &WebState, current: &mut CurrentSession) -> ApiResult<Secret> {
    let Some(stored) = session::read(&state.app.cache, &current.sid).await? else {
        return Err(ApiError::session_expired());
    };
    current.session = stored;
    if !expiring(&current.session) {
        return Ok(current.session.access_token.clone());
    }
    let Some(refresh) = current.session.refresh_token.clone() else {
        return Err(end_session(state, current).await);
    };
    match state.discord.refresh(&state.settings, &refresh).await {
        Ok(tokens) => {
            let now = chrono::Utc::now().timestamp();
            current.session.access_token = tokens.access_token;
            current.session.refresh_token = tokens.refresh_token.or(Some(refresh));
            current.session.token_expires_at = now + tokens.expires_in;
            current.session.last_seen_at = now;
            // Logged out (or in again) while Discord was answering: the session is gone, so stays gone.
            if !session::update(&state.app.cache, &current.sid, &current.session).await? {
                return Err(ApiError::session_expired());
            }
            Ok(current.session.access_token.clone())
        }
        Err(CallError::Unauthorized) => Err(end_session(state, current).await),
        Err(CallError::Other(error)) => Err(error.context("token refresh").into()),
    }
}

fn expiring(session: &Session) -> bool {
    session.token_expires_at - chrono::Utc::now().timestamp() <= REFRESH_MARGIN_SECS
}

async fn end_session(state: &WebState, current: &CurrentSession) -> ApiError {
    if let Err(error) = session::delete(&state.app.cache, &current.sid).await {
        return error.into();
    }
    ApiError::session_expired()
}

/// Whether the bot is in `guild`, asked with the bot's token and cached for a minute. A Discord
/// outage reads as absent and is not cached.
pub async fn bot_present(state: &WebState, guild: u64) -> bool {
    let key = format!("web:bot-in:{guild}");
    let cache = &state.app.cache;
    if let Ok(Some(known)) = cache.get(&key).await {
        return known == "1";
    }
    let present = match state.app.http.guild(Id::new(guild.max(1))).await {
        Ok(_) => true,
        Err(error) => match error.kind() {
            ErrorType::Response { status, .. } if matches!(status.get(), 403 | 404) => false,
            _ => {
                tracing::debug!(%error, guild, "bot presence lookup failed");
                return false;
            }
        },
    };
    if let Err(error) = cache.put(&key, if present { "1" } else { "0" }, LIST_TTL).await {
        tracing::debug!(%error, "bot presence not cached");
    }
    present
}

/// Presence for many servers, a few lookups at a time.
pub async fn bots_present(state: &WebState, guilds: &[u64]) -> Vec<bool> {
    let lookups = guilds.iter().map(|&guild| {
        let state = state.clone();
        async move { bot_present(&state, guild).await }
    });
    stream::iter(lookups.collect::<Vec<_>>()).buffered(8).collect().await
}

/// The authorize URL with the bot's scopes and permissions, before any server is named.
fn bot_authorize(settings: &WebSettings, application_id: u64) -> Url {
    let mut url = Url::parse(&settings.authorize_url).expect("authorize URL was checked at start");
    url.query_pairs_mut()
        .append_pair("client_id", &application_id.to_string())
        .append_pair("scope", "bot applications.commands")
        .append_pair("permissions", &BOT_PERMISSIONS.to_string());
    url
}

/// The link that adds the bot to `guild`, with its commands and the permissions it uses.
pub fn invite_url(settings: &WebSettings, application_id: u64, guild: u64) -> String {
    let mut url = bot_authorize(settings, application_id);
    url.query_pairs_mut()
        .append_pair("guild_id", &guild.to_string())
        .append_pair("disable_guild_select", "true");
    url.into()
}

/// The link the public site hands out: the same scopes and permissions, with the server picked on
/// Discord's own page.
pub fn bot_invite_url(settings: &WebSettings, application_id: u64) -> String {
    bot_authorize(settings, application_id).into()
}

/// Turns a Discord user into what a session stores about them.
pub fn session_for(user: DiscordUser, tokens: TokenResponse, now: i64) -> anyhow::Result<Session> {
    Ok(Session {
        user_id: user.id.parse().context("Discord sent a user id that is not a number")?,
        username: user.username,
        global_name: user.global_name,
        avatar: user.avatar,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        token_expires_at: now + tokens.expires_in,
        csrf: session::random_token(32),
        created_at: now,
        last_seen_at: now,
        dev_guilds: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guild(owner: bool, permissions: &str) -> UserGuild {
        UserGuild {
            id: "1".into(),
            name: "g".into(),
            icon: None,
            owner,
            permissions: permissions.into(),
        }
    }

    #[test]
    fn managers_are_owners_manage_server_or_admins() {
        assert!(guild(true, "0").can_manage());
        assert!(guild(false, &MANAGE_GUILD.to_string()).can_manage());
        assert!(guild(false, &ADMINISTRATOR.to_string()).can_manage());
        assert!(!guild(false, &((1u64 << 11) | (1 << 28)).to_string()).can_manage());
        assert!(!guild(false, "nonsense").can_manage());
        // Discord sends the bits as a string because they pass 2^53.
        assert!(guild(false, &((1u64 << 60) | MANAGE_GUILD).to_string()).can_manage());
    }

    #[test]
    fn the_invite_names_the_server_and_scopes() {
        let settings = WebSettings::resolve(&Default::default(), 9).unwrap();
        let url = invite_url(&settings, 9, 42);
        assert!(url.starts_with("https://discord.com/oauth2/authorize?client_id=9&"));
        assert!(url.contains("scope=bot+applications.commands"));
        assert!(url.contains("guild_id=42"));
        assert!(url.contains(&format!("permissions={BOT_PERMISSIONS}")));
    }

    #[test]
    fn the_public_invite_lets_discord_pick_the_server() {
        let settings = WebSettings::resolve(&Default::default(), 9).unwrap();
        let url = bot_invite_url(&settings, 9);
        assert_eq!(
            url,
            format!(
                "https://discord.com/oauth2/authorize?client_id=9&scope=bot+applications.commands&permissions={BOT_PERMISSIONS}"
            )
        );
    }
}
