//! Utility API routes through the same managed-guild browser harness as the other pages.

use super::*;

#[tokio::test]
async fn schedule_can_be_previewed_edited_paused_and_removed() {
    let Some(h) = Harness::new("utility_schedule_api").await else {
        return;
    };
    let (guild, _owner, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/utility");
    let empty = browser.get(&base).await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.text());
    assert_eq!(empty.json()["scheduled"], json!([]));

    let draft = json!({
        "channel_id": GENERAL.to_string(),
        "content": "Reef cleanup",
        "embed": null,
        "schedule": {"kind":"daily","hour":9,"minute":0},
        "tz": "+03:00",
        "next_run_at": (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
    });
    let mut broad = draft.clone();
    broad["content"] = json!("@everyone Reef cleanup");
    let refused = browser.write(Method::POST, &format!("{base}/scheduled"), broad).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", refused.text());
    assert_eq!(fields_of(&refused), ["content"]);
    let created = browser
        .write(Method::POST, &format!("{base}/scheduled"), draft.clone())
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    let id = created.json()["id"].as_str().unwrap().to_owned();

    let preview = browser.get(&format!("{base}/scheduled/{id}/preview")).await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    assert_eq!(preview.json()["runs"].as_array().unwrap().len(), 5);

    let mut edit = draft;
    edit["content"] = json!("Reef cleanup at noon");
    let edited = browser
        .write(Method::PUT, &format!("{base}/scheduled/{id}"), edit)
        .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text());
    for enabled in [false, true] {
        let patched = browser
            .write(
                Method::PATCH,
                &format!("{base}/scheduled/{id}"),
                json!({"enabled":enabled}),
            )
            .await;
        assert_eq!(patched.status, StatusCode::OK, "{}", patched.text());
    }
    let listed = browser.get(&base).await.json();
    assert_eq!(listed["scheduled"][0]["content"], "Reef cleanup at noon");
    assert_eq!(listed["scheduled"][0]["enabled"], true);

    let removed = browser
        .write(Method::DELETE, &format!("{base}/scheduled/{id}"), json!({}))
        .await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(browser.get(&base).await.json()["scheduled"], json!([]));

    let sticky = browser
        .write(
            Method::POST,
            &format!("{base}/sticky"),
            json!({"channel_id":GENERAL.to_string(),"content":"Pinned note","embed":null,
                   "min_gap_secs":30,"min_messages":5,"enabled":true}),
        )
        .await;
    assert_eq!(sticky.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", sticky.text());
    assert_eq!(fields_of(&sticky), ["channel_id"]);

    h.state
        .app
        .db
        .upsert_sticky(&pinchora_db::UtilityStickyRow {
            channel_id: GENERAL as i64,
            guild_id: guild as i64,
            content: "Pinned note".into(),
            embed: None,
            last_message_id: None,
            min_gap_secs: 30,
            min_messages: 5,
            message_count: 0,
            enabled: true,
            lease_until: None,
            pause_reason: None,
            updated_by: None,
        })
        .await
        .unwrap();
    let removed_sticky = browser
        .write(Method::DELETE, &format!("{base}/sticky/{GENERAL}"), json!({}))
        .await;
    assert_eq!(removed_sticky.status, StatusCode::OK, "{}", removed_sticky.text());
    let audit = h.state.app.db.audit_page(guild, None, 20).await.unwrap();
    for action in [
        "utility.schedule.add",
        "utility.schedule.edit",
        "utility.schedule.toggle",
        "utility.schedule.remove",
        "utility.sticky.remove",
    ] {
        assert!(audit.iter().any(|r| r.action == action), "missing audit: {action}");
    }
}

#[tokio::test]
async fn utility_writes_require_csrf_managed_guild_and_record_audit() {
    let Some(h) = Harness::new("utility_guards_and_audit").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/utility");
    let token = browser.csrf.take();
    let denied = browser
        .write(Method::PUT, &format!("{base}/timezone"), json!({"utc_offset":"+03:00"}))
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    assert_eq!(denied.code(), "csrf");
    browser.csrf = token;
    let denied = browser
        .write(
            Method::PUT,
            &format!("/api/guilds/{}/utility/timezone", guild + 1),
            json!({"utc_offset":"+03:00"}),
        )
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    assert_eq!(denied.code(), "not_manager");
    let saved = browser
        .write(Method::PUT, &format!("{base}/timezone"), json!({"utc_offset":"+03:00"}))
        .await;
    assert_eq!(saved.status, StatusCode::OK);
    let rows = h.state.app.db.audit_page(guild, None, 20).await.unwrap();
    assert!(rows.iter().any(|r| r.action == "utility.timezone"));
    let invalid = browser
        .write(Method::PUT, &format!("{base}/timezone"), json!({"utc_offset":"+💥0"}))
        .await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
}
