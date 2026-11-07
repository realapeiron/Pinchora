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
