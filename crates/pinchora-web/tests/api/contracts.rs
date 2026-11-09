//! Successful GET fixtures for surfaces that previously had no HTTP coverage.
use crate::*;

#[tokio::test]
async fn additional_read_contracts() {
    let Some(h) = Harness::new("read_contracts").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let content = pinchora_game::Content::get();
    h.state.app.db.user(owner).await.unwrap();
    h.state
        .app
        .db
        .set_character(owner, &pinchora_game::character::Character::starter(content).encode())
        .await
        .unwrap();
    h.state.app.db.guild_settings(guild).await.unwrap();
    h.state
        .app
        .db
        .house(
            owner,
            &pinchora_db::NewRoom {
                room_type: "living".into(),
                wallpaper: content.surfaces.wallpapers[0].id.clone(),
                floor: content.surfaces.floors[0].id.clone(),
            },
        )
        .await
        .unwrap();
    for path in [
        "/api/house/guestbook".to_owned(),
        "/api/house/surfaces".to_owned(),
        "/api/me/backgrounds".to_owned(),
        format!("/api/me/economy/pool?guild={guild}"),
        "/api/public/stats".to_owned(),
        "/api/reef/blocks".to_owned(),
        "/api/reef/doorbells".to_owned(),
        "/api/reef/preferences".to_owned(),
        "/api/reef/reports".to_owned(),
        "/api/reef/social".to_owned(),
        format!("/api/reef/houses/{owner}/guestbook"),
        format!("/api/reef/rooms/house:{owner}:0/games"),
    ] {
        let response = browser.get(&path).await;
        assert_eq!(response.status, StatusCode::OK, "{path}: {}", response.text());
    }
    // Art is a binary contract; the existing asset inventory supplies a real ID.
    let response = browser
        .get(&format!("/api/me/art/background/{}", content.backgrounds[0].id))
        .await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.header("content-type"), "image/png");
}

#[tokio::test]
async fn message_studio_race_outcomes_have_deterministic_contract_fixtures() {
    let Some(h) = Harness::new("studio_contract_outcomes").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let root = format!("/api/guilds/{guild}/message-studio");
    let embedded = json!({"name":"Event","content":"Hello","embeds":[{"title":"x"}]});
    let text = json!({"name":"Event","content":"Hello"});
    // Exercise edit-before-bind, independent of which task wins the existing concurrent test.
    assert_eq!(
        browser
            .write(Method::PUT, &format!("{root}/embedded"), embedded.clone())
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        browser
            .write(
                Method::PUT,
                &format!("{root}/events/welcome"),
                json!({"key":"embedded"})
            )
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Exercise bind-before-edit as well, retaining both accepted and refused response shapes.
    assert_eq!(
        browser.write(Method::PUT, &format!("{root}/text"), text).await.status,
        StatusCode::OK
    );
    assert_eq!(
        browser
            .write(Method::PUT, &format!("{root}/events/welcome"), json!({"key":"text"}))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        browser
            .write(Method::PUT, &format!("{root}/text"), embedded)
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn wishlist_delete_contract() {
    let Some(h) = Harness::new("wishlist_delete_contract").await else {
        return;
    };
    let mut browser = h.browser();
    browser.login(&h, unique(), vec![]).await;
    let entry = &pinchora_game::collection::Collection::get().critters[0].id;
    let saved = browser
        .write(
            Method::PUT,
            "/api/me/collection/wishlist",
            json!({"entry_id":entry,"ping_on_spawn":true}),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let removed = browser
        .write(
            Method::DELETE,
            &format!("/api/me/collection/wishlist/{entry}"),
            json!({}),
        )
        .await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(removed.json(), json!({"ok":true}));
    assert_eq!(
        browser.get("/api/me/collection/wishlist").await.json()["items"],
        json!([])
    );
}
