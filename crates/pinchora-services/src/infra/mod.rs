mod clock;
mod job;
mod render;

pub use clock::Clock;
pub use job::Job;
pub use render::PreviewError;

use crate::infra::render::RenderCapacity;
use pinchora_cache::Cache;
use pinchora_db::Db;
use pinchora_render::Renderer;
use std::{future::Future, sync::Arc, time::Duration};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::Instrument;
use twilight_http::Client;

/// Constructed once. Compatibility handles on App clone these clients and pools.
pub struct Infrastructure {
    pub db: Db,
    pub cache: Cache,
    pub http: Arc<Client>,
    pub interaction_http: Arc<Client>,
    pub web: reqwest::Client,
    pub clock: Clock,
    tasks: TaskTracker,
    shutdown: CancellationToken,
    render: RenderCapacity,
}

impl Infrastructure {
    pub fn new(http: Arc<Client>, interaction_http: Arc<Client>, db: Db, cache: Cache) -> Self {
        Self {
            db,
            cache,
            http,
            interaction_http,
            web: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .user_agent("Pinchora (https://github.com/realapeiron/Pinchora)")
                .build()
                .expect("reqwest client"),
            clock: Clock::default(),
            tasks: TaskTracker::new(),
            shutdown: CancellationToken::new(),
            render: RenderCapacity::new(),
        }
    }

    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.tasks.spawn(future.in_current_span());
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    pub async fn drain(&self, limit: Duration) {
        self.shutdown.cancel();
        self.tasks.close();
        let _ = tokio::time::timeout(limit, self.tasks.wait()).await;
    }

    pub async fn render<T, F>(&self, job: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Renderer) -> anyhow::Result<T> + Send + 'static,
    {
        self.render.render(job).await
    }

    pub async fn render_preview<T, F>(&self, job: F) -> Result<T, PreviewError>
    where
        T: Send + 'static,
        F: FnOnce(&Renderer) -> anyhow::Result<T> + Send + 'static,
    {
        self.render.render_preview(job).await
    }
}
