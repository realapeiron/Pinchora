use crate::*;

async fn seed_sources(h: &Harness, guild: u64, owner: u64) {
    let pool = h.state.app.db.pool();
    sqlx::query("INSERT INTO tickets(guild_id,number,opener_id) VALUES($1,1,$2)")
        .bind(guild as i64)
        .bind(owner as i64)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO automod_settings(guild_id) VALUES($1)")
        .bind(guild as i64)
        .execute(pool)
        .await
        .unwrap();
    let rule: i64 = sqlx::query_scalar("INSERT INTO automod_rules(guild_id,name,kind,enabled,log_only,native,params,actions) VALUES($1,'links','links',true,false,false,'{}','[]') RETURNING id")
        .bind(guild as i64).fetch_one(pool).await.unwrap();
    sqlx::query("INSERT INTO automod_hits(guild_id,rule_id,user_id,channel_id,action) VALUES($1,$2,$3,123,'delete'),($1,$2,$3,124,'delete')")
        .bind(guild as i64).bind(rule).bind(owner as i64).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO mod_reports(guild_id,user_id,reporter_id,body) VALUES($1,$2,$2,'report')")
        .bind(guild as i64)
        .bind(owner as i64)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO mod_appeals(guild_id,case_number,user_id,body) VALUES($1,1,$2,'Appeal body here')")
        .bind(guild as i64)
        .bind(owner as i64)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO modmail_threads(guild_id,user_id,thread_id) VALUES($1,$2,$3)")
        .bind(guild as i64)
        .bind(owner as i64)
        .bind(unique() as i64)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO guild_activity_hourly(guild_id,hour,joins) VALUES($1,date_trunc('hour',now()),3)")
        .bind(guild as i64)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dashboard_audit(guild_id,actor_id,actor_name,source,action) VALUES($1,$2,'other','web','settings.patch'),($1,$3,'self','web','settings.patch')")
        .bind(guild as i64).bind(unique() as i64).bind(owner as i64).execute(pool).await.unwrap();
}

#[tokio::test]
async fn notifications_show_sources_and_read_marks() {
    let Some(h) = Harness::new("notifications_sources").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let now = chrono::Utc::now();
    seed_sources(&h, guild, owner).await;
    let path = format!("/api/guilds/{guild}/notifications");
    let response = browser.get(&path).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.text());
    let value = response.json();
    let items = value["items"].as_array().unwrap();
    for kind in ["ticket", "automod", "report", "appeal", "modmail", "members", "audit"] {
        assert!(items.iter().any(|item| item["kind"] == kind), "missing {kind}: {value}");
    }
    assert_eq!(items.iter().filter(|item| item["kind"] == "automod").count(), 1);
    assert_eq!(items.iter().filter(|item| item["kind"] == "audit").count(), 1);
    let auto = items.iter().find(|item| item["kind"] == "automod").unwrap();
    assert_eq!(auto["data"]["count"], 2);
    assert!(auto["data"]["channel_id"].is_null());
    assert_eq!(value["unread"], items.len() - 1);
    assert!(items.iter().all(|item| item["data"].get("body").is_none()));
    let bad = browser
        .write(Method::POST, &format!("{path}/read"), json!({"up_to":"bad"}))
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(bad.code(), "malformed");
    assert_eq!(
        bad.json()["error"]["message"],
        pinchora_core::t!(Lang::En, "notifications.bad_time")
    );
    h.state.app.set_guild_lang(guild, Some(Lang::Tr)).await.unwrap();
    let bad_tr = browser
        .write(Method::POST, &format!("{path}/read"), json!({"up_to":"bad"}))
        .await;
    assert_eq!(bad_tr.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        bad_tr.json()["error"]["message"],
        pinchora_core::t!(Lang::Tr, "notifications.bad_time")
    );
    let mark = browser
        .write(
            Method::POST,
            &format!("{path}/read"),
            json!({"up_to":(now + chrono::Duration::days(1)).to_rfc3339()}),
        )
        .await;
    assert_eq!(mark.status, StatusCode::OK);
    let at = chrono::DateTime::parse_from_rfc3339(mark.json()["read_at"].as_str().unwrap()).unwrap();
    assert!(at <= chrono::Utc::now());
    let older = browser
        .write(
            Method::POST,
            &format!("{path}/read"),
            json!({"up_to":(now - chrono::Duration::days(1)).to_rfc3339()}),
        )
        .await;
    assert_eq!(older.json()["read_at"], mark.json()["read_at"]);
    assert_eq!(browser.get(&path).await.json()["unread"], 0);
    let anonymous = h.browser().get(&path).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        h.browser()
            .write(Method::POST, &format!("{path}/read"), json!({"up_to":now.to_rfc3339()}))
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn notifications_respect_areas_modules_guilds_and_window() {
    let Some(h) = Harness::new("notifications_scope").await else {
        return;
    };
    let (guild, owner, mut full) = h.managed_server().await;
    let pool = h.state.app.db.pool();
    let other_guild = unique();
    sqlx::query("INSERT INTO tickets(guild_id,number,opener_id) VALUES($1,1,$2),($3,1,$2)")
        .bind(guild as i64)
        .bind(owner as i64)
        .bind(other_guild as i64)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tickets(guild_id,number,opener_id,opened_at) VALUES($1,2,$2,now()-interval '8 days')")
        .bind(guild as i64)
        .bind(owner as i64)
        .execute(pool)
        .await
        .unwrap();
    let before_joins = full.get(&format!("/api/guilds/{guild}/notifications")).await.json();
    assert!(
        before_joins["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["kind"] != "members")
    );
    sqlx::query("INSERT INTO guild_activity_hourly(guild_id,hour,joins) VALUES($1,date_trunc('hour',now()),3),($1,date_trunc('hour',now())-interval '1 hour',2)")
        .bind(guild as i64).execute(pool).await.unwrap();
    let path = format!("/api/guilds/{guild}/notifications");
    let initial = full.get(&path).await.json();
    assert_eq!(initial["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        initial["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["kind"] == "members")
            .unwrap()["data"]["count"],
        5
    );
    h.state
        .app
        .db
        .set_module(guild, pinchora_core::admin::modules::Module::Tickets, Some(false), None)
        .await
        .unwrap();
    let off = full.get(&path).await.json();
    assert_eq!(off["items"].as_array().unwrap().len(), 1);
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
                areas: vec!["stats".into()],
            }],
        )
        .await
        .unwrap();
    let mut member = h.browser();
    member
        .login(&h, delegate, vec![guild_entry(guild, "Scoped", false, 0)])
        .await;
    let scoped = member.get(&path).await;
    assert_eq!(scoped.status, StatusCode::OK, "{}", scoped.text());
    assert_eq!(scoped.json()["items"].as_array().unwrap().len(), 1);
    assert_eq!(scoped.json()["items"][0]["kind"], "members");
    assert_eq!(scoped.json()["unread"], 0);
    assert_eq!(
        member
            .write(
                Method::POST,
                &format!("{path}/read"),
                json!({"up_to":chrono::Utc::now().to_rfc3339()})
            )
            .await
            .status,
        StatusCode::OK
    );
    h.mock.member_roles.lock().unwrap().insert(delegate, vec![]);
    assert_eq!(member.get(&path).await.status, StatusCode::FORBIDDEN);
    assert_eq!(
        member
            .write(
                Method::POST,
                &format!("{path}/read"),
                json!({"up_to":chrono::Utc::now().to_rfc3339()})
            )
            .await
            .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn notifications_count_unread_before_display_limit() {
    let Some(h) = Harness::new("notifications_unread_limit").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let pool = h.state.app.db.pool();
    sqlx::query("INSERT INTO tickets(guild_id,number,opener_id) SELECT $1,n,$2 FROM generate_series(1,20) n")
        .bind(guild as i64)
        .bind(owner as i64)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dashboard_audit(guild_id,actor_id,actor_name,source,action) SELECT $1,$2,'other','web','settings.patch' FROM generate_series(1,5)")
        .bind(guild as i64).bind(unique() as i64).execute(pool).await.unwrap();
    let value = browser.get(&format!("/api/guilds/{guild}/notifications")).await.json();
    assert_eq!(value["items"].as_array().unwrap().len(), 20);
    assert_eq!(value["unread"], 25);
}

#[tokio::test]
async fn notifications_group_all_automod_hits_before_limit() {
    let Some(h) = Harness::new("notifications_automod_groups").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let pool = h.state.app.db.pool();
    sqlx::query("INSERT INTO automod_settings(guild_id) VALUES($1)")
        .bind(guild as i64)
        .execute(pool)
        .await
        .unwrap();
    let rule: i64 = sqlx::query_scalar("INSERT INTO automod_rules(guild_id,name,kind,enabled,log_only,native,params,actions) VALUES($1,'links','links',true,false,false,'{}','[]') RETURNING id")
        .bind(guild as i64).fetch_one(pool).await.unwrap();
    sqlx::query("INSERT INTO automod_hits(guild_id,rule_id,user_id,channel_id,action,created_at) SELECT $1,$2,$3,123,'delete',date_trunc('hour',now()) FROM generate_series(1,250)")
        .bind(guild as i64).bind(rule).bind(owner as i64).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO automod_hits(guild_id,rule_id,user_id,channel_id,action,created_at) VALUES($1,$2,$3,124,'delete',date_trunc('hour',now())-interval '1 hour')")
        .bind(guild as i64).bind(rule).bind(owner as i64).execute(pool).await.unwrap();
    let value = browser.get(&format!("/api/guilds/{guild}/notifications")).await.json();
    let groups: Vec<_> = value["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == "automod")
        .collect();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["data"]["count"], 250);
    assert_eq!(groups[0]["data"]["channel_id"], "123");
    assert_eq!(groups[1]["data"]["count"], 1);
    assert_eq!(groups[1]["data"]["channel_id"], "124");
}

#[tokio::test]
async fn notifications_resolve_known_people_and_omit_unknown_names() {
    let Some(h) = Harness::new("notifications_people").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let unknown = unique();
    let pool = h.state.app.db.pool();
    sqlx::query("INSERT INTO tickets(guild_id,number,opener_id) VALUES($1,1,$2),($1,2,$3)")
        .bind(guild as i64)
        .bind(owner as i64)
        .bind(unknown as i64)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO modmail_threads(guild_id,user_id,thread_id) VALUES($1,$2,$4),($1,$3,$5)")
        .bind(guild as i64)
        .bind(owner as i64)
        .bind(unknown as i64)
        .bind(unique() as i64)
        .bind(unique() as i64)
        .execute(pool)
        .await
        .unwrap();
    let value = browser.get(&format!("/api/guilds/{guild}/notifications")).await.json();
    let items = value["items"].as_array().unwrap();
    for (kind, id_key, name_key) in [
        ("ticket", "opener_id", "opener_name"),
        ("modmail", "user_id", "user_name"),
    ] {
        let known = items
            .iter()
            .find(|item| {
                item["kind"] == kind
                    && item["data"][id_key].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(owner)
            })
            .unwrap();
        assert_eq!(known["data"][name_key], format!("user{owner} G"));
        let missing = items
            .iter()
            .find(|item| {
                item["kind"] == kind
                    && item["data"][id_key].as_str().and_then(|id| id.parse::<u64>().ok()) == Some(unknown)
            })
            .unwrap();
        assert!(missing["data"].get(name_key).is_none());
    }
}
