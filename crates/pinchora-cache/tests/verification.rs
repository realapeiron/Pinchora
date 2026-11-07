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

#[tokio::test]
async fn captcha_each_attempt_consumes_image_with_ttl_and_preserves_three_attempt_limit() {
    let Ok(url) = std::env::var("PINCHORA_TEST_REDIS_URL") else {
        return;
    };
    let cache = Cache::connect(&url).await.unwrap();
    let guild = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    let user = 123;
    for attempt in 0..3 {
        let nonce = format!("{attempt:032x}");
        assert!(cache.captcha_issue(guild, user, &nonce, "hashed-answer").await.unwrap());
        let value: String = redis::cmd("GET")
            .arg(format!("pinchora:cq:{guild}:{user}"))
            .query_async(&mut cache.conn())
            .await
            .unwrap();
        assert_eq!(value, format!("{nonce}:hashed-answer"));
        let ttl: i64 = redis::cmd("TTL")
            .arg(format!("pinchora:cq:{guild}:{user}"))
            .query_async(&mut cache.conn())
            .await
            .unwrap();
        assert!((590..=600).contains(&ttl));
        assert_eq!(
            cache
                .captcha_answer(guild, user + 1, &nonce, "hashed-answer")
                .await
                .unwrap(),
            -1
        );
        assert_eq!(cache.captcha_answer(guild, user, "forged", "wrong").await.unwrap(), -1);
        assert_eq!(
            cache.captcha_answer(guild, user, &nonce, "wrong").await.unwrap(),
            if attempt == 2 { -2 } else { 0 }
        );
        assert_eq!(
            cache
                .captcha_answer(guild, user, &nonce, "hashed-answer")
                .await
                .unwrap(),
            if attempt == 2 { -2 } else { -1 }
        );
    }
    cache.captcha_clear(guild, user).await.unwrap();
    assert!(
        !cache.captcha_issue(guild, user, "new", "hash").await.unwrap(),
        "leaving cannot reset the attempt limit"
    );
}

#[tokio::test]
async fn raid_500_requests_share_fleet_admission_budget() {
    let Ok(url) = std::env::var("PINCHORA_TEST_REDIS_URL") else {
        return;
    };
    let cache = Cache::connect(&url).await.unwrap();
    let application = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    let start = std::time::Instant::now();
    let mut admitted = 0;
    for _ in 0..500 {
        admitted += u64::from(cache.verification_rest_slot(application).await.unwrap());
    }
    assert!(admitted <= start.elapsed().as_millis() as u64 / 50 + 1);
    assert!(admitted < 500, "a burst must be paced even across separate callers");
}
