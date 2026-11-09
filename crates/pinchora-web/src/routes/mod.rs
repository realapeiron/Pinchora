//! The dashboard's routes and the headers every response carries.

use std::time::Instant;

use axum::Router;
use axum::extract::MatchedPath;
use axum::extract::Request;
use axum::extract::State;
use axum::http::HeaderName;
use axum::http::HeaderValue;
use axum::http::header;
use axum::middleware;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::any;
use axum::routing::get;
use axum::routing::post;

use crate::{auth::oauth, error::ApiError, ratelimit, state::WebState};

/// Scripts, styles and data only from this origin; avatars and server icons from Discord's CDN.
const CSP: &str = "default-src 'self'; img-src 'self' data: blob: https:; style-src 'self'; \
                   script-src 'self'; worker-src 'self'; connect-src 'self'; font-src 'self'; frame-ancestors 'none'; base-uri 'none'; \
                   form-action 'self'";

/// WebSocket schemes are not covered by `'self'` in every browser. Derive the one allowed
/// socket origin from startup configuration, never from a request's Host/forwarded headers.
pub(crate) fn content_security_policy(origin: &str) -> HeaderValue {
    let socket = reqwest::Url::parse(origin).ok().and_then(|mut url| {
        let scheme = match url.scheme() {
            "https" => "wss",
            "http" => "ws",
            _ => return None,
        };
        url.set_scheme(scheme).ok()?;
        Some(url.origin().ascii_serialization())
    });
    let policy = socket.map_or_else(
        || CSP.to_owned(),
        |socket| CSP.replace("connect-src 'self';", &format!("connect-src 'self' {socket};")),
    );
    HeaderValue::from_str(&policy).unwrap_or_else(|_| HeaderValue::from_static(CSP))
}

pub fn router(state: WebState) -> Router {
    let auth = Router::new()
        .route("/auth/login", get(oauth::login))
        .route("/auth/callback", get(oauth::callback))
        .route("/auth/logout", post(oauth::logout))
        .route("/auth/dev-login", post(oauth::dev_login))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            ratelimit::auth_by_address,
        ));
    let public = Router::new()
        .route("/api/public/landing", get(crate::routes::admin::site::landing))
        .route("/api/public/status", get(crate::routes::admin::site::status))
        .route(
            "/api/public/leaderboard/{slug}",
            get(crate::routes::progression::leveling_policy::public_board),
        )
        .nest("/api/public/stats", crate::routes::progression::stats::public_router())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            ratelimit::public_by_address,
        ));
    let api = Router::new()
        .route("/api/me", get(crate::routes::infra::me::me))
        .nest("/api/me", crate::routes::games::web_player::router())
        .nest("/api/me/economy", crate::routes::economy::player::player_router())
        .route("/api/me/language", post(crate::routes::infra::me::language))
        .route("/api/me/trackers", get(crate::routes::community::trackers::export))
        .route("/api/me/guilds", get(crate::routes::infra::me::guilds))
        .route("/api/me/guilds/refresh", post(crate::routes::infra::me::refresh_guilds))
        .nest(
            "/api/guilds/{g}",
            crate::routes::admin::guilds::router().route_layer(middleware::from_fn_with_state(
                state.clone(),
                crate::dashboard_access::guard,
            )),
        )
        .nest("/api/house", crate::routes::house::house::router())
        .nest("/api/reef", crate::routes::games::reef::router())
        .nest("/api/verify", crate::routes::moderation::verification::member_router())
        .nest("/api/appeals", crate::routes::moderation::policy::member_router())
        .nest("/api/feeds", crate::routes::integrations::feed_links::router())
        .route("/api/{*rest}", any(|| async { ApiError::not_found() }));
    Router::new()
        .merge(auth)
        .merge(public)
        .route("/invite", get(crate::routes::admin::site::invite))
        .nest("/hooks", crate::routes::integrations::feed_webhooks::router())
        .merge(api)
        .merge(crate::routes::house::house::assets())
        .merge(crate::routes::admin::static_files::router())
        .layer(middleware::from_fn_with_state(state.clone(), headers))
        .with_state(state)
}

/// Browsers that saw the dashboard over HTTPS keep to HTTPS for a year.
const HSTS: &str = "max-age=31536000; includeSubDomains";

/// Security headers on everything (HSTS too when cookies are `Secure`, i.e. served over HTTPS),
/// `no-store` on the API, and request metrics.
async fn headers(State(state): State<WebState>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("static", |p| p.as_str())
        .to_owned();
    let dynamic = request.uri().path().starts_with("/api/") || request.uri().path().starts_with("/auth/");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_SECURITY_POLICY, state.csp.clone());
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );
    if dynamic && !headers.contains_key(header::CACHE_CONTROL) {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    if state.settings.cookie_secure {
        headers.insert(header::STRICT_TRANSPORT_SECURITY, HeaderValue::from_static(HSTS));
    }
    let status = response.status().as_u16().to_string();
    metrics::counter!("pinchora_web_requests_total", "route" => route.clone(), "status" => status).increment(1);
    metrics::histogram!("pinchora_web_request_seconds", "route" => route).record(started.elapsed().as_secs_f64());
    response
}

#[cfg(test)]
mod csp_tests {
    use crate::routes::*;

    #[test]
    fn reef_workers_images_and_websockets_keep_a_narrow_policy() {
        for (origin, socket) in [
            ("http://localhost:8090", "ws://localhost:8090"),
            ("https://reef.example", "wss://reef.example"),
            ("https://reef.example:8443", "wss://reef.example:8443"),
            ("http://[::1]:8090", "ws://[::1]:8090"),
        ] {
            let policy = content_security_policy(origin);
            let policy = policy.to_str().unwrap();
            assert!(policy.contains(&format!("connect-src 'self' {socket};")));
            assert!(policy.contains("worker-src 'self';"));
            assert!(policy.contains("img-src 'self' data: blob: https:;"));
            assert!(policy.contains("script-src 'self';"));
            assert!(!policy.contains("unsafe-") && !policy.contains('*'));
        }
        for invalid in ["null", "data:text/plain,foo", "https://reef.example\r\n; script-src *"] {
            assert_eq!(content_security_policy(invalid), HeaderValue::from_static(CSP));
        }
    }
}

pub mod admin;
pub mod community;
pub mod economy;
pub mod games;
pub mod house;
pub mod infra;
pub mod integrations;
pub mod moderation;
pub mod progression;
pub mod roles;
