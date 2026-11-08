//! Server-side sessions in Redis behind an HttpOnly cookie. Tokens never reach the browser.

use std::time::Duration;

use axum::http::{HeaderMap, HeaderValue, header::COOKIE};
use base64::Engine;
use pinchora_cache::Cache;
use pinchora_core::config::Secret;
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
const TOUCH_EVERY: Duration = Duration::from_secs(300);

/// A logged-in user. Lives only in Redis; the tokens never print.
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: u64,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    pub access_token: Secret,
    pub refresh_token: Option<Secret>,
    /// Unix seconds.
    pub token_expires_at: i64,
    /// What `X-CSRF-Token` must carry on every write.
    pub csrf: String,
    pub created_at: i64,
    /// When the session was last written: its login, or its last token refresh.
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

/// Writes a changed session only while it still exists; `false` when it is gone. A request that was
/// running while the user logged out (or in again) must not bring the old session back.
pub async fn update(cache: &Cache, sid: &str, session: &Session) -> anyhow::Result<bool> {
    cache
        .put_if_present(&key(sid), &serde_json::to_string(session)?, IDLE)
        .await
}

/// The live session behind `sid`, its idle clock pushed back; `None` when there is none or it is too old.
///
/// The idle clock lives in the key's expiry, so pushing it back never rewrites the session: a
/// request that read the session just before another one refreshed its tokens cannot put the old
/// tokens back.
pub async fn load(cache: &Cache, sid: &str, now: i64) -> anyhow::Result<Option<Session>> {
    let Some(session) = read(cache, sid).await? else {
        return Ok(None);
    };
    if expired(session.created_at, now) {
        cache.forget(&key(sid)).await?;
        return Ok(None);
    }
    if cache.once(&format!("web:sess-touch:{sid}"), TOUCH_EVERY).await? {
        cache.extend(&key(sid), IDLE).await?;
    }
    Ok(Some(session))
}

fn expired(created_at: i64, now: i64) -> bool {
    created_at > now || now.saturating_sub(created_at) >= LIFETIME_SECS
}

/// The session behind `sid` as stored, without touching it. A session that no longer parses is
/// dropped.
pub async fn read(cache: &Cache, sid: &str) -> anyhow::Result<Option<Session>> {
    if !plausible(sid) {
        return Ok(None);
    }
    let Some(raw) = cache.get(&key(sid)).await? else {
        return Ok(None);
    };
    match serde_json::from_str::<Session>(&raw) {
        Ok(session) => Ok(Some(session)),
        Err(_) => {
            cache.forget(&key(sid)).await?;
            Ok(None)
        }
    }
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

    #[test]
    fn absolute_lifetime_has_an_exact_boundary() {
        assert!(!expired(100, 100 + LIFETIME_SECS - 1));
        assert!(expired(100, 100 + LIFETIME_SECS));
        assert!(expired(101, 100));
    }
}
