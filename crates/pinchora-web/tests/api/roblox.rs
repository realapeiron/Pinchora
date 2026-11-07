//! Managed guild and audit rules for Roblox configuration.
use super::*;

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
