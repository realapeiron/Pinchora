use pinchora_cache::Cache;
use std::time::Duration;

#[tokio::test]
async fn vanity_credit_must_advance_the_baseline_read_by_the_next_join() {
    let cache = Cache::connect(&std::env::var("PINCHORA_TEST_REDIS_URL").unwrap())
        .await
        .unwrap();
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let key = format!("invites:vanity:review:{unique}");
    cache.put(&key, "10", Duration::from_secs(60)).await.unwrap();
    assert!(
        cache
            .claim_observed_value(&key, 11, Duration::from_secs(60))
            .await
            .unwrap()
    );
    assert!(
        !cache
            .claim_observed_value(&key, 11, Duration::from_secs(60))
            .await
            .unwrap()
    );
    let actual = cache.get(&key).await.unwrap();
    cache.forget(&key).await.unwrap();
    assert_eq!(actual, Some("11".into()), "same API sequence used by invite tracking");
}
