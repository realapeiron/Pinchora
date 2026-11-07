//! Safety transition ownership against the assigned real Redis.
use pinchora_cache::Cache;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn verification_lease_excludes_other_workers_and_preserves_a_replacement() {
    let Ok(url) = std::env::var("PINCHORA_TEST_REDIS_URL") else {
        return;
    };
    let cache = Cache::connect(&url).await.unwrap();
    let key = format!(
        "test:{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    );
    let first = cache.verification_lease(&key, "first").await.unwrap().unwrap();
    assert!(cache.verification_lease(&key, "second").await.unwrap().is_none());
    // Simulate expiration/replacement while the old worker is still completing its request.
    let redis_key = format!("pinchora:vf:lease:{key}");
    let _: () = redis::cmd("SET")
        .arg(&redis_key)
        .arg("replacement")
        .arg("EX")
        .arg(5)
        .query_async(&mut cache.conn())
        .await
        .unwrap();
    drop(first);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let owner: String = redis::cmd("GET")
        .arg(&redis_key)
        .query_async(&mut cache.conn())
        .await
        .unwrap();
    assert_eq!(owner, "replacement", "a delayed release must not unlock another worker");
    let _: i32 = redis::cmd("DEL")
        .arg(&redis_key)
        .query_async(&mut cache.conn())
        .await
        .unwrap();
    let second = cache.verification_lease(&key, "second").await.unwrap().unwrap();
    drop(second);
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        if let Some(lease) = cache.verification_lease(&key, "third").await.unwrap() {
            drop(lease);
            return;
        }
    }
    panic!("dropping a lease must release it promptly");
}
