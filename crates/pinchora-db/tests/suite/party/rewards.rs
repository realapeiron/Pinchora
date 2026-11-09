use crate::party::*;

#[tokio::test]
async fn gambling_is_off_until_a_manager_turns_it_on() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
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
async fn the_shell_game_holds_the_stake_pays_a_right_guess_and_refunds_a_forgotten_game() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
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
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
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
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
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
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
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
async fn a_shell_game_is_settled_on_the_day_the_table_started() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
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
