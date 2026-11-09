use crate::*;

fn inventory() -> Vec<(&'static str, &'static str, &'static str)> {
    include_str!("../../../../docs/w7-guild-route-matrix.tsv")
        .lines()
        .skip(1)
        .map(|line| {
            let mut fields = line.split('\t');
            (fields.next().unwrap(), fields.next().unwrap(), fields.next().unwrap())
        })
        .collect()
}

fn concrete(path: &str) -> String {
    let mut out = String::new();
    let mut parameter = false;
    for c in path.chars() {
        if c == '{' {
            parameter = true;
            out.push_str("123");
        } else if c == '}' {
            parameter = false;
        } else if !parameter {
            out.push(c);
        }
    }
    out
}
fn api_path(guild: u64, path: &str) -> String {
    if path.starts_with("/reef/") {
        format!("/api{}", concrete(&path.replace("{guild}", &guild.to_string())))
    } else {
        format!("/api/guilds/{guild}{}", concrete(path).trim_end_matches('/'))
    }
}
async fn clear_limit(h: &Harness, user: u64) {
    let window = chrono::Utc::now().timestamp() / 60;
    for scope in ["read", "write"] {
        let key = format!("web:rl:{scope}:{user}:{window}");
        let hits = h.state.app.cache.counter(&key).await.unwrap();
        h.state
            .app
            .cache
            .incr_by(&key, -hits, Duration::from_secs(60))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn all_guild_routes_require_sessions_csrf_and_their_management_areas() {
    let Some(h) = Harness::new("w7_matrix").await else {
        return;
    };
    let (guild, owner_id, mut owner) = h.managed_server().await;
    let routes = inventory();
    assert_eq!(routes.len(), 257);
    for (method, path, _) in &routes {
        let path = api_path(guild, path);
        let result = h.browser().write(method.parse().unwrap(), &path, json!({})).await;
        assert_eq!(
            result.status,
            StatusCode::UNAUTHORIZED,
            "{method} {path}: {}",
            result.text()
        );
        if *method != "GET" {
            clear_limit(&h, owner_id).await;
            let csrf = owner.csrf.take();
            let result = owner.write(method.parse().unwrap(), &path, json!({})).await;
            owner.csrf = csrf;
            assert_eq!(
                result.status,
                StatusCode::FORBIDDEN,
                "{method} {path}: {}",
                result.text()
            );
            assert_eq!(result.code(), "csrf");
        }
    }
    for area in pinchora_db::DASHBOARD_AREAS {
        let user = unique();
        let mut browser = h.browser();
        h.mock.member_roles.lock().unwrap().insert(user, vec![FAN, REGULAR]);
        browser
            .login(&h, user, vec![guild_entry(guild, "Matrix", false, 0)])
            .await;
        h.state
            .app
            .db
            .replace_dashboard_managers(
                guild,
                owner_id,
                "Owner",
                &[pinchora_db::DashboardManager {
                    role_id: FAN as i64,
                    areas: vec![(*area).into()],
                }],
            )
            .await
            .unwrap();
        for (method, path, required) in &routes {
            if required == area || (*required == "shell" && *method == "GET") {
                continue;
            }
            let reef = path.starts_with("/reef/");
            // Reading plazas/gatherings and RSVP are member actions. Scheduling uses the
            // community management grant; reef_spaces also exercises membership and revocation.
            if reef && !(*method == "POST" && *path == "/reef/guilds/{guild}/gatherings") {
                continue;
            }
            clear_limit(&h, user).await;
            let path = api_path(guild, path);
            let body = if reef {
                json!({"title":"Matrix gathering", "starts_at":chrono::Utc::now()+chrono::Duration::hours(1),
                    "ends_at":chrono::Utc::now()+chrono::Duration::hours(2)})
            } else {
                json!({})
            };
            let result = browser.write(method.parse().unwrap(), &path, body).await;
            assert_eq!(
                result.status,
                StatusCode::FORBIDDEN,
                "{area}: {method} {path}: {}",
                result.text()
            );
            assert_eq!(
                result.code(),
                if reef { "access_denied" } else { "not_manager" },
                "{area}: {method} {path}"
            );
        }
    }
}

#[tokio::test]
async fn stale_full_permissions_and_departed_members_lose_access_immediately() {
    let Some(h) = Harness::new("w7_revoke").await else {
        return;
    };
    let (guild, owner_id, mut owner) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/access");
    let user = unique();
    let mut manager = h.browser();
    manager
        .login(&h, user, vec![guild_entry(guild, "Reef", false, MANAGE_GUILD)])
        .await;
    assert_eq!(manager.get(&path).await.status, StatusCode::OK);
    h.add_user(user, vec![guild_entry(guild, "Reef", false, 0)]);
    assert_eq!(manager.get(&path).await.status, StatusCode::FORBIDDEN);
    assert_eq!(
        manager.write(Method::PUT, &path, json!({"items":[]})).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(manager.get("/api/me/guilds").await.json()["items"], json!([]));
    h.mock.absent_leveling_members.lock().unwrap().insert((guild, owner_id));
    assert_eq!(owner.get(&path).await.status, StatusCode::FORBIDDEN);
    h.mock
        .absent_leveling_members
        .lock()
        .unwrap()
        .remove(&(guild, owner_id));
    assert_eq!(owner.get(&path).await.status, StatusCode::OK);
    owner
        .write(
            Method::PUT,
            &path,
            json!({"items":[{"role_id":FAN.to_string(),"areas":["roles"]}]}),
        )
        .await;
    h.mock.member_roles.lock().unwrap().insert(user, vec![FAN]);
    let roles = format!("/api/guilds/{guild}/roles");
    assert_eq!(manager.get(&roles).await.status, StatusCode::OK);
    h.mock.absent_leveling_members.lock().unwrap().insert((guild, user));
    assert_eq!(manager.get(&roles).await.status, StatusCode::FORBIDDEN);
    assert_eq!(
        manager
            .write(Method::PUT, &format!("{roles}/sticky"), json!({"enabled":true}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn panels_cannot_escalate_dashboard_access_or_retain_unsafe_roles_or_publish_above_bot() {
    let Some(h) = Harness::new("w7_panel_safety").await else {
        return;
    };
    let (guild, _, mut owner) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}");
    // The low FAN role has no dangerous Discord bits, but grants access to another dashboard area.
    owner
        .write(
            Method::PUT,
            &format!("{path}/access"),
            json!({"items":[
                {"role_id":REGULAR.to_string(),"areas":["roles"]},
                {"role_id":FAN.to_string(),"areas":["audit"]}
            ]}),
        )
        .await;
    let user = unique();
    h.mock.member_roles.lock().unwrap().insert(user, vec![REGULAR]);
    let mut delegate = h.browser();
    delegate
        .login(&h, user, vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    let body = json!({"title":"Privileged panel","roles":[{"role_id":FAN.to_string(),"label":"Fan"}]});
    assert_eq!(
        delegate
            .write(Method::POST, &format!("{path}/roles/panels"), body.clone())
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    for content_type in [
        "application/vnd.api+json",
        "application/json ; charset=utf-8",
        "Application/JSON",
    ] {
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("{path}/roles/panels"))
            .header(header::CONTENT_TYPE, content_type)
            .header(header::ORIGIN, PUBLIC_URL)
            .header("x-csrf-token", delegate.csrf.clone().unwrap())
            .body(Body::from(body.to_string()))
            .unwrap();
        assert_eq!(
            delegate.send(request).await.status,
            StatusCode::FORBIDDEN,
            "{content_type}"
        );
    }
    assert_eq!(
        owner
            .write(Method::POST, &format!("{path}/roles/panels"), body.clone())
            .await
            .status,
        StatusCode::OK
    );
    let panel = h
        .state
        .app
        .db
        .find_panel(guild, "privileged panel")
        .await
        .unwrap()
        .unwrap();
    let panel_path = format!("{path}/roles/panels/{}", panel.id);
    assert_eq!(
        delegate
            .write(
                Method::POST,
                &format!("{panel_path}/post"),
                json!({"channel_id":GENERAL.to_string()})
            )
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    owner
        .write(Method::PUT, &format!("{path}/access"), json!({"items":[]}))
        .await;
    // A normal Manage Server user may neither retain nor publish a role at their own position.
    h.add_user(user, vec![guild_entry(guild, "Reef", false, MANAGE_GUILD)]);
    h.mock.member_roles.lock().unwrap().insert(user, vec![FAN]);
    assert_eq!(
        delegate.write(Method::PUT, &panel_path, body).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        delegate
            .write(
                Method::POST,
                &format!("{panel_path}/post"),
                json!({"channel_id":GENERAL.to_string()})
            )
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    h.mock.bot_position.store(2, Ordering::SeqCst);
    assert_eq!(
        owner
            .write(
                Method::POST,
                &format!("{panel_path}/post"),
                json!({"channel_id":GENERAL.to_string()})
            )
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(h.mock.posts.lock().unwrap().is_empty());
}

async fn configuration_delegate(h: &Harness, guild: u64, owner: u64) -> (u64, Browser) {
    h.state
        .app
        .db
        .replace_dashboard_managers(
            guild,
            owner,
            "Owner",
            &[
                pinchora_db::DashboardManager {
                    role_id: REGULAR as i64,
                    areas: vec!["configuration".into(), "feeds".into()],
                },
                pinchora_db::DashboardManager {
                    role_id: FAN as i64,
                    areas: vec!["audit".into()],
                },
            ],
        )
        .await
        .unwrap();
    let user = unique();
    h.mock.member_roles.lock().unwrap().insert(user, vec![REGULAR]);
    let mut browser = h.browser();
    browser.login(h, user, vec![guild_entry(guild, "Reef", false, 0)]).await;
    (user, browser)
}

#[tokio::test]
async fn saved_command_and_responder_activation_cannot_escalate_access() {
    let Some(h) = Harness::new("w7_saved_actions").await else {
        return;
    };
    let (guild, owner_id, mut owner) = h.managed_server().await;
    let (_, mut delegate) = configuration_delegate(&h, guild, owner_id).await;
    for (route, mut body) in [
        ("commands", json!({"name":"privileged","responses":["Hello"]})),
        (
            "responders",
            json!({"name":"privileged","trigger_kind":"member_join","responses":["Hello"],"destination":"channel","channel_id":GENERAL.to_string()}),
        ),
    ] {
        body["enabled"] = json!(false);
        body["cooldown_secs"] = json!(30);
        body["actions"] = json!([{"kind":"add_role","role_id":FAN.to_string()}]);
        let path = format!("/api/guilds/{guild}/{route}");
        let made = owner.write(Method::POST, &path, body).await;
        assert_eq!(made.status, StatusCode::OK, "{}", made.text());
        let id = made.json()["id"].as_str().unwrap().to_owned();
        let enabled = delegate
            .write(Method::PATCH, &format!("{path}/{id}"), json!({"enabled":true}))
            .await;
        assert_eq!(enabled.status, StatusCode::FORBIDDEN, "{route}: {}", enabled.text());
        assert_eq!(owner.get(&format!("{path}/{id}")).await.json()["enabled"], false);
    }
}

#[tokio::test]
async fn xp_edits_imports_and_sync_cannot_grant_saved_privileged_rewards() {
    let Some(h) = Harness::new("w7_saved_rewards").await else {
        return;
    };
    let (guild, owner_id, _) = h.managed_server().await;
    let (user, mut delegate) = configuration_delegate(&h, guild, owner_id).await;
    h.state.app.db.add_level_role(guild, 1, FAN).await.unwrap();
    for (method, route, body) in [
        (
            Method::PUT,
            format!("xp-admin/members/{user}"),
            json!({"operation":"set","amount":1000}),
        ),
        (Method::POST, "xp-admin/import/mee6".into(), json!({"mode":"merge"})),
        (
            Method::POST,
            "leveling-parity/import/apply".into(),
            json!({"preview_id":"0123456789abcdef0123456789abcdef"}),
        ),
        (Method::POST, "level-roles/sync".into(), json!({})),
    ] {
        let result = delegate
            .write(method, &format!("/api/guilds/{guild}/{route}"), body)
            .await;
        assert_eq!(result.status, StatusCode::FORBIDDEN, "{route}: {}", result.text());
        assert_eq!(result.code(), "not_manager");
    }
    assert!(h.state.app.db.member_rank(guild, user).await.unwrap().is_none());
}

#[tokio::test]
async fn module_activation_cannot_bypass_saved_role_checks() {
    let Some(h) = Harness::new("w7_saved_module_roles").await else {
        return;
    };
    let (guild, owner_id, mut owner) = h.managed_server().await;
    let (_, mut delegate) = configuration_delegate(&h, guild, owner_id).await;
    let base = format!("/api/guilds/{guild}");
    let command = owner
        .write(
            Method::POST,
            &format!("{base}/commands"),
            json!({
                "name":"privileged", "responses":["Hello"], "cooldown_secs":30,
                "actions":[{"kind":"add_role","role_id":FAN.to_string()}]
            }),
        )
        .await;
    assert_eq!(command.status, StatusCode::OK, "{}", command.text());
    let panel = owner
        .write(
            Method::POST,
            &format!("{base}/roles/panels"),
            json!({
                "title":"Privileged", "roles":[{"role_id":FAN.to_string(),"label":"Fan"}]
            }),
        )
        .await;
    assert_eq!(panel.status, StatusCode::OK, "{}", panel.text());
    h.state.app.db.add_level_role(guild, 1, FAN).await.unwrap();
    h.state
        .app
        .db
        .add_feed(&pinchora_db::NewFeed {
            guild,
            kind: "twitch",
            source: &format!("review{guild}"),
            title: None,
            channel: GENERAL,
            message: None,
            embed: true,
            ping_role: None,
            live_role: Some(FAN),
            include: &[],
            exclude: &[],
            created_by: owner_id,
            seen: &[],
            interval_secs: 120,
        })
        .await
        .unwrap();
    for module in ["custom_commands", "role_panels", "leveling", "feeds"] {
        let path = format!("{base}/modules/{module}");
        assert_eq!(
            owner.write(Method::PUT, &path, json!({"enabled":false})).await.status,
            StatusCode::OK
        );
        let enabled = delegate.write(Method::PUT, &path, json!({"enabled":true})).await;
        assert_eq!(enabled.status, StatusCode::FORBIDDEN, "{module}: {}", enabled.text());
        assert_eq!(
            owner.write(Method::PUT, &path, json!({"enabled":true})).await.status,
            StatusCode::OK
        );
    }
    let feed = h.state.app.db.feeds_of(guild).await.unwrap().pop().unwrap().feed;
    assert_eq!(delegate.get(&format!("{base}/feeds")).await.status, StatusCode::OK);
    h.state
        .app
        .db
        .edit_feed(
            guild,
            feed.id,
            &pinchora_db::FeedEdit {
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let enabled = delegate
        .write(
            Method::PUT,
            &format!("{base}/feeds/{}", feed.id),
            json!({"enabled":true}),
        )
        .await;
    assert_eq!(enabled.status, StatusCode::FORBIDDEN, "{}", enabled.text());
    assert!(!h.state.app.db.feed(guild, feed.id).await.unwrap().unwrap().enabled);
    let path = format!("{base}/settings");
    assert_eq!(
        owner
            .write(Method::PATCH, &path, json!({"leveling_enabled":false}))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        delegate
            .write(Method::PATCH, &path, json!({"leveling_enabled":true}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn audit_redacts_historic_secrets_in_all_views_and_search_and_bounds_sparse_scans() {
    let Some(h) = Harness::new("w7_audit_review").await else {
        return;
    };
    let (guild, actor, mut browser) = h.managed_server().await;
    sqlx::query("INSERT INTO dashboard_audit(guild_id,actor_id,actor_name,source,action,before,after) VALUES($1,$2,'Actor','web','roles.test',$3::jsonb,$3::jsonb)")
        .bind(guild as i64).bind(actor as i64).bind(r#"{"nested":[{"access_token":"CANARY_SECRET","password":"CANARY_PASS"}],"source":"https://user:CANARY_URL_PASS@feed.example/?api_key=CANARY_QUERY_KEY","safe":true}"#)
        .execute(h.state.app.db.pool()).await.unwrap();
    for suffix in ["", "/audit", "/audit-tools", "/audit-tools/export"] {
        let response = browser.get(&format!("/api/guilds/{guild}{suffix}")).await;
        assert_eq!(response.status, StatusCode::OK);
        assert!(!response.text().contains("CANARY_"), "{suffix}");
        assert!(response.text().contains("REDACTED"), "{suffix}");
    }
    assert_eq!(
        browser
            .get(&format!("/api/guilds/{guild}/audit-tools?text=CANARY_SECRET"))
            .await
            .json()["items"],
        json!([])
    );
    sqlx::query("INSERT INTO dashboard_audit(guild_id,actor_id,actor_name,source,action) SELECT $1,$2,'Actor','web','roles.skip' FROM generate_series(1,501)")
        .bind(guild as i64).bind(actor as i64).execute(h.state.app.db.pool()).await.unwrap();
    let filter = pinchora_db::AuditFilter {
        text: Some("REDACTED".into()),
        ..Default::default()
    };
    let page = h
        .state
        .app
        .db
        .filtered_audit_page(guild, None, 50, &filter)
        .await
        .unwrap();
    assert!(page.items.is_empty());
    assert_eq!(page.scanned, 500);
    assert!(page.next.is_some());
    let page = h
        .state
        .app
        .db
        .filtered_audit_page(guild, page.next, 50, &filter)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.scanned, 2);
    assert!(page.next.is_none());
    let csv = browser
        .get(&format!("/api/guilds/{guild}/audit-tools/export?text=REDACTED"))
        .await;
    assert_eq!(csv.text().lines().count(), 2);
}

#[tokio::test]
async fn slow_csv_consumers_stop_at_the_total_export_deadline() {
    let Some(h) = Harness::new("w7_time_caps").await else {
        return;
    };
    let (guild, actor, mut browser) = h.managed_server().await;
    h.state
        .app
        .db
        .audit(&pinchora_db::NewAudit {
            guild,
            actor,
            actor_name: "Actor",
            source: pinchora_db::AuditSource::Web,
            action: "roles.test",
            target: None,
            before: None,
            after: None,
        })
        .await
        .unwrap();
    // A caller that stalls the stream cannot resume fetching data indefinitely.
    let cookies = browser
        .cookies
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ");
    let response = h
        .router
        .clone()
        .oneshot(
            Request::get(format!("/api/guilds/{guild}/audit-tools/export"))
                .header(header::COOKIE, cookies)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    tokio::time::sleep(Duration::from_millis(10_050)).await;
    let body = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("EXPORT_LIMIT"));
    assert_eq!(
        browser.get(&format!("/api/guilds/{guild}/audit-tools")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn personal_writes_are_audited_without_private_content() {
    let Some(h) = Harness::new("w7_personal_audit").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}/community-v2/personal");
    let added = browser
        .write(
            Method::PUT,
            &format!("{base}/checklist"),
            json!({"text":"PRIVATE_CANARY"}),
        )
        .await;
    assert_eq!(added.status, StatusCode::OK);
    let id = added.json()["id"].as_str().unwrap().to_owned();
    assert_eq!(
        browser
            .write(Method::PUT, &format!("{base}/checklist/{id}"), json!({}))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        browser
            .write(Method::DELETE, &format!("{base}/checklist/{id}"), json!({}))
            .await
            .status,
        StatusCode::OK
    );
    let logs = h.state.app.db.audit_page(guild, None, 20).await.unwrap();
    assert_eq!(logs.len(), 3);
    assert!(logs.iter().all(|r| r.action.starts_with("community.personal.")
        && !r.after.as_deref().unwrap_or_default().contains("PRIVATE_CANARY")));
}

#[tokio::test]
async fn migration_rejects_null_areas_and_audit_candidates_use_the_existing_guild_index() {
    let Some(h) = Harness::new("w7_migration_index").await else {
        return;
    };
    let guild = unique();
    let result = sqlx::query("INSERT INTO dashboard_managers VALUES($1,$2,ARRAY['roles',NULL])")
        .bind(guild as i64)
        .bind(FAN as i64)
        .execute(h.state.app.db.pool())
        .await;
    assert!(result.unwrap_err().as_database_error().unwrap().is_check_violation());
    let mut tx = h.state.app.db.pool().begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan=off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: Value = sqlx::query_scalar(
        "EXPLAIN (FORMAT JSON) SELECT id FROM dashboard_audit WHERE guild_id=$1 AND id<$2 ORDER BY id DESC LIMIT 500",
    )
    .bind(guild as i64)
    .bind(i64::MAX)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(plan.to_string().contains("dashboard_audit_guild_idx"));
    tx.rollback().await.unwrap();
}
