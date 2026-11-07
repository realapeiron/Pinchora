//! Roblox link uniqueness, pending expiry and bind cap on PostgreSQL.
use pinchora_db::Db;

#[tokio::test]
async fn profile_link_and_bind_limits() {
    let Some(url) = std::env::var("PINCHORA_TEST_DATABASE_URL").ok() else {
        return;
    };
    let db = Db::connect(&url, 3).await.unwrap();
    db.migrate().await.unwrap();
    let base = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        >> 8) as u64;
    let (guild, user, other, roblox) = (base, base + 1, base + 2, base + 3);
    db.guild_settings(guild).await.unwrap();
    db.roblox_start(user, roblox, "Crab", "pinchora-crab-ab12")
        .await
        .unwrap();
    let pending = db.roblox_pending(user).await.unwrap().unwrap();
    assert_eq!(pending.roblox_id, roblox as i64);
    assert!(db.roblox_complete(user, &pending).await.unwrap());
    assert!(db.roblox_pending(user).await.unwrap().is_none());
    assert_eq!(db.roblox_link(user).await.unwrap().unwrap().roblox_name, "Crab");
    db.roblox_start(other, roblox, "Crab", "pinchora-shell-ab12")
        .await
        .unwrap();
    let other_pending = db.roblox_pending(other).await.unwrap().unwrap();
    assert!(!db.roblox_complete(other, &other_pending).await.unwrap());
    for n in 0..25 {
        db.roblox_add_bind(guild, "group_rank", Some(42), Some(1), Some(255), base + 100 + n, true)
            .await
            .unwrap();
    }
    assert!(
        db.roblox_add_bind(guild, "verified", None, None, None, base + 200, true)
            .await
            .is_err()
    );
    db.roblox_unlink(user).await.unwrap();
    assert!(db.roblox_link(user).await.unwrap().is_none());
}

#[tokio::test]
async fn challenges_are_single_use_bound_and_invalidated_by_replacement_or_unlink() {
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect(&url, 4).await.unwrap();
    db.migrate().await.unwrap();
    let user = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        >> 6) as u64;
    let other = user + 1;
    let roblox = user + 2;
    db.roblox_start(user, roblox, "OldName", "first").await.unwrap();
    let stale = db.roblox_pending(user).await.unwrap().unwrap();
    db.roblox_start(user, roblox + 1, "OtherName", "replacement")
        .await
        .unwrap();
    assert!(!db.roblox_complete(user, &stale).await.unwrap());
    assert!(!db.roblox_complete(other, &stale).await.unwrap());
    let replaced = db.roblox_pending(user).await.unwrap().unwrap();
    db.roblox_unlink(user).await.unwrap();
    assert!(!db.roblox_complete(user, &replaced).await.unwrap());
    db.roblox_start(user, roblox, "OldName", "fresh").await.unwrap();
    let pending = db.roblox_pending(user).await.unwrap().unwrap();
    let (first, second) = tokio::join!(db.roblox_complete(user, &pending), db.roblox_complete(user, &pending));
    assert_ne!(
        first.unwrap(),
        second.unwrap(),
        "only one concurrent check can consume a code"
    );
    assert!(!db.roblox_complete(user, &pending).await.unwrap());
    db.roblox_refresh_name(user, roblox + 1, "WrongAccount").await.unwrap();
    assert_eq!(db.roblox_link(user).await.unwrap().unwrap().roblox_name, "OldName");
    db.roblox_refresh_name(user, roblox, "Renamed").await.unwrap();
    assert_eq!(db.roblox_link(user).await.unwrap().unwrap().roblox_name, "Renamed");
    db.roblox_start(other, roblox + 3, "Expired", "expired").await.unwrap();
    let expired = db.roblox_pending(other).await.unwrap().unwrap();
    sqlx::query("UPDATE roblox_pending SET expires_at=now()-interval '1 second' WHERE user_id=$1")
        .bind(other as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(!db.roblox_complete(other, &expired).await.unwrap());
    let guild = user + 4;
    db.roblox_queue(guild, user).await.unwrap();
    db.roblox_reschedule(guild, user, 60).await.unwrap();
    db.roblox_cache_put(&format!("groups:{roblox}"), &serde_json::json!([[42, 100]]))
        .await
        .unwrap();
    db.roblox_unlink(user).await.unwrap();
    assert!(db.roblox_cache(&format!("groups:{roblox}")).await.unwrap().is_none());
    let due: bool =
        sqlx::query_scalar("SELECT next_update_at<=now() FROM roblox_sync WHERE guild_id=$1 AND user_id=$2")
            .bind(guild as i64)
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(
        due,
        "unlink wakes every previously queued guild, even without a members row"
    );
}
