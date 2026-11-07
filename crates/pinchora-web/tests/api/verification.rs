//! Safety permissions, settings and member-bound captcha routes through the real API stack.
use super::*;

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
    h.state
        .app
        .db
        .hold_verification(guild, user, chrono::Utc::now(), None)
        .await
        .unwrap();
    let token = format!("{:032x}", unique());
    h.state
        .app
        .cache
        .put(
            &format!("vf:web:{token}"),
            &format!("{guild}:{user}"),
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
