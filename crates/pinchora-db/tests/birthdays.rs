//! Birthdays use the real Postgres test database, including a concurrent claim.
use chrono::Utc;
use pinchora_db::Db;

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 8, std::time::Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}
fn ids<const N: usize>() -> [u64; N] {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        >> 4;
    std::array::from_fn(|n| base + n as u64)
}
#[tokio::test]
async fn opt_in_and_once_across_pods() {
    let Some(db) = db().await else { return };
    let [guild, other, user, unbadged] = ids();
    db.set_birthday(
        user,
        &pinchora_db::BirthdayInput {
            month: 2,
            day: 29,
            year: Some(2000),
            tz: "Europe/Istanbul",
            public: false,
            show_age: false,
        },
    )
    .await
    .unwrap();
    let own = db.birthday(user).await.unwrap().unwrap();
    assert_eq!(own.year, Some(2000));
    assert!(!own.public);
    assert!(db.public_birthdays(guild).await.unwrap().is_empty());
    db.guild_settings(guild).await.unwrap();
    db.guild_settings(other).await.unwrap();
    let a = db.clone();
    let b = db.clone();
    let (one, two) = tokio::join!(
        a.claim_birthday(guild, user, 2028, 100, true, None),
        b.claim_birthday(guild, user, 2028, 100, true, None)
    );
    assert_eq!([one.unwrap(), two.unwrap()].iter().filter(|v| v.is_some()).count(), 1);
    assert_eq!(db.user(user).await.unwrap().credits, 100);
    db.set_birthday(
        unbadged,
        &pinchora_db::BirthdayInput {
            month: 5,
            day: 12,
            year: None,
            tz: "UTC",
            public: true,
            show_age: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        db.claim_birthday(guild, unbadged, 2028, 0, false, None).await.unwrap(),
        Some(0)
    );
    assert!(
        !db.owned_badges(unbadged)
            .await
            .unwrap()
            .contains(&"cake_day".to_owned())
    );
    assert_eq!(
        db.claim_birthday(other, unbadged, 2028, 0, true, None).await.unwrap(),
        Some(0)
    );
    assert!(
        db.owned_badges(unbadged)
            .await
            .unwrap()
            .contains(&"cake_day".to_owned())
    );
    assert!(db.owned_badges(user).await.unwrap().contains(&"cake_day".to_owned()));
    assert_eq!(
        db.claim_birthday(other, user, 2028, 100, false, None).await.unwrap(),
        Some(0)
    );
    assert_eq!(db.user(user).await.unwrap().credits, 100);
    db.set_birthday(
        user,
        &pinchora_db::BirthdayInput {
            month: 2,
            day: 29,
            year: Some(2000),
            tz: "Europe/Istanbul",
            public: true,
            show_age: true,
        },
    )
    .await
    .unwrap();
    assert!(db.remove_birthday(user).await.unwrap());
    assert!(db.birthday(user).await.unwrap().is_none());
    db.delete_user_data(user, Utc::now()).await.unwrap();
    db.delete_user_data(unbadged, Utc::now()).await.unwrap();
}
