//! Telling the bot a setting changed, so it stops serving its cached copy.

use pinchora_cache::Invalidation;

use crate::state::WebState;

/// Forgets this pod's copy and announces the change to every other pod. The write already happened,
/// so a failed announcement is only logged: the bot's cache runs out within a minute anyway.
pub async fn publish(state: &WebState, what: Invalidation) {
    match what {
        Invalidation::Guild(guild) => state.app.forget_guild_settings(guild).await,
        Invalidation::XpRules(guild) => state.app.forget_xp_rules(guild).await,
        Invalidation::Commands(application) => {
            if application == state.app.application_id.get() {
                state.app.forget_command_registration().await;
            }
        }
    }
    match state.app.cache.publish_invalidation(what).await {
        Ok(listeners) => tracing::debug!(?what, listeners, "invalidation published"),
        Err(error) => tracing::warn!(%error, ?what, "invalidation not published; the bot catches up within a minute"),
    }
}
