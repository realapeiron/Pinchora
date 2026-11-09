use crate::locks::*;

#[tokio::test]
async fn opening_a_session_waits_for_its_host_to_finish_deleting() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [host, guest, guild] = ids(7);
    db.earn_shells(host, 5, "test").await.unwrap();
    let mut delete = db.pool().begin().await.unwrap();
    Db::delete_user_data_in(&mut delete, host, Utc::now()).await.unwrap();
    let (caller, caller_pid) = side().await;
    let created = tokio::spawn(async move { caller.create_game(duel(host, guest, 0, guild)).await });
    queued(&db, caller_pid, "the new session").await;
    delete.commit().await.unwrap();
    let session = created.await.unwrap().expect("opening a session fails nowhere");
    assert!(db.game(session.id).await.unwrap().is_some());
    db.delete_user_data(host, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn sweeps_leave_the_rows_somebody_holds_for_the_next_one() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db_with(4).await else { return };

    let [guild, host] = ids(7);
    let mut old = Vec::new();
    for _ in 0..2 {
        let (id,): (i64,) = sqlx::query_as(
            "INSERT INTO game_sessions (kind, guild_id, channel_id, host_id, status, state, expires_at, created_at)
             VALUES ('connect4', $1, 1, $2, 'done', '', now(), now() - interval '3 days') RETURNING id",
        )
        .bind(guild as i64)
        .bind(host as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
        old.push(id);
    }
    let weeks = NaiveDate::from_ymd_opt(2001, 1, 1).unwrap();
    for user in [guild, host] {
        sqlx::query("INSERT INTO weekly_xp (guild_id, week, user_id, xp) VALUES ($1, $2, $3, 1)")
            .bind(guild as i64)
            .bind(weeks)
            .bind(user as i64)
            .execute(db.pool())
            .await
            .unwrap();
    }
    let mut held = db.pool().begin().await.unwrap();
    sqlx::query("SELECT 1 FROM game_sessions WHERE id = $1 FOR UPDATE")
        .bind(old[0])
        .execute(&mut *held)
        .await
        .unwrap();
    sqlx::query("SELECT 1 FROM weekly_xp WHERE guild_id = $1 AND week = $2 AND user_id = $1 FOR UPDATE")
        .bind(guild as i64)
        .bind(weeks)
        .execute(&mut *held)
        .await
        .unwrap();

    // Neither sweep waits for what is held (a plain delete would wait here until the timeout).
    let wait = StdDuration::from_secs(10);
    // Old rows from earlier runs may also be removed; only these fixtures are asserted below.
    let games = tokio::time::timeout(wait, db.forget_old_games()).await;
    let weeks_gone = tokio::time::timeout(
        wait,
        db.forget_weeks_before(NaiveDate::from_ymd_opt(2001, 1, 2).unwrap()),
    )
    .await;
    games.expect("the sweep of games does not wait").unwrap();
    weeks_gone.expect("the sweep of weeks does not wait").unwrap();
    let left = |sql: &'static str, id: i64| {
        let pool = db.pool().clone();
        async move {
            let (n,): (i64,) = sqlx::query_as(sql).bind(id).fetch_one(&pool).await.unwrap();
            n
        }
    };
    assert_eq!(
        left("SELECT count(*) FROM game_sessions WHERE id = $1", old[0]).await,
        1,
        "the held session stays"
    );
    assert_eq!(
        left("SELECT count(*) FROM game_sessions WHERE id = $1", old[1]).await,
        0
    );
    assert_eq!(
        left("SELECT count(*) FROM weekly_xp WHERE user_id = $1", guild as i64).await,
        1
    );
    assert_eq!(
        left("SELECT count(*) FROM weekly_xp WHERE user_id = $1", host as i64).await,
        0
    );

    held.commit().await.unwrap();
    db.forget_old_games().await.unwrap();
    db.forget_weeks_before(NaiveDate::from_ymd_opt(2001, 1, 2).unwrap())
        .await
        .unwrap();
    assert_eq!(
        left("SELECT count(*) FROM game_sessions WHERE id = $1", old[0]).await,
        0
    );
    assert_eq!(
        left("SELECT count(*) FROM weekly_xp WHERE user_id = $1", guild as i64).await,
        0
    );
}

#[tokio::test]
async fn settling_a_derby_while_a_racer_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [host, racer, leaver, guild, channel] = ids(7);
    for user in [host, racer, leaver] {
        db.earn_shells(user, 100, "test").await.unwrap();
    }
    let derby = db
        .open_party_game(lobby_of("derby", guild, channel, host, 10, PENDING))
        .await
        .unwrap()
        .expect("the channel is free");
    for (user, crab) in [(host, 0), (racer, 1), (leaver, 1)] {
        assert_eq!(
            db.derby_join(derby.id, user, "R", crab, 30, LOSS).await.unwrap(),
            DerbyJoin::Joined
        );
    }

    // The race is run while one of its winners is deleting their data.
    let id = derby.id;
    let (settled, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM game_sessions WHERE id = $1 FOR UPDATE",
        id,
        leaver,
        move |db| async move { db.derby_settle(id, Some(host), 2, 1).await },
    )
    .await;
    let settled = settled.expect("the race fails nowhere");
    deleted.expect("the delete fails nowhere");
    assert!(settled.is_some(), "somebody ran the race");
    for user in [host, racer, leaver] {
        assert_eq!(credits(&db, user).await, ledger(&db, user).await, "books of {user}");
    }
    let ended = db.game(derby.id).await.unwrap().expect("the lobby");
    assert_eq!(
        ended.status,
        pinchora_db::DONE,
        "the race was run and nothing waits in the lobby"
    );
    for user in [host, racer] {
        db.delete_user_data(user, Utc::now()).await.unwrap();
    }
}

#[tokio::test]
async fn grabbing_a_card_while_the_player_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [grabber, guild] = ids(7);
    let cards: Vec<String> = ["x_one", "x_two", "x_three"].map(String::from).into();
    let drop = db
        .create_drop(NewDrop {
            kind: DROP_CRITTERS,
            guild_id: guild,
            channel_id: guild + 1,
            host: None,
            cards: &cards,
            expires_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap();
    let award = Award {
        entry: "x_one",
        new_shells: 15,
        dup_shells: 6,
        dup_fragments: 1,
        bonus_fragments: 0,
    };
    let daily = Daily {
        game: "critterdrop",
        day: 20_000,
        cap: 20,
    };
    let id = drop.id;
    let (grabbed, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM collection_drops WHERE id = $1 FOR UPDATE",
        id,
        grabber,
        move |db| async move { db.grab_card(id, 0, grabber, Utc::now(), award, daily).await },
    )
    .await;
    let grabbed = grabbed.expect("the grab fails nowhere");
    deleted.expect("the delete fails nowhere");
    assert!(matches!(grabbed, Grab::Taken { .. }), "{grabbed:?}");
    // The delete came after the grab, so nothing of the grabber is left, not even the shells it paid.
    assert_eq!(credits(&db, grabber).await, 0);
    assert_eq!(ledger(&db, grabber).await, 0);
    sqlx::query("DELETE FROM collection_drops WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn moving_a_party_game_while_a_paid_player_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [host, other, guild, channel] = ids(7);
    let deadline = Utc::now() + Duration::hours(1);
    let game = db
        .create_party(NewParty {
            kind: "hotpearl",
            guild_id: guild,
            channel_id: channel,
            host,
            host_name: "Host",
            rounds: 1,
            status: "lobby",
            phase: "lobby",
            state: "{}",
            deadline,
        })
        .await
        .unwrap();
    assert!(matches!(
        db.join_party(game.id, other, "Other", 8, Utc::now()).await.unwrap(),
        pinchora_db::PartyJoin::Joined
    ));
    let game = db.party(game.id).await.unwrap().expect("the game");
    let (id, seq) = (game.id, game.seq);
    let (moved, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM party_games WHERE id = $1 FOR UPDATE",
        id,
        other,
        move |db| async move {
            db.commit_party(
                id,
                seq,
                Commit {
                    phase: "done",
                    round: 1,
                    state: "{}",
                    deadline,
                    status: "done",
                    players: &[PlayerChange {
                        user: other,
                        score: 1,
                        ..Default::default()
                    }],
                    entries: &[],
                    pay: Some(Pay {
                        game: "hotpearl",
                        day: 20_000,
                        cap: 5,
                        reason: "party_win",
                        shells: &[(host, 7), (other, 5)],
                    }),
                },
            )
            .await
        },
    )
    .await;
    let moved = moved.expect("the move fails nowhere").expect("nobody moved first");
    deleted.expect("the delete fails nowhere");
    assert_eq!(moved.paid, vec![(host, 7), (other, 5)]);
    assert_eq!((credits(&db, host).await, ledger(&db, host).await), (7, 7));
    assert_eq!(credits(&db, other).await, 0, "the delete took the prize with the rest");
    db.delete_user_data(host, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn closing_a_contest_week_while_a_winner_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [leaving, second, third, voter, guild] = ids(7);
    let week = 100;
    for user in [leaving, second, third] {
        sqlx::query(
            "INSERT INTO contest_entries (guild_id, week, user_id, owner_name, room_idx, tier, room_type, wallpaper, floor)
             VALUES ($1, $2, $3, 'Name', 0, 1, 'living', 'w', 'f')",
        )
        .bind(guild as i64)
        .bind(week)
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    }
    // The leaving player leads, the second has one vote, the third none.
    for (entry, by) in [(leaving, voter), (leaving, second), (second, voter)] {
        assert!(db.contest_vote(guild, week, entry, by).await.unwrap());
    }
    // Their delete is under way: the standings still show them first.
    let mut delete = db.pool().begin().await.unwrap();
    Db::delete_user_data_in(&mut delete, leaving, Utc::now()).await.unwrap();
    let (closer, closer_pid) = side().await;
    let closing = tokio::spawn(async move {
        closer
            .close_contest_weeks(guild, week + 1, &[500, 250, 100], "house_of_the_week")
            .await
    });
    queued(&db, closer_pid, "the closing").await;
    delete.commit().await.unwrap();

    let closed = closing.await.unwrap().expect("closing fails nowhere");
    assert_eq!(closed, vec![week]);
    let result = db
        .last_contest_result(guild, week + 1)
        .await
        .unwrap()
        .expect("a result");
    assert_eq!(
        result.winner_ids,
        vec![second as i64],
        "the player who left is neither paid nor stored; the one with votes left wins"
    );
    assert_eq!(credits(&db, second).await, 500);
    assert_eq!((credits(&db, leaving).await, ledger(&db, leaving).await), (0, 0));
    assert_eq!(credits(&db, third).await, 0, "no votes, no prize");
    for user in [second, third, voter] {
        db.delete_user_data(user, Utc::now()).await.unwrap();
    }
}

#[tokio::test]
async fn voting_for_an_entry_while_its_owner_deletes_their_data() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db_with(4).await else { return };
    let [owner, voter, guild] = ids(7);
    sqlx::query(
        "INSERT INTO contest_entries (guild_id, week, user_id, owner_name, room_idx, tier, room_type, wallpaper, floor)
         VALUES ($1, 7, $2, 'Name', 0, 1, 'living', 'w', 'f')",
    )
    .bind(guild as i64)
    .bind(owner as i64)
    .execute(db.pool())
    .await
    .unwrap();
    // The owner's delete has taken the entry; the vote, sent just before, waits for it and finds it gone.
    let mut delete = db.pool().begin().await.unwrap();
    Db::delete_user_data_in(&mut delete, owner, Utc::now()).await.unwrap();
    let (voting, voting_pid) = side().await;
    let vote = tokio::spawn(async move { voting.contest_vote(guild, 7, owner, voter).await });
    queued(&db, voting_pid, "the vote").await;
    delete.commit().await.unwrap();
    assert!(!vote.await.unwrap().expect("the vote fails nowhere"), "nothing counted");
}
