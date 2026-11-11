//! Opens moderation cases and records their staff and mod-log entries.

use crate::infra::util::resolved_display_name;
use chrono::{DateTime, Duration, Utc};
use pinchora_core::moderation::moderation::CaseKind;
use pinchora_db::{NewCase, OpenedCase};
use twilight_interactions::command::ResolvedUser;

use super::{post_case, tell};
use crate::App;

/// What an action records, before it has a number.
pub(crate) struct Act {
    pub(crate) kind: CaseKind,
    pub(crate) target: u64,
    pub(crate) target_name: String,
    pub(crate) reason: Option<String>,
    pub(crate) duration: Option<Duration>,
    pub(crate) details: Option<String>,
}

impl Act {
    pub(crate) fn on(kind: CaseKind, target: &ResolvedUser, reason: Option<String>) -> Self {
        Self {
            kind,
            target: target.resolved.id.get(),
            target_name: resolved_display_name(target),
            reason,
            duration: None,
            details: None,
        }
    }

    /// When a temporary ban ends.
    fn expires_at(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.duration.filter(|_| self.kind == CaseKind::Ban).map(|d| now + d)
    }
}

/// Opens the case, tells the member (unless `told` says it was done before the action), posts the
/// mod-log, and returns the case.
pub(crate) async fn record(
    app: &App,
    guild: u64,
    moderator: (u64, &str),
    act: &Act,
    told: Option<bool>,
) -> anyhow::Result<OpenedCase> {
    record_from(app, guild, moderator, act, told, "command").await
}

pub(crate) async fn record_from(
    app: &App,
    guild: u64,
    moderator: (u64, &str),
    act: &Act,
    told: Option<bool>,
    source: &str,
) -> anyhow::Result<OpenedCase> {
    let opened = app
        .db
        .open_case_from(
            &NewCase {
                guild,
                kind: act.kind,
                target: act.target,
                target_name: &act.target_name,
                moderator: moderator.0,
                moderator_name: moderator.1,
                reason: act.reason.as_deref(),
                duration: act.duration,
                expires_at: act.expires_at(app.now()),
                details: act.details.as_deref(),
            },
            source,
        )
        .await?;
    finish_record(app, guild, act, told, opened).await
}

pub(crate) async fn finish_record(
    app: &App, guild: u64, act: &Act, told: Option<bool>, opened: OpenedCase,
) -> anyhow::Result<OpenedCase> {
    let case = &opened.case;
    let told = match told {
        Some(told) => Some(told),
        None if act.kind.tells_member() => Some(tell(app, guild, act).await.is_some()),
        None => None,
    };
    if let Some(told) = told
        && let Err(error) = app.db.set_case_dm(guild, case.number, told).await
    {
        tracing::warn!(%error, guild, "could not note the direct message");
    }
    post_case(app, guild, case).await;
    crate::commands::moderation::policy::appeal_hint(app, guild, case).await;
    metrics::counter!("pinchora_mod_cases_total", "kind" => act.kind.key()).increment(1);
    Ok(opened)
}
