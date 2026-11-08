//! Giveaways, suggestions and polls against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is
//! set; skipped otherwise.

use chrono::{Duration, Utc};
use pinchora_db::{
    Db, Decision, EntryToggle, GiveawayOpen, GiveawayRow, MAX_OPEN_POLLS, MAX_RUNNING_GIVEAWAYS, NewGiveaway, NewPoll,
    PollOpen, PollVote, Reroll, SuggestionVote,
};

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 10, std::time::Duration::from_secs(60))
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

fn first_n(pool: &[u64], n: usize) -> Vec<u64> {
    pool.iter().copied().take(n).collect()
}

async fn open(db: &Db, guild: u64, host: u64, shells: i64, winners: i32) -> GiveawayRow {
    let new = NewGiveaway {
        guild_id: guild,
        channel_id: 1,
        host,
        prize: "Prize",
        shells,
        winners,
        required_role: None,
        ends_at: Utc::now() + Duration::hours(1),
    };
    match db.create_giveaway(new).await.unwrap() {
        GiveawayOpen::Started(row) => *row,
        other => panic!("giveaway not started: {other:?}"),
    }
}

async fn ledger(db: &Db, users: &[u64]) -> i64 {
    let (sum,): (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM credit_ledger WHERE user_id = ANY($1)")
            .bind(users.iter().map(|u| *u as i64).collect::<Vec<_>>())
            .fetch_one(db.pool())
            .await
            .unwrap();
    sum
}

#[tokio::test]
async fn a_giveaway_holds_the_prize_and_gives_back_what_nobody_won() {
    let Some(db) = db().await else { return };
    let [guild, host, poor, a, b] = ids();
    db.earn_shells(host, 500, "test").await.unwrap();

    // Not enough shells: nothing is held, nothing is created.
    let broke = NewGiveaway {
        guild_id: guild,
        channel_id: 1,
        host: poor,
        prize: "Too much",
        shells: 10,
        winners: 2,
        required_role: None,
        ends_at: Utc::now() + Duration::hours(1),
    };
    assert!(matches!(
        db.create_giveaway(broke).await.unwrap(),
        GiveawayOpen::Broke { balance: 0 }
    ));
    assert!(db.running_giveaways(guild).await.unwrap().is_empty());

    // 3 winners x 40 shells are held; two entrants win, the third prize comes back.
    let giveaway = open(&db, guild, host, 40, 3).await;
    assert_eq!(db.user(host).await.unwrap().credits, 380);
    assert_eq!(
        db.toggle_giveaway_entry(giveaway.id, a, Utc::now()).await.unwrap(),
        EntryToggle::Entered(1)
    );
    assert_eq!(
        db.toggle_giveaway_entry(giveaway.id, b, Utc::now()).await.unwrap(),
        EntryToggle::Entered(2)
    );
    assert_eq!(
        db.toggle_giveaway_entry(giveaway.id, b, Utc::now()).await.unwrap(),
        EntryToggle::Left(1)
    );
    assert_eq!(
        db.toggle_giveaway_entry(giveaway.id, b, Utc::now()).await.unwrap(),
        EntryToggle::Entered(2)
    );
    assert!(
        db.end_giveaway(giveaway.id, Utc::now(), false, first_n)
            .await
            .unwrap()
            .is_none(),
        "not due yet"
    );
    let ended = db
        .end_giveaway(giveaway.id, Utc::now(), true, first_n)
        .await
        .unwrap()
        .expect("ended early");
    assert_eq!(ended.winners.len(), 2);
    assert_eq!(ended.refunded, 40);
    assert_eq!(
        (db.user(a).await.unwrap().credits, db.user(b).await.unwrap().credits),
        (40, 40)
    );
    assert_eq!(db.user(host).await.unwrap().credits, 420, "500 - 120 held + 40 back");
    assert_eq!(
        ledger(&db, &[host, a, b]).await,
        500,
        "only the funding is net: nothing was minted or lost"
    );
    assert_eq!(
        db.toggle_giveaway_entry(giveaway.id, poor, Utc::now()).await.unwrap(),
        EntryToggle::Closed
    );

    // A giveaway nobody entered gives everything back; one whose message failed is taken back whole.
    let lonely = open(&db, guild, host, 25, 2).await;
    assert_eq!(db.user(host).await.unwrap().credits, 370);
    let ended = db
        .end_giveaway(lonely.id, Utc::now(), true, first_n)
        .await
        .unwrap()
        .unwrap();
    assert!(ended.winners.is_empty());
    assert_eq!(db.user(host).await.unwrap().credits, 420);
    let walled = open(&db, guild, host, 10, 1).await;
    assert!(db.abort_giveaway(walled.id).await.unwrap());
    assert!(!db.abort_giveaway(walled.id).await.unwrap());
    assert_eq!(db.user(host).await.unwrap().credits, 420);
    assert!(db.giveaway(walled.id).await.unwrap().is_none());
}

#[tokio::test]
async fn scheduled_giveaway_rejects_early_entries_and_preserves_bonus_weight() {
    let Some(db) = db().await else { return };
    let [guild, host, entrant] = ids();
    let giveaway = open(&db, guild, host, 0, 1).await;
    let start = Utc::now() + Duration::minutes(2);
    sqlx::query("UPDATE giveaways SET status='scheduled',starts_at=$2,ends_at=$3 WHERE id=$1")
        .bind(giveaway.id)
        .bind(start)
        .bind(start + Duration::hours(1))
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(
        db.toggle_giveaway_entry_weighted(giveaway.id, entrant, 4, Utc::now())
            .await
            .unwrap(),
        EntryToggle::Closed
    );
    assert!(
        db.end_giveaway(giveaway.id, Utc::now(), false, first_n)
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("UPDATE giveaways SET status='running' WHERE id=$1 AND starts_at<=$2")
        .bind(giveaway.id)
        .bind(start)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(
        db.toggle_giveaway_entry_weighted(giveaway.id, entrant, 4, start)
            .await
            .unwrap(),
        EntryToggle::Entered(1)
    );
    assert_eq!(
        db.giveaway_entry_weights(giveaway.id).await.unwrap().get(&entrant),
        Some(&4)
    );
}

#[tokio::test]
async fn a_giveaway_ends_once_however_many_pods_try() {
    let Some(db) = db().await else { return };
    let [guild, host, a, b, c, d, e] = ids();
    db.earn_shells(host, 300, "test").await.unwrap();
    let giveaway = open(&db, guild, host, 30, 2).await;
    for user in [a, b, c, d, e] {
        db.toggle_giveaway_entry(giveaway.id, user, Utc::now()).await.unwrap();
    }
    let tasks: Vec<_> = (0..8)
        .map(|_| {
            let db = db.clone();
            tokio::spawn(async move { db.end_giveaway(giveaway.id, Utc::now(), true, first_n).await.unwrap() })
        })
        .collect();
    let mut ended = 0;
    for task in tasks {
        ended += usize::from(task.await.unwrap().is_some());
    }
    assert_eq!(ended, 1, "exactly one caller ends it");
    let paid: i64 = {
        let mut sum = 0;
        for user in [a, b, c, d, e] {
            sum += db.user(user).await.unwrap().credits;
        }
        sum
    };
    assert_eq!(paid, 60, "two winners, paid once");
    assert_eq!(db.user(host).await.unwrap().credits, 240);
    assert_eq!(
        ledger(&db, &[host, a, b, c, d, e]).await,
        300,
        "every shell is accounted for"
    );

    // Announcing is claimed by one caller at a time, and again once the claim lapses.
    let now = Utc::now();
    assert!(db.claim_giveaway_announcement(giveaway.id, now).await.unwrap());
    assert!(
        !db.claim_giveaway_announcement(giveaway.id, now + Duration::seconds(30))
            .await
            .unwrap()
    );
    assert!(
        db.claim_giveaway_announcement(giveaway.id, now + Duration::minutes(3))
            .await
            .unwrap()
    );
    db.mark_giveaway_announced(giveaway.id).await.unwrap();
    assert!(
        !db.claim_giveaway_announcement(giveaway.id, now + Duration::minutes(9))
            .await
            .unwrap()
    );
    assert!(
        db.unannounced_giveaways(1_000)
            .await
            .unwrap()
            .iter()
            .all(|g| g.id != giveaway.id)
    );
}

#[tokio::test]
async fn rerolls_pick_only_entrants_who_have_not_won() {
    let Some(db) = db().await else { return };
    let [guild, other, host, a, b, c] = ids();
    let giveaway = open(&db, guild, host, 0, 1).await;
    for user in [a, b, c] {
        db.toggle_giveaway_entry(giveaway.id, user, Utc::now()).await.unwrap();
    }
    assert!(matches!(
        db.reroll_giveaway(guild, giveaway.id, 1, first_n).await.unwrap(),
        Reroll::StillRunning
    ));
    db.end_giveaway(giveaway.id, Utc::now(), true, first_n)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        db.reroll_giveaway(other, giveaway.id, 1, first_n).await.unwrap(),
        Reroll::Missing
    ));
    let Reroll::Done(row, picked) = db.reroll_giveaway(guild, giveaway.id, 5, first_n).await.unwrap() else {
        panic!("a reroll was expected");
    };
    assert_eq!(picked.len(), 2, "only the two who had not won");
    assert_eq!(row.winner_ids.len(), 3);
    assert!(matches!(
        db.reroll_giveaway(guild, giveaway.id, 1, first_n).await.unwrap(),
        Reroll::NoOneLeft
    ));
}

#[tokio::test]
async fn a_server_runs_a_limited_number_of_giveaways() {
    let Some(db) = db().await else { return };
    let [guild, host] = ids();
    for _ in 0..MAX_RUNNING_GIVEAWAYS {
        open(&db, guild, host, 0, 1).await;
    }
    let new = NewGiveaway {
        guild_id: guild,
        channel_id: 1,
        host,
        prize: "One too many",
        shells: 0,
        winners: 1,
        required_role: None,
        ends_at: Utc::now() + Duration::hours(1),
    };
    assert!(matches!(db.create_giveaway(new).await.unwrap(), GiveawayOpen::Full));
    assert_eq!(
        db.running_giveaways(guild).await.unwrap().len(),
        MAX_RUNNING_GIVEAWAYS as usize
    );
}

#[tokio::test]
async fn polls_take_one_vote_each_and_close_once() {
    let Some(db) = db().await else { return };
    let [guild, author, a, b, c] = ids();
    let options = ["Pizza".to_owned(), "Tacos".to_owned(), "Sushi".to_owned()];
    let new = NewPoll {
        guild_id: guild,
        channel_id: 1,
        author,
        question: "Lunch?",
        options: &options,
        ends_at: Utc::now() + Duration::hours(1),
    };
    let PollOpen::Started(poll) = db.create_poll(new).await.unwrap() else {
        panic!("the poll was not started");
    };
    let now = Utc::now();
    assert_eq!(db.vote_poll(poll.id, a, 0, now).await.unwrap(), PollVote::Cast);
    assert_eq!(db.vote_poll(poll.id, a, 1, now).await.unwrap(), PollVote::Changed);
    assert_eq!(db.vote_poll(poll.id, b, 1, now).await.unwrap(), PollVote::Cast);
    assert_eq!(db.vote_poll(poll.id, c, 2, now).await.unwrap(), PollVote::Cast);
    assert_eq!(db.vote_poll(poll.id, c, 2, now).await.unwrap(), PollVote::Withdrawn);
    assert_eq!(db.poll_tally(poll.id, 3).await.unwrap(), [0, 2, 0]);
    assert_eq!(db.poll_choice(poll.id, a).await.unwrap(), Some(1));
    assert_eq!(db.poll_choice(poll.id, c).await.unwrap(), None);
    assert_eq!(
        db.vote_poll(poll.id, c, 0, now + Duration::hours(2)).await.unwrap(),
        PollVote::Closed,
        "past its end time"
    );
    assert!(db.close_poll(poll.id, now).await.unwrap().is_none(), "not due yet");

    let later = now + Duration::hours(2);
    let tasks: Vec<_> = (0..6)
        .map(|_| {
            let db = db.clone();
            tokio::spawn(async move { db.close_poll(poll.id, later).await.unwrap() })
        })
        .collect();
    let mut closed = 0;
    for task in tasks {
        closed += usize::from(task.await.unwrap().is_some());
    }
    assert_eq!(closed, 1, "exactly one caller closes it");
    assert_eq!(db.vote_poll(poll.id, c, 0, now).await.unwrap(), PollVote::Closed);
    assert!(db.claim_poll_announcement(poll.id, later).await.unwrap());
    assert!(!db.claim_poll_announcement(poll.id, later).await.unwrap());
}

#[tokio::test]
async fn multiple_choice_votes_toggle_independently_and_close() {
    let Some(db) = db().await else { return };
    let [guild, author, voter] = ids();
    let options = ["A".to_owned(), "B".to_owned(), "C".to_owned()];
    let PollOpen::Started(poll) = db
        .create_poll(NewPoll {
            guild_id: guild,
            channel_id: 1,
            author,
            question: "Choose any",
            options: &options,
            ends_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("poll not started")
    };
    sqlx::query("UPDATE polls SET multiple=TRUE,anonymous=TRUE WHERE id=$1")
        .bind(poll.id)
        .execute(db.pool())
        .await
        .unwrap();
    let now = Utc::now();
    assert_eq!(
        db.vote_poll_multiple(poll.id, voter, 0, now).await.unwrap(),
        PollVote::Cast
    );
    assert_eq!(
        db.vote_poll_multiple(poll.id, voter, 2, now).await.unwrap(),
        PollVote::Cast
    );
    assert_eq!(db.poll_tally(poll.id, 3).await.unwrap(), [1, 0, 1]);
    assert_eq!(
        db.vote_poll_multiple(poll.id, voter, 0, now).await.unwrap(),
        PollVote::Withdrawn
    );
    assert_eq!(db.poll_tally(poll.id, 3).await.unwrap(), [0, 0, 1]);
    assert_eq!(
        db.vote_poll_multiple(poll.id, voter, 1, now + Duration::hours(2))
            .await
            .unwrap(),
        PollVote::Closed
    );
}

#[tokio::test]
async fn a_member_cannot_open_more_polls_than_the_limit_even_all_at_once() {
    let Some(db) = db().await else { return };
    let [guild, author] = ids();
    let tasks: Vec<_> = (0..8)
        .map(|_| {
            let db = db.clone();
            tokio::spawn(async move {
                let options = ["Yes".to_owned(), "No".to_owned()];
                db.create_poll(NewPoll {
                    guild_id: guild,
                    channel_id: 1,
                    author,
                    question: "Again?",
                    options: &options,
                    ends_at: Utc::now() + Duration::hours(1),
                })
                .await
                .unwrap()
            })
        })
        .collect();
    let mut started = 0;
    for task in tasks {
        started += usize::from(matches!(task.await.unwrap(), PollOpen::Started(_)));
    }
    assert_eq!(started, MAX_OPEN_POLLS as usize);
}

#[tokio::test]
async fn suggestions_are_numbered_per_server_voted_on_and_decided_once() {
    let Some(db) = db().await else { return };
    let [guild, other, author, a, b, mod_] = ids();
    assert_eq!(db.suggestion_channel(guild).await.unwrap(), None);
    db.set_suggestion_channel(guild, Some(77)).await.unwrap();
    assert_eq!(db.suggestion_channel(guild).await.unwrap(), Some(77));

    let tasks: Vec<_> = (0..6)
        .map(|n| {
            let db = db.clone();
            tokio::spawn(async move {
                db.create_suggestion(guild, author, &format!("Idea {n}"))
                    .await
                    .unwrap()
                    .number
            })
        })
        .collect();
    let mut numbers = Vec::new();
    for task in tasks {
        numbers.push(task.await.unwrap());
    }
    numbers.sort_unstable();
    assert_eq!(
        numbers,
        [1, 2, 3, 4, 5, 6],
        "each server counts from one, without gaps or repeats"
    );
    db.set_suggestion_channel(guild, None).await.unwrap();
    let after_off = db.create_suggestion(guild, author, "Still counted").await.unwrap();
    assert_eq!(after_off.number, 7, "turning the channel off keeps the count");
    let elsewhere = db.create_suggestion(other, author, "Other server").await.unwrap();
    assert_eq!(elsewhere.number, 1);

    let s = db.create_suggestion(guild, author, "Plants").await.unwrap();
    assert_eq!(db.vote_suggestion(s.id, a, true).await.unwrap(), SuggestionVote::Voted);
    assert_eq!(db.vote_suggestion(s.id, b, true).await.unwrap(), SuggestionVote::Voted);
    assert_eq!(
        db.vote_suggestion(s.id, a, false).await.unwrap(),
        SuggestionVote::Switched
    );
    assert_eq!(
        db.vote_suggestion(s.id, b, true).await.unwrap(),
        SuggestionVote::Withdrawn
    );
    let row = db.suggestion(s.id).await.unwrap().unwrap();
    assert_eq!((row.up, row.down), (0, 1));

    let tasks: Vec<_> = [true, false, true, false]
        .into_iter()
        .map(|approve| {
            let db = db.clone();
            tokio::spawn(async move {
                db.decide_suggestion(guild, s.number, approve, mod_, Some("Because"))
                    .await
                    .unwrap()
            })
        })
        .collect();
    let mut done = 0;
    for task in tasks {
        match task.await.unwrap() {
            Decision::Done(row) => {
                done += 1;
                assert_eq!(row.decided_by, Some(mod_ as i64));
            }
            Decision::Already(_) => {}
            Decision::Missing => panic!("the suggestion exists"),
        }
    }
    assert_eq!(done, 1, "decided exactly once");
    assert_eq!(db.vote_suggestion(s.id, b, true).await.unwrap(), SuggestionVote::Closed);
    assert!(matches!(
        db.decide_suggestion(other, s.number, true, mod_, None).await.unwrap(),
        Decision::Missing | Decision::Already(_)
    ));
    assert!(matches!(
        db.decide_suggestion(guild, 999, true, mod_, None).await.unwrap(),
        Decision::Missing
    ));
}

#[tokio::test]
async fn community_quotas_are_atomic_across_dashboard_and_commands() {
    let Some(db) = db().await else { return };
    let [guild, user] = ids();
    let mut tasks = Vec::new();
    for n in 0..35 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.add_keyword_highlight(guild, user, &format!("word{n}"))
                .await
                .unwrap()
        }));
    }
    let mut added = 0;
    for task in tasks {
        added += usize::from(task.await.unwrap());
    }
    assert_eq!(added, 10);
    let (word,): (String,) = sqlx::query_as("SELECT keyword FROM keyword_highlights WHERE guild_id=$1 LIMIT 1")
        .bind(guild as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(
        db.add_keyword_highlight(guild, user, &word).await.unwrap(),
        "same keyword is idempotent at quota"
    );
    let mut tasks = Vec::new();
    for _ in 0..60 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.add_checklist_item(user, "private item").await.unwrap()
        }));
    }
    let mut added = 0;
    for task in tasks {
        added += usize::from(task.await.unwrap().is_some());
    }
    assert_eq!(added, 50);
    let mut tasks = Vec::new();
    for _ in 0..35 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.add_recurring_reminder(user, "private reminder", 60, Utc::now() + Duration::days(1), "tr")
                .await
                .unwrap()
        }));
    }
    let mut added = 0;
    for task in tasks {
        added += usize::from(task.await.unwrap().is_some());
    }
    assert_eq!(added, 25);
}

#[tokio::test]
async fn confession_bans_reports_and_durable_rate_limit_preserve_anonymity() {
    let Some(db) = db().await else { return };
    let [guild, other, user, reporter, moderator] = ids();
    let id = db
        .submit_confession(guild, user, "private thought")
        .await
        .unwrap()
        .unwrap();
    assert!(
        db.submit_confession(guild, user, "second thought")
            .await
            .unwrap()
            .is_none()
    );
    assert!(!db.ban_confession_author(other, id, moderator, true).await.unwrap());
    assert!(
        !db.report_confession(guild, id, reporter).await.unwrap(),
        "pending confessions are not reportable"
    );
    sqlx::query("UPDATE confessions SET status='approved',created_at=now()-interval '2 days' WHERE id=$1")
        .bind(id)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(db.report_confession(guild, id, reporter).await.unwrap());
    assert!(
        !db.report_confession(guild, id, reporter).await.unwrap(),
        "one report per member"
    );
    assert!(db.ban_confession_author(guild, id, moderator, true).await.unwrap());
    assert!(
        db.submit_confession(guild, user, "blocked thought")
            .await
            .unwrap()
            .is_none()
    );
    assert!(db.ban_confession_author(guild, id, moderator, false).await.unwrap());
    let mut tasks = Vec::new();
    for _ in 0..5 {
        let db = db.clone();
        tasks.push(tokio::spawn(async move {
            db.submit_confession(guild, user, "concurrent thought").await.unwrap()
        }));
    }
    let mut added = 0;
    for task in tasks {
        added += usize::from(task.await.unwrap().is_some());
    }
    assert_eq!(added, 1);
    let (audits,): (i64,) = sqlx::query_as("SELECT count(*) FROM confession_moderation WHERE confession_id=$1")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(audits, 2);
}

#[tokio::test]
async fn poll_storage_rejects_wrong_mode_and_out_of_range_votes() {
    let Some(db) = db().await else { return };
    let [guild, author, voter] = ids();
    let options = vec!["Yes".to_owned(), "No".to_owned()];
    let PollOpen::Started(poll) = db
        .create_poll(NewPoll {
            guild_id: guild,
            channel_id: 1,
            author,
            question: "Question",
            options: &options,
            ends_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("poll")
    };
    for choice in [2, 20, usize::MAX] {
        assert_eq!(
            db.vote_poll(poll.id, voter, choice, Utc::now()).await.unwrap(),
            PollVote::Closed
        );
    }
    assert_eq!(
        db.vote_poll_multiple(poll.id, voter, 0, Utc::now()).await.unwrap(),
        PollVote::Closed
    );
    sqlx::query("UPDATE polls SET multiple=true WHERE id=$1")
        .bind(poll.id)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(
        db.vote_poll(poll.id, voter, 0, Utc::now()).await.unwrap(),
        PollVote::Closed
    );
    assert_eq!(
        db.vote_poll_multiple(poll.id, voter, usize::MAX, Utc::now())
            .await
            .unwrap(),
        PollVote::Closed
    );
    assert_eq!(db.poll_tally(poll.id, 2).await.unwrap(), vec![0, 0]);
}

#[tokio::test]
async fn weighted_draw_and_reroll_use_transaction_entry_weights() {
    let Some(db) = db().await else { return };
    let [guild, host, user, other] = ids();
    let draw = open(&db, guild, host, 0, 1).await;
    db.toggle_giveaway_entry_weighted(draw.id, user, 21, Utc::now())
        .await
        .unwrap();
    db.toggle_giveaway_entry_weighted(draw.id, other, 1, Utc::now())
        .await
        .unwrap();
    // The draw's picker receives the weights read in its own transaction, not a stale caller snapshot.
    let ended = db
        .end_giveaway_weighted(draw.id, Utc::now(), true, |pool, n, weights| {
            assert_eq!(weights.get(&user), Some(&21));
            assert_eq!(weights.len(), pool.len());
            first_n(pool, n)
        })
        .await
        .unwrap()
        .unwrap();
    let winner = ended.winners[0];
    let result = db
        .reroll_giveaway_weighted(guild, draw.id, 1, |pool, n, weights| {
            assert!(!pool.contains(&winner));
            assert_eq!(weights.get(&user), Some(&21));
            first_n(pool, n)
        })
        .await
        .unwrap();
    assert!(matches!(result,Reroll::Done(_,ids) if !ids.contains(&winner)));
}
