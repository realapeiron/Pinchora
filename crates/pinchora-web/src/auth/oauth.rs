//! Discord OAuth2 (authorization code with PKCE), logout, and the loopback-only dev login.

use std::time::Duration;

use axum::{
    Json,
    extract::{Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{LOCATION, SET_COOKIE},
    },
    response::{IntoResponse, Response},
};
use base64::Engine;
use pinchora_core::infra::config::Secret;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::auth::session;
use crate::auth::session::LIFETIME_SECS;
use crate::auth::session::OAUTH_COOKIE;
use crate::auth::session::SESSION_COOKIE;
use crate::auth::session::Session;
use crate::discord;
use crate::discord::CallError;
use crate::discord::UserGuild;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::CurrentSession;
use crate::state::WebState;

/// How long a login may take between leaving for Discord and coming back.
const PENDING_TTL: Duration = Duration::from_secs(600);
/// Where a login lands when it was not sent anywhere in particular.
const DEFAULT_NEXT: &str = "/servers";

/// What is remembered about a login while the user is on Discord.
#[derive(Serialize, Deserialize)]
struct Pending {
    next: String,
    verifier: String,
}

#[derive(Deserialize)]
pub struct LoginQuery {
    next: Option<String>,
}

/// `GET /auth/login?next=/g/123`: off to Discord.
pub async fn login(State(state): State<WebState>, Query(query): Query<LoginQuery>) -> ApiResult<Response> {
    let settings = &state.settings;
    if !settings.login_ready() {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_configured",
            format!("Discord login is not set up yet: set {}.", settings.missing.join(", ")),
        ));
    }
    let oauth_state = hex::encode(rand_bytes::<32>());
    let verifier = session::random_token(32);
    let pending = Pending {
        next: safe_next(query.next.as_deref()).to_owned(),
        verifier: verifier.clone(),
    };
    state
        .app
        .cache
        .put(
            &format!("web:oauth:{oauth_state}"),
            &serde_json::to_string(&pending).map_err(anyhow::Error::from)?,
            PENDING_TTL,
        )
        .await?;

    let mut url = Url::parse(&settings.authorize_url).map_err(anyhow::Error::from)?;
    url.query_pairs_mut()
        .append_pair("client_id", &settings.client_id.to_string())
        .append_pair("response_type", "code")
        .append_pair("scope", "identify guilds")
        .append_pair("redirect_uri", &settings.redirect_uri)
        .append_pair("state", &oauth_state)
        .append_pair("code_challenge", &challenge(&verifier))
        .append_pair("code_challenge_method", "S256")
        .append_pair("prompt", "none");
    let cookie = session::set_cookie(
        OAUTH_COOKIE,
        &oauth_state,
        "/auth",
        PENDING_TTL.as_secs() as i64,
        settings.cookie_secure,
    );
    Ok(redirect(url.as_str(), &[cookie]))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// `GET /auth/callback?code&state`: back from Discord. The state must match the cookie and be
/// one we issued (each is used once); then the code becomes tokens and the tokens a session.
pub async fn callback(
    State(state): State<WebState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> ApiResult<Response> {
    let clear_pending = session::set_cookie(OAUTH_COOKIE, "", "/auth", 0, state.settings.cookie_secure);
    let bad_state = || ApiError::malformed("This login link is stale or was not started here. Log in again.");
    let returned = query.state.as_deref().ok_or_else(bad_state)?;
    let expected = session::cookie(&headers, OAUTH_COOKIE).ok_or_else(bad_state)?;
    if returned != expected || returned.len() != 64 || !returned.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad_state());
    }
    let pending = state
        .app
        .cache
        .take(&format!("web:oauth:{returned}"))
        .await?
        .and_then(|raw| serde_json::from_str::<Pending>(&raw).ok())
        .ok_or_else(bad_state)?;
    if query.error.is_some() {
        return Ok(redirect("/login?error=denied", &[clear_pending]));
    }
    let code = query.code.as_deref().ok_or_else(bad_state)?;

    let discord = &state.discord;
    let tokens = match discord.exchange_code(&state.settings, code, &pending.verifier).await {
        Ok(tokens) => tokens,
        Err(CallError::Unauthorized) => return Ok(redirect("/login?error=denied", &[clear_pending])),
        Err(CallError::Other(error)) => {
            tracing::warn!(%error, "OAuth code exchange failed");
            return Ok(redirect("/login?error=discord", &[clear_pending]));
        }
    };
    let user = match discord.me(&tokens.access_token).await {
        Ok(user) => user,
        Err(CallError::Unauthorized) => return Ok(redirect("/login?error=denied", &[clear_pending])),
        Err(CallError::Other(error)) => {
            tracing::warn!(%error, "OAuth user lookup failed");
            return Ok(redirect("/login?error=discord", &[clear_pending]));
        }
    };
    let now = chrono::Utc::now().timestamp();
    let session = discord::session_for(user, tokens, now)?;
    let sid = replace_session(&state, &headers, &session).await?;
    tracing::info!(user_id = session.user_id, "dashboard login");
    Ok(redirect(
        &pending.next,
        &[clear_pending, session_cookie(&state, &sid, LIFETIME_SECS)],
    ))
}

/// `POST /auth/logout`: ends the session and revokes the Discord token.
pub async fn logout(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Response> {
    session::delete(&state.app.cache, &current.sid).await?;
    state
        .reef
        .terminate(&state, current.session.user_id, "logged_out")
        .await;
    if current.session.dev_guilds.is_none() {
        let (state, token) = (state.clone(), current.session.access_token.clone());
        tokio::spawn(async move { state.discord.revoke(&state.settings, &token).await });
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(SET_COOKIE, session_cookie(&state, "", 0));
    Ok(response)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevLogin {
    user_id: String,
    username: String,
    #[serde(default)]
    guilds: Vec<UserGuild>,
}

/// `POST /auth/dev-login`: a session without Discord, for tests and local UI work. Only exists
/// when `PINCHORA_WEB__DEV_LOGIN=true` on a loopback bind.
pub async fn dev_login(
    State(state): State<WebState>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<DevLogin>,
) -> ApiResult<Response> {
    if !state.settings.dev_login {
        return Err(ApiError::not_found());
    }
    let user_id = body
        .user_id
        .parse()
        .map_err(|_| ApiError::malformed("user_id must be a number."))?;
    let now = chrono::Utc::now().timestamp();
    let session = Session {
        user_id,
        username: body.username,
        global_name: None,
        avatar: None,
        access_token: Secret::from(String::new()),
        refresh_token: None,
        token_expires_at: i64::MAX,
        csrf: session::random_token(32),
        created_at: now,
        last_seen_at: now,
        dev_guilds: Some(body.guilds),
    };
    let sid = replace_session(&state, &headers, &session).await?;
    tracing::warn!(user_id, "dev login");
    let mut response = Json(json!({ "csrf": session.csrf })).into_response();
    response
        .headers_mut()
        .insert(SET_COOKIE, session_cookie(&state, &sid, LIFETIME_SECS));
    Ok(response)
}

/// Stores a new session for a login and ends the one this browser had before, so an old cookie
/// (copied, or left on a shared machine) stops working the moment someone logs in again.
async fn replace_session(state: &WebState, headers: &HeaderMap, session: &Session) -> ApiResult<String> {
    if let Some(old) = session::cookie(headers, SESSION_COOKIE) {
        session::delete(&state.app.cache, old).await?;
    }
    Ok(session::create(&state.app.cache, session).await?)
}

fn session_cookie(state: &WebState, sid: &str, max_age: i64) -> HeaderValue {
    session::set_cookie(SESSION_COOKIE, sid, "/", max_age, state.settings.cookie_secure)
}

fn redirect(to: &str, cookies: &[HeaderValue]) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    if let Ok(location) = HeaderValue::from_str(to) {
        headers.insert(LOCATION, location);
    }
    for cookie in cookies {
        headers.append(SET_COOKIE, cookie.clone());
    }
    response
}

/// A path on this site to land on after login; anything else (other sites, `//host`, `\`) is
/// replaced by the server list.
pub fn safe_next(next: Option<&str>) -> &str {
    match next {
        Some(path)
            if path.starts_with('/')
                && !path.starts_with("//")
                && path.len() <= 512
                && !path.contains('\\')
                && !path.chars().any(char::is_control) =>
        {
            path
        }
        _ => DEFAULT_NEXT,
    }
}

/// The PKCE S256 challenge for `verifier`.
fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn rand_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::fill(&mut bytes[..]);
    bytes
}

#[cfg(test)]
mod tests {
    use crate::auth::oauth::*;

    #[test]
    fn next_stays_on_this_site() {
        assert_eq!(safe_next(Some("/g/1/leveling?x=1")), "/g/1/leveling?x=1");
        assert_eq!(safe_next(None), DEFAULT_NEXT);
        for bad in [
            "https://evil.example",
            "//evil.example",
            "/\\evil.example",
            "g/1",
            "",
            "/a\nb",
        ] {
            assert_eq!(safe_next(Some(bad)), DEFAULT_NEXT, "{bad:?}");
        }
    }

    #[test]
    fn pkce_matches_the_rfc_example() {
        // RFC 7636, appendix B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}
