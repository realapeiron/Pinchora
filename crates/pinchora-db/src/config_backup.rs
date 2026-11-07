//! Transactional configuration snapshots. All identifiers originate in the static manifest.
use crate::{Db, sql_id};
use pinchora_core::{
    config_backup::{self, Backup, Names, Policy, SECTIONS, Section},
    i18n::Lang,
};
use serde_json::{Value, json};
use sqlx::{PgConnection, Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};

fn scope(section: &Section) -> &'static str {
    match section.table {
        "guilds" => "t.id=$1",
        "role_panel_roles" | "panel_reactions" => "t.panel_id IN (SELECT id FROM role_panels WHERE guild_id=$1)",
        _ => "t.guild_id=$1",
    }
}
fn id_strings(value: &mut Value, field: &str) {
    let is_id = field == "id"
        || field.ends_with("_id")
        || field == "required_role"
        || field.contains("roles")
        || field.contains("channels");
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                id_strings(v, key);
            }
        }
        Value::Array(values) => {
            for v in values {
                id_strings(v, field);
            }
        }
        Value::Number(n) if is_id => *value = Value::String(n.to_string()),
        _ => {}
    }
}
async fn snapshot(conn: &mut PgConnection, guild: u64, names: Names) -> anyhow::Result<Backup> {
    let mut sections = BTreeMap::new();
    for section in SECTIONS {
        let columns = if section.serial {
            format!("id,{}", section.columns)
        } else {
            section.columns.into()
        };
        let fields = columns
            .split(',')
            .map(|c| format!("'{c}',t.{c}"))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "SELECT jsonb_build_object({fields}) FROM {} t WHERE {}",
            section.table,
            scope(section)
        );
        let mut rows: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str()))
            .bind(sql_id(guild))
            .fetch_all(&mut *conn)
            .await?;
        if section.table == "guilds" && rows.is_empty() {
            let defaults: Vec<(String, Option<String>)> = sqlx::query_as("SELECT column_name,column_default FROM information_schema.columns WHERE table_schema='public' AND table_name='guilds'").fetch_all(&mut *conn).await?;
            // Schema-owned defaults produce a portable empty-server configuration without INSERTs.
            let defaults: BTreeMap<_, _> = defaults.into_iter().collect();
            let fields = section
                .columns
                .split(',')
                .map(|c| format!("'{c}',{}", defaults.get(c).and_then(Option::as_deref).unwrap_or("NULL")))
                .collect::<Vec<_>>()
                .join(",");
            let query = format!("SELECT jsonb_build_object({fields})");
            rows.push(
                sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str()))
                    .fetch_one(&mut *conn)
                    .await?,
            );
        }
        for row in &mut rows {
            id_strings(row, "");
        }
        rows.sort_by_key(|row| config_backup::row_key(section, row));
        sections.insert(section.table.to_owned(), rows);
    }
    Ok(Backup {
        version: config_backup::VERSION,
        source_guild: guild.to_string(),
        names,
        sections,
    })
}
impl Db {
    /// PostgreSQL's own column type decoding, in a read-only transaction (no sequences or triggers).
    pub async fn validate_backup_types(&self, backup: &Backup) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SET TRANSACTION READ ONLY").execute(&mut *tx).await?;
        for section in SECTIONS {
            let constraints: Vec<String> = sqlx::query_scalar(
                "SELECT pg_get_expr(conbin,conrelid) FROM pg_constraint WHERE conrelid=$1::regclass AND contype='c'",
            )
            .bind(section.table)
            .fetch_all(&mut *tx)
            .await?;
            let required: Vec<String> = sqlx::query_scalar("SELECT column_name FROM information_schema.columns WHERE table_schema='public' AND table_name=$1 AND is_nullable='NO'").bind(section.table).fetch_all(&mut *tx).await?;
            for row in &backup.sections[section.table] {
                for column in &required {
                    if section.columns.split(',').any(|c| c == column) {
                        anyhow::ensure!(!row[column].is_null(), "required field {column}");
                    }
                }
                for constraint in &constraints {
                    // Expression comes only from PostgreSQL's schema, never from the document.
                    let query = format!(
                        "SELECT ({constraint}) IS NOT FALSE FROM jsonb_populate_record(NULL::{},$1) r",
                        section.table
                    );
                    let valid: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str()))
                        .bind(row)
                        .fetch_one(&mut *tx)
                        .await?;
                    anyhow::ensure!(valid, "invalid {} constraint", section.table);
                }

                let query = format!(
                    "SELECT to_jsonb(r) FROM jsonb_populate_record(NULL::{},$1) r",
                    section.table
                );
                let _: Value = sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str()))
                    .bind(row)
                    .fetch_one(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Public backup document: credentials embedded in feed URLs never leave the server.
    pub async fn export_config(&self, guild: u64, names: Names) -> anyhow::Result<Backup> {
        let mut backup = self.snapshot_config(guild, names).await?;
        config_backup::redact_feed_credentials(&mut backup);
        Ok(backup)
    }

    /// Internal comparison state. Keep live values intact so previews report redaction as a change.
    pub async fn snapshot_config(&self, guild: u64, names: Names) -> anyhow::Result<Backup> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let backup = snapshot(&mut tx, guild, names).await?;
        tx.commit().await?;
        Ok(backup)
    }
    pub async fn config_restore_points(&self, guild: u64) -> anyhow::Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',id::text,'created_at',created_at) FROM config_restore_points WHERE guild_id=$1 ORDER BY id DESC LIMIT 10")
            .bind(sql_id(guild)).fetch_all(self.pool()).await?)
    }
    pub async fn config_restore_point(&self, guild: u64, id: i64) -> anyhow::Result<Option<Backup>> {
        let value: Option<Value> =
            sqlx::query_scalar("SELECT snapshot FROM config_restore_points WHERE guild_id=$1 AND id=$2")
                .bind(sql_id(guild))
                .bind(id)
                .fetch_optional(self.pool())
                .await?;
        let mut backup: Option<Backup> = value.map(serde_json::from_value).transpose()?;
        if let Some(backup) = &mut backup {
            config_backup::redact_feed_credentials(backup);
        }
        Ok(backup)
    }
    /// The caller validates with editor validators first. The lock serializes imports and the restore
    /// point and writes commit together. No transaction writes are used by dry runs.
    pub async fn import_config(
        &self,
        guild: u64,
        backup: &Backup,
        policy: Policy,
        names: Names,
    ) -> anyhow::Result<i64> {
        config_backup::validate_shape(backup, Lang::En).map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            backup.source_guild == guild.to_string(),
            "backup must be remapped before import"
        );
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO guilds(id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM guilds WHERE id=$1 FOR UPDATE")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        let before = snapshot(&mut tx, guild, names).await?;
        let restore: i64 =
            sqlx::query_scalar("INSERT INTO config_restore_points(guild_id,snapshot) VALUES($1,$2) RETURNING id")
                .bind(sql_id(guild))
                .bind(serde_json::to_value(&before)?)
                .fetch_one(&mut *tx)
                .await?;
        let mut ids: BTreeMap<(String, String), Value> = BTreeMap::new();
        let mut retained: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for section in SECTIONS {
            if section.table == "color_roles" {
                // The palette also has a unique (guild, hex) index. Remove incoming assignments
                // together before inserting them, so swapping two colours never conflicts halfway.
                sqlx::query("DELETE FROM color_roles WHERE guild_id=$1 AND ($2 OR role_id IN (SELECT role_id FROM jsonb_populate_recordset(NULL::color_roles,$3)))")
                    .bind(sql_id(guild))
                    .bind(policy == Policy::Replace)
                    .bind(serde_json::to_value(&backup.sections[section.table])?)
                    .execute(&mut *tx)
                    .await?;
            }
            for original in &backup.sections[section.table] {
                let mut row = original.clone();
                if let Some(map) = row.as_object_mut() {
                    for (field, parent) in [("panel_id", "role_panels"), ("form_id", "ticket_forms")] {
                        if let Some(old) = map.get(field).filter(|v| !v.is_null()) {
                            let mapped = ids
                                .get(&(parent.into(), old.to_string()))
                                .ok_or_else(|| anyhow::anyhow!("unresolved {field}"))?
                                .clone();
                            map.insert(field.into(), mapped);
                        }
                    }
                }
                if section.table == "feeds" {
                    sqlx::query("INSERT INTO feed_sources(kind,source) VALUES($1,$2) ON CONFLICT DO NOTHING")
                        .bind(row["kind"].as_str())
                        .bind(row["source"].as_str())
                        .execute(&mut *tx)
                        .await?;
                }
                let key = config_backup::row_key(section, &row);
                retained.entry(section.table.into()).or_default().insert(key.clone());
                let old = before.sections[section.table]
                    .iter()
                    .find(|old| section.table != "color_roles" && config_backup::row_key(section, old) == key);
                let new_id = write_row(&mut tx, guild, section, &row, old).await?;
                if section.serial {
                    ids.insert((section.table.into(), original["id"].to_string()), new_id);
                }
            }
        }
        if policy == Policy::Replace {
            // Children first; never delete a kept parent just to recreate it (live ticket links survive).
            for section in SECTIONS.iter().rev().filter(|s| s.table != "guilds") {
                for row in &before.sections[section.table] {
                    if retained
                        .get(section.table)
                        .is_some_and(|keys| keys.contains(&config_backup::row_key(section, row)))
                    {
                        continue;
                    }
                    let predicate = key_predicate(section);
                    let query = format!(
                        "DELETE FROM {} t WHERE {} AND {predicate}",
                        section.table,
                        scope(section)
                    );
                    sqlx::query(sqlx::AssertSqlSafe(query.as_str()))
                        .bind(sql_id(guild))
                        .bind(row)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }
        sqlx::query("UPDATE feed_sources s SET subscribers=(SELECT count(*) FROM feeds f WHERE f.kind=s.kind AND f.source=s.source AND f.enabled)").execute(&mut *tx).await?;
        sqlx::query("UPDATE automod_settings SET revision=revision+1,sync_generation=sync_generation+1,sync_after=now() WHERE guild_id=$1")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE invite_settings SET baseline_required=TRUE WHERE guild_id=$1")
            .bind(sql_id(guild))
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM config_restore_points WHERE guild_id=$1 AND id NOT IN (SELECT id FROM config_restore_points WHERE guild_id=$1 ORDER BY id DESC LIMIT 10)").bind(sql_id(guild)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(restore)
    }
}
fn key_predicate(section: &Section) -> String {
    let columns = if section.serial { "id" } else { section.key };
    let terms: Vec<_> = columns
        .split(',')
        .filter(|k| !k.is_empty())
        .map(|k| {
            format!(
                "t.{k} IS NOT DISTINCT FROM (jsonb_populate_record(NULL::{},$2)).{k}",
                section.table
            )
        })
        .collect();
    if terms.is_empty() {
        "TRUE".into()
    } else {
        terms.join(" AND ")
    }
}
async fn write_row(
    tx: &mut Transaction<'_, Postgres>,
    guild: u64,
    section: &Section,
    row: &Value,
    old: Option<&Value>,
) -> anyhow::Result<Value> {
    let mut payload = row.clone();
    let map = payload.as_object_mut().unwrap();
    map.insert(
        if section.table == "guilds" { "id" } else { "guild_id" }.into(),
        json!(guild.to_string()),
    );
    // No authors are transferred. New editor records use the existing privacy tombstone convention.
    if matches!(section.table, "custom_commands" | "custom_responders") {
        map.insert("created_by".into(), json!(0));
    }
    if let Some(old) = old {
        if section.serial {
            map.insert("id".into(), old["id"].clone());
        }
        let mut assignments = section
            .columns
            .split(',')
            .map(|c| format!("{c}=r.{c}"))
            .collect::<Vec<_>>()
            .join(",");
        if section.table == "scheduled_messages" {
            assignments.push_str(",ping_allowed=false,created_by=NULL");
        }
        let query = format!(
            "UPDATE {} t SET {assignments} FROM jsonb_populate_record(NULL::{},$2) r WHERE {} AND {}",
            section.table,
            section.table,
            scope(section),
            key_predicate(section)
        );
        sqlx::query(sqlx::AssertSqlSafe(query.as_str()))
            .bind(sql_id(guild))
            .bind(&payload)
            .execute(&mut **tx)
            .await?;
        return Ok(old.get("id").cloned().unwrap_or(Value::Null));
    }
    let mut columns = section.columns.to_owned();
    if !matches!(section.table, "role_panel_roles" | "panel_reactions") {
        columns.push_str(if section.table == "guilds" { ",id" } else { ",guild_id" });
    }
    if matches!(section.table, "custom_commands" | "custom_responders") {
        columns.push_str(",created_by");
    }
    let returning = if section.serial { "id::text" } else { "NULL::text" };
    let query = format!(
        "INSERT INTO {} ({columns}) SELECT {columns} FROM jsonb_populate_record(NULL::{},$1) RETURNING {returning}",
        section.table, section.table
    );
    let id: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str()))
        .bind(&payload)
        .fetch_one(&mut **tx)
        .await?;
    Ok(id.map(Value::String).unwrap_or(Value::Null))
}
