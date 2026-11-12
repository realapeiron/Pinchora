use crate::*;

async fn wait_for_settings_row_lock(db: &Db, blocker: i32) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)) AND query LIKE '%FOR UPDATE%')",
            )
            .bind(blocker)
            .fetch_one(db.pool())
            .await
            .unwrap();
            if waiting { break }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("API request must reach the locked settings row");
}

#[tokio::test]
async fn api_rechecks_original_patch_after_a_stale_field_becomes_current() {
    let Some(h) = Harness::new("settings_api_race_match").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    h.state.app.db.guild_settings(guild).await.unwrap();
    let mut tx = h.state.app.db.pool().begin().await.unwrap();
    let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE guilds SET points_name='coins' WHERE id=$1")
        .bind(guild as i64)
        .execute(&mut *tx)
        .await
        .unwrap();
    let path = format!("/api/guilds/{guild}/settings");
    let (result, ()) = tokio::join!(
        browser.write(
            Method::PATCH,
            &path,
            json!({"points_name":"stars","expect":{"points_name":"coins"}})
        ),
        async {
            wait_for_settings_row_lock(&h.state.app.db, blocker).await;
            tx.commit().await.unwrap();
        }
    );
    assert_eq!(result.status, StatusCode::OK, "{}", result.text());
    assert_eq!(result.json()["skipped"], json!([]));
    assert_eq!(result.json()["points_name"], "stars");
}

#[tokio::test]
async fn api_checks_repeated_discord_target_and_skips_it_if_locked_row_moves() {
    let Some(h) = Harness::new("settings_api_race_discord").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/settings");
    let initial = browser
        .write(Method::PATCH, &path, json!({"welcome_channel_id":GENERAL.to_string()}))
        .await;
    assert_eq!(initial.status, StatusCode::OK, "{}", initial.text());
    let mut tx = h.state.app.db.pool().begin().await.unwrap();
    let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE guilds SET welcome_channel_id=$1 WHERE id=$2")
        .bind(NEWS as i64)
        .bind(guild as i64)
        .execute(&mut *tx)
        .await
        .unwrap();
    let (result, ()) = tokio::join!(
        browser.write(
            Method::PATCH,
            &path,
            json!({"welcome_channel_id":GENERAL.to_string(),"expect":{}})
        ),
        async {
            wait_for_settings_row_lock(&h.state.app.db, blocker).await;
            tx.commit().await.unwrap();
        }
    );
    assert_eq!(result.status, StatusCode::OK, "{}", result.text());
    assert_eq!(result.json()["skipped"], json!(["welcome_channel_id"]));
    assert_eq!(result.json()["welcome_channel_id"], NEWS.to_string());
    sqlx::query("UPDATE guilds SET welcome_channel_id=$1 WHERE id=$2")
        .bind(999_999_i64)
        .bind(guild as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    let invalid = browser
        .write(Method::PATCH, &path, json!({"welcome_channel_id":"999999","expect":{}}))
        .await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", invalid.text());
    assert_eq!(invalid.json()["error"]["field"], "welcome_channel_id");
    let stale = browser
        .write(
            Method::PATCH,
            &path,
            json!({
                "welcome_channel_id":"888888", "expect":{"welcome_channel_id":GENERAL.to_string()}
            }),
        )
        .await;
    assert_eq!(stale.status, StatusCode::OK, "{}", stale.text());
    assert_eq!(stale.json()["skipped"], json!(["welcome_channel_id"]));
    assert_eq!(stale.json()["welcome_channel_id"], "999999");
}

#[tokio::test]
async fn settings_patch_reports_fields_changed_since_the_save() {
    let Some(h) = Harness::new("conditional_settings").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/settings");
    let initial = browser.get(&format!("/api/guilds/{guild}")).await.json()["settings"].clone();
    let first = browser
        .write(
            Method::PATCH,
            &path,
            json!({
                "points_name":"coins", "points_emoji":"C",
                "expect":{"points_name":initial["points_name"],"points_emoji":initial["points_emoji"]}
            }),
        )
        .await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    assert_eq!(first.json()["skipped"], json!([]));
    let other = browser
        .write(Method::PATCH, &path, json!({"points_name":"stars"}))
        .await;
    assert_eq!(other.status, StatusCode::OK, "{}", other.text());
    assert_eq!(other.json()["skipped"], json!([]));
    let undo = browser
        .write(
            Method::PATCH,
            &path,
            json!({
                "points_name":initial["points_name"], "points_emoji":initial["points_emoji"],
                "expect":{"points_name":"coins", "points_emoji":"C"}
            }),
        )
        .await;
    assert_eq!(undo.status, StatusCode::OK, "{}", undo.text());
    assert_eq!(undo.json()["skipped"], json!(["points_name"]));
    assert_eq!(undo.json()["points_name"], "stars");
    assert_eq!(undo.json()["points_emoji"], initial["points_emoji"]);
    let all_stale = browser
        .write(
            Method::PATCH,
            &path,
            json!({
                "points_name":"ignored", "expect":{"points_name":"coins"}
            }),
        )
        .await;
    assert_eq!(all_stale.status, StatusCode::OK);
    assert_eq!(all_stale.json()["skipped"], json!(["points_name"]));
    assert_eq!(all_stale.json()["points_name"], "stars");
}

#[tokio::test]
async fn module_put_refuses_a_stale_switch_and_command_control_can_return() {
    let Some(h) = Harness::new("conditional_module").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/modules/command_control");
    let off = browser
        .write(Method::PUT, &path, json!({"enabled":false,"expect_enabled":true}))
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    assert_eq!(off.json()["enabled"], false);
    let stale = browser
        .write(Method::PUT, &path, json!({"enabled":true,"expect_enabled":true}))
        .await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{}", stale.text());
    assert_eq!(stale.json()["error"]["code"], "conflict");
    let state = browser.get(&format!("/api/guilds/{guild}/modules")).await;
    assert_eq!(
        state.json()["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["key"] == "command_control")
            .unwrap()["enabled"],
        false
    );
    let on = browser
        .write(Method::PUT, &path, json!({"enabled":true,"expect_enabled":false}))
        .await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.text());
    assert_eq!(on.json()["enabled"], true);
}

#[tokio::test]
async fn module_switch_requires_the_affected_page_area() {
    let Some(h) = Harness::new("module_switch_areas").await else {
        return;
    };
    let (guild, owner, _) = h.managed_server().await;
    let user = unique();
    h.mock.member_roles.lock().unwrap().insert(user, vec![FAN]);
    h.state
        .app
        .db
        .replace_dashboard_managers(
            guild,
            owner,
            "Owner",
            &[pinchora_db::DashboardManager {
                role_id: FAN as i64,
                areas: vec!["configuration".into()],
            }],
        )
        .await
        .unwrap();
    let mut delegate = h.browser();
    delegate
        .login(&h, user, vec![guild_entry(guild, "Modules", false, 0)])
        .await;
    let base = format!("/api/guilds/{guild}");
    assert_eq!(delegate.get(&format!("{base}/modules")).await.status, StatusCode::OK);
    h.state.app.set_guild_lang(guild, Some(Lang::Tr)).await.unwrap();
    for key in [
        "command_control",
        "moderation",
        "role_panels",
        "tickets",
        "suggestions",
        "achievements",
        "utility",
        "feeds",
        "stats",
        "economy",
    ] {
        let response = delegate
            .write(Method::PUT, &format!("{base}/modules/{key}"), json!({"enabled":false}))
            .await;
        assert_eq!(response.status, StatusCode::FORBIDDEN, "{key}: {}", response.text());
        assert_eq!(
            response.json()["error"]["message"],
            pinchora_core::t!(Lang::Tr, "modules.access")
        );
    }
    let allowed = delegate
        .write(Method::PUT, &format!("{base}/modules/points"), json!({"enabled":false}))
        .await;
    assert_eq!(allowed.status, StatusCode::OK, "{}", allowed.text());
    assert_eq!(allowed.json()["enabled"], false);
}

#[tokio::test]
async fn effective_prefix_and_accent_values_round_trip_through_expect() {
    let Some(h) = Harness::new("settings_effective_expect").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/settings");
    let custom = browser
        .write(Method::PATCH, &path, json!({"prefix":"!","welcome_accent":"#AABBCC"}))
        .await;
    assert_eq!(custom.status, StatusCode::OK, "{}", custom.text());
    let default = browser
        .write(Method::PATCH, &path, json!({"prefix":null,"welcome_accent":null}))
        .await;
    assert_eq!(default.status, StatusCode::OK, "{}", default.text());
    assert_eq!(default.json()["prefix"], "p!");
    let undo = browser
        .write(
            Method::PATCH,
            &path,
            json!({
                "prefix":"!", "welcome_accent":"#AABBCC",
                "expect":{"prefix":default.json()["prefix"],"welcome_accent":default.json()["welcome_accent"]}
            }),
        )
        .await;
    assert_eq!(undo.status, StatusCode::OK, "{}", undo.text());
    assert_eq!(undo.json()["skipped"], json!([]));
    assert_eq!(undo.json()["prefix"], "!");
    assert_eq!(undo.json()["welcome_accent"], "#aabbcc");
    let same = browser
        .write(
            Method::PATCH,
            &path,
            json!({
                "prefix":"?", "welcome_accent":null,
                "expect":{"prefix":"!","welcome_accent":"#AABBCC"}
            }),
        )
        .await;
    assert_eq!(same.status, StatusCode::OK, "{}", same.text());
    assert_eq!(same.json()["skipped"], json!([]));
}

#[tokio::test]
async fn malformed_expect_uses_the_guild_language() {
    let Some(h) = Harness::new("settings_expect_language").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    h.state.app.set_guild_lang(guild, Some(Lang::Tr)).await.unwrap();
    let path = format!("/api/guilds/{guild}/settings");
    for expect in [json!({"unknown":true}), json!({"points_name":42}), json!({"expect":{}})] {
        let response = browser
            .write(Method::PATCH, &path, json!({"points_name":"coins","expect":expect}))
            .await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{}", response.text());
        assert_eq!(
            response.json()["error"]["message"],
            pinchora_core::t!(Lang::Tr, "settings-error.expect")
        );
    }
}
