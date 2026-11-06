//! Recording dashboard changes in the audit log.

use pinchora_db::{AuditSource, NewAudit};
use serde_json::Value;

use crate::{auth::session::Session, state::WebState};

/// One change made on the web: `before` and `after` hold only the fields that changed.
pub struct Change<'a> {
    pub guild: u64,
    pub action: &'a str,
    pub target: Option<&'a str>,
    pub before: Value,
    pub after: Value,
}

pub async fn record(state: &WebState, session: &Session, change: Change<'_>) -> anyhow::Result<()> {
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
