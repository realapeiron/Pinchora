//! Managed guild and audit rules for Roblox configuration.
use crate::*;

#[tokio::test]
async fn settings_and_binds_are_validated_and_audited() {
    let Some(h) = Harness::new("roblox_admin").await else {
        return;
    };
    let (guild, user, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/roblox");
    let initial = browser.get(&base).await;
    assert_eq!(initial.status, StatusCode::OK, "{}", initial.text());
    assert_eq!(initial.json()["binds"], json!([]));
    let mut settings = initial.json()["settings"].clone();
    settings["enabled"] = json!(true);
    settings["verified_role_id"] = json!(FAN.to_string());
    settings["nickname_template"] = json!("{discord-name} ({roblox-name})");
    let saved = browser
        .write(Method::PUT, &format!("{base}/settings"), settings.clone())
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(saved.json()["settings"]["enabled"], true);
    let bad=browser.write(Method::PUT,&format!("{base}/settings"),json!({"enabled":true,"verified_role_id":BOT_ROLE.to_string(),"unverified_role_id":null,"nickname_template":null,"verify_channel_id":null,"require_verified":false})).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", bad.text());
    let added=browser.write(Method::POST,&format!("{base}/binds"),json!({"kind":"group_rank","group_id":"123","rank_min":10,"rank_max":20,"role_id":FAN.to_string(),"remove_if_unmet":true})).await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.text());
    let bind = &added.json()["binds"][0];
    assert_eq!(bind["rank_min"], 10);
    let id = bind["id"].as_str().unwrap().to_owned();
    let removed = browser
        .write(Method::DELETE, &format!("{base}/binds/{id}"), json!({}))
        .await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(removed.json()["binds"], json!([]));
    let audit = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert_eq!(audit[0].actor_id, user as i64);
    assert!(audit.iter().any(|row| row.action == "roblox.settings.put"));
    assert!(audit.iter().any(|row| row.action == "roblox.bind.add"));
    assert!(audit.iter().any(|row| row.action == "roblox.bind.remove"));
}

#[tokio::test]
async fn every_roblox_endpoint_requires_management_and_writes_require_csrf() {
    let Some(h) = Harness::new("roblox_permissions").await else {
        return;
    };
    let (guild, _, mut manager) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/roblox");
    let mut member = h.browser();
    member
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    let settings = json!({"enabled":true,"verified_role_id":FAN.to_string(),"unverified_role_id":null,"nickname_template":"{roblox-name}","verify_channel_id":GENERAL.to_string(),"require_verified":false});
    let bind = json!({"kind":"verified","group_id":null,"rank_min":null,"rank_max":null,"role_id":FAN.to_string(),"remove_if_unmet":true});
    for (method, suffix, body) in [
        (Method::GET, "", json!({})),
        (Method::GET, "/groups/42/ranks", json!({})),
        (Method::PUT, "/settings", settings.clone()),
        (Method::POST, "/binds", bind.clone()),
        (Method::DELETE, "/binds/1", json!({})),
    ] {
        let path = format!("{base}{suffix}");
        let response = if method == Method::GET {
            member.get(&path).await
        } else {
            member.write(method.clone(), &path, body.clone()).await
        };
        assert_eq!(
            response.status,
            StatusCode::FORBIDDEN,
            "{method} {path}: {}",
            response.text()
        );
        if method != Method::GET {
            let csrf = manager.csrf.take();
            assert_eq!(manager.write(method, &path, body).await.code(), "csrf");
            manager.csrf = csrf;
        }
    }
    assert!(h.state.app.db.roblox_binds(guild).await.unwrap().is_empty());
    assert!(!h.state.app.db.roblox_settings(guild).await.unwrap().enabled);
    assert!(h.state.app.db.audit_page(guild, None, 10).await.unwrap().is_empty());
    // Owners and administrators still cannot configure privileged, managed, everyone, or unreachable roles.
    for role in [guild, BOT_ROLE, ADMIN, BOOSTER, HELPER] {
        let mut bad = bind.clone();
        bad["role_id"] = json!(role.to_string());
        let response = manager.write(Method::POST, &format!("{base}/binds"), bad).await;
        assert_eq!(
            response.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "role {role}: {}",
            response.text()
        );
        let mut bad = settings.clone();
        bad["verified_role_id"] = json!(role.to_string());
        let response = manager.write(Method::PUT, &format!("{base}/settings"), bad).await;
        assert_eq!(
            response.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "role {role}: {}",
            response.text()
        );
    }
    for template in ["@everyone {roblox-name}", "{unknown}", "**{discord-name}**", "constant"] {
        let mut bad = settings.clone();
        bad["nickname_template"] = json!(template);
        assert_eq!(
            manager
                .write(Method::PUT, &format!("{base}/settings"), bad)
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let added = manager.write(Method::POST, &format!("{base}/binds"), bind).await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.text());
    let id = added.json()["binds"][0]["id"].as_str().unwrap().to_owned();
    let (other_guild, _, mut other_manager) = h.managed_server().await;
    let response = other_manager
        .write(
            Method::DELETE,
            &format!("/api/guilds/{other_guild}/roblox/binds/{id}"),
            json!({}),
        )
        .await;
    assert_eq!(response.status, StatusCode::NOT_FOUND, "{}", response.text());
    assert_eq!(h.state.app.db.roblox_binds(guild).await.unwrap().len(), 1);
}
