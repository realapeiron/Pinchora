use super::*;

/// Discord's update response keeps the message id; the shared mock already handles creation.
pub(super) fn updated_message(channel: &str, message: &str) -> (StatusCode, String) {
    (
        StatusCode::OK,
        message_json(channel.parse().unwrap(), message.parse().unwrap()).to_string(),
    )
}

#[tokio::test]
async fn access_changes_require_manager_csrf_and_are_atomically_audited_in_both_languages() {
    let Some(h) = Harness::new("w7_access").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/access");
    let body = json!({"items":[{"role_id":FAN.to_string(),"areas":["roles","audit"]}]});
    assert_eq!(h.browser().get(&path).await.status, StatusCode::UNAUTHORIZED);
    let csrf = browser.csrf.take();
    assert_eq!(
        browser.write(Method::PUT, &path, body.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    browser.csrf = csrf;
    assert!(h.state.app.db.dashboard_managers(guild).await.unwrap().is_empty());
    for lang in [Lang::En, Lang::Tr] {
        h.state.app.set_user_lang(owner, Some(lang)).await.unwrap();
        for invalid in [
            json!({"items":[{"role_id":guild.to_string(),"areas":["roles"]}]}),
            json!({"items":[{"role_id":BOOSTER.to_string(),"areas":["roles"]}]}),
            json!({"items":[{"role_id":FAN.to_string(),"areas":["access"]}]}),
            json!({"items":[{"role_id":FAN.to_string(),"areas":[]}]}),
            json!({"items":[{"role_id":FAN.to_string(),"areas":["roles"]},{"role_id":FAN.to_string(),"areas":["audit"]}]}),
        ] {
            let refused = browser.write(Method::PUT, &path, invalid).await;
            assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{}", refused.text());
            assert_eq!(
                refused.json()["error"]["message"],
                pinchora_core::t!(lang, "server_tools.access-invalid")
            );
        }
    }
    h.state.app.set_user_lang(owner, None).await.unwrap();
    h.state.app.set_guild_lang(guild, Some(Lang::Tr)).await.unwrap();
    let invalid = browser
        .write(
            Method::PUT,
            &path,
            json!({"items":[{"role_id":FAN.to_string(),"areas":[]}]}),
        )
        .await;
    assert_eq!(
        invalid.json()["error"]["message"],
        pinchora_core::t!(Lang::Tr, "server_tools.access-invalid")
    );
    let saved = browser.write(Method::PUT, &path, body.clone()).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(saved.json()["items"][0]["role_id"], FAN.to_string());
    assert_eq!(browser.write(Method::PUT, &path, body).await.status, StatusCode::OK);
    let log = h.state.app.db.audit_page(guild, None, 100).await.unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].actor_id, owner as i64);
    assert_eq!(log[0].action, "access.replace");
    assert_eq!(
        serde_json::from_str::<Value>(log[0].before.as_ref().unwrap()).unwrap(),
        json!({"roles":[]})
    );
    assert_eq!(
        browser.write(Method::PUT, &path, json!({"items":[]})).await.status,
        StatusCode::OK
    );
    assert_eq!(h.state.app.db.audit_page(guild, None, 100).await.unwrap().len(), 2);
}

#[tokio::test]
async fn delegated_roles_are_live_scoped_discoverable_and_never_grant_discord_powers() {
    let Some(h) = Harness::new("w7_delegate").await else {
        return;
    };
    let (guild, _, mut owner) = h.managed_server().await;
    let user = unique();
    h.mock.member_roles.lock().unwrap().insert(user, vec![FAN, REGULAR]);
    let path = format!("/api/guilds/{guild}");
    assert_eq!(owner.write(Method::PUT, &format!("{path}/access"), json!({"items":[{"role_id":FAN.to_string(),"areas":["roles"]},{"role_id":HELPER.to_string(),"areas":["audit"]}]})).await.status, StatusCode::OK);
    let mut member = h.browser();
    member
        .login(&h, user, vec![guild_entry(guild, "Delegated Reef", false, 0)])
        .await;
    assert_eq!(
        member.get("/api/me/guilds").await.json()["items"][0]["id"],
        guild.to_string()
    );
    let overview = member.get(&path).await;
    assert_eq!(overview.status, StatusCode::OK, "{}", overview.text());
    assert!(overview.json()["settings"].is_null());
    assert_eq!(overview.json()["audit"], json!([]));
    assert_eq!(overview.json()["access"], json!({"full":false,"areas":["roles"]}));
    for endpoint in ["roles", "discord"] {
        assert_eq!(member.get(&format!("{path}/{endpoint}")).await.status, StatusCode::OK);
    }
    for endpoint in [
        "access",
        "backup",
        "command-control",
        "audit",
        "audit-tools",
        "audit-tools/export",
        "modules",
        "tickets",
        "community-v2",
        "stats",
    ] {
        assert_eq!(
            member.get(&format!("{path}/{endpoint}")).await.status,
            StatusCode::FORBIDDEN,
            "{endpoint}"
        );
    }
    assert_eq!(
        member
            .write(
                Method::PATCH,
                &format!("{path}/settings"),
                json!({"leveling_enabled":false})
            )
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        member
            .write(Method::PUT, &format!("{path}/access"), json!({"items":[]}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    // Dangerous roles stay forbidden even when the area itself was delegated.
    let unsafe_panel =
        json!({"title":"Unsafe", "style":"buttons", "roles":[{"role_id":HELPER.to_string(),"label":"Power"}]});
    assert_eq!(
        member
            .write(Method::POST, &format!("{path}/roles/panels"), unsafe_panel)
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let safe_panel =
        json!({"title":"Web delegate", "style":"buttons", "roles":[{"role_id":FAN.to_string(),"label":"Fan"}]});
    let panel = member
        .write(Method::POST, &format!("{path}/roles/panels"), safe_panel)
        .await;
    assert_eq!(panel.status, StatusCode::OK, "{}", panel.text());
    let id = panel.json()["panels"]["items"][0]["id"].as_str().unwrap().to_owned();
    let posted = member
        .write(
            Method::POST,
            &format!("{path}/roles/panels/{id}/post"),
            json!({"channel_id":GENERAL.to_string()}),
        )
        .await;
    assert_eq!(posted.status, StatusCode::OK, "{}", posted.text());
    assert_eq!(posted.json()["posted"]["updated"], false);
    let updated = member
        .write(
            Method::POST,
            &format!("{path}/roles/panels/{id}/post"),
            json!({"channel_id":GENERAL.to_string()}),
        )
        .await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.text());
    assert_eq!(updated.json()["posted"]["updated"], true);
    assert_eq!(
        updated.json()["posted"]["message_id"],
        posted.json()["posted"]["message_id"]
    );
    let privileged = owner.write(Method::POST, &format!("{path}/roles/panels"), json!({"title":"Owner powers", "style":"buttons", "roles":[{"role_id":HELPER.to_string(),"label":"Helper"}]})).await;
    assert_eq!(privileged.status, StatusCode::OK, "{}", privileged.text());
    let protected = h.state.app.db.find_panel(guild, "owner powers").await.unwrap().unwrap();
    assert_eq!(
        member
            .write(
                Method::POST,
                &format!("{path}/roles/panels/{}/post", protected.id),
                json!({"channel_id":GENERAL.to_string()})
            )
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    h.mock
        .member_roles
        .lock()
        .unwrap()
        .insert(user, vec![FAN, REGULAR, HELPER]);
    assert_eq!(member.get(&format!("{path}/audit-tools")).await.status, StatusCode::OK);
    h.mock.member_roles.lock().unwrap().insert(user, vec![]);
    assert_eq!(member.get(&format!("{path}/roles")).await.status, StatusCode::FORBIDDEN);
    h.mock.member_roles.lock().unwrap().insert(user, vec![FAN, REGULAR]);
    assert_eq!(member.get(&format!("{path}/roles")).await.status, StatusCode::OK);
    owner
        .write(Method::PUT, &format!("{path}/access"), json!({"items":[]}))
        .await;
    assert_eq!(member.get(&format!("{path}/roles")).await.status, StatusCode::FORBIDDEN);
    assert_eq!(member.get("/api/me/guilds").await.json()["items"], json!([]));
    // Full Discord managers remain allowed without grants.
    for (is_owner, permission) in [(true, 0), (false, MANAGE_GUILD), (false, ADMINISTRATOR)] {
        let mut full = h.browser();
        full.login(&h, unique(), vec![guild_entry(guild, "Reef", is_owner, permission)])
            .await;
        assert_eq!(full.get(&format!("{path}/access")).await.status, StatusCode::OK);
    }
}

#[tokio::test]
async fn audit_filters_paginate_export_escape_and_isolate_guilds() {
    let Some(h) = Harness::new("w7_audit").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    for index in 0..53 {
        h.state
            .app
            .db
            .audit(&pinchora_db::NewAudit {
                guild,
                actor: owner,
                actor_name: "=HYPERLINK(\"bad\")",
                source: pinchora_db::AuditSource::Web,
                action: "roles.panel.save",
                target: Some(&index.to_string()),
                before: Some("{\"enabled\":false}"),
                after: Some("{\"enabled\":true,\"note\":\"100%_literal\"}"),
            })
            .await
            .unwrap();
    }
    h.state
        .app
        .db
        .audit(&pinchora_db::NewAudit {
            guild: unique(),
            actor: owner,
            actor_name: "Elsewhere",
            source: pinchora_db::AuditSource::Web,
            action: "roles.panel.save",
            target: None,
            before: None,
            after: None,
        })
        .await
        .unwrap();
    let path = format!("/api/guilds/{guild}/audit-tools");
    let query =
        format!("actor={owner}&area=roles&text=100%25_literal&from=2020-01-01T00:00:00Z&until=2099-01-01T00:00:00Z");
    let page = browser.get(&format!("{path}?{query}")).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.text());
    assert_eq!(page.json()["items"].as_array().unwrap().len(), 50);
    let next = page.json()["next"].as_str().unwrap().to_owned();
    assert_eq!(
        browser.get(&format!("{path}?{query}&before={next}")).await.json()["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    for query in [
        format!("actor={}", unique()),
        "area=access".into(),
        "text=nonexistent".into(),
        "until=2020-01-01T00:00:00Z".into(),
    ] {
        assert_eq!(browser.get(&format!("{path}?{query}")).await.json()["items"], json!([]));
    }
    let csv = browser.get(&format!("{path}/export?{query}")).await;
    assert_eq!(csv.status, StatusCode::OK);
    assert!(
        csv.headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/csv")
    );
    assert!(csv.text().contains("\"'=HYPERLINK(\"\"bad\"\")\""));
    assert_eq!(csv.text().lines().count(), 54);
    assert!(!csv.text().contains("Elsewhere"));
    for lang in [Lang::En, Lang::Tr] {
        h.state.app.set_user_lang(owner, Some(lang)).await.unwrap();
        let bad = browser.get(&format!("{path}?actor=abc")).await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
        assert_eq!(
            bad.json()["error"]["message"],
            pinchora_core::t!(lang, "server_tools.audit-invalid")
        );
    }
}

#[tokio::test]
async fn csv_stream_stops_at_row_and_byte_budgets() {
    let Some(h) = Harness::new("w7_export_limits").await else {
        return;
    };
    let (guild, actor, mut browser) = h.managed_server().await;
    sqlx::query("INSERT INTO dashboard_audit(guild_id,actor_id,actor_name,source,action) SELECT $1,$2,'Actor','web','roles.test' FROM generate_series(1,10001)")
        .bind(guild as i64).bind(actor as i64).execute(h.state.app.db.pool()).await.unwrap();
    let path = format!("/api/guilds/{guild}/audit-tools/export");
    let csv = browser.get(&path).await;
    assert_eq!(csv.status, StatusCode::OK);
    assert_eq!(csv.text().lines().count(), 10002);
    assert!(csv.text().lines().last().unwrap().starts_with("\"EXPORT_LIMIT\""));
    sqlx::query("INSERT INTO dashboard_audit(guild_id,actor_id,actor_name,source,action,after) VALUES($1,$2,'Actor','web','access.large',jsonb_build_object('large',repeat('x',8388608)))")
        .bind(guild as i64).bind(actor as i64).execute(h.state.app.db.pool()).await.unwrap();
    let csv = browser.get(&format!("{path}?area=access")).await;
    assert_eq!(csv.status, StatusCode::OK);
    assert!(csv.text().len() < 8 * 1024 * 1024);
    assert_eq!(csv.text().lines().count(), 2);
    assert!(csv.text().contains("EXPORT_LIMIT"));
}
