//! Group admission between database tests and application fixtures.
//!
//! Both families perform global sweeps. Each family may run concurrent tests, but
//! they cannot overlap each other: a simulator's party cleanup must not delete a
//! DB test's live lobby. The admission gate is held while the other family drains,
//! so a stream of new arrivals cannot starve a waiting family. Nested application
//! harnesses reuse their process's admission connection instead of entering again.

pub use crate::connection::connection;

/// Hold the returned family membership on this dedicated session until fixtures
/// are finished. Acquire it before domain locks or the DB suite's isolation lock.
pub async fn enter(connection: &mut sqlx::PgConnection, database: bool) {
    enter_namespace(connection, database, 724034).await;
}

async fn enter_namespace(connection: &mut sqlx::PgConnection, database: bool, namespace: i32) {
    let (own, other) = if database { (2, 1) } else { (1, 2) };
    sqlx::query("SELECT pg_advisory_lock($1, 0)")
        .bind(namespace)
        .execute(&mut *connection)
        .await
        .expect("fixture family admission gate");
    sqlx::query("SELECT pg_advisory_lock($1, $2)")
        .bind(namespace)
        .bind(other)
        .execute(&mut *connection)
        .await
        .expect("wait for the other fixture family");
    sqlx::query("SELECT pg_advisory_lock_shared($1, $2)")
        .bind(namespace)
        .bind(own)
        .execute(&mut *connection)
        .await
        .expect("join fixture family");
    sqlx::query("SELECT pg_advisory_unlock($1, $2), pg_advisory_unlock($1, 0)")
        .bind(namespace)
        .bind(other)
        .execute(&mut *connection)
        .await
        .expect("release fixture admission gate");
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgConnection;
    use std::time::Duration;

    async fn waiting(connection: &mut PgConnection, namespace: i32) -> bool {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_locks waiter JOIN pg_locks gate USING (pid) \
             WHERE waiter.locktype='advisory' AND waiter.classid::bigint=$1 \
             AND waiter.objid=1 AND NOT waiter.granted \
             AND gate.classid=waiter.classid AND gate.objid=0 AND gate.granted)",
        )
        .bind(i64::from(namespace))
        .fetch_one(connection)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn fixture_families_overlap_internally_and_exclude_each_other() {
        let Some(mut first) = connection().await else {
            return;
        };
        // A private namespace lets this protocol test run alongside real fixtures.
        let namespace = i32::MAX - std::process::id() as i32;
        let mut second = connection().await.unwrap();
        let mut database = connection().await.unwrap();
        enter_namespace(&mut first, false, namespace).await;
        tokio::time::timeout(Duration::from_secs(5), enter_namespace(&mut second, false, namespace))
            .await
            .expect("application fixtures may overlap");
        let pending = tokio::spawn(async move {
            enter_namespace(&mut database, true, namespace).await;
            database
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !waiting(&mut first, namespace).await {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("database admission waits while holding the gate against new arrivals");
        drop(first);
        assert!(
            waiting(&mut second, namespace).await,
            "the second application fixture still owns its family"
        );
        drop(second);
        let database = tokio::time::timeout(Duration::from_secs(5), pending)
            .await
            .expect("database admission continues when the application family drains")
            .unwrap();
        let mut application = connection().await.unwrap();
        let available: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, 2)")
            .bind(namespace)
            .fetch_one(&mut application)
            .await
            .unwrap();
        assert!(!available, "the database session retains its family membership");
        drop(database);
        tokio::time::timeout(
            Duration::from_secs(5),
            enter_namespace(&mut application, false, namespace),
        )
        .await
        .expect("application admission continues when the database family drains");
    }
}
