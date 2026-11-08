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
