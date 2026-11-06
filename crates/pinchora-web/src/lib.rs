//! The web dashboard: Discord login, a server picker, and server settings the bot sees at once.
//!
//! Runs as its own role (`pinchora web`) on the same [`App`] the bot uses. Every admin write is
//! checked, written, recorded in the audit log and announced on Redis, so each bot pod drops its
//! cached copy straight away.

mod audit;
mod auth;
mod client_ip;
mod config;
mod discord;
mod error;
mod extract;
mod guild_info;
mod invalidate;
mod ratelimit;
mod routes;
#[cfg(test)]
mod rules;
mod state;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use pinchora_commands::App;
use pinchora_core::{
    Config,
    telemetry::{self, Readiness},
};

pub use config::WebSettings;
pub use state::WebState;

/// How long changes stay in the audit log.
const AUDIT_RETENTION: chrono::Duration = chrono::Duration::days(180);

/// Serves the dashboard until SIGTERM.
pub async fn run(config: &Config, app: Arc<App>, ready: Readiness) -> anyhow::Result<()> {
    let mut settings = WebSettings::resolve(&config.web, config.discord.application_id)?;
    settings.message_content = config.gateway.message_content;
    if !settings.missing.is_empty() {
        tracing::warn!(
            missing = ?settings.missing,
            redirect = %settings.redirect_uri,
            "dashboard login is off until these are set; serving the setup page"
        );
    }
    if config.web.dev_login && !settings.dev_login {
        tracing::warn!("PINCHORA_WEB__DEV_LOGIN is ignored: it only works in debug builds on a loopback bind");
    }
    if settings.dev_login {
        tracing::warn!(bind = %settings.bind, "dev login is on: anyone who can reach this address can sign in as anyone");
    }
    let bind = settings.bind;
    let state = WebState::new(app.clone(), settings);
    tokio::spawn(prune_audit(app.clone()));

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "web dashboard listening");
    ready.set(true);
    let shutdown_ready = ready.clone();
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        telemetry::shutdown_signal().await;
        shutdown_ready.set(false);
    })
    .await?;
    app.drain(Duration::from_secs(10)).await;
    tracing::info!("web dashboard stopped");
    Ok(())
}

/// Every route of the dashboard: `/auth/*`, `/api/*`, the fonts and the SPA.
pub fn router(state: WebState) -> axum::Router {
    routes::router(state)
}

/// Deletes audit rows past their retention once an hour.
async fn prune_audit(app: Arc<App>) {
    let mut tick = tokio::time::interval(Duration::from_secs(3600));
    loop {
        tick.tick().await;
        match app.db.forget_audit_before(chrono::Utc::now() - AUDIT_RETENTION).await {
            Ok(0) => {}
            Ok(gone) => tracing::info!(gone, "pruned old dashboard audit rows"),
            Err(error) => tracing::warn!(%error, "audit prune failed"),
        }
    }
}
