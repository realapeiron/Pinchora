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
    let before = web.infrastructure.clock.now();
    bot.shift_clock(chrono::Duration::days(7));
    assert!((web.infrastructure.clock.now() - before).num_seconds() >= 7 * 86400);
    web.infrastructure.clock.shift(chrono::Duration::days(-7));
    assert!((bot.now() - before).num_seconds().abs() < 5);
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
