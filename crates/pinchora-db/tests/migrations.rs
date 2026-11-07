//! An existing installation must pick up missing older feature migrations.
use sqlx::{Connection, PgConnection, migrate::Migrator};

#[tokio::test]
async fn automod_migrates_after_newer_versions() {
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let mut conn = PgConnection::connect(&url).await.expect("connect");
    // Roll back the isolated schema even if an assertion fails; never modify the live schema.
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("CREATE SCHEMA automod_migration_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL search_path TO automod_migration_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    let old_main = Migrator::with_migrations(
        all.iter()
            .filter(|m| m.version != 45 && m.version <= 53)
            .cloned()
            .collect(),
    );
    old_main.run(&mut *tx).await.unwrap();
    let highest: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(highest, 53);
    all.run(&mut *tx)
        .await
        .expect("apply missing 0045 after 0046, 0047 and 0053");
    let applied: bool = sqlx::query_scalar("SELECT success FROM _sqlx_migrations WHERE version = 45")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(applied);
    sqlx::query("SELECT 1 FROM automod_settings LIMIT 1")
        .execute(&mut *tx)
        .await
        .expect("AutoMod schema exists");
    all.run(&mut *tx).await.expect("migration is idempotent");
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn command_domains_upgrade_applied_0066_and_preserve_constraints() {
    use pinchora_core::command_control::Config;
    use serde_json::json;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let mut conn = PgConnection::connect(&url).await.expect("connect");
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("CREATE SCHEMA command_domains_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL search_path TO command_domains_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    let before = Migrator::with_migrations(
        all.iter()
            .filter(|m| m.version <= 67 && m.version != 66)
            .cloned()
            .collect(),
    );
    before.run(&mut *tx).await.unwrap();
    let consolidated = Migrator::with_migrations(all.iter().filter(|m| m.version <= 67).cloned().collect());
    consolidated.run(&mut *tx).await.expect("0066 still applies after 0067");
    let config = json!({"rules": {
        "setup": {"cooldown_secs":12},
        "feeds": {"denied_channels":["2"]},
        "setup feeds list": {"allowed_roles":["3"]},
        "integrations feeds list": {"allowed_roles":["4"]}
    }, "aliases":{"feed":"setup feeds", "old":"feeds list"}});
    sqlx::query("INSERT INTO command_control(guild_id,config) VALUES(1,$1)")
        .bind(config)
        .execute(&mut *tx)
        .await
        .unwrap();
    all.run(&mut *tx).await.expect("0068 after applied 0066");
    let read: serde_json::Value = sqlx::query_scalar("SELECT config FROM command_control WHERE guild_id=1")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let migrated: Config = serde_json::from_value(read.clone()).unwrap();
    assert!(!migrated.rules.contains_key("setup"));
    assert!(!migrated.rules.contains_key("setup feeds list"));
    let rule = &migrated.rules["integrations feeds list"];
    assert_eq!(rule.allowed_roles, ["4"]);
    assert_eq!(rule.constraints["setup feeds list"].allowed_roles, ["3"]);
    assert_eq!(rule.constraints["feeds"].denied_channels, ["2"]);
    assert_eq!(rule.constraints["setup"].cooldown_secs, 12);
    assert!(rule.constraints.values().all(|r| r.constraints.is_empty()));
    assert_eq!(migrated.aliases["feed"], "integrations feeds");
    assert_eq!(migrated.aliases["old"], "integrations feeds list");
    assert!(!migrated.rules.contains_key("setup server show"));
    assert!(!migrated.rules.contains_key("mod lock"));
    all.run(&mut *tx).await.unwrap();
    sqlx::query("UPDATE command_control SET config=config WHERE guild_id=1")
        .execute(&mut *tx)
        .await
        .unwrap();
    let again: serde_json::Value = sqlx::query_scalar("SELECT config FROM command_control WHERE guild_id=1")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(again, read, "on-write normalization is idempotent");
    tx.rollback().await.unwrap();
}
