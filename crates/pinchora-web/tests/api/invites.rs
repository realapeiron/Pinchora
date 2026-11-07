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
