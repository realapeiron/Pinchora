use crate::*;

#[tokio::test]
async fn backup_round_trips_command_control_off_without_losing_rules() {
    let Some(h) = Harness::new("command_control_off_backup").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}");
    let mut config = pinchora_core::admin::command_control::Config::default();
    config.aliases.insert("w".into(), "play wordle".into());
    let save = browser
        .write(Method::PUT, &format!("{path}/command-control"), json!(config))
        .await;
    assert_eq!(save.status, StatusCode::OK, "{}", save.text());
    let off = browser
        .write(
            Method::PUT,
            &format!("{path}/modules/command_control"),
            json!({"enabled":false}),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    let backup = browser.get(&format!("{path}/backup/export")).await.json();
    let row = backup["sections"]["guild_modules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["module"] == "command_control")
        .unwrap();
    assert_eq!(row["enabled"], false);
    assert_eq!(row["channel_mode"], "all");
    let on = browser
        .write(
            Method::PUT,
            &format!("{path}/modules/command_control"),
            json!({"enabled":true}),
        )
        .await;
    assert_eq!(on.status, StatusCode::OK);
    let body = json!({"backup":backup,"policy":"replace"});
    let preview = browser
        .write(Method::POST, &format!("{path}/backup/preview"), body.clone())
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    assert_eq!(preview.json()["can_import"], true);
    let imported = browser
        .write(Method::POST, &format!("{path}/backup/import"), body)
        .await;
    assert_eq!(imported.status, StatusCode::OK, "{}", imported.text());
    let modules = browser.get(&format!("{path}/modules")).await.json();
    assert_eq!(
        modules["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["key"] == "command_control")
            .unwrap()["enabled"],
        false
    );
    assert_eq!(h.state.app.db.command_control(guild).await.unwrap(), config);
}
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
            .any(|c| c["path"] == "setup commands show" && c["protected"] == true)
    );
    for path in [
        "play memory",
        "play minesweeper",
        "play higherlower",
        "play anagram",
        "play typing",
        "play guessnumber",
        "duel othello",
        "party nhie",
        "party fun",
    ] {
        assert!(
            initial.json()["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|command| command["path"] == path),
            "quick game missing from command controls: {path}"
        );
    }
    let mut config = pinchora_core::admin::command_control::Config::default();
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

#[tokio::test]
async fn backup_routes_require_csrf_scope_restore_points_and_revalidate_dangerous_payloads() {
    let Some(h) = Harness::new("backup_security_boundaries").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/backup");
    let backup = browser.get(&format!("{path}/export")).await.json();
    let csrf = browser.csrf.take();
    for endpoint in ["preview", "import"] {
        let response = browser
            .write(
                Method::POST,
                &format!("{path}/{endpoint}"),
                json!({"backup":backup,"policy":"replace"}),
            )
            .await;
        assert_eq!(response.status, StatusCode::FORBIDDEN);
        assert_eq!(response.code(), "csrf");
    }
    browser.csrf = csrf;
    for (section, field, value) in [
        ("guilds", "owner_id", json!("123")),
        ("guilds", "webhook_token", json!("secret")),
    ] {
        let mut bad = backup.clone();
        bad["sections"][section][0][field] = value;
        let result = browser
            .write(
                Method::POST,
                &format!("{path}/import"),
                json!({"backup":bad,"policy":"replace"}),
            )
            .await;
        assert!(!result.status.is_success());
    }
    let mut version = backup.clone();
    version["version"] = json!(999);
    assert!(
        !browser
            .write(
                Method::POST,
                &format!("{path}/preview"),
                json!({"backup":version,"policy":"merge"})
            )
            .await
            .status
            .is_success()
    );
    assert!(h.state.app.db.config_restore_points(guild).await.unwrap().is_empty());
    assert_eq!(browser.get(&format!("{path}/export")).await.json(), backup);
    let foreign = guild + 1;
    let foreign_backup = h
        .state
        .app
        .db
        .export_config(foreign, pinchora_core::admin::config_backup::Names::default())
        .await
        .unwrap();
    let point = h
        .state
        .app
        .db
        .import_config(
            foreign,
            &foreign_backup,
            pinchora_core::admin::config_backup::Policy::Merge,
            pinchora_core::admin::config_backup::Names::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        browser.get(&format!("{path}/restore/{point}")).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn backup_utility_and_event_settings_validate_and_localize() {
    let Some(h) = Harness::new("backup_utility_events").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}");
    let exported = browser.get(&format!("{path}/backup/export")).await;
    assert_eq!(exported.status, StatusCode::OK);
    let mut backup = exported.json();
    for section in [
        "utility_settings",
        "sticky_messages",
        "scheduled_messages",
        "event_settings",
    ] {
        assert!(backup["sections"][section].is_array());
    }
    for section in ["utility_settings", "event_settings"] {
        backup["sections"][section] = json!([{"timezone":"Europe/Istanbul"}]);
    }
    let imported = browser
        .write(
            Method::POST,
            &format!("{path}/backup/import"),
            json!({"backup":backup,"policy":"merge"}),
        )
        .await;
    assert_eq!(imported.status, StatusCode::OK, "{}", imported.text());
    assert_eq!(h.state.app.db.utility_timezone(guild).await.unwrap(), "Europe/Istanbul");
    let before = h.state.app.db.config_restore_points(guild).await.unwrap();
    for section in ["utility_settings", "event_settings"] {
        let mut bad = backup.clone();
        bad["sections"][section][0]["timezone"] = json!("Invalid/Zone");
        let preview = browser
            .write(
                Method::POST,
                &format!("{path}/backup/preview"),
                json!({"backup":bad,"policy":"merge"}),
            )
            .await;
        assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
        assert_eq!(preview.json()["can_import"], false);
    }
    let mut bad = backup.clone();
    bad["sections"]["scheduled_messages"] = json!([{
        "id":"1","channel_id":"123","content":"note","embed":null,
        "schedule":{"kind":"interval","seconds":1},"tz":"UTC",
        "next_run_at":"2030-01-01T00:00:00Z","enabled":true
    }]);
    assert!(
        pinchora_commands::validate_backup(
            &serde_json::from_value(bad).unwrap(),
            pinchora_core::infra::i18n::Lang::En
        )
        .is_err()
    );
    assert_eq!(h.state.app.db.config_restore_points(guild).await.unwrap(), before);
    let catalog = browser.get(&format!("{path}/command-control")).await.json();
    for command in [
        "afk",
        "community utility sticky-set",
        "community utility schedule-add",
        "event",
    ] {
        assert!(
            catalog["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["path"] == command && c["protected"] == false)
        );
    }
    for lang in [
        pinchora_core::infra::i18n::Lang::En,
        pinchora_core::infra::i18n::Lang::Tr,
    ] {
        for section in [
            "utility_settings",
            "sticky_messages",
            "scheduled_messages",
            "event_settings",
        ] {
            assert!(pinchora_core::infra::i18n::exact(lang, &format!("config_backup.section.{section}")).is_some());
        }
    }
}
