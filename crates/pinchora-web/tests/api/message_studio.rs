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
    let unchanged = browser
        .write(Method::POST, &format!("{root}/greeting/rollback/3"), json!({}))
        .await;
    assert_eq!(unchanged.status, StatusCode::OK);
    assert_eq!(unchanged.json()["version"], 3);
    let audit = h.state.app.db.audit_page(guild, None, 20).await.unwrap();
    assert_eq!(
        audit.iter().filter(|row| row.action == "message_kit.rollback").count(),
        2
    );
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

#[tokio::test]
async fn imports_reject_unknown_nested_fields_and_oversized_bodies() {
    let Some(h) = Harness::new("studio_import_validation").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/message-studio/validate");
    let base = json!({"name":"Test","content":"hello"});
    for extra in [
        json!({"version":1}),
        json!({"allowed_mentions":{"parse":["everyone"]}}),
        json!({"embeds":[{"title":"x","provider":{"name":"spoof"}}]}),
        json!({"embeds":[{"fields":[{"name":"x","value":"y","unknown":true}]}]}),
        json!({"components":[{"type":"button","label":"x","response":"ok","custom_id":"evil"}]}),
        json!({"components":[null]}),
        json!({"content":"x".repeat(33000)}),
    ] {
        let mut value = base.clone();
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let reply = browser.write(Method::POST, &path, value).await;
        assert!(
            matches!(reply.status, StatusCode::UNPROCESSABLE_ENTITY | StatusCode::BAD_REQUEST),
            "{}",
            reply.text()
        );
    }
    let csrf = browser.csrf.take();
    assert_eq!(
        browser.write(Method::POST, &path, base.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    browser.csrf = csrf;
    let reply = browser.write(Method::POST, &path, base).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.json()["components"], json!([]));
}

#[tokio::test]
async fn mass_mention_approval_cannot_be_reused_for_manager_edits_or_rollbacks() {
    let Some(h) = Harness::new("studio_ping_approval").await else {
        return;
    };
    let (guild, _, mut owner) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/message-studio/ping");
    let mut manager = h.browser();
    manager
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, MANAGE_GUILD)])
        .await;
    let mut payload = json!({"name":"Ping","content":"@everyone approved","allow_mass_mentions":true});
    assert_eq!(
        owner.write(Method::PUT, &path, payload.clone()).await.status,
        StatusCode::OK
    );
    payload["content"] = json!("@here <@&123> unapproved");
    assert_eq!(
        manager.write(Method::PUT, &path, payload.clone()).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    payload["allow_mass_mentions"] = json!(false);
    assert_eq!(manager.write(Method::PUT, &path, payload).await.status, StatusCode::OK);
    assert_eq!(
        manager
            .write(Method::POST, &format!("{path}/rollback/1"), json!({}))
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (other, _, mut other_browser) = h.managed_server().await;
    assert_eq!(
        other_browser
            .get(&format!("/api/guilds/{other}/message-studio/ping/history"))
            .await
            .json(),
        json!([])
    );
    assert_eq!(
        other_browser
            .write(
                Method::POST,
                &format!("/api/guilds/{other}/message-studio/ping/rollback/1"),
                json!({})
            )
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn preview_matches_sent_content_embeds_branding_rows_and_mentions() {
    let Some(h) = Harness::new("studio_preview_send").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let root = format!("/api/guilds/{guild}/message-studio");
    let payload = json!({"name":"Preview","content":"Hello", "variants":["First", "Hi {user} in {server}"],
        "embeds":[{"title":"Title","description":"Description","image":"https://example.com/image.png"}],
        "components": (0..25).map(|_| json!({"type":"button","label":"Reply","response":"OK"})).collect::<Vec<_>>() });
    let saved = browser
        .write(Method::PUT, &format!("{root}/preview-kit"), payload.clone())
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let preview = browser
        .write(
            Method::POST,
            &format!("{root}/preview"),
            json!({"payload":payload,"variant":1}),
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    let sent = browser
        .write(
            Method::POST,
            &format!("{root}/send"),
            json!({"key":"preview-kit","channel_id":GENERAL.to_string(),"variant":1}),
        )
        .await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    let posts = h.mock.posts.lock().unwrap();
    let wire: Value = serde_json::from_str(&posts.last().unwrap().1).unwrap();
    assert_eq!(wire["content"], preview.json()["content"]);
    let specs: Vec<pinchora_core::tags::EmbedSpec> = serde_json::from_value(preview.json()["embeds"].clone()).unwrap();
    assert_eq!(
        wire["embeds"],
        json!(specs.iter().map(pinchora_commands::embed_of).collect::<Vec<_>>())
    );
    assert_eq!(wire["allowed_mentions"]["parse"], json!([]));
    let rows = wire["components"].as_array().unwrap();
    assert_eq!(rows.len(), 5);
    for row in rows {
        assert_eq!(row["components"].as_array().unwrap().len(), 5);
        for button in row["components"].as_array().unwrap() {
            assert!(button["custom_id"].as_str().unwrap().len() <= 100);
        }
    }
}

#[tokio::test]
async fn concurrent_first_saves_keep_unique_versions_and_a_bounded_history() {
    let Some(h) = Harness::new("studio_concurrent_saves").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/message-studio/race");
    let mut other = h.browser();
    other.login(&h, owner, vec![guild_entry(guild, "Reef", true, 0)]).await;
    let (a, b) = tokio::join!(
        browser.write(Method::PUT, &path, json!({"name":"Race","content":"A"})),
        other.write(Method::PUT, &path, json!({"name":"Race","content":"B"}))
    );
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(b.status, StatusCode::OK);
    assert_ne!(a.json()["version"], b.json()["version"]);
    for n in 0..21 {
        assert_eq!(
            browser
                .write(
                    Method::PUT,
                    &path,
                    json!({"name":"Race","content":format!("version {n}")})
                )
                .await
                .status,
            StatusCode::OK
        );
    }
    let history = browser.get(&format!("{path}/history")).await;
    assert_eq!(history.json().as_array().unwrap().len(), 20);
    assert_eq!(history.json()[0]["version"], 23);
}

#[tokio::test]
async fn dm_test_quota_precedes_opening_channels_and_tests_are_audited() {
    let Some(h) = Harness::new("studio_dm_limit").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let root = format!("/api/guilds/{guild}");
    let card = browser.get(&format!("{root}/cards")).await.json()["welcome"]["design"].clone();
    let body = json!({"design":card,"delivery":"text_only","dm":true,"channel_id":GENERAL.to_string()});
    let path = format!("{root}/cards/welcome/test");
    for _ in 0..3 {
        assert_eq!(
            browser.write(Method::POST, &path, body.clone()).await.status,
            StatusCode::OK
        );
    }
    let opens = h.mock.dm_opens.load(Ordering::SeqCst);
    assert_eq!(
        browser.write(Method::POST, &path, body).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(h.mock.dm_opens.load(Ordering::SeqCst), opens);
    assert!(
        h.mock
            .posts
            .lock()
            .unwrap()
            .iter()
            .all(|(channel, _)| *channel == owner)
    );
    assert_eq!(
        h.state
            .app
            .db
            .audit_page(guild, None, 20)
            .await
            .unwrap()
            .iter()
            .filter(|r| r.action == "card.test")
            .count(),
        3
    );
}

#[tokio::test]
async fn concurrent_quota_and_binding_checks_are_serialized() {
    let Some(h) = Harness::new("studio_quota_binding_races").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let root = format!("/api/guilds/{guild}/message-studio");
    let mut other = h.browser();
    other.login(&h, owner, vec![guild_entry(guild, "Reef", true, 0)]).await;
    assert_eq!(
        browser
            .write(
                Method::PUT,
                &format!("{root}/event"),
                json!({"name":"Event","content":"Hello"})
            )
            .await
            .status,
        StatusCode::OK
    );
    let bind_path = format!("{root}/events/welcome");
    let event_path = format!("{root}/event");
    let (bind, edit) = tokio::join!(
        browser.write(Method::PUT, &bind_path, json!({"key":"event"})),
        other.write(
            Method::PUT,
            &event_path,
            json!({"name":"Event","content":"Hello","embeds":[{"title":"x"}]})
        )
    );
    assert!(
        (bind.status == StatusCode::OK && edit.status == StatusCode::UNPROCESSABLE_ENTITY)
            || (edit.status == StatusCode::OK && bind.status == StatusCode::UNPROCESSABLE_ENTITY)
    );
    sqlx::query("INSERT INTO message_kits (guild_id,key,name,payload) SELECT $1,'seed-'||n,'Seed','{\"name\":\"Seed\",\"content\":\"OK\"}'::jsonb FROM generate_series(1,48) n")
        .bind(guild as i64).execute(h.state.app.db.pool()).await.unwrap();
    let path_a = format!("{root}/last-a");
    let path_b = format!("{root}/last-b");
    let (a, b) = tokio::join!(
        browser.write(Method::PUT, &path_a, json!({"name":"Last","content":"A"})),
        other.write(Method::PUT, &path_b, json!({"name":"Last","content":"B"}))
    );
    assert!(
        (a.status == StatusCode::OK && b.status == StatusCode::UNPROCESSABLE_ENTITY)
            || (b.status == StatusCode::OK && a.status == StatusCode::UNPROCESSABLE_ENTITY)
    );
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM message_kits WHERE guild_id=$1")
        .bind(guild as i64)
        .fetch_one(h.state.app.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 50);
}
