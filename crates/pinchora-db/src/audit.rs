//! The dashboard's audit log: who changed which server setting, when, from where.

use chrono::{DateTime, Utc};

use crate::{Db, sql_id};

/// Where a change was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSource {
    Web,
    Discord,
}

impl AuditSource {
    pub fn key(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Discord => "discord",
        }
    }
}

/// A change to record. `before` and `after` are JSON objects of the changed fields only.
#[derive(Debug, Clone)]
pub struct NewAudit<'a> {
    pub guild: u64,
    pub actor: u64,
    /// The actor's name as shown when the change was made.
    pub actor_name: &'a str,
    pub source: AuditSource,
    /// What was done, e.g. `settings.patch`.
    pub action: &'a str,
    /// What it was done to, e.g. a role id; `None` for the server itself.
    pub target: Option<&'a str>,
    pub before: Option<&'a str>,
    pub after: Option<&'a str>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditRow {
    pub id: i64,
    pub guild_id: i64,
    pub actor_id: i64,
    pub actor_name: String,
    pub source: String,
    pub action: String,
    pub target: Option<String>,
    /// JSON text.
    pub before: Option<String>,
    /// JSON text.
    pub after: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Db {
    /// Records a change; returns the row's id.
    pub async fn audit(&self, entry: &NewAudit<'_>) -> anyhow::Result<i64> {
        let mut tx = self.pool().begin().await?;
        let id = insert_in(&mut tx, entry).await?;
        tx.commit().await?;
        Ok(id)
    }
}

pub(crate) async fn insert_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    entry: &NewAudit<'_>,
) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO dashboard_audit (guild_id, actor_id, actor_name, source, action, target, before, after)
             VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8::jsonb) RETURNING id",
    )
    .bind(sql_id(entry.guild))
    .bind(sql_id(entry.actor))
    .bind(entry.actor_name)
    .bind(entry.source.key())
    .bind(entry.action)
    .bind(entry.target)
    .bind(entry.before.map(validated_audit_json).transpose()?)
    .bind(entry.after.map(validated_audit_json).transpose()?)
    .fetch_one(&mut **tx)
    .await?)
}

impl Db {
    /// A server's changes, newest first: up to `limit` older than `before_id` (all when `None`).
    pub async fn audit_page(&self, guild: u64, before_id: Option<i64>, limit: i64) -> anyhow::Result<Vec<AuditRow>> {
        Ok(sqlx::query_as(
            "SELECT id, guild_id, actor_id, actor_name, source, action, target,
                    before::text AS before, after::text AS after, created_at
             FROM dashboard_audit
             WHERE guild_id = $1 AND ($2::bigint IS NULL OR id < $2)
             ORDER BY id DESC LIMIT $3",
        )
        .bind(sql_id(guild))
        .bind(before_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Deletes changes made before `cutoff`; returns how many went.
    pub async fn forget_audit_before(&self, cutoff: DateTime<Utc>) -> anyhow::Result<u64> {
        Ok(sqlx::query("DELETE FROM dashboard_audit WHERE created_at < $1")
            .bind(cutoff)
            .execute(self.pool())
            .await?
            .rows_affected())
    }
}

/// Defence in depth for historic rows as well as new audit writers. Search sees the same safe data.
impl AuditRow {
    pub fn redact(&mut self) {
        self.before = self.before.take().map(|s| redacted_json(&s));
        self.after = self.after.take().map(|s| redacted_json(&s));
    }
}

fn validated_audit_json(raw: &str) -> Result<String, serde_json::Error> {
    // Preserve transaction rollback on invalid input; redaction must not turn a
    // malformed audit event into a successful write, even above the display cap.
    serde_json::from_str::<serde::de::IgnoredAny>(raw)?;
    Ok(redacted_json(raw))
}

fn redacted_json(raw: &str) -> String {
    if raw.len() > 65536 {
        return r#"{"truncated":true}"#.into();
    }
    fn clean(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, value) in fields {
                    let key: String = key
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric())
                        .flat_map(char::to_lowercase)
                        .collect();
                    if [
                        "token",
                        "secret",
                        "password",
                        "authorization",
                        "cookie",
                        "apikey",
                        "privatekey",
                        "credential",
                        "webhookurl",
                    ]
                    .iter()
                    .any(|s| key.contains(s))
                    {
                        *value = serde_json::json!("[REDACTED]");
                    } else {
                        clean(value);
                    }
                }
            }
            serde_json::Value::Array(values) => values.iter_mut().for_each(clean),
            serde_json::Value::String(text)
                if text.contains("/api/webhooks/")
                    || text.starts_with("Bearer ")
                    || text.starts_with("Bot ")
                    || pinchora_core::config_backup::redact_url_credentials(text).is_some() =>
            {
                *text = "[REDACTED]".into();
            }
            _ => {}
        }
    }
    let Ok(mut value) = serde_json::from_str(raw) else {
        return "null".into();
    };
    clean(&mut value);
    value.to_string()
}

#[cfg(test)]
mod redaction_tests {
    use super::*;
    #[test]
    fn invalid_audit_writes_fail_before_redaction_or_truncation() {
        assert!(validated_audit_json("invalid JSON").is_err());
        assert!(validated_audit_json(&"x".repeat(65537)).is_err());
        assert_eq!(
            validated_audit_json(r#"{"token":"SECRET"}"#).unwrap(),
            r#"{"token":"[REDACTED]"}"#
        );
    }

    #[test]
    fn nested_credentials_and_webhook_urls_are_never_exposed() {
        let raw = r#"{"access_token":"TOKEN","nested":[{"clientSecret":"SECRET","safe":true}],"url":"https://discord.com/api/webhooks/id/KEY","source":"https://user:PASS@feed.example/?api%5Fkey=QUERY_SECRET"}"#;
        let clean = redacted_json(raw);
        for secret in ["TOKEN", "SECRET", "KEY", "PASS", "QUERY_SECRET"] {
            assert!(!clean.contains(secret));
        }
        assert!(clean.contains("safe"));
        assert_eq!(redacted_json("invalid secret"), "null");
    }
}
