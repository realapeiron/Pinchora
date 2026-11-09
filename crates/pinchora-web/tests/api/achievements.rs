//! Achievements' actual authorization, audit and cache boundaries.
use crate::*;

fn body() -> Value {
    json!({"name":"First pearl","description":"A milestone","icon":"star","kind":"custom",
        "target":1,"reward_role_id":null,"reward_points":50,"announce_channel_id":null,
        "secret":true,"enabled":true})
}

#[tokio::test]
async fn achievement_routes_require_manager_csrf_and_record_real_before_after() {
    let Some(h) = Harness::new("achievements_boundaries").await else {
        return;
    };
    let (guild, owner, mut manager) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/achievements");
    let token = manager.csrf.take();
    assert_eq!(manager.write(Method::POST, &base, body()).await.code(), "csrf");
    manager.csrf = token;
    let added = manager.write(Method::POST, &base, body()).await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());
    let id = added.json()["id"].as_str().unwrap().to_owned();
    let mut member = h.browser();
    member
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    for path in [base.clone(), format!("{base}/{id}/holders")] {
        assert_eq!(member.get(&path).await.code(), "not_manager");
    }
    for (method, path) in [
        (Method::POST, base.clone()),
        (Method::PUT, format!("{base}/{id}")),
        (Method::DELETE, format!("{base}/{id}")),
    ] {
        assert_eq!(member.write(method, &path, body()).await.code(), "not_manager");
    }
    let mut edited = body();
    edited["name"] = json!("Changed pearl");
    let updated = manager.write(Method::PUT, &format!("{base}/{id}"), edited).await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.text());
    let audit = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    let before: Value = serde_json::from_str(audit[0].before.as_deref().unwrap()).unwrap();
    let after: Value = serde_json::from_str(audit[0].after.as_deref().unwrap()).unwrap();
    assert_eq!(before["name"], "First pearl");
    assert_eq!(after["name"], "Changed pearl");
    assert_eq!(audit[0].actor_id, owner as i64);
    h.state
        .app
        .db
        .grant_achievement(
            guild,
            owner,
            id.parse().unwrap(),
            pinchora_core::progression::achievements::Kind::Custom,
        )
        .await
        .unwrap();
    assert_eq!(
        manager.get(&format!("{base}/{id}/holders")).await.json()["members"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    h.state
        .app
        .db
        .set_privacy(owner, pinchora_db::Hide::Profile, true)
        .await
        .unwrap();
    assert!(
        manager.get(&format!("{base}/{id}/holders")).await.json()["members"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let removed = manager.write(Method::DELETE, &format!("{base}/{id}"), json!({})).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    let audit = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    let before: Value = serde_json::from_str(audit[0].before.as_deref().unwrap()).unwrap();
    assert_eq!(before["name"], "Changed pearl");
}

#[tokio::test]
async fn achievement_web_edit_publishes_invalidation_and_turkish_labels() {
    let Some(h) = Harness::new("achievements_invalidation").await else {
        return;
    };
    let (guild, _, mut manager) = h.managed_server().await;
    h.state
        .app
        .set_guild_lang(guild, Some(pinchora_core::infra::i18n::Lang::Tr))
        .await
        .unwrap();
    let base = format!("/api/guilds/{guild}/achievements");
    let response = manager.get(&base).await;
    assert_eq!(response.json()["strings"]["ACHIEVEMENTS"], "BAŞARIMLAR");
    let pod = h.bot_pod().await;
    let stream = pod.cache.invalidations().await.unwrap();
    let mut stream = std::pin::pin!(stream);
    let saved = manager.write(Method::POST, &base, body()).await;
    assert_eq!(saved.status, StatusCode::CREATED);
    use futures_util::StreamExt;
    tokio::time::timeout(Duration::from_secs(3), async {
        while stream.next().await != Some(pinchora_cache::Invalidation::Guild(guild)) {}
    })
    .await
    .expect("web edit reaches other pods");
}
