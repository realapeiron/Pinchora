//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! The money paths of the party games: stakes held and paid out once, refunds, daily limits, and
//! endings that race each other.

use chrono::{Duration, Utc};
use pinchora_db::{
    ACTIVE, CrewJoin, DONE, Db, DerbyJoin, DerbyLeave, DerbyOutcome, HeistSettled, NewGame, PENDING, ShellLimits,
    ShellPick, ShellStart, SurgeHelp,
};

/// The daily derby loss limit the tests pass to joins that are not about the limit.
const LOSS: i64 = 100_000;

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 8, std::time::Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}

/// Ids unique per run, so the suite can run repeatedly against one database.
fn ids<const N: usize>() -> [u64; N] {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        >> 4;
    std::array::from_fn(|i| base + i as u64)
}

async fn shells(db: &Db, user: u64) -> i64 {
    db.user(user).await.unwrap().credits
}

/// What the ledger says the given players hold in total, which must match their balances.
async fn ledger_total(db: &Db, users: &[u64]) -> i64 {
    let ids: Vec<i64> = users.iter().map(|u| *u as i64).collect();
    let (sum,): (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM credit_ledger WHERE user_id = ANY($1)")
            .bind(ids)
            .fetch_one(db.pool())
            .await
            .unwrap();
    sum
}

async fn lobby(db: &Db, kind: &'static str, guild: u64, channel: u64, host: u64, bet: i64) -> i64 {
    db.open_party_game(NewGame {
        kind,
        guild_id: guild,
        channel_id: channel,
        host,
        guest: None,
        bet,
        status: PENDING,
        state: "7",
        ttl_secs: 600,
    })
    .await
    .unwrap()
    .expect("the channel is free")
    .id
}

async fn run_out(db: &Db, session: i64) {
    sqlx::query("UPDATE game_sessions SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(session)
        .execute(db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn gambling_is_off_until_a_manager_turns_it_on() {
    let Some(db) = db().await else { return };
    let [guild] = ids();
    assert!(
        !db.gambling_enabled(guild).await.unwrap(),
        "a server nobody set up is off"
    );
    db.set_gambling(guild, true).await.unwrap();
    assert!(db.gambling_enabled(guild).await.unwrap());
    db.set_gambling(guild, false).await.unwrap();
    assert!(!db.gambling_enabled(guild).await.unwrap());
}

#[tokio::test]
async fn a_staked_derby_holds_stakes_and_the_winners_share_the_pool() {
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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

/// Runs the futures at the same time and collects what they return.
async fn futures_join<T, F: std::future::Future<Output = anyhow::Result<T>> + Send + 'static>(
    futures: impl IntoIterator<Item = F>,
) -> Vec<T>
where
    T: Send + 'static,
{
    let handles: Vec<_> = futures.into_iter().map(tokio::spawn).collect();
    let mut out = Vec::new();
    for handle in handles {
        out.push(handle.await.expect("task").expect("query"));
    }
    out
}

#[tokio::test]
async fn a_heist_pays_the_crew_once_up_to_their_daily_limit() {
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
async fn the_shell_game_holds_the_stake_pays_a_right_guess_and_refunds_a_forgotten_game() {
    let Some(db) = db().await else { return };
    let [guild, channel, player, other] = ids();
    db.earn_shells(player, 200, "test").await.unwrap();
    let limits = ShellLimits {
        plays: 20,
        loss: 300,
        win: 600,
    };
    let table = |state: &'static str, stake: i64| NewGame {
        kind: "shellgame",
        guild_id: guild,
        channel_id: channel,
        host: player,
        guest: None,
        bet: stake,
        status: ACTIVE,
        state,
        ttl_secs: 45,
    };
    let ShellStart::Started(game) = db.shell_start(table("2", 40), limits).await.unwrap() else {
        panic!("a table for one");
    };
    assert_eq!(shells(&db, player).await, 160, "the stake is held");
    assert!(matches!(
        db.shell_pick(game.id, other, 2, |s| s * 2).await.unwrap(),
        ShellPick::NotYours
    ));
    let ShellPick::Done(done) = db.shell_pick(game.id, player, 2, |s| s * 5 / 2).await.unwrap() else {
        panic!("the guess counts");
    };
    assert_eq!((done.pearl, done.picked, done.paid), (2, 2, 100));
    assert_eq!(shells(&db, player).await, 260);
    assert!(matches!(
        db.shell_pick(game.id, player, 2, |s| s * 5 / 2).await.unwrap(),
        ShellPick::Gone
    ));
    assert_eq!(
        db.table_today(player, "shellgame").await.unwrap(),
        (1, 60),
        "net: +100 less the 40 stake"
    );

    // A wrong guess loses the stake. The table's language rides after the pearl (`<shell>|<code>`).
    let ShellStart::Started(game) = db.shell_start(table("0|tr", 50), limits).await.unwrap() else {
        panic!("another game");
    };
    let ShellPick::Done(done) = db.shell_pick(game.id, player, 1, |s| s * 5 / 2).await.unwrap() else {
        panic!("a guess");
    };
    assert_eq!((done.pearl, done.picked, done.paid), (0, 1, 0));
    assert_eq!(shells(&db, player).await, 210);

    // A forgotten game is refunded once it is late, and not before.
    let ShellStart::Started(game) = db.shell_start(table("1", 30), limits).await.unwrap() else {
        panic!("a third game");
    };
    assert_eq!(shells(&db, player).await, 180);
    assert!(db.shell_refund(game.id).await.unwrap().is_none(), "not late yet");
    run_out(&db, game.id).await;
    assert!(matches!(
        db.shell_pick(game.id, player, 1, |s| s * 5 / 2).await.unwrap(),
        ShellPick::Gone
    ));
    assert!(db.shell_refund(game.id).await.unwrap().is_some());
    assert!(db.shell_refund(game.id).await.unwrap().is_none(), "refunded once");
    assert_eq!(shells(&db, player).await, 210);
    assert_eq!(
        db.table_today(player, "shellgame").await.unwrap(),
        (2, 10),
        "the refunded game does not count"
    );
    assert_eq!(ledger_total(&db, &[player]).await, 210);
}

#[tokio::test]
async fn the_shell_table_closes_for_a_player_who_is_down_up_or_out_of_games() {
    let Some(db) = db().await else { return };
    let [guild, channel, player] = ids();
    db.earn_shells(player, 1_000, "test").await.unwrap();
    let table = |stake: i64| NewGame {
        kind: "shellgame",
        guild_id: guild,
        channel_id: channel,
        host: player,
        guest: None,
        bet: stake,
        status: ACTIVE,
        state: "0",
        ttl_secs: 45,
    };
    let limits = ShellLimits {
        plays: 4,
        loss: 100,
        win: 120,
    };
    // Three lost games of 30 leave the player down 90; a fourth of 30 would pass the limit of 100.
    for _ in 0..3 {
        let ShellStart::Started(game) = db.shell_start(table(30), limits).await.unwrap() else {
            panic!("still within the limit");
        };
        db.shell_pick(game.id, player, 1, |s| s * 5 / 2).await.unwrap();
    }
    assert!(matches!(
        db.shell_start(table(30), limits).await.unwrap(),
        ShellStart::Down { net: -90 }
    ));
    assert_eq!(shells(&db, player).await, 910, "a refused game takes nothing");
    // A smaller stake still fits.
    let ShellStart::Started(game) = db.shell_start(table(10), limits).await.unwrap() else {
        panic!("10 fits under 100");
    };
    db.shell_pick(game.id, player, 0, |s| s * 5 / 2).await.unwrap();
    assert!(matches!(
        db.shell_start(table(5), limits).await.unwrap(),
        ShellStart::TooMany
    ));

    // Up to the limit: the table closes too.
    let [rich] = ids::<1>().map(|id| id + 7);
    db.earn_shells(rich, 1_000, "test").await.unwrap();
    let up = |stake: i64| NewGame {
        host: rich,
        ..table(stake)
    };
    let limits = ShellLimits {
        plays: 10,
        loss: 1_000,
        win: 50,
    };
    let ShellStart::Started(game) = db.shell_start(up(40), limits).await.unwrap() else {
        panic!("a game");
    };
    db.shell_pick(game.id, rich, 0, |s| s * 5 / 2).await.unwrap();
    assert!(matches!(
        db.shell_start(up(5), limits).await.unwrap(),
        ShellStart::Up { net: 60 }
    ));
    let broke = ids::<1>()[0] + 13;
    assert!(matches!(
        db.shell_start(
            NewGame {
                host: broke,
                ..table(10)
            },
            limits
        )
        .await
        .unwrap(),
        ShellStart::Broke { balance: 0 }
    ));
}

#[tokio::test]
async fn daily_grants_never_pass_the_cap_even_all_at_once() {
    let Some(db) = db().await else { return };
    let [user] = ids();
    let grants = (0..12).map(|_| {
        let db = db.clone();
        async move { db.grant_capped(user, "heist", 20, 100, "test").await }
    });
    let granted = futures_join(grants).await;
    assert_eq!(granted.iter().sum::<i64>(), 100, "{granted:?}");
    assert_eq!(shells(&db, user).await, 100);
    assert_eq!(db.grant_capped(user, "heist", 20, 100, "test").await.unwrap(), 0);
    assert_eq!(
        db.grant_capped(user, "surge", 20, 100, "test").await.unwrap(),
        20,
        "each game has its own limit"
    );
    assert_eq!(ledger_total(&db, &[user]).await, 120);
}

#[tokio::test]
async fn share_or_grab_pays_from_the_pool_once_up_to_the_cap() {
    let Some(db) = db().await else { return };
    let [guild, channel, host, guest] = ids();
    let session = db
        .create_game(NewGame {
            kind: "sharegrab",
            guild_id: guild,
            channel_id: channel,
            host,
            guest: Some(guest),
            bet: 0,
            status: ACTIVE,
            state: "s:s",
            ttl_secs: 60,
        })
        .await
        .unwrap();
    assert!(
        db.settle_game_granting(session.id, Some("-:-"), "done", [30, 30], 100, false)
            .await
            .unwrap()
            .is_none(),
        "the state moved on"
    );
    let (before, paid) = db
        .settle_game_granting(session.id, Some("s:s"), "done", [30, 30], 100, false)
        .await
        .unwrap()
        .expect("settled");
    assert_eq!((before.status.as_str(), paid), (ACTIVE, [30, 30]));
    assert!(
        db.settle_game_granting(session.id, None, "done", [30, 30], 100, false)
            .await
            .unwrap()
            .is_none(),
        "once"
    );
    // The host already has 30 of a cap of 40 today.
    let next = db
        .create_game(NewGame {
            kind: "sharegrab",
            guild_id: guild,
            channel_id: channel + 1,
            host,
            guest: Some(guest),
            bet: 0,
            status: ACTIVE,
            state: "g:s",
            ttl_secs: 60,
        })
        .await
        .unwrap();
    let (_, paid) = db
        .settle_game_granting(next.id, None, "done", [60, 0], 40, false)
        .await
        .unwrap()
        .expect("settled");
    assert_eq!(paid, [10, 0]);
    assert_eq!(shells(&db, host).await, 40);
    assert_eq!(shells(&db, guest).await, 30);
    // Not late yet: a timeout pass leaves it alone.
    let timed = db
        .create_game(NewGame {
            kind: "sharegrab",
            guild_id: guild,
            channel_id: channel + 2,
            host,
            guest: Some(guest),
            bet: 0,
            status: ACTIVE,
            state: "-:-",
            ttl_secs: 600,
        })
        .await
        .unwrap();
    assert!(
        db.settle_game_granting(timed.id, None, "timeout", [0, 0], 40, true)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn storm_settings_schedule_the_next_storm_and_one_pod_takes_each_slot() {
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

/// A player's rows in `party_daily` for `game` as (days ago, plays, net), newest first.
async fn daily_rows(db: &Db, user: u64, game: &str) -> Vec<(i32, i32, i64)> {
    let today = Utc::now().timestamp().div_euclid(86_400) as i32;
    let rows: Vec<(i32, i32, i64)> =
        sqlx::query_as("SELECT day, plays, net FROM party_daily WHERE user_id = $1 AND game = $2 ORDER BY day DESC")
            .bind(user as i64)
            .bind(game)
            .fetch_all(db.pool())
            .await
            .unwrap();
    rows.into_iter()
        .map(|(day, plays, net)| (today - day, plays, net))
        .collect()
}

/// Pretends the player put their stake in a day ago: their join, and the day's row it was counted on.
async fn joined_yesterday(db: &Db, session: i64, user: u64, game: &str) {
    sqlx::query("UPDATE party_members SET joined_at = joined_at - interval '1 day' WHERE session_id = $1")
        .bind(session)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE party_daily SET day = day - 1 WHERE user_id = $1 AND game = $2")
        .bind(user as i64)
        .bind(game)
        .execute(db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn derby_stakes_count_against_a_daily_loss_limit_until_they_come_back() {
    let Some(db) = db().await else { return };
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
    let Some(db) = db().await else { return };
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

#[tokio::test]
async fn a_shell_game_is_settled_on_the_day_the_table_started() {
    let Some(db) = db().await else { return };
    let [guild, channel, forgetful, lucky] = ids();
    for user in [forgetful, lucky] {
        db.earn_shells(user, 500, "test").await.unwrap();
    }
    let limits = ShellLimits {
        plays: 20,
        loss: 300,
        win: 600,
    };
    let table = |host: u64| NewGame {
        kind: "shellgame",
        guild_id: guild,
        channel_id: channel,
        host,
        guest: None,
        bet: 40,
        status: ACTIVE,
        state: "1",
        ttl_secs: 45,
    };
    // A table started a day ago and never guessed at: the refund goes to the row of that day.
    let ShellStart::Started(game) = db.shell_start(table(forgetful), limits).await.unwrap() else {
        panic!("a table");
    };
    sqlx::query("UPDATE game_sessions SET created_at = created_at - interval '1 day' WHERE id = $1")
        .bind(game.id)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE party_daily SET day = day - 1 WHERE user_id = $1 AND game = 'shellgame'")
        .bind(forgetful as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(daily_rows(&db, forgetful, "shellgame").await, vec![(1, 1, -40)]);
    run_out(&db, game.id).await;
    assert!(db.shell_refund(game.id).await.unwrap().is_some());
    assert_eq!(daily_rows(&db, forgetful, "shellgame").await, vec![(1, 0, 0)]);
    assert_eq!(db.table_today(forgetful, "shellgame").await.unwrap(), (0, 0));
    assert_eq!(shells(&db, forgetful).await, 500);

    // And a right guess made after midnight is paid into the same day's net.
    let ShellStart::Started(game) = db.shell_start(table(lucky), limits).await.unwrap() else {
        panic!("a table");
    };
    sqlx::query("UPDATE game_sessions SET created_at = created_at - interval '1 day' WHERE id = $1")
        .bind(game.id)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE party_daily SET day = day - 1 WHERE user_id = $1 AND game = 'shellgame'")
        .bind(lucky as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let ShellPick::Done(done) = db.shell_pick(game.id, lucky, 1, |s| s * 5 / 2).await.unwrap() else {
        panic!("a guess");
    };
    assert_eq!(done.paid, 100);
    assert_eq!(daily_rows(&db, lucky, "shellgame").await, vec![(1, 1, 60)]);
    assert_eq!(db.table_today(lucky, "shellgame").await.unwrap(), (0, 0));
}
