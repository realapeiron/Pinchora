use std::sync::LazyLock;

use pinchora_cache::Cache;

static TEST_CACHE_RUN: LazyLock<u64> = LazyLock::new(super::unique);

pub(super) async fn connect(url: &str, group: &str) -> anyhow::Result<Cache> {
    anyhow::ensure!(
        !group.is_empty()
            && group
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
        "invalid test cache group"
    );

    let scope = format!("sim-{group}-{:x}", *TEST_CACHE_RUN);
    let client = redis::Client::open(url)?;
    let current_prefix = format!(
        "pinchora:{scope}:db{}",
        client.get_connection_info().redis_settings().db()
    );
    let pattern = format!("pinchora:sim-{group}-*");
    let mut conn = client.get_multiplexed_async_connection().await?;
    let mut cursor = 0_u64;
    let mut old = Vec::new();
    loop {
        let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(100)
            .query_async(&mut conn)
            .await?;
        old.extend(keys.into_iter().filter(|key| !key.starts_with(&current_prefix)));
        cursor = next;
        if cursor == 0 {
            break;
        }
    }
    for batch in old.chunks(100) {
        let _: usize = redis::cmd("DEL").arg(batch).query_async(&mut conn).await?;
    }

    Cache::connect_isolated(url, &scope).await
}
