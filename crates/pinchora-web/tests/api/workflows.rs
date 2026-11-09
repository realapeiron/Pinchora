use crate::*;
fn definition() -> Value {
    json!({"name":"greet","enabled":false,"trigger":{"kind":"button"},"conditions":[],"actions":[{"kind":"set_var","scope":"server","key":"visits","value":"1"}]})
}
#[tokio::test]
async fn workflows_crud_dry_run_and_audit() {
    let Some(h) = Harness::new("workflows_crud").await else {
        return;
    };
    let (guild, user, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/workflows");
    h.state
        .app
        .db
        .set_module(
            guild,
            pinchora_core::admin::modules::Module::Birthdays,
            Some(false),
            None,
        )
        .await
        .unwrap();
    h.state.app.forget_guild_settings(guild).await;
    assert!(
        !browser.get(&path).await.json()["triggers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["kind"] == "birthday")
    );
    let token = browser.csrf.take();
    assert_eq!(
        browser.write(Method::POST, &path, definition()).await.status,
        StatusCode::FORBIDDEN
    );
    browser.csrf = token;
    let other = browser.get(&format!("/api/guilds/{}/workflows", guild + 123)).await;
    assert!(!other.status.is_success());
    let made = browser.write(Method::POST, &path, definition()).await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    let row = made.json();
    let id = row["id"].as_str().unwrap();
    let test = browser
        .write(
            Method::POST,
            &format!("{path}/test"),
            json!({"definition":definition(),"event":{"kind":"button","user_id":user.to_string()}}),
        )
        .await;
    assert_eq!(test.status, StatusCode::OK, "{}", test.text());
    assert_eq!(test.json()["playback"]["steps"][1]["status"], "would_run");
    assert!(h.state.app.db.workflow_vars(guild, user).await.unwrap().is_empty());
    assert!(
        h.state
            .app
            .db
            .workflow_runs(guild, id.parse().unwrap())
            .await
            .unwrap()
            .is_empty()
    );
    check_member_variables(&mut browser, &path, user).await;
    let mut bad = definition();
    bad["actions"] = json!([{"kind":"give_shells","amount":50}]);
    assert!(!browser.write(Method::POST, &path, bad).await.status.is_success());
    let mut keyword = definition();
    keyword["name"] = json!("keyword");
    keyword["enabled"] = json!(true);
    keyword["trigger"] = json!({"kind":"message_keyword","keyword":"hi"});
    assert_eq!(
        browser.write(Method::POST, &path, keyword).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut replacement = definition();
    replacement["name"] = json!("updated");
    assert_eq!(
        browser
            .write(Method::PUT, &format!("{path}/{id}"), replacement)
            .await
            .status,
        StatusCode::OK
    );
    assert!(
        h.state
            .app
            .db
            .audit_page(guild, None, 100)
            .await
            .unwrap()
            .iter()
            .any(|a| a.action == "workflow.save")
    );
    assert_eq!(
        browser
            .write(Method::DELETE, &format!("{path}/{id}"), json!({}))
            .await
            .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn workflow_test_run_has_no_database_or_discord_writes() {
    let Some(h) = Harness::new("workflow_dry_spies").await else {
        return;
    };
    let (guild, user, mut browser) = h.managed_server().await;
    let readonly = Db::connect(
        &format!("{}?options=-c%20default_transaction_read_only%3Don", h.database),
        2,
    )
    .await
    .unwrap();
    // Any attempted database mutation fails the request, including writes outside workflow tables.
    assert!(
        sqlx::query("CREATE TABLE workflow_test_must_not_write(id int)")
            .execute(readonly.pool())
            .await
            .is_err()
    );
    let app = App::new(
        h.state.app.http.clone(),
        h.state.app.http.clone(),
        Id::new(APPLICATION_ID),
        readonly,
        h.state.app.cache.clone(),
        "workflow_readonly".into(),
    );
    let state = WebState::new(Arc::new(app), (*h.state.settings).clone());
    browser.router = pinchora_web::router(state);
    let before = h.mock.workflow_http_writes.load(Ordering::SeqCst);
    for action in [
        json!({"kind":"reply","text":"secret {var.member.key}"}),
        json!({"kind":"tag","action":{"kind":"add_role","role_id":FAN.to_string()}}),
        json!({"kind":"tag","action":{"kind":"remove_role","role_id":FAN.to_string()}}),
        json!({"kind":"tag","action":{"kind":"dm","text":"hello"}}),
        json!({"kind":"tag","action":{"kind":"give_points","amount":1}}),
        json!({"kind":"set_var","scope":"member","key":"key","value":"secret"}),
        json!({"kind":"incr_var","scope":"server","key":"count","amount":1}),
        json!({"kind":"open_ticket","panel_id":1,"topic":0}),
        json!({"kind":"create_thread","channel_id":GENERAL.to_string(),"name":"thread"}),
        json!({"kind":"pin"}),
    ] {
        let mut def = definition();
        def["actions"] = json!([action]);
        let response = browser.write(Method::POST, &format!("/api/guilds/{guild}/workflows/test"), json!({
            "definition":def,"event":{"kind":"button","user_id":user.to_string(),"channel_id":GENERAL.to_string()}
        })).await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.text());
        assert_eq!(response.json()["playback"]["outcome"], "ready");
    }
    assert_eq!(h.mock.workflow_http_writes.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn workflow_save_rechecks_all_branch_roles_even_when_unchanged() {
    let Some(h) = Harness::new("workflow_save_roles").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/workflows");
    let mut def = definition();
    def["actions"] = json!([{"kind":"random_branch","branches":[[{"kind":"tag","action":{"kind":"remove_role","role_id":FAN.to_string()}}]]}]);
    let saved = browser.write(Method::POST, &path, def.clone()).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let row = saved.json();
    let id = row["id"].as_str().unwrap();
    h.mock.bot_position.store(1, Ordering::SeqCst);
    let response = browser.write(Method::PUT, &format!("{path}/{id}"), def).await;
    assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", response.text());
}

async fn check_member_variables(browser: &mut Browser, path: &str, user: u64) {
    let member = user + 17;
    assert_eq!(
        browser
            .write(
                Method::PUT,
                &format!("{path}/variables"),
                json!({"scope":"member","subject_id":member.to_string(),"key":"count","value":"3"})
            )
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        browser.get(&format!("{path}/variables?subject={member}")).await.json()["items"][0][2],
        "3"
    );
    assert!(
        browser.get(&format!("{path}/variables")).await.json()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
