use std::future::Future;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::task::noop_waker_ref;
use pinchora_db::Db;
use sqlx::Connection;

#[tokio::test]
async fn cancelled_begin_cannot_taint_the_next_write() {
    let _isolation = crate::support::shared().await;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let application_name = format!("pinchora_cancel_begin_{}", crate::support::id());
    let separator = if url.contains('?') { '&' } else { '?' };
    let pool_url = format!("{url}{separator}application_name={application_name}");
    let db = Db::connect_waiting(&pool_url, 1, Duration::from_secs(30))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let observer = sqlx::PgPool::connect(&url).await.unwrap();
    let mut failed = Vec::new();
    let first_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(db.pool())
        .await
        .unwrap();
    let next_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(first_pid, next_pid, "healthy connections should stay in the pool");

    for polls in 0..=8 {
        let guild = crate::support::id();
        let role = crate::support::id();
        let mut connection = db.pool().acquire().await.unwrap();
        let mut begin = Box::pin((*connection).begin());
        let mut context = Context::from_waker(noop_waker_ref());
        for _ in 0..polls {
            if let Poll::Ready(result) = begin.as_mut().poll(&mut context) {
                drop(result.unwrap());
                break;
            }
        }
        drop(begin);
        drop(connection);

        // The connection must come back clean on release, before anything else runs on it.
        let held = db.pool().acquire().await.unwrap();
        let open_after_release: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity WHERE application_name = $1 AND state = 'idle in transaction'",
        )
        .bind(&application_name)
        .fetch_one(&observer)
        .await
        .unwrap();
        drop(held);
        if open_after_release != 0 {
            println!("polls={polls}: idle_in_transaction_after_release={open_after_release}");
            failed.push(polls);
        }

        assert!(db.add_level_role(guild, 3, role).await.unwrap(), "polls={polls}");
        let visible: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM level_roles WHERE guild_id = $1 AND role_id = $2)")
                .bind(guild as i64)
                .bind(role as i64)
                .fetch_one(&observer)
                .await
                .unwrap();
        let idle: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity WHERE application_name = $1 AND state = 'idle in transaction'",
        )
        .bind(&application_name)
        .fetch_one(&observer)
        .await
        .unwrap();
        println!("polls={polls}: visible={visible}, idle_in_transaction={idle}");
        if !visible || idle != 0 {
            failed.push(polls);
            sqlx::query("ROLLBACK").execute(db.pool()).await.unwrap();
        }
    }
    assert!(failed.is_empty(), "cancelled begin leaked at poll counts {failed:?}");
}
