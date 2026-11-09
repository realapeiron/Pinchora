use super::*;

#[tokio::test]
async fn a_heist_pays_the_crew_once_up_to_their_daily_limit() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, channel, a, b, c] = ids();
    let id = lobby(&db, "heist", guild, channel, a, 0).await;
    assert_eq!(db.heist_join(id, a, "A", 0, 3).await.unwrap(), CrewJoin::Joined);
    assert_eq!(db.heist_join(id, b, "B", 1, 3).await.unwrap(), CrewJoin::Joined);
    assert_eq!(db.heist_join(id, b, "B", 2, 3).await.unwrap(), CrewJoin::Moved);
    assert_eq!(db.heist_join(id, c, "C", 1, 3).await.unwrap(), CrewJoin::Joined);
    assert_eq!(
        db.heist_join(id, ids::<1>()[0] + 99, "D", 1, 3).await.unwrap(),
        CrewJoin::Full
    );
    assert!(db.heist_leave(id, c).await.unwrap());
    assert!(!db.heist_leave(id, c).await.unwrap());

    // A day's limit of 70 covers one 60 haul and 10 of the next.
    let HeistSettled::Done { each, paid, crew, .. } = db
        .heist_settle(id, Some(a), 2, 70, |_, crew| {
            assert_eq!(crew.len(), 2);
            60
        })
        .await
        .unwrap()
        .expect("the crew sets off")
    else {
        panic!("a crew of two is enough");
    };
    assert_eq!((each, crew.len()), (60, 2));
    assert_eq!(paid, vec![(a, 60), (b, 60)]);
    assert!(
        db.heist_settle(id, Some(a), 2, 70, |_, _| 60).await.unwrap().is_none(),
        "paid once"
    );

    let next = lobby(&db, "heist", guild, channel, a, 0).await;
    db.heist_join(next, a, "A", 0, 8).await.unwrap();
    db.heist_join(next, b, "B", 1, 8).await.unwrap();
    run_out(&db, next).await;
    let HeistSettled::Done { paid, .. } = db
        .heist_settle(next, None, 2, 70, |_, _| 60)
        .await
        .unwrap()
        .expect("late lobbies go")
    else {
        panic!("a crew of two is enough");
    };
    assert_eq!(paid, vec![(a, 10), (b, 10)], "only what is left of the day's 70");
    assert_eq!(shells(&db, a).await, 70);
    assert_eq!(db.paid_today(a, "heist").await.unwrap(), 70);
    assert_eq!(ledger_total(&db, &[a, b]).await, 140);
}

#[tokio::test]
async fn a_heist_with_too_few_in_the_crew_is_called_off() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, channel, a] = ids();
    let id = lobby(&db, "heist", guild, channel, a, 0).await;
    db.heist_join(id, a, "A", 0, 8).await.unwrap();
    run_out(&db, id).await;
    let settled = db
        .heist_settle(id, None, 2, 100, |_, _| 99)
        .await
        .unwrap()
        .expect("ends");
    assert!(matches!(settled, HeistSettled::CalledOff { .. }));
    assert_eq!(shells(&db, a).await, 0);
    assert_eq!(db.game(id).await.unwrap().unwrap().status, DONE);
}

#[tokio::test]
async fn a_storm_counts_sandbags_per_helper_and_pays_the_helpers_once() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, channel, a, b] = ids();
    let storm = db
        .open_party_game(NewGame {
            kind: "surge",
            guild_id: guild,
            channel_id: channel,
            host: a,
            guest: None,
            bet: 0,
            status: ACTIVE,
            state: "0:0:0",
            ttl_secs: 600,
        })
        .await
        .unwrap()
        .expect("no storm yet");
    let SurgeHelp::Helped {
        mine, stacked, helpers, ..
    } = db.surge_help(storm.id, a, "A", 3).await.unwrap()
    else {
        panic!("the storm is running");
    };
    assert_eq!((mine, stacked, helpers), (1, 1, 1));
    db.surge_help(storm.id, a, "A", 3).await.unwrap();
    db.surge_help(storm.id, b, "B", 3).await.unwrap();
    let SurgeHelp::Helped {
        mine, stacked, helpers, ..
    } = db.surge_help(storm.id, a, "A", 3).await.unwrap()
    else {
        panic!("a third sandbag");
    };
    assert_eq!((mine, stacked, helpers), (3, 4, 2));
    assert!(matches!(
        db.surge_help(storm.id, a, "A", 3).await.unwrap(),
        SurgeHelp::Capped
    ));
    assert_eq!(db.surge_totals(storm.id).await.unwrap(), (4, 2));

    assert!(
        db.surge_settle(storm.id, false, true, 100, |_, _| 5)
            .await
            .unwrap()
            .is_none(),
        "a storm with time left cannot be lost"
    );
    let settled = db
        .surge_settle(storm.id, true, false, 100, |held, helps| {
            assert!(held);
            10 + helps
        })
        .await
        .unwrap()
        .expect("the wall holds");
    let paid: Vec<(i64, i64)> = settled.helpers.iter().map(|(m, got)| (m.amount, *got)).collect();
    assert_eq!(paid, vec![(3, 13), (1, 11)]);
    assert_eq!((shells(&db, a).await, shells(&db, b).await), (13, 11));
    assert!(
        db.surge_settle(storm.id, true, false, 100, |_, _| 10)
            .await
            .unwrap()
            .is_none(),
        "paid once"
    );
    assert!(matches!(
        db.surge_help(storm.id, b, "B", 3).await.unwrap(),
        SurgeHelp::Over
    ));
    assert_eq!(ledger_total(&db, &[a, b]).await, 24);
}

#[tokio::test]
async fn a_storm_that_runs_out_of_time_pays_the_consolation_and_a_daily_limit_applies() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guild, channel, a] = ids();
    let mut total = 0;
    for round in 0..3 {
        let storm = db
            .open_party_game(NewGame {
                kind: "surge",
                guild_id: guild,
                channel_id: channel + round,
                host: a,
                guest: None,
                bet: 0,
                status: ACTIVE,
                state: "0:0:0",
                ttl_secs: 600,
            })
            .await
            .unwrap();
        // One storm per server: the next round only starts once the last is over.
        let storm = storm.expect("the previous storm ended");
        db.surge_help(storm.id, a, "A", 40).await.unwrap();
        run_out(&db, storm.id).await;
        assert!(
            matches!(db.surge_help(storm.id, a, "A", 40).await.unwrap(), SurgeHelp::Over),
            "out of time"
        );
        let settled = db
            .surge_settle(storm.id, false, true, 25, |_, _| 10)
            .await
            .unwrap()
            .expect("late storms end");
        assert!(!settled.held);
        total += settled.helpers[0].1;
    }
    assert_eq!(total, 25, "10 + 10 + the 5 left of the day's 25");
    assert_eq!(shells(&db, a).await, 25);
}

#[tokio::test]
async fn storm_settings_schedule_the_next_storm_and_one_pod_takes_each_slot() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db().await else { return };
    let [guild] = ids();
    let defaults = db.party_settings(guild).await.unwrap();
    assert!(!defaults.surge_enabled && defaults.surge_channel_id.is_none() && defaults.surge_every_hours == 24);
    let now = Utc::now();
    let saved = db.set_surge(guild, Some(true), None, None, now).await.unwrap();
    assert!(
        saved.surge_enabled && saved.surge_next_at.is_none(),
        "no channel, nothing to schedule"
    );
    let saved = db.set_surge(guild, None, Some(77), Some(12), now).await.unwrap();
    assert_eq!((saved.surge_channel_id, saved.surge_every_hours), (Some(77), 12));
    let next = saved.surge_next_at.expect("scheduled");
    let off_by = (next - (now + Duration::hours(12))).num_milliseconds().abs();
    assert!(off_by < 5, "scheduled 12 hours out, microseconds aside: {off_by} ms");
    assert!(
        db.due_surges(now, 100)
            .await
            .unwrap()
            .iter()
            .all(|s| s.guild_id != guild as i64)
    );
    let later = now + Duration::hours(13);
    let due = db.due_surges(later, 1_000).await.unwrap();
    let mine = due
        .iter()
        .find(|s| s.guild_id == guild as i64)
        .expect("due after 12 hours");
    let slot = mine.surge_next_at.expect("a due time");
    let (first, second) = tokio::join!(
        db.claim_surge_slot(guild, slot, later),
        db.claim_surge_slot(guild, slot, later)
    );
    assert_eq!(
        [first.unwrap(), second.unwrap()].iter().filter(|won| **won).count(),
        1,
        "exactly one pod starts the storm"
    );
    assert!(
        db.due_surges(later, 1_000)
            .await
            .unwrap()
            .iter()
            .all(|s| s.guild_id != guild as i64)
    );
    let off = db.set_surge(guild, Some(false), None, None, later).await.unwrap();
    assert!(off.surge_next_at.is_none(), "switching it off cancels the next storm");
}
