//! Feature migrations retain their assigned number even when later packages shipped first.
use sqlx::{Connection, PgConnection, migrate::Migrator};

#[tokio::test]
async fn assigned_feature_migrations_apply_after_later_versions() {
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let mut conn = PgConnection::connect(&url).await.unwrap();
    // Keep all schema changes in a transaction so failures also clean up.
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("CREATE SCHEMA merge_migrations_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL search_path TO merge_migrations_regression")
        .execute(&mut *tx)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    let baseline = Migrator::with_migrations(
        all.iter()
            .filter(|m| ![50, 54, 55].contains(&m.version))
            .cloned()
            .collect(),
    );
    baseline.run(&mut *tx).await.unwrap();
    let latest: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(latest > 55, "exercise migrations older than the applied maximum");
    all.run(&mut *tx).await.unwrap();
    for version in [50, 54, 55].into_iter().filter(|v| all.iter().any(|m| m.version == *v)) {
        let applied: bool = sqlx::query_scalar("SELECT success FROM _sqlx_migrations WHERE version = $1")
            .bind(version)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert!(applied, "migration {version} must apply out of order");
    }
    all.run(&mut *tx).await.unwrap();
    tx.rollback().await.unwrap();
}
