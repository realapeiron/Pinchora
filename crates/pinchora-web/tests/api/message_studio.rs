use super::*;

#[tokio::test]
async fn managed_csrf_validation_history_binding_and_audit() {
    let Some(h) = Harness::new("message_studio_api").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let root = format!("/api/guilds/{guild}/message-studio");
    let mut stranger = h.browser();
    stranger
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    assert_eq!(stranger.get(&root).await.status, StatusCode::FORBIDDEN);
    let payload = json!({"name":"Welcome A","content":"Hi {{if returning}}again{{else}}friend{{end}} {user}",
        "variants":["Hi {user}","Welcome back {user}"],"embeds":[],"components":[],"allow_mass_mentions":false});
    let mut manager = h.browser();
    manager
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, 1 << 5)])
        .await;
    let mut unsafe_payload = payload.clone();
    unsafe_payload["allow_mass_mentions"] = json!(true);
    assert_eq!(
        manager
            .write(Method::PUT, &format!("{root}/unsafe"), unsafe_payload)
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let csrf = browser.csrf.take();
    assert_eq!(
        browser
            .write(Method::PUT, &format!("{root}/greeting"), payload.clone())
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    browser.csrf = csrf;
    let created = browser
        .write(Method::PUT, &format!("{root}/greeting"), payload.clone())
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    assert_eq!(created.json()["version"], 1);
    let preview = browser
        .write(
            Method::POST,
            &format!("{root}/preview"),
            json!({"payload":payload,"variant":1}),
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    assert!(preview.json()["content"].as_str().unwrap().starts_with("Welcome back"));
    let bad = browser
        .write(
            Method::PUT,
            &format!("{root}/bad"),
            json!({"name":"Bad","content":"", "embeds":[],
        "components":[{"type":"button","label":"bad","url":"http://insecure.test","response":null}]}),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    let bound = browser
        .write(
            Method::PUT,
            &format!("{root}/events/welcome"),
            json!({"key":"greeting"}),
        )
        .await;
    assert_eq!(bound.status, StatusCode::OK, "{}", bound.text());
    let mut changed = created.json()["payload"].clone();
    changed["content"] = json!("Changed");
    let updated = browser.write(Method::PUT, &format!("{root}/greeting"), changed).await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.text());
    assert_eq!(updated.json()["version"], 2);
    let restore = browser
        .write(Method::POST, &format!("{root}/greeting/rollback/1"), json!({}))
        .await;
    assert_eq!(restore.status, StatusCode::OK, "{}", restore.text());
    assert_eq!(restore.json()["version"], 3);
    let history = browser.get(&format!("{root}/greeting/history")).await;
    assert_eq!(history.json().as_array().unwrap().len(), 3);
    let audit = h.state.app.db.audit_page(guild, None, 20).await.unwrap();
    assert!(audit.iter().any(|row| row.action == "message_kit.save"));
    assert!(audit.iter().any(|row| row.action == "message_kit.bind_event"));
}

#[tokio::test]
async fn card_test_really_posts_to_requesters_dm_with_alt_text() {
    let Some(h) = Harness::new("card_test_real_dm").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let root = format!("/api/guilds/{guild}");
    let card = browser.get(&format!("{root}/cards")).await.json()["welcome"]["design"].clone();
    let mut card = card;
    card["alt_text"] = json!("A bright reef welcome card");
    let sent = browser
        .write(
            Method::POST,
            &format!("{root}/cards/welcome/test"),
            json!({"design":card,"delivery":"with_message","dm":true,"channel_id":null}),
        )
        .await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    let posts = h.mock.posts.lock().unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].0, owner);
    assert!(posts[0].1.contains("A bright reef welcome card"));
    assert!(posts[0].1.contains(r#""allowed_mentions":{"parse":[]}"#));
}
