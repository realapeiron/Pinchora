//! The dashboard's routes and the headers every response carries.

mod achievements;
mod anti_nuke;
mod audit_log;
mod automod;
mod birthdays;
mod cards;
mod commands;
mod community;
mod discord_info;
mod events;
mod feed_links;
mod feed_webhooks;
mod feeds;
mod guilds;
mod house;
mod invites;
mod leveling;
mod me;
mod moderation;
mod modules;
mod overview;
mod preview;
mod reef;
mod responders;
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
const CSP: &str = "default-src 'self'; img-src 'self' data: blob: https://cdn.discordapp.com; style-src 'self'; \
                   script-src 'self'; connect-src 'self'; font-src 'self'; frame-ancestors 'none'; base-uri 'none'; \
                   form-action 'self'";

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
        .nest("/api/public/stats", stats::public_router())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            ratelimit::public_by_address,
        ));
    let api = Router::new()
        .route("/api/me", get(me::me))
        .route("/api/me/language", post(me::language))
        .route("/api/me/trackers", get(trackers::export))
        .route("/api/me/guilds", get(me::guilds))
        .route("/api/me/guilds/refresh", post(me::refresh_guilds))
        .nest("/api/guilds/{g}", guilds::router())
        .nest("/api/house", house::router())
        .nest("/api/reef", reef::router())
        .nest("/api/verify", verification::member_router())
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
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP));
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
