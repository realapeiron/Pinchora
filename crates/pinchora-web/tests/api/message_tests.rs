use super::*;

#[tokio::test]
async fn message_tests_send_drafts_and_refuse_unsafe_delivery() {
    let Some(h) = Harness::new("message_tests").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}");
    let path = format!("{base}/messages/welcome/test");
    let body = json!({"message":"Ahoy {user} at {server}!", "channel_id":GENERAL.to_string()});
    let anonymous = h.browser().write(Method::POST, &path, body.clone()).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    let bad_kind = browser
        .write(Method::POST, &format!("{base}/messages/other/test"), body.clone())
        .await;
    assert_eq!(
        (bad_kind.status, bad_kind.code().as_str()),
        (StatusCode::BAD_REQUEST, "malformed")
    );
    let off = browser.write(Method::POST, &path, body.clone()).await;
    assert_eq!(
        (off.status, off.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "blocked")
    );
    let outsider = unique();
    let mut no_access = h.browser();
    no_access
        .login(
            &h,
            outsider,
            vec![guild_entry(guild, "Pinchora Reef", false, SEND_MESSAGES)],
        )
        .await;
    let denied = no_access.write(Method::POST, &path, body.clone()).await;
    assert_eq!(
        (denied.status, denied.code().as_str()),
        (StatusCode::FORBIDDEN, "not_manager")
    );
    let saved = browser.write(Method::PATCH, &format!("{base}/settings"), json!({"welcome_enabled":true,"welcome_channel_id":GENERAL.to_string(),"goodbye_channel_id":GENERAL.to_string()})).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let missing = browser
        .write(Method::POST, &path, json!({"message":"Hi", "channel_id":null}))
        .await;
    assert_eq!(
        (missing.status, missing.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "blocked")
    );
    h.mock.refuse_post.store(true, Ordering::SeqCst);
    let refused = browser.write(Method::POST, &path, body.clone()).await;
    assert_eq!(
        (refused.status, refused.code().as_str()),
        (StatusCode::BAD_GATEWAY, "discord")
    );
    assert!(
        refused.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Discord refused")
    );
    h.mock.refuse_post.store(false, Ordering::SeqCst);
    let sent = browser.write(Method::POST, &path, body.clone()).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    assert_eq!(sent.json(), json!({"posted":true}));
    {
        let posts = h.mock.posts.lock().unwrap();
        assert!(posts.last().unwrap().1.contains("Ahoy @user") && posts.last().unwrap().1.contains("Pinchora Reef"));
    }
    let limited = browser.write(Method::POST, &path, body).await;
    assert_eq!(
        (limited.status, limited.code().as_str()),
        (StatusCode::TOO_MANY_REQUESTS, "rate_limited")
    );
}

#[tokio::test]
async fn goodbye_and_dm_tests_and_discord_permission_payload() {
    let Some(h) = Harness::new("goodbye_dm_tests").await else {
        return;
    };
    let (second, _, mut second_browser) = h.managed_server().await;
    let base = format!("/api/guilds/{second}");
    let saved = second_browser.write(Method::PATCH, &format!("{base}/settings"), json!({"welcome_enabled":true,"welcome_channel_id":GENERAL.to_string(),"goodbye_channel_id":GENERAL.to_string()})).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let goodbye = second_browser
        .write(
            Method::POST,
            &format!("{base}/messages/goodbye/test"),
            json!({"message":"Bye {user}","channel_id":GENERAL.to_string()}),
        )
        .await;
    assert_eq!(goodbye.status, StatusCode::OK, "{}", goodbye.text());
    let delivery = second_browser
        .write(
            Method::PUT,
            &format!("{base}/cards/welcome"),
            json!({"design":null,"delivery":"dm"}),
        )
        .await;
    assert_eq!(delivery.status, StatusCode::OK, "{}", delivery.text());
    let dm = second_browser
        .write(
            Method::POST,
            &format!("{base}/messages/welcome_dm/test"),
            json!({"message":"Welcome {user}"}),
        )
        .await;
    assert_eq!(dm.status, StatusCode::OK, "{}", dm.text());
    assert_eq!(h.mock.dm_opens.load(Ordering::SeqCst), 1);
    let channel_while_dm = second_browser
        .write(
            Method::POST,
            &format!("{base}/messages/welcome/test"),
            json!({"message":"Hello", "channel_id":GENERAL.to_string()}),
        )
        .await;
    assert_eq!(
        (channel_while_dm.status, channel_while_dm.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "blocked")
    );
    let voice = second_browser
        .write(
            Method::POST,
            &format!("{base}/messages/goodbye/test"),
            json!({"message":"Bye", "channel_id":LOUNGE.to_string()}),
        )
        .await;
    assert_eq!(
        (voice.status, voice.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "invalid")
    );
    assert_eq!(fields_of(&voice), ["channel_id"]);
    h.mock.stats_public_channels.store(true, Ordering::SeqCst);
    let (third, _, mut third_browser) = h.managed_server().await;
    let info = third_browser.get(&format!("/api/guilds/{third}/discord")).await;
    assert_eq!(info.status, StatusCode::OK, "{}", info.text());
    assert_eq!(
        info.json()["channels"][1]["missing_permissions"],
        json!(["view_channel"])
    );
}

#[tokio::test]
async fn message_test_accepts_visible_draft_channel_and_refuses_foreign_guild() {
    let Some(h) = Harness::new("message_test_target").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}");
    assert_eq!(
        browser
            .write(
                Method::PATCH,
                &format!("{base}/settings"),
                json!({"welcome_enabled":true,"welcome_channel_id":GENERAL.to_string()})
            )
            .await
            .status,
        StatusCode::OK
    );
    let path = format!("{base}/messages/welcome/test");
    let visible = browser
        .write(
            Method::POST,
            &path,
            json!({"message":"Hello", "channel_id":NEWS.to_string()}),
        )
        .await;
    assert_eq!(visible.status, StatusCode::OK, "{}", visible.text());
    let (other_guild, _, _) = h.managed_server().await;
    let foreign_channel = unique();
    *h.mock.foreign_channel.lock().unwrap() = Some((other_guild, foreign_channel));
    let foreign = browser
        .write(
            Method::POST,
            &path,
            json!({"message":"Hello", "channel_id":foreign_channel.to_string()}),
        )
        .await;
    assert_eq!(foreign.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", foreign.text());
    assert_eq!(h.mock.posts.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn text_delivery_ignores_image_permissions_but_card_requires_them() {
    let Some(h) = Harness::new("message_test_delivery").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}");
    h.mock.no_image_permissions.store(true, Ordering::SeqCst);
    assert_eq!(
        browser
            .write(
                Method::PATCH,
                &format!("{base}/settings"),
                json!({"welcome_enabled":true,"welcome_channel_id":GENERAL.to_string()})
            )
            .await
            .status,
        StatusCode::OK
    );
    let path = format!("{base}/messages/welcome/test");
    assert_eq!(
        browser
            .write(
                Method::PUT,
                &format!("{base}/cards/welcome"),
                json!({"design":null,"delivery":"text_only"})
            )
            .await
            .status,
        StatusCode::OK
    );
    let text = browser
        .write(
            Method::POST,
            &path,
            json!({"message":"Hello", "channel_id":GENERAL.to_string()}),
        )
        .await;
    assert_eq!(text.status, StatusCode::OK, "{}", text.text());
    assert_eq!(
        browser
            .write(
                Method::PUT,
                &format!("{base}/cards/welcome"),
                json!({"design":null,"delivery":"with_message"})
            )
            .await
            .status,
        StatusCode::OK
    );
    let card = browser
        .write(
            Method::POST,
            &path,
            json!({"message":"Hello", "channel_id":GENERAL.to_string()}),
        )
        .await;
    assert_eq!(
        (card.status, card.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "blocked")
    );
}

#[tokio::test]
async fn dm_open_distinguishes_privacy_from_discord_outage_and_localizes() {
    let Some(h) = Harness::new("message_test_dm_failure").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}");
    assert_eq!(
        browser
            .write(
                Method::PATCH,
                &format!("{base}/settings"),
                json!({"welcome_enabled":true})
            )
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        browser
            .write(
                Method::PUT,
                &format!("{base}/cards/welcome"),
                json!({"design":null,"delivery":"dm"})
            )
            .await
            .status,
        StatusCode::OK
    );
    let path = format!("{base}/messages/welcome_dm/test");
    h.state.app.set_user_lang(owner, Some(Lang::Tr)).await.unwrap();
    h.mock.dm_open_failure.store(1, Ordering::SeqCst);
    let privacy = browser.write(Method::POST, &path, json!({"message":"Hi"})).await;
    assert_eq!(
        (privacy.status, privacy.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "blocked")
    );
    assert!(
        privacy.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("özel mesajlarını")
    );
    h.mock.dm_open_failure.store(2, Ordering::SeqCst);
    let outage = browser.write(Method::POST, &path, json!({"message":"Hi"})).await;
    assert_eq!(
        (outage.status, outage.code().as_str()),
        (StatusCode::BAD_GATEWAY, "discord")
    );
    assert!(outage.json()["error"]["message"].as_str().unwrap().contains("şu anda"));
    let kind = browser
        .write(
            Method::POST,
            &format!("{base}/messages/other/test"),
            json!({"message":"Hi"}),
        )
        .await;
    assert_eq!(kind.status, StatusCode::BAD_REQUEST);
    assert!(kind.json()["error"]["message"].as_str().unwrap().contains("Mesaj türü"));
    let invalid = browser.write(Method::POST, &path, json!({"message":""})).await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        invalid.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Bir mesaj yaz")
    );
}

#[tokio::test]
async fn configured_channel_hidden_from_delegate_is_not_tested() {
    let Some(h) = Harness::new("message_test_hidden").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}");
    assert_eq!(
        browser
            .write(
                Method::PATCH,
                &format!("{base}/settings"),
                json!({"welcome_enabled":true,"welcome_channel_id":GENERAL.to_string()})
            )
            .await
            .status,
        StatusCode::OK
    );
    let delegate = unique();
    h.mock.member_roles.lock().unwrap().insert(delegate, vec![FAN]);
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
    let mut manager = h.browser();
    manager
        .login(&h, delegate, vec![guild_entry(guild, "Pinchora Reef", false, 0)])
        .await;
    h.mock.hidden_from_fan.store(true, Ordering::SeqCst);
    let result = manager
        .write(
            Method::POST,
            &format!("{base}/messages/welcome/test"),
            json!({"message":"Hi", "channel_id":GENERAL.to_string()}),
        )
        .await;
    assert_eq!(
        (result.status, result.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "blocked")
    );
    assert!(h.mock.posts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn unknown_overwrites_make_no_post_claim_and_discord_decides_test() {
    let Some(h) = Harness::new("message_test_overwrites").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    h.mock.channel_overwrites_unknown.store(true, Ordering::SeqCst);
    let base = format!("/api/guilds/{guild}");
    assert_eq!(
        browser
            .write(
                Method::PATCH,
                &format!("{base}/settings"),
                json!({"welcome_enabled":true,"welcome_channel_id":GENERAL.to_string()})
            )
            .await
            .status,
        StatusCode::OK
    );
    let info = browser.get(&format!("{base}/discord")).await;
    assert_eq!(info.json()["channels"][1]["posting_permissions_known"], false);
    assert_eq!(info.json()["channels"][1]["missing_permissions"], json!([]));
    h.mock.refuse_post.store(true, Ordering::SeqCst);
    let result = browser
        .write(
            Method::POST,
            &format!("{base}/messages/welcome/test"),
            json!({"message":"Hi", "channel_id":GENERAL.to_string()}),
        )
        .await;
    assert_eq!(
        (result.status, result.code().as_str()),
        (StatusCode::BAD_GATEWAY, "discord")
    );
}
