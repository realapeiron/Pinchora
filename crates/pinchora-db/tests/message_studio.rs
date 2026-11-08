use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pinchora_core::card_design::{CardKind, Delivery};
use pinchora_db::Db;

#[tokio::test]
async fn card_revisions_are_durable_bounded_and_returning_is_per_member() {
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect_waiting(&url, 4, Duration::from_secs(60)).await.unwrap();
    db.migrate().await.unwrap();
    let guild = (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64) >> 4;
    let actor = guild + 1;
    db.guild_settings(guild).await.unwrap();
    assert!(!db.card_member_returning(guild, actor).await.unwrap());
    assert!(db.card_member_returning(guild, actor).await.unwrap());
    assert!(!db.card_was_returning(guild, actor + 1).await.unwrap());
    for n in 0..24 {
        let design = format!(
            r#"{{"version":1,"width":300,"height":150,"background":{{"type":"transparent"}},"dim":0,"layers":[],"alt_text":"version {n}"}}"#
        );
        db.save_card_revision(guild, CardKind::Boost, Some(&design), Delivery::WithMessage, actor)
            .await
            .unwrap();
    }
    let versions = db.card_history(guild, CardKind::Boost).await.unwrap();
    assert_eq!(versions.len(), 20);
    assert!(versions[0].design.as_deref().unwrap().contains("version 23"));
    assert!(
        db.card_revision(guild, CardKind::Boost, versions[19].id)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(versions[0].actor_id, actor as i64);
}

#[tokio::test]
async fn migration_0081_preserves_live_cards_and_enforces_guild_binding() {
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect_waiting(&url, 2, Duration::from_secs(60)).await.unwrap();
    let mut tx = db.pool().begin().await.unwrap();
    // Exercise the exact unreleased migration over a pre-D4 schema, isolated from real tables.
    sqlx::query("SET LOCAL search_path TO pg_temp")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../../../migrations/0043_card_designs.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO card_designs (guild_id,kind,design,delivery) VALUES (1,'welcome','{\"legacy\":true}','dm')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!("../../../migrations/0081_message_studio.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    let legacy: (serde_json::Value, String) =
        sqlx::query_as("SELECT design,delivery FROM card_designs WHERE guild_id=1 AND kind='welcome'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(legacy, (serde_json::json!({"legacy":true}), "dm".into()));
    sqlx::query("INSERT INTO card_designs (guild_id,kind,delivery) VALUES (1,'boost','text_only'),(1,'level_up','with_message')")
        .execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO message_kits (guild_id,key,name,payload) VALUES (1,'test','Test','{}')")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        sqlx::query("INSERT INTO message_event_kits (guild_id,kind,kit_key) VALUES (2,'welcome','test')")
            .execute(&mut *tx)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}
