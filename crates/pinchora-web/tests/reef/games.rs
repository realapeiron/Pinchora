use super::*;
use reqwest::{Method, StatusCode};
async fn create(b: &Browser, owner: u64, kind: &str) -> Value {
    let (status, g) = b
        .api_value(
            Method::POST,
            &format!("/api/reef/rooms/house:{owner}:0/games"),
            json!({"kind":kind,"request_id":unique().to_string(),"lang":"en"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{g}");
    g
}
async fn action(b: &Browser, g: &Value, intent: Value) -> (StatusCode, Value) {
    let mut a = intent;
    a["request_id"] = json!(unique().to_string());
    a["revision"] = g["revision"].clone();
    b.api_value(
        Method::POST,
        &format!("/api/reef/games/{}/actions", g["id"].as_str().unwrap()),
        a,
    )
    .await
}
#[tokio::test]
async fn room_games_chess_match_spectator_stale_retry_csrf_and_private_alts() {
    let _capacity = fixture_capacity().await;
    let Some(site) = Site::new().await else {
        return;
    };
    let host = unique();
    site.house(host).await;
    let a = site.browser(host).await;
    let b = site.browser(unique()).await;
    let spectator = site.browser(unique()).await;
    let mut g = create(&a, host, "chess").await;
    let (code, next) = action(&b, &g, json!({"action":"join"})).await;
    assert_eq!(code, StatusCode::OK, "{next}");
    g = next;
    assert_eq!(
        action(&spectator, &g, json!({"action":"move","code":"f2f3"})).await.0,
        StatusCode::FORBIDDEN
    );
    let old = g.clone();
    for (i, mv) in ["f2f3", "e7e5", "g2g4", "d8h4"].into_iter().enumerate() {
        let (code, next) = action(if i % 2 == 0 { &a } else { &b }, &g, json!({"action":"move","code":mv})).await;
        assert_eq!(code, StatusCode::OK, "{next}");
        g = next;
        if i == 0 {
            assert_eq!(
                action(&a, &old, json!({"action":"move","code":"e2e4"})).await.0,
                StatusCode::CONFLICT
            );
        }
    }
    assert_eq!(g["board"]["phase"], "done");
    assert_eq!(g["board"]["winner"], 1);
    assert!(g["players"].as_array().unwrap().iter().all(|p| p["paid"] == 0));
    let stats = site.app.db.game_stats(host).await.unwrap();
    assert_eq!(stats[0].played, 1);
    let response = reqwest::Client::new()
        .post(format!("{}/api/reef/rooms/house:{host}:0/games", site.url))
        .header("cookie", format!("pinchora_sid={}", a.sid))
        .header("origin", &site.url)
        .json(&json!({"kind":"chess","request_id":"csrf"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
#[tokio::test]
async fn room_games_trivia_conceal_answer_reconnect_and_deadline() {
    let _capacity = fixture_capacity().await;
    let Some(site) = Site::new().await else {
        return;
    };
    let host = unique();
    site.house(host).await;
    let a = site.browser(host).await;
    let b = site.browser(unique()).await;
    let g = create(&a, host, "trivia").await;
    let (_, g) = action(&b, &g, json!({"action":"join"})).await;
    let (code, g) = action(&a, &g, json!({"action":"start"})).await;
    assert_eq!(code, StatusCode::OK, "{g}");
    assert!(g["board"].get("correct").is_none());
    assert!(g["board"].get("order").is_none());
    let (code, answered) = action(&a, &g, json!({"action":"answer","choice":0})).await;
    assert_eq!(code, StatusCode::OK, "{answered}");
    assert_eq!(answered["answered"], true);
    let (code, resumed) = a
        .api_value(
            Method::GET,
            &format!("/api/reef/games/{}", g["id"].as_str().unwrap()),
            Value::Null,
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(resumed["answered"], true);
    assert!(resumed["board"].get("correct").is_none());
    assert_ne!(
        action(&a, &answered, json!({"action":"answer","choice":1})).await.0,
        StatusCode::OK
    );
    sqlx::query("UPDATE reef_games SET deadline=now()-interval '1 second' WHERE id=$1")
        .bind(g["id"].as_str().unwrap().parse::<i64>().unwrap())
        .execute(&site.pool)
        .await
        .unwrap();
    let (code, _) = action(&b, &g, json!({"action":"answer","choice":3})).await;
    assert_eq!(code, StatusCode::CONFLICT);
    let (code, reveal) = b
        .api_value(
            Method::GET,
            &format!("/api/reef/games/{}", g["id"].as_str().unwrap()),
            Value::Null,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{reveal}");
    assert_eq!(reveal["board"]["phase"], "done");
    assert!(reveal["board"]["correct"].is_number());
    assert!(!reveal["answered"].as_bool().unwrap());
}
#[tokio::test]
async fn room_games_picross_cross_transport_resume_stale_and_private_projection() {
    let _capacity = fixture_capacity().await;
    let Some(site) = Site::new().await else {
        return;
    };
    let host = unique();
    site.house(host).await;
    let a = site.browser(host).await;
    let path = format!("/api/reef/rooms/house:{host}:0/picross");
    let (code, p) = a.api_value(Method::POST, &path, json!({"mode":"daily"})).await;
    assert_eq!(code, StatusCode::OK, "{p}");
    assert!(p.get("solution").is_none());
    let day = p["day"].as_i64().unwrap() as i32;
    let grid = format!("#{}", ".".repeat(99));
    site.app
        .db
        .picross_marks(host, "daily", day, p["grid"].as_str().unwrap(), &grid, false)
        .await
        .unwrap();
    let (code, stale) = a
        .api_value(
            Method::POST,
            &format!("{path}/marks"),
            json!({"mode":"daily","day":day,"grid":p["grid"],"revision":p["revision"],"cell":1,"filled":true}),
        )
        .await;
    assert_eq!(code, StatusCode::CONFLICT, "{stale}");
    let (_, resumed) = a.api_value(Method::POST, &path, json!({"mode":"daily"})).await;
    assert_eq!(resumed["grid"], grid);
    let (code,result)=a.api_value(Method::POST,&format!("{path}/marks"),json!({"mode":"daily","day":day,"grid":resumed["grid"],"revision":resumed["revision"],"cell":1,"filled":true})).await;
    assert_eq!(code, StatusCode::OK, "{result}");
    assert_eq!(
        &site
            .app
            .db
            .picross_game(host, "daily", day)
            .await
            .unwrap()
            .unwrap()
            .grid[..2],
        "##"
    );
}
#[tokio::test]
async fn picross_full_board_budget_is_separate_but_still_authenticated_and_limited() {
    let _capacity = fixture_capacity().await;
    let Some(site) = Site::new().await else { return };
    let host = unique();
    site.house(host).await;
    let a = site.browser(host).await;
    let path = format!("/api/reef/rooms/house:{host}:0/picross");

    // Exhaust ordinary writes in this and the next minute, without depending on the
    // wall clock staying in the same fixed window for the duration of this test.
    let window = chrono::Utc::now().timestamp() / 60;
    for minute in window..=window + 1 {
        site.app
            .cache
            .incr_by(&format!("web:rl:write:{host}:{minute}"), 60, Duration::from_secs(120))
            .await
            .unwrap();
    }
    assert_eq!(
        a.api_value(Method::POST, "/api/reef/socket-ticket", json!({})).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    let (code, mut p) = a.api_value(Method::POST, &path, json!({"mode":"daily"})).await;
    assert_eq!(code, StatusCode::OK, "{p}");
    for index in 0..100 {
        let (code, next) = a
            .api_value(
                Method::POST,
                &format!("{path}/marks"),
                json!({
                    "mode":"daily", "day":p["day"], "revision":p["revision"],
                    "grid":p["grid"], "cell":0, "filled":index % 2 == 0,
                }),
            )
            .await;
        assert_eq!(code, StatusCode::OK, "mark {index}: {next}");
        assert_eq!(next["grid"].as_str().unwrap().starts_with('#'), index % 2 == 0);
        p = next;
    }

    // This route-specific budget does not bypass session or CSRF checks.
    let client = reqwest::Client::new();
    let url = format!("{}{path}/marks", site.url);
    assert_eq!(
        client.post(&url).json(&json!({})).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .post(&url)
            .header("cookie", format!("pinchora_sid={}", a.sid))
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );

    // The existing 180-mark limit is shared across sessions for the account.
    let window = chrono::Utc::now().timestamp() / 60;
    for minute in window..=window + 1 {
        site.app
            .cache
            .incr_by(
                &format!("web:rl:reef:picross-mark:{host}:{minute}"),
                180,
                Duration::from_secs(120),
            )
            .await
            .unwrap();
    }
    let b = site.browser(host).await;
    let (code, error) = b
        .api_value(
            Method::POST,
            &format!("{path}/marks"),
            json!({
                "mode":"daily", "day":p["day"], "revision":p["revision"],
                "grid":p["grid"], "cell":0, "filled":true,
            }),
        )
        .await;
    assert_eq!(code, StatusCode::TOO_MANY_REQUESTS, "{error}");
}

#[tokio::test]
async fn room_games_checkers_socket_and_http_share_authority() {
    let _capacity = fixture_capacity().await;
    let Some(site) = Site::new().await else {
        return;
    };
    let host = unique();
    let guest = unique();
    site.house(host).await;
    let a = site.browser(host).await;
    let b = site.browser(guest).await;
    let g = create(&a, host, "checkers").await;
    let (_, g) = action(&b, &g, json!({"action":"join"})).await;
    let mut socket = a.connect().await;
    socket.join(host).await;
    let action = json!({"request_id":"socket-move","revision":g["revision"],"action":"move","code":g["board"]["legal"][0]["code"]});
    socket
        .send(json!({"type":"game_action","game_id":g["id"],"payload":action}))
        .await;
    let event = socket.until_any(&["game_changed", "error"]).await;
    assert_eq!(event["type"], "game_changed", "{event}");
    let (code, replayed) = a
        .api_value(
            Method::POST,
            &format!("/api/reef/games/{}/actions", g["id"].as_str().unwrap()),
            action,
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{replayed}");
    assert_eq!(replayed["revision"], event["game"]["revision"]);
}

#[tokio::test]
async fn room_games_furniture_revision_binding_blocks_and_wire_limits() {
    let _capacity = fixture_capacity().await;
    let Some(site) = Site::new().await else {
        return;
    };
    let host = unique();
    site.house(host).await;
    let a = site.browser(host).await;
    let guest = unique();
    let b = site.browser(guest).await;
    let item = "blinkslot_cabinet__plum";
    site.app.db.grant_item(host, item, 1).await.unwrap();
    site.app
        .db
        .place_item(host, 0, item, 1, 1, false, |_| Ok(()))
        .await
        .unwrap();
    let placement = site.app.db.placements(host, 0).await.unwrap()[0].id;
    let revision = site.app.db.reef_edit_revision(host).await.unwrap();
    let path = format!("/api/reef/rooms/house:{host}:0/games");
    let mut body =
        json!({"kind":"checkers","request_id":"bound","placement_id":placement.to_string(),"layout_revision":"-1"});
    assert_eq!(
        a.api_value(Method::POST, &path, body.clone()).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    body["layout_revision"] = json!(revision);
    let (code, g) = a.api_value(Method::POST, &path, body.clone()).await;
    assert_eq!(code, StatusCode::OK, "{g}");
    assert_eq!(g["placement_id"], placement.to_string());
    let (_, replay) = a.api_value(Method::POST, &path, body.clone()).await;
    assert_eq!(replay["id"], g["id"]);
    body["request_id"] = json!("occupied");
    assert_eq!(
        a.api_value(Method::POST, &path, body).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (code, g) = action(&b, &g, json!({"action":"join"})).await;
    assert_eq!(code, StatusCode::OK, "{g}");
    let url = format!("/api/reef/games/{}", g["id"].as_str().unwrap());
    assert_eq!(
        action(&a, &g, json!({"action":"move","code":"x".repeat(65)})).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        action(&a, &g, json!({"action":"move","code":"0.1","reward":999}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    site.app.db.pick_up(host, placement).await.unwrap();
    let (code, moved) = action(&a, &g, json!({"action":"move","code":g["board"]["legal"][0]["code"]})).await;
    assert_eq!(code, StatusCode::OK, "{moved}");
    assert!(moved["placement_id"].is_null());
    site.app.db.block_signer(host, guest, "guest").await.unwrap();
    assert_eq!(
        b.api_value(Method::GET, &url, Value::Null).await.0,
        StatusCode::FORBIDDEN
    );
    let forged = format!("/api/reef/rooms/house:{host}:99/games");
    assert_ne!(
        a.api_value(
            Method::POST,
            &forged,
            json!({"kind":"chess","request_id":"missing-room"})
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn room_games_picross_retired_practice_cannot_expose_upcoming_daily() {
    let _capacity = fixture_capacity().await;
    let Some(site) = Site::new().await else {
        return;
    };
    let user = unique();
    site.house(user).await;
    let b = site.browser(user).await;
    let day = chrono::Utc::now().timestamp().div_euclid(86400);
    let (picture, variant) = pinchora_game::play::picross::daily(day);
    let old = site
        .app
        .db
        .start_picross(user, "practice", 0, (picture as u8, variant), true)
        .await
        .unwrap();
    let path = format!("/api/reef/rooms/house:{user}:0/picross");
    let (code,_) = b.api_value(Method::POST,&format!("{path}/marks"),json!({"mode":"practice","day":0,"revision":old.revision.to_string(),"grid":old.grid,"cell":0,"filled":true})).await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    let (code, _) = b.api_value(Method::POST, &path, json!({"mode":"practice"})).await;
    assert_eq!(code, StatusCode::OK);
    let next = site.app.db.picross_game(user, "practice", 0).await.unwrap().unwrap();
    assert_ne!(next.picture, old.picture);
    assert!(next.revision > old.revision);
}

#[tokio::test]
async fn trivia_waiting_stale_spectator_and_leave_cannot_bypass_the_round() {
    let _capacity = fixture_capacity().await;
    let Some(site) = Site::new().await else {
        return;
    };
    let host = unique();
    site.house(host).await;
    let a = site.browser(host).await;
    let b = site.browser(unique()).await;
    let c = site.browser(unique()).await;
    let spectator = site.browser(unique()).await;
    let waiting = create(&a, host, "trivia").await;
    assert!(waiting["board"].get("question").is_none());
    assert!(waiting["board"].get("options").is_none());
    let (_, joined) = action(&b, &waiting, json!({"action":"join"})).await;
    let (_, joined) = action(&c, &joined, json!({"action":"join"})).await;
    let (_, active) = action(&a, &joined, json!({"action":"start"})).await;
    assert!(active["board"]["question"].is_string());
    assert_eq!(
        action(&b, &joined, json!({"action":"answer","choice":0})).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        action(&spectator, &active, json!({"action":"answer","choice":0}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (code, answered) = action(&a, &active, json!({"action":"answer","choice":0})).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(
        action(&b, &active, json!({"action":"answer","choice":1})).await.0,
        StatusCode::CONFLICT
    );
    let (code, left) = action(&b, &answered, json!({"action":"leave"})).await;
    assert_eq!(code, StatusCode::OK, "{left}");
    assert_eq!(left["board"]["phase"], "active");
    assert!(left["board"].get("correct").is_none());
    assert!(left["side"].is_null());
    assert_eq!(left["players"].as_array().unwrap().len(), 2);
    let (code, done) = action(&c, &left, json!({"action":"answer","choice":1})).await;
    assert_eq!(code, StatusCode::OK, "{done}");
    assert!(done["board"]["correct"].is_number());
    let waiting = create(&a, host, "trivia").await;
    let (_, cancelled) = action(&a, &waiting, json!({"action":"leave"})).await;
    assert!(cancelled["board"].get("correct").is_none());
}

#[tokio::test]
async fn private_picross_http_completion_has_no_ledger_payment() {
    let _capacity = fixture_capacity().await;
    let Some(site) = Site::new().await else {
        return;
    };
    let host = unique();
    site.house(host).await;
    let a = site.browser(host).await;
    let path = format!("/api/reef/rooms/house:{host}:0/picross");
    let (_, p) = a.api_value(Method::POST, &path, json!({"mode":"daily"})).await;
    let day = p["day"].as_i64().unwrap() as i32;
    let row = site.app.db.picross_game(host, "daily", day).await.unwrap().unwrap();
    let puzzle = pinchora_game::play::picross::Puzzle::new(row.picture as usize, row.variant as u8);
    let mut grid = pinchora_game::play::picross::Grid::default();
    let mut last = 0;
    for cell in 0..100 {
        if puzzle.filled(cell) {
            grid.set(cell, true);
            last = cell;
        }
    }
    grid.set(last, false);
    let row = site
        .app
        .db
        .picross_marks(host, "daily", day, &row.grid, &grid.encode(), false)
        .await
        .unwrap()
        .unwrap();
    let (code, solved) = a.api_value(Method::POST,&format!("{path}/marks"),
        json!({"mode":"daily","day":day,"grid":row.grid,"revision":row.revision.to_string(),"cell":last,"filled":true})).await;
    assert_eq!(code, StatusCode::OK, "{solved}");
    assert_eq!(solved["status"], "solved");
    assert_eq!(solved["paid"], 0);
}
