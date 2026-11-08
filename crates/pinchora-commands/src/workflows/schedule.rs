use crate::App;
use pinchora_core::workflows::Event;
use std::{sync::Arc, time::Duration};
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(error) = tick(&app).await {
                tracing::warn!(%error,"workflow scheduler failed");
            }
        }
    });
}
pub async fn tick(app: &Arc<App>) -> anyhow::Result<()> {
    app.db.prune_workflow_runs().await?;
    for row in app.db.workflow_due().await? {
        if let Err(error) = super::engine::run(
            app,
            &row,
            Event {
                kind: "schedule".into(),
                ..Default::default()
            },
        )
        .await
        {
            tracing::warn!(workflow=row.id,%error,"scheduled workflow failed");
        }
    }
    Ok(())
}
