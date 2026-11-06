//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! Privacy choices, "delete my data", the weekly and monthly rewards, reminders and the monthly
//! numbers behind Wrapped.

use chrono::{Duration, NaiveDate, Utc};
use pinchora_core::economy::Period;
use pinchora_db::{
    Counter, Db, DuelStart, Hide, ID_LISTS, NewGame, OTHER_TABLES, PENDING, PeriodOutcome, USER_COLUMNS, XpGrant,
};

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect(&url, 8).await.expect("connect");
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
    std::array::from_fn(|i| base + i as u64)
}

/// Postgres keeps microseconds, so a time that is compared with one it stored is cut to them.
fn micros(t: chrono::DateTime<Utc>) -> chrono::DateTime<Utc> {
    chrono::DateTime::from_timestamp_micros(t.timestamp_micros()).expect("a time")
}

async fn run(db: &Db, sql: &'static str, user: u64, other: u64) {
    sqlx::query(sql)
        .bind(user as i64)
        .bind(other as i64)
        .execute(db.pool())
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
}

/// How many rows of `table` name `user` in `column`.
async fn count(db: &Db, table: &str, column: &str, user: u64) -> i64 {
    let sql = format!("SELECT count(*) FROM {table} WHERE {column} = $1");
    let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    n
}

/// Fills every table that can name `user` (with `other` on the far side of the ones that name two
/// players), deletes `user`, and checks nothing is left. Returns the columns it filled.
async fn fill_and_delete(db: &Db, user: u64, other: u64, guild: u64) -> Vec<(&'static str, &'static str)> {
    // Through the game's own calls: the account, the ledger, chat XP and the monthly numbers.
    db.earn_shells(user, 500, "test").await.unwrap();
    db.earn_shells(other, 500, "test").await.unwrap();
    let grant = XpGrant {
        server_xp: 20,
        global_xp: 20,
        points: 2,
    };
    db.add_xp(guild, user, grant, NaiveDate::from_ymd_opt(2026, 1, 5).unwrap())
        .await
        .unwrap();
    db.claim_period(user, Period::Weekly, Utc::now()).await.unwrap();
    db.set_optin(user, "daily", true).await.unwrap();
    db.add_text_reminder(user, "water the plants", Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    db.set_privacy(user, Hide::Profile, true).await.unwrap();
    db.log_catch(user, "minnow", 4.0).await.unwrap();
    db.remember_name(user, "Pinch").await.unwrap();
    db.remember_name(other, "Other").await.unwrap();
    db.pair_partners(user, other).await.unwrap();
    db.befriend(user, other).await.unwrap();

    // A wager both players put money on: deleting one player must not cost the other their bet.
    let game = |host: u64, guest: u64, bet: i64| NewGame {
        kind: "connect4",
        guild_id: guild,
        channel_id: 8,
        host,
        guest: Some(guest),
        bet,
        status: PENDING,
        state: "start",
        ttl_secs: 600,
    };
    let session = db.create_game(game(user, other, 100)).await.unwrap();
    let DuelStart::Started(_) = db.start_duel(session.id, other, (8, 9), "go", 600).await.unwrap() else {
        panic!("the guest accepts");
    };
    assert_eq!(db.user(other).await.unwrap().credits, 400, "the bet is held");
    // And a challenge the other player sent them.
    db.create_game(game(other, user, 0)).await.unwrap();
    sqlx::query("INSERT INTO game_answers (session_id, user_id, choice) VALUES ($1, $2, 1), ($1, $3, 2)")
        .bind(session.id)
        .bind(user as i64)
        .bind(other as i64)
        .execute(db.pool())
        .await
        .unwrap();

    // Everything else, with the player on each side of every table that names two players.
    // ($1 is the player, $2 someone else.)
    let inserts: &[&str] = &[
        "INSERT INTO inventory (user_id, item_id, quantity) VALUES ($1, 'chair', 2)",
        "INSERT INTO houses (user_id) VALUES ($1)",
        "INSERT INTO rooms (user_id, idx, room_type, wallpaper, floor) VALUES ($1, 0, 'cottage', 'plain', 'oak')",
        "INSERT INTO placements (user_id, room_idx, item_id, x, y) VALUES ($1, 0, 'chair', 1, 1)",
        "INSERT INTO visits (host_id, guest_id, day) VALUES ($1, $2, current_date), ($2, $1, current_date)",
        "INSERT INTO stall_listings (seller_id, item_id, price) VALUES ($1, 'chair', 50)",
        "INSERT INTO user_backgrounds (user_id, background_id) VALUES ($1, 'beach')",
        "INSERT INTO user_badges (user_id, badge_id) VALUES ($1, 'star')",
        "INSERT INTO user_titles (user_id, title_id) VALUES ($1, 'angler')",
        "INSERT INTO guestbook_notes (host_id, author_id, author_name, day, body)
         VALUES ($1, $2, 'Other', current_date, 'hi'), ($2, $1, 'Pinch', current_date, 'hello')",
        "INSERT INTO guestbook_blocks (host_id, blocked_id, blocked_name)
         VALUES ($1, $2, 'Other'), ($2, $1, 'Pinch')",
        "INSERT INTO house_likes (host_id, liker_id, day) VALUES ($1, $2, current_date), ($2, $1, current_date)",
        "INSERT INTO gifts (from_id, to_id, item_id) VALUES ($1, $2, 'chair'), ($2, $1, 'lamp')",
        "INSERT INTO interactions (kind, from_user, to_user, times) VALUES ('hug', $1, $2, 3), ('hug', $2, $1, 1)",
        "INSERT INTO pets (owner_id, species, name) VALUES ($1, 'crab', 'Pinchy')",
        "INSERT INTO game_stats (user_id, game, played, won) VALUES ($1, 'connect4', 2, 1)",
        "INSERT INTO wordle_games (user_id, mode, day, word) VALUES ($1, 'daily', 1, 'coral')",
        "INSERT INTO contest_entries (guild_id, week, user_id, owner_name, room_idx, tier, room_type, wallpaper, floor)
         VALUES (1, 1, $1, 'Pinch', 0, 1, 'cottage', 'plain', 'oak'), (1, 1, $2, 'Other', 0, 1, 'cottage', 'plain', 'oak')",
        "INSERT INTO contest_pieces (guild_id, week, user_id, n, item_id, x, y, flipped)
         VALUES (1, 1, $1, 0, 'chair', 1, 1, false)",
        "INSERT INTO contest_votes (guild_id, week, entry_id, voter_id) VALUES (1, 1, $2, $1), (1, 1, $1, $2)",
        "INSERT INTO contest_results (guild_id, week, winner_ids, winner_votes, entries)
         VALUES (1, 1, ARRAY[$1, $2], ARRAY[3, 2], 2)
         ON CONFLICT (guild_id, week) DO UPDATE SET winner_ids = EXCLUDED.winner_ids",
        "INSERT INTO activity_monthly (user_id, month, visits) VALUES ($1, '2025-01-01', 4) ON CONFLICT DO NOTHING",
    ];
    for sql in inserts {
        run(db, sql, user, other).await;
    }
    let mut filled = Vec::new();
    for (table, column) in USER_COLUMNS {
        if count(db, table, column, user).await > 0 {
            filled.push((*table, *column));
        }
    }

    let deleted = db.delete_user_data(user).await.unwrap();
    assert!(deleted.rows >= filled.len() as u64, "{deleted:?}");
    let mut tables: Vec<&str> = filled.iter().map(|(t, _)| *t).collect();
    tables.dedup();
    assert!(deleted.tables >= tables.len(), "{deleted:?} for {tables:?}");

    for (table, column) in USER_COLUMNS {
        assert_eq!(
            count(db, table, column, user).await,
            0,
            "{table}.{column} still names the deleted player"
        );
    }
    for (table, column) in ID_LISTS {
        let sql = format!("SELECT count(*) FROM {table} WHERE $1 = ANY({column})");
        let (left,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(left, 0, "{table}.{column} still lists the deleted player");
    }
    let (ids, votes): (Vec<i64>, Vec<i32>) =
        sqlx::query_as("SELECT winner_ids, winner_votes FROM contest_results WHERE guild_id = 1 AND week = 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        (ids, votes),
        (vec![0, other as i64], vec![3, 2]),
        "the podium keeps its places"
    );

    // The other player keeps their own things, and their bet is back.
    assert_eq!(
        db.user(other).await.unwrap().credits,
        500,
        "the held wager went back to the player left"
    );
    assert_eq!(count(db, "social_names", "user_id", other).await, 1);
    assert!(db.partner(other).await.unwrap().is_none(), "the pair is gone");
    assert_eq!(count(db, "contest_entries", "user_id", other).await, 1);
    assert_eq!(
        count(db, "contest_votes", "voter_id", other).await,
        0,
        "the vote on the deleted entry went with it"
    );

    // Deleting again is quiet, and the player starts over as a new one.
    assert_eq!(db.delete_user_data(user).await.unwrap().rows, 0);
    assert_eq!(db.user(user).await.unwrap().credits, 0);
    filled
}

#[tokio::test]
async fn deleting_a_player_leaves_nothing_in_any_table() {
    let Some(db) = db().await else { return };
    let [low, high, guild_a, other_low, other_high, guild_b] = ids();
    // The pair table keeps the smaller id first, so the player is on each side of it in turn.
    let mut filled = fill_and_delete(&db, low, high, guild_a).await;
    filled.extend(fill_and_delete(&db, other_high, other_low, guild_b).await);
    for (table, column) in USER_COLUMNS {
        assert!(
            filled.contains(&(*table, *column)),
            "the test put nothing in {table}.{column}, so it proves nothing about it"
        );
    }
}

#[tokio::test]
async fn every_table_is_a_player_table_or_says_it_is_not() {
    let Some(db) = db().await else { return };
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT table_name::TEXT FROM information_schema.tables
         WHERE table_schema = current_schema() AND table_type = 'BASE TABLE' ORDER BY table_name",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    let listed: Vec<&str> = USER_COLUMNS.iter().map(|(t, _)| *t).collect();
    let unknown: Vec<&str> = tables
        .iter()
        .map(|(t,)| t.as_str())
        .filter(|t| !listed.contains(t) && !OTHER_TABLES.contains(t))
        .collect();
    assert!(
        unknown.is_empty(),
        "tables {unknown:?} are in neither USER_COLUMNS nor OTHER_TABLES (crates/pinchora-db/src/privacy.rs): \
         \"delete my data\" would miss them"
    );
    for (table, column) in USER_COLUMNS.iter().chain(ID_LISTS) {
        let (found,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns
                            WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2)",
        )
        .bind(*table)
        .bind(*column)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert!(found, "{table}.{column} does not exist");
    }

    // A column that looks like it holds a player's id, in a table that is listed, must be listed.
    let words = [
        "user", "owner", "host", "guest", "author", "liker", "voter", "friend", "seller", "blocked", "from_", "to_id",
        "partner", "entry_id",
    ];
    let columns: Vec<(String, String)> = sqlx::query_as(
        "SELECT table_name::TEXT, column_name::TEXT FROM information_schema.columns
         WHERE table_schema = current_schema() AND data_type = 'bigint' ORDER BY 1, 2",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    for (table, column) in columns {
        let looks_like_a_player = words.iter().any(|w| column.contains(w)) && !column.contains("guestbook");
        let listed = USER_COLUMNS.iter().any(|(t, c)| *t == table && *c == column);
        assert!(
            !looks_like_a_player || listed || OTHER_TABLES.contains(&table.as_str()),
            "{table}.{column} looks like a player's id but \"delete my data\" does not clear it"
        );
    }
}

#[tokio::test]
async fn hiding_from_the_boards_and_the_house_shortlist() {
    let Some(db) = db().await else { return };
    // Later than anything an earlier run left in the shared database, so these are on top.
    let [shown, hidden, viewer] = ids();
    for (user, xp) in [(shown, 0i64), (hidden, 1)] {
        db.earn_shells(user, 1, "test").await.unwrap();
        sqlx::query("UPDATE users SET xp = $2, credits = $2 WHERE id = $1")
            .bind(user as i64)
            .bind(user as i64 + xp)
            .execute(db.pool())
            .await
            .unwrap();
    }
    let top = |rows: Vec<pinchora_db::LeaderboardRow>| rows.into_iter().map(|r| r.user_id as u64).collect::<Vec<_>>();
    assert_eq!(top(db.global_xp_leaderboard(2, 0).await.unwrap()), [hidden, shown]);
    assert_eq!(top(db.global_credits_leaderboard(2, 0).await.unwrap()), [hidden, shown]);
    let rank_below = db.global_xp_rank(shown as i64 - 1).await.unwrap();

    let privacy = db.set_privacy(hidden, Hide::Leaderboard, true).await.unwrap();
    assert!(privacy.hide_leaderboard && !privacy.hide_profile && !privacy.hide_house);
    assert_eq!(db.privacy(hidden).await.unwrap(), privacy);
    assert_eq!(top(db.global_xp_leaderboard(1, 0).await.unwrap()), [shown]);
    assert_eq!(top(db.global_credits_leaderboard(1, 0).await.unwrap()), [shown]);
    assert_eq!(
        db.global_xp_rank(shown as i64 - 1).await.unwrap(),
        rank_below - 1,
        "the hidden player no longer stands above anyone"
    );
    // The other choices are untouched, and turning it off brings the player back.
    let privacy = db.set_privacy(hidden, Hide::House, true).await.unwrap();
    assert!(privacy.hide_leaderboard && privacy.hide_house);
    db.set_privacy(hidden, Hide::Leaderboard, false).await.unwrap();
    assert_eq!(top(db.global_xp_leaderboard(1, 0).await.unwrap()), [hidden]);
    assert!(
        !db.privacy(viewer).await.unwrap().hide_house,
        "no row means nothing hidden"
    );

    // A hidden house is not offered to friends; a locked one is not either; an open one is.
    db.befriend(viewer, hidden).await.unwrap();
    db.befriend(viewer, shown).await.unwrap();
    for user in [hidden, shown] {
        sqlx::query("INSERT INTO houses (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(user as i64)
            .execute(db.pool())
            .await
            .unwrap();
    }
    let offered = |rows: Vec<pinchora_db::FriendRow>| rows.into_iter().map(|f| f.friend_id).collect::<Vec<_>>();
    assert_eq!(offered(db.visitable_friends(viewer, 10).await.unwrap()), [shown]);
    db.set_privacy(hidden, Hide::House, false).await.unwrap();
    let mut both = offered(db.visitable_friends(viewer, 10).await.unwrap());
    both.sort_unstable();
    assert_eq!(both, [shown, hidden]);
}

#[tokio::test]
async fn weekly_and_monthly_rewards_have_their_own_cooldowns() {
    let Some(db) = db().await else { return };
    let [user] = ids();
    let now = micros(Utc::now());

    let PeriodOutcome::Claimed {
        amount,
        balance,
        next_at,
    } = db.claim_period(user, Period::Weekly, now).await.unwrap()
    else {
        panic!("the first weekly pays");
    };
    assert_eq!((amount, balance), (Period::Weekly.reward(), Period::Weekly.reward()));
    assert_eq!(next_at, now + Duration::days(7));
    assert!(matches!(
        db.claim_period(user, Period::Weekly, now + Duration::days(6)).await.unwrap(),
        PeriodOutcome::Cooldown { next_at: at } if at == next_at
    ));
    // The monthly reward does not care about the weekly one.
    assert!(matches!(
        db.claim_period(user, Period::Monthly, now + Duration::days(1))
            .await
            .unwrap(),
        PeriodOutcome::Claimed { .. }
    ));
    assert!(matches!(
        db.claim_period(user, Period::Monthly, now + Duration::days(29))
            .await
            .unwrap(),
        PeriodOutcome::Cooldown { .. }
    ));
    let PeriodOutcome::Claimed { balance, .. } = db
        .claim_period(user, Period::Weekly, now + Duration::days(7))
        .await
        .unwrap()
    else {
        panic!("a week later it pays again");
    };
    assert_eq!(balance, 2 * Period::Weekly.reward() + Period::Monthly.reward());
    assert_eq!(
        db.period_claimed_at(user, Period::Weekly).await.unwrap(),
        Some(now + Duration::days(7))
    );

    // Claims at the same moment pay once.
    let [racer] = ids();
    let claims = futures_join(&db, racer, now).await;
    assert_eq!(
        claims
            .iter()
            .filter(|c| matches!(c, PeriodOutcome::Claimed { .. }))
            .count(),
        1
    );
    assert_eq!(db.user(racer).await.unwrap().credits, Period::Weekly.reward());

    // Every reward is in the ledger, and counted as shells earned this month.
    let (ledger,): (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM credit_ledger WHERE user_id = $1")
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(ledger, 2 * Period::Weekly.reward() + Period::Monthly.reward());
}

async fn futures_join(db: &Db, user: u64, now: chrono::DateTime<Utc>) -> Vec<PeriodOutcome> {
    let (a, b, c) = tokio::join!(
        db.claim_period(user, Period::Weekly, now),
        db.claim_period(user, Period::Weekly, now),
        db.claim_period(user, Period::Weekly, now),
    );
    vec![a.unwrap(), b.unwrap(), c.unwrap()]
}

#[tokio::test]
async fn reminders_go_out_once_and_survive_a_crash() {
    let Some(db) = db().await else { return };
    let [user, other] = ids();
    let now = micros(Utc::now());
    let past = now - Duration::minutes(5);

    let first = db
        .add_text_reminder(user, "tea", past)
        .await
        .unwrap()
        .expect("room for one");
    db.add_text_reminder(user, "later", now + Duration::hours(2))
        .await
        .unwrap();
    db.add_text_reminder(other, "not mine", past).await.unwrap();
    db.set_optin(user, "daily", true).await.unwrap();
    db.schedule_reminder(user, "daily", 0, "daily is ready", past)
        .await
        .unwrap();
    // The same thing scheduled twice is one reminder, at the later time given.
    db.schedule_reminder(user, "daily", 0, "daily is ready", past + Duration::seconds(1))
        .await
        .unwrap();
    assert_eq!(db.text_reminders(user).await.unwrap().len(), 2);

    // Pods that ask at the same moment never get the same reminder.
    let mine = |rows: Vec<pinchora_db::ReminderRow>| {
        rows.into_iter()
            .filter(|r| r.user_id == user as i64 || r.user_id == other as i64)
            .collect::<Vec<_>>()
    };
    let (a, b, c) = tokio::join!(
        db.claim_due_reminders(now, 1000),
        db.claim_due_reminders(now, 1000),
        db.claim_due_reminders(now, 1000)
    );
    let batches = [mine(a.unwrap()), mine(b.unwrap()), mine(c.unwrap())];
    let all: Vec<&pinchora_db::ReminderRow> = batches.iter().flatten().collect();
    assert_eq!(all.len(), 3, "tea, not mine and the daily one, once each: {batches:?}");
    assert!(all.iter().any(|r| r.id == first && r.body == "tea"));
    assert!(
        db.claim_due_reminders(now, 1000)
            .await
            .unwrap()
            .iter()
            .all(|r| r.user_id != user as i64),
        "leased reminders wait for the pod that took them"
    );
    assert!(all.iter().all(|r| r.due_at <= now), "nothing that is not due yet");

    // Sent: gone. A crash instead of that leaves the lease to run out and the reminder to come back.
    for r in all.iter().filter(|r| r.user_id == user as i64) {
        db.finish_reminder(r).await.unwrap();
    }
    assert_eq!(
        db.text_reminders(user).await.unwrap().len(),
        1,
        "only the later one is left"
    );
    sqlx::query("UPDATE reminders SET lease_until = now() - interval '1 second' WHERE user_id = $1")
        .bind(other as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let again = mine(db.claim_due_reminders(now, 1000).await.unwrap());
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].body, "not mine");

    // One that was moved while its message went out is not deleted by the sender.
    let sent = &again[0];
    db.schedule_reminder(other, "pet", 5, "pet", past).await.unwrap();
    let pet = mine(db.claim_due_reminders(now, 1000).await.unwrap());
    assert_eq!(pet.len(), 1);
    db.schedule_reminder(other, "pet", 5, "pet", now + Duration::hours(3))
        .await
        .unwrap();
    db.finish_reminder(&pet[0]).await.unwrap();
    assert_eq!(
        db.scheduled_reminder(other, "pet", 5).await.unwrap(),
        Some(now + Duration::hours(3))
    );
    db.postpone_reminder(sent, now + Duration::hours(1)).await.unwrap();
    assert!(
        mine(db.claim_due_reminders(now, 1000).await.unwrap()).is_empty(),
        "postponed means not due yet"
    );

    // Whoever cancels can only cancel their own text reminders.
    let later = db.text_reminders(user).await.unwrap()[0].id;
    assert!(!db.cancel_text_reminder(other, later).await.unwrap());
    assert!(db.cancel_text_reminder(user, later).await.unwrap());

    // Turning a notification off drops what was waiting for it.
    db.schedule_reminder(user, "daily", 0, "daily is ready", now + Duration::hours(1))
        .await
        .unwrap();
    db.set_optin(user, "daily", false).await.unwrap();
    assert_eq!(db.scheduled_reminder(user, "daily", 0).await.unwrap(), None);
    assert!(db.reminder_optins(user).await.unwrap().is_empty());
    assert!(db.set_optin(user, "bogus", true).await.is_err());
    // Nothing of this stays in the shared database.
    db.delete_user_data(user).await.unwrap();
    db.delete_user_data(other).await.unwrap();
}

#[tokio::test]
async fn at_most_25_reminders_are_open_even_when_added_at_once() {
    let Some(db) = db().await else { return };
    let [user] = ids();
    let due = Utc::now() + Duration::hours(1);
    let mut adds = Vec::new();
    for n in 0..30 {
        let db = db.clone();
        adds.push(tokio::spawn(async move {
            db.add_text_reminder(user, &format!("r{n}"), due).await.unwrap()
        }));
    }
    let mut made = 0;
    for add in adds {
        made += usize::from(add.await.unwrap().is_some());
    }
    assert_eq!(made, 25);
    assert_eq!(db.text_reminders(user).await.unwrap().len(), 25);
    // Notifications do not count against the 25.
    db.schedule_reminder(user, "weekly", 0, "x", due).await.unwrap();
    let first = db.text_reminders(user).await.unwrap()[0].id;
    assert!(db.cancel_text_reminder(user, first).await.unwrap());
    assert!(db.add_text_reminder(user, "again", due).await.unwrap().is_some());
    db.delete_user_data(user).await.unwrap();
}

#[tokio::test]
async fn the_months_are_counted_where_things_happen() {
    let Some(db) = db().await else { return };
    let [user, other, guild] = ids();
    let month = Utc::now().date_naive();
    let this_year = chrono::Datelike::year(&month);
    let sum = |rows: &[(u32, pinchora_db::Activity)]| {
        rows.iter()
            .fold(pinchora_db::Activity::default(), |a, (_, m)| a.plus(*m))
    };

    db.add_xp(
        guild,
        user,
        XpGrant {
            server_xp: 20,
            global_xp: 18,
            points: 0,
        },
        NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
    )
    .await
    .unwrap();
    db.add_voice_xp(guild, &[(user, 7)], NaiveDate::from_ymd_opt(2026, 1, 5).unwrap())
        .await
        .unwrap();
    db.earn_shells(user, 300, "fishing").await.unwrap();
    db.earn_shells(user, 99, "duel_refund").await.unwrap();
    db.spend_shells(user, 10, "shop").await.unwrap();
    db.log_catch(user, "minnow", 3.0).await.unwrap();
    db.log_catch(user, "minnow", 3.5).await.unwrap();
    db.transfer_credits(user, other, 100).await.unwrap();
    sqlx::query("INSERT INTO houses (user_id) VALUES ($1), ($2) ON CONFLICT DO NOTHING")
        .bind(user as i64)
        .bind(other as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.record_visit(other, user, month, 25, 10).await.unwrap();
    db.record_visit(other, user, month, 25, 10).await.unwrap();
    db.like_house(other, user, month).await.unwrap();
    db.bump_activity(user, Counter::PetCare, 3).await.unwrap();

    let me = db.activity_year(user, this_year).await.unwrap();
    assert_eq!(me.len(), 1, "everything landed in this month: {me:?}");
    assert_eq!(me[0].0, chrono::Datelike::month(&month));
    let total = sum(&me);
    assert_eq!(total.messages, 1, "only the chat message");
    assert_eq!(total.xp, 18 + 7, "chat's global XP and voice XP");
    assert_eq!(
        total.shells,
        300 + 25,
        "fishing and the visit pay; a refund, a purchase and a transfer out do not"
    );
    assert_eq!(total.fish, 2);
    assert_eq!(total.visits, 1, "only the first visit of the day");
    assert_eq!((total.likes_given, total.likes_received), (1, 0));
    assert_eq!(total.pet_care, 3);
    let theirs = sum(&db.activity_year(other, this_year).await.unwrap());
    assert_eq!((theirs.likes_received, theirs.likes_given, theirs.shells), (1, 0, 0));
    assert_eq!(theirs.xp, 0);

    // Months of other years are kept apart, and the first of the month is where a day lands.
    let jan = NaiveDate::from_ymd_opt(2001, 1, 17).unwrap();
    db.add_activity_in(user, jan, Counter::Messages, 40).await.unwrap();
    db.add_activity_in(user, jan, Counter::Messages, 2).await.unwrap();
    db.add_activity_in(user, NaiveDate::from_ymd_opt(2001, 3, 1).unwrap(), Counter::Fish, 5)
        .await
        .unwrap();
    let old = db.activity_year(user, 2001).await.unwrap();
    assert_eq!(old.iter().map(|(m, _)| *m).collect::<Vec<_>>(), [1, 3]);
    assert_eq!((old[0].1.messages, old[1].1.fish), (42, 5));
    assert!(db.activity_year(user, 2002).await.unwrap().is_empty());
}
