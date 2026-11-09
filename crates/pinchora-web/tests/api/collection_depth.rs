use crate::*;

#[tokio::test]
async fn collection_pages_require_session_csrf_validate_wishes_and_audit() {
    let Some(h) = Harness::new("collection_depth_pages").await else {
        return;
    };
    let user = unique();
    let mut b = h.browser();
    assert_eq!(b.get("/api/me/collection/book").await.status, StatusCode::UNAUTHORIZED);
    b.login(&h, user, vec![]).await;
    let book = b.get("/api/me/collection/book").await;
    assert_eq!(book.status, StatusCode::OK, "{}", book.text());
    assert_eq!(book.json()["total"], 84);
    let entry = pinchora_game::collection::Collection::get().critters[0].id.clone();
    let token = b.csrf.take();
    assert_eq!(
        b.write(
            Method::PUT,
            "/api/me/collection/wishlist",
            json!({"entry_id":entry,"ping_on_spawn":true})
        )
        .await
        .code(),
        "csrf"
    );
    b.csrf = token;
    assert_eq!(
        b.write(
            Method::PUT,
            "/api/me/collection/wishlist",
            json!({"entry_id":entry,"ping_on_spawn":true,"user_id":unique()})
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let saved = b
        .write(
            Method::PUT,
            "/api/me/collection/wishlist",
            json!({"entry_id":entry,"ping_on_spawn":true}),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(
        b.get("/api/me/collection/wishlist").await.json()["items"][0]["ping_on_spawn"],
        true
    );
    let roll = b.write(Method::POST, "/api/me/collection/roll", json!({})).await;
    assert_eq!(roll.status, StatusCode::OK, "{}", roll.text());
    let claim = b.write(Method::POST, "/api/me/collection/roll/claim", json!({})).await;
    assert_eq!(claim.status, StatusCode::OK, "{}", claim.text());
    assert_ne!(
        b.write(Method::POST, "/api/me/collection/roll/claim", json!({}))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(h.state.app.db.dex(user).await.unwrap().len(), 1);
    let audit = h.state.app.db.audit_page(0, None, 100).await.unwrap();
    assert!(
        audit
            .iter()
            .any(|r| r.actor_id == user as i64 && r.action == "player.collection.roll.claim")
    );
    h.state.app.set_user_lang(user, Some(Lang::Tr)).await.unwrap();
    let tr = b
        .write(
            Method::POST,
            "/api/me/collection/recycle",
            json!({"entry_id":entry,"confirmed":true,"request_id":"empty"}),
        )
        .await;
    assert_ne!(tr.status, StatusCode::OK);
    assert!(
        tr.text().contains("fazlan") || tr.text().contains("kopya"),
        "{}",
        tr.text()
    );
}

#[tokio::test]
async fn collection_recycle_requires_confirmation_and_retries_do_not_duplicate_rewards() {
    let Some(h) = Harness::new("collection_depth_retries").await else {
        return;
    };
    let user = unique();
    let mut b = h.browser();
    b.login(&h, user, vec![]).await;
    let entry = &pinchora_game::collection::Collection::get().critters[0];
    sqlx::query("INSERT INTO dex_entries(user_id,entry_id,caught,kept) VALUES($1,$2,4,4)")
        .bind(user as i64)
        .bind(&entry.id)
        .execute(h.state.app.db.pool())
        .await
        .unwrap();
    let original = h.state.app.db.dex(user).await.unwrap()[0].kept;
    for body in [
        json!({"entry_id":entry.id}),
        json!({"entry_id":entry.id,"confirmed":false,"request_id":"one"}),
    ] {
        assert_ne!(
            b.write(Method::POST, "/api/me/collection/recycle", body).await.status,
            StatusCode::OK
        );
    }
    assert_eq!(h.state.app.db.dex(user).await.unwrap()[0].kept, original);
    let request = json!({"entry_id":entry.id,"confirmed":true,"request_id":"one"});
    let first = b
        .write(Method::POST, "/api/me/collection/recycle", request.clone())
        .await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    let retry = b.write(Method::POST, "/api/me/collection/recycle", request).await;
    assert_eq!(retry.status, StatusCode::OK, "{}", retry.text());
    assert_eq!(first.json(), retry.json());
    assert_eq!(h.state.app.db.dex(user).await.unwrap()[0].kept, original - 1);
    // Seed an extra ledger-backed credit, then replay the same craft; response names the original item.
    h.state
        .app
        .db
        .recycle_critter(user, &entry.id, 45, "fund-craft")
        .await
        .unwrap();
    let request = json!({"request_id":"craft-one"});
    let first = b.write(Method::POST, "/api/me/collection/craft", request.clone()).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    let retry = b.write(Method::POST, "/api/me/collection/craft", request).await;
    assert_eq!(retry.status, StatusCode::OK, "{}", retry.text());
    assert_eq!(first.json(), retry.json());
    assert_eq!(
        h.state
            .app
            .db
            .inventory(user)
            .await
            .unwrap()
            .iter()
            .map(|r| r.quantity)
            .sum::<i32>(),
        1
    );
    let book = b.get("/api/me/collection/book").await;
    for key in ["challenge_word", "protected_user_id", "takers", "drop_id"] {
        assert!(!book.text().contains(key));
    }
    assert_eq!(b.get("/api/me/collection/drops").await.status, StatusCode::NOT_FOUND);
}
