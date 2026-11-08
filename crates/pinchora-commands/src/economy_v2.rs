//! Restart-safe auction settlement and trade expiry, shared by all gateway processes.
use crate::App;
use std::{sync::Arc, time::Duration};
pub fn spawn_ticker(app: Arc<App>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut ticks = 0_u32;
        loop {
            tick.tick().await;
            if ticks.is_multiple_of(120)
                && let Err(error) = app.db.forget_economy_limits(app.now()).await
            {
                tracing::warn!(%error, "economy retention failed");
            }
            ticks = ticks.wrapping_add(1);
            if let Err(error) = app.db.sweep_economy(app.now()).await {
                tracing::warn!(%error,"economy settlement failed");
            }
        }
    });
}
