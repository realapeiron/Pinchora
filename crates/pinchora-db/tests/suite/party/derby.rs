use crate::party::*;

#[tokio::test]
async fn a_staked_derby_holds_stakes_and_the_winners_share_the_pool() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let [guild, channel, a, b, c, d] = ids();
    for user in [a, b, c, d] {
        db.earn_shells(user, 100, "test").await.unwrap();
    }
    let id = lobby(&db, "derby", guild, channel, a, 40).await;
    assert_eq!(db.derby_join(id, a, "A", 0, 30, LOSS).await.unwrap(), DerbyJoin::Joined);
    assert_eq!(db.derby_join(id, b, "B", 1, 30, LOSS).await.unwrap(), DerbyJoin::Joined);
    assert_eq!(db.derby_join(id, c, "C", 1, 30, LOSS).await.unwrap(), DerbyJoin::Joined);
    assert_eq!(db.derby_join(id, d, "D", 2, 30, LOSS).await.unwrap(), DerbyJoin::Joined);
    assert_eq!(shells(&db, a).await, 60, "the stake is held");
    // Picking again moves the crab and charges nothing.
    assert_eq!(db.derby_join(id, d, "D", 1, 30, LOSS).await.unwrap(), DerbyJoin::Moved);
    assert_eq!(shells(&db, d).await, 60);
    assert_eq!(db.party_members(id).await.unwrap().len(), 4);

    assert!(
        db.derby_settle(id, None, 2, 1).await.unwrap().is_none(),
        "the lobby still has time and nobody started it"
    );
    assert!(
        db.derby_settle(id, Some(b), 2, 1).await.unwrap().is_none(),
        "only the host starts it early"
    );
    let result = db
        .derby_settle(id, Some(a), 2, 1)
        .await
        .unwrap()
        .expect("the host starts the race");
    assert_eq!(result.winner, 1);
    // Pool 160 between B, C and D who all backed crab 1: 54, 53, 53.
    let DerbyOutcome::Paid(paid) = &result.outcome else {
        panic!("{:?}", result.outcome);
    };
    assert_eq!(
        paid.iter().map(|(_, s)| s).sum::<i64>(),
        160,
        "the whole pool is paid out"
    );
    assert_eq!(paid.iter().map(|(u, _)| *u).collect::<Vec<_>>(), vec![b, c, d]);
    assert_eq!(shells(&db, a).await, 60, "the loser lost the stake");
    assert_eq!(
        shells(&db, b).await + shells(&db, c).await + shells(&db, d).await,
        60 * 3 + 160
    );
    assert_eq!(
        ledger_total(&db, &[a, b, c, d]).await,
        400,
        "shells only moved between the racers"
    );
    assert!(
        db.derby_settle(id, Some(a), 2, 1).await.unwrap().is_none(),
        "no second payout"
    );
    assert_eq!(db.game(id).await.unwrap().unwrap().status, DONE);
    assert_eq!(db.derby_join(id, a, "A", 3, 30, LOSS).await.unwrap(), DerbyJoin::Closed);
}

#[tokio::test]
async fn a_derby_nobody_won_or_too_few_joined_gives_every_stake_back() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let [guild, channel, other, a, b, c] = ids();
    for user in [a, b, c] {
        db.earn_shells(user, 100, "test").await.unwrap();
    }
    // Nobody backed the winning crab.
    let id = lobby(&db, "derby", guild, channel, a, 25).await;
    db.derby_join(id, a, "A", 0, 30, LOSS).await.unwrap();
    db.derby_join(id, b, "B", 1, 30, LOSS).await.unwrap();
    db.derby_join(id, c, "C", 2, 30, LOSS).await.unwrap();
    let result = db.derby_settle(id, Some(a), 2, 5).await.unwrap().expect("settled");
    assert_eq!(result.outcome, DerbyOutcome::Refunded);
    assert_eq!(
        (shells(&db, a).await, shells(&db, b).await, shells(&db, c).await),
        (100, 100, 100)
    );

    // A race with shells on it needs two players; the lone racer is refunded.
    let id = lobby(&db, "derby", guild, other, a, 25).await;
    db.derby_join(id, a, "A", 0, 30, LOSS).await.unwrap();
    assert_eq!(shells(&db, a).await, 75);
    run_out(&db, id).await;
    let result = db
        .derby_settle(id, None, 2, 0)
        .await
        .unwrap()
        .expect("late lobbies run");
    assert_eq!(result.outcome, DerbyOutcome::Cancelled);
    assert_eq!(shells(&db, a).await, 100);

    // An empty lobby just closes.
    let id = lobby(&db, "derby", guild, other, a, 0).await;
    run_out(&db, id).await;
    let result = db.derby_settle(id, None, 2, 0).await.unwrap().expect("settled");
    assert_eq!(result.outcome, DerbyOutcome::Cancelled);
    assert_eq!(ledger_total(&db, &[a, b, c]).await, 300);
}

#[tokio::test]
async fn a_friendly_derby_moves_no_shells_and_names_the_winning_backers() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let [guild, channel, a, b] = ids();
    let id = lobby(&db, "derby", guild, channel, a, 0).await;
    db.derby_join(id, a, "A", 3, 30, LOSS).await.unwrap();
    db.derby_join(id, b, "B", 4, 30, LOSS).await.unwrap();
    let result = db.derby_settle(id, Some(a), 2, 4).await.unwrap().expect("settled");
    assert_eq!(result.outcome, DerbyOutcome::Friendly(vec![b]));
    assert_eq!((shells(&db, a).await, shells(&db, b).await), (0, 0));
}

#[tokio::test]
async fn leaving_a_derby_refunds_and_a_short_wallet_cannot_join() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let [guild, channel, a, broke] = ids();
    db.earn_shells(a, 100, "test").await.unwrap();
    db.earn_shells(broke, 10, "test").await.unwrap();
    let id = lobby(&db, "derby", guild, channel, a, 50).await;
    assert_eq!(
        db.derby_join(id, broke, "Broke", 0, 30, LOSS).await.unwrap(),
        DerbyJoin::Broke { balance: 10 }
    );
    assert_eq!(db.derby_leave(id, broke).await.unwrap(), DerbyLeave::NotIn);
    db.derby_join(id, a, "A", 0, 30, LOSS).await.unwrap();
    assert_eq!(shells(&db, a).await, 50);
    assert_eq!(db.derby_leave(id, a).await.unwrap(), DerbyLeave::Left { refund: 50 });
    assert_eq!(shells(&db, a).await, 100);
    assert_eq!(
        db.derby_leave(id, a).await.unwrap(),
        DerbyLeave::NotIn,
        "a refund is paid once"
    );
    // The lobby holds at most `max` racers.
    db.earn_shells(broke, 100, "test").await.unwrap();
    assert_eq!(db.derby_join(id, a, "A", 0, 1, LOSS).await.unwrap(), DerbyJoin::Joined);
    assert_eq!(
        db.derby_join(id, broke, "B", 0, 1, LOSS).await.unwrap(),
        DerbyJoin::Full
    );
    assert_eq!(shells(&db, broke).await, 110, "a full lobby takes nothing");
}

#[tokio::test]
async fn a_channel_holds_one_derby_and_one_heist_and_a_server_one_storm() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let [guild, channel, host] = ids();
    let first = lobby(&db, "derby", guild, channel, host, 0).await;
    let again = NewGame {
        kind: "derby",
        guild_id: guild,
        channel_id: channel,
        host,
        guest: None,
        bet: 0,
        status: PENDING,
        state: "1",
        ttl_secs: 60,
    };
    assert!(
        db.open_party_game(again).await.unwrap().is_none(),
        "one derby per channel"
    );
    assert!(
        db.open_party_game(NewGame { kind: "heist", ..again })
            .await
            .unwrap()
            .is_some(),
        "a heist is another game"
    );
    run_out(&db, first).await;
    db.derby_settle(first, None, 2, 0).await.unwrap().expect("ends");
    assert!(
        db.open_party_game(again).await.unwrap().is_some(),
        "a finished derby frees the channel"
    );
    let storm = NewGame {
        kind: "surge",
        status: ACTIVE,
        state: "0:0:0",
        ..again
    };
    assert!(db.open_party_game(storm).await.unwrap().is_some());
    assert!(
        db.open_party_game(NewGame {
            channel_id: channel + 1,
            ..storm
        })
        .await
        .unwrap()
        .is_none(),
        "one storm per server, whatever the channel"
    );
}

#[tokio::test]
async fn simultaneous_derby_endings_pay_once() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let [guild, channel, a, b] = ids();
    for user in [a, b] {
        db.earn_shells(user, 100, "test").await.unwrap();
    }
    let id = lobby(&db, "derby", guild, channel, a, 30).await;
    db.derby_join(id, a, "A", 2, 30, LOSS).await.unwrap();
    db.derby_join(id, b, "B", 2, 30, LOSS).await.unwrap();
    run_out(&db, id).await;
    let tries = (0..6).map(|_| {
        let db = db.clone();
        async move { db.derby_settle(id, None, 2, 2).await }
    });
    let results = futures_join(tries).await;
    assert_eq!(results.iter().filter(|r| r.is_some()).count(), 1, "one ending owns it");
    assert_eq!(shells(&db, a).await + shells(&db, b).await, 200);
    assert_eq!(ledger_total(&db, &[a, b]).await, 200);
}

#[tokio::test]
async fn derby_stakes_count_against_a_daily_loss_limit_until_they_come_back() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let [guild, c1, c2, c3, c4, c5, player, other, host] = ids();
    db.earn_shells(player, 2_000, "test").await.unwrap();
    db.earn_shells(other, 2_000, "test").await.unwrap();
    let net = |user: u64| {
        let db = db.clone();
        async move { db.table_today(user, "derby").await.unwrap().1 }
    };
    let (a, b, c) = (
        lobby(&db, "derby", guild, c1, host, 200).await,
        lobby(&db, "derby", guild, c2, host, 200).await,
        lobby(&db, "derby", guild, c3, host, 200).await,
    );
    // A limit of 500: two stakes of 200 fit, a third (to 600 down) does not.
    assert_eq!(
        db.derby_join(a, player, "P", 0, 30, 500).await.unwrap(),
        DerbyJoin::Joined
    );
    assert_eq!(
        db.derby_join(b, player, "P", 0, 30, 500).await.unwrap(),
        DerbyJoin::Joined
    );
    assert_eq!(net(player).await, -400);
    assert_eq!(
        db.derby_join(c, player, "P", 0, 30, 500).await.unwrap(),
        DerbyJoin::Down { net: -400 }
    );
    assert_eq!(shells(&db, player).await, 1_600, "a refused stake takes nothing");
    assert_eq!(db.party_members(c).await.unwrap().len(), 0);
    // Moving to another crab is free and counts nothing more.
    assert_eq!(
        db.derby_join(a, player, "P", 3, 30, 500).await.unwrap(),
        DerbyJoin::Moved
    );
    assert_eq!(net(player).await, -400);
    // Leaving gives the stake back, and the room under the limit with it.
    assert_eq!(
        db.derby_leave(a, player).await.unwrap(),
        DerbyLeave::Left { refund: 200 }
    );
    assert_eq!(net(player).await, -200);
    assert_eq!(
        db.derby_join(c, player, "P", 0, 30, 500).await.unwrap(),
        DerbyJoin::Joined
    );
    assert_eq!(net(player).await, -400);

    // A lost race stays lost; the winner's share is counted in; a race nobody backed gives it all back.
    db.derby_join(b, other, "O", 1, 30, 500).await.unwrap();
    let lost = db.derby_settle(b, Some(host), 2, 1).await.unwrap().expect("settled");
    assert_eq!(lost.outcome, DerbyOutcome::Paid(vec![(other, 400)]));
    assert_eq!(net(player).await, -400, "P lost the stake of the race B");
    assert_eq!(net(other).await, 200, "O staked 200 and was paid 400");
    let nobody = db.derby_settle(c, Some(host), 2, 5).await.unwrap().expect("settled");
    assert_eq!(nobody.outcome, DerbyOutcome::Cancelled, "one racer is no race");
    assert_eq!(net(player).await, -200, "the stake of the called-off race is back");

    // The same for a race nobody backed the winner of, and a free race counts nothing.
    let d = lobby(&db, "derby", guild, c4, host, 100).await;
    db.derby_join(d, player, "P", 0, 30, 500).await.unwrap();
    db.derby_join(d, other, "O", 1, 30, 500).await.unwrap();
    assert_eq!(net(player).await, -300);
    let refunded = db.derby_settle(d, Some(host), 2, 4).await.unwrap().expect("settled");
    assert_eq!(refunded.outcome, DerbyOutcome::Refunded);
    assert_eq!(net(player).await, -200);
    assert_eq!(net(other).await, 200);
    let free = lobby(&db, "derby", guild, c5, host, 0).await;
    db.derby_join(free, player, "P", 0, 30, 500).await.unwrap();
    db.derby_settle(free, Some(host), 2, 0).await.unwrap().expect("settled");
    assert_eq!(net(player).await, -200, "a race for fun moves no net");
    // The day's net is exactly what the balances moved, and shells only changed hands.
    assert_eq!(shells(&db, player).await, 2_000 + net(player).await);
    assert_eq!(shells(&db, other).await, 2_000 + net(other).await);
    assert_eq!(ledger_total(&db, &[player, other]).await, 4_000);
}

#[tokio::test]
async fn a_derby_stake_is_settled_on_the_day_it_was_put_up() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let [guild, c1, c2, host, leaver, loner] = ids();
    for user in [leaver, loner] {
        db.earn_shells(user, 500, "test").await.unwrap();
    }
    // Put up a day ago and given back now (a leave, and a called-off race): yesterday's row is made
    // whole, and today has no row at all.
    let a = lobby(&db, "derby", guild, c1, host, 80).await;
    db.derby_join(a, leaver, "L", 0, 30, 1_000).await.unwrap();
    joined_yesterday(&db, a, leaver, "derby").await;
    assert_eq!(daily_rows(&db, leaver, "derby").await, vec![(1, 0, -80)]);
    assert_eq!(
        db.derby_leave(a, leaver).await.unwrap(),
        DerbyLeave::Left { refund: 80 }
    );
    assert_eq!(daily_rows(&db, leaver, "derby").await, vec![(1, 0, 0)]);

    let b = lobby(&db, "derby", guild, c2, host, 80).await;
    db.derby_join(b, loner, "S", 0, 30, 1_000).await.unwrap();
    joined_yesterday(&db, b, loner, "derby").await;
    let called_off = db.derby_settle(b, Some(host), 2, 0).await.unwrap().expect("settled");
    assert_eq!(called_off.outcome, DerbyOutcome::Cancelled);
    assert_eq!(daily_rows(&db, loner, "derby").await, vec![(1, 0, 0)]);
    assert_eq!(db.table_today(loner, "derby").await.unwrap(), (0, 0));
}
