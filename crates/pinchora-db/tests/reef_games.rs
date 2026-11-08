//! Real database rollback, durable replay, shared caps and deletion tombstones.
use pinchora_db::{ACTIVE, Db, NewGame, NewReefGame, ReefGameStep};
use serde_json::json;
async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect(&url, 5).await.unwrap();
    db.migrate().await.unwrap();
    Some(db)
}
fn id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        >> 4
}
async fn board(db: &Db, user: u64) -> pinchora_db::GameSession {
    db.create_game(NewGame {
        kind: "chess",
        guild_id: 1,
        channel_id: user,
        host: user,
        guest: Some(user + 1),
        bet: 0,
        status: ACTIVE,
        state: "before",
        ttl_secs: 300,
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn settlement_crash_at_stats_and_ledger_rolls_back_every_stage_then_retries_once() {
    let Some(db) = db().await else {
        return;
    };
    let user = id();
    let s = board(&db, user).await;
    // Faults happen inside PostgreSQL, after the session UPDATE and before COMMIT.
    for table in ["game_stats", "credit_ledger"] {
        let name = format!("reef8_fault_{user}");
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE {table} ADD CONSTRAINT {name} CHECK(user_id <> {user})"
        )))
        .execute(db.pool())
        .await
        .unwrap();
        let failed = db.settle_board_game(&s, "after", Some(user), 30, 5).await;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE {table} DROP CONSTRAINT {name}"
        )))
        .execute(db.pool())
        .await
        .unwrap();
        assert!(failed.is_err());
        assert_eq!(db.game(s.id).await.unwrap().unwrap().state, "before");
        assert!(db.game_stats(user).await.unwrap().is_empty());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM credit_ledger WHERE user_id=$1")
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
    assert_eq!(
        db.settle_board_game(&s, "after", Some(user), 30, 5).await.unwrap(),
        Some(30)
    );
    // A crash after COMMIT loses the response; retrying that final action cannot repeat effects.
    assert!(
        db.settle_board_game(&s, "after", Some(user), 30, 5)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(db.game_stats(user).await.unwrap()[0].played, 1);
    assert_eq!(db.user(user).await.unwrap().credits, 30);
}
#[tokio::test]
async fn bot_and_room_wins_share_cap_and_today_survives_privacy_deletion() {
    let Some(db) = db().await else {
        return;
    };
    let user = id();
    let legacy = user + 100;
    db.delete_user_data(legacy, chrono::Utc::now()).await.unwrap();
    assert_eq!(
        db.capped_game_win(legacy, "chess", 30, 5, "chess_win").await.unwrap(),
        0,
        "a legacy same-day deletion cannot reset an unseeded allowance"
    );
    for _ in 0..4 {
        assert_eq!(db.capped_game_win(user, "chess", 30, 5, "chess_win").await.unwrap(), 30);
    }
    db.house(
        user,
        &pinchora_db::NewRoom {
            room_type: "living".into(),
            wallpaper: "sand".into(),
            floor: "sand".into(),
        },
    )
    .await
    .unwrap();
    let game = db
        .reef_create_game(NewReefGame {
            room: &format!("plaza:{user}:0:0"),
            owner: user,
            host: user,
            kind: "chess",
            request: "new",
            placement: None,
            layout_revision: None,
            state: json!({"phase":"active"}),
        })
        .await
        .unwrap();
    let payload = json!({"final":true});
    let finish = |_: &pinchora_db::ReefGame, _: &[pinchora_db::ReefGamePlayer], _: i64| {
        Ok(ReefGameStep {
            state: json!({"phase":"done"}),
            finished: true,
            seconds: 0,
            join: false,
            answer: None,
            winners: vec![0],
            reward: 30,
            cap: 5,
        })
    };
    // Fail after state, ledger and stats have been written, just before the action receipt commits.
    let fault = format!("reef8_receipt_{user}");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER TABLE reef_game_actions ADD CONSTRAINT {fault} CHECK(user_id <> {user})"
    )))
    .execute(db.pool())
    .await
    .unwrap();
    let failed = db.reef_game_step(game.id, user, "last", &payload, None, finish).await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER TABLE reef_game_actions DROP CONSTRAINT {fault}"
    )))
    .execute(db.pool())
    .await
    .unwrap();
    assert!(failed.is_err());
    assert!(!db.reef_game(game.id).await.unwrap().unwrap().finished);
    assert!(db.game_stats(user).await.unwrap().is_empty());
    assert_eq!(db.user(user).await.unwrap().credits, 120);
    db.reef_game_step(game.id, user, "last", &payload, None, finish)
        .await
        .unwrap();
    db.reef_game_step(game.id, user, "last", &payload, None, |_, _, _| {
        panic!("receipt must bypass settlement")
    })
    .await
    .unwrap();
    assert_eq!(db.reef_game_players(game.id).await.unwrap()[0].paid, 30);
    assert_eq!(db.capped_game_win(user, "chess", 30, 5, "chess_win").await.unwrap(), 0);
    db.delete_user_data(user, chrono::Utc::now()).await.unwrap();
    assert_eq!(db.capped_game_win(user, "chess", 30, 5, "chess_win").await.unwrap(), 0);
}
#[tokio::test]
async fn picross_completion_is_atomic_shared_and_cannot_be_repaid_after_deletion() {
    let Some(db) = db().await else {
        return;
    };
    let user = id();
    let day = chrono::Utc::now().timestamp().div_euclid(86400) as i32;
    let g = db.start_picross(user, "daily", day, (0, 0), false).await.unwrap();
    let name = format!("reef8_picross_fault_{user}");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER TABLE credit_ledger ADD CONSTRAINT {name} CHECK(user_id <> {user})"
    )))
    .execute(db.pool())
    .await
    .unwrap();
    let fail = db
        .picross_marks_at(user, "daily", day, &g.grid, &g.grid, true, Some(g.revision))
        .await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER TABLE credit_ledger DROP CONSTRAINT {name}"
    )))
    .execute(db.pool())
    .await
    .unwrap();
    assert!(fail.is_err());
    assert!(!db.picross_game(user, "daily", day).await.unwrap().unwrap().solved());
    assert!(db.game_stats(user).await.unwrap().is_empty());
    let solved = db
        .picross_marks_at(user, "daily", day, &g.grid, &g.grid, true, Some(g.revision))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(solved.paid, 35);
    assert!(
        db.picross_marks_at(user, "daily", day, &g.grid, &g.grid, true, Some(g.revision))
            .await
            .unwrap()
            .is_none()
    );
    db.delete_user_data(user, chrono::Utc::now()).await.unwrap();
    let g = db.start_picross(user, "daily", day, (0, 0), false).await.unwrap();
    let again = db
        .picross_marks(user, "daily", day, &g.grid, &g.grid, true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(again.paid, 0);
}

#[tokio::test]
async fn bot_trivia_settlement_is_atomic_and_late_answers_are_rejected() {
    let Some(db) = db().await else {
        return;
    };
    let user = id();
    let s = db
        .create_game(NewGame {
            kind: "trivia",
            guild_id: 1,
            channel_id: user,
            host: user,
            guest: None,
            bet: 0,
            status: ACTIVE,
            state: "q:0123",
            ttl_secs: 30,
        })
        .await
        .unwrap();
    assert!(db.add_game_answer(s.id, user, 1).await.unwrap());
    assert!(!db.add_game_answer(s.id, user, 2).await.unwrap());
    let name = format!("reef8_trivia_fault_{user}");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER TABLE credit_ledger ADD CONSTRAINT {name} CHECK(user_id <> {user})"
    )))
    .execute(db.pool())
    .await
    .unwrap();
    let failed = db.settle_trivia_game(&s, Some(1), 20, 5).await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER TABLE credit_ledger DROP CONSTRAINT {name}"
    )))
    .execute(db.pool())
    .await
    .unwrap();
    assert!(failed.is_err());
    assert!(db.game(s.id).await.unwrap().unwrap().is_active());
    assert!(db.game_stats(user).await.unwrap().is_empty());
    assert_eq!(
        db.settle_trivia_game(&s, Some(1), 20, 5).await.unwrap(),
        Some(vec![(user, true, 20)])
    );
    assert!(db.settle_trivia_game(&s, Some(1), 20, 5).await.unwrap().is_none());
    assert!(!db.add_game_answer(s.id, user + 1, 1).await.unwrap());
    assert_eq!(db.user(user).await.unwrap().credits, 20);
}
