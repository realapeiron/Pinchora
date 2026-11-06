//! The second lock on writes, after SameSite=Lax: a header only the dashboard's own script can set,
//! plus `Sec-Fetch-Site` and `Origin` when the browser sends them.

use axum::http::{Method, request::Parts};

use crate::{auth::session::Session, config::WebSettings, error::ApiError};

pub const HEADER: &str = "x-csrf-token";

/// Lets reads through; a write must carry the session's token and come from our own origin.
pub fn check(settings: &WebSettings, parts: &Parts, session: &Session) -> Result<(), ApiError> {
    if matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return Ok(());
    }
    let header = |name: &str| parts.headers.get(name).and_then(|v| v.to_str().ok());
    if let Some(site) = header("sec-fetch-site")
        && !matches!(site, "same-origin" | "none")
    {
        return Err(ApiError::csrf());
    }
    if let Some(origin) = header("origin")
        && origin != settings.origin
    {
        return Err(ApiError::csrf());
    }
    match header(HEADER) {
        Some(token) if same(token.as_bytes(), session.csrf.as_bytes()) => Ok(()),
        _ => Err(ApiError::csrf()),
    }
}

/// Compares in time that does not depend on where the first difference is.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_needs_every_byte() {
        assert!(same(b"abc", b"abc"));
        assert!(!same(b"abc", b"abd"));
        assert!(!same(b"abc", b"ab"));
        assert!(!same(b"", b"a"));
    }
}
