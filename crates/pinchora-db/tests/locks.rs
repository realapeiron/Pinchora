//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! The lock order of `pinchora_db`'s transactions (player locks first, then the object, then the
//! writes): the interleavings that used to end in a deadlock, and a mixed load with deletes that
//! must give no player an error and lose or make no shells.

use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use chrono::{Duration, NaiveDate, Utc};
use pinchora_core::economy::Period;
use pinchora_db::{
    ACTIVE, Award, BottlePrize, Commit, DROP_BOTTLE, DROP_CRITTERS, Daily, Db, DerbyJoin, DuelStart, GiveawayOpen,
    Grab, NewDrop, NewGame, NewGiveaway, NewParty, NewRoom, PENDING, Pay, Payout, PlayerChange, Reroll, ShellLimits,
    ShellPick, ShellStart, StallBuy, TripClaim, TripPayout, Wake, XpGrant,
};

// Global cleanup must not remove the sweep test's old fixtures before it can lock them.
// Mixed-load sweeps share a read lock, so they still run concurrently with one another.
static SWEEP_FIXTURES: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

async fn db_with(pool: u32) -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, pool, std::time::Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    Some(db)
}

/// Ids unique per run, and larger than any earlier run's, so the suite can run repeatedly
/// against one database.
fn ids<const N: usize>() -> [u64; N] {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        >> 4;
    std::array::from_fn(|i| base + i as u64 * 7)
}

fn duel(host: u64, guest: u64, bet: i64, guild: u64) -> NewGame<'static> {
    NewGame {
        kind: "connect4",
        guild_id: guild,
        channel_id: 8,
        host,
        guest: Some(guest),
        bet,
        status: PENDING,
        state: "start",
        ttl_secs: 600,
    }
}

/// A connection of its own whose backend can be watched, so a test knows when the call it makes
/// is queued on a lock (other tests waiting on locks in the same database do not count).
async fn side() -> (Db, i32) {
    let db = db_with(1).await.expect("a test database");
    let (pid,): (i32,) = sqlx::query_as("SELECT pg_backend_pid()")
        .fetch_one(db.pool())
        .await
        .unwrap();
    (db, pid)
}

/// Waits until backend `pid` is waiting on a lock; panics after ten seconds.
async fn queued(db: &Db, pid: i32, what: &str) {
    for _ in 0..200 {
        let (waiting,): (bool,) = sqlx::query_as(
            "SELECT COALESCE(bool_or(wait_event_type = 'Lock'), FALSE) FROM pg_stat_activity WHERE pid = $1",
        )
        .bind(pid)
        .fetch_one(db.pool())
        .await
        .unwrap();
        if waiting {
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(50)).await;
    }
    panic!("{what} never queued on a lock");
}

async fn credits(db: &Db, user: u64) -> i64 {
    let row: Option<(i64,)> = sqlx::query_as("SELECT credits FROM users WHERE id = $1")
        .bind(user as i64)
        .fetch_optional(db.pool())
        .await
        .unwrap();
    row.map_or(0, |(c,)| c)
}

/// What the ledger says the player holds, which must be their balance.
async fn ledger(db: &Db, user: u64) -> i64 {
    let (sum,): (i64,) = sqlx::query_as("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM credit_ledger WHERE user_id = $1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    sum
}

/// Locks the row `sql` selects (with `id` bound) in a transaction of its own, left open.
async fn hold(db: &Db, sql: &'static str, id: i64) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = db.pool().begin().await.unwrap();
    sqlx::query(sql).bind(id).execute(&mut *tx).await.unwrap();
    tx
}

/// Runs `call` on its own connection while a blocker holds the row of `held` (`id` bound), then a
/// delete of `deleted` on another, waits until both queue, lets the blocker go and returns both
/// results. Before the rule this was a deadlock for every caller below.
async fn race_a_delete<T, F, Fut>(
    db: &Db,
    held: &'static str,
    id: i64,
    deleted: u64,
    call: F,
) -> (anyhow::Result<T>, anyhow::Result<pinchora_db::Deleted>)
where
    T: Send + 'static,
    F: FnOnce(Db) -> Fut,
    Fut: Future<Output = anyhow::Result<T>> + Send + 'static,
{
    let ((caller, caller_pid), (deleter, deleter_pid)) = (side().await, side().await);
    let blocker = hold(db, held, id).await;
    let called = tokio::spawn(call(caller));
    queued(db, caller_pid, "the call").await;
    let delete = tokio::spawn(async move { deleter.delete_user_data(deleted, Utc::now()).await });
    queued(db, deleter_pid, "the delete").await;
    blocker.commit().await.unwrap();
    (called.await.unwrap(), delete.await.unwrap())
}

#[tokio::test]
async fn accepting_a_duel_while_its_host_deletes_their_data() {
    let Some(db) = db_with(4).await else { return };
    let [host, guest, guild] = ids();
    db.earn_shells(host, 100, "test").await.unwrap();
    db.earn_shells(guest, 100, "test").await.unwrap();
    let session = db.create_game(duel(host, guest, 10, guild)).await.unwrap();

    let (started, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM game_sessions WHERE id = $1 FOR UPDATE",
        session.id,
        host,
        move |db| async move { db.start_duel(session.id, guest, (8, 9), "go", 600).await },
    )
    .await;
    let started = started.expect("the accept fails nowhere");
    deleted.expect("the delete fails nowhere");
    assert!(matches!(started, DuelStart::Started(_)), "{started:?}");
    assert_eq!(
        credits(&db, guest).await,
        100,
        "the guest's bet came back with the delete"
    );
    assert_eq!(ledger(&db, guest).await, 100);
    db.delete_user_data(guest, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn settling_a_duel_while_its_winner_deletes_their_data() {
    let Some(db) = db_with(4).await else { return };
    let [winner, loser, guild] = ids();
    db.earn_shells(winner, 100, "test").await.unwrap();
    db.earn_shells(loser, 100, "test").await.unwrap();
    let session = db.create_game(duel(winner, loser, 10, guild)).await.unwrap();
    assert!(matches!(
        db.start_duel(session.id, loser, (8, 9), "go", 600).await.unwrap(),
        DuelStart::Started(_)
    ));

    let (settled, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM game_sessions WHERE id = $1 FOR UPDATE",
        session.id,
        winner,
        move |db| async move {
            db.settle_game(session.id, None, "won", Payout::Winner(winner), false)
                .await
        },
    )
    .await;
    assert!(settled.expect("the settle fails nowhere").is_some());
    deleted.expect("the delete fails nowhere");
    assert_eq!((credits(&db, loser).await, ledger(&db, loser).await), (90, 90));
    assert_eq!(credits(&db, winner).await, 0, "the winner's pot went with their data");
    db.delete_user_data(loser, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn ending_a_giveaway_while_its_host_deletes_their_data() {
    let Some(db) = db_with(4).await else { return };
    let [host, guest, guild] = ids();
    db.earn_shells(host, 100, "test").await.unwrap();
    let GiveawayOpen::Started(giveaway) = db
        .create_giveaway(NewGiveaway {
            guild_id: guild,
            channel_id: 1,
            host,
            prize: "Shells",
            shells: 10,
            winners: 2,
            required_role: None,
            ends_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("the giveaway starts");
    };
    db.toggle_giveaway_entry(giveaway.id, guest, Utc::now()).await.unwrap();

    let id = giveaway.id;
    let (ended, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM giveaways WHERE id = $1 FOR UPDATE",
        id,
        host,
        move |db| async move {
            db.end_giveaway(id, Utc::now(), true, |entrants, n| {
                entrants.iter().copied().take(n).collect()
            })
            .await
        },
    )
    .await;
    let ended = ended.expect("the end fails nowhere").expect("it was running");
    deleted.expect("the delete fails nowhere");
    assert_eq!((ended.winners, ended.refunded), (vec![guest], 10));
    assert_eq!((credits(&db, guest).await, ledger(&db, guest).await), (10, 10));
    assert_eq!(credits(&db, host).await, 0);
    db.delete_user_data(guest, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn chat_xp_while_the_player_deletes_their_data() {
    let Some(db) = db_with(4).await else { return };
    let [user, guild] = ids();
    let grant = XpGrant {
        server_xp: 5,
        global_xp: 5,
        points: 1,
    };
    let week = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
    db.add_xp(guild, user, grant, week).await.unwrap();
    db.claim_daily(user).await.unwrap();

    let ((caller, caller_pid), (deleter, deleter_pid)) = (side().await, side().await);
    let mut blocker = db.pool().begin().await.unwrap();
    sqlx::query("SELECT 1 FROM members WHERE guild_id = $1 AND user_id = $2 FOR UPDATE")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let xp = tokio::spawn(async move { caller.add_xp(guild, user, grant, week).await });
    queued(&db, caller_pid, "the chat XP").await;
    let delete = tokio::spawn(async move { deleter.delete_user_data(user, Utc::now()).await });
    queued(&db, deleter_pid, "the delete").await;
    blocker.commit().await.unwrap();

    let gain = xp.await.unwrap().expect("chat XP fails nowhere");
    delete.await.unwrap().expect("the delete fails nowhere");
    assert_eq!((gain.old_xp, gain.new_xp), (5, 10));
    let (members,): (i64,) = sqlx::query_as("SELECT count(*) FROM members WHERE user_id = $1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(members, 0, "the delete ran after the XP and took it");
}

#[tokio::test]
async fn claiming_a_trip_while_the_owner_deletes_their_data() {
    let Some(db) = db_with(4).await else { return };
    let [owner] = ids();
    let pet = db.adopt(owner, "cat", "Tom").await.unwrap();
    assert!(
        db.start_trip(
            owner,
            &pet,
            "beach",
            [50, 50, 50, 50],
            Utc::now() - Duration::minutes(1)
        )
        .await
        .unwrap()
    );

    let pet_id = pet.id;
    let (claimed, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM pets WHERE id = $1 FOR UPDATE",
        pet_id,
        owner,
        move |db| async move {
            db.claim_trip(owner, pet_id, Utc::now(), |p, _| TripPayout {
                shells: 5,
                item: None,
                level: p.level,
                xp: p.xp,
            })
            .await
        },
    )
    .await;
    let claimed = claimed.expect("the claim fails nowhere");
    deleted.expect("the delete fails nowhere");
    assert!(matches!(claimed, TripClaim::Claimed { balance: 5, .. }), "{claimed:?}");
    assert_eq!(credits(&db, owner).await, 0, "the souvenir went with their data");
}

#[tokio::test]
async fn waking_a_pet_while_the_owner_deletes_their_data() {
    let Some(db) = db_with(4).await else { return };
    let [owner] = ids();
    db.earn_shells(owner, 100, "test").await.unwrap();
    let pet = db.adopt(owner, "cat", "Tom").await.unwrap();
    let day = (Utc::now().timestamp() / 86_400) as i32;

    let pet_id = pet.id;
    let (woke, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM pets WHERE id = $1 FOR UPDATE",
        pet_id,
        owner,
        move |db| async move {
            db.wake_pet(owner, pet_id, pet.needs_at, [90, 90, 90, 90], Utc::now(), day, 10)
                .await
        },
    )
    .await;
    let woke = woke.expect("the wake-up fails nowhere");
    deleted.expect("the delete fails nowhere");
    assert!(matches!(woke, Wake::Woke { free: true, .. }), "{woke:?}");
    assert!(
        !db.free_wake_ready(owner, day).await.unwrap(),
        "the day's free wake-up stays used after the delete"
    );
    // Back with a new row: the trigger gives the used wake-up back with it.
    assert_eq!(db.user(owner).await.unwrap().credits, 0);
    let (used,): (Option<i32>,) = sqlx::query_as("SELECT pet_wake_day FROM users WHERE id = $1")
        .bind(owner as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(used, Some(day));
    assert!(!db.free_wake_ready(owner, day).await.unwrap());
    db.delete_user_data(owner, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn buying_from_a_stall_while_the_seller_deletes_their_data() {
    let Some(db) = db_with(4).await else { return };
    let [seller, buyer] = ids();
    db.earn_shells(seller, 5, "test").await.unwrap();
    db.earn_shells(buyer, 100, "test").await.unwrap();
    db.grant_item(seller, "chair", 1).await.unwrap();
    assert!(db.list_item(seller, "chair", 40).await.unwrap());
    let listing = db.listings(seller).await.unwrap()[0].id;

    let (bought, deleted) = race_a_delete(
        &db,
        "SELECT 1 FROM stall_listings WHERE id = $1 FOR UPDATE",
        listing,
        seller,
        move |db| async move { db.buy_listing(buyer, listing).await },
    )
    .await;
    let bought = bought.expect("the purchase fails nowhere");
    deleted.expect("the delete fails nowhere");
    assert!(matches!(bought, StallBuy::Bought { balance: 60, .. }), "{bought:?}");
    assert_eq!((credits(&db, buyer).await, ledger(&db, buyer).await), (60, 60));
    assert_eq!(credits(&db, seller).await, 0);
    db.delete_user_data(buyer, Utc::now()).await.unwrap();
}

#[tokio::test]
async fn a_like_landing_while_the_delete_starts_cannot_cross_a_pair_call() {
    let Some(db) = db_with(6).await else { return };
    // `ids` count upwards: the host's lock comes before the deleter's in id order.
    let [host, deleter_id] = ids();
    let today = Utc::now().date_naive();
    sqlx::query("INSERT INTO houses (user_id) VALUES ($1)")
        .bind(host as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.earn_shells(host, 1, "test").await.unwrap();
    db.earn_shells(deleter_id, 1, "test").await.unwrap();

    // The like is given in a transaction that holds the deleter's lock, as a like does: the delete's
    // first read cannot see it, and its lock waits.
    let mut like = db.pool().begin().await.unwrap();
    sqlx::query("SELECT lock_user($1)")
        .bind(deleter_id as i64)
        .execute(&mut *like)
        .await
        .unwrap();
    sqlx::query("INSERT INTO house_likes (host_id, liker_id, day) VALUES ($1, $2, $3)")
        .bind(host as i64)
        .bind(deleter_id as i64)
        .bind(today)
        .execute(&mut *like)
        .await
        .unwrap();
    let ((deleter, deleter_pid), (friend, friend_pid)) = (side().await, side().await);
    let delete = tokio::spawn(async move { deleter.delete_user_data(deleter_id, Utc::now()).await });
    queued(&db, deleter_pid, "the delete").await;
    // A call about both players: takes the host's lock and waits for the deleter's.
    let befriend = tokio::spawn(async move { friend.befriend(host, deleter_id).await });
    queued(&db, friend_pid, "the pair call").await;
    let before = pinchora_db::conflict_retries();
    like.commit().await.unwrap();

    befriend.await.unwrap().expect("the pair call fails nowhere");
    delete.await.unwrap().expect("the delete fails nowhere");
    assert!(
        pinchora_db::conflict_retries() > before,
        "the delete found the host too late to wait for, and started over"
    );
    for user in [host, deleter_id] {
        db.delete_user_data(user, Utc::now()).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn friends_who_delete_their_data_at_once_both_get_through() {
    let Some(db) = db_with(8).await else { return };
    let day = Utc::now().date_naive();
    for _ in 0..4 {
        let [a, b] = ids();
        db.earn_shells(a, 10, "test").await.unwrap();
        db.earn_shells(b, 10, "test").await.unwrap();
        db.befriend(a, b).await.unwrap();
        // Rows both ways, so each delete removes what the other one removes from the other end.
        db.record_visit(a, b, day, 1, 3).await.unwrap();
        db.record_visit(b, a, day, 1, 3).await.unwrap();
        db.record_interaction("hug", a, b).await.unwrap();
        db.record_interaction("hug", b, a).await.unwrap();
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let deletes: Vec<_> = [a, b]
            .into_iter()
            .map(|user| {
                let (db, barrier) = (db.clone(), barrier.clone());
                tokio::spawn(async move {
                    barrier.wait().await;
                    db.delete_user_data(user, Utc::now()).await
                })
            })
            .collect();
        for delete in deletes {
            delete.await.unwrap().expect("a delete fails nowhere");
        }
        assert!(!db.are_friends(a, b).await.unwrap());
    }
}

#[tokio::test]
async fn rerolling_a_giveaway_while_an_entrant_deletes_their_data() {
    let Some(db) = db_with(4).await else { return };
    let [host, first, leaving, last, guild] = ids();
    let GiveawayOpen::Started(giveaway) = db
        .create_giveaway(NewGiveaway {
            guild_id: guild,
            channel_id: 1,
            host,
            prize: "A role",
            shells: 0,
            winners: 1,
            required_role: None,
            ends_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("the giveaway starts");
    };
    let id = giveaway.id;
    for user in [first, leaving, last] {
        db.toggle_giveaway_entry(id, user, Utc::now()).await.unwrap();
    }
    db.end_giveaway(id, Utc::now(), true, |entrants, _| {
        entrants.iter().copied().filter(|e| *e == first).collect()
    })
    .await
    .unwrap()
    .expect("it was running");

    // One entrant's delete is under way: their entry is still there for anybody who reads it.
    let mut delete = db.pool().begin().await.unwrap();
    Db::delete_user_data_in(&mut delete, leaving, Utc::now()).await.unwrap();
    let (roller, roller_pid) = side().await;
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = asked.clone();
    let reroll = tokio::spawn(async move {
        roller
            .reroll_giveaway(guild, id, 1, move |entrants, _| {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                // Always the one who is leaving, while there is one.
                if entrants.contains(&leaving) {
                    vec![leaving]
                } else {
                    entrants.to_vec()
                }
            })
            .await
    });
    queued(&db, roller_pid, "the reroll").await;
    delete.commit().await.unwrap();

    let Reroll::Done(row, picked) = reroll.await.unwrap().expect("the reroll fails nowhere") else {
        panic!("somebody is left to win");
    };
    assert_eq!(picked, vec![last], "the pick started over without the deleted entrant");
    assert!(asked.load(std::sync::atomic::Ordering::SeqCst) >= 2);
    assert!(!row.winner_ids.contains(&(leaving as i64)));
    let (stored,): (Vec<i64>,) = sqlx::query_as("SELECT winner_ids FROM giveaways WHERE id = $1")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(
        stored,
        vec![first as i64, last as i64],
        "nothing of the deleted player is stored"
    );
    for user in [host, first, last] {
        db.delete_user_data(user, Utc::now()).await.unwrap();
    }
}

#[tokio::test]
async fn ending_a_giveaway_without_shells_while_an_entrant_deletes_their_data() {
    let Some(db) = db_with(4).await else { return };
    let [host, first, leaving, guild] = ids();
    let GiveawayOpen::Started(giveaway) = db
        .create_giveaway(NewGiveaway {
            guild_id: guild,
            channel_id: 1,
            host,
            prize: "A role",
            shells: 0,
            winners: 1,
            required_role: None,
            ends_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("the giveaway starts");
    };
    let id = giveaway.id;
    for user in [first, leaving] {
        db.toggle_giveaway_entry(id, user, Utc::now()).await.unwrap();
    }

    let mut delete = db.pool().begin().await.unwrap();
    Db::delete_user_data_in(&mut delete, leaving, Utc::now()).await.unwrap();
    let (ender, ender_pid) = side().await;
    let end = tokio::spawn(async move {
        ender
            .end_giveaway(id, Utc::now(), true, move |entrants, _| {
                // Always the one who is leaving, while there is one.
                if entrants.contains(&leaving) {
                    vec![leaving]
                } else {
                    entrants.to_vec()
                }
            })
            .await
    });
    queued(&db, ender_pid, "the end").await;
    delete.commit().await.unwrap();

    let ended = end
        .await
        .unwrap()
        .expect("the end fails nowhere")
        .expect("it was running");
    assert_eq!(
        ended.winners,
        vec![first],
        "the pick started over without the deleted entrant"
    );
    let (stored,): (Vec<i64>,) = sqlx::query_as("SELECT winner_ids FROM giveaways WHERE id = $1")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(stored, vec![first as i64], "nothing of the deleted player is stored");
    for user in [host, first] {
        db.delete_user_data(user, Utc::now()).await.unwrap();
    }
}

#[tokio::test]
async fn opening_a_session_waits_for_its_host_to_finish_deleting() {
    let Some(db) = db_with(4).await else { return };
    let [host, guest, guild] = ids();
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
    let Some(db) = db_with(4).await else { return };
    let _fixtures = SWEEP_FIXTURES.write().await;
    let [guild, host] = ids();
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

// ---------------------------------------------------------------- the games

/// The daily limit of the tests: nothing here is about it.
const LOSS: i64 = 1_000_000;

fn lobby_of(
    kind: &'static str,
    guild: u64,
    channel: u64,
    host: u64,
    bet: i64,
    status: &'static str,
) -> NewGame<'static> {
    NewGame {
        kind,
        guild_id: guild,
        channel_id: channel,
        host,
        guest: None,
        bet,
        status,
        state: "7",
        ttl_secs: 600,
    }
}

#[tokio::test]
async fn settling_a_derby_while_a_racer_deletes_their_data() {
    let Some(db) = db_with(4).await else { return };
    let [host, racer, leaver, guild, channel] = ids();
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
    let Some(db) = db_with(4).await else { return };
    let [grabber, guild] = ids();
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
    let Some(db) = db_with(4).await else { return };
    let [host, other, guild, channel] = ids();
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
    let Some(db) = db_with(4).await else { return };
    let [leaving, second, third, voter, guild] = ids();
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
    let Some(db) = db_with(4).await else { return };
    let [owner, voter, guild] = ids();
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

// ---------------------------------------------------------------- mixed load

#[derive(Default, Clone)]
struct Tally {
    ok: u64,
    failed: u64,
    samples: Vec<String>,
}

const OPS: [&str; 24] = [
    "daily", "weekly", "rep", "transfer", "like", "visit", "xp", "duel", "giveaway", "delete", "wallet", "earn",
    "stall", "trip", "friend", "house", "sweep", "derby", "heist", "surge", "shells", "collect", "party", "contest",
];

/// What players do at once in a busy server, deletes of their data among it, from a few workers for
/// a while (`PINCHORA_TEST_MIX_SECS`, `PINCHORA_TEST_MIX_WORKERS`). No call may fail (a deadlock
/// would reach the player as "something went wrong"), and every balance must be what its ledger
/// says, with no shells left held in a duel, a giveaway or a party game.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_mixed_load_with_deletes_fails_nobody_and_keeps_the_books() {
    let env = |name: &str, default: u64| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
    let (secs, workers) = (env("PINCHORA_TEST_MIX_SECS", 15), env("PINCHORA_TEST_MIX_WORKERS", 8));
    let Some(db) = db_with(workers as u32 * 2).await else {
        return;
    };
    // Players who are deleted all the time, and a few house owners nobody deletes (placing a piece in
    // a house whose owner deleted it in between is a foreign key error, a different matter).
    let all = ids::<14>();
    let users: Arc<Vec<u64>> = Arc::new(all[..10].to_vec());
    let owners: Arc<Vec<u64>> = Arc::new(all[10..].to_vec());
    let [guild] = ids();
    for user in users.iter() {
        db.earn_shells(*user, 1_000_000, "test").await.unwrap();
    }
    let retries_before = pinchora_db::conflict_retries();
    let deadline = Instant::now() + StdDuration::from_secs(secs);
    let mut handles = Vec::new();
    for worker in 0..workers {
        let db = db.clone();
        let (users, owners) = (users.clone(), owners.clone());
        handles.push(tokio::spawn(async move {
            let mut tally = vec![Tally::default(); OPS.len()];
            let mut seed = 0x9E37_79B9_7F4A_7C15u64 ^ (worker + 1).wrapping_mul(0x2545_F491_4F6C_DD1D);
            let mut roll = move || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            let mut calls = 0u64;
            while Instant::now() < deadline {
                calls += 1;
                let i = (roll() % users.len() as u64) as usize;
                let a = users[i];
                let b = users[(i + 1 + (roll() % (users.len() as u64 - 1)) as usize) % users.len()];
                // One worker deletes every tenth call, so deletes always run, whatever the dice say.
                let op = match roll() % 200 {
                    _ if worker == 0 && calls.is_multiple_of(10) => 9,
                    0..=7 => 0,
                    8..=13 => 1,
                    14..=19 => 2,
                    20..=31 => 3,
                    32..=39 => 4,
                    40..=47 => 5,
                    48..=59 => 6,
                    60..=67 => 7,
                    68..=73 => 8,
                    74..=75 => 9,
                    76..=81 => 10,
                    82..=87 => 11,
                    88..=93 => 12,
                    94..=99 => 13,
                    100..=105 => 14,
                    106..=111 => 15,
                    112..=119 => 16,
                    120..=134 => 17,
                    135..=144 => 18,
                    145..=154 => 19,
                    155..=164 => 20,
                    165..=179 => 21,
                    180..=189 => 22,
                    _ => 23,
                };
                let day = Utc::now().date_naive();
                let result: anyhow::Result<()> = async {
                    match op {
                        0 => db.claim_daily(a).await.map(drop),
                        1 => db.claim_period(a, Period::Weekly, Utc::now()).await.map(drop),
                        2 => db.give_rep(a, b).await.map(drop),
                        3 => db.transfer_credits(a, b, 1).await.map(drop),
                        4 => {
                            sqlx::query("INSERT INTO houses (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
                                .bind(b as i64)
                                .execute(db.pool())
                                .await?;
                            db.like_house(b, a, day).await.map(drop)
                        }
                        5 => db.record_visit(b, a, day, 25, 3).await.map(drop),
                        6 => {
                            let grant = XpGrant {
                                server_xp: 3,
                                global_xp: 3,
                                points: 1,
                            };
                            db.add_xp(guild, a, grant, NaiveDate::from_ymd_opt(2026, 11, 2).unwrap())
                                .await
                                .map(drop)
                        }
                        7 => {
                            let session = db.create_game(duel(a, b, 2, guild)).await?;
                            db.start_duel(session.id, b, (8, 9), "go", 600).await?;
                            db.settle_game(session.id, None, "won", Payout::Winner(a), false)
                                .await
                                .map(drop)
                        }
                        8 => {
                            let new = NewGiveaway {
                                guild_id: guild + roll() % 50,
                                channel_id: 1,
                                host: a,
                                prize: "Shells",
                                shells: 2,
                                winners: 2,
                                required_role: None,
                                ends_at: Utc::now() + Duration::hours(1),
                            };
                            let guild_id = new.guild_id;
                            if let GiveawayOpen::Started(g) = db.create_giveaway(new).await? {
                                db.toggle_giveaway_entry(g.id, b, Utc::now()).await?;
                                db.toggle_giveaway_entry(g.id, a, Utc::now()).await?;
                                db.end_giveaway(g.id, Utc::now(), true, |e, n| e.iter().copied().take(n).collect())
                                    .await?;
                                db.reroll_giveaway(guild_id, g.id, 1, |e, n| e.iter().copied().take(n).collect())
                                    .await?;
                                db.mark_giveaway_announced(g.id).await?;
                            }
                            Ok(())
                        }
                        9 => {
                            db.delete_user_data(a, Utc::now()).await?;
                            db.earn_shells(a, 1_000_000, "test").await.map(drop)
                        }
                        10 => db.user(a).await.map(drop),
                        11 => db.earn_shells(a, 3, "test").await.map(drop),
                        12 => {
                            db.grant_item(a, "chair", 1).await?;
                            db.list_item(a, "chair", 2).await?;
                            if let Some(listing) = db.listings(a).await?.first() {
                                db.buy_listing(b, listing.id).await?;
                            }
                            Ok(())
                        }
                        14 => {
                            db.befriend(a, b).await?;
                            db.record_interaction("hug", a, b).await?;
                            db.block_signer(a, b, "Somebody").await.map(drop)
                        }
                        15 => {
                            let room = NewRoom {
                                room_type: "living".into(),
                                wallpaper: "plain".into(),
                                floor: "oak".into(),
                            };
                            let owner = owners[(roll() % owners.len() as u64) as usize];
                            db.house(owner, &room).await?;
                            db.grant_item(owner, "chair", 2).await?;
                            db.place_item(
                                owner,
                                0,
                                "chair",
                                (roll() % 8) as i16,
                                (roll() % 8) as i16,
                                false,
                                |_| Ok(()),
                            )
                            .await?;
                            db.clear_room(owner, 0).await.map(drop)
                        }
                        16 => {
                            let _fixtures = SWEEP_FIXTURES.read().await;
                            db.forget_old_giveaways(Utc::now() + Duration::days(1)).await?;
                            db.forget_old_games().await?;
                            db.forget_old_parties(Utc::now() + Duration::days(1), 0).await?;
                            db.forget_old_collection(Utc::now() + Duration::days(1), 0).await?;
                            db.forget_weeks_before(NaiveDate::from_ymd_opt(2001, 1, 2).unwrap())
                                .await
                                .map(drop)
                        }
                        17 => {
                            // A staked derby: two racers, one leaves and comes back, the host runs it.
                            let channel = roll() >> 2;
                            if let Some(derby) = db
                                .open_party_game(lobby_of("derby", guild, channel, a, 3, PENDING))
                                .await?
                            {
                                db.derby_join(derby.id, a, "A", 0, 30, LOSS).await?;
                                db.derby_join(derby.id, b, "B", 1, 30, LOSS).await?;
                                if roll() % 2 == 0 {
                                    db.derby_leave(derby.id, b).await?;
                                    db.derby_join(derby.id, b, "B", 0, 30, LOSS).await?;
                                }
                                db.derby_settle(derby.id, Some(a), 2, (roll() % 2) as i16).await?;
                            }
                            Ok(())
                        }
                        18 => {
                            let channel = roll() >> 2;
                            if let Some(heist) = db
                                .open_party_game(lobby_of("heist", guild, channel, a, 0, PENDING))
                                .await?
                            {
                                db.heist_join(heist.id, a, "A", 0, 5).await?;
                                db.heist_join(heist.id, b, "B", 1, 5).await?;
                                db.heist_settle(heist.id, Some(a), 2, 1_000_000, |_, _| 2).await?;
                            }
                            Ok(())
                        }
                        19 => {
                            let storm_guild = guild + 100 + (roll() >> 8) % 1_000_000_007;
                            if let Some(storm) = db
                                .open_party_game(lobby_of("surge", storm_guild, 5, a, 0, ACTIVE))
                                .await?
                            {
                                db.surge_help(storm.id, a, "A", 3).await?;
                                db.surge_help(storm.id, b, "B", 3).await?;
                                db.surge_settle(storm.id, true, false, 1_000_000, |_, helps| helps)
                                    .await?;
                            }
                            Ok(())
                        }
                        20 => {
                            let limits = ShellLimits {
                                plays: 1_000_000,
                                loss: 1_000_000,
                                win: 1_000_000,
                            };
                            let table = NewGame {
                                state: "1",
                                ttl_secs: 45,
                                ..lobby_of("shellgame", guild, roll() >> 2, a, 2, ACTIVE)
                            };
                            if let ShellStart::Started(game) = db.shell_start(table, limits).await? {
                                if roll() % 2 == 0 {
                                    let picked = db.shell_pick(game.id, a, (roll() % 3) as u8, |stake| stake * 2).await?;
                                    anyhow::ensure!(!matches!(picked, ShellPick::NotYours), "the host's own pick");
                                } else {
                                    sqlx::query("UPDATE game_sessions SET expires_at = now() - interval '1 second' WHERE id = $1")
                                        .bind(game.id)
                                        .execute(db.pool())
                                        .await?;
                                    db.shell_refund(game.id).await?;
                                }
                            }
                            Ok(())
                        }
                        21 => {
                            let (award, daily) = (
                                Award {
                                    entry: "x_one",
                                    new_shells: 3,
                                    dup_shells: 2,
                                    dup_fragments: 1,
                                    bonus_fragments: 0,
                                },
                                Daily {
                                    game: "critterdrop",
                                    day: 20_000,
                                    cap: 1_000_000,
                                },
                            );
                            let cards: Vec<String> = ["x_one", "x_two", "x_three"].map(String::from).into();
                            let channel = (roll() >> 2) + 1;
                            let critters = db
                                .create_drop(NewDrop {
                                    kind: DROP_CRITTERS,
                                    guild_id: guild,
                                    channel_id: channel,
                                    host: None,
                                    cards: &cards,
                                    expires_at: Utc::now() + Duration::hours(1),
                                })
                                .await?;
                            db.grab_card(critters.id, 0, a, Utc::now(), award, daily).await?;
                            db.grab_card(critters.id, 0, b, Utc::now(), award, daily).await?;
                            let bottle = db
                                .create_drop(NewDrop {
                                    kind: DROP_BOTTLE,
                                    guild_id: guild,
                                    channel_id: channel + 1,
                                    host: None,
                                    cards: &["bottle_01".to_owned()],
                                    expires_at: Utc::now() + Duration::hours(1),
                                })
                                .await?;
                            db.open_bottle(bottle.id, a, Utc::now(), BottlePrize::Fragments(5), daily)
                                .await?;
                            db.exchange_fragments(a, 5, "chair").await?;
                            db.start_pot(b, 1, Utc::now() - Duration::hours(2)).await?;
                            db.haul_pot(b, Utc::now(), daily, |_, _, _| vec![award]).await?;
                            db.upgrade_pot(b, 1, 1).await.map(drop)
                        }
                        22 => {
                            let deadline = Utc::now() + Duration::hours(1);
                            let game = db
                                .create_party(NewParty {
                                    kind: "hotpearl",
                                    guild_id: guild,
                                    channel_id: roll() >> 2,
                                    host: a,
                                    host_name: "A",
                                    rounds: 1,
                                    status: "lobby",
                                    phase: "lobby",
                                    state: "{}",
                                    deadline,
                                })
                                .await?;
                            db.join_party(game.id, b, "B", 8, Utc::now()).await?;
                            if roll() % 2 == 0 {
                                db.leave_party(game.id, b, Utc::now()).await?;
                                db.join_party(game.id, b, "B", 8, Utc::now()).await?;
                            }
                            if let Some(game) = db.party(game.id).await? {
                                db.commit_party(
                                    game.id,
                                    game.seq,
                                    Commit {
                                        phase: "done",
                                        round: 1,
                                        state: "{}",
                                        deadline,
                                        status: "done",
                                        players: &[
                                            PlayerChange {
                                                user: a,
                                                score: 1,
                                                ..Default::default()
                                            },
                                            PlayerChange {
                                                user: b,
                                                score: 1,
                                                ..Default::default()
                                            },
                                        ],
                                        entries: &[],
                                        pay: Some(Pay {
                                            game: "hotpearl",
                                            day: 20_000,
                                            cap: 1_000_000,
                                            reason: "party_win",
                                            shells: &[(a, 2), (b, 1)],
                                        }),
                                    },
                                )
                                .await?;
                            }
                            Ok(())
                        }
                        23 => {
                            let contest_guild = guild + 100 + (roll() >> 8) % 1_000_000_007;
                            for user in [a, b] {
                                sqlx::query(
                                    "INSERT INTO contest_entries (guild_id, week, user_id, owner_name, room_idx, tier, room_type, wallpaper, floor)
                                     VALUES ($1, 5, $2, 'Name', 0, 1, 'living', 'w', 'f') ON CONFLICT DO NOTHING",
                                )
                                .bind(contest_guild as i64)
                                .bind(user as i64)
                                .execute(db.pool())
                                .await?;
                            }
                            db.contest_vote(contest_guild, 5, a, b).await?;
                            db.contest_vote(contest_guild, 5, b, a).await?;
                            db.close_contest_weeks(contest_guild, 6, &[5, 3], "house_of_the_week")
                                .await
                                .map(drop)
                        }
                        _ => {
                            let pet = match db.active_pet(a).await? {
                                Some(pet) => pet,
                                None => db.adopt(a, "cat", "Tom").await?,
                            };
                            let back = Utc::now() - Duration::seconds(1);
                            db.start_trip(a, &pet, "beach", [50, 50, 50, 50], back).await?;
                            db.claim_trip(a, pet.id, Utc::now(), |p, _| TripPayout {
                                shells: 1,
                                item: None,
                                level: p.level,
                                xp: p.xp,
                            })
                            .await
                            .map(drop)
                        }
                    }
                }
                .await;
                let t = &mut tally[op];
                match result {
                    Ok(()) => t.ok += 1,
                    Err(error) => {
                        t.failed += 1;
                        if t.samples.len() < 3 {
                            t.samples.push(format!("{error:#}"));
                        }
                    }
                }
            }
            tally
        }));
    }
    let mut total = vec![Tally::default(); OPS.len()];
    for handle in handles {
        for (sum, t) in total.iter_mut().zip(handle.await.unwrap()) {
            sum.ok += t.ok;
            sum.failed += t.failed;
            sum.samples
                .extend(t.samples.into_iter().take(3 - sum.samples.len().min(3)));
        }
    }
    let retried = pinchora_db::conflict_retries() - retries_before;
    let mut report = format!("MIX secs={secs} workers={workers} retried={retried}\n");
    for (name, t) in OPS.iter().zip(&total) {
        report.push_str(&format!(
            "  {name:<9} ok={:<6} failed={:<4} {:?}\n",
            t.ok, t.failed, t.samples
        ));
    }
    println!("{report}");

    let mut books = Vec::new();
    for user in users.iter() {
        let (balance, booked) = (credits(&db, *user).await, ledger(&db, *user).await);
        if balance != booked {
            books.push(format!("{user}: balance {balance}, ledger {booked}"));
        }
    }
    let ids: Vec<i64> = users.iter().map(|u| *u as i64).collect();
    let (held,): (i64,) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM game_sessions WHERE status = 'active' AND host_id = ANY($1))
              + (SELECT count(*) FROM giveaways WHERE status = 'running' AND host_id = ANY($1))
              + (SELECT COALESCE(SUM(m.amount), 0) FROM party_members m JOIN game_sessions s ON s.id = m.session_id
                 WHERE s.status <> 'done' AND m.user_id = ANY($1))::BIGINT",
    )
    .bind(&ids)
    .fetch_one(db.pool())
    .await
    .unwrap();
    for user in users.iter().chain(owners.iter()) {
        db.delete_user_data(*user, Utc::now()).await.unwrap();
    }
    // The drops stay open for an hour, and the simulator's collection tick closes the oldest due ones first.
    sqlx::query("DELETE FROM collection_drops WHERE guild_id = $1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let failed: u64 = total.iter().map(|t| t.failed).sum();
    assert_eq!(failed, 0, "calls failed under the mixed load:\n{report}");
    assert!(
        books.is_empty(),
        "balances that are not what the ledger says: {books:?}"
    );
    assert_eq!(held, 0, "shells left held in a duel, a giveaway or a game");
    assert!(total.iter().all(|t| t.ok > 0), "every kind of call ran:\n{report}");
}
