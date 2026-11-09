use pinchora_core::workflows::{Definition, Event, Scope, StepAction, Trigger};
use pinchora_db::Db;
// Due-work scans are global; scheduler tests must not consume each other's fixtures.
static SCHEDULER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect(&url, 4).await.unwrap();
    db.migrate().await.unwrap();
    Some(db)
}
fn id() -> u64 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64)
        >> 4
}
fn definition(name: &str) -> Definition {
    Definition {
        name: name.into(),
        enabled: true,
        trigger: Trigger::Button {},
        conditions: vec![],
        actions: vec![StepAction::Log { text: "ok".into() }],
    }
}
#[tokio::test]
async fn workflows_enforce_caps_and_tenant_isolation() {
    let Some(db) = db().await else { return };
    let guild = id();
    let user = guild + 1;
    for n in 0..25 {
        db.save_workflow(guild, None, &definition(&format!("wf{n}")), user)
            .await
            .unwrap();
    }
    assert!(db.save_workflow(guild, None, &definition("extra"), user).await.is_err());
    assert!(db.save_workflow(guild, None, &definition("wf0"), user).await.is_err());
    let row = db.workflows(guild).await.unwrap().remove(0);
    assert!(db.workflow(guild + 2, row.id).await.unwrap().is_none());
    assert!(!db.delete_workflow(guild + 2, row.id).await.unwrap());
    let event = Event {
        kind: "button".into(),
        user_id: user.to_string(),
        ..Default::default()
    };
    for _ in 0..100 {
        assert!(db.claim_workflow_run(guild, row.id, &event).await.unwrap().is_some());
    }
    assert!(db.claim_workflow_run(guild, row.id, &event).await.unwrap().is_none());
    assert_eq!(db.workflow_runs(guild, row.id).await.unwrap().len(), 100);
    assert!(
        db.claim_workflow_run(guild, row.id, &Event { depth: 1, ..event })
            .await
            .is_err()
    );
    for n in 0..100 {
        db.set_workflow_var(guild, Scope::Server, 0, &format!("key{n}"), "0", None)
            .await
            .unwrap();
    }
    assert!(
        db.set_workflow_var(guild, Scope::Member, user, "overflow", "0", None)
            .await
            .is_err()
    );
    assert_eq!(
        db.set_workflow_var(guild, Scope::Server, 0, "key0", "", Some(3))
            .await
            .unwrap(),
        "3"
    );
    assert!(
        db.set_workflow_var(guild, Scope::Server, 0, "key0", &"x".repeat(201), None)
            .await
            .is_err()
    );
    sqlx::query("DELETE FROM guilds WHERE id=$1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
}
#[tokio::test]
async fn workflow_concurrent_variable_updates_and_run_claims_are_atomic() {
    let Some(db) = db().await else { return };
    let guild = id();
    let row = db
        .save_workflow(guild, None, &definition("atomic"), guild + 1)
        .await
        .unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..110 {
        let db = db.clone();
        let row = row.clone();
        tasks.spawn(async move {
            db.claim_workflow_run(
                guild,
                row.id,
                &Event {
                    kind: "button".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .is_some()
        });
    }
    let mut claimed = 0;
    while let Some(result) = tasks.join_next().await {
        claimed += usize::from(result.unwrap());
    }
    assert_eq!(claimed, 100);
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let db = db.clone();
        tasks.spawn(async move {
            db.set_workflow_var(guild, Scope::Server, 0, "count", "", Some(1))
                .await
                .unwrap()
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    assert_eq!(
        db.workflow_vars(guild, 0).await.unwrap(),
        vec![("server".into(), "count".into(), "20".into())]
    );
    sqlx::query("DELETE FROM guilds WHERE id=$1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
}
#[tokio::test]
async fn workflow_history_expires_and_schedule_is_single_claim() {
    let _turn = SCHEDULER.lock().await;
    let Some(db) = db().await else { return };
    let guild = id();
    let mut def = definition("timer");
    def.trigger = Trigger::Schedule {
        spec: pinchora_core::workflows::Schedule::Interval { minutes: 1 },
        tz: "Europe/Istanbul".into(),
    };
    let row = db.save_workflow(guild, None, &def, guild + 1).await.unwrap();
    sqlx::query("UPDATE workflow_schedules SET next_run_at=now()-interval '1 second' WHERE workflow_id=$1")
        .bind(row.id)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(db.workflow_due().await.unwrap().iter().any(|r| r.id == row.id));
    assert!(!db.workflow_due().await.unwrap().iter().any(|r| r.id == row.id));
    let run = db
        .claim_workflow_run(
            guild,
            row.id,
            &Event {
                kind: "schedule".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE workflow_runs SET started_at=now()-interval '8 days' WHERE id=$1")
        .bind(run)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(db.workflow_runs(guild, row.id).await.unwrap().is_empty());
    db.prune_workflow_runs().await.unwrap();
    sqlx::query("DELETE FROM guilds WHERE id=$1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn workflow_budget_survives_deletion_and_history_never_keeps_content() {
    let Some(db) = db().await else { return };
    let guild = id();
    let row = db
        .save_workflow(guild, None, &definition("secret"), guild + 1)
        .await
        .unwrap();
    let event = Event {
        kind: "button".into(),
        user_id: (guild + 1).to_string(),
        ..Default::default()
    };
    let run = db.claim_workflow_run(guild, row.id, &event).await.unwrap().unwrap();
    let secret = "Bearer very-private-token";
    db.finish_workflow_run(
        guild,
        run,
        "completed",
        &serde_json::json!([
            {"phase":"action","index":0,"status":"done","detail":{"text":secret,"embed":{"description":secret}}},
            {"phase":"condition","index":1,"status":"passed","detail":{"kind":"var_equals","value":secret}},
            {"phase":secret,"index":2,"status":secret,"detail":{}}
        ]),
    )
    .await
    .unwrap();
    let history = db.workflow_runs(guild, row.id).await.unwrap();
    assert!(!history[0].steps.to_string().contains(secret));
    assert_eq!(history[0].steps[0]["status"], "done");
    for _ in 1..100 {
        assert!(db.claim_workflow_run(guild, row.id, &event).await.unwrap().is_some());
    }
    db.delete_workflow(guild, row.id).await.unwrap();
    let replacement = db
        .save_workflow(guild, None, &definition("replacement"), guild + 1)
        .await
        .unwrap();
    assert!(
        db.claim_workflow_run(guild, replacement.id, &event)
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("UPDATE workflow_run_budget SET reserved_at=now()-interval '2 minutes' WHERE guild_id=$1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.prune_workflow_runs().await.unwrap();
    assert!(
        db.claim_workflow_run(guild, replacement.id, &event)
            .await
            .unwrap()
            .is_some()
    );
    sqlx::query("DELETE FROM guilds WHERE id=$1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn workflow_variable_key_cap_is_atomic_across_members_and_scopes() {
    let Some(db) = db().await else { return };
    let guild = id();
    db.save_workflow(guild, None, &definition("vars"), guild + 1)
        .await
        .unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    for n in 0..110 {
        let db = db.clone();
        tasks.spawn(async move {
            let scope = if n % 2 == 0 { Scope::Server } else { Scope::Member };
            db.set_workflow_var(guild, scope, guild + n + 1, &format!("key{n}"), "0", None)
                .await
                .is_ok()
        });
    }
    let mut written = 0;
    while let Some(result) = tasks.join_next().await {
        written += usize::from(result.unwrap());
    }
    assert_eq!(written, 100);
    sqlx::query("DELETE FROM guilds WHERE id=$1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn workflow_schedule_concurrent_claims_skip_downtime_and_retention_deletes_rows() {
    let _turn = SCHEDULER.lock().await;
    let Some(db) = db().await else { return };
    let guild = id();
    let mut def = definition("restart");
    def.trigger = Trigger::Schedule {
        spec: pinchora_core::workflows::Schedule::Interval { minutes: 1 },
        tz: "UTC".into(),
    };
    let row = db.save_workflow(guild, None, &def, guild + 1).await.unwrap();
    sqlx::query("UPDATE workflow_schedules SET next_run_at=now()-interval '10 days' WHERE workflow_id=$1")
        .bind(row.id)
        .execute(db.pool())
        .await
        .unwrap();
    let (a, b) = tokio::join!(db.workflow_due(), db.workflow_due());
    assert_eq!(
        a.unwrap()
            .iter()
            .chain(b.unwrap().iter())
            .filter(|r| r.id == row.id)
            .count(),
        1
    );
    assert!(!db.workflow_due().await.unwrap().iter().any(|r| r.id == row.id));
    let next: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT next_run_at FROM workflow_schedules WHERE workflow_id=$1")
            .bind(row.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(next > chrono::Utc::now());
    let run = db
        .claim_workflow_run(
            guild,
            row.id,
            &Event {
                kind: "schedule".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE workflow_runs SET started_at=now()-interval '8 days' WHERE id=$1")
        .bind(run)
        .execute(db.pool())
        .await
        .unwrap();
    db.prune_workflow_runs().await.unwrap();
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workflow_runs WHERE id=$1)")
        .bind(run)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(!exists);
    sqlx::query("DELETE FROM guilds WHERE id=$1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn workflow_member_writes_obey_the_privacy_lock_before_guild_lock() {
    for run_first in [false, true] {
        assert_workflow_privacy_lock_order(run_first).await;
    }
}

async fn assert_workflow_privacy_lock_order(run_first: bool) {
    let Some(db) = db().await else { return };
    let guild = id();
    let user = guild + 1;
    let row = db
        .save_workflow(guild, None, &definition("privacy_lock"), user)
        .await
        .unwrap();
    let mut privacy = db.pool().begin().await.unwrap();
    sqlx::query("SELECT lock_user($1)")
        .bind(user as i64)
        .execute(&mut *privacy)
        .await
        .unwrap();
    let event = Event {
        kind: "button".into(),
        user_id: user.to_string(),
        ..Default::default()
    };
    let mut run = Box::pin(db.claim_workflow_run(guild, row.id, &event));
    // Either writer can reach Postgres first when connection acquisition is delayed.
    if run_first {
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), &mut run)
                .await
                .is_err()
        );
    }
    let mut variable = Box::pin(db.set_workflow_var(guild, Scope::Member, user, "private", "value", None));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut variable)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut run)
            .await
            .is_err()
    );
    // A blocked member operation must not monopolize the guild lock.
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        db.set_workflow_var(guild, Scope::Server, 0, "public", "value", None),
    )
    .await
    .unwrap()
    .unwrap();
    privacy.commit().await.unwrap();
    // Keep both transactions polled: either can own the user lock after privacy commits.
    let (variable, run) = tokio::join!(variable, run);
    variable.unwrap();
    run.unwrap();
    db.delete_user_data(user, chrono::Utc::now()).await.unwrap();
    assert_eq!(
        db.workflow_vars(guild, user).await.unwrap(),
        vec![("server".into(), "public".into(), "value".into())]
    );
    assert!(db.workflow_runs(guild, row.id).await.unwrap().is_empty());
    sqlx::query("DELETE FROM guilds WHERE id=$1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
}
