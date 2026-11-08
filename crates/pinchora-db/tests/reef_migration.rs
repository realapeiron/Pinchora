//! Upgrade a populated installation, including a rolling deployment's legacy writes.
use sqlx::{Connection, PgConnection, migrate::Migrator};

#[tokio::test]
async fn reward_cutover_preserves_midday_counts_and_caps_legacy_writers() {
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let mut conn = PgConnection::connect(&url).await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    sqlx::raw_sql("CREATE SCHEMA reef_reward_upgrade_fixture; SET LOCAL search_path TO reef_reward_upgrade_fixture")
        .execute(&mut *tx)
        .await
        .unwrap();
    let all = sqlx::migrate!("../../migrations");
    Migrator::with_migrations(all.iter().filter(|m| m.version < 85).cloned().collect())
        .run(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO users(id,credits) VALUES(42,90),(43,0);
        INSERT INTO credit_ledger(user_id,delta,reason) VALUES
        (42,8,'trivia'),(42,8,'trivia_win'),(42,30,'chess_win'),(42,30,'chess_win'),
        (42,10,'quick_game');
        INSERT INTO credit_ledger(user_id,delta,reason,created_at)
        VALUES(42,30,'chess_win',now()-interval '2 days')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    all.run(&mut *tx).await.unwrap();
    let limits: (String, String) =
        sqlx::query_as("SELECT current_setting('lock_timeout'), current_setting('statement_timeout')")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(
        limits,
        ("5s".into(), "30s".into()),
        "live cutover cannot wait or scan without a bound"
    );
    let counts: Vec<(String, i64)> =
        sqlx::query_as("SELECT game,wins FROM game_reward_daily WHERE user_id=42 ORDER BY game")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(counts, vec![("chess".into(), 2), ("trivia".into(), 2)]);
    // A legacy bot's normal wallet + ledger transaction is counted after cutover too.
    sqlx::raw_sql(
        "UPDATE users SET credits=credits+90 WHERE id=42;
        INSERT INTO credit_ledger(user_id,delta,reason) VALUES
        (42,30,'chess_win'),(42,30,'chess_win'),(42,30,'chess_win')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let mut attempt = tx.begin().await.unwrap();
    sqlx::query("UPDATE users SET credits=credits+30 WHERE id=42")
        .execute(&mut *attempt)
        .await
        .unwrap();
    assert!(
        sqlx::query("INSERT INTO credit_ledger(user_id,delta,reason) VALUES(42,30,'chess_win')")
            .execute(&mut *attempt)
            .await
            .is_err()
    );
    attempt.rollback().await.unwrap();
    let balance: i64 = sqlx::query_scalar("SELECT credits FROM users WHERE id=42")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(balance, 180, "over-cap legacy payment rolls back the wallet too");
    let wins: i64 = sqlx::query_scalar("SELECT wins FROM game_reward_daily WHERE user_id=42 AND game='chess'")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(wins, 5);
    all.run(&mut *tx).await.expect("repeat migration is a no-op");
    tx.rollback().await.unwrap();
}
