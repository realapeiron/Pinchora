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
async fn rename_reservation_can_only_be_released_by_its_owner() {
    let Some(cache) = cache().await else { return };
    let key = format!("test:rename:{}", unique());
    assert!(
        cache
            .try_reserve(&key, "first", Duration::from_secs(30))
            .await
            .unwrap()
            .is_none()
    );
    assert!(!cache.release_reservation(&key, "second").await.unwrap());
    assert!(
        cache
            .try_reserve(&key, "second", Duration::from_secs(30))
            .await
            .unwrap()
            .is_some()
    );
    assert!(cache.release_reservation(&key, "first").await.unwrap());
    assert!(
        cache
            .try_reserve(&key, "second", Duration::from_secs(30))
            .await
            .unwrap()
            .is_none()
    );
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
    // Releasing one starts the next at once; releasing a cooldown that is not running is no error.
    cache.release_cooldown(&key).await.unwrap();
    assert!(
        cache
            .try_cooldown(&key, Duration::from_secs(5))
            .await
            .unwrap()
            .is_none()
    );
    cache.release_cooldown(&key).await.unwrap();
    cache.release_cooldown(&key).await.unwrap();

    let shard = 4_000_000 + (Instant::now().elapsed().as_nanos() % 1000) as u32;
    let session = StoredSession {
        session_id: "abc".into(),
        sequence: 42,
        resume_url: Some("wss://resume".into()),
        intents: 1,
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

#[tokio::test]
async fn hashes_and_sets_hold_who_is_where() {
    let Some(cache) = cache().await else { return };
    let (hash, set) = (format!("test-hash:{}", unique()), format!("test-set:{}", unique()));
    let ttl = Duration::from_secs(30);

    assert!(cache.map_all(&hash).await.unwrap().is_empty());
    cache.map_set(&hash, "1", "lounge", ttl).await.unwrap();
    cache.map_set(&hash, "2", "den", ttl).await.unwrap();
    cache.map_set(&hash, "1", "tavern", ttl).await.unwrap();
    let mut all = cache.map_all(&hash).await.unwrap();
    all.sort();
    assert_eq!(
        all,
        vec![
            ("1".to_owned(), "tavern".to_owned()),
            ("2".to_owned(), "den".to_owned())
        ]
    );
    cache.map_remove(&hash, "1").await.unwrap();
    cache.map_remove(&hash, "nobody").await.unwrap();
    assert_eq!(cache.map_all(&hash).await.unwrap().len(), 1);
    assert_eq!(cache.map_len(&hash).await.unwrap(), 1);
    cache.map_set(&hash, "3", "attic", ttl).await.unwrap();
    assert_eq!(cache.map_len(&hash).await.unwrap(), 2);
    assert_eq!(cache.map_len(&format!("{hash}:none")).await.unwrap(), 0);

    assert!(cache.set_members(&set).await.unwrap().is_empty());
    cache.set_add(&set, "10").await.unwrap();
    cache.set_add(&set, "10").await.unwrap();
    cache.set_add(&set, "11").await.unwrap();
    let mut members = cache.set_members(&set).await.unwrap();
    members.sort();
    assert_eq!(members, vec!["10".to_owned(), "11".to_owned()]);
    cache.set_remove(&set, "10").await.unwrap();
    assert_eq!(cache.set_members(&set).await.unwrap(), vec!["11".to_owned()]);
    cache.set_remove(&set, "11").await.unwrap();
}

#[tokio::test]
async fn merging_a_snapshot_keeps_the_records_written_since_it_was_taken() {
    let Some(cache) = cache().await else { return };
    let key = format!("test-merge:{}", unique());
    let ttl = Duration::from_secs(30);
    // Voice presence records are `channel|flags|seen|roles`; the snapshot was taken at second 200.
    let record = |channel: u64, seen: i64| format!("{channel}|0|{seen}|");
    cache.map_set(&key, "gone", &record(7, 100), ttl).await.unwrap();
    cache.map_set(&key, "replaced", &record(7, 100), ttl).await.unwrap();
    cache.map_set(&key, "newer", &record(7, 205), ttl).await.unwrap();
    cache.map_set(&key, "left", &record(0, 205), ttl).await.unwrap();
    cache.map_set(&key, "moved", &record(9, 210), ttl).await.unwrap();
    let snapshot = [
        ("replaced".to_owned(), record(8, 200)),
        ("left".to_owned(), record(8, 200)),
        ("moved".to_owned(), record(8, 200)),
        ("fresh".to_owned(), record(8, 200)),
    ];
    cache.map_merge(&key, &snapshot, 200, ttl).await.unwrap();

    let mut all = cache.map_all(&key).await.unwrap();
    all.sort();
    let expected: Vec<(String, String)> = [
        ("fresh", record(8, 200)),
        ("left", record(0, 205)),
        ("moved", record(9, 210)),
        ("newer", record(7, 205)),
        ("replaced", record(8, 200)),
    ]
    .into_iter()
    .map(|(field, value)| (field.to_owned(), value))
    .collect();
    assert_eq!(
        all, expected,
        "what is older than the snapshot is replaced by it or dropped; what is as new stays as it is"
    );

    // Nothing in the snapshot and nothing newer: the hash is gone.
    cache.map_merge(&key, &[], 1_000, ttl).await.unwrap();
    assert_eq!(cache.map_len(&key).await.unwrap(), 0);
    // And a snapshot alone fills an empty one.
    cache
        .map_merge(&key, &[("a".to_owned(), record(1, 5))], 1_000, ttl)
        .await
        .unwrap();
    assert_eq!(cache.map_len(&key).await.unwrap(), 1);
}
