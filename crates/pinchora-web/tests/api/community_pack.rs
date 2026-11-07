//! Managed-guild, CSRF and audit boundaries for the four Community sections.
use super::*;

#[tokio::test]
async fn community_pack_writes_are_scoped_and_audited() {
    let Some(h) = Harness::new("community_pack_writes_are_scoped_and_audited").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    // The dashboard mock reuses one fixed Discord channel ID across test guilds.
    sqlx::query("DELETE FROM counting_channels WHERE channel_id=$1")
        .bind(GENERAL as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    let base = format!("/api/guilds/{guild}/community-pack");
    let initial = browser.get(&base).await;
    assert_eq!(initial.status, StatusCode::OK, "{}", initial.text());
    assert_eq!(initial.json()["questions"], json!([]));

    let counting = browser
        .write(
            Method::PUT,
            &format!("{base}/counting"),
            json!({
                "channel_id":GENERAL.to_string(),"reset_on_fail":true,"allow_math":true,"fail_role_id":null
            }),
        )
        .await;
    assert_eq!(counting.status, StatusCode::OK, "{}", counting.text());
    let qotd=browser.write(Method::PUT,&format!("{base}/qotd"),json!({
        "channel_id":GENERAL.to_string(),"ping_role_id":null,"hour":12,"timezone":"Europe/Istanbul","thread":true
    })).await;
    assert_eq!(qotd.status, StatusCode::OK, "{}", qotd.text());
    let form = browser
        .write(
            Method::POST,
            &format!("{base}/forms"),
            json!({
                "name":"Helper","questions":["Why join?"],"submit_channel_id":GENERAL.to_string(),
                "review_roles":[],"accept_role_id":null,"dm_result":true,"open":true
            }),
        )
        .await;
    assert_eq!(form.status, StatusCode::OK, "{}", form.text());
    let mail = browser
        .write(
            Method::PUT,
            &format!("{base}/modmail"),
            json!({
                "enabled":false,"forum_channel_id":null,"staff_roles":[],"greeting":"",
                "close_after_hours":72,"anonymous_staff":false,"log_channel_id":null
            }),
        )
        .await;
    assert_eq!(mail.status, StatusCode::OK, "{}", mail.text());
    let current = browser.get(&base).await.json();
    assert_eq!(current["questions"].as_array().unwrap().len(), 300);
    assert_eq!(current["counting"]["channel_id"], GENERAL.to_string());
    assert_eq!(current["forms"][0]["name"], "Helper");
    let actions: Vec<_> = h
        .state
        .app
        .db
        .audit_page(guild, None, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.action)
        .collect();
    for action in [
        "community.counting.settings",
        "community.qotd.settings",
        "community.forms.save",
        "community.modmail.settings",
    ] {
        assert!(actions.iter().any(|item| item == action), "{action}: {actions:?}");
    }
    assert!(owner > 0);
    sqlx::query("DELETE FROM counting_channels WHERE guild_id=$1")
        .bind(guild as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn community_pack_refuses_missing_csrf_and_bad_channel() {
    let Some(h) = Harness::new("community_pack_refuses_missing_csrf_and_bad_channel").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/community-pack/counting");
    let token = browser.csrf.take();
    let denied = browser
        .write(
            Method::PUT,
            &base,
            json!({"channel_id":GENERAL.to_string(),
        "reset_on_fail":true,"allow_math":false,"fail_role_id":null}),
        )
        .await;
    assert_eq!(denied.code(), "csrf");
    browser.csrf = token;
    let bad = browser
        .write(
            Method::PUT,
            &base,
            json!({"channel_id":"9999999",
        "reset_on_fail":true,"allow_math":false,"fail_role_id":null}),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
}
