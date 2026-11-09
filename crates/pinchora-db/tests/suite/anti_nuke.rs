use pinchora_core::moderation::anti_nuke::Config;
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
use crate::support::id as unique;
#[tokio::test]
async fn antinuke_privacy_cleans_actor_trust_and_member_overwrites() {
    let _isolation = crate::support::shared().await;
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
    assert!(
        db.antinuke_snapshot(guild, "channel", guild).await.unwrap().is_none(),
        "privacy erasure must not silently drop a deny overwrite on restore"
    );
}
#[tokio::test]
async fn antinuke_snapshot_and_backup_limits() {
    let _isolation = crate::support::shared().await;
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
    assert!(
        db.save_antinuke_snapshot(
            guild,
            "channel",
            guild,
            &json!({"permission_overwrites":[]}),
            &[(guild as i64, "1".repeat(65537), "0".into())]
        )
        .await
        .is_err(),
        "normalized ACL data is bounded too"
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
    let _isolation = crate::support::shared().await;
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

#[tokio::test]
async fn antinuke_restored_snapshots_are_immutable_and_member_grants_single_use() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let guild = unique();
    let data = json!({"permission_overwrites":[]});
    db.save_antinuke_snapshot(guild, "channel", guild, &data, &[(1, "0".into(), "1024".into())])
        .await
        .unwrap();
    db.antinuke_restored_object(guild, "channel", guild, guild + 1)
        .await
        .unwrap();
    db.save_antinuke_snapshot(guild, "channel", guild, &data, &[])
        .await
        .unwrap();
    let snapshot = db.antinuke_snapshot(guild, "channel", guild).await.unwrap().unwrap();
    assert_eq!(snapshot["permission_overwrites"][0]["deny"], "1024");
    db.backup_antinuke_roles(guild, guild, &[123], true).await.unwrap();
    assert!(db.claim_antinuke_member_restore(guild, guild).await.unwrap());
    assert!(!db.claim_antinuke_member_restore(guild, guild).await.unwrap());
}
