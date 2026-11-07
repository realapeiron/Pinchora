use pinchora_core::anti_nuke::Config;
use pinchora_db::Db;
use serde_json::json;
async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 5, std::time::Duration::from_secs(60))
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
        >> 4) as u64
}
#[tokio::test]
async fn antinuke_privacy_cleans_actor_trust_and_member_overwrites() {
    let Some(db) = db().await else { return };
    let guild = unique();
    let user = guild + 1;
    let role = guild + 2;
    let c = Config {
        trusted_users: vec![user.to_string()],
        ..Config::default()
    };
    db.save_antinuke(guild, guild, &c).await.unwrap();
    db.backup_antinuke_roles(guild, user, &[role as i64], true)
        .await
        .unwrap();
    db.save_antinuke_snapshot(
        guild,
        "channel",
        guild,
        &json!({"permission_overwrites":[{"id":role.to_string(),"type":0,"allow":"1024","deny":"0"}]}),
        &[(user as i64, "1024".into(), "0".into())],
    )
    .await
    .unwrap();
    db.delete_user_data(user, chrono::Utc::now()).await.unwrap();
    assert!(db.antinuke_config(guild).await.unwrap().trusted_users.is_empty());
    assert!(db.antinuke_backup(guild, user).await.unwrap().is_none());
    let snapshot = db.antinuke_snapshot(guild, "channel", guild).await.unwrap().unwrap();
    assert_eq!(snapshot["permission_overwrites"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["permission_overwrites"][0]["id"], role.to_string());
}
#[tokio::test]
async fn antinuke_snapshot_and_backup_limits() {
    let Some(db) = db().await else { return };
    let guild = unique();
    assert!(
        db.save_antinuke_snapshot(guild, "role", guild, &json!({"name":"x".repeat(65537)}), &[])
            .await
            .is_err()
    );
    assert!(
        db.backup_antinuke_roles(guild, guild, &vec![1; 251], false)
            .await
            .is_err()
    );
    for n in 1..=752 {
        db.save_antinuke_snapshot(guild, "role", n, &json!({"name":"bounded"}), &[])
            .await
            .unwrap();
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM antinuke_snapshots WHERE guild_id=$1")
        .bind(guild as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(count, 750);
}
#[tokio::test]
async fn antinuke_actor_lock_bursts_do_not_exhaust_the_pool() {
    let Some(db) = db().await else { return };
    let guild = unique();
    let db = std::sync::Arc::new(db);
    let mut work = tokio::task::JoinSet::new();
    for _ in 0..12 {
        let db = db.clone();
        work.spawn(async move {
            let _lock = db.lock_antinuke(guild).await.unwrap();
            db.antinuke_config(guild).await.unwrap();
        });
    }
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        while let Some(result) = work.join_next().await {
            result.unwrap();
        }
    })
    .await
    .expect("contenders release pool connections");
}
