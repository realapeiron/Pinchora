//! Drops cached settings the moment the dashboard changes them.

use std::{sync::Arc, time::Duration};

use futures_util::StreamExt;
use pinchora_cache::Invalidation;

use crate::App;

/// How long to wait before subscribing again after the connection dropped.
const RETRY: Duration = Duration::from_secs(5);

/// Subscribes to the dashboard's change announcements and forgets what they name, so a setting saved
/// on the web counts from the next message. Returns once the first subscription is up (or failed);
/// a dropped one is retried every few seconds, and the caches' own minute bounds staleness meanwhile.
pub async fn listen(app: &Arc<App>) {
    let first = app.cache.invalidations().await;
    let app = app.clone();
    tokio::spawn(async move {
        let mut next = first;
        loop {
            match next {
                Ok(stream) => {
                    let mut stream = std::pin::pin!(stream);
                    while let Some(what) = stream.next().await {
                        forget(&app, what).await;
                    }
                    tracing::warn!("invalidation subscription dropped, resubscribing");
                }
                Err(error) => tracing::warn!(%error, "cannot subscribe to invalidations"),
            }
            tokio::time::sleep(RETRY).await;
            next = app.cache.invalidations().await;
        }
    });
}

async fn forget(app: &App, what: Invalidation) {
    tracing::debug!(?what, "cache invalidated");
    match what {
        Invalidation::House(_) => {}
        Invalidation::Guild(guild) => {
            app.forget_guild_settings(guild).await;
            crate::commands::collection::settings::forget(guild).await;
            crate::commands::achievements::forget(guild).await;
        }
        Invalidation::XpRules(guild) => app.forget_xp_rules(guild).await,
    }
}
