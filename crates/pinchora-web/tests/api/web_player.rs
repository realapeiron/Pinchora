//! W5: ordinary members can edit themselves; previews and purchases use the bot's rules.
use super::*;
use pinchora_game::{
    BadgeKind, Content,
    character::{Character, Shelf},
};

#[tokio::test]
async fn player_profile_session_csrf_validation_and_audit_without_managed_guilds() {
    let Some(h) = Harness::new("player_profile_boundaries").await else {
        return;
    };
    let id = unique();
    let mut browser = h.browser();
    assert_eq!(browser.get("/api/me/profile").await.status, StatusCode::UNAUTHORIZED);
    browser.login(&h, id, vec![]).await;
    let profile = browser.get("/api/me/profile").await;
    assert_eq!(profile.status, StatusCode::OK, "{}", profile.text());
    let mut draft = profile.json()["profile"].clone();
    draft["bio"] = json!("My global reef");
    let token = browser.csrf.take();
    assert_eq!(
        browser
            .write(Method::PUT, "/api/me/profile", draft.clone())
            .await
            .code(),
        "csrf"
    );
    browser.csrf = token;
    let mut forged = draft.clone();
    forged["credits"] = json!(999999);
    assert_eq!(
        browser.write(Method::PUT, "/api/me/profile", forged).await.status,
        StatusCode::BAD_REQUEST
    );
    let mut forged = draft.clone();
    forged["showcase_pet"] = json!(unique().to_string());
    assert_eq!(
        browser.write(Method::PUT, "/api/me/profile", forged).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let saved = browser.write(Method::PUT, "/api/me/profile", draft.clone()).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(h.state.app.db.user(id).await.unwrap().bio, "My global reef");
    let audit = h.state.app.db.audit_page(0, None, 100).await.unwrap();
    let event = audit
        .iter()
        .find(|a| a.actor_id == id as i64 && a.action == "player.profile")
        .unwrap();
    let after: Value = serde_json::from_str(event.after.as_deref().unwrap()).unwrap();
    draft["bio"] = json!({"characters": 14});
    draft["showcase_pet"] = json!(false);
    assert_eq!(after, draft);
    for language in [Lang::En, Lang::Tr] {
        h.state.app.set_user_lang(id, Some(language)).await.unwrap();
        let refusal = browser
            .write(
                Method::PUT,
                "/api/me/style",
                json!({"accent":"#ff6b4a","panel_opacity":1,"frame":"gold"}),
            )
            .await;
        assert_eq!(refusal.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            refusal.json()["error"]["message"],
            pinchora_core::t!(
                language,
                "customize.style.frame-refused",
                frame = pinchora_core::t!(language, "customize.style.frame.gold"),
                at = 30,
                level = 0
            )
        );
    }
    assert_eq!(h.state.app.db.user(id).await.unwrap().frame, "plain");
    let mut other = h.browser();
    other.login(&h, unique(), vec![]).await;
    assert_ne!(
        other.get("/api/me/profile").await.json()["profile"]["bio"],
        json!("My global reef")
    );
    assert_eq!(browser.get("/api/me/home").await.status, StatusCode::OK);
    assert_eq!(browser.get("/api/me/wallet").await.status, StatusCode::OK);
}

#[tokio::test]
async fn player_badges_order_locks_auto_and_shop_purchase_charge_once() {
    let Some(h) = Harness::new("player_badge_shop").await else {
        return;
    };
    let id = unique();
    let mut b = h.browser();
    b.login(&h, id, vec![]).await;
    h.state.app.db.user(id).await.unwrap();
    let c = Content::get();
    let shop: Vec<_> = c
        .badges
        .iter()
        .filter_map(|b| match b.kind {
            BadgeKind::Shop { price } => Some((b.id.clone(), price)),
            _ => None,
        })
        .take(2)
        .collect();
    sqlx::query("UPDATE users SET credits=100000 WHERE id=$1")
        .bind(id as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    let path = format!("/api/me/badges/{}/buy", shop[0].0);
    let (web, bot) = tokio::join!(
        b.write(Method::POST, &path, json!({})),
        h.state.app.db.buy_badge(id, &shop[0].0, shop[0].1)
    );
    assert_eq!(web.status, StatusCode::OK, "{}", web.text());
    bot.unwrap();
    assert_eq!(h.state.app.db.user(id).await.unwrap().credits, 100000 - shop[0].1);
    let ledger = h.state.app.db.player_ledger(id).await.unwrap();
    assert_eq!(ledger.len(), 1);
    h.state.app.db.buy_badge(id, &shop[1].0, shop[1].1).await.unwrap();
    let order = vec![shop[1].0.clone(), shop[0].0.clone()];
    assert_eq!(
        b.write(Method::PUT, "/api/me/badges", json!({"ids":order}))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(h.state.app.db.user(id).await.unwrap().badges, order);
    for ids in [
        vec![shop[0].0.clone(); 7],
        vec![shop[0].0.clone(); 2],
        vec!["not-a-badge".into()],
    ] {
        assert_eq!(
            b.write(Method::PUT, "/api/me/badges", json!({"ids":ids})).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let badges = b.get("/api/me/badges").await.json();
    let locked = badges["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["earned"] == false && v["target"].is_number())
        .unwrap();
    assert!(locked["description"].as_str().unwrap().len() > 3);
    assert_eq!(
        b.write(Method::PUT, "/api/me/badges", json!({"ids":[locked["id"]]}))
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        b.write(Method::DELETE, "/api/me/badges", json!({})).await.status,
        StatusCode::OK
    );
    assert!(!h.state.app.db.user(id).await.unwrap().badges_set);
    let bg = &c.backgrounds[0];
    assert_eq!(
        b.write(Method::PUT, "/api/me/background", json!({"id":bg.id}))
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        b.write(Method::POST, &format!("/api/me/backgrounds/{}/buy", bg.id), json!({}))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        b.write(Method::PUT, "/api/me/background", json!({"id":bg.id}))
            .await
            .status,
        StatusCode::OK
    );
    b.write(Method::POST, "/api/me/badges/seen", json!({})).await;
    assert!(
        h.state
            .app
            .db
            .user(id)
            .await
            .unwrap()
            .badges_seen
            .unwrap()
            .contains(&shop[0].0)
    );
}

#[tokio::test]
async fn player_profile_preview_is_shared_assembler_bytes_and_does_not_save() {
    let Some(h) = Harness::new("player_preview_parity").await else {
        return;
    };
    let id = unique();
    let mut b = h.browser();
    let me = b.login(&h, id, vec![]).await;
    // Keep ordinary parallel fixtures earning XP from changing the live rank between renders.
    h.state.app.db.user(id).await.unwrap();
    sqlx::query("UPDATE users SET xp=1000000000 WHERE id=$1")
        .bind(id as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    let user: twilight_model::user::User = serde_json::from_value(user_json(id, &format!("user{id}"))).unwrap();
    let name = me["user"]["global_name"].as_str().unwrap_or(&user.name).to_owned();
    for l in [Lang::En, Lang::Tr] {
        h.state.app.set_user_lang(id, Some(l)).await.unwrap();
        let response = b.get("/api/me/preview/profile-card.png").await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.text());
        let expected = pinchora_commands::profile_card_for(
            &h.state.app,
            &user,
            name.clone(),
            None,
            pinchora_commands::ProfileContext {
                guild: None,
                viewer: id,
                lang: l,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.body.as_ref(), expected);
        save_png("player_preview_parity", l.code(), &response.body);
    }
    let before = h.state.app.db.user(id).await.unwrap();
    let preview = b
        .get("/api/me/preview/profile-card.png?accent=%2306d6a0&background=not-real")
        .await;
    assert_eq!(preview.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(h.state.app.db.user(id).await.unwrap().accent, before.accent);
    for l in [Lang::En, Lang::Tr] {
        let wallet = b
            .get(&format!("/api/me/preview/wallet-card.png?lang={}", l.code()))
            .await;
        assert_eq!(wallet.status, StatusCode::OK);
        let expected = pinchora_commands::wallet_card_for(&h.state.app, &user, name.clone(), l)
            .await
            .unwrap();
        assert_eq!(
            wallet.body.as_ref(),
            expected,
            "explicit web locale overrides the saved locale"
        );
    }
}

#[tokio::test]
async fn player_character_drafts_ownership_outfits_randomise_and_png() {
    let Some(h) = Harness::new("player_character").await else {
        return;
    };
    let id = unique();
    let mut b = h.browser();
    b.login(&h, id, vec![]).await;
    let initial = b.get("/api/me/character").await;
    assert_eq!(initial.status, StatusCode::OK, "{}", initial.text());
    let original = h.state.app.db.character(id).await.unwrap();
    let random = b.write(Method::POST, "/api/me/character/randomise", json!({})).await;
    assert_eq!(random.status, StatusCode::OK, "{}", random.text());
    assert_eq!(
        h.state.app.db.character(id).await.unwrap(),
        original,
        "randomise is a draft"
    );
    let spec = random.json()["spec"].as_str().unwrap().to_owned();
    let c = Character::decode(Content::get(), &spec).unwrap();
    let shelf = Shelf::new(Content::get(), &h.state.app.db.wardrobe_facts(id).await.unwrap());
    assert!(shelf.blocker(&c).is_none());
    let mut url = Url::parse("http://dash.test/api/me/character/preview.png").unwrap();
    url.query_pairs_mut().append_pair("spec", &spec);
    let preview = b.get(&format!("{}?{}", url.path(), url.query().unwrap())).await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    save_png("player_character", "draft", &preview.body);
    assert_eq!(
        b.write(Method::PUT, "/api/me/character", json!({"spec":spec}))
            .await
            .status,
        StatusCode::OK
    );
    let outfit = json!({"name":"Reef day","spec":spec});
    assert_eq!(
        b.write(Method::PUT, "/api/me/character/outfits/1", outfit.clone())
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        b.write(Method::PUT, "/api/me/character/outfits/4", outfit).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        b.write(Method::POST, "/api/me/character/outfits/1/wear", json!({}))
            .await
            .status,
        StatusCode::OK
    );
    let locked = pinchora_game::character::accessories()
        .iter()
        .find(|a| shelf.access(a) != pinchora_game::character::Access::Owned)
        .unwrap();
    let mut forged = c.clone();
    forged.wear(locked.slot, Some(&locked.id), None);
    assert_eq!(
        b.write(Method::PUT, "/api/me/character", json!({"spec":forged.encode()}))
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        b.write(Method::DELETE, "/api/me/character/outfits/1", json!({}))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert!(h.state.app.db.outfits(id).await.unwrap().is_empty());
}

/// Deterministic funded player for the real Playwright purchase flow, only on the test database.
#[tokio::test]
#[ignore]
async fn seed_web_player() {
    let Some(h) = Harness::new("seed_web_player").await else {
        return;
    };
    let id = 770_000_000_000_000_051;
    h.state.app.db.delete_user_data(id, chrono::Utc::now()).await.unwrap();
    h.state.app.db.user(id).await.unwrap();
    sqlx::query("UPDATE users SET credits=100000, xp=10000 WHERE id=$1")
        .bind(id as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    for (badge, price) in Content::get()
        .badges
        .iter()
        .filter_map(|b| match b.kind {
            BadgeKind::Shop { price } => Some((b, price)),
            _ => None,
        })
        .take(2)
    {
        h.state.app.db.buy_badge(id, &badge.id, price).await.unwrap();
    }
}
