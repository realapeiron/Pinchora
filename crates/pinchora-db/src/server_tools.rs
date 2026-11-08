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

impl Db {
    pub async fn filtered_audit_page(
        &self,
        guild: u64,
        before: Option<i64>,
        limit: i64,
        filter: &AuditFilter,
    ) -> anyhow::Result<Vec<AuditRow>> {
        Ok(sqlx::query_as(
            "SELECT id, guild_id, actor_id, actor_name, source, action, target, before::text AS before, after::text AS after, created_at
             FROM dashboard_audit WHERE guild_id=$1 AND ($2::bigint IS NULL OR id<$2)
             AND ($3::bigint IS NULL OR actor_id=$3)
             AND ($4::text IS NULL OR split_part(action,'.',1)=$4)
             AND ($5::timestamptz IS NULL OR created_at >= $5)
             AND ($6::timestamptz IS NULL OR created_at < $6)
             AND ($7::text IS NULL OR position(lower($7) in lower(concat_ws(' ',actor_name,action,target,before::text,after::text))) > 0)
             ORDER BY id DESC LIMIT $8")
            .bind(sql_id(guild)).bind(before).bind(filter.actor).bind(&filter.area).bind(filter.from).bind(filter.until).bind(&filter.text)
            .bind(limit.clamp(1, 501)).fetch_all(self.pool()).await?)
    }
}
