//! The public stats boundary through the real router and a mock Discord server.
use crate::*;

#[tokio::test]
async fn public_stats_aggregate_hidden_activity_without_channel_ids() {
    let Some(h) = Harness::new("public_stats_aggregate_hidden_activity_without_channel_ids").await else {
        return;
    };
    let (guild, owner, mut manager) = h.managed_server().await;
    h.mock.stats_public_channels.store(true, Ordering::SeqCst);
    let hour = chrono::Utc::now();
    let mut tally = pinchora_core::admin::server_stats::HourTally {
        messages: 9,
        ..Default::default()
    };
    tally.channels.insert(
        GENERAL,
        pinchora_core::admin::server_stats::Pair {
            messages: 9,
            voice_minutes: 0,
        },
    );
    h.state
        .app
        .db
        .write_stats_batch(&format!("web-private:{guild}"), guild, hour, &tally)
        .await
        .unwrap();
    let mut visible = pinchora_core::admin::server_stats::HourTally {
        messages: 2,
        ..Default::default()
    };
    visible.channels.insert(
        NEWS,
        pinchora_core::admin::server_stats::Pair {
            messages: 2,
            voice_minutes: 0,
        },
    );
    visible.public_channels.insert(
        NEWS,
        pinchora_core::admin::server_stats::Pair {
            messages: 2,
            voice_minutes: 0,
        },
    );
    h.state
        .app
        .db
        .write_stats_batch(&format!("web-visible:{guild}"), guild, hour, &visible)
        .await
        .unwrap();
    let path = format!("/api/guilds/{guild}/stats/settings");
    assert_eq!(
        manager.write(Method::PUT, &path, json!({"public": true})).await.status,
        StatusCode::OK
    );
    let mut anonymous = h.browser();
    let response = anonymous.get(&format!("/api/public/stats/{guild}")).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.text());
    let body = response.json();
    let printed = body.to_string();
    assert!(!printed.contains(&format!("\"id\":\"{GENERAL}\"")));
    assert_eq!(body["top_channels"][0]["id"], NEWS.to_string());
    assert_eq!(body["top_channels"][0]["messages"], 2);
    assert_eq!(body["top_channels"][1]["name"], "Other");
    assert_eq!(body["top_channels"][1]["messages"], 9);
    assert_eq!(body["totals"]["messages"], 2);
    assert_eq!(
        h.state.app.db.audit_page(guild, None, 10).await.unwrap()[0].actor_id,
        owner as i64
    );
}

#[tokio::test]
async fn disabling_public_rejects_even_a_stale_cached_response() {
    let Some(h) = Harness::new("disabling_public_rejects_even_a_stale_cached_response").await else {
        return;
    };
    let (guild, _, mut manager) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/stats/settings");
    assert_eq!(
        manager.write(Method::PUT, &path, json!({"public": true})).await.status,
        StatusCode::OK
    );
    h.state
        .app
        .cache
        .put(
            &format!("web:stats:public:v2:{guild}"),
            "{\"guild\":{\"name\":\"stale\"}}",
            Duration::from_secs(300),
        )
        .await
        .unwrap();
    assert_eq!(
        manager.write(Method::PUT, &path, json!({"public": false})).await.status,
        StatusCode::OK
    );
    let mut anonymous = h.browser();
    let response = anonymous.get(&format!("/api/public/stats/{guild}")).await;
    assert_eq!(response.status, StatusCode::NOT_FOUND, "{}", response.text());
}

#[tokio::test]
async fn an_in_flight_public_get_cannot_reopen_a_closed_page() {
    let Some(h) = Harness::new("an_in_flight_public_get_cannot_reopen_a_closed_page").await else {
        return;
    };
    let (guild, _, mut manager) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/stats/settings");
    assert_eq!(
        manager.write(Method::PUT, &path, json!({"public": true})).await.status,
        StatusCode::OK
    );
    let seen = h.mock.guild_lookups.load(Ordering::SeqCst);
    h.mock.guild_delay_ms.store(500, Ordering::SeqCst);
    let mut anonymous = h.browser();
    let get = tokio::spawn(async move { anonymous.get(&format!("/api/public/stats/{guild}")).await });
    for _ in 0..100 {
        if h.mock.guild_lookups.load(Ordering::SeqCst) > seen {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        h.mock.guild_lookups.load(Ordering::SeqCst) > seen,
        "GET did not reach Discord"
    );
    assert_eq!(
        manager.write(Method::PUT, &path, json!({"public": false})).await.status,
        StatusCode::OK
    );
    assert_eq!(get.await.unwrap().status, StatusCode::NOT_FOUND);
    let mut anonymous = h.browser();
    assert_eq!(
        anonymous.get(&format!("/api/public/stats/{guild}")).await.status,
        StatusCode::NOT_FOUND
    );
}
