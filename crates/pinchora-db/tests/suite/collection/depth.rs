use super::*;

#[tokio::test]
async fn depth_64_claimers_have_one_winner_and_limits_survive_a_new_pool() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, user] = ids();
    let players: [u64; 64] = ids();
    let drop = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    let now = Utc::now();
    let mut tasks = Vec::new();
    for player in players {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.grab_card(drop.id, 0, player, now, award("x_one"), daily("critterdrop", 20))
                .await
                .unwrap()
        }));
    }
    let mut winners = 0;
    for task in tasks {
        winners += usize::from(matches!(task.await.unwrap(), Grab::Taken { .. }));
    }
    assert_eq!(winners, 1);
    assert_eq!(ledger_total(&db, &players).await, 15);
    let mut tasks = Vec::new();
    for day in 0..24 {
        let drop = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.grab_card(
                drop.id,
                0,
                user,
                now,
                award("x_one"),
                Daily {
                    day,
                    ..daily("critterdrop", 20)
                },
            )
            .await
            .unwrap()
        }));
    }
    let mut claims = 0;
    for task in tasks {
        claims += usize::from(matches!(task.await.unwrap(), Grab::Taken { .. }));
    }
    assert_eq!(
        claims, 10,
        "caller supplied payout day cannot bypass the hard claim window"
    );
    let restarted = Db::connect(&std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap(), 2)
        .await
        .unwrap();
    let drop = critter_drop(&restarted, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert!(matches!(
        restarted
            .grab_card(drop.id, 0, user, now, award("x_one"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Limit
    ));
    assert_eq!(restarted.drop_row(drop.id).await.unwrap().unwrap().takers[0], 0);
    let start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    for hour in 0..6 {
        assert!(matches!(
            db.start_collection_roll(user, "x_one", false, start + Duration::hours(hour))
                .await
                .unwrap(),
            RollStart::Started(_)
        ));
    }
    assert!(matches!(
        restarted
            .start_collection_roll(user, "x_one", false, start + Duration::hours(6))
            .await
            .unwrap(),
        RollStart::Limit
    ));
    assert!(matches!(
        restarted
            .start_collection_roll(user, "x_one", false, start + Duration::days(1))
            .await
            .unwrap(),
        RollStart::Started(_)
    ));
}

#[tokio::test]
async fn depth_word_attempts_persist_and_normalize_only_surrounding_space_and_case() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, a, b, c] = ids();
    let drop = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    db.set_drop_mode(drop.id, "word", Some("Ab12cD34")).await.unwrap();
    for _ in 0..3 {
        assert!(matches!(
            db.grab_word_card(
                drop.id,
                0,
                a,
                Utc::now(),
                award("x_one"),
                daily("critterdrop", 20),
                "wrong"
            )
            .await
            .unwrap(),
            Grab::Challenge
        ));
    }
    let restarted = Db::connect(&std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap(), 2)
        .await
        .unwrap();
    assert!(matches!(
        restarted
            .grab_word_card(
                drop.id,
                0,
                a,
                Utc::now(),
                award("x_one"),
                daily("critterdrop", 20),
                "Ab12cD34"
            )
            .await
            .unwrap(),
        Grab::Challenge
    ));
    assert!(matches!(
        db.grab_word_card(
            drop.id,
            0,
            b,
            Utc::now(),
            award("x_one"),
            daily("critterdrop", 20),
            " \tAB12cd34\n"
        )
        .await
        .unwrap(),
        Grab::Taken { .. }
    ));
    assert!(matches!(
        db.grab_word_card(
            drop.id,
            1,
            c,
            Utc::now(),
            award("x_two"),
            daily("critterdrop", 20),
            "ab12 cd34"
        )
        .await
        .unwrap(),
        Grab::Challenge
    ));
    assert!(db.dex(a).await.unwrap().is_empty());
}

#[tokio::test]
async fn depth_shard_ledger_deduplicates_concurrent_retries_and_preserves_original_and_glimmers() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [user] = ids();
    sqlx::query("INSERT INTO dex_entries (user_id,entry_id,caught,kept,glimmer) VALUES ($1,'x_one',5,5,2)")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.recycle_critter(user, "x_one", 20, "once").await.unwrap()
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap(), Some(20));
    }
    assert_eq!(db.dex(user).await.unwrap()[0].kept, 4);
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.craft_shard_loot(user, 12, "dewtrail_print", "craft-once")
                .await
                .unwrap()
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap(), Some(8));
    }
    assert_eq!(pieces(&db, user, "dewtrail_print~bound").await, 1);
    assert_eq!(
        db.recycle_critter(user, "x_one", 2, "last-spare").await.unwrap(),
        Some(10)
    );
    assert_eq!(db.recycle_critter(user, "x_one", 2, "original").await.unwrap(), None);
    assert!(db.recycle_critter(user, "x_one", -1, "negative").await.is_err());
    assert!(
        db.craft_shard_loot(user, -1, "dewtrail_print", "negative")
            .await
            .is_err()
    );
    let (sum, count): (i64, i64) =
        sqlx::query_as("SELECT sum(delta)::bigint,count(*) FROM collection_shard_ledger WHERE user_id=$1")
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!((sum, count), (10, 3));
    assert_eq!(sum, db.shards(user).await.unwrap());
    let moves: Vec<(String, i64)> = sqlx::query_as(
        "SELECT asset, sum(delta)::bigint FROM economy_ledger
         WHERE user_id=$1 AND account='wallet' GROUP BY asset ORDER BY asset",
    )
    .bind(user as i64)
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert_eq!(
        moves,
        vec![
            ("collection:x_one".into(), -2),
            ("item:dewtrail_print~bound".into(), 1),
            ("shards".into(), 10),
        ]
    );
    let broken: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT movement FROM economy_ledger WHERE user_id=$1
         GROUP BY movement HAVING count(*)<>2 OR sum(delta)<>0 OR count(DISTINCT asset)<>1) p",
    )
    .bind(user as i64)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(broken, 0);
    db.delete_user_data(user, Utc::now()).await.unwrap();
    let remaining: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM economy_ledger WHERE user_id=$1)
         + (SELECT count(*) FROM collection_shard_ledger WHERE user_id=$1)",
    )
    .bind(user as i64)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(remaining, 0);
}

#[tokio::test]
async fn depth_reservation_honors_consent_blocks_claim_cap_and_delivery_presence() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, user, host] = ids();
    db.earn_shells(user, 1, "test").await.unwrap();
    sqlx::query("INSERT INTO members (guild_id,user_id) VALUES ($1,$2)")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.set_wish(user, "x_one", false).await.unwrap();
    let d = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert!(d.protected_user_id.is_none());
    db.set_wish(user, "x_one", true).await.unwrap();
    sqlx::query("UPDATE users SET dm_opt_out=true WHERE id=$1")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let d = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert!(d.protected_user_id.is_none());
    sqlx::query("UPDATE users SET dm_opt_out=false WHERE id=$1")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO guestbook_blocks(host_id,blocked_id,blocked_name) VALUES($1,$2,'host')")
        .bind(user as i64)
        .bind(host as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let d = db
        .create_drop(NewDrop {
            kind: DROP_CRITTERS,
            guild_id: guild,
            channel_id: guild + 1,
            host: Some(host),
            cards: &["x_one".into()],
            expires_at: Utc::now() + Duration::seconds(90),
        })
        .await
        .unwrap();
    assert!(d.protected_user_id.is_none());
    let d = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert_eq!(d.protected_user_id, Some(user as i64));
    assert!(
        db.prepare_collection_ping(d.id, false)
            .await
            .unwrap()
            .protected_user_id
            .is_none(),
        "left member not pinged"
    );
    let d = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    db.set_wish(user, "x_one", false).await.unwrap();
    assert!(
        db.prepare_collection_ping(d.id, true)
            .await
            .unwrap()
            .protected_user_id
            .is_none(),
        "late opt-out honored"
    );
    db.set_wish(user, "x_one", true).await.unwrap();
    let d = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    let d = db.prepare_collection_ping(d.id, true).await.unwrap();
    let until = d.protected_until.unwrap();
    assert!(matches!(
        db.grab_card(
            d.id,
            0,
            host,
            until - Duration::milliseconds(1),
            award("x_one"),
            daily("critterdrop", 20)
        )
        .await
        .unwrap(),
        Grab::Protected
    ));
    assert!(matches!(
        db.grab_card(d.id, 0, host, until, award("x_one"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Taken { .. }
    ));
    let another = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert!(another.protected_user_id.is_none(), "cannot hoard reservations");
    db.abort_drop(d.id).await.unwrap();
    sqlx::query("INSERT INTO collection_daily(user_id,day,game,plays) VALUES($1,$2,'critter_claim',10)")
        .bind(user as i64)
        .bind(Utc::now().timestamp().div_euclid(86400) as i32)
        .execute(db.pool())
        .await
        .unwrap();
    let d = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert!(d.protected_user_id.is_none());
}

#[tokio::test]
async fn depth_votes_are_immutable_capped_and_never_pay() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild] = ids();
    let users: [u64; 101] = ids();
    let d = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    db.set_drop_mode(d.id, "vote", None).await.unwrap();
    let now = Utc::now();
    for user in &users[..100] {
        assert_eq!(db.vote_card(d.id, 1, *user, now).await.unwrap(), VoteCard::Cast);
    }
    assert_eq!(db.vote_card(d.id, 0, users[0], now).await.unwrap(), VoteCard::Cast);
    assert_eq!(db.vote_card(d.id, 0, users[100], now).await.unwrap(), VoteCard::Full);
    let (slot,): (i16,) = sqlx::query_as("SELECT slot FROM collection_votes WHERE drop_id=$1 AND user_id=$2")
        .bind(d.id)
        .bind(users[0] as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(slot, 1);
    assert_eq!(ledger_total(&db, &users).await, 0);
    assert_eq!(
        db.vote_card(d.id, 0, users[0], now + Duration::seconds(31))
            .await
            .unwrap(),
        VoteCard::Gone
    );
}

#[tokio::test]
async fn depth_deletion_erases_identity_without_reopening_claims_or_hourly_cooldowns() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, user, other] = ids();
    let now = Utc::now();
    sqlx::query("INSERT INTO members(guild_id,user_id) VALUES($1,$2)")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.set_wish(user, "x_one", true).await.unwrap();
    let drop = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert!(matches!(
        db.grab_card(drop.id, 0, user, now, award("x_one"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Taken { .. }
    ));
    db.start_collection_roll(user, "x_two", false, now).await.unwrap();
    db.delete_user_data(user, Utc::now()).await.unwrap();
    let row = db.drop_row(drop.id).await.unwrap().unwrap();
    assert_eq!(row.takers, [-1, 0, 0]);
    assert!(row.protected_user_id.is_none());
    assert!(db.wishlist(user).await.unwrap().is_empty());
    assert!(db.collection_roll(user).await.unwrap().is_none());
    assert!(matches!(
        db.grab_card(drop.id, 0, other, now, award("x_one"), daily("critterdrop", 20))
            .await
            .unwrap(),
        Grab::Gone
    ));
    assert!(
        matches!(
            db.grab_card(drop.id, 1, user, Utc::now(), award("x_two"), daily("critterdrop", 20))
                .await
                .unwrap(),
            Grab::Gone
        ),
        "deleting cannot take a second card from the same drop"
    );
    assert!(matches!(
        db.start_collection_roll(user, "x_one", false, now + Duration::minutes(1))
            .await
            .unwrap(),
        RollStart::Cooldown
    ));
    assert!(matches!(
        db.start_collection_roll(user, "x_one", false, now + Duration::hours(1))
            .await
            .unwrap(),
        RollStart::Started(_)
    ));
    sqlx::query("INSERT INTO members(guild_id,user_id) VALUES($1,$2)")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.set_wish(user, "x_one", true).await.unwrap();
    let next = critter_drop(&db, guild, &["x_one", "x_two", "x_three"], Duration::seconds(90)).await;
    assert!(next.protected_user_id.is_none(), "delete cannot refresh a reservation");
}
