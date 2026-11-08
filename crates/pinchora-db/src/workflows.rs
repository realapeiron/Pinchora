//! Guild-serialized workflow definitions, run reservations and variables.
use crate::{Db, locks, sql_id};
use chrono::{DateTime, Utc};
use pinchora_core::workflows::{Definition, Event, MAX_RUNS, MAX_VARIABLES, MAX_WORKFLOWS, Scope, Trigger};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Postgres, Transaction};
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct WorkflowRow {
    pub id: i64,
    pub guild_id: i64,
    pub name: String,
    pub enabled: bool,
    pub trigger: Value,
    pub conditions: Value,
    pub actions: Value,
    pub runs: i64,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_by: i64,
    pub updated_at: DateTime<Utc>,
}
impl WorkflowRow {
    pub fn definition(&self) -> anyhow::Result<Definition> {
        Ok(Definition {
            name: self.name.clone(),
            enabled: self.enabled,
            trigger: serde_json::from_value(self.trigger.clone())?,
            conditions: serde_json::from_value(self.conditions.clone())?,
            actions: serde_json::from_value(self.actions.clone())?,
        })
    }
    pub fn json(&self) -> Value {
        json!({"id":self.id.to_string(),"name":self.name,"enabled":self.enabled,"trigger":self.trigger,"conditions":self.conditions,"actions":self.actions,"runs":self.runs,"last_run_at":self.last_run_at,"last_error":self.last_error})
    }
}
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WorkflowRun {
    pub id: i64,
    pub guild_id: i64,
    pub workflow_id: i64,
    pub trigger_summary: String,
    pub user_id: i64,
    pub started_at: DateTime<Utc>,
    pub outcome: String,
    pub steps: Value,
}
async fn lock(tx: &mut Transaction<'_, Postgres>, guild: u64) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(sql_id(guild) ^ 0x57464c4f57)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
impl Db {
    pub async fn workflows(&self, guild: u64) -> anyhow::Result<Vec<WorkflowRow>> {
        Ok(
            sqlx::query_as("SELECT * FROM workflows WHERE guild_id=$1 ORDER BY name")
                .bind(sql_id(guild))
                .fetch_all(self.pool())
                .await?,
        )
    }
    pub async fn workflow(&self, guild: u64, id: i64) -> anyhow::Result<Option<WorkflowRow>> {
        Ok(sqlx::query_as("SELECT * FROM workflows WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }
    pub async fn save_workflow(
        &self,
        guild: u64,
        id: Option<i64>,
        def: &Definition,
        author: u64,
    ) -> anyhow::Result<WorkflowRow> {
        def.validate().map_err(anyhow::Error::msg)?;
        self.guild_settings(guild).await?;
        let mut tx = self.pool().begin().await?;
        locks::lock_users(&mut tx, &[sql_id(author)]).await?;
        lock(&mut tx, guild).await?;
        if id.is_none() {
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM workflows WHERE guild_id=$1")
                .bind(sql_id(guild))
                .fetch_one(&mut *tx)
                .await?;
            anyhow::ensure!(n < MAX_WORKFLOWS as i64, "workflow_limit");
        }
        let trigger = serde_json::to_value(&def.trigger)?;
        let conditions = serde_json::to_value(&def.conditions)?;
        let actions = serde_json::to_value(&def.actions)?;
        let row: WorkflowRow = if let Some(id) = id {
            sqlx::query_as("UPDATE workflows SET name=$3,enabled=$4,trigger=$5,conditions=$6,actions=$7,updated_at=now() WHERE guild_id=$1 AND id=$2 RETURNING *")
            .bind(sql_id(guild)).bind(id).bind(&def.name).bind(def.enabled).bind(trigger).bind(conditions).bind(actions).fetch_one(&mut *tx).await?
        } else {
            sqlx::query_as("INSERT INTO workflows(guild_id,name,enabled,trigger,conditions,actions,created_by) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING *")
            .bind(sql_id(guild)).bind(&def.name).bind(def.enabled).bind(trigger).bind(conditions).bind(actions).bind(sql_id(author)).fetch_one(&mut *tx).await?
        };
        if let Trigger::Schedule { spec, tz } = &def.trigger {
            let next = spec.next(Utc::now(), tz).ok_or_else(|| anyhow::anyhow!("schedule"))?;
            sqlx::query("INSERT INTO workflow_schedules(workflow_id,spec,tz,next_run_at) VALUES($1,$2,$3,$4) ON CONFLICT(workflow_id) DO UPDATE SET spec=$2,tz=$3,next_run_at=$4")
            .bind(row.id).bind(serde_json::to_value(spec)?).bind(tz).bind(next).execute(&mut *tx).await?;
        } else {
            sqlx::query("DELETE FROM workflow_schedules WHERE workflow_id=$1")
                .bind(row.id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(row)
    }
    pub async fn delete_workflow(&self, guild: u64, id: i64) -> anyhow::Result<bool> {
        Ok(sqlx::query("DELETE FROM workflows WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .execute(self.pool())
            .await?
            .rows_affected()
            > 0)
    }
    pub async fn toggle_workflow(&self, guild: u64, name: &str) -> anyhow::Result<Option<bool>> {
        Ok(sqlx::query_scalar(
            "UPDATE workflows SET enabled=NOT enabled,updated_at=now() WHERE guild_id=$1 AND name=$2 RETURNING enabled",
        )
        .bind(sql_id(guild))
        .bind(name)
        .fetch_optional(self.pool())
        .await?)
    }
    /// A rolling minute, serialized across processes; attempted runs count even if conditions fail.
    pub async fn claim_workflow_run(&self, guild: u64, id: i64, event: &Event) -> anyhow::Result<Option<i64>> {
        anyhow::ensure!(event.depth == 0, "workflow_depth");
        let mut tx = self.pool().begin().await?;
        let user = event.user_id.parse::<i64>().unwrap_or(0);
        if user > 0 {
            locks::lock_users(&mut tx, &[user]).await?;
        }
        lock(&mut tx, guild).await?;
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM workflow_run_budget WHERE guild_id=$1 AND reserved_at > clock_timestamp()-interval '1 minute'",
        )
        .bind(sql_id(guild))
        .fetch_one(&mut *tx)
        .await?;
        if n >= MAX_RUNS {
            sqlx::query("UPDATE workflows SET last_error='budget' WHERE guild_id=$1 AND id=$2")
                .bind(sql_id(guild))
                .bind(id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(None);
        }
        let run:Option<i64>=sqlx::query_scalar("INSERT INTO workflow_runs(guild_id,workflow_id,trigger_summary,user_id,outcome,started_at) SELECT guild_id,id,$3,$4,'running',clock_timestamp() FROM workflows WHERE guild_id=$1 AND id=$2 AND enabled RETURNING id")
            .bind(sql_id(guild)).bind(id).bind(&event.kind).bind(event.user_id.parse::<i64>().unwrap_or(0)).fetch_optional(&mut *tx).await?;
        if run.is_some() {
            sqlx::query("INSERT INTO workflow_run_budget(guild_id) VALUES($1)")
                .bind(sql_id(guild))
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE workflows SET runs=runs+1,last_run_at=now(),last_error=NULL WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(run)
    }
    pub async fn finish_workflow_run(&self, guild: u64, id: i64, outcome: &str, steps: &Value) -> anyhow::Result<()> {
        sqlx::query("UPDATE workflow_runs SET outcome=$3,steps=$4 WHERE guild_id=$1 AND id=$2")
            .bind(sql_id(guild))
            .bind(id)
            .bind(outcome)
            .bind(history_steps(steps))
            .execute(self.pool())
            .await?;
        if outcome == "failed" {
            sqlx::query("UPDATE workflows SET last_error='failed' WHERE guild_id=$1 AND id=(SELECT workflow_id FROM workflow_runs WHERE id=$2 AND guild_id=$1)").bind(sql_id(guild)).bind(id).execute(self.pool()).await?;
        }
        Ok(())
    }
    pub async fn workflow_runs(&self, guild: u64, id: i64) -> anyhow::Result<Vec<WorkflowRun>> {
        Ok(sqlx::query_as("SELECT * FROM workflow_runs WHERE guild_id=$1 AND workflow_id=$2 AND started_at > now()-interval '7 days' ORDER BY started_at DESC LIMIT 100").bind(sql_id(guild)).bind(id).fetch_all(self.pool()).await?)
    }
    pub async fn workflow_vars(&self, guild: u64, user: u64) -> anyhow::Result<Vec<(String, String, String)>> {
        Ok(sqlx::query_as("SELECT scope,key,value FROM workflow_vars WHERE guild_id=$1 AND (scope='server' OR subject_id=$2) ORDER BY scope,key").bind(sql_id(guild)).bind(sql_id(user)).fetch_all(self.pool()).await?)
    }
    pub async fn set_workflow_var(
        &self,
        guild: u64,
        scope: Scope,
        user: u64,
        key: &str,
        value: &str,
        increment: Option<i64>,
    ) -> anyhow::Result<String> {
        anyhow::ensure!(
            pinchora_core::workflows::valid_key(key) && value.chars().count() <= 200,
            "variable"
        );
        let subject = if scope == Scope::Server {
            0
        } else {
            anyhow::ensure!(user > 0, "member");
            sql_id(user)
        };
        self.guild_settings(guild).await?;
        let mut tx = self.pool().begin().await?;
        if subject > 0 {
            locks::lock_users(&mut tx, &[subject]).await?;
        }
        lock(&mut tx, guild).await?;
        let count: i64 = sqlx::query_scalar("SELECT count(DISTINCT key) FROM workflow_vars WHERE guild_id=$1")
            .bind(sql_id(guild))
            .fetch_one(&mut *tx)
            .await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workflow_vars WHERE guild_id=$1 AND key=$2)")
                .bind(sql_id(guild))
                .bind(key)
                .fetch_one(&mut *tx)
                .await?;
        anyhow::ensure!(exists || count < MAX_VARIABLES as i64, "variable_limit");
        let value = if let Some(by) = increment {
            let old: Option<String> = sqlx::query_scalar(
                "SELECT value FROM workflow_vars WHERE guild_id=$1 AND scope=$2 AND subject_id=$3 AND key=$4",
            )
            .bind(sql_id(guild))
            .bind(scope.key())
            .bind(subject)
            .bind(key)
            .fetch_optional(&mut *tx)
            .await?;
            pinchora_core::workflows::variable_value(old.as_deref(), value, Some(by)).map_err(anyhow::Error::msg)?
        } else {
            value.into()
        };
        sqlx::query("INSERT INTO workflow_vars(guild_id,scope,subject_id,key,value) VALUES($1,$2,$3,$4,$5) ON CONFLICT(guild_id,scope,subject_id,key) DO UPDATE SET value=$5").bind(sql_id(guild)).bind(scope.key()).bind(subject).bind(key).bind(&value).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(value)
    }
    pub async fn workflow_due(&self) -> anyhow::Result<Vec<WorkflowRow>> {
        let mut tx = self.pool().begin().await?;
        let ids:Vec<i64>=sqlx::query_scalar("SELECT s.workflow_id FROM workflow_schedules s JOIN workflows w ON w.id=s.workflow_id WHERE s.next_run_at<=now() AND w.enabled ORDER BY s.next_run_at LIMIT 100 FOR UPDATE OF s SKIP LOCKED").fetch_all(&mut *tx).await?;
        let mut rows = Vec::new();
        for id in ids {
            let row: WorkflowRow = sqlx::query_as("SELECT * FROM workflows WHERE id=$1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            if let Trigger::Schedule { spec, tz } = row.definition()?.trigger {
                let next = spec.next(Utc::now(), &tz).ok_or_else(|| anyhow::anyhow!("schedule"))?;
                sqlx::query("UPDATE workflow_schedules SET next_run_at=$2 WHERE workflow_id=$1")
                    .bind(id)
                    .bind(next)
                    .execute(&mut *tx)
                    .await?;
                rows.push(row);
            }
        }
        tx.commit().await?;
        Ok(rows)
    }
    pub async fn prune_workflow_runs(&self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM workflow_runs WHERE id IN (SELECT id FROM workflow_runs WHERE started_at < now()-interval '7 days' FOR UPDATE SKIP LOCKED)")
            .execute(self.pool())
            .await?;
        sqlx::query("DELETE FROM workflow_run_budget WHERE reserved_at < clock_timestamp()-interval '1 minute'")
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

/// History is an execution trace, never a copy of message content or variable values.
fn history_steps(steps: &Value) -> Value {
    let allowed = |value: &Value, choices: &[&str]| {
        value
            .as_str()
            .filter(|s| choices.contains(s))
            .unwrap_or("unknown")
            .to_owned()
    };
    json!(steps.as_array().into_iter().flatten().take(16).map(|step| {
        json!({
            "phase": allowed(&step["phase"], &["trigger", "condition", "action", "context"]),
            "index": step["index"].as_u64().unwrap_or(0).min(10),
            "status": allowed(&step["status"], &["passed", "failed", "running", "done", "skipped", "would_run"]),
            "detail": {}
        })
    }).collect::<Vec<_>>())
}

impl Db {
    /// Persist provenance before the Discord request; no timeout can turn a delayed callback into a new run.
    pub async fn mark_workflow_role_change(&self, guild: u64, user: u64, role: u64, added: bool) -> anyhow::Result<()> {
        let mut tx = self.pool().begin().await?;
        locks::lock_users(&mut tx, &[sql_id(user)]).await?;
        sqlx::query("INSERT INTO workflow_role_changes(guild_id,user_id,role_id,added) VALUES($1,$2,$3,$4) ON CONFLICT(guild_id,user_id,role_id,added) DO UPDATE SET pending=workflow_role_changes.pending+1")
            .bind(sql_id(guild)).bind(sql_id(user)).bind(sql_id(role)).bind(added).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn consume_workflow_role_change(
        &self,
        guild: u64,
        user: u64,
        role: u64,
        added: bool,
    ) -> anyhow::Result<bool> {
        let mut tx = self.pool().begin().await?;
        locks::lock_users(&mut tx, &[sql_id(user)]).await?;
        let consumed = sqlx::query("UPDATE workflow_role_changes SET pending=pending-1 WHERE guild_id=$1 AND user_id=$2 AND role_id=$3 AND added=$4 AND pending>0")
            .bind(sql_id(guild)).bind(sql_id(user)).bind(sql_id(role)).bind(added).execute(&mut *tx).await?.rows_affected()>0;
        sqlx::query("DELETE FROM workflow_role_changes WHERE guild_id=$1 AND user_id=$2 AND role_id=$3 AND added=$4 AND pending=0")
            .bind(sql_id(guild)).bind(sql_id(user)).bind(sql_id(role)).bind(added).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(consumed)
    }
}
