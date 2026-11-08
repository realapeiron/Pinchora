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
