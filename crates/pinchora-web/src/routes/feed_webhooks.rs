//! Twitch EventSub callback: verify before parsing, deduplicate, then wake the existing poller.

use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

use crate::state::WebState;

type HmacSha256 = Hmac<Sha256>;

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/twitch", post(twitch))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn(bounded))
}

// Bound body buffering, concurrent work and HMAC work even if Redis is unavailable.
static IN_FLIGHT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(16);
static WINDOW: std::sync::Mutex<(u64, u32)> = std::sync::Mutex::new((0, 0));

async fn bounded(request: Request, next: Next) -> Response {
    let Ok(_permit) = IN_FLIGHT.try_acquire() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let minute = Utc::now().timestamp().max(0) as u64 / 60;
    {
        let mut window = WINDOW.lock().expect("webhook window");
        if window.0 != minute {
            *window = (minute, 0);
        }
        window.1 += 1;
        if window.1 > 600 {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), next.run(request))
        .await
        .unwrap_or_else(|_| StatusCode::SERVICE_UNAVAILABLE.into_response())
}

fn fresh(timestamp: &str, now: DateTime<Utc>) -> bool {
    timestamp.parse::<DateTime<Utc>>().is_ok_and(|at| {
        let age = now.signed_duration_since(at);
        age >= chrono::Duration::seconds(-60) && age <= chrono::Duration::minutes(10)
    })
}

fn subscription<'a>(value: &'a Value, callback: &str) -> Option<&'a str> {
    let sub = &value["subscription"];
    if !matches!(sub["type"].as_str(), Some("stream.online" | "stream.offline"))
        || sub["version"] != "1"
        || sub["transport"]["method"] != "webhook"
        || sub["transport"]["callback"] != callback
        || sub["id"].as_str().is_none_or(str::is_empty)
    {
        return None;
    }
    sub["condition"]["broadcaster_user_id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_digit()))
}

// The message-type header is not part of Twitch's MAC. Its meaning must agree with the signed body.
fn message_shape(kind: &str, value: &Value) -> bool {
    match kind {
        "webhook_callback_verification" => {
            value["subscription"]["status"] == "webhook_callback_verification_pending"
                && value["challenge"].is_string()
                && value.get("event").is_none()
        }
        "notification" => value["subscription"]["status"] == "enabled" && value["event"].is_object(),
        "revocation" => {
            matches!(
                value["subscription"]["status"].as_str(),
                Some("authorization_revoked" | "notification_failures_exceeded" | "user_removed" | "version_removed")
            ) && value.get("event").is_none()
                && value.get("challenge").is_none()
        }
        _ => false,
    }
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

fn signed(secret: &str, id: &str, timestamp: &str, body: &[u8], signature: &str) -> bool {
    let Some(hex) = signature.strip_prefix("sha256=") else {
        return false;
    };
    if hex.len() != 64 {
        return false;
    }
    let Ok(bytes) = hex::decode(hex) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(id.as_bytes());
    mac.update(timestamp.as_bytes());
    mac.update(body);
    mac.verify_slice(&bytes).is_ok()
}

async fn twitch(State(state): State<WebState>, headers: HeaderMap, body: Bytes) -> Response {
    if !pinchora_commands::feeds::live::configured(pinchora_core::feeds::Kind::Twitch) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let Some(callback) = pinchora_commands::feeds::live::webhook_callback() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(secret) = std::env::var("PINCHORA_FEEDS__TWITCH_CLIENT_SECRET")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    process(state, headers, body, &secret, &callback).await
}

async fn process(state: WebState, headers: HeaderMap, body: Bytes, secret: &str, callback: &str) -> Response {
    if body.len() > 64 * 1024 {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let (Some(id), Some(timestamp), Some(signature), Some(kind)) = (
        header(&headers, "twitch-eventsub-message-id"),
        header(&headers, "twitch-eventsub-message-timestamp"),
        header(&headers, "twitch-eventsub-message-signature"),
        header(&headers, "twitch-eventsub-message-type"),
    ) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if id.is_empty() || id.len() > 255 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if !fresh(timestamp, Utc::now()) || !signed(secret, id, timestamp, &body, signature) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !message_shape(kind, &value) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(broadcaster) = subscription(&value, callback) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    // Authentication precedes shared storage work; signed floods are bounded across pods too.
    let budget = format!("feeds:webhooks:{}", Utc::now().timestamp() / 60);
    match state.app.cache.incr(&budget, std::time::Duration::from_secs(60)).await {
        Ok(n) if n <= 1200 => {}
        Ok(_) => return StatusCode::TOO_MANY_REQUESTS.into_response(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
    let source = match state.app.db.twitch_subscription_source(broadcaster).await {
        Ok(Some(source)) => source,
        Ok(None) => return StatusCode::FORBIDDEN.into_response(),
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    match kind {
        "webhook_callback_verification" => {
            let Some(challenge) = value["challenge"].as_str() else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            if challenge.len() > 500 {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if state.app.db.record_twitch_challenge(id).await.is_err() {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            (
                StatusCode::OK,
                [
                    ("content-type", "text/plain; charset=utf-8".to_owned()),
                    ("content-length", challenge.len().to_string()),
                ],
                challenge.to_owned(),
            )
                .into_response()
        }
        "notification" => {
            if value["event"]["broadcaster_user_id"] != broadcaster {
                return StatusCode::FORBIDDEN.into_response();
            }
            let event = &value["event"];
            let Some(login) = event["broadcaster_user_login"].as_str() else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            if !login.eq_ignore_ascii_case(&source) {
                return StatusCode::FORBIDDEN.into_response();
            }
            if pinchora_core::feeds::parse_source(pinchora_core::feeds::Kind::Twitch, &source).is_err() {
                return StatusCode::BAD_REQUEST.into_response();
            }
            match state.app.db.record_twitch_webhook(id, &source).await {
                Ok(_) => StatusCode::NO_CONTENT.into_response(),
                Err(error) => {
                    tracing::warn!(%error, "Twitch webhook was not recorded");
                    StatusCode::SERVICE_UNAVAILABLE.into_response()
                }
            }
        }
        "revocation" => {
            if let Err(error) = state.app.db.record_twitch_revocation(id, broadcaster).await {
                tracing::warn!(%error, "EventSub revocation was not recorded");
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            StatusCode::NO_CONTENT.into_response()
        }
        _ => StatusCode::BAD_REQUEST.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn authenticated_challenge_only_answers_for_a_followed_broadcaster() {
        use pinchora_commands::App;
        use pinchora_db::{Db, NewFeed};
        use std::sync::Arc;
        use twilight_model::id::Id;
        let (Ok(database), Ok(redis)) = (
            std::env::var("PINCHORA_TEST_DATABASE_URL"),
            std::env::var("PINCHORA_TEST_REDIS_URL"),
        ) else {
            return;
        };
        crate::fixture_isolation::shared().await;
        let db = Db::connect_waiting(&database, 5, std::time::Duration::from_secs(5))
            .await
            .unwrap();
        db.migrate().await.unwrap();
        let cache = pinchora_cache::Cache::connect(&redis).await.unwrap();
        let http = Arc::new(twilight_http::Client::new("test".into()));
        let app = Arc::new(App::new(
            http.clone(),
            http,
            Id::new(1),
            db,
            cache,
            "webhook-test".into(),
        ));
        let settings = crate::WebSettings::resolve(&pinchora_core::config::WebConfig::default(), 1).unwrap();
        let state = WebState::new(app, settings);
        let user = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            >> 4) as u64;
        let source = format!("stream{user}");
        state
            .app
            .db
            .add_feed(&NewFeed {
                guild: user,
                kind: "twitch",
                source: &source,
                title: None,
                channel: user + 1,
                message: None,
                embed: true,
                ping_role: None,
                live_role: None,
                include: &[],
                exclude: &[],
                created_by: user,
                seen: &[],
                interval_secs: 120,
            })
            .await
            .unwrap();
        state
            .app
            .db
            .set_live_stream("twitch", &source, &user.to_string(), None)
            .await
            .unwrap();
        let callback = "https://example.com/hooks/twitch";
        let payload = serde_json::json!({"challenge":"raw-challenge", "subscription": {
            "id": "our-sub", "status": "webhook_callback_verification_pending", "type": "stream.online", "version": "1", "condition": {"broadcaster_user_id": user.to_string()},
            "transport": {"method":"webhook", "callback":callback}
        }});
        for (n, body, status) in [
            (0, payload.clone(), StatusCode::OK),
            (0, payload.clone(), StatusCode::OK),
            (
                1,
                {
                    let mut v = payload.clone();
                    v["subscription"]["condition"]["broadcaster_user_id"] = "0".into();
                    v
                },
                StatusCode::FORBIDDEN,
            ),
            (
                2,
                {
                    let mut v = payload.clone();
                    v["subscription"]["transport"]["callback"] = "https://foreign.example/".into();
                    v
                },
                StatusCode::FORBIDDEN,
            ),
        ] {
            let id = format!("challenge-{user}-{n}");
            let timestamp = Utc::now().to_rfc3339();
            let bytes = body.to_string();
            let mut mac = HmacSha256::new_from_slice(b"secret").unwrap();
            mac.update(id.as_bytes());
            mac.update(timestamp.as_bytes());
            mac.update(bytes.as_bytes());
            let mut headers = HeaderMap::new();
            for (name, value) in [
                ("twitch-eventsub-message-id", id),
                ("twitch-eventsub-message-timestamp", timestamp),
                ("twitch-eventsub-message-type", "webhook_callback_verification".into()),
                (
                    "twitch-eventsub-message-signature",
                    format!("sha256={}", hex::encode(mac.finalize().into_bytes())),
                ),
            ] {
                headers.insert(
                    axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                    value.parse().unwrap(),
                );
            }
            let response = process(state.clone(), headers, Bytes::from(bytes), "secret", callback).await;
            assert_eq!(response.status(), status);
            if status == StatusCode::OK {
                assert_eq!(response.headers()["content-length"], "13");
                assert_eq!(
                    axum::body::to_bytes(response.into_body(), 500).await.unwrap(),
                    "raw-challenge"
                );
            }
        }
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM feed_webhook_messages WHERE message_id = $1")
            .bind(format!("challenge-{user}-0"))
            .fetch_one(state.app.db.pool())
            .await
            .unwrap();
        assert_eq!(count, 1);
        sqlx::query("DELETE FROM feeds WHERE guild_id = $1")
            .bind(user as i64)
            .execute(state.app.db.pool())
            .await
            .unwrap();
        sqlx::query("DELETE FROM feed_sources WHERE source = $1")
            .bind(source)
            .execute(state.app.db.pool())
            .await
            .unwrap();
    }

    #[test]
    fn unsigned_message_type_cannot_reinterpret_a_signed_notification() {
        let notification = serde_json::json!({"subscription":{"status":"enabled"}, "event":{}});
        assert!(message_shape("notification", &notification));
        assert!(!message_shape("revocation", &notification));
        assert!(!message_shape("webhook_callback_verification", &notification));
        let revocation = serde_json::json!({"subscription":{"status":"authorization_revoked"}});
        assert!(message_shape("revocation", &revocation));
        assert!(!message_shape("notification", &revocation));
    }

    #[test]
    fn timestamps_reject_stale_future_and_invalid_requests() {
        let now: DateTime<Utc> = "2026-01-01T00:10:00Z".parse().unwrap();
        assert!(fresh("2026-01-01T00:00:00Z", now));
        assert!(!fresh("2025-12-31T23:59:59Z", now));
        assert!(!fresh("2026-01-01T00:11:01Z", now));
        assert!(!fresh("garbage", now));
    }

    #[test]
    fn challenges_require_our_callback_and_supported_subscription() {
        let callback = "https://example.com/hooks/twitch";
        let value = serde_json::json!({"subscription": {
            "id": "sub", "type": "stream.online", "version": "1",
            "condition": {"broadcaster_user_id": "42"},
            "transport": {"method": "webhook", "callback": callback}
        }});
        assert_eq!(subscription(&value, callback), Some("42"));
        assert_eq!(subscription(&value, "https://other.example/hooks/twitch"), None);
        for (key, val) in [("type", "channel.follow"), ("version", "2"), ("id", "")] {
            let mut bad = value.clone();
            bad["subscription"][key] = val.into();
            assert_eq!(subscription(&bad, callback), None);
        }
        assert_eq!(subscription(&serde_json::json!({"challenge": "hello"}), callback), None);
    }

    #[tokio::test]
    async fn oversized_webhooks_are_rejected_before_the_handler() {
        use axum::body::Body;
        use tower::ServiceExt;
        let app = Router::new()
            .route("/", post(|_: Bytes| async { StatusCode::OK }))
            .layer(DefaultBodyLimit::max(64 * 1024));
        let response = app
            .oneshot(
                axum::http::Request::post("/")
                    .body(Body::from(vec![0; 64 * 1024 + 1]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let permits = IN_FLIGHT.acquire_many(16).await.unwrap();
        let app = Router::new()
            .route("/", post(|| async { StatusCode::OK }))
            .layer(middleware::from_fn(bounded));
        let response = app
            .oneshot(axum::http::Request::post("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        drop(permits);
    }

    #[test]
    fn signature_uses_raw_body_and_constant_time_comparison() {
        let mut mac = HmacSha256::new_from_slice(b"secret").unwrap();
        mac.update(b"id2026-01-01T00:00:00Z{\"event\":1}");
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(signed("secret", "id", "2026-01-01T00:00:00Z", br#"{"event":1}"#, &sig));
        assert!(!signed("secret", "id", "2026-01-01T00:00:00Z", br#"{"event":2}"#, &sig));
    }
}
