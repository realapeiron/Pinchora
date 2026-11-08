use super::*;

#[tokio::test]
async fn player_depth_session_csrf_and_shared_economy_transactions() {
    let Some(h) = Harness::new("player_depth_economy").await else {
        return;
    };
    let user = unique();
    let friend = unique();
    let mut b = h.browser();
    assert_eq!(b.get("/api/me/inventory").await.status, StatusCode::UNAUTHORIZED);
    b.login(&h, user, vec![]).await;
    let item = &pinchora_game::Content::get().furniture[0].id;
    h.state.app.db.user(user).await.unwrap();
    h.state.app.db.user(friend).await.unwrap();
    h.state.app.db.grant_item(user, item, 2).await.unwrap();
    let page = b.get("/api/me/inventory").await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.text());
    assert!(
        page.json()["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == *item)
    );
    let filtered = b.get("/api/me/inventory?q=&category=&rarity=&set=&page=1").await;
    assert_eq!(filtered.status, StatusCode::OK, "{}", filtered.text());
    assert!(
        filtered.json()["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == *item)
    );
    assert!(page.header("content-security-policy").starts_with("default-src 'self'"));
    assert!(
        page.header("content-security-policy")
            .contains("connect-src 'self' ws://dash.test;")
    );
    assert!(page.header("content-security-policy").contains("worker-src 'self';"));
    let token = b.csrf.take();
    assert_eq!(
        b.write(
            Method::POST,
            "/api/me/inventory/gift",
            json!({"to_user_id":friend.to_string(),"item_id":item})
        )
        .await
        .code(),
        "csrf"
    );
    b.csrf = token;
    let gifted = b
        .write(
            Method::POST,
            "/api/me/inventory/gift",
            json!({"to_user_id":friend.to_string(),"item_id":item}),
        )
        .await;
    assert_eq!(gifted.status, StatusCode::OK, "{}", gifted.text());
    assert!(h.state.app.db.owns(friend, item).await.unwrap());
    let listed = b
        .write(
            Method::POST,
            "/api/me/stall/listings",
            json!({"item_id":item,"price":200}),
        )
        .await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    assert!(!h.state.app.db.owns(user, item).await.unwrap());
    let listing = b.get("/api/me/stall").await.json()["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let unlisted = b
        .write(Method::DELETE, &format!("/api/me/stall/listings/{listing}"), json!({}))
        .await;
    assert_eq!(unlisted.status, StatusCode::OK, "{}", unlisted.text());
    assert!(h.state.app.db.owns(user, item).await.unwrap());
}

#[tokio::test]
async fn web_pet_care_uses_discord_cooldown_key_and_game_xp() {
    let Some(h) = Harness::new("player_depth_pets").await else {
        return;
    };
    let user = unique();
    let mut b = h.browser();
    b.login(&h, user, vec![]).await;
    let pet = h.state.app.db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap();
    let path = format!("/api/me/pets/{}/care", pet.id);
    let first = b.write(Method::POST, &path, json!({"action":"play"})).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    assert_eq!(h.state.app.db.pets(user).await.unwrap()[0].xp, 15);
    // The bot's `pc:play` handler reads exactly this key; a web click must occupy it.
    let key = format!("pet:{}:Play", pet.id);
    assert!(
        h.state
            .app
            .cache
            .try_cooldown(&key, Duration::from_secs(20 * 60))
            .await
            .unwrap()
            .is_some()
    );
    let second = b.write(Method::POST, &path, json!({"action":"play"})).await;
    assert_eq!(second.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(h.state.app.db.pets(user).await.unwrap()[0].xp, 15);
    let mut other = h.browser();
    other.login(&h, unique(), vec![]).await;
    assert_eq!(
        other.write(Method::POST, &path, json!({"action":"feed"})).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(b.get("/api/me/pets").await.status, StatusCode::OK);
    assert_eq!(
        b.get(&format!("/api/me/pets/{}/card.png", pet.id)).await.status,
        StatusCode::OK
    );
    let trip = b
        .write(
            Method::POST,
            &format!("/api/me/pets/{}/trip", pet.id),
            json!({"dest":"tidepool_meadow","hours":1}),
        )
        .await;
    assert_eq!(trip.status, StatusCode::OK, "{}", trip.text());
    assert_eq!(
        b.write(Method::POST, &path, json!({"action":"feed"})).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let ended = chrono::Utc::now() - chrono::Duration::minutes(1);
    sqlx::query("UPDATE pets SET trip_ends_at=$2, needs_at=$2 WHERE id=$1")
        .bind(pet.id)
        .bind(ended)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    let claimed = b.write(Method::POST, "/api/me/pets/claim", json!({})).await;
    assert_eq!(claimed.status, StatusCode::OK, "{}", claimed.text());
    assert_eq!(claimed.json()["items"].as_array().unwrap().len(), 1);
    assert!(
        b.write(Method::POST, "/api/me/pets/claim", json!({})).await.json()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let slept = chrono::Utc::now() - chrono::Duration::days(2);
    sqlx::query("UPDATE pets SET hunger=0,fun=0,hygiene=0,needs_at=$2 WHERE id=$1")
        .bind(pet.id)
        .bind(slept)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    let woke = b
        .write(Method::POST, &format!("/api/me/pets/{}/wake", pet.id), json!({}))
        .await;
    assert_eq!(woke.status, StatusCode::OK, "{}", woke.text());
    assert_eq!(woke.json()["free"], true);
}

#[tokio::test]
async fn player_collections_and_social_pages_are_session_scoped() {
    let Some(h) = Harness::new("player_depth_social").await else {
        return;
    };
    let user = unique();
    let friend = unique();
    let mut b = h.browser();
    for path in [
        "/api/me/fish-book",
        "/api/me/quests",
        "/api/me/guestbook",
        "/api/me/friends",
        "/api/me/partner",
    ] {
        assert_eq!(b.get(path).await.status, StatusCode::UNAUTHORIZED);
    }
    b.login(&h, user, vec![]).await;
    h.state.app.db.user(friend).await.unwrap();
    h.state.app.db.befriend(user, friend).await.unwrap();
    assert_eq!(b.get("/api/me/fish-book").await.status, StatusCode::OK);
    assert_eq!(
        b.get("/api/me/quests").await.json()["items"].as_array().unwrap().len(),
        3
    );
    assert_eq!(
        b.get("/api/me/friends").await.json()["items"][0]["id"],
        friend.to_string()
    );
    let path = format!("/api/me/guestbook/blocks/{friend}");
    let token = b.csrf.take();
    assert_eq!(b.write(Method::POST, &path, json!({})).await.code(), "csrf");
    b.csrf = token;
    assert_eq!(b.write(Method::POST, &path, json!({})).await.status, StatusCode::OK);
    assert_eq!(
        b.get("/api/me/guestbook").await.json()["blocks"][0]["id"],
        friend.to_string()
    );
    assert_eq!(b.write(Method::DELETE, &path, json!({})).await.status, StatusCode::OK);
    assert!(
        b.get("/api/me/guestbook").await.json()["blocks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let audit = h.state.app.db.audit_page(0, None, 100).await.unwrap();
    assert!(
        audit
            .iter()
            .any(|r| r.actor_id == user as i64 && r.action == "player.guestbook.block")
    );
}

#[tokio::test]
async fn private_friends_expose_only_relationship_identity() {
    let Some(h) = Harness::new("depth_private_friends").await else {
        return;
    };
    let (user, friend) = (unique(), unique());
    let mut b = h.browser();
    b.login(&h, user, vec![]).await;
    h.state.app.db.befriend(user, friend).await.unwrap();
    for setting in [
        pinchora_db::Hide::Profile,
        pinchora_db::Hide::House,
        pinchora_db::Hide::Leaderboard,
    ] {
        h.state.app.db.set_privacy(friend, setting, true).await.unwrap();
    }
    let response = b.get("/api/me/friends").await;
    assert_eq!(response.status, StatusCode::OK);
    let body = response.json();
    let row = body["items"][0].as_object().unwrap();
    assert_eq!(row.len(), 3);
    assert_eq!(row["id"], friend.to_string());
    assert!(row.contains_key("name") && row.contains_key("since"));
    let mut other = h.browser();
    other.login(&h, unique(), vec![]).await;
    assert_eq!(
        other.get(&format!("/api/me/friends?user_id={user}")).await.json()["total"],
        0
    );
}

#[tokio::test]
async fn failed_feed_releases_cooldown_without_charging_or_changing_pet() {
    let Some(h) = Harness::new("depth_feed_retry").await else {
        return;
    };
    let user = unique();
    let mut b = h.browser();
    b.login(&h, user, vec![]).await;
    let pet = h.state.app.db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap();
    let path = format!("/api/me/pets/{}/care", pet.id);
    assert_eq!(
        b.write(Method::POST, &path, json!({"action":"feed"})).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(h.state.app.db.pets(user).await.unwrap()[0].xp, 0);
    h.state.app.db.earn_shells(user, 100, "test").await.unwrap();
    let fed = b.write(Method::POST, &path, json!({"action":"feed"})).await;
    assert_eq!(fed.status, StatusCode::OK, "{}", fed.text());
    assert_eq!(
        h.state.app.db.user(user).await.unwrap().credits,
        100 - pinchora_game::pets::FOOD_PRICE
    );
    assert_eq!(
        h.state.app.db.pets(user).await.unwrap()[0].xp,
        pinchora_game::pets::Care::Feed.xp()
    );
}

#[tokio::test]
async fn depth_writes_require_session_csrf_and_rate_limit_before_mutation() {
    let Some(h) = Harness::new("depth_write_guards").await else {
        return;
    };
    let user = unique();
    let mut b = h.browser();
    let writes = [
        (Method::POST, "/api/me/inventory/gift"),
        (Method::POST, "/api/me/stall/listings"),
        (Method::DELETE, "/api/me/stall/listings/1"),
        (Method::POST, "/api/me/pets/1/care"),
        (Method::POST, "/api/me/pets/1/active"),
        (Method::POST, "/api/me/pets/1/trip"),
        (Method::POST, "/api/me/pets/claim"),
        (Method::POST, "/api/me/pets/1/wake"),
        (Method::PATCH, "/api/me/pets/1"),
        (Method::DELETE, "/api/me/guestbook/1"),
        (Method::POST, "/api/me/guestbook/blocks/1"),
        (Method::DELETE, "/api/me/guestbook/blocks/1"),
    ];
    for (method, path) in &writes {
        assert_eq!(
            b.write(method.clone(), path, json!({})).await.status,
            StatusCode::UNAUTHORIZED,
            "{path}"
        );
    }
    b.login(&h, user, vec![]).await;
    let csrf = b.csrf.take();
    for (method, path) in &writes {
        assert_eq!(b.write(method.clone(), path, json!({})).await.code(), "csrf", "{path}");
    }
    b.csrf = csrf;
    for _ in 0..60 {
        assert_eq!(
            b.write(Method::POST, "/api/me/pets/claim", json!({})).await.status,
            StatusCode::OK
        );
    }
    assert_eq!(
        b.write(Method::POST, "/api/me/pets/claim", json!({})).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
    // These operations intentionally remain Discord-only; arbitrary payloads cannot create them.
    for path in ["/api/me/friends", "/api/me/guestbook"] {
        assert_eq!(
            b.write(Method::POST, path, json!({"user_id": user.to_string(), "body":"spam"}))
                .await
                .status,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
}

#[tokio::test]
async fn concurrent_web_listings_and_discord_purchases_conserve_items_and_shells() {
    let Some(h) = Harness::new("depth_stall_races").await else {
        return;
    };
    let (seller, buyer, rival) = (unique(), unique(), unique());
    let mut a = h.browser();
    let mut b = h.browser();
    a.login(&h, seller, vec![]).await;
    b.login(&h, seller, vec![]).await;
    let item = &pinchora_game::Content::get().furniture[0].id;
    h.state.app.db.grant_item(seller, item, 1).await.unwrap();
    let (one, two) = tokio::join!(
        a.write(
            Method::POST,
            "/api/me/stall/listings",
            json!({"item_id":item,"price":100})
        ),
        b.write(
            Method::POST,
            "/api/me/stall/listings",
            json!({"item_id":item,"price":100})
        ),
    );
    assert_eq!(
        [one.status, two.status]
            .iter()
            .filter(|s| **s == StatusCode::OK)
            .count(),
        1
    );
    let listings = h.state.app.db.listings(seller).await.unwrap();
    assert_eq!(listings.len(), 1);
    let listing = listings[0].id;
    for who in [buyer, rival] {
        h.state.app.db.earn_shells(who, 100, "test").await.unwrap();
    }
    let (one, two) = tokio::join!(
        h.state.app.db.buy_listing(buyer, listing),
        h.state.app.db.buy_listing(rival, listing)
    );
    assert_eq!(
        [one.unwrap(), two.unwrap()]
            .iter()
            .filter(|v| matches!(v, pinchora_db::StallBuy::Bought { .. }))
            .count(),
        1
    );
    assert_eq!(h.state.app.db.user(seller).await.unwrap().credits, 95);
    assert_eq!(
        h.state.app.db.user(buyer).await.unwrap().credits + h.state.app.db.user(rival).await.unwrap().credits,
        100
    );
    assert_ne!(
        h.state.app.db.owns(buyer, item).await.unwrap(),
        h.state.app.db.owns(rival, item).await.unwrap()
    );
    assert_eq!(
        a.write(Method::DELETE, &format!("/api/me/stall/listings/{listing}"), json!({}))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert!(!h.state.app.db.owns(seller, item).await.unwrap());
}

#[tokio::test]
async fn active_sessions_survive_old_last_seen_but_stop_at_absolute_lifetime() {
    let Some(h) = Harness::new("depth_session_lifetime").await else {
        return;
    };
    let mut b = h.browser();
    b.login(&h, unique(), vec![]).await;
    let key = format!("web:sess:{}", b.cookies["pinchora_sid"]);
    let mut session: Value = serde_json::from_str(&h.state.app.cache.get(&key).await.unwrap().unwrap()).unwrap();
    let now = chrono::Utc::now().timestamp();
    session["created_at"] = json!(now + 2);
    h.state
        .app
        .cache
        .put(&key, &session.to_string(), Duration::from_secs(600))
        .await
        .unwrap();
    assert_eq!(b.get("/api/me/pets").await.status, StatusCode::OK);
    session["created_at"] = json!(now - 29 * 86_400);
    session["last_seen_at"] = session["created_at"].clone();
    h.state
        .app
        .cache
        .put(&key, &session.to_string(), Duration::from_secs(600))
        .await
        .unwrap();
    assert_eq!(b.get("/api/me/pets").await.status, StatusCode::OK);
    session["created_at"] = json!(now - 30 * 86_400);
    session["last_seen_at"] = json!(now);
    h.state
        .app
        .cache
        .put(&key, &session.to_string(), Duration::from_secs(600))
        .await
        .unwrap();
    assert_eq!(b.get("/api/me/pets").await.status, StatusCode::UNAUTHORIZED);
    assert!(h.state.app.cache.get(&key).await.unwrap().is_none());
}

#[tokio::test]
async fn actual_web_care_blocks_the_discord_button_in_both_languages() {
    for (label, language, refusal) in [
        ("depth_actual_interleave_en", Lang::En, "break"),
        ("depth_actual_interleave_tr", Lang::Tr, "dinlenmeli"),
    ] {
        let Some(h) = Harness::new(label).await else { return };
        let user = unique();
        let mut b = h.browser();
        b.login(&h, user, vec![]).await;
        h.state.app.db.set_user_language(user, Some(language)).await.unwrap();
        let pet = h.state.app.db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap();
        let response = b
            .write(
                Method::POST,
                &format!("/api/me/pets/{}/care", pet.id),
                json!({"action":"play"}),
            )
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.text());
        let interaction = serde_json::from_value(json!({
            "id":unique().to_string(), "application_id":APPLICATION_ID.to_string(),
            "type":3, "token":"depth-interleave", "version":1, "locale":language.code(),
            "channel":{"id":unique().to_string(),"type":1},
            "user":user_json(user,"Pip's owner"),
            "authorizing_integration_owners":{}, "entitlements":[], "attachment_size_limit":26214400,
            "data":{"custom_id":format!("pc:play:{}",pet.id),"component_type":2}
        }))
        .unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let responder = Arc::new(pinchora_commands::Responder::http(
            h.state.app.clone(),
            &interaction,
            tx,
        ));
        pinchora_commands::handle_interaction(h.state.app.clone(), interaction, responder).await;
        let response = rx.await.unwrap();
        let content = response.data.unwrap().content.unwrap_or_default();
        assert!(content.contains(refusal), "{content}");
        assert_eq!(h.state.app.db.pets(user).await.unwrap()[0].xp, 15);
    }
}
