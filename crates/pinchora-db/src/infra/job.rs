use futures_util::FutureExt;
use sqlx::{Connection, PgConnection};
use std::{future::Future, panic::AssertUnwindSafe};

/// The transaction only owns the advisory lock; work uses its existing atomic database operations.
pub async fn run_job_lease<T>(
    connection: &mut PgConnection,
    key: &str,
    work: impl Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<Option<T>> {
    let mut transaction = connection.begin().await?;
    let result = AssertUnwindSafe(async {
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1,0))")
            .bind(key)
            .fetch_one(&mut *transaction)
            .await?;
        if acquired { work.await.map(Some) } else { Ok(None) }
    })
    .catch_unwind()
    .await;
    // Drop only queues rollback; await completion before another connection may take the lock.
    transaction.rollback().await?;
    match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[cfg(test)]
mod tests {
    use crate::run_job_lease;
    use sqlx::{Connection, PgConnection};

    #[tokio::test]
    async fn lease_is_released_before_return_on_success_error_and_early_return() {
        let url = std::env::var("PINCHORA_TEST_DATABASE_URL").expect("test database URL");
        let mut holder = PgConnection::connect(&url).await.unwrap();
        let mut observer = PgConnection::connect(&url).await.unwrap();
        let key = format!("job-release-test:{}", std::process::id());
        for outcome in ["success", "error", "early"] {
            let result = run_job_lease(&mut holder, &key, async {
                assert!(!try_lock(&mut observer, &key).await);
                if outcome == "early" {
                    return Ok(7);
                }
                if outcome == "error" {
                    anyhow::bail!("expected job failure");
                }
                Ok(42)
            })
            .await;
            match outcome {
                "success" => assert_eq!(result.unwrap(), Some(42)),
                "early" => assert_eq!(result.unwrap(), Some(7)),
                _ => assert!(result.is_err()),
            }
            // Do not poll the holder: this also detects a merely queued rollback.
            assert!(try_lock(&mut observer, &key).await, "lock retained after {outcome}");
            sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1,0))")
                .bind(&key)
                .execute(&mut observer)
                .await
                .unwrap();
        }
        assert!(try_lock(&mut observer, &key).await);
        let skipped: Option<()> = run_job_lease(&mut holder, &key, async {
            panic!("contended lease must not poll work")
        })
        .await
        .unwrap();
        assert_eq!(skipped, None);
        sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1,0))")
            .bind(&key)
            .execute(&mut observer)
            .await
            .unwrap();
        holder.close().await.unwrap();
        observer.close().await.unwrap();
    }

    async fn try_lock(connection: &mut PgConnection, key: &str) -> bool {
        sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1,0))")
            .bind(key)
            .fetch_one(connection)
            .await
            .unwrap()
    }
}
