use pinchora_core::workflows::{Definition, Event, Scope, StepAction, Trigger};
use pinchora_db::Db;
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
        trigger: Trigger::Button,
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
