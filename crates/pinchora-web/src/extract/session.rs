use std::time::Duration;

use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{
    auth::{
        csrf,
        session::{self, SESSION_COOKIE, Session},
    },
    error::ApiError,
    ratelimit,
    state::WebState,
};

/// Reads per session per minute.
const READS_PER_MINUTE: i64 = 300;
/// Writes per session per minute.
const WRITES_PER_MINUTE: i64 = 60;

/// A server-assigned write budget for routes with finer-grained operation limits.
/// Authentication and CSRF checks still run before any quota is counted.
#[derive(Clone, Copy)]
pub(crate) struct SessionWriteLimit {
    pub scope: &'static str,
    pub per_minute: i64,
}

/// The logged-in user. Rejects with 401 without a live session, with 403 `csrf` on a write that
/// fails the CSRF check, and with 429 past the session's rate limit.
#[derive(Clone)]
pub struct CurrentSession {
    pub sid: String,
    pub session: Session,
}

impl FromRequestParts<WebState> for CurrentSession {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &WebState) -> Result<Self, Self::Rejection> {
        let sid = session::cookie(&parts.headers, SESSION_COOKIE)
            .ok_or_else(ApiError::unauthorized)?
            .to_owned();
        let now = chrono::Utc::now().timestamp();
        let session = session::load(&state.app.cache, &sid, now)
            .await?
            .ok_or_else(ApiError::unauthorized)?;
        csrf::check(&state.settings, parts, &session)?;
        let (scope, limit) = if parts.method.is_safe() {
            ("read", READS_PER_MINUTE)
        } else if let Some(limit) = parts.extensions.get::<SessionWriteLimit>() {
            (limit.scope, limit.per_minute)
        } else {
            ("write", WRITES_PER_MINUTE)
        };
        ratelimit::check(
            &state.app.cache,
            scope,
            &session.user_id.to_string(),
            limit,
            Duration::from_secs(60),
        )
        .await?;
        Ok(Self { sid, session })
    }
}
