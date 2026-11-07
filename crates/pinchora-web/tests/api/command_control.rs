use super::*;
#[tokio::test]
async fn command_control_backup_preview_import_restore_and_permissions() {
    let Some(h) = Harness::new("command_control_backup").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}");
    let initial = browser.get(&format!("{path}/command-control")).await;
    assert_eq!(initial.status, StatusCode::OK, "{}", initial.text());
    assert!(
        initial.json()["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["path"] == "commandcontrol" && c["protected"] == true)
    );
    let mut config = pinchora_core::command_control::Config::default();
    config.aliases.insert("w".into(), "play wordle".into());
    let saved = browser
        .write(Method::PUT, &format!("{path}/command-control"), json!(config))
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let exported = browser.get(&format!("{path}/backup/export")).await;
    assert_eq!(exported.status, StatusCode::OK, "{}", exported.text());
    let backup = exported.json();
    let body = json!({"backup":backup,"policy":"replace"});
    let preview = browser
        .write(Method::POST, &format!("{path}/backup/preview"), body.clone())
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    assert_eq!(preview.json()["can_import"], true, "{}", preview.text());
    assert!(h.state.app.db.config_restore_points(guild).await.unwrap().is_empty());
    assert_eq!(h.state.app.db.command_control(guild).await.unwrap(), config);
    let imported = browser
        .write(Method::POST, &format!("{path}/backup/import"), body)
        .await;
    assert_eq!(imported.status, StatusCode::OK, "{}", imported.text());
    let point = imported.json()["restore_point"].as_str().unwrap().to_owned();
    let restored = browser.get(&format!("{path}/backup/restore/{point}")).await;
    assert_eq!(restored.status, StatusCode::OK);
    assert_eq!(browser.get(&format!("{path}/backup/export")).await.json(), backup);
    let mut bad = backup.clone();
    bad["sections"]["members"] = json!([{"user_id":"1234"}]);
    assert!(
        !browser
            .write(
                Method::POST,
                &format!("{path}/backup/preview"),
                json!({"backup":bad,"policy":"replace"})
            )
            .await
            .status
            .is_success()
    );
    let mut member = h.browser();
    member
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    for endpoint in ["command-control", "backup", "backup/export"] {
        assert_eq!(
            member.get(&format!("{path}/{endpoint}")).await.status,
            StatusCode::FORBIDDEN
        );
    }
}
