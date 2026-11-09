use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use ed25519_dalek::{Signature, VerifyingKey};
use pinchora_commands::{App, Responder, handle_interaction};
use pinchora_core::Config;
use pinchora_core::infra::telemetry;
use pinchora_core::infra::telemetry::Readiness;
use tokio::sync::oneshot;
use twilight_model::{
    application::interaction::{Interaction, InteractionType},
    http::interaction::{InteractionResponse, InteractionResponseType},
};

/// Discord gives 3 s for the first answer; keep a margin for the network.
const BUDGET: Duration = Duration::from_millis(2_000);

struct HttpState {
    app: Arc<App>,
    key: VerifyingKey,
}

pub async fn run(config: &Config, app: Arc<App>, ready: Readiness) -> anyhow::Result<()> {
    let key = parse_public_key(
        config
            .discord
            .public_key
            .as_deref()
            .context("discord.public_key is required for the interactions server")?,
    )?;
    let state = Arc::new(HttpState { app: app.clone(), key });
    let router = Router::new()
        .route("/interactions", post(endpoint))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state)
        .layer(axum::middleware::from_fn(telemetry::trace_request));
    let listener = tokio::net::TcpListener::bind(config.interactions.bind).await?;
    tracing::info!(bind = %config.interactions.bind, "interactions endpoint listening");
    ready.set(true);
    let shutdown_ready = ready.clone();
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            telemetry::shutdown_signal().await;
            shutdown_ready.set(false);
        })
        .await?;
    app.drain(Duration::from_secs(10)).await;
    tracing::info!("interactions endpoint stopped");
    Ok(())
}

pub fn parse_public_key(hex_key: &str) -> anyhow::Result<VerifyingKey> {
    let bytes: [u8; 32] = hex::decode(hex_key.trim())?
        .try_into()
        .map_err(|_| anyhow::anyhow!("public key must be 32 bytes"))?;
    Ok(VerifyingKey::from_bytes(&bytes)?)
}

/// Checks Discord's Ed25519 signature over `timestamp || body`.
pub fn verify(key: &VerifyingKey, headers: &HeaderMap, body: &[u8]) -> bool {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let (Some(signature), Some(timestamp)) = (header("x-signature-ed25519"), header("x-signature-timestamp")) else {
        return false;
    };
    let Some(signature) = hex::decode(signature).ok().and_then(|b| <[u8; 64]>::try_from(b).ok()) else {
        return false;
    };
    let mut message = Vec::with_capacity(timestamp.len() + body.len());
    message.extend_from_slice(timestamp.as_bytes());
    message.extend_from_slice(body);
    key.verify_strict(&message, &Signature::from_bytes(&signature)).is_ok()
}

async fn endpoint(State(state): State<Arc<HttpState>>, headers: HeaderMap, body: Bytes) -> Response {
    if !verify(&state.key, &headers, &body) {
        return (StatusCode::UNAUTHORIZED, "invalid request signature").into_response();
    }
    let interaction: Interaction = match serde_json::from_slice(&body) {
        Ok(interaction) => interaction,
        Err(error) => {
            tracing::warn!(%error, "undecodable interaction");
            return StatusCode::BAD_REQUEST.into_response();
        }
    };
    if interaction.kind == InteractionType::Ping {
        let span =
            tracing::info_span!("discord.interaction", request_id = %interaction.id, guild_id = ?interaction.guild_id);
        return span.in_scope(|| {
            Json(InteractionResponse {
                kind: InteractionResponseType::Pong,
                data: None,
            })
            .into_response()
        });
    }

    let (tx, mut rx) = oneshot::channel();
    let responder = Arc::new(Responder::http(state.app.clone(), &interaction, tx));
    state
        .app
        .spawn(handle_interaction(state.app.clone(), interaction, responder.clone()));
    match tokio::time::timeout(BUDGET, &mut rx).await {
        Ok(Ok(response)) => Json(response).into_response(),
        _ => match responder.defer_on_timeout().await {
            Some(defer) => Json(defer).into_response(),
            // The handler answered between the timeout and the claim.
            None => match rx.try_recv() {
                Ok(response) => Json(response).into_response(),
                Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;
    use ed25519_dalek::{Signer, SigningKey};

    use crate::http::*;

    fn signed(key: &SigningKey, timestamp: &str, body: &[u8]) -> HeaderMap {
        let mut message = timestamp.as_bytes().to_vec();
        message.extend_from_slice(body);
        let signature = key.sign(&message);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-signature-ed25519",
            HeaderValue::from_str(&hex::encode(signature.to_bytes())).unwrap(),
        );
        headers.insert("x-signature-timestamp", HeaderValue::from_str(timestamp).unwrap());
        headers
    }

    #[test]
    fn accepts_valid_and_rejects_tampered_signatures() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let public = parse_public_key(&hex::encode(signing.verifying_key().to_bytes())).unwrap();
        let body = br#"{"type":1}"#;
        let headers = signed(&signing, "1730000000", body);

        assert!(verify(&public, &headers, body));
        assert!(!verify(&public, &headers, br#"{"type":2}"#));
        assert!(!verify(&public, &HeaderMap::new(), body));

        let other = SigningKey::from_bytes(&[8; 32]);
        assert!(!verify(&public, &signed(&other, "1730000000", body), body));
    }
}
