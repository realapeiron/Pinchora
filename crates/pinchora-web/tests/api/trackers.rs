use super::*;

#[tokio::test]
async fn tracker_settings_require_manager_csrf_and_write_audit() {
    let Some(h) = Harness::new("trackers_api").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
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
