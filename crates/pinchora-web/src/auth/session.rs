//! Server-side sessions in Redis behind an HttpOnly cookie. Tokens never reach the browser.

use std::time::Duration;

use axum::http::{HeaderMap, HeaderValue, header::COOKIE};
use base64::Engine;
use pinchora_cache::Cache;
use serde::{Deserialize, Serialize};

use crate::discord::UserGuild;

pub const SESSION_COOKIE: &str = "pinchora_sid";
/// Holds the OAuth `state` between the redirect to Discord and the callback.
pub const OAUTH_COOKIE: &str = "pinchora_oauth";

/// A session expires after this long without a request.
const IDLE: Duration = Duration::from_secs(7 * 86_400);
/// And after this long whatever happens.
pub const LIFETIME_SECS: i64 = 30 * 86_400;
/// The idle clock is pushed back at most this often.
const TOUCH_EVERY_SECS: i64 = 300;

/// A logged-in user. Lives only in Redis.
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: u64,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix seconds.
    pub token_expires_at: i64,
    /// What `X-CSRF-Token` must carry on every write.
    pub csrf: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    /// Set by dev login: the server list to use instead of asking Discord.
    #[serde(default)]
    pub dev_guilds: Option<Vec<UserGuild>>,
}

impl Session {
    /// The name the dashboard shows and the audit log records.
    pub fn display_name(&self) -> &str {
        self.global_name.as_deref().unwrap_or(&self.username)
    }
}

fn key(sid: &str) -> String {
    format!("web:sess:{sid}")
}

/// `bytes` random bytes as unpadded base64url.
pub fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::fill(&mut buf[..]);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// Whether `sid` looks like one we issued (43 base64url characters), before it goes near Redis.
fn plausible(sid: &str) -> bool {
    sid.len() == 43 && sid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Stores a new session; returns its id for the cookie.
pub async fn create(cache: &Cache, session: &Session) -> anyhow::Result<String> {
    let sid = random_token(32);
    save(cache, &sid, session).await?;
    Ok(sid)
}

pub async fn save(cache: &Cache, sid: &str, session: &Session) -> anyhow::Result<()> {
    cache.put(&key(sid), &serde_json::to_string(session)?, IDLE).await
}

/// The live session behind `sid`, its idle clock pushed back; `None` when there is none or it is too old.
pub async fn load(cache: &Cache, sid: &str, now: i64) -> anyhow::Result<Option<Session>> {
    if !plausible(sid) {
        return Ok(None);
    }
    let Some(raw) = cache.get(&key(sid)).await? else {
        return Ok(None);
    };
    let Ok(mut session) = serde_json::from_str::<Session>(&raw) else {
        cache.forget(&key(sid)).await?;
        return Ok(None);
    };
    if now - session.created_at > LIFETIME_SECS {
        cache.forget(&key(sid)).await?;
        return Ok(None);
    }
    if now - session.last_seen_at > TOUCH_EVERY_SECS {
        session.last_seen_at = now;
        save(cache, sid, &session).await?;
    }
    Ok(Some(session))
}

pub async fn delete(cache: &Cache, sid: &str) -> anyhow::Result<()> {
    if plausible(sid) {
        cache.forget(&key(sid)).await?;
    }
    Ok(())
}

/// The value of cookie `name`, from any `Cookie` header.
pub fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}

/// A `Set-Cookie` value: HttpOnly, SameSite=Lax, `Secure` when asked. `max_age` 0 deletes it.
pub fn set_cookie(name: &str, value: &str, path: &str, max_age: i64, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{name}={value}; Path={path}; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}"
    ))
    .expect("cookie values are ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_are_found_among_others() {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_static("a=1; pinchora_sid=abc; b=2"));
        headers.append(COOKIE, HeaderValue::from_static("pinchora_oauth=xyz"));
        assert_eq!(cookie(&headers, SESSION_COOKIE), Some("abc"));
        assert_eq!(cookie(&headers, OAUTH_COOKIE), Some("xyz"));
        assert_eq!(cookie(&headers, "pinchora"), None);
    }

    #[test]
    fn tokens_are_long_random_and_plausible() {
        let (a, b) = (random_token(32), random_token(32));
        assert_ne!(a, b);
        assert!(plausible(&a) && plausible(&b));
        assert!(!plausible("short"));
        assert!(!plausible(&format!("{}:", &a[..42])));
    }

    #[test]
    fn cookies_are_http_only_and_lax() {
        let set = set_cookie(SESSION_COOKIE, "v", "/", 60, true);
        assert_eq!(
            set.to_str().unwrap(),
            "pinchora_sid=v; Path=/; Max-Age=60; HttpOnly; SameSite=Lax; Secure"
        );
        assert!(
            !set_cookie(SESSION_COOKIE, "v", "/", 0, false)
                .to_str()
                .unwrap()
                .contains("Secure")
        );
    }
}
