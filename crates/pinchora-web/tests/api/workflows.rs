use super::*;
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
        .set_module(guild, pinchora_core::modules::Module::Birthdays, Some(false), None)
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
