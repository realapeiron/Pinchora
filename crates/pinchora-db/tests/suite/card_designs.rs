//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! Welcome and goodbye designs, and the uploads kept for them.

use pinchora_core::admin::card_design::CardKind;
use pinchora_core::admin::card_design::Delivery;
use pinchora_core::admin::card_design::MAX_UPLOADS;

use crate::support::db;

/// A server id unique per run, so the suite can run repeatedly against one database.
use crate::support::id as guild;

fn upload_id(n: u128) -> String {
    format!("{n:032x}")
}

#[tokio::test]
async fn designs_are_saved_per_kind_and_cleared_back_to_classic() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let g = guild();
    assert_eq!(db.card_design(g, CardKind::Welcome).await.unwrap(), None);
    let json = r#"{"version": 1, "width": 1100}"#;
    let row = db
        .put_card_design(g, CardKind::Welcome, Some(json), Delivery::BeforeMessage)
        .await
        .unwrap();
    assert_eq!(row.delivery(CardKind::Welcome), Delivery::BeforeMessage);
    let stored: serde_json::Value = serde_json::from_str(row.design.as_deref().unwrap()).unwrap();
    assert_eq!(stored["width"], 1100);
    assert_eq!(
        db.card_design(g, CardKind::Goodbye).await.unwrap(),
        None,
        "kinds are apart"
    );

    let again = db
        .put_card_design(g, CardKind::Welcome, Some(json), Delivery::Dm)
        .await
        .unwrap();
    assert_eq!(again.delivery, "dm");
    assert!(db.clear_card_design(g, CardKind::Welcome).await.unwrap());
    assert!(
        !db.clear_card_design(g, CardKind::Welcome).await.unwrap(),
        "already classic"
    );
    let classic = db.card_design(g, CardKind::Welcome).await.unwrap().unwrap();
    assert_eq!(
        (classic.design, classic.delivery.as_str()),
        (None, "dm"),
        "the delivery stays"
    );
}

#[tokio::test]
async fn uploads_are_kept_per_server_and_the_oldest_unused_go() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let (g, other) = (guild(), guild() + 1);
    let base = u128::from(g) * 100;
    for n in 0..MAX_UPLOADS + 3 {
        db.add_card_upload(g, &upload_id(base + n as u128), b"png", 10, 20)
            .await
            .unwrap();
        if n == 0 {
            // The first one is in use: it survives however many come after it.
            let design = format!(r#"{{"background": {{"type": "upload", "id": "{}"}}}}"#, upload_id(base));
            db.put_card_design(g, CardKind::Goodbye, Some(&design), Delivery::ImageOnly)
                .await
                .unwrap();
        }
    }
    let kept = db.card_uploads(g).await.unwrap();
    assert_eq!(kept.len(), MAX_UPLOADS + 1);
    assert!(kept.iter().any(|u| u.id == upload_id(base)), "the one in use stays");
    assert!(
        !kept.iter().any(|u| u.id == upload_id(base + 1)),
        "the oldest unused one went"
    );
    assert_eq!((kept[0].width, kept[0].height), (10, 20));
    assert_eq!(
        db.card_upload(g, &upload_id(base)).await.unwrap().as_deref(),
        Some(&b"png"[..])
    );
    assert_eq!(
        db.card_upload(other, &upload_id(base)).await.unwrap(),
        None,
        "not another server's"
    );
}

#[tokio::test]
async fn branding_null_slots_do_not_disable_upload_pruning_and_active_images_survive() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let g = guild();
    db.save_guild_branding(g, &pinchora_db::GuildBranding::default())
        .await
        .unwrap();
    let protected = format!("{g:032x}");
    db.add_card_upload(g, &protected, b"png", 1, 1).await.unwrap();
    sqlx::query("UPDATE guild_branding SET active_avatar_upload_id=$2 WHERE guild_id=$1")
        .bind(g as i64)
        .bind(&protected)
        .execute(db.pool())
        .await
        .unwrap();
    for i in 1..=MAX_UPLOADS + 5 {
        db.add_card_upload(g, &format!("{:032x}", u128::from(g) + i as u128), b"png", 1, 1)
            .await
            .unwrap();
    }
    assert_eq!(db.card_uploads(g).await.unwrap().len(), MAX_UPLOADS + 1);
    assert!(db.card_upload(g, &protected).await.unwrap().is_some());
}

#[tokio::test]
async fn branding_reset_does_not_erase_a_concurrent_new_draft() {
    let _isolation = crate::support::shared().await;
    use pinchora_db::GuildBranding;
    let Some(db) = db(5).await else { return };
    let g = guild();
    let before = GuildBranding {
        nickname: Some("Before".into()),
        embed_color: Some(1),
        ..Default::default()
    };
    db.save_guild_branding(g, &before).await.unwrap();
    assert!(db.claim_branding_apply(g, None).await.unwrap());
    let next = GuildBranding {
        nickname: Some("Saved during Discord request".into()),
        embed_color: Some(2),
        ..Default::default()
    };
    db.save_guild_branding(g, &next).await.unwrap();
    db.branding_result(g, &GuildBranding::default(), [true; 4], [Some(true); 3], Some(&before))
        .await
        .unwrap();
    let after = db.guild_branding(g).await.unwrap();
    assert_eq!(after.nickname, next.nickname);
    assert_eq!(after.embed_color, next.embed_color);
    assert!(after.active_nickname.is_none() && after.active_embed_color.is_none());
}
