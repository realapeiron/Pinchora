use super::*;

#[tokio::test]
async fn weekly_and_monthly_rewards_have_their_own_cooldowns() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [user] = ids();
    let now = micros(Utc::now());

    let PeriodOutcome::Claimed {
        amount,
        balance,
        next_at,
    } = db.claim_period(user, Period::Weekly, now).await.unwrap()
    else {
        panic!("the first weekly pays");
    };
    assert_eq!((amount, balance), (Period::Weekly.reward(), Period::Weekly.reward()));
    assert_eq!(next_at, now + Duration::days(7));
    assert!(matches!(
        db.claim_period(user, Period::Weekly, now + Duration::days(6)).await.unwrap(),
        PeriodOutcome::Cooldown { next_at: at } if at == next_at
    ));
    // The monthly reward does not care about the weekly one.
    assert!(matches!(
        db.claim_period(user, Period::Monthly, now + Duration::days(1))
            .await
            .unwrap(),
        PeriodOutcome::Claimed { .. }
    ));
    assert!(matches!(
        db.claim_period(user, Period::Monthly, now + Duration::days(29))
            .await
            .unwrap(),
        PeriodOutcome::Cooldown { .. }
    ));
    let PeriodOutcome::Claimed { balance, .. } = db
        .claim_period(user, Period::Weekly, now + Duration::days(7))
        .await
        .unwrap()
    else {
        panic!("a week later it pays again");
    };
    assert_eq!(balance, 2 * Period::Weekly.reward() + Period::Monthly.reward());
    assert_eq!(
        db.period_claimed_at(user, Period::Weekly).await.unwrap(),
        Some(now + Duration::days(7))
    );

    // Claims at the same moment pay once.
    let [racer] = ids();
    let claims = futures_join(&db, racer, now).await;
    assert_eq!(
        claims
            .iter()
            .filter(|c| matches!(c, PeriodOutcome::Claimed { .. }))
            .count(),
        1
    );
    assert_eq!(db.user(racer).await.unwrap().credits, Period::Weekly.reward());

    // Every reward is in the ledger, and counted as shells earned this month.
    let (ledger,): (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM credit_ledger WHERE user_id = $1")
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(ledger, 2 * Period::Weekly.reward() + Period::Monthly.reward());
}

/// What deleting data must not be: a way to get a reward, a rep, a paid visit, a wordle payout or
/// a walk a second time.
#[tokio::test]
async fn deleting_and_coming_back_resets_no_cooldown() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db().await else { return };
    let [user, alt, other_alt] = ids();
    let now = micros(Utc::now());
    let weekly = |at| db.claim_period(user, Period::Weekly, at);
    let monthly = |at| db.claim_period(user, Period::Monthly, at);

    // The loop: collect everything, hand it to an alt, delete, come back and do it again.
    let mut legit = 0;
    for round in 0..4 {
        let daily = db.claim_daily(user).await.unwrap();
        let week = weekly(now).await.unwrap();
        let month = monthly(now).await.unwrap();
        let rep = db.give_rep(user, other_alt).await.unwrap();
        if round == 0 {
            assert!(matches!(daily, DailyOutcome::Claimed { .. }), "{daily:?}");
            assert!(matches!(week, PeriodOutcome::Claimed { .. }), "{week:?}");
            assert!(matches!(month, PeriodOutcome::Claimed { .. }), "{month:?}");
            assert!(matches!(rep, RepOutcome::Given { .. }), "{rep:?}");
            legit = daily_reward(1) + Period::Weekly.reward() + Period::Monthly.reward();
        } else {
            assert!(
                matches!(daily, DailyOutcome::Cooldown { .. }),
                "round {round}: {daily:?}"
            );
            assert!(
                matches!(week, PeriodOutcome::Cooldown { .. }),
                "round {round}: {week:?}"
            );
            assert!(
                matches!(month, PeriodOutcome::Cooldown { .. }),
                "round {round}: {month:?}"
            );
            assert!(matches!(rep, RepOutcome::Cooldown { .. }), "round {round}: {rep:?}");
        }
        let balance = db.user(user).await.unwrap().credits;
        if balance > 0 {
            db.transfer_credits(user, alt, balance).await.unwrap();
        }
        db.delete_user_data(user, now).await.unwrap();
        assert_eq!(db.user(user).await.unwrap().credits, 0, "they come back with nothing");
    }
    assert_eq!(
        db.user(alt).await.unwrap().credits,
        legit,
        "one legitimate cycle, not four"
    );
    assert_eq!(db.user(other_alt).await.unwrap().reputation, 1, "one rep, not four");
    let (rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM credit_ledger WHERE user_id = $1 AND delta > 0")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0, "nothing was paid to them after coming back");

    // The wallet reads the cooldowns off the row, and they are on it.
    let back = db.user(user).await.unwrap();
    assert!(back.last_daily_at.is_some() && back.last_rep_at.is_some());
    assert_eq!(db.period_claimed_at(user, Period::Weekly).await.unwrap(), Some(now));

    // Time still passes: a day later the daily and the rep work again, a week later the weekly one,
    // a month later the monthly one.
    sqlx::query(
        "UPDATE deleted_users SET last_daily_at = last_daily_at - interval '30 hours',
                                  last_rep_at = last_rep_at - interval '30 hours' WHERE user_id = $1",
    )
    .bind(user as i64)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE users SET last_daily_at = last_daily_at - interval '30 hours', last_rep_at = last_rep_at - interval '30 hours' WHERE id = $1")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(matches!(
        db.claim_daily(user).await.unwrap(),
        DailyOutcome::Claimed { .. }
    ));
    assert!(matches!(
        db.give_rep(user, other_alt).await.unwrap(),
        RepOutcome::Given { .. }
    ));
    assert!(matches!(
        weekly(now + Duration::days(6)).await.unwrap(),
        PeriodOutcome::Cooldown { .. }
    ));
    assert!(matches!(
        weekly(now + Duration::days(8)).await.unwrap(),
        PeriodOutcome::Claimed { .. }
    ));
    assert!(matches!(
        monthly(now + Duration::days(29)).await.unwrap(),
        PeriodOutcome::Cooldown { .. }
    ));
    assert!(matches!(
        monthly(now + Duration::days(31)).await.unwrap(),
        PeriodOutcome::Claimed { .. }
    ));

    // Deleting twice keeps the later time. Even an empty player needs a tombstone so a pending
    // stats batch cannot restore their activity.
    db.delete_user_data(user, now).await.unwrap();
    db.delete_user_data(user, now + Duration::hours(1)).await.unwrap();
    assert!(matches!(
        db.claim_daily(user).await.unwrap(),
        DailyOutcome::Cooldown { .. }
    ));
    let [nobody] = ids();
    db.user(nobody).await.unwrap();
    db.delete_user_data(nobody, now).await.unwrap();
    assert_eq!(count(&db, "deleted_users", "user_id", nobody).await, 1);

    // Tombstones are dropped when the longest cooldown is long over.
    sqlx::query("UPDATE deleted_users SET deleted_at = $2 WHERE user_id = $1")
        .bind(user as i64)
        .bind(now - Duration::days(36))
        .execute(db.pool())
        .await
        .unwrap();
    assert!(db.forget_old_tombstones(now).await.unwrap() >= 1);
    assert_eq!(count(&db, "deleted_users", "user_id", user).await, 0);
    db.delete_user_data(user, now).await.unwrap();
    db.delete_user_data(alt, now).await.unwrap();
    db.delete_user_data(other_alt, now).await.unwrap();
}

/// The other limits that sit in rows: paid visits, likes, the daily wordle and the pet walk.
#[tokio::test]
async fn paid_visits_likes_wordle_and_walks_survive_deleting() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [guest, h1, h2, h3, liker, host] = ids();
    let now = micros(Utc::now());
    let day = now.date_naive();
    let (reward, cap) = (25, 2);
    let paid = |outcome: VisitOutcome| match outcome {
        VisitOutcome::Rewarded { reward } => reward,
        VisitOutcome::AlreadyToday | VisitOutcome::Unavailable => -1,
    };

    // A guest is paid for two houses a day, whoever deletes what afterwards.
    assert_eq!(paid(db.record_visit(h1, guest, day, reward, cap).await.unwrap()), 25);
    assert_eq!(paid(db.record_visit(h2, guest, day, reward, cap).await.unwrap()), 25);
    assert_eq!(
        paid(db.record_visit(h3, guest, day, reward, cap).await.unwrap()),
        0,
        "the cap"
    );
    // The hosts delete their data and come back: the guest's visits of the day still count.
    for host in [h1, h2, h3] {
        db.delete_user_data(host, now).await.unwrap();
    }
    assert_eq!(
        count(&db, "visits", "host_id", h1).await,
        0,
        "the host's id is gone from the visit"
    );
    assert_eq!(
        paid(db.record_visit(h1, guest, day, reward, cap).await.unwrap()),
        0,
        "the host starting over does not reset the guest's day"
    );
    // And the guest deleting does not either.
    db.delete_user_data(guest, now).await.unwrap();
    assert_eq!(
        paid(db.record_visit(h2, guest, day, reward, cap).await.unwrap()),
        0,
        "the guest starting over does not reset their day"
    );
    // Tomorrow is a new day.
    assert_eq!(
        paid(
            db.record_visit(h2, guest, day + Duration::days(1), reward, cap)
                .await
                .unwrap()
        ),
        25
    );
    // The Wrapped numbers never point at a deleted host.
    assert_eq!(
        db.most_visited_house(guest, chrono::Datelike::year(&day))
            .await
            .unwrap()
            .map(|(h, _, _)| h),
        Some(h2)
    );

    // Likes: the one given today is still given, and a deleted liker's like comes off the house.
    sqlx::query("INSERT INTO houses (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(host as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(db.like_house(host, liker, day).await.unwrap(), Some(1));
    db.delete_user_data(liker, now).await.unwrap();
    let (likes,): (i32,) = sqlx::query_as("SELECT likes FROM houses WHERE user_id = $1")
        .bind(host as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(likes, 0, "the deleted player's like came off the house");
    assert_eq!(
        db.like_house(host, liker, day).await.unwrap(),
        None,
        "today's like is already given"
    );
    assert_eq!(
        db.like_house(host, liker, day + Duration::days(1)).await.unwrap(),
        Some(1),
        "tomorrow they can like again"
    );

    // The daily wordle pays once a day; a board deleted with the data is still played.
    let [player] = ids();
    db.start_wordle(player, "daily", 20_000, "coral", false).await.unwrap();
    db.delete_user_data(player, now).await.unwrap();
    assert!(db.daily_wordle_played_before_deleting(player, 20_000).await.unwrap());
    assert!(!db.daily_wordle_played_before_deleting(player, 20_001).await.unwrap());

    // The walk's cooldown follows the player into a new pet.
    let [walker] = ids();
    db.adopt(walker, "crab", "Pinchy").await.unwrap();
    assert_eq!(db.walk_locked_until(walker, now).await.unwrap(), None);
    db.delete_user_data(walker, now).await.unwrap();
    let until = db.walk_locked_until(walker, now).await.unwrap().expect("a walk lock");
    assert_eq!(until, now + Duration::minutes(45));
    assert_eq!(
        db.walk_locked_until(walker, now + Duration::minutes(46)).await.unwrap(),
        None
    );

    for user in [guest, h1, h2, h3, liker, host, player, walker] {
        db.delete_user_data(user, now + Duration::days(40)).await.unwrap();
    }
}

/// The games' limits: a capped party payout, the beachcomb wait and its paid plays of the day, and the
/// daily picross. The same loop as above (collect, hand it to an alt, delete, come back) earns no more
/// than a player who never deleted anything.
#[tokio::test]
async fn the_games_limits_survive_deleting() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [user, alt, solver, idler] = ids();
    // Early in today (UTC), so the walks below stay on the day the heist cap counts in.
    let now = micros(Utc::now().date_naive().and_hms_opt(0, 30, 0).unwrap().and_utc());
    let day = now.timestamp().div_euclid(86_400) as i32;
    let wait = Duration::minutes(20);
    let award = Award {
        entry: "sea_glass",
        new_shells: 30,
        dup_shells: 5,
        dup_fragments: 1,
        bonus_fragments: 0,
    };
    let daily = Daily {
        game: "beachcomb",
        day,
        cap: 2,
    };
    let paid = |comb: Comb| match comb {
        Comb::Found { paid, .. } => Some(paid),
        Comb::Wait { .. } => None,
    };

    // The heist pays up to its cap a day, deleted or not.
    assert_eq!(
        db.grant_capped(user, "heist", 150, 100, "heist_loot").await.unwrap(),
        100
    );
    assert_eq!(
        paid(db.beachcomb(user, now, wait, award, daily).await.unwrap()),
        Some(true)
    );
    hand_over(&db, user, alt, now).await;
    assert_eq!(
        db.grant_capped(user, "heist", 150, 100, "heist_loot").await.unwrap(),
        0,
        "the cap"
    );
    assert_eq!(
        paid(db.beachcomb(user, now, wait, award, daily).await.unwrap()),
        None,
        "the wait is still on"
    );
    hand_over(&db, user, alt, now).await;
    // After the wait the second walk of the day pays, as it would have anyway; the third does not.
    let later = now + Duration::minutes(21);
    assert_eq!(
        paid(db.beachcomb(user, later, wait, award, daily).await.unwrap()),
        Some(true)
    );
    hand_over(&db, user, alt, later).await;
    let latest = later + Duration::minutes(21);
    assert_eq!(
        paid(db.beachcomb(user, latest, wait, award, daily).await.unwrap()),
        Some(false),
        "two walks pay a day"
    );
    hand_over(&db, user, alt, latest).await;
    // The dex went with the data, so the find counted as new both times it paid: no more walks
    // paid than the day allows, whatever they were worth.
    assert_eq!(db.user(alt).await.unwrap().credits, 100 + 30 + 30);
    let (rows,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM credit_ledger WHERE user_id = $1 AND reason IN ('heist_loot', 'beachcomb')",
    )
    .bind(user as i64)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(rows, 0, "nothing of theirs is left in the ledger");

    // The daily picross pays once a day; a solved board deleted with the data is still solved.
    for player in [solver, idler] {
        db.start_picross(player, "daily", day, (0, 0), false).await.unwrap();
    }
    let blank = ".".repeat(100);
    db.picross_marks(solver, "daily", day, &blank, &"#".repeat(100), true)
        .await
        .unwrap()
        .expect("solved");
    db.delete_user_data(solver, now).await.unwrap();
    db.delete_user_data(idler, now).await.unwrap();
    assert!(db.daily_picross_solved_before_deleting(solver, day).await.unwrap());
    assert!(!db.daily_picross_solved_before_deleting(solver, day + 1).await.unwrap());
    assert!(
        !db.daily_picross_solved_before_deleting(idler, day).await.unwrap(),
        "a board started but not solved was not paid"
    );

    for player in [user, alt, solver, idler] {
        db.delete_user_data(player, now + Duration::days(40)).await.unwrap();
    }
}
