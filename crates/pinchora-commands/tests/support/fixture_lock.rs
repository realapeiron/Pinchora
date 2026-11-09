//! Session locks for fixtures shared by Cargo threads and nextest processes.
use sqlx::PgConnection;

#[path = "isolation.rs"]
mod isolation;
use isolation::connection;
pub use isolation::shared;

/// Domain numbers define the global order: games (1), party (2), voice XP (3),
/// temp voice (4), then the independent fixtures (5+). Acquire in increasing order
/// and keep each guard in its original scope. Shared DB isolation always comes first.
#[derive(Clone, Copy)]
pub struct FixtureLock(pub i32);

impl FixtureLock {
    pub async fn lock(self) -> Option<PgConnection> {
        shared().await;
        let mut connection = connection().await?;
        sqlx::query("SELECT pg_advisory_lock(724033, $1)")
            .bind(self.0)
            .execute(&mut connection)
            .await
            .expect("acquire fixture domain lock");
        // Never return this socket to a pool: dropping it (including on panic)
        // closes the session and releases every advisory lock it owns.
        Some(connection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn fixture_locks_coordinate_sessions_and_release_on_drop() {
        let Some(guard) = FixtureLock(101).lock().await else {
            return;
        };
        let mut other = connection().await.unwrap();
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(724033, 101)")
            .fetch_one(&mut other)
            .await
            .unwrap();
        assert!(!acquired, "another session must not enter the same fixture domain");
        let independent: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(724033, 102)")
            .fetch_one(&mut other)
            .await
            .unwrap();
        assert!(independent, "unrelated fixture domains may run concurrently");
        sqlx::query("SELECT pg_advisory_unlock(724033, 102)")
            .execute(&mut other)
            .await
            .unwrap();
        drop(guard);
        tokio::time::timeout(
            Duration::from_secs(5),
            sqlx::query("SELECT pg_advisory_lock(724033, 101)").execute(&mut other),
        )
        .await
        .expect("dropping the guard must close its session and release the lock")
        .unwrap();
    }
}
