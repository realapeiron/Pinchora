use crate::*;

async fn write_with_accept(browser: &mut Browser, path: &str, body: Value, accept: &str) -> Response {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, PUBLIC_URL)
        .header("sec-fetch-site", "same-origin")
        .header(header::ACCEPT_LANGUAGE, accept);
    if let Some(csrf) = &browser.csrf {
        request = request.header("x-csrf-token", csrf);
    }
    browser.send(request.body(Body::from(body.to_string())).unwrap()).await
}

#[tokio::test]
async fn dashboard_errors_follow_manager_choice_then_accept_language() {
    let Some(h) = Harness::new("dashboard_error_language").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/commands/import");
    let oversized = json!({"items": vec![json!({}); 201]});

    h.state.app.set_guild_lang(guild, Some(Lang::Tr)).await.unwrap();
    h.state.app.set_user_lang(owner, Some(Lang::En)).await.unwrap();
    let english = browser.write(Method::POST, &path, oversized.clone()).await;
    assert_eq!(english.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        english.json()["error"]["message"],
        pinchora_core::t!(Lang::En, "command_control_web.errors.commands-import-limit", max = 200)
    );
    let bad_time = browser
        .write(
            Method::POST,
            &format!("/api/guilds/{guild}/notifications/read"),
            json!({"up_to":"bad"}),
        )
        .await;
    assert_eq!(
        bad_time.json()["error"]["message"],
        pinchora_core::t!(Lang::En, "notifications.bad_time")
    );

    h.state.app.set_guild_lang(guild, Some(Lang::En)).await.unwrap();
    h.state.app.set_user_lang(owner, Some(Lang::Tr)).await.unwrap();
    let turkish = browser.write(Method::POST, &path, oversized.clone()).await;
    assert_eq!(
        turkish.json()["error"]["message"],
        pinchora_core::t!(Lang::Tr, "command_control_web.errors.commands-import-limit", max = 200)
    );
    let bad_path = browser.get("/api/guilds/not-a-number/commands").await;
    assert_eq!(
        bad_path.json()["error"]["message"],
        pinchora_core::t!(Lang::Tr, "common.web.errors.server-ids-are-numbers")
    );

    h.state.app.set_user_lang(owner, None).await.unwrap();
    let browser_tr = write_with_accept(&mut browser, &path, oversized, "tr-TR,tr;q=0.9,en;q=0.8").await;
    assert_eq!(
        browser_tr.json()["error"]["message"],
        pinchora_core::t!(Lang::Tr, "command_control_web.errors.commands-import-limit", max = 200)
    );
}

#[tokio::test]
async fn ticket_panel_field_errors_share_the_manager_language() {
    let Some(h) = Harness::new("ticket_panel_field_language").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    h.state.app.set_guild_lang(guild, Some(Lang::En)).await.unwrap();
    h.state.app.set_user_lang(owner, Some(Lang::Tr)).await.unwrap();
    let body = json!({
        "name":"", "channel_id":GENERAL.to_string(), "title":"Need help?",
        "description":"Press a button.", "color":"wrong", "style":"buttons",
        "topics":[{"label":"Support","emoji":"🎫","tone":"primary"}],
        "questions":[], "support_roles":[]
    });
    let response = browser
        .write(Method::POST, &format!("/api/guilds/{guild}/tickets/panels"), body)
        .await;
    assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", response.text());
    let errors = response.json()["error"]["errors"].as_array().unwrap().clone();
    assert_eq!(
        errors.iter().find(|error| error["field"] == "name").unwrap()["message"],
        pinchora_core::t!(Lang::Tr, "tickets.check.name-empty")
    );
    assert_eq!(
        errors.iter().find(|error| error["field"] == "color").unwrap()["message"],
        "Renkler `#ff6b4a` biçiminde olmalı."
    );
}

#[tokio::test]
async fn language_choice_is_authenticated_persisted_and_shared_with_the_bot() {
    let Some(h) = Harness::new("web_language_choice").await else {
        return;
    };
    let mut browser = h.browser();
    assert_eq!(browser.get("/api/me").await.status, StatusCode::UNAUTHORIZED);
    let user = unique();
    let me = browser.login(&h, user, vec![]).await;
    assert!(me["language"].is_null());

    let csrf = browser.csrf.take();
    let refused = browser
        .write(Method::POST, "/api/me/language", json!({"language":"tr"}))
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(h.state.app.db.user_language(user).await.unwrap(), None);
    browser.csrf = csrf;

    for (code, lang) in [("tr", Lang::Tr), ("en", Lang::En)] {
        let saved = browser
            .write(Method::POST, "/api/me/language", json!({"language":code}))
            .await;
        assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
        assert_eq!(saved.json()["language"], code);
        assert_eq!(browser.get("/api/me").await.json()["language"], code);
        assert_eq!(h.state.app.db.user_language(user).await.unwrap(), Some(lang));
        assert_eq!(h.state.app.user_lang_choice(user).await, Some(lang));

        let invalid = browser
            .write(Method::POST, "/api/me/language", json!({"language":"de"}))
            .await;
        assert_eq!(invalid.status, StatusCode::BAD_REQUEST);
        assert_eq!(
            invalid.json()["error"]["message"],
            pinchora_core::t!(lang, "language.choose-supported")
        );
        assert_eq!(h.state.app.db.user_language(user).await.unwrap(), Some(lang));
    }

    h.state.app.set_user_lang(user, Some(Lang::Tr)).await.unwrap();
    assert_eq!(browser.get("/api/me").await.json()["language"], "tr");
    h.state.app.set_user_lang(user, None).await.unwrap();
    assert!(browser.get("/api/me").await.json()["language"].is_null());
}
