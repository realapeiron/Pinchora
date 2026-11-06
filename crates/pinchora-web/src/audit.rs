//! Recording dashboard changes in the audit log, and telling the bot about them.

use pinchora_cache::Invalidation;
use pinchora_db::{AuditSource, NewAudit};
use serde_json::Value;

use crate::{auth::session::Session, invalidate, state::WebState};

/// One change made on the web: `before` and `after` hold only the fields that changed.
pub struct Change<'a> {
    pub guild: u64,
    pub action: &'a str,
    pub target: Option<&'a str>,
    pub before: Value,
    pub after: Value,
}

pub async fn record(state: &WebState, session: &Session, change: &Change<'_>) -> anyhow::Result<()> {
    let (before, after) = (change.before.to_string(), change.after.to_string());
    state
        .app
        .db
        .audit(&NewAudit {
            guild: change.guild,
            actor: session.user_id,
            actor_name: session.display_name(),
            source: AuditSource::Web,
            action: change.action,
            target: change.target,
            before: Some(&before),
            after: Some(&after),
        })
        .await?;
    Ok(())
}

/// Finishes an admin write that already happened: the bot hears about it first, whatever else
/// fails, and then the change is recorded. A failed audit insert is logged and counted but does not
/// fail the request, because the change itself is real.
pub async fn finish(state: &WebState, session: &Session, change: Change<'_>, what: Option<Invalidation>) {
    if let Some(what) = what {
        invalidate::publish(state, what).await;
    }
    if let Err(error) = record(state, session, &change).await {
        metrics::counter!("pinchora_web_audit_failures_total").increment(1);
        tracing::error!(
            error = ?error,
            guild = change.guild,
            action = change.action,
            "a dashboard change was made but not written to the audit log"
        );
    }
    tracing::info!(
        guild = change.guild,
        user_id = session.user_id,
        action = change.action,
        "changed on the web"
    );
}
