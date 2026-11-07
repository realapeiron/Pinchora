use super::*;

#[tokio::test]
async fn branding_web_managed_csrf_audit_cooldown_and_discord_capability() {
    let Some(h) = Harness::new("branding_web").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/branding");
    let mut stranger = h.browser();
    stranger
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    assert_eq!(stranger.get(&path).await.status, StatusCode::FORBIDDEN);
    let draft = json!({ "nickname": "Coral", "avatar_upload_id": null,
        "banner_upload_id": null, "bio": "Reef helper", "embed_color": "#2BB9D3" });
    let csrf = browser.csrf.take();
    assert_eq!(
        browser.write(Method::PUT, &path, draft.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    browser.csrf = csrf;
    let saved = browser.write(Method::PUT, &path, draft).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(
        h.state.app.brand_color(guild).await.unwrap(),
        0xff6b4a,
        "save is a draft until apply"
    );
    let applied = browser.write(Method::POST, &format!("{path}/apply"), json!({})).await;
    assert_eq!(applied.status, StatusCode::OK, "{}", applied.text());
    assert_eq!(h.state.app.brand_color(guild).await.unwrap(), 0x2bb9d3);
    let patches = h.mock.profile_patches.lock().unwrap().clone();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0]["nick"], "Coral");
    assert_eq!(patches[0]["bio"], "Reef helper");
    assert_eq!(
        browser
            .write(Method::POST, &format!("{path}/apply"), json!({}))
            .await
            .status,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        browser
            .write(Method::POST, &format!("{path}/reset"), json!({}))
            .await
            .status,
        StatusCode::TOO_MANY_REQUESTS
    );
    sqlx::query("UPDATE guild_branding SET applied_at=now()-interval '11 minutes' WHERE guild_id=$1")
        .bind(guild as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    let reset = browser.write(Method::POST, &format!("{path}/reset"), json!({})).await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    assert_eq!(h.state.app.brand_color(guild).await.unwrap(), 0xff6b4a);
    assert_eq!(
        browser
            .write(Method::POST, &format!("{path}/apply"), json!({}))
            .await
            .status,
        StatusCode::TOO_MANY_REQUESTS
    );
    let audit = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert!(audit.iter().any(|row| row.action == "branding.save"));
    assert!(audit.iter().any(|row| row.action == "branding.apply"));
    assert!(audit.iter().any(|row| row.action == "branding.reset"));
}

#[tokio::test]
async fn branding_banner_refusal_is_detected_without_losing_embed_colour() {
    let Some(h) = Harness::new("branding_banner_capability").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    h.state.app.db.guild_settings(guild).await.unwrap();
    let upload = format!("{:032x}", unique());
    h.state
        .app
        .db
        .add_card_upload(guild, &upload, b"PNG", 1, 1)
        .await
        .unwrap();
    h.mock.refuse_banner.store(true, Ordering::SeqCst);
    let path = format!("/api/guilds/{guild}/branding");
    let saved = browser
        .write(
            Method::PUT,
            &path,
            json!({
                "nickname": null, "avatar_upload_id": null, "banner_upload_id": upload,
                "bio": null, "embed_color": "#204060"
            }),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let applied = browser.write(Method::POST, &format!("{path}/apply"), json!({})).await;
    assert_eq!(applied.status, StatusCode::OK, "{}", applied.text());
    assert_eq!(applied.json()["banner_supported"], false);
    assert!(applied.json()["last_error"].as_str().unwrap().contains("banner"));
    assert_eq!(h.state.app.brand_color(guild).await.unwrap(), 0x204060);
    assert!(h.mock.profile_patches.lock().unwrap().iter().any(|p| {
        p["banner"]
            .as_str()
            .is_some_and(|v| v.starts_with("data:image/png;base64,"))
    }));
}
