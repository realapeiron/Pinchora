//! Scoped dashboard grants and bounded audit searches.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{AuditRow, AuditSource, Db, NewAudit, audit, sql_id};

pub const DASHBOARD_AREAS: &[&str] = &[
    "configuration",
    "roles",
    "safety",
    "community",
    "feeds",
    "stats",
    "audit",
];
pub const MAX_DASHBOARD_MANAGERS: usize = 25;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
#[serde(deny_unknown_fields)]
pub struct DashboardManager {
    pub role_id: i64,
    pub areas: Vec<String>,
}

impl Db {
    pub async fn dashboard_managers(&self, guild: u64) -> anyhow::Result<Vec<DashboardManager>> {
        Ok(
            sqlx::query_as("SELECT role_id, areas FROM dashboard_managers WHERE guild_id=$1 ORDER BY role_id")
                .bind(sql_id(guild))
                .fetch_all(self.pool())
                .await?,
        )
    }

    /// Serialise replacements and their before/after audit together. No-op saves do not create changes.
    pub async fn replace_dashboard_managers(
        &self,
        guild: u64,
        actor: u64,
        name: &str,
        grants: &[DashboardManager],
    ) -> anyhow::Result<()> {
        anyhow::ensure!(grants.len() <= MAX_DASHBOARD_MANAGERS, "too many manager roles");
        let mut next = grants.to_vec();
        next.sort_by_key(|g| g.role_id);
        anyhow::ensure!(
            next.windows(2).all(|w| w[0].role_id != w[1].role_id),
            "duplicate manager role"
        );
        for grant in &mut next {
            anyhow::ensure!(
                grant.role_id > 0 && grant.role_id != sql_id(guild),
                "invalid manager role"
            );
            anyhow::ensure!(
                !grant.areas.is_empty() && grant.areas.iter().all(|a| DASHBOARD_AREAS.contains(&a.as_str())),
                "invalid dashboard area"
            );
            grant.areas.sort();
            grant.areas.dedup();
        }
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('dashboard-access:' || ($1::bigint)::text, 0))")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        let before: Vec<DashboardManager> =
            sqlx::query_as("SELECT role_id, areas FROM dashboard_managers WHERE guild_id=$1 ORDER BY role_id")
                .bind(sql_id(guild))
                .fetch_all(&mut *tx)
                .await?;
        if before != next {
            sqlx::query("DELETE FROM dashboard_managers WHERE guild_id=$1")
                .bind(sql_id(guild))
                .execute(&mut *tx)
                .await?;
            for grant in &next {
                sqlx::query("INSERT INTO dashboard_managers (guild_id, role_id, areas) VALUES ($1,$2,$3)")
                    .bind(sql_id(guild))
                    .bind(grant.role_id)
                    .bind(&grant.areas)
                    .execute(&mut *tx)
                    .await?;
            }
            // Snowflakes remain strings even inside the audit diff consumed by browsers.
            let view = |rows: &[DashboardManager]| {
                serde_json::json!({"roles": rows.iter().map(|r| serde_json::json!({"role_id":r.role_id.to_string(),"areas":r.areas})).collect::<Vec<_>>()}).to_string()
            };
            audit::insert_in(
                &mut tx,
                &NewAudit {
                    guild,
                    actor,
                    actor_name: name,
                    source: AuditSource::Web,
                    action: "access.replace",
                    target: None,
                    before: Some(&view(&before)),
                    after: Some(&view(&next)),
                },
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub actor: Option<i64>,
    pub area: Option<String>,
    pub from: Option<DateTime<Utc>>,
    /// Exclusive end of the time range.
    pub until: Option<DateTime<Utc>>,
    pub text: Option<String>,
}

/// A bounded scan page. `next` advances past inspected rows, including non-matches.
pub struct FilteredAuditPage {
    pub items: Vec<AuditRow>,
    pub next: Option<i64>,
    pub scanned: usize,
}

impl Db {
    pub async fn filtered_audit_page(
        &self,
        guild: u64,
        before: Option<i64>,
        limit: i64,
        filter: &AuditFilter,
    ) -> anyhow::Result<FilteredAuditPage> {
        // The existing (guild_id, id DESC) index bounds work BEFORE arbitrary filters.
        // No search can force a scan of a guild's entire history or another guild's rows.
        let mut tx = self.pool().begin().await?;
        sqlx::query("SET LOCAL statement_timeout = '2s'")
            .execute(&mut *tx)
            .await?;
        let rows: Vec<AuditRow> = sqlx::query_as(
            "SELECT id, guild_id, actor_id, actor_name, source, action, target,
             CASE WHEN octet_length(before::text) <= 65536 THEN before::text ELSE '{\"truncated\":true}' END AS before,
             CASE WHEN octet_length(after::text) <= 65536 THEN after::text ELSE '{\"truncated\":true}' END AS after,
             created_at FROM dashboard_audit WHERE guild_id=$1 AND id<$2 ORDER BY id DESC LIMIT 500",
        )
        .bind(sql_id(guild))
        .bind(before.unwrap_or(i64::MAX))
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let count = rows.len();
        let mut page = FilteredAuditPage {
            items: Vec::new(),
            next: None,
            scanned: 0,
        };
        let text = filter.text.as_ref().map(|s| s.to_lowercase());
        for mut row in rows {
            page.scanned += 1;
            page.next = Some(row.id);
            row.redact();
            if filter.actor.is_some_and(|a| a != row.actor_id)
                || filter
                    .area
                    .as_ref()
                    .is_some_and(|a| row.action.split('.').next() != Some(a.as_str()))
                || filter.from.is_some_and(|d| row.created_at < d)
                || filter.until.is_some_and(|d| row.created_at >= d)
                || text.as_ref().is_some_and(|text| {
                    !format!(
                        "{} {} {} {} {}",
                        row.actor_name,
                        row.action,
                        row.target.as_deref().unwrap_or_default(),
                        row.before.as_deref().unwrap_or_default(),
                        row.after.as_deref().unwrap_or_default()
                    )
                    .to_lowercase()
                    .contains(text)
                })
            {
                continue;
            }
            page.items.push(row);
            if page.items.len() >= limit.clamp(1, 500) as usize {
                break;
            }
        }
        if page.scanned == count && count < 500 {
            page.next = None;
        }
        Ok(page)
    }
}
