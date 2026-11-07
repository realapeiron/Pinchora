//! Twitch EventSub callback: verify before parsing, deduplicate, then wake the existing poller.

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
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
    Router::new().route("/twitch", post(twitch))
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

fn signed(secret: &str, id: &str, timestamp: &str, body: &[u8], signature: &str) -> bool {
    let Some(hex) = signature.strip_prefix("sha256=") else {
        return false;
    };
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
    let Some(secret) = std::env::var("PINCHORA_FEEDS__TWITCH_CLIENT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
    else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    if body.len() > 1024 * 1024 {
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
    let Some(at) = timestamp.parse::<DateTime<Utc>>().ok() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if (Utc::now() - at).num_seconds().abs() > 600 || !signed(&secret, id, timestamp, &body, signature) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match kind {
        "webhook_callback_verification" => {
            let Some(challenge) = value["challenge"].as_str() else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            if challenge.len() > 500 {
                return StatusCode::BAD_REQUEST.into_response();
            }
            (
                StatusCode::OK,
                [("content-type", "text/plain; charset=utf-8")],
                challenge.to_owned(),
            )
                .into_response()
        }
        "notification" => {
            let event = &value["event"];
            let Some(source) = event["broadcaster_user_login"].as_str() else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            let source = source.to_ascii_lowercase();
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
            if let Some(broadcaster) = value["subscription"]["condition"]["broadcaster_user_id"].as_str()
                && let Err(error) = state.app.db.revoke_eventsub(broadcaster).await
            {
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
    #[test]
    fn signature_uses_raw_body_and_constant_time_comparison() {
        let mut mac = HmacSha256::new_from_slice(b"secret").unwrap();
        mac.update(b"id2026-01-01T00:00:00Z{\"event\":1}");
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(signed("secret", "id", "2026-01-01T00:00:00Z", br#"{"event":1}"#, &sig));
        assert!(!signed("secret", "id", "2026-01-01T00:00:00Z", br#"{"event":2}"#, &sig));
    }
}
