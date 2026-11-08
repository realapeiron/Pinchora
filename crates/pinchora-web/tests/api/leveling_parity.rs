use super::*;

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
    assert_eq!(replay.status, StatusCode::UNPROCESSABLE_ENTITY);
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
    assert_eq!(
        h.state.app.db.audit_page(guild, None, 10).await.unwrap()[0].action,
        "xp.import.offline"
    );
}
