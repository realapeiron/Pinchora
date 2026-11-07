//! Twitch Helix and Kick public API lookups. Only fixed provider hosts are contacted in production.
//! Tokens are app credentials, cached until shortly before expiry and never logged.

use std::{
    collections::HashMap,
    sync::LazyLock,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use pinchora_core::feeds::{Item, Kind, page_url};
use reqwest::Client;
use serde_json::Value;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct LiveRead {
    pub title: String,
    pub broadcaster_id: String,
    pub bio: String,
    pub item: Option<Item>,
}

struct Token {
    value: String,
    until: Instant,
}
static TWITCH_TOKEN: LazyLock<Mutex<Option<Token>>> = LazyLock::new(|| Mutex::new(None));
static KICK_TOKEN: LazyLock<Mutex<Option<Token>>> = LazyLock::new(|| Mutex::new(None));
static HTTP: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .expect("live HTTP client")
});

/// Media is embedded remotely, never fetched by our server. Restrict live media to provider CDNs.
pub fn media_url(kind: Kind, raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw).ok()?;
    let host = url.host_str()?;
    let allowed = match kind {
        Kind::Twitch => host == "static-cdn.jtvnw.net",
        Kind::Kick => host == "images.kick.com" || host == "stream.kick.com",
        _ => false,
    };
    (allowed
        && url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && raw.len() <= 1000)
        .then(|| url.to_string())
}

fn credentials(kind: Kind) -> Option<(String, String)> {
    let prefix = match kind {
        Kind::Twitch => "TWITCH",
        Kind::Kick => "KICK",
        _ => return None,
    };
    let id = std::env::var(format!("PINCHORA_FEEDS__{prefix}_CLIENT_ID")).ok()?;
    let secret = std::env::var(format!("PINCHORA_FEEDS__{prefix}_CLIENT_SECRET")).ok()?;
    (!id.trim().is_empty() && !secret.trim().is_empty()).then_some((id, secret))
}

pub fn configured(kind: Kind) -> bool {
    credentials(kind).is_some()
}

async fn token(kind: Kind) -> anyhow::Result<(String, String)> {
    let (client_id, client_secret) =
        credentials(kind).ok_or_else(|| anyhow::anyhow!("provider credentials are missing"))?;
    let cache = if kind == Kind::Twitch {
        &TWITCH_TOKEN
    } else {
        &KICK_TOKEN
    };
    let mut cached = cache.lock().await;
    if let Some(token) = cached.as_ref().filter(|token| token.until > Instant::now()) {
        return Ok((client_id, token.value.clone()));
    }
    let endpoint = if kind == Kind::Twitch {
        "https://id.twitch.tv/oauth2/token"
    } else {
        "https://id.kick.com/oauth/token"
    };
    let answer = request_token(&HTTP, endpoint, &client_id, &client_secret).await?;
    let value = answer["access_token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("provider did not return a token"))?
        .to_owned();
    let seconds = answer["expires_in"].as_u64().unwrap_or(3600).saturating_sub(60).max(1);
    *cached = Some(Token {
        value: value.clone(),
        until: Instant::now() + Duration::from_secs(seconds),
    });
    Ok((client_id, value))
}

async fn request_token(http: &Client, endpoint: &str, client_id: &str, client_secret: &str) -> anyhow::Result<Value> {
    let response = http
        .post(endpoint)
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("provider token request failed"))?;
    response_json(response).await
}

async fn get(kind: Kind, path: &str) -> anyhow::Result<Value> {
    let (id, token) = token(kind).await?;
    let base = if kind == Kind::Twitch {
        "https://api.twitch.tv/helix"
    } else {
        "https://api.kick.com/public/v1"
    };
    request_json(&HTTP, kind, &format!("{base}{path}"), &id, &token).await
}

async fn request_json(http: &Client, kind: Kind, url: &str, id: &str, token: &str) -> anyhow::Result<Value> {
    let mut request = http.get(url).bearer_auth(token);
    if kind == Kind::Twitch {
        request = request.header("Client-Id", id);
    }
    response_json(
        request
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("provider request failed"))?,
    )
    .await
}

async fn response_json(mut response: reqwest::Response) -> anyhow::Result<Value> {
    if !response.status().is_success() {
        anyhow::bail!("provider request was refused");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("provider response failed"))?
    {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            anyhow::bail!("provider response too large");
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("invalid provider response"))
}

fn string(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}
fn date(value: &Value, key: &str) -> Option<DateTime<Utc>> {
    value[key].as_str()?.parse().ok()
}

pub fn twitch_read(source: &str, user: &Value, stream: &Value) -> Option<LiveRead> {
    let user = user["data"]
        .as_array()?
        .iter()
        .find(|row| row["login"].as_str().is_some_and(|v| v.eq_ignore_ascii_case(source)))?;
    let login = user["login"].as_str()?;
    if !login.eq_ignore_ascii_case(source) {
        return None;
    }
    let entry = stream["data"].as_array()?.iter().find(|entry| {
        entry["user_login"]
            .as_str()
            .is_some_and(|v| v.eq_ignore_ascii_case(source))
    });
    let item = entry.and_then(|entry| {
        let id = entry["id"].as_str()?;
        let thumbnail = entry["thumbnail_url"]
            .as_str()
            .map(|s| s.replace("{width}", "640").replace("{height}", "360"));
        Some(Item {
            id: id.to_owned(),
            title: string(entry, "title"),
            url: page_url(Kind::Twitch, source),
            author: Some(string(user, "display_name")),
            summary: Some(string(entry, "game_name")),
            published: date(entry, "started_at"),
            image: thumbnail.as_deref().and_then(|url| media_url(Kind::Twitch, url)),
            ..Item::default()
        })
    });
    Some(LiveRead {
        title: string(user, "display_name"),
        broadcaster_id: string(user, "id"),
        bio: string(user, "description"),
        item,
    })
}

/// Helix accepts up to 100 repeated `login`/`user_login` parameters per request.
pub async fn lookup_twitch_batch(sources: &[String]) -> anyhow::Result<HashMap<String, LiveRead>> {
    let mut found = HashMap::new();
    for chunk in sources.chunks(100) {
        let query = chunk.iter().map(|s| format!("login={s}")).collect::<Vec<_>>().join("&");
        let streams_query = chunk
            .iter()
            .map(|s| format!("user_login={s}"))
            .collect::<Vec<_>>()
            .join("&");
        let users = get(Kind::Twitch, &format!("/users?{query}")).await?;
        let streams = get(Kind::Twitch, &format!("/streams?{streams_query}")).await?;
        for source in chunk {
            if let Some(read) = twitch_read(source, &users, &streams) {
                found.insert(source.clone(), read);
            }
        }
    }
    Ok(found)
}

pub fn kick_read(source: &str, response: &Value) -> Option<LiveRead> {
    let channel = response["data"]
        .as_array()?
        .iter()
        .find(|channel| channel["slug"].as_str().is_some_and(|v| v.eq_ignore_ascii_case(source)))?;
    let stream = &channel["stream"];
    let item = stream["is_live"].as_bool().unwrap_or(false).then(|| {
        let started = date(stream, "start_time");
        Item {
            id: format!(
                "{}:{}",
                source,
                started.map_or_else(|| "live".to_owned(), |v| v.to_rfc3339())
            ),
            title: string(channel, "stream_title"),
            url: page_url(Kind::Kick, source),
            author: Some(source.to_owned()),
            summary: channel["category"]["name"].as_str().map(str::to_owned),
            published: started,
            image: stream["thumbnail"].as_str().and_then(|url| media_url(Kind::Kick, url)),
            ..Item::default()
        }
    });
    Some(LiveRead {
        title: source.to_owned(),
        broadcaster_id: channel["broadcaster_user_id"].to_string(),
        bio: string(channel, "channel_description"),
        item,
    })
}

pub async fn lookup(kind: Kind, source: &str) -> anyhow::Result<LiveRead> {
    let read = match kind {
        Kind::Twitch => {
            let users = get(kind, &format!("/users?login={source}")).await?;
            let streams = get(kind, &format!("/streams?user_login={source}")).await?;
            twitch_read(source, &users, &streams)
        }
        Kind::Kick => kick_read(source, &get(kind, &format!("/channels?slug={source}")).await?),
        _ => None,
    };
    read.ok_or_else(|| anyhow::anyhow!("channel was not found"))
}

/// Register stream start/end callbacks only when an HTTPS public origin is configured.
pub fn webhook_callback() -> Option<String> {
    let raw = std::env::var("PINCHORA_FEEDS__WEBHOOK_PUBLIC_URL").ok()?;
    let url = reqwest::Url::parse(raw.trim()).ok()?;
    (url.scheme() == "https"
        && url.host_str().is_some()
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then(|| format!("{}/hooks/twitch", raw.trim_end_matches('/')))
}

pub async fn subscribe_twitch(broadcaster_id: &str) -> anyhow::Result<()> {
    let Some(callback) = webhook_callback() else {
        return Ok(());
    };
    let (client_id, access_token) = token(Kind::Twitch).await?;
    let secret = credentials(Kind::Twitch).expect("token needs credentials").1;
    for event in ["stream.online", "stream.offline"] {
        let body = serde_json::json!({
            "type": event, "version": "1", "condition": { "broadcaster_user_id": broadcaster_id },
            "transport": { "method": "webhook", "callback": callback, "secret": secret }
        });
        let response = HTTP
            .post("https://api.twitch.tv/helix/eventsub/subscriptions")
            .header("Client-Id", &client_id)
            .bearer_auth(&access_token)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await?;
        if !response.status().is_success() && response.status().as_u16() != 409 {
            anyhow::bail!("EventSub subscription returned {}", response.status());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        http::HeaderMap,
        routing::{get, post},
    };
    use serde_json::json;
    use tokio::net::TcpListener;
    #[test]
    fn live_media_cannot_target_internal_or_untrusted_hosts() {
        for kind in [Kind::Twitch, Kind::Kick] {
            for url in [
                "http://127.0.0.1/a",
                "https://[::1]/a",
                "https://169.254.169.254/a",
                "https://static-cdn.jtvnw.net.evil.test/a",
                "https://user@static-cdn.jtvnw.net/a",
                "https://images.kick.com:8443/a",
                "file:///etc/passwd",
            ] {
                assert!(media_url(kind, url).is_none(), "{url}");
            }
        }
        assert!(media_url(Kind::Twitch, "https://static-cdn.jtvnw.net/a.jpg").is_some());
        assert!(media_url(Kind::Kick, "https://images.kick.com/a.jpg").is_some());
    }

    #[tokio::test]
    async fn provider_redirects_are_not_followed_and_errors_omit_secrets() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = Router::new().route(
            "/",
            get(|| async {
                (
                    axum::http::StatusCode::TEMPORARY_REDIRECT,
                    [("location", "http://127.0.0.1/private?secret=do-not-log")],
                )
            }),
        );
        let router = router.route("/large", get(|| async { "x".repeat(2 * 1024 * 1024 + 1) }));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let error = request_json(&HTTP, Kind::Twitch, &base, "id", "secret")
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "provider request was refused");
        let error = request_json(&HTTP, Kind::Twitch, &format!("{base}/large"), "id", "secret")
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "provider response too large");
        server.abort();
    }

    #[test]
    fn parses_helix_live_and_offline() {
        let user = json!({"data":[{"id":"42","login":"abc","display_name":"ABC","description":"hello"}]});
        let stream = json!({"data":[{"id":"stream-1","user_login":"abc","title":"Hi","game_name":"Chess","thumbnail_url":"https://example.com/{width}x{height}","started_at":"2026-01-01T10:00:00Z"}]});
        let live = twitch_read("abc", &user, &stream).unwrap();
        assert_eq!(live.item.unwrap().summary.as_deref(), Some("Chess"));
        assert!(twitch_read("abc", &user, &json!({"data":[]})).unwrap().item.is_none());
    }
    #[test]
    fn parses_kick_live_and_offline() {
        let channel = json!({"data":[{"slug":"abc","broadcaster_user_id":42,"channel_description":"bio","stream_title":"Hi","category":{"name":"Chess"},"stream":{"is_live":true,"start_time":"2026-01-01T10:00:00Z","thumbnail":"https://example.com/thumb"}}]});
        let live = kick_read("abc", &channel).unwrap();
        assert_eq!(live.item.unwrap().summary.as_deref(), Some("Chess"));
        assert!(
            kick_read("abc", &json!({"data":[{"slug":"abc","stream":{"is_live":false}}]}))
                .unwrap()
                .item
                .is_none()
        );
    }

    #[tokio::test]
    async fn provider_token_and_helix_use_mocked_http() {
        let router = Router::new()
            .route(
                "/oauth2/token",
                post(|| async { Json(json!({"access_token":"test-token","expires_in":3600})) }),
            )
            .route(
                "/helix/users",
                get(|headers: HeaderMap| async move {
                    assert_eq!(headers.get("client-id").unwrap(), "test-id");
                    assert_eq!(headers.get("authorization").unwrap(), "Bearer test-token");
                    Json(json!({"data":[{"id":"42","login":"abc","display_name":"ABC"}]}))
                }),
            )
            .route(
                "/public/v1/channels",
                get(|headers: HeaderMap| async move {
                    assert_eq!(headers.get("authorization").unwrap(), "Bearer test-token");
                    Json(json!({"data":[{"slug":"abc","broadcaster_user_id":42,"stream":{"is_live":false}}]}))
                }),
            );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let token = request_token(&HTTP, &format!("{base}/oauth2/token"), "test-id", "test-secret")
            .await
            .unwrap();
        assert_eq!(token["access_token"], "test-token");
        let twitch = request_json(
            &HTTP,
            Kind::Twitch,
            &format!("{base}/helix/users"),
            "test-id",
            "test-token",
        )
        .await
        .unwrap();
        assert_eq!(twitch["data"][0]["login"], "abc");
        let kick = request_json(
            &HTTP,
            Kind::Kick,
            &format!("{base}/public/v1/channels"),
            "",
            "test-token",
        )
        .await
        .unwrap();
        assert!(kick_read("abc", &kick).unwrap().item.is_none());
        server.abort();
    }
}
