use super::*;
#[tokio::test]
async fn mod_parity_management_requires_session_csrf_and_guild_scope() {
    let Some(h) = Harness::new("mod_parity_api").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    h.mock.guild_owners.lock().unwrap().insert(guild, owner);
    let path = format!("/api/guilds/{guild}/mod-parity");
    let shown = browser.get(&path).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    assert_eq!(shown.json()["config"]["appeals"], false);
    let token = browser.csrf.take();
    let refused = browser
        .write(Method::PUT, &path, json!(pinchora_core::mod_parity::Config::default()))
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    browser.csrf = token;
    let c = pinchora_core::mod_parity::Config {
        appeals: true,
        ..Default::default()
    };
    let saved = browser.write(Method::PUT, &path, json!(c)).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(h.state.app.db.mod_parity_config(guild).await.unwrap(), c);
    let audit = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].action, "moderation.policy");
    assert_eq!(audit[0].source, "web");
    let other = browser.get(&format!("/api/guilds/{}/mod-parity", guild + 1)).await;
    assert_ne!(other.status, StatusCode::OK);
    let bad = browser
        .write(
            Method::POST,
            &format!("{path}/appeals/999999"),
            json!({"accept":true,"reason":"reviewed"}),
        )
        .await;
    assert_ne!(bad.status, StatusCode::OK);
}
#[tokio::test]
async fn mod_parity_appeal_cannot_read_another_members_case() {
    let Some(h) = Harness::new("mod_parity_appeal").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    h.state
        .app
        .db
        .save_mod_parity_config(
            guild,
            &pinchora_core::mod_parity::Config {
                appeals: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let case = h
        .state
        .app
        .db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: pinchora_core::moderation::CaseKind::Warn,
            target: owner + 1,
            target_name: "Other",
            moderator: owner,
            moderator_name: "Staff",
            reason: Some("private"),
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap();
    let path = format!("/api/appeals/{guild}/{}", case.case.number);
    assert_eq!(browser.get(&path).await.status, StatusCode::NOT_FOUND);
    let refused = browser
        .write(Method::POST, &path, json!({"body":"Please reconsider this decision"}))
        .await;
    assert_ne!(refused.status, StatusCode::OK);
    let own = h
        .state
        .app
        .db
        .open_case(&pinchora_db::NewCase {
            guild,
            kind: pinchora_core::moderation::CaseKind::Ban,
            target: owner,
            target_name: "Member",
            moderator: owner + 2,
            moderator_name: "Staff",
            reason: Some("Appealable"),
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap();
    let path = format!("/api/appeals/{guild}/{}", own.case.number);
    assert_eq!(browser.get(&path).await.status, StatusCode::OK);
    let token = browser.csrf.take();
    assert_eq!(
        browser
            .write(Method::POST, &path, json!({"body":"Please reconsider this sanction"}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    browser.csrf = token;
    assert_eq!(
        browser
            .write(Method::POST, &path, json!({"body":"Please reconsider this sanction"}))
            .await
            .status,
        StatusCode::OK
    );
    let listed = browser.get("/api/appeals").await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(listed.json()["cases"].as_array().unwrap().len(), 1);
    assert_eq!(browser.get(&path).await.json()["appeal"]["status"], "pending");
}

#[tokio::test]
async fn mod_parity_policy_checks_fresh_permissions_and_channel_scope_on_preview_and_save() {
    let Some(h) = Harness::new("mod_parity_policy_permissions").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/mod-parity");
    let policy = pinchora_core::mod_parity::Config {
        mute_role: Some(FAN.to_string()),
        ..Default::default()
    };
    // OAuth still calls this person owner, but the fresh guild facts do not.
    for (method, suffix) in [(Method::PUT, ""), (Method::POST, "/preview")] {
        assert_ne!(
            browser
                .write(method, &format!("{path}{suffix}"), json!(policy))
                .await
                .status,
            StatusCode::OK
        );
    }
    h.mock.guild_owners.lock().unwrap().insert(guild, owner);
    // Use another guild so the first request's cached owner cannot mask validation.
    let (guild, owner, mut browser) = h.managed_server().await;
    h.mock.guild_owners.lock().unwrap().insert(guild, owner);
    let path = format!("/api/guilds/{guild}/mod-parity");
    for role in [guild, BOOSTER, ADMIN] {
        let bad = pinchora_core::mod_parity::Config {
            mute_role: Some(role.to_string()),
            ..Default::default()
        };
        assert_ne!(
            browser.write(Method::PUT, &path, json!(bad)).await.status,
            StatusCode::OK
        );
    }
    for channel in [unique(), LOUNGE] {
        let bad = pinchora_core::mod_parity::Config {
            report_channel: Some(channel.to_string()),
            ..Default::default()
        };
        assert_ne!(
            browser
                .write(Method::POST, &format!("{path}/preview"), json!(bad))
                .await
                .status,
            StatusCode::OK
        );
    }
}

#[tokio::test]
async fn mod_parity_export_is_session_bound_and_redacts_reporters_and_staff_notes() {
    let Some(h) = Harness::new("mod_parity_export").await else {
        return;
    };
    let (guild, user, mut browser) = h.managed_server().await;
    let reporter = unique();
    sqlx::query("INSERT INTO mod_notes(guild_id,user_id,author_id,body) VALUES($1,$2,$3,'private staff evidence')")
        .bind(guild as i64)
        .bind(user as i64)
        .bind(reporter as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    h.state
        .app
        .db
        .submit_mod_report(guild, user, reporter, None, None, "anonymous evidence")
        .await
        .unwrap();
    let response = browser.get("/api/appeals/export").await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.json()["mod_reports"].as_array().unwrap().len(), 1);
    assert_eq!(response.json()["mod_notes"].as_array().unwrap().len(), 1);
    assert!(!response.text().contains(&reporter.to_string()));
    assert!(!response.text().contains("evidence"));
    assert_eq!(response.json().as_object().unwrap().len(), 7);
    let mut anonymous = h.browser();
    assert_ne!(anonymous.get("/api/appeals/export").await.status, StatusCode::OK);
}
