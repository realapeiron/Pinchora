use crate::*;

#[tokio::test]
async fn leveling_policy_import_and_public_board_require_manager_and_preview() {
    let Some(h) = Harness::new("leveling_parity").await else {
        return;
    };
    let (guild, other, member, admin) = (unique(), unique(), unique(), unique());
    h.add_bot(guild);
    h.add_bot(other);
    let mut browser = h.browser();
    browser
        .login(
            &h,
            admin,
            vec![
                guild_entry(guild, "Reef", true, 0),
                guild_entry(other, "Other", false, 0),
            ],
        )
        .await;
    let path = format!("/api/guilds/{guild}/leveling-parity");
    let initial = browser.get(&path).await;
    assert_eq!(initial.status, StatusCode::OK, "{}", initial.text());
    assert_eq!(initial.json()["message_min"], 15);
    assert_eq!(initial.json()["curve"], "classic");
    let mut config = initial.json();
    config["message_min"] = json!(8);
    config["message_max"] = json!(12);
    let slug = format!("reef-{guild}");
    config["vanity_slug"] = json!(slug);
    let csrf = browser.csrf.take();
    assert_eq!(browser.write(Method::PUT, &path, config.clone()).await.code(), "csrf");
    browser.csrf = csrf;
    assert_eq!(
        browser
            .write(
                Method::PUT,
                &format!("/api/guilds/{other}/leveling-parity"),
                config.clone()
            )
            .await
            .code(),
        "not_manager"
    );
    let saved = browser.write(Method::PUT, &path, config).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(saved.json()["message_min"], 8);
    let preview_path = format!("/api/guilds/{guild}/leveling-parity/import/preview");
    let data = json!([{"id": member.to_string(), "xp": 500}]).to_string();
    let preview = browser
        .write(
            Method::POST,
            &preview_path,
            json!({"format":"json", "data":data, "mode":"merge"}),
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    assert_eq!(preview.json()["count"], 1);
    assert_eq!(preview.json()["changed"], 1);
    assert_eq!(preview.json()["sample"][0]["current_xp"], 0);
    assert_eq!(preview.json()["sample"][0]["result_xp"], 500);
    assert!(
        h.state.app.db.member_rank(guild, member).await.unwrap().is_none(),
        "preview never writes XP"
    );
    let id = preview.json()["preview_id"].as_str().unwrap().to_owned();
    let apply_path = format!("/api/guilds/{guild}/leveling-parity/import/apply");
    let mut second_session = h.browser();
    second_session
        .login(&h, admin, vec![guild_entry(guild, "Reef", true, 0)])
        .await;
    assert_eq!(
        second_session
            .write(Method::POST, &apply_path, json!({"preview_id": id}))
            .await
            .code(),
        "blocked"
    );
    let applied = browser.write(Method::POST, &apply_path, json!({"preview_id":id})).await;
    assert_eq!(applied.status, StatusCode::OK, "{}", applied.text());
    assert_eq!(applied.json()["updated"], 1);
    assert_eq!(
        h.state.app.db.member_rank(guild, member).await.unwrap().unwrap().xp,
        500
    );
    let replay = browser.write(Method::POST, &apply_path, json!({"preview_id":id})).await;
    assert_eq!(replay.status, StatusCode::OK);
    assert_eq!(replay.json(), applied.json());
    let board = browser.get(&format!("/api/public/leaderboard/{slug}")).await;
    assert_eq!(board.status, StatusCode::OK, "{}", board.text());
    assert_eq!(board.json()["leaders"][0]["user_id"], member.to_string());
    sqlx::query("INSERT INTO user_privacy (user_id, hide_leaderboard) VALUES ($1, true)")
        .bind(member as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    let hidden = browser.get(&format!("/api/public/leaderboard/{slug}")).await;
    assert!(hidden.json()["leaders"].as_array().unwrap().is_empty());
    h.state
        .app
        .db
        .set_privacy(member, pinchora_db::Hide::Leaderboard, false)
        .await
        .unwrap();
    h.state
        .app
        .db
        .set_privacy(member, pinchora_db::Hide::Profile, true)
        .await
        .unwrap();
    h.state
        .app
        .db
        .add_event_xp(
            guild,
            member,
            5,
            pinchora_core::progression::leveling::week_start(chrono::Utc::now()),
        )
        .await
        .unwrap();
    let hidden_profile = browser.get(&format!("/api/public/leaderboard/{slug}")).await;
    assert_eq!(hidden_profile.header("cache-control"), "no-store");
    assert!(hidden_profile.json()["leaders"].as_array().unwrap().is_empty());
    assert!(hidden_profile.json()["weekly"].as_array().unwrap().is_empty());
    assert!(!hidden_profile.text().contains(&member.to_string()));
    assert_eq!(
        h.state.app.db.audit_page(guild, None, 10).await.unwrap()[0].action,
        "xp.import.offline"
    );
}

#[tokio::test]
async fn offline_import_rechecks_membership_and_scopes_preview_to_guild() {
    let Some(h) = Harness::new("leveling_import_membership").await else {
        return;
    };
    let (guild, other, user, admin) = (unique(), unique(), unique(), unique());
    h.add_bot(guild);
    h.add_bot(other);
    let mut browser = h.browser();
    browser
        .login(
            &h,
            admin,
            vec![
                guild_entry(guild, "Reef", true, 0),
                guild_entry(other, "Other", true, 0),
            ],
        )
        .await;
    let base = format!("/api/guilds/{guild}/leveling-parity");
    let body = json!({"format":"csv","data":format!("id,xp\n{user},500"),"mode":"replace"});
    h.mock.absent_leveling_members.lock().unwrap().insert((guild, user));
    let denied = browser
        .write(Method::POST, &format!("{base}/import/preview"), body.clone())
        .await;
    assert_eq!(denied.code(), "blocked");
    assert!(h.state.app.db.member_rank(guild, user).await.unwrap().is_none());
    h.mock.absent_leveling_members.lock().unwrap().clear();
    let preview = browser
        .write(Method::POST, &format!("{base}/import/preview"), body)
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    let apply = json!({"preview_id":preview.json()["preview_id"]});
    assert_eq!(
        browser
            .write(
                Method::POST,
                &format!("/api/guilds/{other}/leveling-parity/import/apply"),
                apply.clone()
            )
            .await
            .code(),
        "blocked"
    );
    h.mock.absent_leveling_members.lock().unwrap().insert((guild, user));
    assert_eq!(
        browser
            .write(Method::POST, &format!("{base}/import/apply"), apply)
            .await
            .code(),
        "blocked"
    );
    assert!(h.state.app.db.member_rank(guild, user).await.unwrap().is_none());
    assert!(h.state.app.db.audit_page(guild, None, 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn every_leveling_endpoint_requires_session_manager_and_csrf() {
    let Some(h) = Harness::new("leveling_all_permissions").await else {
        return;
    };
    let guild = unique();
    h.add_bot(guild);
    let mut anon = h.browser();
    let mut member = h.browser();
    member
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    let mut admin = h.browser();
    admin
        .login(&h, unique(), vec![guild_entry(guild, "Reef", true, 0)])
        .await;
    admin.csrf = None;
    for (method, suffix, body) in [
        (Method::GET, "", json!({})),
        (Method::PUT, "", json!({})),
        (Method::GET, "/highlights", json!({})),
        (Method::GET, "/voice-roles", json!({})),
        (Method::PUT, "/voice-roles/9001", json!({"level":1})),
        (Method::DELETE, "/voice-roles/9001", json!({})),
        (
            Method::POST,
            "/import/preview",
            json!({"format":"csv","data":"id,xp\n1,2","mode":"merge"}),
        ),
        (
            Method::POST,
            "/import/apply",
            json!({"preview_id":"00000000000000000000000000000000"}),
        ),
    ] {
        let path = format!("/api/guilds/{guild}/leveling-parity{suffix}");
        let unauthorized = if method == Method::GET {
            anon.get(&path).await
        } else {
            anon.write(method.clone(), &path, body.clone()).await
        };
        assert!(!unauthorized.status.is_success(), "{path}");
        let denied = if method == Method::GET {
            member.get(&path).await
        } else {
            member.write(method.clone(), &path, body.clone()).await
        };
        assert_eq!(denied.code(), "not_manager", "{path}: {}", denied.text());
        if method != Method::GET {
            assert_eq!(admin.write(method, &path, body).await.code(), "csrf", "{path}");
        }
    }
}

#[tokio::test]
async fn voice_reward_configuration_uses_role_safety_rules() {
    let Some(h) = Harness::new("voice_reward_safety").await else {
        return;
    };
    let (guild, admin) = (unique(), unique());
    h.add_bot(guild);
    h.mock.member_roles.lock().unwrap().insert(admin, vec![REGULAR]);
    let mut browser = h.browser();
    browser
        .login(&h, admin, vec![guild_entry(guild, "Reef", false, MANAGE_GUILD)])
        .await;
    for role in [guild, ADMIN, BOOSTER, HELPER, REGULAR] {
        let result = browser
            .write(
                Method::PUT,
                &format!("/api/guilds/{guild}/leveling-parity/voice-roles/{role}"),
                json!({"level":1}),
            )
            .await;
        assert!(!result.status.is_success(), "unsafe role {role}: {}", result.text());
    }
    let result = browser
        .write(
            Method::PUT,
            &format!("/api/guilds/{guild}/leveling-parity/voice-roles/{FAN}"),
            json!({"level":1}),
        )
        .await;
    assert_eq!(result.status, StatusCode::OK, "{}", result.text());
    assert_eq!(h.state.app.db.voice_level_roles(guild).await.unwrap().len(), 1);
    let removed = browser
        .write(
            Method::DELETE,
            &format!("/api/guilds/{guild}/leveling-parity/voice-roles/{FAN}"),
            json!({}),
        )
        .await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert!(h.state.app.db.voice_level_roles(guild).await.unwrap().is_empty());
}
