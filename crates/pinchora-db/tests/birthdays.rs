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
    sqlx::query("UPDATE birthdays SET public=true WHERE user_id=$1")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
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

#[tokio::test]
async fn date_lock_survives_removal_and_privacy_stays_editable() {
    let Some(db) = db().await else { return };
    let [user] = ids();
    let mut input = pinchora_db::BirthdayInput {
        month: 1,
        day: 1,
        year: None,
        tz: "UTC",
        public: false,
        show_age: false,
    };
    assert!(db.set_birthday(user, &input).await.unwrap());
    input.public = true;
    assert!(db.set_birthday(user, &input).await.unwrap());
    input.day = 2;
    assert!(!db.set_birthday(user, &input).await.unwrap());
    input.day = 1;
    input.tz = "Pacific/Tongatapu";
    assert!(!db.set_birthday(user, &input).await.unwrap());
    assert!(db.remove_birthday(user).await.unwrap());
    assert!(!db.set_birthday(user, &input).await.unwrap());
    sqlx::query("UPDATE birthday_gift_years SET locked_until=now()-interval '1 second' WHERE user_id=$1")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(db.set_birthday(user, &input).await.unwrap());
    db.delete_user_data(user, Utc::now()).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM birthday_gift_years WHERE user_id=$1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn real_year_cap_private_profiles_and_durable_delivery() {
    let Some(db) = db().await else { return };
    let [user, guild, other, role, channel] = ids();
    let now = Utc::now();
    db.add_xp(
        guild,
        user,
        pinchora_db::XpGrant {
            server_xp: 1,
            global_xp: 1,
            points: 0,
        },
        now.date_naive(),
    )
    .await
    .unwrap();
    db.guild_settings(other).await.unwrap();
    db.set_birthday(
        user,
        &pinchora_db::BirthdayInput {
            month: 1,
            day: 1,
            year: Some(2000),
            tz: "UTC",
            public: true,
            show_age: true,
        },
    )
    .await
    .unwrap();
    db.save_birthday_settings(
        guild,
        &pinchora_core::birthdays::BirthdaySettings {
            enabled: true,
            channel_id: Some(channel),
            role_id: Some(role),
            badge: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    db.set_privacy(user, pinchora_db::Hide::Profile, true).await.unwrap();
    assert!(db.public_birthdays(guild).await.unwrap().is_empty());
    assert!(db.birthday_profile(guild, user).await.unwrap().is_none());
    assert_eq!(
        db.claim_birthday(guild, user, 2026, 500, true, None).await.unwrap(),
        None
    );
    db.set_privacy(user, pinchora_db::Hide::Profile, false).await.unwrap();
    let until = now + chrono::Duration::hours(24);
    // Configuration may change after the live role check: persist only the role that was checked.
    sqlx::query("UPDATE birthday_settings SET role_id=$2 WHERE guild_id=$1")
        .bind(guild as i64)
        .bind((role + 100) as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(
        db.claim_birthday(guild, user, 2026, 500, true, Some((role, until)))
            .await
            .unwrap(),
        Some(500)
    );
    assert_eq!(
        db.claim_birthday(guild, user, 2027, 500, true, Some((role, until)))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        db.claim_birthday(other, user, 2027, 500, true, None).await.unwrap(),
        Some(0)
    );
    assert_eq!(db.user(user).await.unwrap().credits, 500);
    assert_eq!(
        db.lapsed_roles(guild, user, until + chrono::Duration::seconds(1))
            .await
            .unwrap(),
        vec![role as i64]
    );
    let lock = db.birthday_dispatch_lock().await.unwrap().unwrap();
    assert!(db.birthday_dispatch_lock().await.unwrap().is_none());
    drop(lock);
    let pending = db.birthday_deliveries().await.unwrap();
    let row = pending.iter().find(|r| r.birthday.user_id == user as i64).unwrap();
    db.birthday_batch(&[row.delivery_id], row.delivery_id).await.unwrap();
    assert!(
        db.birthday_deliveries()
            .await
            .unwrap()
            .iter()
            .any(|r| r.delivery_id == row.delivery_id && r.batch_id == Some(row.delivery_id))
    );
    db.set_privacy(user, pinchora_db::Hide::Profile, true).await.unwrap();
    assert!(db.lock_birthday_batch(&[row.delivery_id]).await.unwrap().ids.is_empty());
    db.set_privacy(user, pinchora_db::Hide::Profile, false).await.unwrap();
    let guard = db.lock_birthday_batch(&[row.delivery_id]).await.unwrap();
    assert_eq!(guard.ids, vec![row.delivery_id]);
    let toggler = db.clone();
    let mut toggle = tokio::spawn(async move {
        toggler
            .set_privacy(user, pinchora_db::Hide::Profile, true)
            .await
            .unwrap()
    });
    let remover = db.clone();
    let mut remove = tokio::spawn(async move { remover.remove_birthday(user).await.unwrap() });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut remove)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut toggle)
            .await
            .is_err()
    );
    drop(guard);
    assert!(remove.await.unwrap());
    assert!(toggle.await.unwrap().hide_profile);
    assert!(db.lock_birthday_batch(&[row.delivery_id]).await.unwrap().ids.is_empty());
    db.finish_birthday_batch(&[row.delivery_id]).await.unwrap();
    assert!(
        !db.birthday_deliveries()
            .await
            .unwrap()
            .iter()
            .any(|r| r.delivery_id == row.delivery_id)
    );
    db.delete_user_data(user, now).await.unwrap();
}

#[tokio::test]
async fn deleting_data_cannot_reset_the_yearly_claim_or_date_lock() {
    use chrono::Datelike;
    let Some(db) = db().await else { return };
    let [user, guild] = ids();
    let now = Utc::now();
    let input = pinchora_db::BirthdayInput {
        month: 1,
        day: 1,
        year: None,
        tz: "UTC",
        public: true,
        show_age: false,
    };
    db.guild_settings(guild).await.unwrap();
    assert!(db.set_birthday(user, &input).await.unwrap());
    assert_eq!(
        db.claim_birthday(guild, user, now.year() as i16, 500, true, None)
            .await
            .unwrap(),
        Some(500)
    );
    db.delete_user_data(user, now).await.unwrap();
    assert!(!db.set_birthday(user, &input).await.unwrap());
    for table in ["birthdays", "birthday_sent", "birthday_gift_years"] {
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE user_id=$1"
        )))
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(count, 0);
    }
    let until: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT birthday_until FROM deleted_users WHERE user_id=$1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    let next_year = chrono::NaiveDate::from_ymd_opt(now.year() + 1, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    assert!(until >= next_year);
    // The normal tombstone sweep must not clear a still-live birthday lock.
    sqlx::query("UPDATE deleted_users SET deleted_at=now()-interval '100 days',last_daily_at=now(),liked=ARRAY[123::bigint] WHERE user_id=$1")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.forget_old_tombstones(now).await.unwrap();
    let remains: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM deleted_users WHERE user_id=$1)")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(remains);
    let cleared: (Option<chrono::DateTime<Utc>>, Vec<i64>, chrono::DateTime<Utc>) =
        sqlx::query_as("SELECT last_daily_at,liked,deleted_at FROM deleted_users WHERE user_id=$1")
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(cleared.0, None);
    assert!(cleared.1.is_empty());
    assert_eq!(cleared.2.timestamp(), 0);
    // Even a stale writer restoring a row cannot pay through the tombstone.
    sqlx::query("INSERT INTO birthdays (user_id,month,day,public) VALUES ($1,1,1,true)")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(
        db.claim_birthday(guild, user, now.year() as i16, 500, true, None)
            .await
            .unwrap(),
        None
    );
    db.delete_user_data(user, now).await.unwrap();
}
