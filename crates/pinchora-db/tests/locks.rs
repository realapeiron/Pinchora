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
    Db, DuelStart, GiveawayOpen, NewGame, NewGiveaway, PENDING, Payout, StallBuy, TripClaim, TripPayout, Wake, XpGrant,
};

async fn db_with(pool: u32) -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect(&url, pool).await.expect("connect");
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
            pet.id,
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

// ---------------------------------------------------------------- mixed load

#[derive(Default, Clone)]
struct Tally {
    ok: u64,
    failed: u64,
    samples: Vec<String>,
}

const OPS: [&str; 14] = [
    "daily", "weekly", "rep", "transfer", "like", "visit", "xp", "duel", "giveaway", "delete", "wallet", "earn",
    "stall", "trip",
];

/// What players do at once in a busy server, deletes of their data among it, from a few workers for
/// a while (`PINCHORA_TEST_MIX_SECS`, `PINCHORA_TEST_MIX_WORKERS`). No call may fail (a deadlock
/// would reach the player as "something went wrong"), and every balance must be what its ledger
/// says, with no shells left held in a duel or a giveaway.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_mixed_load_with_deletes_fails_nobody_and_keeps_the_books() {
    let env = |name: &str, default: u64| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
    let (secs, workers) = (env("PINCHORA_TEST_MIX_SECS", 15), env("PINCHORA_TEST_MIX_WORKERS", 8));
    let Some(db) = db_with(workers as u32 * 2).await else {
        return;
    };
    let users: Arc<Vec<u64>> = Arc::new(ids::<10>().to_vec());
    let [guild] = ids();
    for user in users.iter() {
        db.earn_shells(*user, 1_000_000, "test").await.unwrap();
    }
    let retries_before = pinchora_db::conflict_retries();
    let deadline = Instant::now() + StdDuration::from_secs(secs);
    let mut handles = Vec::new();
    for worker in 0..workers {
        let db = db.clone();
        let users = users.clone();
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
                let op = match roll() % 100 {
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
                    _ => 13,
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
                            if let GiveawayOpen::Started(g) = db.create_giveaway(new).await? {
                                db.toggle_giveaway_entry(g.id, b, Utc::now()).await?;
                                db.end_giveaway(g.id, Utc::now(), true, |e, n| e.iter().copied().take(n).collect())
                                    .await?;
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
                        _ => {
                            let pet = match db.active_pet(a).await? {
                                Some(pet) => pet,
                                None => db.adopt(a, "cat", "Tom").await?,
                            };
                            let back = Utc::now() - Duration::seconds(1);
                            db.start_trip(a, pet.id, "beach", [50, 50, 50, 50], back).await?;
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
              + (SELECT count(*) FROM giveaways WHERE status = 'running' AND host_id = ANY($1))",
    )
    .bind(&ids)
    .fetch_one(db.pool())
    .await
    .unwrap();
    for user in users.iter() {
        db.delete_user_data(*user, Utc::now()).await.unwrap();
    }
    let failed: u64 = total.iter().map(|t| t.failed).sum();
    assert_eq!(failed, 0, "calls failed under the mixed load:\n{report}");
    assert!(
        books.is_empty(),
        "balances that are not what the ledger says: {books:?}"
    );
    assert_eq!(held, 0, "shells left held in a duel or a giveaway");
    assert!(total.iter().all(|t| t.ok > 0), "every kind of call ran:\n{report}");
}
