//! API errors: `{ "error": { "code", "message", "field"? } }` with the matching status.

use axum::{
    Json,
    extract::{FromRequest, Request, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header::RETRY_AFTER},
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;
use serde_json::json;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    /// The request field at fault, for forms to point at.
    pub field: Option<&'static str>,
    /// Where to add the bot, when it is missing from the server.
    pub invite_url: Option<String>,
    /// Seconds until a rate limit lifts.
    pub retry_after: Option<u64>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            field: None,
            invite_url: None,
            retry_after: None,
        }
    }

    pub fn malformed(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "malformed", message)
    }

    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "Log in to continue.")
    }

    pub fn session_expired() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "session_expired",
            "Your Discord login ran out. Log in again.",
        )
    }

    pub fn csrf() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "csrf",
            "This request did not come from the dashboard. Reload the page and try again.",
        )
    }

    pub fn not_manager() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "not_manager",
            "You need Manage Server in that server to change its settings.",
        )
    }

    pub fn bot_missing(invite_url: String) -> Self {
        Self {
            invite_url: Some(invite_url),
            ..Self::new(
                StatusCode::FORBIDDEN,
                "bot_missing",
                "Pinchora is not in that server yet.",
            )
        }
    }

    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "Nothing here.")
    }

    pub fn too_many(retry_after: u64) -> Self {
        Self {
            retry_after: Some(retry_after.max(1)),
            ..Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Slow down a little and try again in a moment.",
            )
        }
    }

    pub fn internal(error: anyhow::Error) -> Self {
        tracing::error!(error = ?error, "dashboard request failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Something went wrong on our side. Please try again in a moment.",
        )
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::internal(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut error = json!({ "code": self.code, "message": self.message });
        if let Some(field) = self.field {
            error["field"] = field.into();
        }
        if let Some(url) = &self.invite_url {
            error["invite_url"] = url.as_str().into();
        }
        let mut response = (self.status, Json(json!({ "error": error }))).into_response();
        if let Some(seconds) = self.retry_after {
            response.headers_mut().insert(RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}

/// A JSON body whose rejections are [`ApiError`]s (400 `malformed`) like every other error.
pub struct ApiJson<T>(pub T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(ApiError::malformed(rejection_text(&rejection))),
        }
    }
}

fn rejection_text(rejection: &JsonRejection) -> String {
    match rejection {
        JsonRejection::MissingJsonContentType(_) => "Send JSON (Content-Type: application/json).".to_owned(),
        other => other.body_text(),
    }
}
