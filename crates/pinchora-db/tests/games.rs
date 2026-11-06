//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.

use pinchora_db::{ACTIVE, DONE, Db, DuelStart, NewGame, PENDING, Payout};

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 5, std::time::Duration::from_secs(60))
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

fn duel(host: u64, guest: u64, bet: i64) -> NewGame<'static> {
    NewGame {
        kind: "connect4",
        guild_id: 7,
        channel_id: 8,
        host,
        guest: Some(guest),
        bet,
        status: PENDING,
        state: "start",
        ttl_secs: 120,
    }
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

#[tokio::test]
async fn a_won_wager_pays_the_pot_once() {
    let Some(db) = db().await else { return };
    let [host, guest, stranger] = ids();
    db.earn_shells(host, 100, "test").await.unwrap();
    db.earn_shells(guest, 100, "test").await.unwrap();

    let session = db.create_game(duel(host, guest, 60)).await.unwrap();
    assert_eq!(session.status, PENDING);
    assert!(session.has_player(host) && session.has_player(guest) && !session.has_player(stranger));
    assert_eq!(
        shells(&db, host).await,
        100,
        "nothing is held before the challenge is accepted"
    );

    assert!(matches!(
        db.start_duel(session.id, stranger, (8, 1), "go", 90).await.unwrap(),
        DuelStart::NotInvited
    ));
    let DuelStart::Started(started) = db.start_duel(session.id, guest, (8, 99), "go", 90).await.unwrap() else {
        panic!("the guest accepts");
    };
    assert_eq!(
        (started.status.as_str(), started.message_id, started.state.as_str()),
        (ACTIVE, Some(99), "go")
    );
    assert_eq!(
        (shells(&db, host).await, shells(&db, guest).await),
        (40, 40),
        "both bets are held"
    );
    assert!(
        matches!(
            db.start_duel(session.id, guest, (8, 99), "go", 90).await.unwrap(),
            DuelStart::Gone
        ),
        "a challenge can be accepted once"
    );

    let settled = db
        .settle_game(session.id, Some("go"), "won", Payout::Winner(guest), false)
        .await
        .unwrap()
        .expect("the first settlement wins the race");
    assert_eq!(settled.status, ACTIVE, "returns the session as it was");
    assert_eq!((shells(&db, host).await, shells(&db, guest).await), (40, 160));
    assert!(
        db.settle_game(session.id, None, "won", Payout::Winner(guest), false)
            .await
            .unwrap()
            .is_none(),
        "no second payout"
    );
    assert_eq!(shells(&db, guest).await, 160);
    assert_eq!(db.game(session.id).await.unwrap().unwrap().status, DONE);
    assert_eq!(
        ledger_total(&db, &[host, guest]).await,
        200,
        "shells only moved between the two"
    );
}

#[tokio::test]
async fn draws_and_timeouts_give_the_bets_back() {
    let Some(db) = db().await else { return };
    let [host, guest] = ids();
    db.earn_shells(host, 100, "test").await.unwrap();
    db.earn_shells(guest, 100, "test").await.unwrap();

    let session = db.create_game(duel(host, guest, 100)).await.unwrap();
    let DuelStart::Started(_) = db.start_duel(session.id, guest, (8, 5), "go", 90).await.unwrap() else {
        panic!("accepted");
    };
    assert_eq!((shells(&db, host).await, shells(&db, guest).await), (0, 0));
    assert!(
        db.settle_game(session.id, None, "timeout", Payout::Refund, true)
            .await
            .unwrap()
            .is_none(),
        "a game with time left is not late"
    );
    sqlx::query("UPDATE game_sessions SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(session.id)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(db.late_games(1000).await.unwrap().iter().any(|g| g.id == session.id));
    db.settle_game(session.id, None, "timeout", Payout::Refund, true)
        .await
        .unwrap()
        .expect("late games settle");
    assert_eq!((shells(&db, host).await, shells(&db, guest).await), (100, 100));
    assert!(!db.late_games(1000).await.unwrap().iter().any(|g| g.id == session.id));
}

#[tokio::test]
async fn a_player_who_cannot_cover_the_bet_blocks_the_game() {
    let Some(db) = db().await else { return };
    let [host, guest] = ids();
    db.earn_shells(host, 100, "test").await.unwrap();
    db.earn_shells(guest, 20, "test").await.unwrap();

    let session = db.create_game(duel(host, guest, 50)).await.unwrap();
    let DuelStart::Broke { user, balance } = db.start_duel(session.id, guest, (8, 5), "go", 90).await.unwrap() else {
        panic!("the guest is short");
    };
    assert_eq!((user, balance), (guest, 20));
    assert_eq!(
        (shells(&db, host).await, shells(&db, guest).await),
        (100, 20),
        "nothing was taken"
    );
    assert_eq!(db.game(session.id).await.unwrap().unwrap().status, PENDING);

    // A declined challenge holds nothing either, and is over.
    let ended = db
        .settle_game(session.id, None, "declined", Payout::Refund, false)
        .await
        .unwrap()
        .expect("pending sessions can be closed");
    assert_eq!(ended.status, PENDING);
    assert_eq!(
        (shells(&db, host).await, shells(&db, guest).await),
        (100, 20),
        "no refund of what was never held"
    );
}

#[tokio::test]
async fn a_decline_that_loses_the_race_to_accept_keeps_the_bets() {
    let Some(db) = db().await else { return };
    let [host, guest] = ids();
    db.earn_shells(host, 100, "test").await.unwrap();
    db.earn_shells(guest, 100, "test").await.unwrap();
    let session = db.create_game(duel(host, guest, 40)).await.unwrap();
    db.start_duel(session.id, guest, (8, 5), "go", 90).await.unwrap();
    // The decline was read while pending, so it only ends the pending state.
    let declined = db.settle_game(session.id, Some("start"), "declined", Payout::Refund, false);
    assert!(declined.await.unwrap().is_none(), "the accepted duel goes on");
    assert_eq!(db.game(session.id).await.unwrap().unwrap().status, ACTIVE);
    assert_eq!((shells(&db, host).await, shells(&db, guest).await), (60, 60));
}

#[tokio::test]
async fn simultaneous_endings_pay_exactly_once() {
    let Some(db) = db().await else { return };
    let [host, guest] = ids();
    db.earn_shells(host, 100, "test").await.unwrap();
    db.earn_shells(guest, 100, "test").await.unwrap();
    let session = db.create_game(duel(host, guest, 100)).await.unwrap();
    db.start_duel(session.id, guest, (8, 5), "go", 90).await.unwrap();

    let mut tasks = Vec::new();
    for i in 0..8 {
        let db = db.clone();
        let payout = if i % 2 == 0 {
            Payout::Winner(host)
        } else {
            Payout::Refund
        };
        tasks.push(tokio::spawn(async move {
            db.settle_game(session.id, None, "end", payout, false)
                .await
                .unwrap()
                .is_some()
        }));
    }
    let mut winners = 0;
    for t in tasks {
        winners += usize::from(t.await.unwrap());
    }
    assert_eq!(winners, 1);
    assert_eq!(
        shells(&db, host).await + shells(&db, guest).await,
        200,
        "the pot was paid once"
    );
}

#[tokio::test]
async fn moves_only_apply_to_the_state_they_were_made_on() {
    let Some(db) = db().await else { return };
    let [host, guest] = ids();
    let session = db.create_game(duel(host, guest, 0)).await.unwrap();
    assert!(
        !db.advance_game(session.id, "start", "one", 60).await.unwrap(),
        "a pending challenge has no moves"
    );
    db.start_duel(session.id, guest, (8, 5), "start", 60).await.unwrap();
    assert!(db.advance_game(session.id, "start", "one", 60).await.unwrap());
    assert!(
        !db.advance_game(session.id, "start", "two", 60).await.unwrap(),
        "the second of two simultaneous clicks loses"
    );
    assert_eq!(db.game(session.id).await.unwrap().unwrap().state, "one");
    // A late game accepts no more moves.
    sqlx::query("UPDATE game_sessions SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(session.id)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(!db.advance_game(session.id, "one", "two", 60).await.unwrap());
}

#[tokio::test]
async fn trivia_answers_count_once_per_player() {
    let Some(db) = db().await else { return };
    let [host, a, b] = ids();
    let round = db
        .create_game(NewGame {
            guest: None,
            ..duel(host, 0, 0)
        })
        .await
        .unwrap();
    assert!(db.add_game_answer(round.id, a, 2).await.unwrap());
    assert!(!db.add_game_answer(round.id, a, 3).await.unwrap(), "one answer each");
    assert!(db.add_game_answer(round.id, b, 0).await.unwrap());
    assert_eq!(db.game_answers(round.id).await.unwrap(), vec![(a, 2), (b, 0)]);
}

#[tokio::test]
async fn stats_and_wordle_boards() {
    let Some(db) = db().await else { return };
    let [user] = ids();
    db.record_game(user, "connect4", true).await.unwrap();
    db.record_game(user, "connect4", false).await.unwrap();
    db.record_game(user, "rps", true).await.unwrap();
    let mut stats = db.game_stats(user).await.unwrap();
    stats.sort_by(|a, b| a.game.cmp(&b.game));
    assert_eq!(
        stats
            .iter()
            .map(|s| (s.game.as_str(), s.played, s.won))
            .collect::<Vec<_>>(),
        vec![("connect4", 2, 1), ("rps", 1, 1)]
    );

    let board = db.start_wordle(user, "daily", 20_800, "pearl", false).await.unwrap();
    assert_eq!((board.status.as_str(), board.guesses.len()), ("playing", 0));
    let kept = db.start_wordle(user, "daily", 20_800, "other", false).await.unwrap();
    assert_eq!(kept.word, "pearl", "the daily word is not replaced");
    let first = db.wordle_guess(user, "daily", 20_800, "crane").await.unwrap().unwrap();
    assert_eq!((first.status.as_str(), first.guesses.len()), ("playing", 1));
    let won = db.wordle_guess(user, "daily", 20_800, "pearl").await.unwrap().unwrap();
    assert_eq!((won.status.as_str(), won.guesses.len()), ("won", 2));
    assert!(
        db.wordle_guess(user, "daily", 20_800, "crane").await.unwrap().is_none(),
        "a finished board takes no guesses"
    );

    let practice = db.start_wordle(user, "practice", 0, "coral", true).await.unwrap();
    for word in ["aaaaa", "bbbbb", "ccccc", "ddddd", "eeeee"] {
        assert_eq!(
            db.wordle_guess(user, "practice", 0, word)
                .await
                .unwrap()
                .unwrap()
                .status,
            "playing"
        );
    }
    assert_eq!(practice.word, "coral");
    let lost = db.wordle_guess(user, "practice", 0, "fffff").await.unwrap().unwrap();
    assert_eq!((lost.status.as_str(), lost.guesses.len()), ("lost", 6));
    let fresh = db.start_wordle(user, "practice", 0, "shell", true).await.unwrap();
    assert_eq!(
        (fresh.word.as_str(), fresh.guesses.len(), fresh.status.as_str()),
        ("shell", 0, "playing")
    );
}
