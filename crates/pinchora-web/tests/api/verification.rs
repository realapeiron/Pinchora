//! Safety permissions, settings and member-bound captcha routes through the real API stack.
use crate::*;

#[tokio::test]
async fn safety_settings_require_manage_and_csrf_and_preview_a_real_png() {
    let Some(h) = Harness::new("safety_api").await else {
        return;
    };
    let (guild, _, mut owner) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/safety");
    let initial = owner.get(&path).await;
    assert_eq!(initial.status, StatusCode::OK, "{}", initial.text());
    let mut config = initial.json()["config"].clone();
    config["verification"]["min_account_age_hours"] = json!(24);
    let csrf = owner.csrf.take();
    assert_eq!(owner.write(Method::PUT, &path, config.clone()).await.code(), "csrf");
    owner.csrf = csrf;
    let saved = owner.write(Method::PUT, &path, config.clone()).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let audit = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].action, "safety.set");
    assert_eq!(
        owner.get(&path).await.json()["config"]["verification"]["min_account_age_hours"],
        24
    );
    config["raid"]["window_secs"] = json!(0);
    assert!(!owner.write(Method::PUT, &path, config).await.status.is_success());
    let preview = owner.write(Method::POST, &format!("{path}/preview"), json!({})).await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    let image = preview.json()["image"].as_str().unwrap().to_owned();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image.strip_prefix("data:image/png;base64,").unwrap())
        .unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let mut member = h.browser();
    member
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    for method in [Method::GET, Method::PUT] {
        let response = if method == Method::GET {
            member.get(&path).await
        } else {
            member.write(method, &path, json!({})).await
        };
        assert_eq!(response.status, StatusCode::FORBIDDEN, "{}", response.text());
    }
}

#[tokio::test]
async fn web_captcha_is_bound_to_its_member_and_requires_csrf() {
    let Some(h) = Harness::new("safety_member_api").await else {
        return;
    };
    let (guild, user, mut browser) = h.managed_server().await;
    let mut config = pinchora_core::moderation::verification::Config::default();
    config.verification.enabled = true;
    config.verification.verified_role_id = Some("123".into());
    h.state.app.db.save_safety(guild, &config).await.unwrap();
    h.state
        .app
        .db
        .hold_verification(guild, user, chrono::Utc::now(), None)
        .await
        .unwrap();
    let joined = h
        .state
        .app
        .db
        .pending_verification(guild, user)
        .await
        .unwrap()
        .unwrap()
        .joined_at
        .timestamp_micros();
    let token = format!("{:032x}", unique());
    h.state
        .app
        .cache
        .put(
            &format!("vf:web:{token}"),
            &format!("{guild}:{user}:{joined}"),
            Duration::from_secs(600),
        )
        .await
        .unwrap();
    let path = format!("/api/verify/{token}");
    assert_eq!(browser.get(&path).await.status, StatusCode::OK);
    let csrf = browser.csrf.take();
    assert_eq!(
        browser
            .write(Method::POST, &format!("{path}/captcha"), json!({}))
            .await
            .code(),
        "csrf"
    );
    browser.csrf = csrf;
    let card = browser.write(Method::POST, &format!("{path}/captcha"), json!({})).await;
    assert_eq!(card.status, StatusCode::OK, "{}", card.text());
    assert_eq!(card.json()["nonce"].as_str().unwrap().len(), 32);
    let mut stranger = h.browser();
    stranger.login(&h, unique(), vec![]).await;
    assert!(!stranger.get(&path).await.status.is_success());
    assert!(
        !stranger
            .write(
                Method::POST,
                &format!("{path}/answer"),
                json!({"nonce":card.json()["nonce"], "answer":"AAAAA"})
            )
            .await
            .status
            .is_success()
    );
    assert_eq!(
        h.state
            .app
            .db
            .pending_verification(guild, user)
            .await
            .unwrap()
            .unwrap()
            .attempts,
        0
    );
}

#[tokio::test]
async fn web_captcha_token_cannot_cross_rejoin_or_disabled_module() {
    let Some(h) = Harness::new("safety_rejoin_api").await else {
        return;
    };
    let (guild, user, mut browser) = h.managed_server().await;
    let mut c = pinchora_core::moderation::verification::Config::default();
    c.verification.enabled = true;
    c.verification.verified_role_id = Some("123".into());
    h.state.app.db.save_safety(guild, &c).await.unwrap();
    let joined = chrono::Utc::now();
    h.state
        .app
        .db
        .hold_verification(guild, user, joined, None)
        .await
        .unwrap();
    let token = format!("{:032x}", unique());
    h.state
        .app
        .cache
        .put(
            &format!("vf:web:{token}"),
            &format!("{guild}:{user}:{}", joined.timestamp_micros()),
            Duration::from_secs(600),
        )
        .await
        .unwrap();
    let path = format!("/api/verify/{token}");
    assert_eq!(browser.get(&path).await.status, StatusCode::OK);
    h.state
        .app
        .db
        .hold_verification(guild, user, joined + chrono::Duration::microseconds(1), None)
        .await
        .unwrap();
    assert!(!browser.get(&path).await.status.is_success());
    assert!(
        !browser
            .write(Method::POST, &format!("{path}/captcha"), json!({}))
            .await
            .status
            .is_success()
    );
    h.state
        .app
        .cache
        .put(
            &format!("vf:web:{token}"),
            &format!("{guild}:{user}:{}", joined.timestamp_micros() + 1),
            Duration::from_secs(600),
        )
        .await
        .unwrap();
    c.verification.enabled = false;
    h.state.app.db.save_safety(guild, &c).await.unwrap();
    assert!(
        !browser
            .write(Method::POST, &format!("{path}/captcha"), json!({}))
            .await
            .status
            .is_success()
    );
}

#[tokio::test]
async fn safety_copy_and_expired_links_use_the_selected_language() {
    let Some(h) = Harness::new("safety_turkish").await else {
        return;
    };
    let (guild, user, mut browser) = h.managed_server().await;
    h.state
        .app
        .set_guild_lang(guild, Some(pinchora_core::infra::i18n::Lang::Tr))
        .await
        .unwrap();
    h.state
        .app
        .set_user_lang(user, Some(pinchora_core::infra::i18n::Lang::Tr))
        .await
        .unwrap();
    let response = browser.get(&format!("/api/guilds/{guild}/safety")).await;
    assert_eq!(response.json()["language"], "tr");
    assert_eq!(response.json()["strings"]["title"], "Güvenlik");
    let overview = browser.get(&format!("/api/guilds/{guild}")).await;
    assert_eq!(overview.json()["navigation"]["invites"], "Davetler");
    assert_eq!(overview.json()["navigation"]["safety"], "Güvenlik");
    let expired = browser.get("/api/verify/expired").await;
    assert!(!expired.status.is_success());
    assert!(
        expired.text().contains("Bu doğrulamanın süresi doldu"),
        "{}",
        expired.text()
    );
}
