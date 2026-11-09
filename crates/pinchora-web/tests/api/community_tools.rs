use crate::*;

#[tokio::test]
async fn community_tools_settings_personal_data_and_csrf() {
    let Some(h) = Harness::new("community_v2_settings_personal_data_and_csrf").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/community-v2");
    let empty = browser.get(&base).await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.text());
    assert_eq!(empty.json()["giveaways"], json!([]));

    let saved = browser
        .write(
            Method::PUT,
            &format!("{base}/autopost"),
            json!({
                "channel_id":GENERAL.to_string(),"thread":true,"crosspost":false
            }),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let bad = browser
        .write(
            Method::PUT,
            &format!("{base}/autopost"),
            json!({
                "channel_id":GENERAL.to_string(),"thread":false,"crosspost":true
            }),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    let current = browser.get(&base).await.json();
    assert_eq!(current["autopost"][0]["channel_id"], GENERAL.to_string());

    let personal = format!("{base}/personal");
    let checklist = browser
        .write(
            Method::PUT,
            &format!("{personal}/checklist"),
            json!({"text":"Prepare event"}),
        )
        .await;
    assert_eq!(checklist.status, StatusCode::OK, "{}", checklist.text());
    let reminder = browser
        .write(
            Method::PUT,
            &format!("{personal}/reminders"),
            json!({"text":"Check queue","interval_minutes":1440}),
        )
        .await;
    assert_eq!(reminder.status, StatusCode::OK, "{}", reminder.text());
    let own = browser.get(&personal).await.json();
    assert_eq!(own["checklist"][0]["body"], "Prepare event");
    assert_eq!(own["reminders"][0]["body"], "Check queue");
    assert!(own.get("author_id").is_none());

    let token = browser.csrf.take();
    let denied = browser
        .write(
            Method::PUT,
            &format!("{base}/anniversary"),
            json!({"channel_id":GENERAL.to_string(),"enabled":true}),
        )
        .await;
    assert_eq!(denied.code(), "csrf");
    browser.csrf = token;
    let anniversary = browser
        .write(
            Method::PUT,
            &format!("{base}/anniversary"),
            json!({"channel_id":GENERAL.to_string(),"enabled":true}),
        )
        .await;
    assert_eq!(anniversary.status, StatusCode::OK, "{}", anniversary.text());
    let actions: Vec<_> = h
        .state
        .app
        .db
        .audit_page(guild, None, 20)
        .await
        .unwrap()
        .into_iter()
        .map(|a| a.action)
        .collect();
    assert!(actions.iter().any(|a| a == "community.autopost.save"));
    assert!(actions.iter().any(|a| a == "community.anniversary.save"));
}

#[tokio::test]
async fn community_tools_hides_confession_authors_and_refuses_non_managers() {
    let Some(h) = Harness::new("community_v2_hides_confession_authors_and_refuses_non_managers").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/community-v2");
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO confessions(guild_id,author_id,body) VALUES($1,$2,'A private thought') RETURNING id",
    )
    .bind(guild as i64)
    .bind(unique() as i64)
    .fetch_one(h.state.app.db.pool())
    .await
    .unwrap();
    let list = browser.get(&base).await.json();
    assert_eq!(list["confessions"][0]["id"], row.0.to_string());
    assert!(list["confessions"][0].get("author_id").is_none());
    let invalid = browser
        .write(
            Method::PUT,
            &format!("{base}/suggestions/1"),
            json!({"status":"bad","staff_note":null}),
        )
        .await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", invalid.text());
    let mut stranger = h.browser();
    stranger
        .login(
            &h,
            unique(),
            vec![guild_entry(guild, "Member only", false, SEND_MESSAGES)],
        )
        .await;
    assert_eq!(stranger.get(&base).await.status, StatusCode::FORBIDDEN);
    assert_eq!(
        stranger
            .write(Method::PUT, &format!("{base}/confessions/{}/reveal", row.0), json!({}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert!(owner > 0);
}

#[tokio::test]
async fn confession_reveal_requires_csrf_owner_and_records_audit_without_author() {
    let Some(h) = Harness::new("confession_reveal_requires_csrf_owner_and_records_audit_without_author").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    h.mock.guild_owners.lock().unwrap().insert(guild, owner);
    let author = unique();
    let id = h
        .state
        .app
        .db
        .submit_confession(guild, author, "anonymous thought")
        .await
        .unwrap()
        .unwrap();
    let base = format!("/api/guilds/{guild}/community-v2");
    let reveal = format!("{base}/confessions/{id}/reveal");
    assert_eq!(browser.get(&reveal).await.status, StatusCode::METHOD_NOT_ALLOWED);
    let token = browser.csrf.take();
    assert_eq!(browser.write(Method::PUT, &reveal, json!({})).await.code(), "csrf");
    browser.csrf = token;
    let revealed = browser.write(Method::PUT, &reveal, json!({})).await;
    assert_eq!(revealed.status, StatusCode::OK, "{}", revealed.text());
    assert_eq!(revealed.json()["author_id"], author.to_string());
    let audit = h.state.app.db.audit_page(guild, None, 20).await.unwrap();
    assert_eq!(
        audit
            .iter()
            .filter(|a| a.action == "community.confessions.reveal")
            .count(),
        1
    );
    let mut manager = h.browser();
    manager
        .login(&h, unique(), vec![guild_entry(guild, "Manager", false, MANAGE_GUILD)])
        .await;
    assert_eq!(
        manager.write(Method::PUT, &reveal, json!({})).await.status,
        StatusCode::NOT_FOUND
    );
    let banned = manager
        .write(
            Method::PUT,
            &format!("{base}/confessions/{id}/ban"),
            json!({"banned":true}),
        )
        .await;
    assert_eq!(banned.status, StatusCode::OK, "{}", banned.text());
    assert!(!banned.text().contains(&author.to_string()));
    assert!(
        h.state
            .app
            .db
            .submit_confession(guild, author, "blocked thought")
            .await
            .unwrap()
            .is_none()
    );
    let readonly = browser
        .write(
            Method::PUT,
            &format!("{base}/anniversary"),
            json!({"enabled":false,"channel_id":null,"guild_id":guild.to_string()}),
        )
        .await;
    assert_eq!(readonly.status, StatusCode::BAD_REQUEST);
    assert!(readonly.text().contains("unknown field"), "{}", readonly.text());
    let mut member = h.browser();
    member
        .login(&h, unique(), vec![guild_entry(guild, "Member", false, SEND_MESSAGES)])
        .await;
    for (path, body) in [
        (
            "autopost".to_string(),
            json!({"channel_id":GENERAL.to_string(),"thread":true,"crosspost":false}),
        ),
        ("anniversary".into(), json!({"channel_id":null,"enabled":false})),
        (
            "confessions/settings".into(),
            json!({"review_channel_id":GENERAL.to_string(),"publish_channel_id":"2","enabled":true}),
        ),
        (format!("confessions/{id}/review"), json!({"approve":false})),
        (format!("confessions/{id}/ban"), json!({"banned":true})),
        ("suggestions/1".into(), json!({"status":"denied","staff_note":null})),
    ] {
        let response = member.write(Method::PUT, &format!("{base}/{path}"), body).await;
        assert_eq!(response.status, StatusCode::FORBIDDEN, "{path}: {}", response.text());
    }
}
