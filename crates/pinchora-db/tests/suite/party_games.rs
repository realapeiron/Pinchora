//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.

use chrono::{DateTime, Duration, Utc};
use pinchora_db::{
    Commit, Db, NewEntry, NewParty, PartyGame, PartyJoin, PartyLeave, PartyVote, Pay, PlayerChange, PollChoice,
};

use crate::support::db_eight as db;

/// Ids unique per run, so the suite can run repeatedly against one database.
use crate::support::ids;

fn soon() -> DateTime<Utc> {
    Utc::now() + Duration::minutes(5)
}

async fn lobby(db: &Db, kind: &str, host: u64, channel: u64) -> PartyGame {
    db.create_party(NewParty {
        kind,
        guild_id: host,
        channel_id: channel,
        host,
        host_name: "Host",
        rounds: 3,
        status: "lobby",
        phase: "lobby",
        state: "{}",
        deadline: soon(),
    })
    .await
    .unwrap()
}

/// Moves a lobby to `playing` in `phase`, round 1, with no other changes.
fn into<'a>(phase: &'a str, game: &PartyGame) -> Commit<'a> {
    Commit {
        phase,
        round: 1,
        state: "{}",
        deadline: game.deadline,
        status: "playing",
        players: &[],
        entries: &[],
        pay: None,
    }
}

async fn shells(db: &Db, user: u64) -> i64 {
    db.user(user).await.map(|u| u.credits).unwrap_or(0)
}

async fn ledger_total(db: &Db, user: u64) -> i64 {
    let (sum,): (i64,) = sqlx::query_as("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM credit_ledger WHERE user_id = $1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    sum
}

#[tokio::test]
async fn a_lobby_takes_players_up_to_its_limit_even_when_they_all_click_at_once() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [host, channel, ..] = ids::<14>();
    let game = lobby(&db, "hotpearl", host, channel).await;
    let now = Utc::now();

    // Ten clicks at once, each on its own task, racing through the connection pool.
    let id = game.id;
    let clicks: Vec<_> = (1..=10)
        .map(|n| {
            let db = db.clone();
            tokio::spawn(async move { db.join_party(id, host + 100 + n, "Guest", 4, now).await.unwrap() })
        })
        .collect();
    let mut results = Vec::new();
    for click in clicks {
        results.push(click.await.unwrap());
    }
    let joined = results.iter().filter(|r| **r == PartyJoin::Joined).count();
    let full = results.iter().filter(|r| **r == PartyJoin::Full).count();
    assert_eq!((joined, full), (3, 7), "the host holds the fourth seat: {results:?}");
    assert_eq!(db.party_players(game.id).await.unwrap().len(), 4);

    // The same person twice is told so, and so is somebody joining a lobby that has started.
    assert_eq!(
        db.join_party(game.id, host, "Host", 4, now).await.unwrap(),
        PartyJoin::Already
    );
    let moved = db
        .commit_party(
            game.id,
            db.party(game.id).await.unwrap().unwrap().seq,
            into("turn", &game),
        )
        .await;
    assert!(moved.unwrap().is_some());
    assert_eq!(
        db.join_party(game.id, host + 999, "Late", 4, now).await.unwrap(),
        PartyJoin::Closed
    );
    // An expired lobby is closed to joiners too.
    let old = lobby(&db, "hotpearl", host, channel + 1).await;
    assert_eq!(
        db.join_party(old.id, host + 5, "Slow", 4, now + Duration::minutes(10))
            .await
            .unwrap(),
        PartyJoin::Closed
    );
}

#[tokio::test]
async fn leaving_a_lobby_frees_the_seat_but_the_host_cannot_leave() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [host, guest, stranger, channel] = ids();
    let game = lobby(&db, "acronym", host, channel).await;
    let now = Utc::now();
    assert_eq!(
        db.join_party(game.id, guest, "Guest", 10, now).await.unwrap(),
        PartyJoin::Joined
    );
    assert_eq!(db.leave_party(game.id, host, now).await.unwrap(), PartyLeave::Host);
    assert_eq!(db.leave_party(game.id, stranger, now).await.unwrap(), PartyLeave::NotIn);
    assert_eq!(db.leave_party(game.id, guest, now).await.unwrap(), PartyLeave::Left);
    assert_eq!(db.party_players(game.id).await.unwrap().len(), 1);
    assert_eq!(db.leave_party(game.id, guest, now).await.unwrap(), PartyLeave::NotIn);
    // Each change moved the sequence, so a start made on an older picture is refused.
    let after = db.party(game.id).await.unwrap().unwrap();
    assert_eq!(after.seq, 2, "one for the join, one for the leave");
    assert!(
        db.commit_party(game.id, game.seq, into("write", &game))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn two_moves_from_the_same_picture_apply_once_and_pay_once() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [host, rival, channel, ..] = ids::<4>();
    let game = lobby(&db, "hotpearl", host, channel).await;
    db.join_party(game.id, rival, "Rival", 8, Utc::now()).await.unwrap();
    let seq = db.party(game.id).await.unwrap().unwrap().seq;
    let day = 20_000;
    let finish = |winner: u64| {
        let db = db.clone();
        let game = game.clone();
        async move {
            let shells = [(winner, 30)];
            db.commit_party(
                game.id,
                seq,
                Commit {
                    phase: "done",
                    round: 4,
                    state: "{}",
                    deadline: soon(),
                    status: "done",
                    players: &[PlayerChange {
                        user: winner,
                        score: 1,
                        ..Default::default()
                    }],
                    entries: &[NewEntry {
                        round: 4,
                        user: winner,
                        text: "pearl",
                    }],
                    pay: Some(Pay {
                        game: "hotpearl",
                        day,
                        cap: 6,
                        reason: "hotpearl_win",
                        shells: &shells,
                    }),
                },
            )
            .await
            .unwrap()
        }
    };
    let (a, b) = tokio::join!(tokio::spawn(finish(host)), tokio::spawn(finish(rival)));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert!(a.is_some() ^ b.is_some(), "exactly one move wins the race");
    let winner = if a.is_some() { host } else { rival };
    let loser = if a.is_some() { rival } else { host };
    let done = a.or(b).unwrap();
    assert_eq!(done.paid, vec![(winner, 30)]);
    assert_eq!(shells(&db, winner).await, 30);
    assert_eq!(shells(&db, loser).await, 0);
    assert_eq!(ledger_total(&db, winner).await, 30, "one ledger line, not two");
    assert!(done.game.is_done());
    let players = db.party_players(game.id).await.unwrap();
    assert_eq!(
        players.iter().map(|p| p.score).sum::<i32>(),
        1,
        "the loser's changes were never applied"
    );
    // A finished game takes no more moves.
    assert!(
        db.commit_party(game.id, done.game.seq, into("turn", &game))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_player_is_paid_for_a_few_games_a_day_and_not_for_the_rest() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [host, channel, ..] = ids::<3>();
    let day = 20_001;
    let mut paid = Vec::new();
    for round in 0..4 {
        let game = lobby(&db, "wordrace", host, channel + round).await;
        let shells_won = [(host, 25), (channel + 77, 0)];
        let moved = db
            .commit_party(
                game.id,
                game.seq,
                Commit {
                    phase: "done",
                    round: 1,
                    state: "{}",
                    deadline: soon(),
                    status: "done",
                    players: &[],
                    entries: &[],
                    pay: Some(Pay {
                        game: "wordrace",
                        day,
                        cap: 2,
                        reason: "wordrace_win",
                        shells: &shells_won,
                    }),
                },
            )
            .await
            .unwrap()
            .unwrap();
        paid.push(moved.paid);
    }
    // Only the first two pay; the player with nothing to earn never appears.
    assert_eq!(
        paid,
        vec![vec![(host, 25)], vec![(host, 25)], vec![(host, 0)], vec![(host, 0)]]
    );
    assert_eq!(shells(&db, host).await, 50);
    assert_eq!(ledger_total(&db, host).await, 50);
    // Another game is counted on its own, and so is another day.
    let other = lobby(&db, "hotpearl", host, channel + 9).await;
    let moved = db
        .commit_party(
            other.id,
            other.seq,
            Commit {
                phase: "done",
                round: 1,
                state: "{}",
                deadline: soon(),
                status: "done",
                players: &[],
                entries: &[],
                pay: Some(Pay {
                    game: "hotpearl",
                    day,
                    cap: 2,
                    reason: "hotpearl_win",
                    shells: &[(host, 10)],
                }),
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(moved.paid, vec![(host, 10)]);
}

#[tokio::test]
async fn written_entries_and_votes_follow_the_phase_the_round_and_the_clock() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [host, a, b, outsider, channel] = ids();
    let game = lobby(&db, "acronym", host, channel).await;
    let now = Utc::now();
    db.join_party(game.id, a, "A", 10, now).await.unwrap();
    db.join_party(game.id, b, "B", 10, now).await.unwrap();
    let seq = db.party(game.id).await.unwrap().unwrap().seq;
    let game = db
        .commit_party(game.id, seq, into("write", &game))
        .await
        .unwrap()
        .unwrap()
        .game;

    // Entries: players only, in the write phase of the round, before the deadline; a rewrite replaces.
    assert_eq!(
        db.put_entry(game.id, 1, "write", a, "First try", now).await.unwrap(),
        Some(true)
    );
    assert_eq!(
        db.put_entry(game.id, 1, "write", a, "Second try", now).await.unwrap(),
        Some(false)
    );
    assert_eq!(
        db.put_entry(game.id, 1, "write", outsider, "Hi", now).await.unwrap(),
        None
    );
    assert_eq!(
        db.put_entry(game.id, 2, "write", b, "Wrong round", now).await.unwrap(),
        None
    );
    assert_eq!(
        db.put_entry(game.id, 1, "vote", b, "Wrong phase", now).await.unwrap(),
        None
    );
    assert_eq!(
        db.put_entry(game.id, 1, "write", b, "Too late", now + Duration::hours(1))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        db.put_entry(game.id, 1, "write", b, "B's line", now).await.unwrap(),
        Some(true)
    );
    assert_eq!(
        db.put_entry(game.id, 1, "write", host, "Host line", now).await.unwrap(),
        Some(true)
    );
    let entries = db.party_entries(game.id, 1).await.unwrap();
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().any(|e| e.author() == a && e.text == "Second try"));
    // The order does not change from one look to the next.
    let again = db.party_entries(game.id, 1).await.unwrap();
    assert_eq!(
        entries.iter().map(|e| e.author()).collect::<Vec<_>>(),
        again.iter().map(|e| e.author()).collect::<Vec<_>>()
    );

    // Not voting yet.
    assert_eq!(db.put_vote(game.id, 1, a, b, now).await.unwrap(), PartyVote::Closed);
    let seq = db.party(game.id).await.unwrap().unwrap().seq;
    let game = db
        .commit_party(game.id, seq, into("vote", &game))
        .await
        .unwrap()
        .unwrap()
        .game;
    assert_eq!(db.put_vote(game.id, 1, a, a, now).await.unwrap(), PartyVote::OwnEntry);
    assert_eq!(
        db.put_vote(game.id, 1, outsider, a, now).await.unwrap(),
        PartyVote::NotPlayer
    );
    assert_eq!(
        db.put_vote(game.id, 1, a, outsider, now).await.unwrap(),
        PartyVote::NoEntry
    );
    assert_eq!(db.put_vote(game.id, 2, a, b, now).await.unwrap(), PartyVote::Closed);
    assert_eq!(db.put_vote(game.id, 1, a, b, now).await.unwrap(), PartyVote::Counted);
    assert_eq!(db.put_vote(game.id, 1, a, b, now).await.unwrap(), PartyVote::Same);
    assert_eq!(db.put_vote(game.id, 1, a, host, now).await.unwrap(), PartyVote::Changed);
    assert_eq!(db.put_vote(game.id, 1, b, host, now).await.unwrap(), PartyVote::Counted);
    assert_eq!(
        db.put_vote(game.id, 1, host, b, now + Duration::hours(1))
            .await
            .unwrap(),
        PartyVote::Closed
    );
    let votes = db.party_votes(game.id, 1).await.unwrap();
    assert_eq!(votes.len(), 2);
    assert!(
        votes.iter().all(|(_, pick)| *pick == host),
        "a's vote moved to the host: {votes:?}"
    );
    // No entry can be written once voting has begun.
    assert_eq!(db.put_entry(game.id, 1, "write", a, "Sneaky", now).await.unwrap(), None);
}

#[tokio::test]
async fn race_boards_take_six_guesses_and_stop_once_the_word_is_found() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [host, rival, channel, ..] = ids::<4>();
    let game = lobby(&db, "wordrace", host, channel).await;
    db.join_party(game.id, rival, "Rival", 4, Utc::now()).await.unwrap();
    let seq = db.party(game.id).await.unwrap().unwrap().seq;
    let game = db
        .commit_party(game.id, seq, into("race", &game))
        .await
        .unwrap()
        .unwrap()
        .game;
    let now = Utc::now();
    for guess in ["crane", "stomp"] {
        assert!(
            db.race_guess(game.id, host, guess, "pearl", now)
                .await
                .unwrap()
                .is_some()
        );
    }
    let solved = db
        .race_guess(game.id, host, "pearl", "pearl", now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(solved.guesses, vec!["crane", "stomp", "pearl"]);
    // The word was found: a double click adds nothing.
    assert!(
        db.race_guess(game.id, host, "pearl", "pearl", now)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.race_guess(game.id, host, "fight", "pearl", now)
            .await
            .unwrap()
            .is_none()
    );
    // The rival's board is theirs alone, and stops at six.
    for n in 0..6 {
        let word = format!("guess{n}");
        assert!(
            db.race_guess(game.id, rival, &word, "pearl", now)
                .await
                .unwrap()
                .is_some()
        );
    }
    assert!(
        db.race_guess(game.id, rival, "seven", "pearl", now)
            .await
            .unwrap()
            .is_none()
    );
    // Not after the clock, and not by someone who is not playing.
    assert!(
        db.race_guess(game.id, host, "later", "pearl", now + Duration::hours(1))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.race_guess(game.id, channel, "crane", "pearl", now)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_poll_counts_one_vote_each_and_lets_people_change_their_mind() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db().await else { return };
    let [host, a, b, channel] = ids();
    let poll = db
        .create_party(NewParty {
            kind: "wouldyourrather",
            guild_id: host,
            channel_id: channel,
            host,
            host_name: "Host",
            rounds: 1,
            status: "playing",
            phase: "vote",
            state: "{}",
            deadline: soon(),
        })
        .await
        .unwrap();
    let now = Utc::now();
    assert_eq!(db.put_poll_vote(poll.id, a, 0, now).await.unwrap(), PollChoice::First);
    assert_eq!(db.put_poll_vote(poll.id, a, 0, now).await.unwrap(), PollChoice::Same);
    assert_eq!(db.put_poll_vote(poll.id, b, 1, now).await.unwrap(), PollChoice::First);
    assert_eq!(db.poll_counts(poll.id).await.unwrap(), (1, 1));
    assert_eq!(db.put_poll_vote(poll.id, a, 1, now).await.unwrap(), PollChoice::Changed);
    assert_eq!(db.poll_counts(poll.id).await.unwrap(), (0, 2));
    let mut voters = db.poll_voters(poll.id).await.unwrap();
    voters.sort_unstable();
    assert_eq!(voters.len(), 2);
    assert!(voters.iter().all(|(_, side)| *side == 1));
    // Closed by the clock, and a poll does not keep its channel busy.
    assert_eq!(
        db.put_poll_vote(poll.id, host, 0, now + Duration::hours(1))
            .await
            .unwrap(),
        PollChoice::Closed
    );
    assert!(db.party_in_channel(channel, now).await.unwrap().is_none());
    // Votes on something that is not a poll are refused.
    let game = lobby(&db, "hotpearl", host, channel + 1).await;
    assert_eq!(db.put_poll_vote(game.id, a, 0, now).await.unwrap(), PollChoice::Closed);
}

#[tokio::test]
async fn due_games_come_oldest_first_and_a_channel_holds_one_game() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db().await else { return };
    // The exclusive guard makes old fixtures safe to clear before the bounded worker scan.
    sqlx::query("DELETE FROM party_games").execute(db.pool()).await.unwrap();
    let [host, channel, ..] = ids::<3>();
    let now = Utc::now();
    let game = lobby(&db, "acronym", host, channel).await;
    assert_eq!(
        db.party_in_channel(channel, now).await.unwrap().map(|g| g.id),
        Some(game.id)
    );
    assert!(db.party_in_channel(channel + 1, now).await.unwrap().is_none());

    assert!(db.due_parties(now, 50).await.unwrap().iter().all(|g| g.id != game.id));
    let due = db.due_parties(now + Duration::minutes(10), 1000).await.unwrap();
    assert!(due.iter().any(|g| g.id == game.id));
    assert!(due.windows(2).all(|w| w[0].deadline <= w[1].deadline), "oldest first");

    // A game far past its deadline no longer blocks its channel; a finished one never does.
    assert!(
        db.party_in_channel(channel, now + Duration::hours(1))
            .await
            .unwrap()
            .is_none()
    );
    db.commit_party(
        game.id,
        game.seq,
        Commit {
            phase: "done",
            round: 0,
            state: "{}",
            deadline: now,
            status: "done",
            players: &[],
            entries: &[],
            pay: None,
        },
    )
    .await
    .unwrap();
    assert!(db.party_in_channel(channel, now).await.unwrap().is_none());
    assert!(
        db.due_parties(now + Duration::minutes(10), 1000)
            .await
            .unwrap()
            .iter()
            .all(|g| g.id != game.id)
    );
}

#[tokio::test]
async fn message_ids_the_strict_setting_and_cleanup() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db().await else { return };
    let [host, channel, guild, ..] = ids::<3>();
    let game = lobby(&db, "postcard", host, channel).await;
    assert!(game.location().is_none());
    db.set_party_message(game.id, channel + 5, 99).await.unwrap();
    assert_eq!(
        db.party(game.id).await.unwrap().unwrap().location(),
        Some((channel + 5, 99))
    );

    assert!(!db.party_strict(guild).await.unwrap());
    db.set_party_strict(guild, true).await.unwrap();
    assert!(db.party_strict(guild).await.unwrap());
    db.set_party_strict(guild, false).await.unwrap();
    assert!(!db.party_strict(guild).await.unwrap());

    // Old finished games go with their players; running ones stay.
    let done = db
        .commit_party(
            game.id,
            game.seq,
            Commit {
                phase: "done",
                round: 0,
                state: "{}",
                deadline: Utc::now(),
                status: "done",
                players: &[],
                entries: &[],
                pay: None,
            },
        )
        .await
        .unwrap()
        .unwrap();
    let kept = lobby(&db, "hotpearl", host, channel + 1).await;
    let gone = db.forget_old_parties(Utc::now() + Duration::days(1), 0).await.unwrap();
    assert!(gone >= 1);
    assert!(db.party(done.game.id).await.unwrap().is_none());
    assert!(db.party_players(done.game.id).await.unwrap().is_empty());
    assert!(db.party(kept.id).await.unwrap().is_some());
}
