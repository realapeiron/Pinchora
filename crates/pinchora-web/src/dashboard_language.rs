//! Language negotiation shared by dashboard and pre-session routes.

use axum::http::{HeaderMap, header::ACCEPT_LANGUAGE};
use pinchora_core::infra::i18n::Lang;

pub(crate) fn accepted(headers: &HeaderMap) -> Lang {
    headers
        .get(ACCEPT_LANGUAGE)
        .and_then(|value| value.to_str().ok())
        .map(|value| Lang::from_discord(value.split([',', ';']).next().unwrap_or("en").trim()))
        .unwrap_or_default()
}
