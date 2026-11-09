use crate::support::db;
use crate::support::id;
use chrono::{Duration, Utc};
use pinchora_db::{NewGame, ReefMove};
fn new(host: u64, channel: u64) -> NewGame<'static> {
    NewGame {
        kind: "d9:reefquiz",
        guild_id: host,
        channel_id: channel,
        host,
        guest: None,
        bet: 0,
        status: "active",
        state: "initial",
        ttl_secs: 90,
    }
}
#[tokio::test]
async fn concurrent_opens_and_finishes_have_one_winner() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else {
        return;
    };
    let (host, channel) = (id(), id() + 1);
    let now = Utc::now();
    let n = new(host, channel);
    let (a, b) = tokio::join!(db.reef_open(n, "Host", now), db.reef_open(n, "Host", now));
    let game = a.unwrap().or(b.unwrap()).unwrap();
    let rows: Vec<(i64,)> = sqlx::query_as("SELECT id FROM game_sessions WHERE channel_id=$1 AND status<>'done'")
        .bind(channel as i64)
        .fetch_all(db.pool())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let make = || ReefMove {
        state: "finished",
        status: "done",
        deadline: now,
        now,
        timeout: false,
        join: None,
        results: &[(0, 5, true, true)],
    };
    let (a, b) = tokio::join!(db.reef_move(&game, make()), db.reef_move(&game, make()));
    assert_ne!(a.unwrap().is_some(), b.unwrap().is_some());
    assert_eq!(db.user(host).await.unwrap().credits, 15);
    assert_eq!(db.game_stats(host).await.unwrap()[0].played, 1);
}
#[tokio::test]
async fn daily_cap_is_shared_across_sessions() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else {
        return;
    };
    let host = id();
    let now = Utc::now();
    for _ in 0..9 {
        let game = db.reef_open(new(host, id()), "Host", now).await.unwrap().unwrap();
        db.reef_move(
            &game,
            ReefMove {
                state: "done",
                status: "done",
                deadline: now,
                now,
                timeout: false,
                join: None,
                results: &[(0, 5, true, true)],
            },
        )
        .await
        .unwrap()
        .unwrap();
    }
    assert_eq!(db.user(host).await.unwrap().credits, 90);
    assert_eq!(db.game_stats(host).await.unwrap()[0].played, 9);
}
#[tokio::test]
async fn deadline_and_stale_membership_cannot_move() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else {
        return;
    };
    let host = id();
    let now = Utc::now();
    let game = db.reef_open(new(host, id()), "Host", now).await.unwrap().unwrap();
    let late = now + Duration::seconds(100);
    assert!(
        db.reef_move(
            &game,
            ReefMove {
                state: "bad",
                status: "active",
                deadline: late,
                now: late,
                timeout: false,
                join: None,
                results: &[]
            }
        )
        .await
        .unwrap()
        .is_none()
    );
    let done = db
        .reef_move(
            &game,
            ReefMove {
                state: "expired",
                status: "done",
                deadline: late,
                now: late,
                timeout: true,
                join: None,
                results: &[],
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(done.status, "done");
    assert!(db.game_stats(host).await.unwrap().is_empty());
}
#[tokio::test]
async fn legacy_multiplayer_creation_cannot_overlap() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else {
        return;
    };
    let (host, channel) = (id(), id() + 1);
    let now = Utc::now();
    let game = db.reef_open(new(host, channel), "Host", now).await.unwrap().unwrap();
    let mut other = new(id(), channel);
    other.kind = "trivia";
    assert!(db.create_game(other).await.is_err());
    assert!(
        db.create_party(pinchora_db::NewParty {
            kind: "hotpearl",
            guild_id: host,
            channel_id: channel,
            host: id(),
            host_name: "Guest",
            rounds: 1,
            status: "lobby",
            phase: "lobby",
            state: "{}",
            deadline: now + Duration::minutes(1)
        })
        .await
        .is_err()
    );
    db.settle_game(game.id, None, "done", pinchora_db::Payout::None, false)
        .await
        .unwrap();
}

#[tokio::test]
async fn erasure_removes_seat_identity_and_cannot_restore_deleted_rewards() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else {
        return;
    };
    let host = id();
    let guest = id() + 1;
    let now = Utc::now();
    let game = db.reef_open(new(host, id()), "Host", now).await.unwrap().unwrap();
    let game = db
        .reef_move(
            &game,
            ReefMove {
                state: "joined",
                status: "active",
                deadline: now + Duration::seconds(90),
                now,
                timeout: false,
                join: Some((guest, "Guest", 1)),
                results: &[],
            },
        )
        .await
        .unwrap()
        .unwrap();
    db.delete_user_data(guest, now).await.unwrap();
    db.reef_move(
        &game,
        ReefMove {
            state: "done",
            status: "done",
            deadline: now,
            now,
            timeout: false,
            join: None,
            results: &[(0, 1, true, true), (1, 2, true, true)],
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert!(db.game_stats(guest).await.unwrap().is_empty());
    assert!(
        !db.party_members(game.id)
            .await
            .unwrap()
            .iter()
            .any(|p| p.user_id == guest as i64)
    );
    db.delete_user_data(host, now).await.unwrap();
    assert!(db.game(game.id).await.unwrap().is_none());
}

#[tokio::test]
async fn concurrent_session_rewards_cannot_exceed_the_daily_cap() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let host = id();
    let now = Utc::now();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..12 {
        let game = db.reef_open(new(host, id()), "Host", now).await.unwrap().unwrap();
        let db = db.clone();
        tasks.spawn(async move {
            db.reef_move(
                &game,
                ReefMove {
                    state: "done",
                    status: "done",
                    deadline: now,
                    now,
                    timeout: false,
                    join: None,
                    results: &[(0, 5, true, true)],
                },
            )
            .await
            .unwrap()
            .unwrap();
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    assert_eq!(db.user(host).await.unwrap().credits, 90);
    assert_eq!(db.game_stats(host).await.unwrap()[0].played, 12);
}

#[tokio::test]
async fn concurrent_legacy_and_reef_starts_share_the_channel_lock() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    for _ in 0..8 {
        let now = Utc::now();
        let channel = id();
        let reef = new(id(), channel);
        let mut legacy = new(id(), channel);
        legacy.kind = "trivia";
        let (a, b) = tokio::join!(db.reef_open(reef, "Host", now), db.create_game(legacy));
        assert_ne!(a.unwrap().is_some(), b.is_ok());
        let (count,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM game_sessions WHERE channel_id=$1 AND status<>'done'")
                .bind(channel as i64)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(count, 1);
    }
}
