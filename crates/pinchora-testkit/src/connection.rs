#[rustfmt::skip]
#[macro_export]
macro_rules! connection {
    ($name:ident) => {
        mod $name {
//! Dedicated fixture sessions with the same bounded acquisition as application pools.
use std::time::Duration;

use sqlx::{PgConnection, postgres::PgPoolOptions};

pub async fn connection() -> Option<PgConnection> {
    let url = match std::env::var("PINCHORA_TEST_DATABASE_URL") {
        Ok(url) => url,
        Err(std::env::VarError::NotPresent) => return None,
        Err(error) => panic!("invalid test database URL: {error}"),
    };
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(60))
        .connect(&url)
        .await
        .expect("connect fixture lock session");
    let connection = pool.acquire().await.expect("fixture lock socket").detach();
    pool.close().await;
    Some(connection)
}

        }
    };
}
