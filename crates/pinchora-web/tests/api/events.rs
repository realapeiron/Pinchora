use super::*;
fn body() -> Value {
    let start = chrono::Utc::now() + chrono::Duration::hours(4);
    json!({"title":"@everyone test", "description":"<@&123>","starts_at":start,
        "ends_at":start+chrono::Duration::hours(1),"tz":"UTC","location":"reef",
        "channel_id":GENERAL.to_string(),"required_role_id":null,
        "options":pinchora_core::events::default_options(),"recurrence":null,"sync":false})
}
#[tokio::test]
async fn events_scope_csrf_audit_and_sync_failure_keep_local_changes() {
    let Some(h) = Harness::new("events_review").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/events");
    let token = browser.csrf.take();
    assert_eq!(browser.write(Method::POST, &path, body()).await.code(), "csrf");
    browser.csrf = token;
    let mut foreign = body();
    foreign["channel_id"] = json!("999999999");
    assert_eq!(
        browser.write(Method::POST, &path, foreign).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut role = body();
    role["required_role_id"] = json!("999999999");
    assert_eq!(
        browser.write(Method::POST, &path, role).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut past = body();
    past["starts_at"] = json!("2000-01-01T00:00:00Z");
    assert_eq!(
        browser.write(Method::POST, &path, past).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut mirrored = body();
    mirrored["sync"] = json!(true);
    let made = browser.write(Method::POST, &path, mirrored.clone()).await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    let value = made.json();
    assert!(
        value["warning"].is_string(),
        "sync failure is a warning after local save"
    );
    let id = value["id"].as_str().unwrap().parse::<i64>().unwrap();
    let row = h.state.app.db.event(guild, id).await.unwrap().unwrap();
    assert_eq!(row.host_id, Some(owner as i64));
    assert!(!row.title.contains('@'));
    assert_eq!(h.state.app.db.audit_page(guild, None, 10).await.unwrap().len(), 1);
    let (other, _, mut other_browser) = h.managed_server().await;
    assert_eq!(
        other_browser
            .get(&format!("/api/guilds/{other}/events/{id}"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        other_browser
            .write(Method::DELETE, &format!("/api/guilds/{other}/events/{id}"), json!({}))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        other_browser.get(&format!("{path}/{id}")).await.status,
        StatusCode::FORBIDDEN
    );
    h.state.app.db.rsvp_event(guild, id, owner, "going").await.unwrap();
    h.state.app.db.set_discord_event(id, Some(90001)).await.unwrap();
    let changed =
        chrono::DateTime::from_timestamp_micros((chrono::Utc::now() + chrono::Duration::hours(6)).timestamp_micros())
            .unwrap();
    mirrored["starts_at"] = json!(changed);
    mirrored["ends_at"] = json!(changed + chrono::Duration::hours(1));
    let updated = browser.write(Method::PUT, &format!("{path}/{id}"), mirrored).await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.text());
    assert_eq!(
        h.state.app.db.scheduled_reminder(owner, "event_60", id).await.unwrap(),
        Some(changed - chrono::Duration::hours(1))
    );
    let cancelled = browser.write(Method::DELETE, &format!("{path}/{id}"), json!({})).await;
    assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.text());
    assert_eq!(
        h.state.app.db.event(guild, id).await.unwrap().unwrap().status,
        "cancelled"
    );
    assert!(
        h.state
            .app
            .db
            .scheduled_reminder(owner, "event_60", id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(h.state.app.db.audit_page(guild, None, 10).await.unwrap().len(), 3);
    h.state
        .app
        .db
        .set_module(guild, pinchora_core::modules::Module::Events, Some(false), None)
        .await
        .unwrap();
    h.state.app.forget_guild_settings(guild).await;
    assert_eq!(
        browser.write(Method::POST, &path, body()).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
