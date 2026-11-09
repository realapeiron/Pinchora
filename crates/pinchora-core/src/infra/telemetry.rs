pub use crate::infra::request_trace::request as trace_request;

use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use axum::{Router, extract::State, http::StatusCode, routing::get};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tracing_subscriber::{EnvFilter, fmt};

use crate::infra::config::LogConfig;
use crate::infra::config::LogFormat;

pub fn init_tracing(cfg: &LogConfig) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cfg.filter));
    let builder = fmt().with_env_filter(filter);
    match cfg.format {
        LogFormat::Pretty => builder.init(),
        LogFormat::Json => builder.json().flatten_event(true).init(),
    }
}

/// Readiness flag shared with the Kubernetes probe endpoint.
#[derive(Clone, Default)]
pub struct Readiness(Arc<AtomicBool>);

impl Readiness {
    pub fn set(&self, ready: bool) {
        self.0.store(ready, Ordering::Relaxed);
    }

    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

pub fn install_metrics() -> anyhow::Result<PrometheusHandle> {
    let handle = PrometheusBuilder::new().install_recorder()?;
    let upkeep = handle.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            upkeep.run_upkeep();
        }
    });
    Ok(handle)
}

/// Serves `/metrics`, `/healthz` (liveness) and `/readyz` (readiness).
pub async fn serve_ops(bind: SocketAddr, metrics: PrometheusHandle, ready: Readiness) -> anyhow::Result<()> {
    let app = Router::new()
        .route(
            "/metrics",
            get(|State((m, _)): State<(PrometheusHandle, Readiness)>| async move { m.render() }),
        )
        .route("/healthz", get(|| async { "ok" }))
        .route(
            "/readyz",
            get(|State((_, r)): State<(PrometheusHandle, Readiness)>| async move {
                if r.get() {
                    (StatusCode::OK, "ready")
                } else {
                    (StatusCode::SERVICE_UNAVAILABLE, "starting")
                }
            }),
        )
        .with_state((metrics, ready))
        .layer(axum::middleware::from_fn(trace_request));
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "ops server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Resolves on Ctrl-C or SIGTERM (what Kubernetes sends before killing a pod).
pub async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = term => {},
    }
}
