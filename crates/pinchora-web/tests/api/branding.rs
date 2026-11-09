use crate::*;

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
        .add_card_upload(guild, &upload, &png_of(1, 1), 1, 1)
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

#[tokio::test]
async fn branding_partial_apply_clear_and_reset_keep_accepted_state_and_audit() {
    let Some(h) = Harness::new("branding_partial_reset").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/branding");
    let upload = format!("{:032x}", unique());
    h.state.app.db.guild_settings(guild).await.unwrap();
    h.state
        .app
        .db
        .add_card_upload(guild, &upload, &png_of(1, 1), 1, 1)
        .await
        .unwrap();
    let draft = json!({"nickname":"First", "bio":"Accepted", "banner_upload_id":upload, "embed_color":"#102030"});
    assert_eq!(browser.write(Method::PUT, &path, draft).await.status, StatusCode::OK);
    assert_eq!(
        browser
            .write(Method::POST, &format!("{path}/apply"), json!({}))
            .await
            .status,
        StatusCode::OK
    );
    h.mock.refuse_banner.store(true, Ordering::SeqCst);
    sqlx::query("UPDATE guild_branding SET applied_at=now()-interval '11 minutes' WHERE guild_id=$1")
        .bind(guild as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    assert_eq!(
        browser
            .write(
                Method::PUT,
                &path,
                json!({"nickname":"Second", "bio":null, "banner_upload_id":null})
            )
            .await
            .status,
        StatusCode::OK
    );
    let applied = browser
        .write(Method::POST, &format!("{path}/apply"), json!({}))
        .await
        .json();
    assert_eq!(applied["active_nickname"], "Second");
    assert!(applied["active_bio"].is_null());
    assert_eq!(applied["active_banner_upload_id"], upload);
    assert_eq!(applied["last_error"], "banner");
    sqlx::query("UPDATE guild_branding SET applied_at=now()-interval '11 minutes' WHERE guild_id=$1")
        .bind(guild as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    let reset = browser
        .write(Method::POST, &format!("{path}/reset"), json!({}))
        .await
        .json();
    assert_eq!(reset["discord_failed"], json!(["banner"]));
    assert_eq!(reset["branding"]["active_banner_upload_id"], upload);
    assert!(reset["branding"]["active_nickname"].is_null());
    assert_eq!(reset["branding"]["last_error"], "banner");
    let patches = h.mock.profile_patches.lock().unwrap().clone();
    assert!(patches.iter().any(|p| p.get("nick") == Some(&Value::Null)));
    assert!(patches.iter().any(|p| p.get("bio") == Some(&Value::Null)));
    assert!(patches.iter().any(|p| p.get("avatar") == Some(&Value::Null)));
    assert!(patches.iter().any(|p| p.get("banner") == Some(&Value::Null)));
    let audit = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert!(audit.iter().any(|r| r.action == "branding.reset"));
}

#[tokio::test]
async fn branding_snapshot_race_and_redis_invalidation_across_pods() {
    use pinchora_commands::admin::branding::ApplyResult;
    use pinchora_commands::admin::branding::apply;
    let Some(h) = Harness::new("branding_race").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let bot = h.bot_pod().await;
    listen_for_invalidations(&bot).await;
    assert_eq!(bot.brand_color(guild).await.unwrap(), 0xff6b4a);
    let old = pinchora_db::GuildBranding {
        nickname: Some("Snapshot".into()),
        embed_color: Some(0x112233),
        ..Default::default()
    };
    let new = pinchora_db::GuildBranding {
        nickname: Some("New draft".into()),
        embed_color: Some(0x445566),
        ..Default::default()
    };
    h.state.app.db.save_guild_branding(guild, &new).await.unwrap();
    assert!(matches!(
        apply(&h.state.app, guild, &old).await.unwrap(),
        ApplyResult::Applied
    ));
    assert_eq!(
        h.state.app.db.guild_branding(guild).await.unwrap().active_embed_color,
        old.embed_color
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while bot.brand_color(guild).await.unwrap() != 0x112233 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    sqlx::query("UPDATE guild_branding SET applied_at=now()-interval '11 minutes' WHERE guild_id=$1")
        .bind(guild as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    h.mock.profile_patches.lock().unwrap().clear();
    let path = format!("/api/guilds/{guild}/branding/apply");
    let (web, command) = tokio::join!(browser.write(Method::POST, &path, json!({})), apply(&bot, guild, &new));
    let successes =
        usize::from(web.status == StatusCode::OK) + usize::from(matches!(command.unwrap(), ApplyResult::Applied));
    assert_eq!(successes, 1);
    assert_eq!(h.mock.profile_patches.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn branding_mutations_require_manage_server_and_csrf() {
    let Some(h) = Harness::new("branding_permissions").await else {
        return;
    };
    let (guild, _, mut manager) = h.managed_server().await;
    let mut member = h.browser();
    member
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    let base = format!("/api/guilds/{guild}/branding");
    manager.csrf = None;
    for (method, path) in [
        (Method::PUT, base.clone()),
        (Method::POST, format!("{base}/apply")),
        (Method::POST, format!("{base}/reset")),
    ] {
        assert_eq!(
            member.write(method.clone(), &path, json!({})).await.status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            manager.write(method, &path, json!({})).await.status,
            StatusCode::FORBIDDEN
        );
    }
    assert!(h.mock.profile_patches.lock().unwrap().is_empty());
    assert!(h.state.app.db.audit_page(guild, None, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn branding_backup_preserves_runtime_state_and_validates_profile() {
    use pinchora_core::admin::config_backup::Names;
    use pinchora_core::admin::config_backup::Policy;
    let Some(h) = Harness::new("branding_backup").await else {
        return;
    };
    let (guild, _, _) = h.managed_server().await;
    let db = &h.state.app.db;
    let row = pinchora_db::GuildBranding {
        nickname: Some("Accepted".into()),
        embed_color: Some(0x123456),
        ..Default::default()
    };
    db.save_guild_branding(guild, &row).await.unwrap();
    pinchora_commands::admin::branding::apply(&h.state.app, guild, &row)
        .await
        .unwrap();
    let mut backup = db.snapshot_config(guild, Names::default()).await.unwrap();
    assert!(backup.sections["guild_branding"][0].get("active_embed_color").is_none());
    backup.sections.get_mut("guild_branding").unwrap()[0]["nickname"] = json!("@everyone");
    assert!(pinchora_commands::validate_backup(&backup, pinchora_core::infra::i18n::Lang::En).is_err());
    backup.sections.get_mut("guild_branding").unwrap().clear();
    db.import_config(guild, &backup, Policy::Replace, Names::default())
        .await
        .unwrap();
    let after = db.guild_branding(guild).await.unwrap();
    assert_eq!(after.active_nickname.as_deref(), Some("Accepted"));
    assert_eq!(after.active_embed_color, Some(0x123456));
    assert!(
        !db.claim_branding_apply(guild, None).await.unwrap(),
        "backup must not reset cooldown"
    );
}

#[tokio::test]
async fn branding_avatar_refusal_and_transport_failure_are_recorded_and_audited() {
    let Some(h) = Harness::new("branding_avatar_failure").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/branding");
    let id = format!("{:032x}", unique());
    h.state.app.db.guild_settings(guild).await.unwrap();
    h.state
        .app
        .db
        .add_card_upload(guild, &id, &png_of(1, 1), 1, 1)
        .await
        .unwrap();
    h.mock.refuse_avatar.store(true, Ordering::SeqCst);
    assert_eq!(
        browser
            .write(
                Method::PUT,
                &path,
                json!({"nickname":"Accepted", "avatar_upload_id":id})
            )
            .await
            .status,
        StatusCode::OK
    );
    let response = browser
        .write(Method::POST, &format!("{path}/apply"), json!({}))
        .await
        .json();
    assert_eq!(response["active_nickname"], "Accepted");
    assert!(response["active_avatar_upload_id"].is_null());
    assert_eq!(response["avatar_supported"], false);
    assert_eq!(response["last_error"], "avatar");
    sqlx::query("UPDATE guild_branding SET applied_at=now()-interval '11 minutes' WHERE guild_id=$1")
        .bind(guild as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    h.mock.refuse_profile.store(true, Ordering::SeqCst);
    assert_eq!(
        browser
            .write(Method::PUT, &path, json!({"nickname":"Refused"}))
            .await
            .status,
        StatusCode::OK
    );
    let response = browser
        .write(Method::POST, &format!("{path}/apply"), json!({}))
        .await
        .json();
    assert_eq!(response["active_nickname"], "Accepted");
    assert_eq!(response["last_error"], "nickname,avatar,banner,bio");
    assert_eq!(
        h.state
            .app
            .db
            .audit_page(guild, None, 20)
            .await
            .unwrap()
            .iter()
            .filter(|r| r.action == "branding.apply")
            .count(),
        2
    );
}

#[tokio::test]
async fn branding_backup_copies_upload_references_to_the_destination_guild() {
    use pinchora_core::admin::config_backup::Import;
    use pinchora_core::admin::config_backup::Names;
    use pinchora_core::admin::config_backup::Policy;
    let Some(h) = Harness::new("branding_backup_remap").await else {
        return;
    };
    let (source, actor, _) = h.managed_server().await;
    let (target, _, _) = h.managed_server().await;
    let id = format!("{:032x}", unique());
    h.state.app.db.guild_settings(source).await.unwrap();
    h.state
        .app
        .db
        .add_card_upload(source, &id, &png_of(1, 1), 1, 1)
        .await
        .unwrap();
    let row = pinchora_db::GuildBranding {
        avatar_upload_id: Some(id.clone()),
        banner_upload_id: Some(id.clone()),
        ..Default::default()
    };
    h.state.app.db.save_guild_branding(source, &row).await.unwrap();
    let backup = h.state.app.db.snapshot_config(source, Names::default()).await.unwrap();
    let (mapped, _) = pinchora_commands::backup_preview(
        &h.state.app,
        target,
        &Import {
            backup,
            policy: Policy::Merge,
        },
        pinchora_core::infra::i18n::Lang::En,
        actor,
    )
    .await
    .unwrap();
    let copied = &mapped.sections["card_uploads"][0]["id"];
    assert_ne!(copied, &json!(id));
    assert_eq!(&mapped.sections["guild_branding"][0]["avatar_upload_id"], copied);
    assert_eq!(&mapped.sections["guild_branding"][0]["banner_upload_id"], copied);
    assert!(pinchora_commands::validate_backup(&mapped, pinchora_core::infra::i18n::Lang::En).is_ok());
}
