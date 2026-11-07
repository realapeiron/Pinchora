use pinchora_core::{mod_parity::Config, moderation::CaseKind};
use pinchora_db::{Db, NewCase};
async fn database() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 5, std::time::Duration::from_secs(10))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    Some(db)
}
fn unique() -> u64 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        >> 6) as u64
}
#[tokio::test]
async fn reports_are_guild_scoped_and_rate_limited() {
    let Some(db) = database().await else { return };
    let g = unique();
    let user = g + 1;
    let reporter = g + 2;
    let first = db
        .submit_mod_report(g, user, reporter, None, None, "Please review")
        .await
        .unwrap();
    assert!(first.is_some());
    assert!(
        db.submit_mod_report(g, user, reporter, None, None, "Duplicate")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.mod_parity_queues(g + 9).await.unwrap()["reports"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        db.mod_parity_queues(g).await.unwrap()["reports"][0]["user_id"],
        user.to_string()
    );
}
#[tokio::test]
async fn appeals_require_own_active_sanction_and_are_unique() {
    let Some(db) = database().await else { return };
    let g = unique();
    let user = g + 1;
    let case = db
        .open_case(&NewCase {
            guild: g,
            kind: CaseKind::Warn,
            target: user,
            target_name: "Member",
            moderator: g + 2,
            moderator_name: "Staff",
            reason: Some("Reason"),
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap();
    let n = case.case.number;
    assert!(
        !db.submit_mod_appeal(g, user + 1, n, "Please reconsider this warning")
            .await
            .unwrap()
    );
    assert!(
        !db.submit_mod_appeal(g + 1, user, n, "Please reconsider this warning")
            .await
            .unwrap()
    );
    assert!(
        db.submit_mod_appeal(g, user, n, "Please reconsider this warning")
            .await
            .unwrap()
    );
    assert!(
        !db.submit_mod_appeal(g, user, n, "Please reconsider this warning")
            .await
            .unwrap()
    );
}
#[tokio::test]
async fn policy_roundtrips_and_rejects_invalid_rules() {
    let Some(db) = database().await else { return };
    let g = unique();
    let c = Config {
        appeals: true,
        ..Default::default()
    };
    db.save_mod_parity_config(g, &c).await.unwrap();
    assert_eq!(db.mod_parity_config(g).await.unwrap(), c);
    let bad = Config {
        mute_role: Some("0".into()),
        ..c
    };
    assert!(db.save_mod_parity_config(g, &bad).await.is_err());
}
