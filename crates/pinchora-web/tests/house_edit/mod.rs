use super::*;

impl Browser {
    async fn op(&mut self, path: &str, expected: &str, op: Value, csrf: bool) -> Response {
        let me = self.get("/api/me").await.json();
        let mut req = Request::post(path)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, PUBLIC_URL)
            .header("sec-fetch-site", "same-origin");
        if csrf {
            req = req.header("X-CSRF-Token", me["csrf"].as_str().expect("csrf"));
        }
        self.send(
            req.body(Body::from(json!({"expected":expected,"op":op}).to_string()))
                .unwrap(),
        )
        .await
    }
}
const CHAIR: &str = "plainwhorl_chair__sand";

#[tokio::test]
async fn ops_are_atomic_versioned_and_share_discord_history() {
    let Some(site) = Site::new("house_ops").await else {
        return;
    };
    let user = unique();
    site.move_in(user, 1, &[]).await;
    site.app.db.grant_item(user, CHAIR, 2).await.unwrap();
    let mut browser = site.browser();
    browser.login(user).await;
    let initial = browser.get("/api/house/rooms/0").await.json();
    let expected = initial["fingerprint"].as_str().unwrap();
    let op = json!({"type":"place","item_id":CHAIR,"x":0,"y":0,"flipped":false});
    assert_eq!(
        browser
            .op("/api/house/rooms/0/ops", expected, op.clone(), false)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let placed = browser.op("/api/house/rooms/0/ops", expected, op.clone(), true).await;
    assert_eq!(placed.status, StatusCode::OK, "{}", placed.text());
    let placed = placed.json();
    let stale = browser.op("/api/house/rooms/0/ops", expected, op.clone(), true).await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    assert_eq!(stale.json()["code"], "room_changed");
    assert_eq!(stale.json()["placements"], placed["placements"]);
    let expected = placed["fingerprint"].as_str().unwrap();
    let blocked = browser.op("/api/house/rooms/0/ops", expected, op, true).await;
    assert_eq!(blocked.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(blocked.code(), "blocked");
    let rows = site.app.db.placements(user, 0).await.unwrap();
    let mut history = pinchora_commands::undo::load(&site.app, pinchora_commands::Who::own(user), 0, &rows).await;
    let step = history.pop().expect("web operation is in Discord history");
    assert_eq!(step.kind, pinchora_commands::undo::Kind::Place);
    let id = placed["placements"][0]["id"].as_str().unwrap();
    let moved = browser
        .op(
            "/api/house/rooms/0/ops",
            expected,
            json!({"type":"move","id":id,"x":3,"y":2,"flipped":true}),
            true,
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    let moved = moved.json();
    let undo = browser
        .op(
            "/api/house/rooms/0/ops",
            moved["fingerprint"].as_str().unwrap(),
            json!({"type":"undo"}),
            true,
        )
        .await;
    assert_eq!(undo.status, StatusCode::OK, "{}", undo.text());
    assert_eq!(undo.json()["placements"][0]["x"], 0);
    let clear = browser
        .op(
            "/api/house/rooms/0/ops",
            undo.json()["fingerprint"].as_str().unwrap(),
            json!({"type":"clear"}),
            true,
        )
        .await;
    assert_eq!(clear.status, StatusCode::OK);
    assert_eq!(clear.json()["placements"], json!([]));
    let restored = browser
        .op(
            "/api/house/rooms/0/ops",
            clear.json()["fingerprint"].as_str().unwrap(),
            json!({"type":"undo"}),
            true,
        )
        .await;
    assert_eq!(restored.json()["placements"].as_array().unwrap().len(), 1);
    let thumb = browser.get("/api/house/thumbnail.png").await;
    assert_eq!(thumb.status, StatusCode::OK, "{}", thumb.text());
    assert!(thumb.body.starts_with(b"\x89PNG"));
    let etag = thumb.header("etag").to_owned();
    assert_eq!(
        browser
            .get_with("/api/house/thumbnail.png", "if-none-match", &etag)
            .await
            .status,
        StatusCode::NOT_MODIFIED
    );
}

#[tokio::test]
async fn concurrent_ops_accept_only_one_expected_version() {
    let Some(site) = Site::new("house_ops_concurrent").await else {
        return;
    };
    let user = unique();
    site.move_in(user, 1, &[]).await;
    site.app.db.grant_item(user, CHAIR, 2).await.unwrap();
    let expected = pinchora_commands::house_edit::fingerprint(&[]);
    let who = pinchora_commands::Who::own(user);
    let operation = |x| pinchora_commands::house_edit::Operation::Place {
        item_id: CHAIR.into(),
        x,
        y: 0,
        flipped: false,
    };
    let (a, b) = tokio::join!(
        pinchora_commands::house_edit::apply(&site.app, who, 0, (8, 8), &expected, operation(0)),
        pinchora_commands::house_edit::apply(&site.app, who, 0, (8, 8), &expected, operation(3))
    );
    let outcomes = [a.unwrap(), b.unwrap()];
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o, pinchora_commands::house_edit::Outcome::Done(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o, pinchora_commands::house_edit::Outcome::Conflict(_)))
            .count(),
        1
    );
    assert_eq!(site.app.db.placements(user, 0).await.unwrap().len(), 1);
}

#[tokio::test]
async fn partner_scope_consumes_editor_stock_and_revokes_access() {
    let Some(site) = Site::new("house_ops_partner").await else {
        return;
    };
    let (a, b) = (unique(), unique());
    site.move_in(a, 1, &[]).await;
    site.move_in(b, 1, &[]).await;
    site.app.db.pair_partners(a, b).await.unwrap();
    site.app.db.grant_item(a, CHAIR, 1).await.unwrap();
    let mut browser = site.browser();
    browser.login(a).await;
    let room = browser.get("/api/house/rooms/0?scope=partner").await.json();
    assert_eq!(room["whose"], b.to_string());
    let placed = browser
        .op(
            "/api/house/rooms/0/ops?scope=partner",
            room["fingerprint"].as_str().unwrap(),
            json!({"type":"place","item_id":CHAIR,"x":0,"y":0,"flipped":false}),
            true,
        )
        .await;
    assert_eq!(placed.status, StatusCode::OK, "{}", placed.text());
    assert_eq!(site.app.db.placements(b, 0).await.unwrap().len(), 1);
    assert!(!site.app.db.owns(a, CHAIR).await.unwrap());
    let denied = browser
        .op(
            "/api/house/rooms/0/ops?scope=partner",
            placed.json()["fingerprint"].as_str().unwrap(),
            json!({"type":"lock","locked":true}),
            true,
        )
        .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    let undo = browser
        .op(
            "/api/house/rooms/0/ops?scope=partner",
            placed.json()["fingerprint"].as_str().unwrap(),
            json!({"type":"undo"}),
            true,
        )
        .await;
    assert_eq!(undo.status, StatusCode::OK);
    assert!(site.app.db.owns(a, CHAIR).await.unwrap());
    site.app.db.leave_partner(a).await.unwrap();
    assert_eq!(
        browser.get("/api/house/rooms/0?scope=partner").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        browser.get("/api/house?scope=plaza&guild=123").await.status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn surface_confirmation_upgrade_and_lock_use_existing_tables() {
    let Some(site) = Site::new("house_ops_surfaces").await else {
        return;
    };
    let user = unique();
    site.move_in(user, 1, &[]).await;
    site.app.db.user(user).await.unwrap();
    sqlx::query("UPDATE users SET credits=0,pearls=0 WHERE id=$1")
        .bind(user as i64)
        .execute(&site.pool)
        .await
        .unwrap();
    let mut browser = site.browser();
    browser.login(user).await;
    let room = browser.get("/api/house/rooms/0").await.json();
    let expected = room["fingerprint"].as_str().unwrap();
    let surface = &Content::get().surfaces.wallpapers[1];
    let confirm = browser
        .op(
            "/api/house/rooms/0/ops",
            expected,
            json!({"type":"surface","wallpaper":surface.id}),
            true,
        )
        .await;
    assert_eq!(confirm.status, StatusCode::CONFLICT);
    assert_eq!(confirm.json()["price"], surface.price);
    let insufficient = browser
        .op(
            "/api/house/rooms/0/ops",
            expected,
            json!({"type":"surface","wallpaper":surface.id,"confirm_price":surface.price}),
            true,
        )
        .await;
    assert_eq!(insufficient.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(insufficient.json()["balance"], 0);
    assert!(!site.app.db.owns(user, &format!("wp:{}", surface.id)).await.unwrap());
    sqlx::query("UPDATE users SET credits=1000000,pearls=1000000 WHERE id=$1")
        .bind(user as i64)
        .execute(&site.pool)
        .await
        .unwrap();
    let bought = browser
        .op(
            "/api/house/rooms/0/ops",
            expected,
            json!({"type":"surface","wallpaper":surface.id,"confirm_price":surface.price}),
            true,
        )
        .await;
    assert_eq!(bought.status, StatusCode::OK, "{}", bought.text());
    assert!(site.app.db.owns(user, &format!("wp:{}", surface.id)).await.unwrap());
    let upgraded = browser
        .op(
            "/api/house/rooms/0/ops",
            expected,
            json!({"type":"upgrade","from_tier":1}),
            true,
        )
        .await;
    assert_eq!(upgraded.status, StatusCode::OK);
    assert_eq!(browser.get("/api/house").await.json()["house"]["tier"], 2);
    let locked = browser
        .op(
            "/api/house/rooms/0/ops",
            expected,
            json!({"type":"lock","locked":true}),
            true,
        )
        .await;
    assert_eq!(locked.status, StatusCode::OK);
    assert_eq!(browser.get("/api/house").await.json()["house"]["locked"], true);
}

#[tokio::test]
async fn plaza_requires_manage_server_and_records_web_audit() {
    let Some(site) = Site::new("house_ops_plaza").await else {
        return;
    };
    let (user, guild) = (unique(), unique());
    site.app.db.guild_settings(guild).await.unwrap();
    site.move_in(guild, 1, &[]).await;
    site.app.db.grant_item(guild, CHAIR, 1).await.unwrap();
    site.app
        .cache
        .put(&format!("web:bot-in:{guild}"), "1", std::time::Duration::from_secs(120))
        .await
        .unwrap();
    let mut browser = site.browser();
    let login=Request::post("/auth/dev-login").header(header::CONTENT_TYPE,"application/json").header(header::ORIGIN,PUBLIC_URL).header("sec-fetch-site","same-origin").body(Body::from(json!({"user_id":user.to_string(),"username":"manager","guilds":[{"id":guild.to_string(),"name":"Plaza","icon":null,"owner":false,"permissions":"32"}]}).to_string())).unwrap();
    assert_eq!(browser.send(login).await.status, StatusCode::OK);
    let room = browser
        .get(&format!("/api/house/rooms/0?scope=plaza&guild={guild}"))
        .await
        .json();
    let result = browser
        .op(
            &format!("/api/house/rooms/0/ops?scope=plaza&guild={guild}"),
            room["fingerprint"].as_str().unwrap(),
            json!({"type":"place","item_id":CHAIR,"x":0,"y":0,"flipped":false}),
            true,
        )
        .await;
    assert_eq!(result.status, StatusCode::OK, "{}", result.text());
    let (count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM dashboard_audit WHERE guild_id=$1 AND action='house.edit'")
            .bind(guild as i64)
            .fetch_one(&site.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let mut member = site.browser();
    let login = Request::post("/auth/dev-login").header(header::CONTENT_TYPE,"application/json")
        .header(header::ORIGIN,PUBLIC_URL).header("sec-fetch-site","same-origin")
        .body(Body::from(json!({"user_id":unique().to_string(),"username":"member","guilds":[{"id":guild.to_string(),"name":"Plaza","icon":null,"owner":false,"permissions":"0"}]}).to_string())).unwrap();
    assert_eq!(member.send(login).await.status, StatusCode::OK);
    assert_eq!(
        member
            .get(&format!("/api/house/rooms/0?scope=plaza&guild={guild}"))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        member
            .op(
                &format!("/api/house/rooms/0/ops?scope=plaza&guild={guild}"),
                result.json()["fingerprint"].as_str().unwrap(),
                json!({"type":"clear"}),
                true
            )
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        member
            .get(&format!("/api/house/inventory?scope=plaza&guild={guild}"))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(site.app.db.placements(guild, 0).await.unwrap().len(), 1);
    sqlx::query("INSERT INTO guild_modules (guild_id,module,enabled) VALUES ($1,'plaza',false)")
        .bind(guild as i64)
        .execute(&site.pool)
        .await
        .unwrap();
    assert_eq!(
        browser
            .get(&format!("/api/house?scope=plaza&guild={guild}"))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn surface_buy_is_idempotent_under_concurrent_first_use() {
    let Some(site) = Site::new("house_surface_race").await else {
        return;
    };
    let user = unique();
    site.app.db.user(user).await.unwrap();
    sqlx::query("UPDATE users SET credits=1000 WHERE id=$1")
        .bind(user as i64)
        .execute(&site.pool)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        site.app
            .db
            .buy_item(user, "wp:race-test", 100, pinchora_db::Currency::Shells, 1),
        site.app
            .db
            .buy_item(user, "wp:race-test", 100, pinchora_db::Currency::Shells, 1)
    );
    assert!(matches!(a.unwrap(), pinchora_db::BuyOutcome::Bought { .. }));
    assert!(matches!(b.unwrap(), pinchora_db::BuyOutcome::Bought { .. }));
    assert_eq!(site.app.db.user(user).await.unwrap().credits, 900);
    assert_eq!(
        site.app
            .db
            .inventory(user)
            .await
            .unwrap()
            .iter()
            .find(|r| r.item_id == "wp:race-test")
            .unwrap()
            .quantity,
        1
    );
}

#[tokio::test]
async fn invalid_coordinates_and_old_partner_bonds_cannot_write() {
    let Some(site) = Site::new("house_ops_invalid").await else {
        return;
    };
    let (a, b) = (unique(), unique());
    site.move_in(a, 1, &[]).await;
    site.move_in(b, 1, &[]).await;
    site.app.db.grant_item(a, CHAIR, 2).await.unwrap();
    site.app.db.pair_partners(a, b).await.unwrap();
    let pair = site.app.db.partner(a).await.unwrap().unwrap();
    let who = pinchora_commands::Who::partner_of(a, pair);
    let expected = pinchora_commands::house_edit::fingerprint(&[]);
    let op = |x| pinchora_commands::house_edit::Operation::Place {
        item_id: CHAIR.into(),
        x,
        y: 0,
        flipped: false,
    };
    let negative = pinchora_commands::house_edit::apply(&site.app, who, 0, (8, 8), &expected, op(-1))
        .await
        .unwrap();
    assert!(matches!(
        negative,
        pinchora_commands::house_edit::Outcome::Refused { .. }
    ));
    site.app.db.leave_partner(a).await.unwrap();
    site.app.db.pair_partners(a, b).await.unwrap();
    let stale = pinchora_commands::house_edit::apply(&site.app, who, 0, (8, 8), &expected, op(0))
        .await
        .unwrap();
    assert!(matches!(stale, pinchora_commands::house_edit::Outcome::Refused { .. }));
    assert!(site.app.db.placements(b, 0).await.unwrap().is_empty());
}

#[tokio::test]
async fn partner_cannot_move_or_transfer_owners_furniture() {
    let Some(site) = Site::new("partner_move_regression").await else {
        return;
    };
    let (guest, owner) = (unique(), unique());
    site.move_in(guest, 1, &[]).await;
    site.move_in(owner, 1, &[(CHAIR, 0, 0, false)]).await;
    site.app.db.pair_partners(guest, owner).await.unwrap();
    site.app.db.grant_item(guest, CHAIR, 1).await.unwrap();
    let mut browser = site.browser();
    browser.login(guest).await;
    let room = browser.get("/api/house/rooms/0?scope=partner").await.json();
    let response = browser
        .op(
            "/api/house/rooms/0/ops?scope=partner",
            room["fingerprint"].as_str().unwrap(),
            json!({"type":"move","id":room["placements"][0]["id"],"x":3,"y":2,"flipped":false}),
            true,
        )
        .await;
    assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response.json()["code"], "owner_only");
    assert!(site.app.db.owns(guest, CHAIR).await.unwrap());
    assert_eq!(site.app.db.placements(owner, 0).await.unwrap()[0].x, 0);
}

#[tokio::test]
async fn late_history_save_cannot_erase_or_resurrect_newer_history() {
    use pinchora_commands::{Who, undo};
    let Some(site) = Site::new("history_cas").await else {
        return;
    };
    let user = unique();
    site.move_in(user, 1, &[]).await;
    let who = Who::own(user);
    let mut older = undo::load(&site.app, who, 0, &[]).await;
    let mut newer = undo::load(&site.app, who, 0, &[]).await;
    older.push(undo::Step::new(undo::Kind::Store, "old"));
    newer.push(undo::Step::new(undo::Kind::Store, "new"));
    undo::save(&site.app, who, 0, &mut newer, &[]).await;
    undo::save(&site.app, who, 0, &mut older, &[]).await;
    let mut current = undo::load(&site.app, who, 0, &[]).await;
    assert_eq!(current.steps()[0].name, "new");
    let mut stale = current.clone();
    current.pop();
    undo::save(&site.app, who, 0, &mut current, &[]).await;
    stale.push(undo::Step::new(undo::Kind::Store, "resurrection"));
    undo::save(&site.app, who, 0, &mut stale, &[]).await;
    assert!(undo::load(&site.app, who, 0, &[]).await.is_empty());
    let mut writer = undo::load(&site.app, who, 0, &[]).await;
    writer.push(undo::Step::new(undo::Kind::Place, "committed"));
    let committed = [pinchora_db::PlacementRow {
        id: 77,
        room_idx: 0,
        item_id: CHAIR.into(),
        x: 0,
        y: 0,
        flipped: false,
    }];
    // A Discord editor opening between commit and save observes a different fingerprint.
    let mut observer = undo::load(&site.app, who, 0, &committed).await;
    undo::save(&site.app, who, 0, &mut observer, &committed).await;
    undo::save(&site.app, who, 0, &mut writer, &committed).await;
    assert_eq!(
        undo::load(&site.app, who, 0, &committed).await.steps()[0].name,
        "committed"
    );
}

#[tokio::test]
async fn house_editor_lock_is_shared_between_database_handles() {
    let Some(site) = Site::new("history_lock").await else {
        return;
    };
    let owner = unique();
    let guard = site.app.db.house_editor(owner).await.unwrap();
    let other = Db::connect(&std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap(), 4)
        .await
        .unwrap();
    let task = tokio::spawn(async move { other.house_editor(owner).await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(!task.is_finished(), "another pod must wait through history persistence");
    drop(guard);
    let guard = tokio::time::timeout(std::time::Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
    drop(guard);
}

#[tokio::test]
async fn settings_version_check_rolls_back_purchase_and_all_settings() {
    use pinchora_db::{Currency, HouseUpdate, HouseUpdateOutcome};
    let Some(site) = Site::new("settings_atomic").await else {
        return;
    };
    let owner = unique();
    site.move_in(owner, 1, &[(CHAIR, 0, 0, false)]).await;
    let before = site.app.db.user(owner).await.unwrap().credits;
    let surface = &Content::get().surfaces.wallpapers[1];
    let key = format!("wp:{}", surface.id);
    let changes = [
        HouseUpdate::Surface {
            wallpaper: Some(&surface.id),
            floor: None,
            key: &key,
            price: 1,
            currency: Currency::Shells,
            confirmed: true,
        },
        HouseUpdate::Upgrade {
            from_tier: 1,
            price: 1,
            rooms: &[],
        },
        HouseUpdate::Lock(true),
    ];
    for change in changes {
        let result = site
            .app
            .db
            .update_house_checked(owner, 0, change, |rows| rows.is_empty())
            .await
            .unwrap();
        assert_eq!(result, HouseUpdateOutcome::Conflict);
    }
    assert_eq!(site.app.db.user(owner).await.unwrap().credits, before);
    assert!(!site.app.db.owns(owner, &key).await.unwrap());
    let (house, rooms) = site.app.db.house_if_any(owner).await.unwrap().unwrap();
    assert_eq!(house.tier, 1);
    assert!(!house.locked);
    assert_ne!(rooms[0].wallpaper, surface.id);
}

#[tokio::test]
async fn personal_ops_are_audited_bounded_and_rate_limited() {
    let Some(site) = Site::new("ops_limits").await else {
        return;
    };
    let user = unique();
    site.move_in(user, 1, &[]).await;
    let mut browser = site.browser();
    browser.login(user).await;
    let version = pinchora_commands::house_edit::fingerprint(&[]);
    let large = browser
        .op(
            "/api/house/rooms/0/ops",
            &"x".repeat(5000),
            json!({"type":"clear"}),
            true,
        )
        .await;
    assert!(large.status.is_client_error());
    let ok = browser
        .op(
            "/api/house/rooms/0/ops",
            &version,
            json!({"type":"lock","locked":true}),
            true,
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.text());
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM dashboard_audit WHERE actor_id=$1 AND action='house.edit'")
            .bind(user as i64)
            .fetch_one(&site.pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    let recorded: serde_json::Value =
        sqlx::query_scalar("SELECT after FROM dashboard_audit WHERE actor_id=$1 AND action='house.edit'")
            .bind(user as i64)
            .fetch_one(&site.pool)
            .await
            .unwrap();
    assert_eq!(recorded["operation"], json!({"type":"lock","locked":true}));
    assert!(recorded.get("whose").is_none());
    let scope_id: i64 =
        sqlx::query_scalar("SELECT guild_id FROM dashboard_audit WHERE actor_id=$1 AND action='house.edit'")
            .bind(user as i64)
            .fetch_one(&site.pool)
            .await
            .unwrap();
    assert_eq!(scope_id, 0, "personal house ids are not guild ids");
    // Pre-fill both possible fixed windows so the regression is independent of minute boundaries.
    let window = chrono::Utc::now().timestamp() / 60;
    for minute in [window, window + 1] {
        site.app
            .cache
            .incr_by(
                &format!("web:rl:write:{user}:{minute}"),
                60,
                std::time::Duration::from_secs(120),
            )
            .await
            .unwrap();
    }
    let response = browser
        .op("/api/house/rooms/0/ops", &version, json!({"type":"clear"}), true)
        .await;
    assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(!response.header("retry-after").is_empty());
}

#[tokio::test]
async fn inventory_bounds_surfaces_and_sold_undo_never_duplicate() {
    let Some(site) = Site::new("ops_stock_bounds").await else {
        return;
    };
    let user = unique();
    site.move_in(user, 1, &[]).await;
    site.app.db.grant_item(user, CHAIR, 1).await.unwrap();
    let mut browser = site.browser();
    browser.login(user).await;
    let mut version = pinchora_commands::house_edit::fingerprint(&[]);
    for x in [-1, 8, 32767] {
        let result = browser
            .op(
                "/api/house/rooms/0/ops",
                &version,
                json!({"type":"place","item_id":CHAIR,"x":x,"y":0,"flipped":false}),
                true,
            )
            .await;
        assert_eq!(result.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(result.json()["code"], "blocked");
        assert!(site.app.db.owns(user, CHAIR).await.unwrap());
    }
    let place = json!({"type":"place","item_id":CHAIR,"x":0,"y":0,"flipped":false});
    let placed = browser.op("/api/house/rooms/0/ops", &version, place, true).await.json();
    version = placed["fingerprint"].as_str().unwrap().into();
    let duplicate = browser
        .op(
            "/api/house/rooms/0/ops",
            &version,
            json!({"type":"place","item_id":CHAIR,"x":3,"y":0,"flipped":false}),
            true,
        )
        .await;
    assert_eq!(duplicate.json()["code"], "not_owned");
    let stored = browser
        .op(
            "/api/house/rooms/0/ops",
            &version,
            json!({"type":"pickup","id":placed["placements"][0]["id"]}),
            true,
        )
        .await
        .json();
    // A sale in Discord consumes the stored item before web undo.
    sqlx::query("UPDATE inventory SET quantity=0 WHERE user_id=$1 AND item_id=$2")
        .bind(user as i64)
        .bind(CHAIR)
        .execute(&site.pool)
        .await
        .unwrap();
    let undo = browser
        .op(
            "/api/house/rooms/0/ops",
            stored["fingerprint"].as_str().unwrap(),
            json!({"type":"undo"}),
            true,
        )
        .await;
    assert_eq!(undo.json()["code"], "not_owned");
    assert!(site.app.db.placements(user, 0).await.unwrap().is_empty());
    assert_eq!(
        browser
            .op("/api/house/rooms/99/ops", &version, json!({"type":"clear"}), true)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn settings_purchase_race_charges_once_and_keeps_surface_inventory_unique() {
    use pinchora_db::{Currency, HouseUpdate, HouseUpdateOutcome};
    let Some(site) = Site::new("surface_checked_race").await else {
        return;
    };
    let owner = unique();
    site.move_in(owner, 1, &[]).await;
    let fresh = site
        .app
        .db
        .update_house_checked(
            owner,
            0,
            HouseUpdate::Surface {
                wallpaper: Some("plain_cream"),
                floor: None,
                key: "wp:plain_cream",
                price: 100,
                currency: Currency::Shells,
                confirmed: false,
            },
            |rows| rows.is_empty(),
        )
        .await
        .unwrap();
    assert_eq!(fresh, HouseUpdateOutcome::Confirm { balance: 0 });
    site.app.db.user(owner).await.unwrap();
    sqlx::query("UPDATE users SET credits=100 WHERE id=$1")
        .bind(owner as i64)
        .execute(&site.pool)
        .await
        .unwrap();
    let update = || HouseUpdate::Surface {
        wallpaper: Some("plain_cream"),
        floor: None,
        key: "wp:plain_cream",
        price: 100,
        currency: Currency::Shells,
        confirmed: true,
    };
    let (a, b) = tokio::join!(
        site.app
            .db
            .update_house_checked(owner, 0, update(), |rows| rows.is_empty()),
        site.app
            .db
            .update_house_checked(owner, 0, update(), |rows| rows.is_empty())
    );
    assert_eq!(a.unwrap(), HouseUpdateOutcome::Done);
    assert_eq!(b.unwrap(), HouseUpdateOutcome::Done);
    assert_eq!(site.app.db.user(owner).await.unwrap().credits, 0);
    assert_eq!(
        site.app
            .db
            .inventory(owner)
            .await
            .unwrap()
            .iter()
            .find(|i| i.item_id == "wp:plain_cream")
            .unwrap()
            .quantity,
        1
    );
}

#[tokio::test]
async fn surface_furniture_respects_occupied_tiles_and_stores_and_restores_as_one_step() {
    let Some(site) = Site::new("surface_support").await else {
        return;
    };
    let user = unique();
    site.move_in(user, 1, &[(CHAIR, 0, 0, false)]).await;
    let table = "crumbcorner_table__moss";
    let books = "pagepair_books__moss";
    for item in [table, books] {
        site.app.db.grant_item(user, item, 1).await.unwrap();
    }
    let mut browser = site.browser();
    browser.login(user).await;
    let mut version = pinchora_commands::house_edit::fingerprint(&site.app.db.placements(user, 0).await.unwrap());
    let op = |item| json!({"type":"place","item_id":item,"x":0,"y":0,"flipped":false});
    let rejected = browser.op("/api/house/rooms/0/ops", &version, op(books), true).await;
    assert_eq!(rejected.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        rejected.json()["message"],
        "small items need a free tile or a table/shelf top"
    );
    let cleared = browser
        .op("/api/house/rooms/0/ops", &version, json!({"type":"clear"}), true)
        .await;
    assert_eq!(cleared.status, StatusCode::OK);
    version = cleared.json()["fingerprint"].as_str().unwrap().into();
    for item in [table, books] {
        let placed = browser.op("/api/house/rooms/0/ops", &version, op(item), true).await;
        assert_eq!(placed.status, StatusCode::OK, "{}", placed.text());
        version = placed.json()["fingerprint"].as_str().unwrap().into();
    }
    let rows = site.app.db.placements(user, 0).await.unwrap();
    let table_id = rows.iter().find(|p| p.item_id == table).unwrap().id;
    let stored = browser
        .op(
            "/api/house/rooms/0/ops",
            &version,
            json!({"type":"pickup","id":table_id.to_string()}),
            true,
        )
        .await;
    assert_eq!(stored.status, StatusCode::OK, "{}", stored.text());
    assert_eq!(stored.json()["placements"], json!([]));
    let restored = browser
        .op(
            "/api/house/rooms/0/ops",
            stored.json()["fingerprint"].as_str().unwrap(),
            json!({"type":"undo"}),
            true,
        )
        .await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.text());
    assert_eq!(site.app.db.placements(user, 0).await.unwrap().len(), 2);
    assert!(!site.app.db.owns(user, table).await.unwrap());
    assert!(!site.app.db.owns(user, books).await.unwrap());
}
