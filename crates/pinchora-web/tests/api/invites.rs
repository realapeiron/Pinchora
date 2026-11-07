//! Invite routes with both the guild and item IDs in their paths.

use super::*;

#[tokio::test]
async fn reward_and_member_routes_accept_nested_path_ids() {
    let Some(h) = Harness::new("invite_paths").await else {
        return;
    };
    let (guild, user, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/invites");

    let added = browser
        .write(
            Method::PUT,
            &format!("{base}/rewards/5"),
            json!({ "role_id": FAN.to_string() }),
        )
        .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.text());
    assert_eq!(added.json()["role_id"], FAN.to_string());

    let member = browser.get(&format!("{base}/member/{user}")).await;
    assert_eq!(member.status, StatusCode::OK, "{}", member.text());
    assert_eq!(member.json()["score"]["user_id"], user.to_string());

    let removed = browser
        .write(Method::DELETE, &format!("{base}/rewards/5"), json!({}))
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{}", removed.text());
    assert!(
        browser.get(&base).await.json()["rewards"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let rows = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    let actions: Vec<_> = rows.iter().map(|row| row.action.as_str()).collect();
    assert!(actions.contains(&"invites.reward.put"));
    assert!(actions.contains(&"invites.reward.delete"));
}

#[tokio::test]
async fn settings_edit_is_audited_with_the_manager() {
    let Some(h) = Harness::new("invite_settings_audit").await else {
        return;
    };
    let (guild, user, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/invites");
    let mut settings = browser.get(&base).await.json()["settings"].clone();
    settings["temp_enabled"] = json!(true);
    let saved = browser.write(Method::PUT, &format!("{base}/settings"), settings).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let rows = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert_eq!(rows[0].action, "invites.settings.put");
    assert_eq!(rows[0].actor_id, user as i64);
}

#[tokio::test]
async fn bonus_can_be_the_first_invite_action_on_a_server() {
    let Some(h) = Harness::new("invite_bonus_first").await else {
        return;
    };
    let (guild, user, _) = h.managed_server().await;
    assert_eq!(h.state.app.db.change_invite_bonus(guild, user, 3).await.unwrap(), 3);
    assert_eq!(h.state.app.db.invite_score(guild, user).await.unwrap().bonus, 3);
}

#[tokio::test]
async fn dashboard_and_validation_follow_the_server_language() {
    let Some(h) = Harness::new("invites_turkish").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    h.state
        .app
        .set_guild_lang(guild, Some(pinchora_core::i18n::Lang::Tr))
        .await
        .unwrap();
    let base = format!("/api/guilds/{guild}/invites");
    let response = browser.get(&base).await;
    assert_eq!(response.json()["strings"]["INVITES"], "DAVETLER");
    assert_eq!(response.json()["language"], "tr");
    let mut settings = response.json()["settings"].clone();
    settings["fake_days"] = json!(366);
    let invalid = browser.write(Method::PUT, &format!("{base}/settings"), settings).await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(invalid.text().contains("Hesap yaşı"));
}
