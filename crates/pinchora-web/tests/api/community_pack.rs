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

#[tokio::test]
async fn community_all_routes_reject_non_managers_and_foreign_guilds() {
    let Some(h) = Harness::new("community_all_routes_reject_non_managers_and_foreign_guilds").await else {
        return;
    };
    let (guild, _, _) = h.managed_server().await;
    let mut stranger = h.browser();
    stranger
        .login(
            &h,
            unique(),
            vec![guild_entry(guild, "Member only", false, SEND_MESSAGES)],
        )
        .await;
    let base = format!("/api/guilds/{guild}/community-pack");
    for (method, suffix) in [
        (Method::GET, ""),
        (Method::PUT, "/qotd"),
        (Method::PUT, "/qotd/1/decision"),
        (Method::PUT, "/counting"),
        (Method::POST, "/counting/reset"),
        (Method::POST, "/forms"),
        (Method::PUT, "/forms/1"),
        (Method::POST, "/forms/1/post"),
        (Method::GET, "/forms/1/submissions/1"),
        (Method::PUT, "/forms/1/submissions/1"),
        (Method::PUT, "/modmail"),
        (Method::POST, "/modmail/1/close"),
    ] {
        let response = if method == Method::GET {
            stranger.get(&format!("{base}{suffix}")).await
        } else {
            stranger
                .write(method.clone(), &format!("{base}{suffix}"), json!({}))
                .await
        };
        assert_eq!(
            response.status,
            StatusCode::FORBIDDEN,
            "{method} {suffix}: {}",
            response.text()
        );
    }
}

#[tokio::test]
async fn community_form_modal_title_limit_and_private_audit() {
    let Some(h) = Harness::new("community_form_modal_title_limit_and_private_audit").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/community-pack");
    let mut body = json!({"name":"x".repeat(46),"questions":["Why?"],"submit_channel_id":GENERAL.to_string(),
        "review_roles":[],"accept_role_id":null,"dm_result":false,"open":true});
    assert_eq!(
        browser
            .write(Method::POST, &format!("{base}/forms"), body.clone())
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    body["name"] = json!("x".repeat(45));
    let saved = browser.write(Method::POST, &format!("{base}/forms"), body).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let form = saved.json()["id"].as_str().unwrap().parse::<i64>().unwrap();
    let submission = h
        .state
        .app
        .db
        .submit_form(guild, form, unique(), &json!(["PRIVATE_ANSWER_SENTINEL"]))
        .await
        .unwrap()
        .unwrap();
    let review = browser
        .write(
            Method::PUT,
            &format!("{base}/forms/{form}/submissions/{submission}"),
            json!({"accept":true,"reason":"PRIVATE_REASON_SENTINEL"}),
        )
        .await;
    assert_eq!(review.status, StatusCode::OK, "{}", review.text());
    let audit: String = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(to_jsonb(a))::text,'[]') FROM dashboard_audit a WHERE guild_id=$1",
    )
    .bind(guild as i64)
    .fetch_one(h.state.app.db.pool())
    .await
    .unwrap();
    assert!(!audit.contains("PRIVATE_ANSWER_SENTINEL") && !audit.contains("PRIVATE_REASON_SENTINEL"));
}

#[tokio::test]
async fn community_backup_rechecks_role_safety_and_channel_types() {
    let Some(h) = Harness::new("community_backup_rechecks_role_safety_and_channel_types").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    h.mock.guild_owners.lock().unwrap().insert(guild, owner);
    let path = format!("/api/guilds/{guild}/backup");
    let backup = browser.get(&format!("{path}/export")).await.json();
    for (table, field, row) in [
        (
            "forms",
            "accept_role_id",
            json!({"id":"1","name":"Form","questions":["Why?"],"submit_channel_id":GENERAL.to_string(),"review_roles":[],"accept_role_id":FAN.to_string(),"dm_result":false,"open":true}),
        ),
        (
            "counting_channels",
            "fail_role_id",
            json!({"channel_id":GENERAL.to_string(),"reset_on_fail":true,"allow_math":false,"fail_role_id":FAN.to_string()}),
        ),
    ] {
        let mut candidate = backup.clone();
        candidate["sections"][table] = json!([row]);
        let good = browser
            .write(
                Method::POST,
                &format!("{path}/preview"),
                json!({"backup":candidate,"policy":"merge"}),
            )
            .await;
        assert_eq!(good.json()["can_import"], true, "{}", good.text());
        candidate["sections"][table][0][field] = json!(ADMIN.to_string());
        let bad = browser
            .write(
                Method::POST,
                &format!("{path}/preview"),
                json!({"backup":candidate,"policy":"merge"}),
            )
            .await;
        assert_eq!(bad.json()["can_import"], false, "{}", bad.text());
    }
    let mut candidate = backup.clone();
    candidate["sections"]["modmail_settings"] = json!([{"enabled":true,"forum_channel_id":GENERAL.to_string(),"staff_roles":[],"greeting":"","close_after_hours":72,"anonymous_staff":true,"log_channel_id":null}]);
    let bad = browser
        .write(
            Method::POST,
            &format!("{path}/preview"),
            json!({"backup":candidate,"policy":"merge"}),
        )
        .await;
    assert_eq!(
        bad.json()["can_import"],
        false,
        "a text channel is not a forum: {}",
        bad.text()
    );
}
