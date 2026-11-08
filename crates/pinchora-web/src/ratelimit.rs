//! Fixed-window rate limits in Redis, shared by every web pod.

use std::{net::SocketAddr, time::Duration};

use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use pinchora_cache::Cache;

use crate::{error::ApiError, state::WebState};

/// Login attempts per address per minute.
const AUTH_PER_MINUTE: i64 = 10;
/// Reads of the public landing data per address per minute.
const PUBLIC_PER_MINUTE: i64 = 60;

/// Counts one hit on `scope` for `who`; 429 once more than `limit` land in one `window`. When Redis
/// cannot count, the request goes through: a limiter must not take the dashboard down.
pub async fn check(cache: &Cache, scope: &str, who: &str, limit: i64, window: Duration) -> Result<(), ApiError> {
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let span = window.as_secs().max(1);
    let key = format!("web:rl:{scope}:{who}:{}", now / span);
    match cache.incr(&key, window).await {
        Ok(hits) if hits > limit => Err(ApiError::too_many(span - now % span)),
        Ok(_) => Ok(()),
        Err(error) => {
            tracing::warn!(%error, "rate limit not counted");
            Ok(())
        }
    }
}

/// Side-effecting tests must not bypass quotas during a Redis outage.
pub async fn check_strict(cache: &Cache, scope: &str, who: &str, limit: i64, window: Duration) -> Result<(), ApiError> {
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let span = window.as_secs().max(1);
    let hits = cache
        .incr(&format!("web:rl:{scope}:{who}:{}", now / span), window)
        .await
        .map_err(ApiError::internal)?;
    if hits > limit {
        return Err(ApiError::too_many(span - now % span));
    }
    Ok(())
}

/// Who a request comes from: the connection's address, or what a trusted proxy says it is.
fn address_of(state: &WebState, request: &Request) -> String {
    request.extensions().get::<ConnectInfo<SocketAddr>>().map_or_else(
        || "unknown".to_owned(),
        |info| {
            state
                .settings
                .trusted_proxies
                .client(info.0.ip(), request.headers())
                .to_string()
        },
    )
}

/// Limits `/auth/*` per client address: the connection's, or what a trusted proxy says it is.
pub async fn auth_by_address(State(state): State<WebState>, request: Request, next: Next) -> Response {
    if state.settings.dev_login_unlimited && request.uri().path() == "/auth/dev-login" {
        return next.run(request).await;
    }
    let address = address_of(&state, &request);
    match check(
        &state.app.cache,
        "auth",
        &address,
        AUTH_PER_MINUTE,
        Duration::from_secs(60),
    )
    .await
    {
        Ok(()) => next.run(request).await,
        Err(error) => error.into_response(),
    }
}

/// Limits the public, logged-out API (`/api/public/*`) per client address.
pub async fn public_by_address(State(state): State<WebState>, request: Request, next: Next) -> Response {
    let address = address_of(&state, &request);
    match check(
        &state.app.cache,
        "public",
        &address,
        PUBLIC_PER_MINUTE,
        Duration::from_secs(60),
    )
    .await
    {
        Ok(()) => next.run(request).await,
        Err(error) => error.into_response(),
    }
}
