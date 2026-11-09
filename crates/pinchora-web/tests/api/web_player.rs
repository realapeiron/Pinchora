//! ordinary members can edit themselves; previews and purchases use the bot's rules.
use crate::*;
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
                preview: false,
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

#[tokio::test]
async fn player_all_writes_require_session_and_csrf_and_seen_is_audited() {
    let Some(h) = Harness::new("player_all_writes").await else {
        return;
    };
    let id = unique();
    let mut anon = h.browser();
    let mut b = h.browser();
    b.login(&h, id, vec![]).await;
    let token = b.csrf.take();
    for (method, path) in [
        (Method::PUT, "/profile"),
        (Method::PATCH, "/profile"),
        (Method::PUT, "/badges"),
        (Method::DELETE, "/badges"),
        (Method::POST, "/badges/seen"),
        (Method::PUT, "/style"),
        (Method::DELETE, "/style"),
        (Method::PUT, "/background"),
        (Method::PUT, "/title"),
        (Method::PUT, "/showcase"),
        (Method::POST, "/badges/example/buy"),
        (Method::POST, "/backgrounds/example/buy"),
        (Method::PUT, "/character"),
        (Method::POST, "/character/randomise"),
        (Method::POST, "/character/accessories/example/buy"),
        (Method::PUT, "/character/outfits/1"),
        (Method::DELETE, "/character/outfits/1"),
        (Method::POST, "/character/outfits/1/wear"),
    ] {
        let path = format!("/api/me{path}");
        assert_eq!(
            anon.write(method.clone(), &path, json!({})).await.status,
            StatusCode::UNAUTHORIZED,
            "{path}"
        );
        assert_eq!(b.write(method, &path, json!({})).await.code(), "csrf", "{path}");
    }
    b.csrf = token;
    h.state.app.db.user(id).await.unwrap();
    sqlx::query("UPDATE users SET badges_seen=NULL WHERE id=$1")
        .bind(id as i64)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    assert_eq!(
        b.write(Method::PATCH, "/api/me/profile", json!({"bio":"x".repeat(16 * 1024)}))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    for _ in 0..2 {
        assert_eq!(
            b.write(Method::POST, "/api/me/badges/seen", json!({})).await.status,
            StatusCode::NO_CONTENT
        );
    }
    let events = h.state.app.db.audit_page(0, None, 100).await.unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|a| a.actor_id == id as i64 && a.action == "player.badges.seen")
            .count(),
        1
    );
}

#[tokio::test]
async fn player_preview_bounds_cache_privacy_and_shared_rate_limit() {
    let Some(h) = Harness::new("player_preview_abuse").await else {
        return;
    };
    let id = unique();
    let mut b = h.browser();
    b.login(&h, id, vec![]).await;
    for path in [
        "/api/me/preview/profile-card.png?badges=",
        "/api/me/character/preview.png?spec=",
        "/api/me/preview/wallet-card.png?lang=",
    ] {
        let response = b.get(&format!("{path}{}", "a".repeat(16 * 1024))).await;
        assert_eq!(response.status, StatusCode::URI_TOO_LONG);
        assert!(response.header("cache-control").contains("no-store"));
    }
    assert_eq!(
        b.get("/api/me/preview/profile-card.png?user_id=1").await.status,
        StatusCode::BAD_REQUEST
    );
    h.state.app.db.user(id).await.unwrap();
    h.state
        .app
        .db
        .set_privacy(id, pinchora_db::Hide::Profile, true)
        .await
        .unwrap();
    let png = b.get("/api/me/preview/profile-card.png").await;
    assert_eq!(png.status, StatusCode::OK);
    assert_eq!(png.header("cache-control"), "private, no-store");
    let user = serde_json::from_value(user_json(id, "Private")).unwrap();
    assert!(
        pinchora_commands::profile_card_for(
            &h.state.app,
            &user,
            "Private".into(),
            None,
            pinchora_commands::ProfileContext {
                guild: None,
                viewer: unique(),
                lang: Lang::En,
                preview: true
            }
        )
        .await
        .is_err()
    );
    let other = h.state.app.db.user(unique()).await.unwrap();
    assert!(
        pinchora_commands::profile_card_for(
            &h.state.app,
            &user,
            "Private".into(),
            Some(other),
            pinchora_commands::ProfileContext {
                guild: None,
                viewer: id,
                lang: Lang::En,
                preview: true
            }
        )
        .await
        .is_err()
    );
    // Prime both adjacent fixed windows, so the test is stable across a minute boundary.
    let window = chrono::Utc::now().timestamp().max(0) as u64 / 60;
    for w in [window, window + 1] {
        for _ in 0..60 {
            h.state
                .app
                .cache
                .incr(&format!("web:rl:player-render:{id}:{w}"), Duration::from_secs(120))
                .await
                .unwrap();
        }
    }
    let mut second_session = h.browser();
    second_session.login(&h, id, vec![]).await;
    for path in [
        "/api/me/preview/profile-card.png",
        "/api/me/preview/wallet-card.png",
        "/api/me/character/preview.png?spec=x",
    ] {
        let response = second_session.get(path).await;
        assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS, "{path}");
        assert!(!response.header("retry-after").is_empty());
    }
}

#[tokio::test]
async fn player_previews_use_bounded_render_queue() {
    let Some(h) = Harness::new("player_preview_queue").await else {
        return;
    };
    let mut b = h.browser();
    b.login(&h, unique(), vec![]).await;
    let spec = b.get("/api/me/character").await.json()["spec"]
        .as_str()
        .unwrap()
        .to_owned();
    let slots = std::thread::available_parallelism().map_or(2, usize::from).min(2);
    let gate = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let (started, mut ready) = tokio::sync::mpsc::unbounded_channel();
    let mut tasks = Vec::new();
    for _ in 0..slots {
        let app = h.state.app.clone();
        let gate = gate.clone();
        let started = started.clone();
        tasks.push(tokio::spawn(async move {
            app.render_preview(move |_| {
                started.send(()).unwrap();
                let (lock, wake) = &*gate;
                let _guard = wake
                    .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(20), |done| !*done)
                    .unwrap();
                Ok(())
            })
            .await
        }));
    }
    for _ in 0..slots {
        ready.recv().await.unwrap();
    }
    let mut b2 = b.clone();
    let mut b3 = b.clone();
    let character_path = format!("/api/me/character/preview.png?spec={spec}");
    let (profile, wallet, character) = tokio::join!(
        b.get("/api/me/preview/profile-card.png"),
        b2.get("/api/me/preview/wallet-card.png"),
        b3.get(&character_path),
    );
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    for task in tasks {
        let _ = task.await.unwrap();
    }
    for result in [profile, wallet, character] {
        assert_eq!(result.status, StatusCode::TOO_MANY_REQUESTS, "{}", result.text());
    }
}

#[tokio::test]
async fn player_purchases_are_atomic_and_reject_forged_eligibility() {
    let Some(h) = Harness::new("player_purchase_attacks").await else {
        return;
    };
    let id = unique();
    let db = &h.state.app.db;
    let mut b = h.browser();
    b.login(&h, id, vec![]).await;
    db.user(id).await.unwrap();
    let c = Content::get();
    let bg = &c.backgrounds[0];
    let shop = c
        .badges
        .iter()
        .find_map(|b| match b.kind {
            BadgeKind::Shop { price } => Some((b, price)),
            _ => None,
        })
        .unwrap();
    let initial = bg.price.max(shop.1);
    sqlx::query("UPDATE users SET credits=$2 WHERE id=$1")
        .bind(id as i64)
        .bind(initial)
        .execute(db.pool())
        .await
        .unwrap();
    let mut second = b.clone();
    let bg_path = format!("/api/me/backgrounds/{}/buy", bg.id);
    let badge_path = format!("/api/me/badges/{}/buy", shop.0.id);
    let (a, z) = tokio::join!(
        b.write(Method::POST, &bg_path, json!({"price":-9223372036854775808_i64})),
        second.write(Method::POST, &badge_path, json!({"price":"18446744073709551616"}))
    );
    assert_eq!([a.status, z.status].iter().filter(|s| **s == StatusCode::OK).count(), 1);
    assert_eq!(
        [a.status, z.status]
            .iter()
            .filter(|s| **s == StatusCode::UNPROCESSABLE_ENTITY)
            .count(),
        1
    );
    let ledger = db.player_ledger(id).await.unwrap();
    assert_eq!(ledger.len(), 1);
    let charged = if a.status == StatusCode::OK { bg.price } else { shop.1 };
    assert_eq!(ledger[0].delta, -charged);
    assert_eq!(
        ledger[0].reason,
        if a.status == StatusCode::OK {
            "background"
        } else {
            "badge"
        }
    );
    assert_eq!(db.user(id).await.unwrap().credits, initial - charged);
    assert_eq!(
        db.owned_backgrounds(id).await.unwrap().contains(&bg.id),
        a.status == StatusCode::OK
    );
    assert_eq!(
        db.owned_badges(id).await.unwrap().contains(&shop.0.id),
        z.status == StatusCode::OK
    );
    // Failed charges roll ownership back; retries cannot mint refunds or extra ledger entries.
    let bought = if a.status == StatusCode::OK {
        &bg_path
    } else {
        &badge_path
    };
    assert_eq!(
        b.write(Method::POST, bought, json!({"refund":true})).await.json()["owned"],
        true
    );
    assert_eq!(
        b.write(Method::DELETE, bought, json!({})).await.status,
        StatusCode::METHOD_NOT_ALLOWED
    );
    for price in [i64::MIN, -1, 0] {
        assert!(db.buy_background(id, "negative", price).await.is_err());
        assert!(db.buy_badge(id, "negative", price).await.is_err());
        assert!(db.buy_accessory(id, "negative", price).await.is_err());
    }
    assert!(matches!(
        db.buy_background(id, "overflow", i64::MAX).await.unwrap(),
        pinchora_db::CustomizeBuy::Insufficient { .. }
    ));
    assert_eq!(db.player_ledger(id).await.unwrap().len(), 1);
    let earned = c
        .badges
        .iter()
        .find(|b| !matches!(b.kind, BadgeKind::Shop { .. }))
        .unwrap();
    for path in [
        format!("/api/me/badges/{}/buy", earned.id),
        "/api/me/backgrounds/missing/buy".into(),
    ] {
        assert_eq!(
            b.write(Method::POST, &path, json!({"owned":true})).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let locked = pinchora_game::character::accessories()
        .iter()
        .find(|a| a.level.is_some_and(|n| n > 0))
        .unwrap();
    assert_eq!(
        b.write(
            Method::POST,
            &format!("/api/me/character/accessories/{}/buy", locked.id),
            json!({"level":999})
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(db.owned_accessories(id).await.unwrap().is_empty());
    let title = b.get("/api/me/titles").await.json()["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["owned"] == false)
        .unwrap()["id"]
        .clone();
    assert_eq!(
        b.write(Method::PUT, "/api/me/title", json!({"id":title})).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(db.user(id).await.unwrap().credits, initial - charged);
}

#[tokio::test]
async fn player_outfit_default_name_is_localized_and_noop_is_not_audited() {
    let Some(h) = Harness::new("player_outfit_name").await else {
        return;
    };
    let id = unique();
    let mut b = h.browser();
    b.login(&h, id, vec![]).await;
    h.state.app.set_user_lang(id, Some(Lang::Tr)).await.unwrap();
    for _ in 0..2 {
        assert_eq!(
            b.write(Method::PUT, "/api/me/character/outfits/1", json!({}))
                .await
                .status,
            StatusCode::NO_CONTENT
        );
    }
    let outfits = h.state.app.db.outfits(id).await.unwrap();
    assert_eq!(outfits[0].name, "Kombin 1");
    let events = h.state.app.db.audit_page(0, None, 100).await.unwrap();
    let event = events
        .iter()
        .find(|a| a.actor_id == id as i64 && a.action == "player.outfit.save")
        .unwrap();
    let after: Value = serde_json::from_str(event.after.as_deref().unwrap()).unwrap();
    assert_eq!(after["slot"], 1);
    assert_eq!(after["name"]["characters"], 8);
    assert_eq!(
        events
            .iter()
            .filter(|a| a.actor_id == id as i64 && a.action == "player.outfit.save")
            .count(),
        1
    );
}
