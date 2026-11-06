//! The board and puzzle games' tables: best scores, picross boards, solo games and forfeited wagers. Runs
//! against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.

use pinchora_db::{Db, DuelStart, NewGame, PENDING, Payout, SoloStep};

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect(&url, 8).await.expect("connect");
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
    std::array::from_fn(|i| base + 1_000 + i as u64)
}

async fn shells(db: &Db, user: u64) -> i64 {
    db.user(user).await.unwrap().credits
}

#[tokio::test]
async fn the_best_score_only_goes_up() {
    let Some(db) = db().await else { return };
    let [user] = ids();
    db.record_game_score(user, "tide2048", false, 900).await.unwrap();
    db.record_game_score(user, "tide2048", true, 5_000).await.unwrap();
    db.record_game_score(user, "tide2048", false, 100).await.unwrap();
    let stats = db.game_stats(user).await.unwrap();
    let row = stats.iter().find(|s| s.game == "tide2048").expect("a row");
    assert_eq!((row.played, row.won, row.best), (3, 1, 5_000));
    // Games without a score keep a best of zero.
    db.record_game(user, "chess", true).await.unwrap();
    let stats = db.game_stats(user).await.unwrap();
    assert_eq!(stats.iter().find(|s| s.game == "chess").map(|s| s.best), Some(0));
}

#[tokio::test]
async fn a_picross_board_is_kept_unless_it_is_replaced() {
    let Some(db) = db().await else { return };
    let [user] = ids();
    let first = db.start_picross(user, "daily", 20_800, (3, 1), false).await.unwrap();
    assert_eq!((first.picture, first.variant, first.grid.len()), (3, 1, 100));
    let marked = format!("#{}", ".".repeat(99));
    db.picross_marks(user, "daily", 20_800, &first.grid, &marked, false)
        .await
        .unwrap()
        .expect("marks");
    // Starting the day's puzzle again keeps the marks, whatever puzzle it is asked for.
    let kept = db.start_picross(user, "daily", 20_800, (9, 0), false).await.unwrap();
    assert_eq!((kept.picture, kept.grid.as_str()), (3, marked.as_str()));
    // A practice board is thrown away when the next one starts.
    db.start_picross(user, "practice", 0, (1, 0), true).await.unwrap();
    db.picross_marks(user, "practice", 0, &".".repeat(100), &marked, true)
        .await
        .unwrap()
        .expect("solved");
    let next = db.start_picross(user, "practice", 0, (5, 2), true).await.unwrap();
    assert_eq!((next.picture, next.variant, next.status.as_str()), (5, 2, "playing"));
    assert_eq!(next.grid, ".".repeat(100));
    assert!(db.picross_game(user, "practice", 1).await.unwrap().is_none());
}

#[tokio::test]
async fn two_marks_at_once_make_one_change_and_a_board_is_solved_once() {
    let Some(db) = db().await else { return };
    let [user] = ids();
    let board = db.start_picross(user, "daily", 20_801, (0, 0), false).await.unwrap();
    let (a, b) = (format!("#{}", ".".repeat(99)), format!(".#{}", ".".repeat(98)));
    let (one, two) = tokio::join!(
        db.picross_marks(user, "daily", 20_801, &board.grid, &a, false),
        db.picross_marks(user, "daily", 20_801, &board.grid, &b, false)
    );
    let winners = [one.unwrap(), two.unwrap()].iter().filter(|r| r.is_some()).count();
    assert_eq!(winners, 1, "the second click saw a board that had already changed");

    // Both claim the solution from the same marks: only one of them ends the board.
    let current = db
        .picross_game(user, "daily", 20_801)
        .await
        .unwrap()
        .expect("board")
        .grid;
    let done = ".".repeat(98) + "##";
    let (one, two) = tokio::join!(
        db.picross_marks(user, "daily", 20_801, &current, &done, true),
        db.picross_marks(user, "daily", 20_801, &current, &done, true)
    );
    let solved: Vec<_> = [one.unwrap(), two.unwrap()].into_iter().flatten().collect();
    assert_eq!(solved.len(), 1, "one solve, so one payout");
    assert!(solved[0].solved());
    // A finished board takes no more marks.
    assert!(
        db.picross_marks(user, "daily", 20_801, &done, &a, false)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(db.picross_days_solved(user, 10).await.unwrap(), [20_801]);
}

#[tokio::test]
async fn solved_days_come_newest_first_and_only_the_daily_counts() {
    let Some(db) = db().await else { return };
    let [user] = ids();
    for day in [20_810, 20_812, 20_811] {
        let board = db.start_picross(user, "daily", day, (0, 0), false).await.unwrap();
        db.picross_marks(user, "daily", day, &board.grid, &board.grid, true)
            .await
            .unwrap()
            .expect("solved");
    }
    db.start_picross(user, "daily", 20_813, (0, 0), false).await.unwrap();
    let practice = db.start_picross(user, "practice", 0, (0, 0), true).await.unwrap();
    db.picross_marks(user, "practice", 0, &practice.grid, &practice.grid, true)
        .await
        .unwrap();
    assert_eq!(
        db.picross_days_solved(user, 10).await.unwrap(),
        [20_812, 20_811, 20_810]
    );
    assert_eq!(db.picross_days_solved(user, 2).await.unwrap(), [20_812, 20_811]);
}

#[tokio::test]
async fn a_solo_game_moves_one_click_at_a_time_and_ends_once() {
    let Some(db) = db().await else { return };
    let [user] = ids();
    let game = db.start_solo(user, "tide2048", "board0", false).await.unwrap();
    assert_eq!((game.moves, game.score, game.status.as_str()), (0, 0, "playing"));
    // Starting again without replace keeps the game.
    assert_eq!(
        db.start_solo(user, "tide2048", "other", false).await.unwrap().state,
        "board0"
    );

    let step = |state: &'static str, score, over| SoloStep { state, score, over };
    let (a, b) = (step("board1a", 4, false), step("board1b", 8, false));
    let (one, two) = tokio::join!(
        db.advance_solo(user, "tide2048", 0, &a),
        db.advance_solo(user, "tide2048", 0, &b)
    );
    assert_eq!([one.unwrap(), two.unwrap()].iter().filter(|moved| **moved).count(), 1);
    let game = db.solo_game(user, "tide2048").await.unwrap().expect("game");
    assert_eq!(game.moves, 1);

    // Two clicks race to end the game from the same position: one of them does.
    let (a, b) = (step("end-a", 100, true), step("end-b", 200, true));
    let (one, two) = tokio::join!(
        db.advance_solo(user, "tide2048", 1, &a),
        db.advance_solo(user, "tide2048", 1, &b)
    );
    assert_eq!(
        [one.unwrap(), two.unwrap()].iter().filter(|moved| **moved).count(),
        1,
        "one ending, one payout"
    );
    let game = db.solo_game(user, "tide2048").await.unwrap().expect("game");
    assert!(game.over());
    assert!(
        !db.advance_solo(user, "tide2048", 2, &step("late", 1, false))
            .await
            .unwrap()
    );
    // A new game replaces the finished one.
    let fresh = db.start_solo(user, "tide2048", "board9", true).await.unwrap();
    assert_eq!(
        (fresh.state.as_str(), fresh.moves, fresh.score, fresh.status.as_str()),
        ("board9", 0, 0, "playing")
    );
}

#[tokio::test]
async fn a_forfeited_wager_goes_to_the_other_player_once_and_only_if_nobody_moved() {
    let Some(db) = db().await else { return };
    let [host, guest] = ids();
    db.earn_shells(host, 100, "test").await.unwrap();
    db.earn_shells(guest, 100, "test").await.unwrap();
    let session = db
        .create_game(NewGame {
            kind: "chess",
            guild_id: 7,
            channel_id: 8,
            host,
            guest: Some(guest),
            bet: 60,
            status: PENDING,
            state: "pending",
            ttl_secs: 60,
        })
        .await
        .unwrap();
    let DuelStart::Started(_) = db.start_duel(session.id, guest, (8, 5), "moves-0", 60).await.unwrap() else {
        panic!("accepted");
    };
    assert_eq!((shells(&db, host).await, shells(&db, guest).await), (40, 40));

    // The janitor read the host as slow, but the host moved just before the clock ran out: the state changed,
    // so the forfeit it works out from what it read does nothing.
    assert!(db.advance_game(session.id, "moves-0", "moves-1", 60).await.unwrap());
    sqlx::query("UPDATE game_sessions SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(session.id)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(
        db.settle_game(session.id, Some("moves-0"), "timeout", Payout::Winner(guest), true)
            .await
            .unwrap()
            .is_none(),
        "a game that has moved on is not forfeited on stale news"
    );
    assert_eq!((shells(&db, host).await, shells(&db, guest).await), (40, 40));

    // Late with the state the janitor read this time: the guest takes the pot, once.
    let late = db.game(session.id).await.unwrap().expect("session");
    let (one, two) = tokio::join!(
        db.settle_game(session.id, Some(&late.state), "timeout", Payout::Winner(guest), true),
        db.settle_game(session.id, Some(&late.state), "timeout", Payout::Winner(guest), true)
    );
    assert_eq!([one.unwrap(), two.unwrap()].iter().filter(|s| s.is_some()).count(), 1);
    assert_eq!((shells(&db, host).await, shells(&db, guest).await), (40, 160));
}
