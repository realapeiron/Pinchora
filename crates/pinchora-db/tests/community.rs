//! Database transitions for the community pack, using the assigned isolated PostgreSQL.
use pinchora_core::community::CountResult;
use pinchora_db::Db;

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 8, std::time::Duration::from_secs(60))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    Some(db)
}
fn ids() -> (u64, u64, u64) {
    let base = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        >> 4) as u64;
    (base, base + 1, base + 2)
}
#[tokio::test]
async fn counting_updates_are_serial_and_records_survive_reset() {
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
    let (guild, _, _) = ids();
    assert_eq!(db.seed_qotd(guild).await.unwrap(), 300);
    assert_eq!(db.seed_qotd(guild).await.unwrap(), 0);
    let rows = db.qotd_queue(guild).await.unwrap();
    assert_eq!(rows.len(), 300);
    assert!(rows.iter().all(|q| q.approved && q.text_tr.is_some()));
}
#[tokio::test]
async fn form_submission_can_be_decided_once_and_is_guild_scoped() {
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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
