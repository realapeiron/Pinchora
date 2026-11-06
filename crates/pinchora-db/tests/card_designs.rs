//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! Welcome and goodbye designs, and the uploads kept for them.

use pinchora_core::card_design::{CardKind, Delivery, MAX_UPLOADS};
use pinchora_db::Db;

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 5, std::time::Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}

/// A server id unique per run, so the suite can run repeatedly against one database.
fn guild() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        >> 4
}

fn upload_id(n: usize) -> String {
    format!("{n:032x}")
}

#[tokio::test]
async fn designs_are_saved_per_kind_and_cleared_back_to_classic() {
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
    let (g, other) = (guild(), guild() + 1);
    let base = g as usize * 100;
    for n in 0..MAX_UPLOADS + 3 {
        db.add_card_upload(g, &upload_id(base + n), b"png", 10, 20)
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
