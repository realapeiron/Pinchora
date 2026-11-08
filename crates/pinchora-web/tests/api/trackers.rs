use super::*;

#[tokio::test]
async fn tracker_settings_require_manager_csrf_and_write_audit() {
    let Some(h) = Harness::new("trackers_api").await else {
        return;
    };
    let (guild, user, mut browser) = h.managed_server().await;
    h.state
        .app
        .db
        .save_riot_link(
            user,
            "lol",
            &pinchora_db::RiotLink {
                puuid: "export-puuid".into(),
                game_name: "Çağrı".into(),
                tag_line: "TR1".into(),
                region: "tr1".into(),
            },
        )
        .await
        .unwrap();
    h.state
        .app
        .db
        .save_riot_link(
            user + 1,
            "lol",
            &pinchora_db::RiotLink {
                puuid: "another-user".into(),
                game_name: "Other".into(),
                tag_line: "TR1".into(),
                region: "tr1".into(),
            },
        )
        .await
        .unwrap();
    let exported = browser.get("/api/me/trackers").await;
    assert_eq!(exported.status, StatusCode::OK);
    assert_eq!(exported.json()["riot_links"].as_array().unwrap().len(), 1);
    assert_eq!(exported.json()["riot_links"][0]["puuid"], "export-puuid");
    let path = format!("/api/guilds/{guild}/trackers");
    let initial = browser.get(&path).await;
    assert_eq!(initial.status, StatusCode::OK, "{}", initial.text());
    assert_eq!(initial.json()["mc_watch"], json!([]));
    let body = json!({"patch_channels":{"lol":GENERAL.to_string()},"mc_watch":[{"host":"8.8.8.8:25565","channel_id":GENERAL.to_string()}],"mc_news_channel_id":GENERAL.to_string()});
    let csrf = browser.csrf.take();
    assert_eq!(
        browser.write(Method::PUT, &path, body.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    browser.csrf = csrf;
    let stranger = guild + 1;
    h.add_bot(stranger);
    assert_eq!(
        browser.get(&format!("/api/guilds/{stranger}/trackers")).await.status,
        StatusCode::FORBIDDEN
    );
    let saved = browser.write(Method::PUT, &path, body.clone()).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(saved.json()["patch_channels"]["lol"], GENERAL.to_string());
    assert_eq!(browser.get(&path).await.json()["mc_watch"], saved.json()["mc_watch"]);
    let mut forged = body.clone();
    forged["mc_watch"][0]["message_id"] = json!("123");
    let saved = browser.write(Method::PUT, &path, forged).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert!(saved.json()["mc_watch"][0]["message_id"].is_null());
    h.state
        .app
        .db
        .tracker_watch_message(guild, "8.8.8.8:25565", GENERAL, 456)
        .await
        .unwrap();
    let saved = browser.write(Method::PUT, &path, body.clone()).await;
    assert_eq!(saved.json()["mc_watch"][0]["message_id"], "456");
    let mut oversized = body.clone();
    oversized["mc_watch"] = json!(
        (0..21)
            .map(|i| json!({"host":format!("host{i}.test"),"channel_id":GENERAL.to_string()}))
            .collect::<Vec<_>>()
    );
    assert_ne!(
        browser.write(Method::PUT, &path, oversized).await.status,
        StatusCode::OK
    );
    let mut invalid = body;
    invalid["mc_watch"][0]["host"] = json!("bad/path");
    assert_ne!(browser.write(Method::PUT, &path, invalid).await.status, StatusCode::OK);
    let audit = browser.get(&format!("/api/guilds/{guild}/audit")).await.json();
    assert!(
        audit["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["action"] == "trackers.set")
    );
}
