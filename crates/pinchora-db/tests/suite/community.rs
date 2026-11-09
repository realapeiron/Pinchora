//! Database transitions for the community pack, using the assigned isolated PostgreSQL.
use pinchora_core::community::community::CountResult;
use pinchora_db::Db;

use crate::support::db;
fn ids() -> (u64, u64, u64) {
    let base = crate::support::id();
    (base, base + 1, base + 2)
}
#[tokio::test]
async fn counting_updates_are_serial_and_records_survive_reset() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let (guild, channel, user) = ids();
    db.save_counting(guild, channel, true, true, None).await.unwrap();
    let (first, second) = tokio::join!(
        db.count_message(channel, user, "1"),
        db.count_message(channel, user + 1, "1"),
    );
    let outcomes = [first.unwrap().unwrap().0, second.unwrap().unwrap().0];
    assert_eq!(outcomes.iter().filter(|&&x| x == CountResult::Correct).count(), 1);
    assert_eq!(db.counting_for_guild(guild).await.unwrap().unwrap().record, 1);
    db.reset_counting(guild).await.unwrap();
    let row = db.counting_for_guild(guild).await.unwrap().unwrap();
    assert_eq!((row.current, row.record), (0, 1));
}
#[tokio::test]
async fn counting_failure_without_reset_keeps_the_last_counter() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let (guild, channel, user) = ids();
    db.save_counting(guild, channel, false, false, None).await.unwrap();
    assert_eq!(
        db.count_message(channel, user, "1").await.unwrap().unwrap().0,
        CountResult::Correct
    );
    assert_eq!(
        db.count_message(channel, user + 1, "4").await.unwrap().unwrap().0,
        CountResult::Wrong
    );
    let row = db.counting_for_guild(guild).await.unwrap().unwrap();
    assert_eq!((row.current, row.last_user_id), (1, Some(user as i64)));
    assert_eq!(
        db.count_message(channel, user, "2").await.unwrap().unwrap().0,
        CountResult::SameUser
    );
}
#[tokio::test]
async fn qotd_uses_the_whole_queue_before_repeating() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let (guild, _, user) = ids();
    for text in ["First?", "Second?", "Third?"] {
        let id = db.suggest_qotd(guild, user, text).await.unwrap().unwrap();
        db.decide_qotd(guild, id, true).await.unwrap();
    }
    let asked = (0..3).map(|_| ()).collect::<Vec<_>>();
    let mut ids = Vec::new();
    for _ in asked {
        ids.push(db.next_qotd(guild).await.unwrap().unwrap().id);
    }
    assert_eq!(ids.iter().collect::<std::collections::HashSet<_>>().len(), 3);
    assert_eq!(db.next_qotd(guild).await.unwrap().unwrap().id, ids[0]);
}
#[tokio::test]
async fn bundled_qotd_pack_has_300_bilingual_questions() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let (guild, _, _) = ids();
    assert_eq!(db.seed_qotd(guild).await.unwrap(), 300);
    assert_eq!(db.seed_qotd(guild).await.unwrap(), 0);
    let rows = db.qotd_queue(guild).await.unwrap();
    assert_eq!(rows.len(), 300);
    assert!(rows.iter().all(|q| q.approved && q.text_tr.is_some()));
}
#[tokio::test]
async fn form_submission_can_be_decided_once_and_is_guild_scoped() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let (guild, _, user) = ids();
    let form: i64 =
        sqlx::query_scalar("INSERT INTO forms(guild_id,name,questions) VALUES($1,'Helper',$2) RETURNING id")
            .bind(guild as i64)
            .bind(serde_json::json!(["Why?"]))
            .fetch_one(db.pool())
            .await
            .unwrap();
    let sub = db
        .submit_form(guild, form, user, &serde_json::json!(["Because."]))
        .await
        .unwrap()
        .unwrap();
    assert!(db.decide_form(guild + 10, sub, user + 1, true).await.unwrap().is_none());
    assert!(db.decide_form(guild, sub, user + 1, true).await.unwrap().is_some());
    assert!(db.decide_form(guild, sub, user + 1, false).await.unwrap().is_none());
}
#[tokio::test]
async fn community_user_rows_are_removed_by_privacy_deletion() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let (guild, channel, user) = ids();
    db.suggest_qotd(guild, user, "A private suggestion?").await.unwrap();
    db.save_counting(guild, channel, true, false, None).await.unwrap();
    db.count_message(channel, user, "1").await.unwrap();
    sqlx::query("INSERT INTO modmail_blocks(guild_id,user_id) VALUES($1,$2)")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.delete_user_data(user, chrono::Utc::now()).await.unwrap();
    let q: i64 = sqlx::query_scalar("SELECT count(*) FROM qotd_questions WHERE suggested_by=$1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    let stats: i64 = sqlx::query_scalar("SELECT count(*) FROM counting_stats WHERE user_id=$1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!((q, stats), (0, 0));
    assert_eq!(db.counting_for_guild(guild).await.unwrap().unwrap().last_user_id, None);
}

fn draft(guild: u64, name: &str) -> pinchora_db::FormRow {
    pinchora_db::FormRow {
        id: 0,
        guild_id: guild as i64,
        name: name.into(),
        questions: serde_json::json!(["Why?"]),
        submit_channel_id: None,
        review_roles: vec![],
        accept_role_id: None,
        dm_result: false,
        open: true,
        message_id: None,
    }
}

#[tokio::test]
async fn concurrent_form_creation_and_submissions_obey_limits() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let (guild, _, user) = ids();
    for i in 0..9 {
        assert!(
            db.save_form(&draft(guild, &format!("Form {i}")))
                .await
                .unwrap()
                .is_some()
        );
    }
    let a = draft(guild, "Tenth");
    let b = draft(guild, "Eleventh");
    let (a, b) = tokio::join!(db.save_form(&a), db.save_form(&b));
    let form = a.unwrap().or(b.unwrap()).unwrap();
    assert_eq!(db.forms(guild).await.unwrap().len(), 10);
    let mut backup = db.export_config(guild, Default::default()).await.unwrap();
    let mut extra = backup.sections["forms"][0].clone();
    extra["id"] = serde_json::json!("999999999");
    extra["name"] = serde_json::json!("Imported eleventh");
    backup.sections.get_mut("forms").unwrap().push(extra);
    assert!(
        db.import_config(
            guild,
            &backup,
            pinchora_core::admin::config_backup::Policy::Merge,
            Default::default()
        )
        .await
        .is_err()
    );
    assert_eq!(db.forms(guild).await.unwrap().len(), 10);
    let answers = serde_json::json!(["Private answer"]);
    let (a, b) = tokio::join!(
        db.submit_form(guild, form, user, &answers),
        db.submit_form(guild, form, user, &answers)
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_ne!(a.is_some(), b.is_some());
    db.decide_form(guild, a.or(b).unwrap(), user + 1, false).await.unwrap();
    assert!(
        db.submit_form(guild, form, user, &answers).await.unwrap().is_none(),
        "review must not bypass the hourly throttle"
    );
    assert!(
        db.save_form(&draft(guild + 1, &"x".repeat(46)))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.submit_form(guild, form, user + 2, &serde_json::json!(["x".repeat(1001)]))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.submit_form(guild, form, user + 2, &serde_json::json!(["one", "two"]))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn qotd_claim_is_atomic_across_connections_and_restarts() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let (guild, _, user) = ids();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
    let id = db.suggest_qotd(guild, user, "Today?").await.unwrap().unwrap();
    db.decide_qotd(guild, id, true).await.unwrap();
    let other = Db::connect_waiting(
        &std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap(),
        2,
        std::time::Duration::from_secs(10),
    )
    .await
    .unwrap();
    let (a, b) = tokio::join!(db.claim_qotd(guild, day), other.claim_qotd(guild, day));
    assert_ne!(a.unwrap().is_some(), b.unwrap().is_some());
    assert!(other.claim_qotd(guild, day).await.unwrap().is_none());
    assert!(
        other
            .claim_qotd(guild, day.pred_opt().unwrap())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        other
            .claim_qotd(guild, day.succ_opt().unwrap())
            .await
            .unwrap()
            .is_some()
    );
    // An empty queue must not consume the day.
    sqlx::query("DELETE FROM qotd_questions WHERE guild_id=$1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let next = day.succ_opt().unwrap().succ_opt().unwrap();
    assert!(db.claim_qotd(guild, next).await.unwrap().is_none());
    let id = db.suggest_qotd(guild, user, "Later?").await.unwrap().unwrap();
    db.decide_qotd(guild, id, true).await.unwrap();
    assert!(db.claim_qotd(guild, next).await.unwrap().is_some());
}

#[tokio::test]
async fn reviewer_deletion_preserves_other_members_data_and_exports_exclude_answers() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let (guild, channel, user) = ids();
    let staff = user + 1;
    let form = db.save_form(&draft(guild, "Private form")).await.unwrap().unwrap();
    let sub = db
        .submit_form(guild, form, user, &serde_json::json!(["PRIVATE_SENTINEL"]))
        .await
        .unwrap()
        .unwrap();
    db.decide_form(guild, sub, staff, true).await.unwrap();
    db.modmail_create(guild, user, channel).await.unwrap();
    db.modmail_close(guild, channel, staff).await.unwrap();
    sqlx::query("INSERT INTO modmail_pending(user_id,body) VALUES($1,'PRIVATE_PENDING')")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let export = serde_json::to_string(&db.export_config(guild, Default::default()).await.unwrap()).unwrap();
    assert!(!export.contains("PRIVATE_SENTINEL") && !export.contains("PRIVATE_PENDING"));
    db.delete_user_data(staff, chrono::Utc::now()).await.unwrap();
    let reviewer: Option<i64> = sqlx::query_scalar("SELECT decided_by FROM form_submissions WHERE id=$1")
        .bind(sub)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(reviewer, None);
    let closer: Option<i64> = sqlx::query_scalar("SELECT closed_by FROM modmail_threads WHERE thread_id=$1")
        .bind(channel as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(closer, None);
    db.delete_user_data(user, chrono::Utc::now()).await.unwrap();
    for table in ["form_submissions", "modmail_threads", "modmail_pending"] {
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE user_id=$1"
        )))
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(count, 0, "{table}");
    }
}
