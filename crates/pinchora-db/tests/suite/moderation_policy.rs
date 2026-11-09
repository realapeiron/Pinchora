use pinchora_core::moderation::moderation::CaseKind;
use pinchora_core::moderation::policy::Config;
use pinchora_db::{Db, NewCase};
async fn database() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 5, std::time::Duration::from_secs(10))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    Some(db)
}
use crate::support::id as unique;
#[tokio::test]
async fn reports_are_guild_scoped_and_rate_limited() {
    let _isolation = crate::support::shared().await;
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
        db.moderation_policy_queues(g + 9).await.unwrap()["reports"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        db.moderation_policy_queues(g).await.unwrap()["reports"][0]["user_id"],
        user.to_string()
    );
}
#[tokio::test]
async fn appeals_require_own_active_sanction_and_are_unique() {
    let _isolation = crate::support::shared().await;
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
    let _isolation = crate::support::shared().await;
    let Some(db) = database().await else { return };
    let g = unique();
    let c = Config {
        appeals: true,
        ..Default::default()
    };
    db.save_moderation_policy_config(g, &c).await.unwrap();
    assert_eq!(db.moderation_policy_config(g).await.unwrap(), c);
    let bad = Config {
        mute_role: Some("0".into()),
        ..c
    };
    assert!(db.save_moderation_policy_config(g, &bad).await.is_err());
}

#[tokio::test]
async fn autopurge_claim_excludes_other_pods_until_transaction_ends() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = database().await else {
        return;
    };
    let guard = db.claim_moderation_policy_sweep().await.unwrap().expect("first pod");
    assert!(db.claim_moderation_policy_sweep().await.unwrap().is_none());
    guard.rollback().await.unwrap();
    assert!(db.claim_moderation_policy_sweep().await.unwrap().is_some());
}

#[tokio::test]
async fn report_blocks_self_reports_and_hourly_flood_are_enforced() {
    let _isolation = crate::support::shared().await;
    let Some(db) = database().await else {
        return;
    };
    let g = unique();
    let user = g + 1;
    let reporter = g + 2;
    assert!(
        db.submit_mod_report(g, reporter, reporter, None, None, "self")
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("INSERT INTO mod_report_blocks(guild_id,user_id) VALUES($1,$2)")
        .bind(g as i64)
        .bind(reporter as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        db.submit_mod_report(g, user, reporter, None, None, "blocked")
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("DELETE FROM mod_report_blocks WHERE guild_id=$1")
        .bind(g as i64)
        .execute(db.pool())
        .await
        .unwrap();
    for _ in 0..5 {
        assert!(
            db.submit_mod_report(g, user, reporter, None, None, "evidence")
                .await
                .unwrap()
                .is_some()
        );
        sqlx::query("UPDATE mod_reports SET created_at=now()-interval '2 minutes' WHERE guild_id=$1")
            .bind(g as i64)
            .execute(db.pool())
            .await
            .unwrap();
    }
    assert!(
        db.submit_mod_report(g, user, reporter, None, None, "flood")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn appeals_serialize_different_cases_and_throttle_after_review() {
    let _isolation = crate::support::shared().await;
    let Some(db) = database().await else {
        return;
    };
    let g = unique();
    let user = g + 1;
    let mut numbers = Vec::new();
    for _ in 0..2 {
        numbers.push(
            db.open_case(&NewCase {
                guild: g,
                kind: CaseKind::Warn,
                target: user,
                target_name: "member",
                moderator: g + 2,
                moderator_name: "staff",
                reason: Some("reason"),
                duration: None,
                expires_at: None,
                details: None,
            })
            .await
            .unwrap()
            .case
            .number,
        );
    }
    let (a, b) = tokio::join!(
        db.submit_mod_appeal(g, user, numbers[0], "Please reconsider"),
        db.submit_mod_appeal(g, user, numbers[1], "Please reconsider")
    );
    assert_ne!(a.unwrap(), b.unwrap());
    sqlx::query("UPDATE mod_appeals SET status='rejected' WHERE guild_id=$1")
        .bind(g as i64)
        .execute(db.pool())
        .await
        .unwrap();
    for n in &numbers {
        assert!(!db.submit_mod_appeal(g, user, *n, "Please reconsider").await.unwrap());
    }
    sqlx::query("UPDATE mod_appeals SET created_at=now()-interval '11 minutes' WHERE guild_id=$1")
        .bind(g as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let mut accepted = 0;
    for n in numbers {
        accepted += usize::from(db.submit_mod_appeal(g, user, n, "Please reconsider").await.unwrap());
    }
    assert_eq!(accepted, 1);
}

#[tokio::test]
async fn d11_export_and_delete_cover_every_new_personal_table() {
    let _isolation = crate::support::shared().await;
    let Some(db) = database().await else {
        return;
    };
    let g = unique();
    let user = g + 1;
    let other = g + 2;
    for sql in [
        "INSERT INTO mod_notes(guild_id,user_id,author_id,body) VALUES($1,$2,$3,'private staff note')",
        "INSERT INTO mod_reports(guild_id,user_id,reporter_id,body) VALUES($1,$2,$3,'anonymous evidence')",
        "INSERT INTO mod_appeals(guild_id,user_id,case_number,body,reviewer_id) VALUES($1,$2,1,'my own appeal',$3)",
        "INSERT INTO mod_restrictions(guild_id,user_id,role_id,role_ids,kind) VALUES($1,$2,$3,ARRAY[$3::bigint],'jail')",
        "INSERT INTO mod_voice_bans(guild_id,user_id) SELECT $1,$2 WHERE $3::bigint>0",
        "INSERT INTO mod_report_blocks(guild_id,user_id) SELECT $1,$2 WHERE $3::bigint>0",
        "INSERT INTO mod_booster_roles(guild_id,user_id,role_id) VALUES($1,$2,$3)",
    ] {
        sqlx::query(sql)
            .bind(g as i64)
            .bind(user as i64)
            .bind(other as i64)
            .execute(db.pool())
            .await
            .unwrap();
    }
    let export = db.export_moderation_policy_data(user).await.unwrap();
    for rows in export.as_object().unwrap().values() {
        assert_eq!(rows.as_array().unwrap().len(), 1);
    }
    assert!(export["mod_notes"][0]["body"].is_null());
    assert!(export["mod_reports"][0]["body"].is_null());
    db.delete_user_data(user, chrono::Utc::now()).await.unwrap();
    let export = db.export_moderation_policy_data(user).await.unwrap();
    for rows in export.as_object().unwrap().values() {
        assert!(rows.as_array().unwrap().is_empty());
    }
}
