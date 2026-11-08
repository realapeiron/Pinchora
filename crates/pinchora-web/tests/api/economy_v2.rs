use super::*;
#[tokio::test]
async fn economy_session_csrf_idempotency_and_trade_ownership() {
    let Some(h) = Harness::new("economy_player_api").await else {
        return;
    };
    let mut browser = h.browser();
    let u = unique();
    browser.login(&h, u, vec![]).await;
    let root = "/api/me/economy";
    let anon = h.browser().get(root).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
    let start = json!({"request_id":"web-shift","command":{"action":"start_shift","job":"kelp_keeper"}});
    let csrf = browser.csrf.take();
    assert_eq!(
        browser
            .write(Method::POST, &format!("{root}/action"), start.clone())
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    browser.csrf = csrf;
    let response = browser
        .write(Method::POST, &format!("{root}/action"), start.clone())
        .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.text());
    assert_eq!(
        browser
            .write(Method::POST, &format!("{root}/action"), start)
            .await
            .json(),
        response.json()
    );
    let token = response.json()["token"].as_i64().unwrap();
    for step in 0..3 {
        let r = browser
            .write(
                Method::POST,
                &format!("{root}/action"),
                json!({"request_id":format!("step{step}"),"command":{"action":"step","token":token,"step":step}}),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    }
    assert_eq!(h.state.app.db.user(u).await.unwrap().credits, 100);
    let snap = browser.get(root).await;
    assert_eq!(snap.json()["shifts_today"], 1);
    let forged = browser
        .write(
            Method::POST,
            &format!("{root}/action"),
            json!({"request_id":"forged","user_id":"1","command":{"action":"chest"}}),
        )
        .await;
    assert_eq!(forged.status, StatusCode::BAD_REQUEST);
    let rejected = browser
        .write(
            Method::POST,
            &format!("{root}/action"),
            json!({"request_id":"unknown-trade","command":{"action":"confirm_trade","id":9223372036854775807i64}}),
        )
        .await;
    assert_eq!(rejected.status, StatusCode::UNPROCESSABLE_ENTITY);
}
#[tokio::test]
async fn economy_admin_area_validation_audit_and_point_reset() {
    let Some(h) = Harness::new("economy_admin_api").await else {
        return;
    };
    let (guild, user, mut browser) = h.managed_server().await;
    let root = format!("/api/guilds/{guild}/economy");
    let mut outsider = h.browser();
    outsider
        .login(&h, unique(), vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    assert_eq!(outsider.get(&root).await.status, StatusCode::FORBIDDEN);
    let body =
        json!({"revision":0,"settings":{"pool_goal":20000,"shop_min_level":5,"shop_min_days":7,"shop_enabled":true}});
    let r = browser.write(Method::PUT, &root, body.clone()).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.json()["revision"], 1);
    assert_eq!(
        browser.write(Method::PUT, &root, body).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM dashboard_audit WHERE guild_id=$1 AND action='economy.settings'")
            .bind(guild as i64)
            .fetch_one(h.state.app.db.pool())
            .await
            .unwrap();
    assert_eq!(count, 1);
    h.state.app.db.earn_shells(user, 500, "test").await.unwrap();
    sqlx::query("INSERT INTO members(guild_id,user_id,points) VALUES($1,$2,1000) ON CONFLICT(guild_id,user_id) DO UPDATE SET points=1000").bind(guild as i64).bind(user as i64).execute(h.state.app.db.pool()).await.unwrap();
    let p = browser.get(&format!("{root}/points")).await.json();
    let r = browser
        .write(
            Method::POST,
            &format!("{root}/points"),
            json!({"percent":0,"version":p["version"]}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.json()["total"], "0");
    assert_eq!(h.state.app.db.user(user).await.unwrap().credits, 500);
}
