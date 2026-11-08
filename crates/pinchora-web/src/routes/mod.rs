//! The dashboard's routes and the headers every response carries.

mod achievements;
mod anti_nuke;
mod audit_log;
mod automod;
mod birthdays;
mod branding;
mod cards;
mod commands;
mod community;
mod community_pack;
mod community_v2;
mod discord_info;
mod events;
mod feed_links;
mod feed_webhooks;
mod feeds;
mod guilds;
mod house;
mod house_edit;
mod invites;
mod leveling;
mod leveling_parity;
mod me;
mod message_studio;
mod mod_parity;
mod moderation;
mod modules;
mod overview;
mod preview;
mod reef;
mod responders;
mod roblox;
mod roles;
mod settings;
mod site;
mod starboard;
mod static_files;
mod stats;
mod store;
mod tag_common;
mod temp_voice;
mod tickets;
mod trackers;
mod utility;
mod verification;
mod web_player;
mod xp_admin;

use std::time::Instant;

use axum::{
    Router,
    extract::{MatchedPath, Request, State},
    http::{HeaderName, HeaderValue, header},
    middleware::{self, Next},
    response::Response,
    routing::{any, get, post},
};

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
        .route("/api/public/landing", get(site::landing))
        .route("/api/public/status", get(site::status))
        .route("/api/public/leaderboard/{slug}", get(leveling_parity::public_board))
        .nest("/api/public/stats", stats::public_router())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            ratelimit::public_by_address,
        ));
    let api = Router::new()
        .route("/api/me", get(me::me))
        .nest("/api/me", web_player::router())
        .route("/api/me/language", post(me::language))
        .route("/api/me/trackers", get(trackers::export))
        .route("/api/me/guilds", get(me::guilds))
        .route("/api/me/guilds/refresh", post(me::refresh_guilds))
        .nest("/api/guilds/{g}", guilds::router())
        .nest("/api/house", house::router())
        .nest("/api/reef", reef::router())
        .nest("/api/verify", verification::member_router())
        .nest("/api/appeals", mod_parity::member_router())
        .nest("/api/feeds", feed_links::router())
        .route("/api/{*rest}", any(|| async { ApiError::not_found() }));
    Router::new()
        .merge(auth)
        .merge(public)
        .route("/invite", get(site::invite))
        .nest("/hooks", feed_webhooks::router())
        .merge(api)
        .merge(house::assets())
        .merge(static_files::router())
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

mod command_control;
mod config_backup;

#[cfg(test)]
mod csp_tests {
    use super::*;

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
mod reef_loop;
