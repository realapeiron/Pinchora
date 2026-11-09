use pinchora_render::Renderer;
use std::{sync::Arc, time::Duration};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// How many dashboard previews are drawn (or waiting for a render slot) at once, at most: a small share of the
/// render slots, so an editor being dragged around cannot crowd the member events' cards out.
const PREVIEW_SLOTS: usize = 2;
/// How long a preview waits for its turn before the dashboard is told the bot is busy.
const PREVIEW_WAIT: Duration = Duration::from_secs(3);
/// How long a preview may take from its turn to its picture. A card is within the card design's cost cap, which
/// draws in well under a second; this is for a starved or stuck pod.
const PREVIEW_BUDGET: Duration = Duration::from_secs(6);

/// Why a dashboard preview was not drawn.
#[derive(Debug)]
pub enum PreviewError {
    /// All the preview slots stayed taken for [`PREVIEW_WAIT`].
    Busy,
    /// The drawing did not finish within [`PREVIEW_BUDGET`]. The job still runs to its end (it cannot be
    /// interrupted) and keeps its slots until then.
    TooSlow,
    Failed(anyhow::Error),
}

impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("the preview slots are all taken"),
            Self::TooSlow => f.write_str("the preview took too long to draw"),
            Self::Failed(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for PreviewError {}

impl From<anyhow::Error> for PreviewError {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed(error)
    }
}

pub(crate) struct RenderCapacity {
    renderer: Arc<Renderer>,
    render_slots: Arc<Semaphore>,
    preview_slots: Arc<Semaphore>,
}

impl RenderCapacity {
    pub(crate) fn new() -> Self {
        let cpus = std::thread::available_parallelism().map_or(2, usize::from);
        Self {
            renderer: Arc::new(Renderer::new()),
            render_slots: Arc::new(Semaphore::new(cpus)),
            preview_slots: Arc::new(Semaphore::new(PREVIEW_SLOTS.min(cpus))),
        }
    }
    /// Runs a render job on the blocking pool, at most one per CPU at a time.
    pub async fn render<T, F>(&self, job: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Renderer) -> anyhow::Result<T> + Send + 'static,
    {
        self.render_holding(None, job).await
    }

    /// [`RenderCapacity::render`] for the dashboard's previews: they share a few preview slots, so however many are asked
    /// for, at most that many wait among the member events' cards for a render slot; one that waits too long
    /// for its turn, or takes too long to draw, gives up with an error the dashboard can show.
    pub async fn render_preview<T, F>(&self, job: F) -> Result<T, PreviewError>
    where
        T: Send + 'static,
        F: FnOnce(&Renderer) -> anyhow::Result<T> + Send + 'static,
    {
        let turn = match tokio::time::timeout(PREVIEW_WAIT, self.preview_slots.clone().acquire_owned()).await {
            Ok(turn) => turn.map_err(anyhow::Error::from)?,
            Err(_) => {
                metrics::counter!("pinchora_preview_refused_total", "why" => "busy").increment(1);
                return Err(PreviewError::Busy);
            }
        };
        match tokio::time::timeout(PREVIEW_BUDGET, self.render_holding(Some(turn), job)).await {
            Ok(drawn) => Ok(drawn?),
            Err(_) => {
                metrics::counter!("pinchora_preview_refused_total", "why" => "slow").increment(1);
                Err(PreviewError::TooSlow)
            }
        }
    }

    /// The slots (`turn`, if any, and a render slot) go with the job to the blocking pool and are given back
    /// when it ends, even if whoever waits for it has given up.
    async fn render_holding<T, F>(&self, turn: Option<OwnedSemaphorePermit>, job: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Renderer) -> anyhow::Result<T> + Send + 'static,
    {
        let slot = self.render_slots.clone().acquire_owned().await?;
        let renderer = self.renderer.clone();
        let started = std::time::Instant::now();
        let out = tokio::task::spawn_blocking(move || {
            let out = job(&renderer);
            drop((turn, slot));
            out
        })
        .await??;
        metrics::histogram!("pinchora_render_seconds").record(started.elapsed().as_secs_f64());
        Ok(out)
    }
}
