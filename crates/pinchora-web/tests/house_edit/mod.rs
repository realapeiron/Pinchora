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
