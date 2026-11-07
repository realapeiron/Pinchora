use super::*;

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
