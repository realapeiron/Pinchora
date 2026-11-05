//! Runs against a real Redis when `PINCHORA_TEST_REDIS_URL` is set; skipped otherwise.

use std::time::{Duration, Instant};

use pinchora_cache::{Cache, StoredSession};
use twilight_gateway_queue::Queue;

async fn cache() -> Option<Cache> {
    let url = std::env::var("PINCHORA_TEST_REDIS_URL").ok()?;
    Some(Cache::connect(&url).await.expect("connect"))
}

fn unique() -> String {
    format!("{:?}", std::time::SystemTime::now())
}

#[tokio::test]
async fn cooldowns_and_sessions() {
    let Some(cache) = cache().await else { return };
    cache.ping().await.unwrap();
    let key = format!("test:{}", unique());
    assert!(
        cache
            .try_cooldown(&key, Duration::from_secs(5))
            .await
            .unwrap()
            .is_none()
    );
    let left = cache.try_cooldown(&key, Duration::from_secs(5)).await.unwrap().unwrap();
    assert!(left > Duration::from_secs(3) && left <= Duration::from_secs(5));

    let shard = 4_000_000 + (Instant::now().elapsed().as_nanos() % 1000) as u32;
    let session = StoredSession {
        session_id: "abc".into(),
        sequence: 42,
        resume_url: Some("wss://resume".into()),
    };
    cache
        .store_session(shard, &session, Duration::from_secs(30))
        .await
        .unwrap();
    let got = cache.take_session(shard).await.unwrap().unwrap();
    assert_eq!((got.session_id.as_str(), got.sequence), ("abc", 42));
    assert!(
        cache.take_session(shard).await.unwrap().is_none(),
        "a session resumes once"
    );
}

#[tokio::test]
async fn identify_queue_spaces_same_bucket() {
    let Some(cache) = cache().await else { return };
    // A bucket count nobody else uses, so other test runs cannot interfere.
    let queue = cache.identify_queue(997);
    let started = Instant::now();
    queue.enqueue(13).await.unwrap();
    let first = started.elapsed();
    queue.enqueue(13 + 997).await.unwrap();
    let second = started.elapsed();
    assert!(
        first < Duration::from_secs(1),
        "free bucket identifies at once ({first:?})"
    );
    assert!(
        second >= Duration::from_millis(4_500),
        "same bucket waits ~5 s ({second:?})"
    );
}
