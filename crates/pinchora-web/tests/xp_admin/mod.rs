use super::*;

#[tokio::test]
async fn xp_routes_require_manager_csrf_and_audit_edits() {
    let Some(h) = Harness::new("xp_admin").await else {
        return;
    };
    let (guild, other, user) = (unique(), unique(), unique());
    h.add_bot(guild);
    h.add_bot(other);
    let mut browser = h.browser();
    browser
        .login(
            &h,
            unique(),
            vec![
                guild_entry(guild, "Reef", true, 0),
                guild_entry(other, "Other", false, 0),
            ],
        )
        .await;
    let body = json!({"operation": "set", "amount": 123});
    let path = format!("/api/guilds/{guild}/xp-admin/members/{user}");
    let csrf = browser.csrf.take();
    let denied = browser.write(Method::PUT, &path, body.clone()).await;
    assert_eq!(denied.code(), "csrf");
    let import_denied = browser
        .write(
            Method::POST,
            &format!("/api/guilds/{guild}/xp-admin/import/mee6"),
            json!({"mode":"merge"}),
        )
        .await;
    assert_eq!(import_denied.code(), "csrf");
    browser.csrf = csrf;
    let denied = browser
        .write(
            Method::PUT,
            &format!("/api/guilds/{other}/xp-admin/members/{user}"),
            body.clone(),
        )
        .await;
    assert_eq!(denied.code(), "not_manager");
    let denied = browser
        .get(&format!("/api/guilds/{other}/xp-admin/members?search={user}"))
        .await;
    assert_eq!(denied.code(), "not_manager");
    let denied = browser
        .write(
            Method::POST,
            &format!("/api/guilds/{other}/xp-admin/import/mee6"),
            json!({"mode":"merge"}),
        )
        .await;
    assert_eq!(denied.code(), "not_manager");
    for amount in [-1, 1_000_000_001, i64::MAX] {
        let response = browser
            .write(Method::PUT, &path, json!({"operation":"set", "amount":amount}))
            .await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{}", response.text());
    }
    let zero = browser
        .get(&format!("/api/guilds/{guild}/xp-admin/members?search=0"))
        .await;
    assert_eq!(zero.status, StatusCode::BAD_REQUEST);
    let saved = browser.write(Method::PUT, &path, body).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(saved.json()["xp"], 123);
    let rows = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert_eq!(rows[0].action, "xp.member.edit");
    let id = "scope-test";
    h.state.app.cache.put(&format!("web:xp-import-job:{id}"), &json!({"id":id,"guild_id":other.to_string(),"status":"done","checked":0,"updated":0,"total":0,"message":null}).to_string(), Duration::from_secs(60)).await.unwrap();
    let hidden = browser.get(&format!("/api/guilds/{guild}/xp-admin/jobs/{id}")).await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn xp_validation_uses_the_servers_language() {
    let Some(h) = Harness::new("xp_admin_turkish").await else {
        return;
    };
    let (guild, user) = (unique(), unique());
    h.add_bot(guild);
    h.state
        .app
        .set_guild_lang(guild, Some(pinchora_core::i18n::Lang::Tr))
        .await
        .unwrap();
    let mut browser = h.browser();
    browser
        .login(&h, unique(), vec![guild_entry(guild, "Reef", true, 0)])
        .await;
    let response = browser
        .write(
            Method::PUT,
            &format!("/api/guilds/{guild}/xp-admin/members/{user}"),
            json!({"operation":"set", "amount":-1}),
        )
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        response.json()["error"]["message"],
        pinchora_core::t!(pinchora_core::i18n::Lang::Tr, "xp.invalid-amount")
    );
    let response = browser
        .get(&format!("/api/guilds/{guild}/xp-admin/members?search=0"))
        .await;
    assert_eq!(response.json()["error"]["message"], "Geçersiz üye kimliği.");
}
