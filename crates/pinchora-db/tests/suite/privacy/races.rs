use crate::privacy::*;

/// A derby the deleted player hosts is called off, and every racer gets their stake back.
#[tokio::test]
async fn deleting_a_derby_host_gives_the_racers_their_stakes_back() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let [host, racer, rival, guild] = ids();
    let now = micros(Utc::now());
    for player in [host, racer, rival] {
        db.earn_shells(player, 200, "test").await.unwrap();
    }
    let lobby = db
        .open_party_game(NewGame {
            kind: "derby",
            guild_id: guild,
            channel_id: guild,
            host,
            guest: None,
            bet: 50,
            status: PENDING,
            state: "7",
            ttl_secs: 600,
        })
        .await
        .unwrap()
        .expect("a lobby");
    for (player, crab) in [(host, 0), (racer, 1), (rival, 2)] {
        let joined = db.derby_join(lobby.id, player, "Crab", crab, 8, 1_000).await.unwrap();
        assert_eq!(joined, DerbyJoin::Joined);
        assert_eq!(db.user(player).await.unwrap().credits, 150, "the stake is held");
    }

    db.delete_user_data(host, now).await.unwrap();
    for player in [racer, rival] {
        assert_eq!(db.user(player).await.unwrap().credits, 200, "the stake came back");
    }
    assert_eq!(
        db.user(host).await.unwrap().credits,
        0,
        "and nothing came back to the host"
    );
    assert!(
        db.party_members(lobby.id).await.unwrap().is_empty(),
        "the lobby is gone"
    );
    let (net,): (i64,) = sqlx::query_as("SELECT COALESCE(sum(net), 0)::BIGINT FROM party_daily WHERE user_id = $1")
        .bind(racer as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(net, 0, "the refunded stake is off the day's losses too");
    for player in [host, racer, rival] {
        db.delete_user_data(player, now + Duration::days(40)).await.unwrap();
    }
}

/// Shells held for a giveaway or a duel are not lost to others, and not resurrected for the host.
#[tokio::test]
async fn deleting_a_host_calls_the_giveaway_off_and_keeps_the_money_in_one_place() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db(8).await else { return };
    sqlx::query("DELETE FROM giveaways").execute(db.pool()).await.unwrap();
    let [host, entrant, guild] = ids();
    let now = micros(Utc::now());
    db.earn_shells(host, 500, "test").await.unwrap();
    let GiveawayOpen::Started(giveaway) = db
        .create_giveaway(NewGiveaway {
            guild_id: guild,
            channel_id: 1,
            host,
            prize: "Shells",
            shells: 100,
            winners: 2,
            required_role: None,
            ends_at: now - Duration::minutes(1),
        })
        .await
        .unwrap()
    else {
        panic!("the giveaway starts");
    };
    db.toggle_giveaway_entry(giveaway.id, entrant, now - Duration::minutes(2))
        .await
        .unwrap();
    assert_eq!(db.user(host).await.unwrap().credits, 300, "the prize is held");

    db.delete_user_data(host, now).await.unwrap();
    assert!(db.giveaway(giveaway.id).await.unwrap().is_none());
    assert!(
        db.due_giveaways(now, 1_000)
            .await
            .unwrap()
            .iter()
            .all(|due| due.id != giveaway.id)
    );
    assert_eq!(
        db.user(host).await.unwrap().credits,
        0,
        "no prize comes back to a player who left"
    );
    assert_eq!(
        db.user(entrant).await.unwrap().credits,
        0,
        "and nobody is paid from it either"
    );
    db.delete_user_data(entrant, now).await.unwrap();
}

/// A claim that arrives while a delete of the same player has removed the row and written the
/// tombstone but not committed yet must wait for it and then find the tombstone: no second payout,
/// no fresh cooldown, and no error for the player. (Before the player lock, the claim's insert
/// created a row with no cooldowns under the delete, and the claims that were already running lost
/// their row and failed.)
#[tokio::test]
async fn claims_that_overlap_a_delete_wait_for_it_and_pay_nothing() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let [user, other, host, other_host] = ids();
    let now = micros(Utc::now());
    let day = now.date_naive();
    let (reward, cap) = (25, 1);

    // Everything the player can use up in a day, used up.
    assert!(matches!(
        db.claim_daily(user).await.unwrap(),
        DailyOutcome::Claimed { .. }
    ));
    for period in [Period::Weekly, Period::Monthly] {
        assert!(matches!(
            db.claim_period(user, period, now).await.unwrap(),
            PeriodOutcome::Claimed { .. }
        ));
    }
    assert!(matches!(
        db.give_rep(user, other).await.unwrap(),
        RepOutcome::Given { .. }
    ));
    for house in [host, other_host] {
        sqlx::query("INSERT INTO houses (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(house as i64)
            .execute(db.pool())
            .await
            .unwrap();
    }
    assert_eq!(db.like_house(host, user, day).await.unwrap(), Some(1));
    assert!(matches!(
        db.record_visit(host, user, day, reward, cap).await.unwrap(),
        VisitOutcome::Rewarded { reward: 25 }
    ));

    // The delete is done but for its commit. Claims of every kind start now.
    let mut delete = db.pool().begin().await.unwrap();
    Db::delete_user_data_in(&mut delete, user, now).await.unwrap();
    let daily = tokio::spawn({
        let db = db.clone();
        async move { db.claim_daily(user).await }
    });
    let weekly = tokio::spawn({
        let db = db.clone();
        async move { db.claim_period(user, Period::Weekly, now).await }
    });
    let monthly = tokio::spawn({
        let db = db.clone();
        async move { db.claim_period(user, Period::Monthly, now).await }
    });
    let rep = tokio::spawn({
        let db = db.clone();
        async move { db.give_rep(user, other).await }
    });
    let like = tokio::spawn({
        let db = db.clone();
        async move { db.like_house(host, user, day).await }
    });
    let visit = tokio::spawn({
        let db = db.clone();
        async move { db.record_visit(other_host, user, day, reward, cap).await }
    });
    // A claim-free way to create the row, and the two reads of the tombstone, wait too.
    let wallet = tokio::spawn({
        let db = db.clone();
        async move { db.user(user).await }
    });
    let walk = tokio::spawn({
        let db = db.clone();
        async move { db.walk_locked_until(user, now).await }
    });
    let wordle = tokio::spawn({
        let db = db.clone();
        async move { db.daily_wordle_played_before_deleting(user, 20_000).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    assert!(
        !daily.is_finished() && !weekly.is_finished() && !monthly.is_finished(),
        "the claims wait for the delete instead of running under it"
    );
    assert!(!rep.is_finished() && !like.is_finished() && !visit.is_finished());
    assert!(!wallet.is_finished() && !walk.is_finished() && !wordle.is_finished());
    delete.commit().await.unwrap();

    let daily = daily.await.unwrap().unwrap();
    let weekly = weekly.await.unwrap().unwrap();
    let monthly = monthly.await.unwrap().unwrap();
    assert!(matches!(daily, DailyOutcome::Cooldown { .. }), "{daily:?}");
    assert!(matches!(weekly, PeriodOutcome::Cooldown { .. }), "{weekly:?}");
    assert!(matches!(monthly, PeriodOutcome::Cooldown { .. }), "{monthly:?}");
    let rep = rep.await.unwrap().unwrap();
    assert!(matches!(rep, RepOutcome::Cooldown { .. }), "{rep:?}");
    assert_eq!(like.await.unwrap().unwrap(), None, "today's like was already given");
    let visit = visit.await.unwrap().unwrap();
    assert!(
        matches!(visit, VisitOutcome::Rewarded { reward: 0 }),
        "the day's paid visit was already used: {visit:?}"
    );
    let wallet = wallet.await.unwrap().unwrap();
    assert!(wallet.last_daily_at.is_some() && wallet.last_rep_at.is_some());
    assert_eq!(wallet.credits, 0, "they come back with nothing");
    assert!(walk.await.unwrap().is_ok() && wordle.await.unwrap().is_ok());
    assert_eq!(db.user(user).await.unwrap().credits, 0);
    let (paid,): (i64,) = sqlx::query_as("SELECT count(*) FROM credit_ledger WHERE user_id = $1 AND delta > 0")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(paid, 0, "nothing was paid to the player after the delete");
    for gone in [user, other, host, other_host] {
        db.delete_user_data(gone, now).await.unwrap();
    }
}

/// The real delete against claims that come in on their own connections across the whole of it, as
/// separate interactions would: over many rounds nothing is paid beyond the one legitimate claim,
/// and no claim fails.
#[tokio::test]
async fn claims_spread_over_a_delete_pay_nothing_and_never_fail() {
    // This regression intentionally opens 32 connections. Own the fixture budget
    // while they run so other tests can still connect to wait on their guards.
    let _isolation = crate::support::exclusive().await;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect_waiting(&url, 32, std::time::Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");

    // How long a delete takes on this machine, to spread the claims over a little more than that.
    let [probe] = ids();
    db.claim_daily(probe).await.unwrap();
    let started = std::time::Instant::now();
    db.delete_user_data(probe, Utc::now()).await.unwrap();
    let span_us = (started.elapsed().as_micros() as u64 * 3 / 2).max(2_000);

    let (rounds, per_round) = (30, 60u64);
    let (mut paid, mut shells, mut errors) = (0u32, 0i64, Vec::new());
    for round in 0..rounds {
        let [user, other] = ids();
        let now = Utc::now();
        db.claim_daily(user).await.unwrap();
        db.claim_period(user, Period::Weekly, now).await.unwrap();
        db.claim_period(user, Period::Monthly, now).await.unwrap();
        db.give_rep(user, other).await.unwrap();
        let mut claims = Vec::new();
        for i in 0..per_round {
            let db = db.clone();
            let at = std::time::Duration::from_micros(span_us * i / per_round);
            claims.push(tokio::spawn(async move {
                tokio::time::sleep(at).await;
                match i % 4 {
                    0 => db.claim_daily(user).await.map(|o| match o {
                        DailyOutcome::Claimed { amount, .. } => amount,
                        DailyOutcome::Cooldown { .. } => 0,
                    }),
                    1 => db
                        .claim_period(user, Period::Weekly, Utc::now())
                        .await
                        .map(|o| match o {
                            PeriodOutcome::Claimed { amount, .. } => amount,
                            PeriodOutcome::Cooldown { .. } => 0,
                        }),
                    2 => db
                        .claim_period(user, Period::Monthly, Utc::now())
                        .await
                        .map(|o| match o {
                            PeriodOutcome::Claimed { amount, .. } => amount,
                            PeriodOutcome::Cooldown { .. } => 0,
                        }),
                    _ => db.give_rep(user, other).await.map(|o| match o {
                        RepOutcome::Given { .. } => 1,
                        RepOutcome::Cooldown { .. } => 0,
                    }),
                }
            }));
        }
        db.delete_user_data(user, Utc::now()).await.unwrap();
        for claim in claims {
            match claim.await.unwrap() {
                Ok(0) => {}
                Ok(amount) => {
                    paid += 1;
                    shells += amount;
                }
                Err(error) => errors.push(format!("round {round}: {error}")),
            }
        }
        db.delete_user_data(user, Utc::now()).await.unwrap();
        db.delete_user_data(other, Utc::now()).await.unwrap();
    }
    assert_eq!(
        (paid, shells),
        (0, 0),
        "claims that came in during a delete were paid again"
    );
    assert!(errors.is_empty(), "claims failed: {errors:#?}");
    db.pool().close().await;
}

#[tokio::test]
async fn tracker_unlink_erases_owned_cards_and_patch_claims_are_atomic() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let [user, other, guild] = ids();
    let key = format!("riot:card:{user}");
    let other_key = format!("riot:card:{other}");
    db.tracker_user_cache_put(user, &key, &serde_json::json!({"title":"Private#TR"}))
        .await
        .unwrap();
    db.tracker_user_cache_put(other, &other_key, &serde_json::json!({"title":"Other#TR"}))
        .await
        .unwrap();
    db.save_riot_link(
        user,
        "lol",
        &RiotLink {
            puuid: "private".into(),
            game_name: "Private".into(),
            tag_line: "TR".into(),
            region: "tr1".into(),
        },
    )
    .await
    .unwrap();
    let exported = db.tracker_user_data(user).await.unwrap();
    assert_eq!(exported["riot_links"].as_array().unwrap().len(), 1);
    assert_eq!(exported["cards"], serde_json::json!([{"title":"Private#TR"}]));
    assert!(db.delete_riot_link(user, "lol").await.unwrap());
    assert_eq!(
        db.tracker_user_data(user).await.unwrap(),
        serde_json::json!({"riot_links":[],"cards":[]})
    );
    assert!(db.riot_link(user, "lol").await.unwrap().is_none());
    assert!(db.tracker_cached(&key, 300).await.unwrap().is_none());
    assert!(db.tracker_cached(&other_key, 300).await.unwrap().is_some());
    let destination = format!("lol:{guild}:123");
    let (first, second) = tokio::join!(
        db.patch_mark_seen(&destination, "release"),
        db.patch_mark_seen(&destination, "release")
    );
    assert_ne!(first.unwrap(), second.unwrap());
    assert!(!db.patch_mark_seen(&destination, "release").await.unwrap());
    assert!(
        db.patch_mark_seen(&format!("lol:{guild}:456"), "release")
            .await
            .unwrap()
    );
    let mut settings = db.tracker_settings(guild).await.unwrap();
    settings.mc_watch = serde_json::json!([{"host":"example.com:25565","channel_id":"123"}]);
    db.save_tracker_settings(guild, &settings).await.unwrap();
    db.tracker_watch_message(guild, "example.com:25565", 456, 789)
        .await
        .unwrap();
    assert!(db.tracker_settings(guild).await.unwrap().mc_watch[0]["message_id"].is_null());
    db.tracker_watch_message(guild, "example.com:25565", 123, 789)
        .await
        .unwrap();
    db.save_tracker_settings(guild, &settings).await.unwrap();
    assert_eq!(
        db.tracker_settings(guild).await.unwrap().mc_watch[0]["message_id"],
        "789"
    );
    settings.mc_watch[0]["channel_id"] = serde_json::json!("456");
    settings.mc_watch[0]["message_id"] = serde_json::json!("123456");
    db.save_tracker_settings(guild, &settings).await.unwrap();
    assert!(db.tracker_settings(guild).await.unwrap().mc_watch[0]["message_id"].is_null());
    settings.mc_watch = serde_json::json!((0..21).map(|_| serde_json::json!({})).collect::<Vec<_>>());
    assert!(db.save_tracker_settings(guild, &settings).await.is_err());
}
