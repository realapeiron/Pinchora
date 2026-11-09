use crate::infra::Infrastructure;
use std::{future::Future, time::Instant};
use tracing::Instrument;

/// Metrics wrap the existing claim/lease policy; a ticker need not acquire a second lock.
pub struct Job {
    kind: &'static str,
}

impl Job {
    pub fn new(kind: &'static str) -> Self {
        Self { kind }
    }

    pub async fn run<T>(&self, work: impl Future<Output = anyhow::Result<T>>) -> anyhow::Result<T> {
        let mut observation = Observation::new(self.kind);
        let result = work.await;
        observation.outcome = if result.is_ok() { "ok" } else { "error" };
        result
    }

    /// The tracked owner finishes and awaits rollback even if its caller stops waiting.
    pub async fn leased<T, F>(&self, infrastructure: &Infrastructure, key: String, work: F) -> anyhow::Result<Option<T>>
    where
        T: Send + 'static,
        F: Future<Output = anyhow::Result<T>> + Send + 'static,
    {
        let db = infrastructure.db.clone();
        let kind = self.kind;
        let finished = infrastructure
            .tasks
            .spawn(
                async move {
                    let mut observation = Observation::new(kind);
                    let mut connection = db.pool().acquire().await?;
                    let result = pinchora_db::run_job_lease(&mut connection, &key, work).await;
                    observation.outcome = match &result {
                        Ok(Some(_)) => "ok",
                        Ok(None) => "skipped",
                        Err(_) => "error",
                    };
                    result
                }
                .in_current_span(),
            )
            .await;
        match finished {
            Ok(result) => result,
            Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
            Err(error) => Err(error.into()),
        }
    }
}

struct Observation {
    kind: &'static str,
    outcome: &'static str,
    started: Instant,
}

impl Observation {
    fn new(kind: &'static str) -> Self {
        Self {
            kind,
            outcome: "error",
            started: Instant::now(),
        }
    }
}

impl Drop for Observation {
    fn drop(&mut self) {
        metrics::counter!("pinchora_job_runs_total", "kind" => self.kind, "outcome" => self.outcome).increment(1);
        metrics::histogram!("pinchora_job_seconds", "kind" => self.kind, "outcome" => self.outcome)
            .record(self.started.elapsed().as_secs_f64());
    }
}
