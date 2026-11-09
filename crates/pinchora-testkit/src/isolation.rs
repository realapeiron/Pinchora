#[rustfmt::skip]
#[macro_export]
macro_rules! isolation {
    ($name:ident) => {
        mod $name {
//! Shared admission for application fixtures.
use sqlx::PgConnection;

$crate::fixture_family!(family);
pub(super) use family::connection;

/// One connection per test process protects its fixtures from the DB suite's global
/// cleanup (724031, 1). It deliberately lives until process exit: Cargo runs each
/// integration binary separately, while nextest runs one test per process. Nested
/// harnesses must reuse it, otherwise a queued exclusive lock can deadlock them.
static FIXTURES: tokio::sync::OnceCell<Option<PgConnection>> = tokio::sync::OnceCell::const_new();

pub async fn shared() {
    FIXTURES
        .get_or_init(|| async {
            let mut connection = connection().await?;
            family::enter(&mut connection, false).await;
            sqlx::query("SELECT pg_advisory_lock_shared(724031, 1)")
                .execute(&mut connection)
                .await
                .expect("protect fixtures from global database cleanup");
            Some(connection)
        })
        .await;
}

        }
    };
}
