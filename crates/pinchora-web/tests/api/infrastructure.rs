use crate::{Arc, Duration, Harness, unique};

#[tokio::test]
async fn bot_and_web_share_infrastructure_clock_and_caches() {
    let h = Harness::new("shared_infrastructure").await.expect("test services");
    let bot = h.state.app.clone();
    let web = h.state.services();
    assert!(Arc::ptr_eq(&bot.services, web));
    let infrastructure = &bot.services.infrastructure;
    assert!(Arc::ptr_eq(infrastructure, &web.infrastructure));
    assert!(std::ptr::eq(&infrastructure.clock, &web.infrastructure.clock));
    assert!(std::ptr::eq(&infrastructure.cache, &web.infrastructure.cache));
    let fixed = chrono::DateTime::from_timestamp(1_800_000_000, 123_000_000).unwrap();
    infrastructure.clock.freeze(fixed);
    assert_eq!(bot.now(), fixed);
    let before = web.infrastructure.clock.now();
    bot.shift_clock(chrono::Duration::days(7));
    assert!((web.infrastructure.clock.now() - before).num_seconds() >= 7 * 86400);
    web.infrastructure.clock.shift(chrono::Duration::days(-7));
    assert!((bot.now() - before).num_seconds().abs() < 5);
    assert_eq!(bot.now(), fixed);
    assert!(Arc::ptr_eq(&bot.http, &infrastructure.http));
    assert!(Arc::ptr_eq(&bot.interaction_http, &infrastructure.interaction_http));
    let key = format!("shared-infrastructure:{}", unique());
    assert!(bot.cache.once(&key, Duration::from_secs(30)).await.unwrap());
    assert!(
        !web.infrastructure
            .cache
            .once(&key, Duration::from_secs(30))
            .await
            .unwrap()
    );
    let guild = unique();
    let bot_settings = bot.guild_settings(guild).await.unwrap();
    let web_settings = h.state.app.guild_settings(guild).await.unwrap();
    assert!(Arc::ptr_eq(&bot_settings, &web_settings));
    bot.drain(Duration::from_secs(1)).await;
    assert!(web.infrastructure.cancellation().is_cancelled());
}

#[tokio::test]
async fn http_request_has_one_span_with_request_id() {
    use crate::{Body, Request};
    use tower::ServiceExt;
    use tracing::instrument::WithSubscriber;
    let h = Harness::new("request_span").await.expect("test services");
    let spans = pinchora_testkit::traces::Spans::default();
    let request = Request::builder().uri("/api/me").body(Body::empty()).unwrap();
    let response = h
        .router
        .clone()
        .oneshot(request)
        .with_subscriber(spans.dispatch())
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    let recorded = spans.named("http.request");
    assert_eq!(recorded.len(), 1);
    assert!(!recorded[0]["request_id"].is_empty());
}

#[tokio::test]
async fn http_spans_redact_path_parameters_queries_and_fallbacks() {
    use crate::{Body, Request};
    use tower::ServiceExt;
    use tracing::instrument::WithSubscriber;
    let h = Harness::new("request_redaction").await.expect("test services");
    for (uri, expected_path) in [
        ("/api/verify/path-secret?token=query-secret", "/api/verify/{token}"),
        ("/unknown-path-secret?token=query-secret", "<unmatched>"),
    ] {
        let spans = pinchora_testkit::traces::Spans::default();
        h.router
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .with_subscriber(spans.dispatch())
            .await
            .unwrap();
        let recorded = spans.named("http.request");
        assert_eq!(recorded.len(), 1);
        assert!(!recorded[0]["request_id"].is_empty());
        assert_eq!(recorded[0]["path"], format!("{expected_path:?}"));
        assert!(recorded[0].values().all(|value| !value.contains("secret")));
    }
}

#[tokio::test]
async fn cancelled_lease_caller_keeps_tracked_owner_until_rollback() {
    use pinchora_services::infra::Job;
    use sqlx::Connection;
    let h = Harness::new("lease_cancellation").await.expect("test services");
    let infrastructure = h.state.services().infrastructure.clone();
    let key = format!("lease-cancellation:{}", unique());
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, finish) = tokio::sync::oneshot::channel();
    let mut release = Some(release);
    let worker = infrastructure.clone();
    let worker_key = key.clone();
    let caller = tokio::spawn(async move {
        Job::new("cancellation_test")
            .leased(&worker, worker_key, async move {
                started.send(()).unwrap();
                finish.await.unwrap();
                Ok(())
            })
            .await
    });
    ready.await.unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    let mut observer =
        sqlx::PgConnection::connect(&std::env::var("PINCHORA_TEST_DATABASE_URL").expect("test database URL"))
            .await
            .unwrap();
    for expected in [false, true] {
        if expected {
            // The owner survives caller cancellation and drain awaits its rollback.
            release.take().unwrap().send(()).unwrap();
            infrastructure.drain(Duration::from_secs(5)).await;
        }
        let mut transaction = observer.begin().await.unwrap();
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1,0))")
            .bind(&key)
            .fetch_one(&mut *transaction)
            .await
            .unwrap();
        assert_eq!(acquired, expected);
        transaction.rollback().await.unwrap();
        if expected {
            break;
        }
    }
    observer.close().await.unwrap();
}
