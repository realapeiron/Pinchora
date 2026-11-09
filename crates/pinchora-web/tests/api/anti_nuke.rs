use crate::*;
use pinchora_core::moderation::anti_nuke::Config;

#[tokio::test]
async fn antinuke_web_checks_live_owner_csrf_managed_guild_and_private_audit() {
    let Some(h) = Harness::new("antinuke_web_authorization").await else {
        return;
    };
    let guild = unique();
    let owner_id = unique();
    h.mock.guild_owners.lock().unwrap().insert(guild, owner_id);
    let trusted = unique();
    h.add_bot(guild);
    let path = format!("/api/guilds/{guild}/anti_nuke");
    let mut owner = h.browser();
    owner
        .login(&h, owner_id, vec![guild_entry(guild, "Reef", true, MANAGE_GUILD)])
        .await;
    let c = Config {
        owner_only_config: false,
        trusted_users: vec![trusted.to_string()],
        ..Config::default()
    };
    let csrf = owner.csrf.take();
    let denied = owner.write(Method::PUT, &path, json!(c)).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    owner.csrf = csrf;
    let saved = owner.write(Method::PUT, &path, json!(c)).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let audit = owner.get(&format!("/api/guilds/{guild}/audit")).await.json();
    let row = &audit["items"][0];
    assert_eq!(row["action"], "antinuke.set");
    assert_eq!(row["after"]["trusted_count"], 1);
    assert!(
        row["after"].get("trusted_users").is_none(),
        "audit JSON must not retain unindexed user IDs"
    );
    let mut staff = h.browser();
    staff
        .login(&h, trusted, vec![guild_entry(guild, "Reef", false, MANAGE_GUILD)])
        .await;
    let mut changed = c.clone();
    changed.trusted_users.clear();
    let denied = staff.write(Method::PUT, &path, json!(changed)).await;
    assert!(!denied.status.is_success(), "trusted staff may not edit owner trust");
    let denied_module = staff
        .write(
            Method::PUT,
            &format!("/api/guilds/{guild}/modules/anti_nuke"),
            json!({"enabled":false}),
        )
        .await;
    assert!(
        !denied_module.status.is_success(),
        "the module switch cannot bypass the owner's policy"
    );
    let mut stranger = h.browser();
    stranger
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    assert!(
        !stranger.get(&path).await.status.is_success(),
        "ManagedGuild is required"
    );
    assert_eq!(h.state.app.db.antinuke_config(guild).await.unwrap(), c);
}

#[tokio::test]
async fn antinuke_web_restore_is_authorized_and_audited() {
    let Some(h) = Harness::new("antinuke_web_restore").await else {
        return;
    };
    let guild = unique();
    let owner_id = unique();
    h.mock.guild_owners.lock().unwrap().insert(guild, owner_id);
    h.add_bot(guild);
    let mut owner = h.browser();
    owner
        .login(&h, owner_id, vec![guild_entry(guild, "Reef", true, MANAGE_GUILD)])
        .await;
    let event = pinchora_db::AntiNukeEvent {
        id: unique() as i64,
        guild_id: guild as i64,
        actor_id: unique() as i64,
        target_user_id: None,
        target_id: Some(unique() as i64),
        kind: "role_delete".into(),
        count: 2,
        action_taken: "observed".into(),
        details: json!({"window_secs":5}),
        created_at: chrono::Utc::now(),
        reverted: false,
        recovery_state: "ready".into(),
    };
    h.state.app.db.open_antinuke_event(&event).await.unwrap();
    let path = format!("/api/guilds/{guild}/anti_nuke/events/{}/restore", event.id);
    let denied = owner.write(Method::POST, &path, json!({"confirm":false})).await;
    assert!(!denied.status.is_success());
    let restored = owner.write(Method::POST, &path, json!({"confirm":true})).await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.text());
    let audit = owner.get(&format!("/api/guilds/{guild}/audit")).await.json();
    assert_eq!(audit["items"][0]["action"], "antinuke.restore");
    assert_eq!(audit["items"][0]["source"], "web");
}
