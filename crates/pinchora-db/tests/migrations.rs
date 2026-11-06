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
