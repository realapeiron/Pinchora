//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! Privacy choices, "delete my data", the weekly and monthly rewards, reminders and the monthly
//! numbers behind Wrapped.

use chrono::{Duration, NaiveDate, Utc};
use pinchora_core::economy::{Period, daily_reward};
use pinchora_db::{
    CLEARED_COLUMNS, Counter, DailyOutcome, Db, DuelStart, GiveawayOpen, Hide, ID_LISTS, ListEdit, NewGame,
    NewGiveaway, OTHER_TABLES, PENDING, PeriodOutcome, RepOutcome, USER_COLUMNS, VisitOutcome, XpGrant,
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
    // A giveaway they host, with its prize held from them.
    let before = db.user(user).await.unwrap().credits;
    let GiveawayOpen::Started(hosted) = db
        .create_giveaway(NewGiveaway {
            guild_id: guild,
            channel_id: 1,
            host: user,
            prize: "Shells",
            shells: 50,
            winners: 2,
            required_role: None,
            ends_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("the giveaway starts");
    };
    assert_eq!(db.user(user).await.unwrap().credits, before - 100, "the prize is held");
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
        "INSERT INTO fishing_gear (user_id, spot, rod, bait) VALUES ($1, 'shore', 'driftwood_rod', 'worm')",
        "INSERT INTO user_rods (user_id, rod_id) VALUES ($1, 'reed_rod')",
        "INSERT INTO user_baits (user_id, bait_id, qty) VALUES ($1, 'worm', 3)",
        "INSERT INTO fish_bucket (user_id, fish_id, size_cm) VALUES ($1, 'minnow', 4.5)",
        "INSERT INTO fish_milestones (user_id, species) VALUES ($1, 5)",
        // ($1 doubles as the server here too.)
        "INSERT INTO weekly_catch (guild_id, week, user_id, fish_id, size_cm, ratio)
         VALUES ($1, current_date, $1, 'minnow', 4.5, 1.1)",
        // Somebody else's giveaway the player entered and won ($1 doubles as the server).
        "INSERT INTO giveaways (guild_id, channel_id, host_id, prize, shells, winners, ends_at, status, winner_ids, announced)
         VALUES ($1, 2, $2, 'Hat', 0, 2, now(), 'ended', ARRAY[$1, $2], TRUE)",
        "INSERT INTO giveaway_entries (giveaway_id, user_id)
         SELECT id, $1 FROM giveaways WHERE guild_id = $1 AND prize = 'Hat'
         UNION ALL SELECT id, $2 FROM giveaways WHERE guild_id = $1 AND prize = 'Hat'",
        "INSERT INTO suggestions (guild_id, number, author_id, body, status, decided_by)
         VALUES ($1, 1, $1, 'Add a garden', 'approved', $2), ($1, 2, $2, 'Remove it', 'denied', $1)",
        "INSERT INTO suggestion_votes (suggestion_id, user_id, up)
         SELECT id, $1, TRUE FROM suggestions WHERE guild_id = $1 AND number = 2",
        "INSERT INTO polls (guild_id, channel_id, author_id, question, options, ends_at)
         VALUES ($1, 2, $1, 'Mine?', ARRAY['a', 'b'], now() + interval '1 day'),
                ($1, 2, $2, 'Theirs?', ARRAY['a', 'b'], now() + interval '1 day')",
        "INSERT INTO poll_votes (poll_id, user_id, choice)
         SELECT id, $1, 0 FROM polls WHERE guild_id = $1 AND author_id = $2",
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

    let deleted = db.delete_user_data(user, Utc::now()).await.unwrap();
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
    for (table, column, _) in ID_LISTS {
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
    for (table, column) in CLEARED_COLUMNS {
        assert_eq!(
            count(db, table, column, user).await,
            0,
            "{table}.{column} still names the player"
        );
    }
    let (winners,): (Vec<i64>,) =
        sqlx::query_as("SELECT winner_ids FROM giveaways WHERE host_id = $1 AND prize = 'Hat'")
            .bind(other as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        winners,
        [other as i64],
        "the player is taken out of the winners, not blanked"
    );
    let (decided,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM suggestions WHERE guild_id = $1 AND decided_by IS NULL")
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        decided, 1,
        "the suggestion the player decided stays, undecided by anyone"
    );
    let (kept,): (i64,) = sqlx::query_as("SELECT count(*) FROM suggestions WHERE guild_id = $1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(kept, 1, "only the other player's suggestion is left");

    // The other player keeps their own things, and their bet is back.
    assert_eq!(
        db.user(other).await.unwrap().credits,
        500,
        "the held wager went back to the player left"
    );
    assert_eq!(count(db, "social_names", "user_id", other).await, 1);
    assert!(db.partner(other).await.unwrap().is_none(), "the pair is gone");
    assert_eq!(count(db, "contest_entries", "user_id", other).await, 1);
    assert!(
        db.giveaway(hosted.id).await.unwrap().is_none(),
        "the giveaway they hosted is called off"
    );
    assert_eq!(count(db, "giveaway_entries", "user_id", other).await, 1);
    assert_eq!(count(db, "polls", "author_id", other).await, 1, "their polls stay");
    assert_eq!(
        count(db, "contest_votes", "voter_id", other).await,
        0,
        "the vote on the deleted entry went with it"
    );

    // Deleting again is quiet, and the player starts over as a new one.
    assert_eq!(db.delete_user_data(user, Utc::now()).await.unwrap().rows, 0);
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
    let named: Vec<(&str, &str)> = USER_COLUMNS
        .iter()
        .copied()
        .chain(CLEARED_COLUMNS.iter().copied())
        .chain(ID_LISTS.iter().map(|(t, c, _)| (*t, *c)))
        .collect();
    for (table, column) in &named {
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

    // A column that looks like it holds a player's id (or a list of them) must be dealt with: listed
    // as a player column, cleared, or an id list; or sit in a table that belongs to no player.
    let words = [
        "user",
        "owner",
        "host",
        "guest",
        "author",
        "liker",
        "voter",
        "friend",
        "seller",
        "blocked",
        "from_",
        "to_id",
        "partner",
        "entry_id",
        "winner",
        "recipient",
        "sender",
        "creator",
    ];
    let columns: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT table_name::TEXT, column_name::TEXT, udt_name::TEXT FROM information_schema.columns
         WHERE table_schema = current_schema() AND udt_name IN ('int8', '_int8') ORDER BY 1, 2",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    for (table, column, kind) in columns {
        let looks_like_a_player =
            (words.iter().any(|w| column.contains(w)) || column.ends_with("_by")) && !column.contains("guestbook");
        if !looks_like_a_player {
            continue;
        }
        if kind == "_int8" {
            assert!(
                ID_LISTS.iter().any(|(t, c, _)| *t == table && *c == column),
                "{table}.{column} looks like a list of players' ids but \"delete my data\" does not take the player out of it"
            );
        } else {
            let dealt_with =
                named.contains(&(table.as_str(), column.as_str())) || OTHER_TABLES.contains(&table.as_str());
            assert!(
                dealt_with,
                "{table}.{column} looks like a player's id but \"delete my data\" does not clear it"
            );
        }
    }
    // Lists that run in step with another one are blanked, not shortened.
    assert!(ID_LISTS.iter().any(|(_, _, edit)| *edit == ListEdit::Blank));
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
    db.delete_user_data(user, Utc::now()).await.unwrap();
    db.delete_user_data(other, Utc::now()).await.unwrap();
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
    db.delete_user_data(user, Utc::now()).await.unwrap();
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
    db.earn_shells(user, 200, "duel_pot").await.unwrap();
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
    assert_eq!(total.xp, 20 + 7, "chat's server XP and voice XP");
    assert_eq!(
        total.shells,
        300 + 25 + 100,
        "fishing and the visit pay, and a duel adds what was won (not the stake in the pot); a refund, a purchase and a transfer out do not"
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

    // The favourite fish is the year's, not the book's.
    assert_eq!(
        db.favourite_fish(user, this_year).await.unwrap(),
        Some(("minnow".to_owned(), 2))
    );
    for _ in 0..3 {
        db.add_catch_in(user, 2001, "pearl_oyster").await.unwrap();
    }
    db.add_catch_in(user, 2001, "minnow").await.unwrap();
    assert_eq!(
        db.favourite_fish(user, 2001).await.unwrap(),
        Some(("pearl_oyster".to_owned(), 3))
    );
    assert_eq!(db.favourite_fish(user, 2002).await.unwrap(), None);
}

/// What deleting data must not be: a way to get a reward, a rep, a paid visit, a wordle payout or
/// a walk a second time.
#[tokio::test]
async fn deleting_and_coming_back_resets_no_cooldown() {
    let Some(db) = db().await else { return };
    let [user, alt, other_alt] = ids();
    let now = micros(Utc::now());
    let weekly = |at| db.claim_period(user, Period::Weekly, at);
    let monthly = |at| db.claim_period(user, Period::Monthly, at);

    // The loop: collect everything, hand it to an alt, delete, come back and do it again.
    let mut legit = 0;
    for round in 0..4 {
        let daily = db.claim_daily(user).await.unwrap();
        let week = weekly(now).await.unwrap();
        let month = monthly(now).await.unwrap();
        let rep = db.give_rep(user, other_alt).await.unwrap();
        if round == 0 {
            assert!(matches!(daily, DailyOutcome::Claimed { .. }), "{daily:?}");
            assert!(matches!(week, PeriodOutcome::Claimed { .. }), "{week:?}");
            assert!(matches!(month, PeriodOutcome::Claimed { .. }), "{month:?}");
            assert!(matches!(rep, RepOutcome::Given { .. }), "{rep:?}");
            legit = daily_reward(1) + Period::Weekly.reward() + Period::Monthly.reward();
        } else {
            assert!(
                matches!(daily, DailyOutcome::Cooldown { .. }),
                "round {round}: {daily:?}"
            );
            assert!(
                matches!(week, PeriodOutcome::Cooldown { .. }),
                "round {round}: {week:?}"
            );
            assert!(
                matches!(month, PeriodOutcome::Cooldown { .. }),
                "round {round}: {month:?}"
            );
            assert!(matches!(rep, RepOutcome::Cooldown { .. }), "round {round}: {rep:?}");
        }
        let balance = db.user(user).await.unwrap().credits;
        if balance > 0 {
            db.transfer_credits(user, alt, balance).await.unwrap();
        }
        db.delete_user_data(user, now).await.unwrap();
        assert_eq!(db.user(user).await.unwrap().credits, 0, "they come back with nothing");
    }
    assert_eq!(
        db.user(alt).await.unwrap().credits,
        legit,
        "one legitimate cycle, not four"
    );
    assert_eq!(db.user(other_alt).await.unwrap().reputation, 1, "one rep, not four");
    let (rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM credit_ledger WHERE user_id = $1 AND delta > 0")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0, "nothing was paid to them after coming back");

    // The wallet reads the cooldowns off the row, and they are on it.
    let back = db.user(user).await.unwrap();
    assert!(back.last_daily_at.is_some() && back.last_rep_at.is_some());
    assert_eq!(db.period_claimed_at(user, Period::Weekly).await.unwrap(), Some(now));

    // Time still passes: a day later the daily and the rep work again, a week later the weekly one,
    // a month later the monthly one.
    sqlx::query(
        "UPDATE deleted_users SET last_daily_at = last_daily_at - interval '30 hours',
                                  last_rep_at = last_rep_at - interval '30 hours' WHERE user_id = $1",
    )
    .bind(user as i64)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE users SET last_daily_at = last_daily_at - interval '30 hours', last_rep_at = last_rep_at - interval '30 hours' WHERE id = $1")
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(matches!(
        db.claim_daily(user).await.unwrap(),
        DailyOutcome::Claimed { .. }
    ));
    assert!(matches!(
        db.give_rep(user, other_alt).await.unwrap(),
        RepOutcome::Given { .. }
    ));
    assert!(matches!(
        weekly(now + Duration::days(6)).await.unwrap(),
        PeriodOutcome::Cooldown { .. }
    ));
    assert!(matches!(
        weekly(now + Duration::days(8)).await.unwrap(),
        PeriodOutcome::Claimed { .. }
    ));
    assert!(matches!(
        monthly(now + Duration::days(29)).await.unwrap(),
        PeriodOutcome::Cooldown { .. }
    ));
    assert!(matches!(
        monthly(now + Duration::days(31)).await.unwrap(),
        PeriodOutcome::Claimed { .. }
    ));

    // Deleting twice in a row keeps the later of each time, and a player with nothing leaves no trace.
    db.delete_user_data(user, now).await.unwrap();
    db.delete_user_data(user, now + Duration::hours(1)).await.unwrap();
    assert!(matches!(
        db.claim_daily(user).await.unwrap(),
        DailyOutcome::Cooldown { .. }
    ));
    let [nobody] = ids();
    db.user(nobody).await.unwrap();
    db.delete_user_data(nobody, now).await.unwrap();
    assert_eq!(count(&db, "deleted_users", "user_id", nobody).await, 0);

    // Tombstones are dropped when the longest cooldown is long over.
    sqlx::query("UPDATE deleted_users SET deleted_at = $2 WHERE user_id = $1")
        .bind(user as i64)
        .bind(now - Duration::days(36))
        .execute(db.pool())
        .await
        .unwrap();
    assert!(db.forget_old_tombstones(now).await.unwrap() >= 1);
    assert_eq!(count(&db, "deleted_users", "user_id", user).await, 0);
    db.delete_user_data(user, now).await.unwrap();
    db.delete_user_data(alt, now).await.unwrap();
    db.delete_user_data(other_alt, now).await.unwrap();
}

/// The other limits that sit in rows: paid visits, likes, the daily wordle and the pet walk.
#[tokio::test]
async fn paid_visits_likes_wordle_and_walks_survive_deleting() {
    let Some(db) = db().await else { return };
    let [guest, h1, h2, h3, liker, host] = ids();
    let now = micros(Utc::now());
    let day = now.date_naive();
    let (reward, cap) = (25, 2);
    let paid = |outcome: VisitOutcome| match outcome {
        VisitOutcome::Rewarded { reward } => reward,
        VisitOutcome::AlreadyToday => -1,
    };

    // A guest is paid for two houses a day, whoever deletes what afterwards.
    assert_eq!(paid(db.record_visit(h1, guest, day, reward, cap).await.unwrap()), 25);
    assert_eq!(paid(db.record_visit(h2, guest, day, reward, cap).await.unwrap()), 25);
    assert_eq!(
        paid(db.record_visit(h3, guest, day, reward, cap).await.unwrap()),
        0,
        "the cap"
    );
    // The hosts delete their data and come back: the guest's visits of the day still count.
    for host in [h1, h2, h3] {
        db.delete_user_data(host, now).await.unwrap();
    }
    assert_eq!(
        count(&db, "visits", "host_id", h1).await,
        0,
        "the host's id is gone from the visit"
    );
    assert_eq!(
        paid(db.record_visit(h1, guest, day, reward, cap).await.unwrap()),
        0,
        "the host starting over does not reset the guest's day"
    );
    // And the guest deleting does not either.
    db.delete_user_data(guest, now).await.unwrap();
    assert_eq!(
        paid(db.record_visit(h2, guest, day, reward, cap).await.unwrap()),
        0,
        "the guest starting over does not reset their day"
    );
    // Tomorrow is a new day.
    assert_eq!(
        paid(
            db.record_visit(h2, guest, day + Duration::days(1), reward, cap)
                .await
                .unwrap()
        ),
        25
    );
    // The Wrapped numbers never point at a deleted host.
    assert_eq!(
        db.most_visited_house(guest, chrono::Datelike::year(&day))
            .await
            .unwrap()
            .map(|(h, _, _)| h),
        Some(h2)
    );

    // Likes: the one given today is still given, and a deleted liker's like comes off the house.
    sqlx::query("INSERT INTO houses (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(host as i64)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(db.like_house(host, liker, day).await.unwrap(), Some(1));
    db.delete_user_data(liker, now).await.unwrap();
    let (likes,): (i32,) = sqlx::query_as("SELECT likes FROM houses WHERE user_id = $1")
        .bind(host as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(likes, 0, "the deleted player's like came off the house");
    assert_eq!(
        db.like_house(host, liker, day).await.unwrap(),
        None,
        "today's like is already given"
    );
    assert_eq!(
        db.like_house(host, liker, day + Duration::days(1)).await.unwrap(),
        Some(1),
        "tomorrow they can like again"
    );

    // The daily wordle pays once a day; a board deleted with the data is still played.
    let [player] = ids();
    db.start_wordle(player, "daily", 20_000, "coral", false).await.unwrap();
    db.delete_user_data(player, now).await.unwrap();
    assert!(db.daily_wordle_played_before_deleting(player, 20_000).await.unwrap());
    assert!(!db.daily_wordle_played_before_deleting(player, 20_001).await.unwrap());

    // The walk's cooldown follows the player into a new pet.
    let [walker] = ids();
    db.adopt(walker, "crab", "Pinchy").await.unwrap();
    assert_eq!(db.walk_locked_until(walker, now).await.unwrap(), None);
    db.delete_user_data(walker, now).await.unwrap();
    let until = db.walk_locked_until(walker, now).await.unwrap().expect("a walk lock");
    assert_eq!(until, now + Duration::minutes(45));
    assert_eq!(
        db.walk_locked_until(walker, now + Duration::minutes(46)).await.unwrap(),
        None
    );

    for user in [guest, h1, h2, h3, liker, host, player, walker] {
        db.delete_user_data(user, now + Duration::days(40)).await.unwrap();
    }
}

/// Shells held for a giveaway or a duel are not lost to others, and not resurrected for the host.
#[tokio::test]
async fn deleting_a_host_calls_the_giveaway_off_and_keeps_the_money_in_one_place() {
    let Some(db) = db().await else { return };
    let [host, entrant, guild] = ids();
    let now = micros(Utc::now());
    db.earn_shells(host, 500, "test").await.unwrap();
    let GiveawayOpen::Started(giveaway) = db
        .create_giveaway(NewGiveaway {
            guild_id: guild,
            channel_id: 1,
            host,
            prize: "Shells",
            shells: 100,
            winners: 2,
            required_role: None,
            ends_at: now - Duration::minutes(1),
        })
        .await
        .unwrap()
    else {
        panic!("the giveaway starts");
    };
    db.toggle_giveaway_entry(giveaway.id, entrant, now - Duration::minutes(2))
        .await
        .unwrap();
    assert_eq!(db.user(host).await.unwrap().credits, 300, "the prize is held");

    db.delete_user_data(host, now).await.unwrap();
    assert!(db.giveaway(giveaway.id).await.unwrap().is_none());
    assert!(
        db.due_giveaways(now, 1_000)
            .await
            .unwrap()
            .iter()
            .all(|id| *id != giveaway.id)
    );
    assert_eq!(
        db.user(host).await.unwrap().credits,
        0,
        "no prize comes back to a player who left"
    );
    assert_eq!(
        db.user(entrant).await.unwrap().credits,
        0,
        "and nobody is paid from it either"
    );
    db.delete_user_data(entrant, now).await.unwrap();
}

/// A claim that arrives while a delete of the same player has removed the row and written the
/// tombstone but not committed yet must wait for it and then find the tombstone: no second payout,
/// no fresh cooldown, and no error for the player. (Before the player lock, the claim's insert
/// created a row with no cooldowns under the delete, and the claims that were already running lost
/// their row and failed.)
#[tokio::test]
async fn claims_that_overlap_a_delete_wait_for_it_and_pay_nothing() {
    let Some(db) = db().await else { return };
    let [user, other, host, other_host] = ids();
    let now = micros(Utc::now());
    let day = now.date_naive();
    let (reward, cap) = (25, 1);

    // Everything the player can use up in a day, used up.
    assert!(matches!(
        db.claim_daily(user).await.unwrap(),
        DailyOutcome::Claimed { .. }
    ));
    for period in [Period::Weekly, Period::Monthly] {
        assert!(matches!(
            db.claim_period(user, period, now).await.unwrap(),
            PeriodOutcome::Claimed { .. }
        ));
    }
    assert!(matches!(
        db.give_rep(user, other).await.unwrap(),
        RepOutcome::Given { .. }
    ));
    for house in [host, other_host] {
        sqlx::query("INSERT INTO houses (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(house as i64)
            .execute(db.pool())
            .await
            .unwrap();
    }
    assert_eq!(db.like_house(host, user, day).await.unwrap(), Some(1));
    assert!(matches!(
        db.record_visit(host, user, day, reward, cap).await.unwrap(),
        VisitOutcome::Rewarded { reward: 25 }
    ));

    // The delete is done but for its commit. Claims of every kind start now.
    let mut delete = db.pool().begin().await.unwrap();
    Db::delete_user_data_in(&mut delete, user, now).await.unwrap();
    let daily = tokio::spawn({
        let db = db.clone();
        async move { db.claim_daily(user).await }
    });
    let weekly = tokio::spawn({
        let db = db.clone();
        async move { db.claim_period(user, Period::Weekly, now).await }
    });
    let monthly = tokio::spawn({
        let db = db.clone();
        async move { db.claim_period(user, Period::Monthly, now).await }
    });
    let rep = tokio::spawn({
        let db = db.clone();
        async move { db.give_rep(user, other).await }
    });
    let like = tokio::spawn({
        let db = db.clone();
        async move { db.like_house(host, user, day).await }
    });
    let visit = tokio::spawn({
        let db = db.clone();
        async move { db.record_visit(other_host, user, day, reward, cap).await }
    });
    // A claim-free way to create the row, and the two reads of the tombstone, wait too.
    let wallet = tokio::spawn({
        let db = db.clone();
        async move { db.user(user).await }
    });
    let walk = tokio::spawn({
        let db = db.clone();
        async move { db.walk_locked_until(user, now).await }
    });
    let wordle = tokio::spawn({
        let db = db.clone();
        async move { db.daily_wordle_played_before_deleting(user, 20_000).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    assert!(
        !daily.is_finished() && !weekly.is_finished() && !monthly.is_finished(),
        "the claims wait for the delete instead of running under it"
    );
    assert!(!rep.is_finished() && !like.is_finished() && !visit.is_finished());
    assert!(!wallet.is_finished() && !walk.is_finished() && !wordle.is_finished());
    delete.commit().await.unwrap();

    let daily = daily.await.unwrap().unwrap();
    let weekly = weekly.await.unwrap().unwrap();
    let monthly = monthly.await.unwrap().unwrap();
    assert!(matches!(daily, DailyOutcome::Cooldown { .. }), "{daily:?}");
    assert!(matches!(weekly, PeriodOutcome::Cooldown { .. }), "{weekly:?}");
    assert!(matches!(monthly, PeriodOutcome::Cooldown { .. }), "{monthly:?}");
    let rep = rep.await.unwrap().unwrap();
    assert!(matches!(rep, RepOutcome::Cooldown { .. }), "{rep:?}");
    assert_eq!(like.await.unwrap().unwrap(), None, "today's like was already given");
    let visit = visit.await.unwrap().unwrap();
    assert!(
        matches!(visit, VisitOutcome::Rewarded { reward: 0 }),
        "the day's paid visit was already used: {visit:?}"
    );
    let wallet = wallet.await.unwrap().unwrap();
    assert!(wallet.last_daily_at.is_some() && wallet.last_rep_at.is_some());
    assert_eq!(wallet.credits, 0, "they come back with nothing");
    assert!(walk.await.unwrap().is_ok() && wordle.await.unwrap().is_ok());
    assert_eq!(db.user(user).await.unwrap().credits, 0);
    let (paid,): (i64,) = sqlx::query_as("SELECT count(*) FROM credit_ledger WHERE user_id = $1 AND delta > 0")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(paid, 0, "nothing was paid to the player after the delete");
    for gone in [user, other, host, other_host] {
        db.delete_user_data(gone, now).await.unwrap();
    }
}

/// The real delete against claims that come in on their own connections across the whole of it, as
/// separate interactions would: over many rounds nothing is paid beyond the one legitimate claim,
/// and no claim fails.
#[tokio::test]
async fn claims_spread_over_a_delete_pay_nothing_and_never_fail() {
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect(&url, 32).await.expect("connect");
    db.migrate().await.expect("migrate");

    // How long a delete takes on this machine, to spread the claims over a little more than that.
    let [probe] = ids();
    db.claim_daily(probe).await.unwrap();
    let started = std::time::Instant::now();
    db.delete_user_data(probe, Utc::now()).await.unwrap();
    let span_us = (started.elapsed().as_micros() as u64 * 3 / 2).max(2_000);

    let (rounds, per_round) = (30, 60u64);
    let (mut paid, mut shells, mut errors) = (0u32, 0i64, Vec::new());
    for round in 0..rounds {
        let [user, other] = ids();
        let now = Utc::now();
        db.claim_daily(user).await.unwrap();
        db.claim_period(user, Period::Weekly, now).await.unwrap();
        db.claim_period(user, Period::Monthly, now).await.unwrap();
        db.give_rep(user, other).await.unwrap();
        let mut claims = Vec::new();
        for i in 0..per_round {
            let db = db.clone();
            let at = std::time::Duration::from_micros(span_us * i / per_round);
            claims.push(tokio::spawn(async move {
                tokio::time::sleep(at).await;
                match i % 4 {
                    0 => db.claim_daily(user).await.map(|o| match o {
                        DailyOutcome::Claimed { amount, .. } => amount,
                        DailyOutcome::Cooldown { .. } => 0,
                    }),
                    1 => db
                        .claim_period(user, Period::Weekly, Utc::now())
                        .await
                        .map(|o| match o {
                            PeriodOutcome::Claimed { amount, .. } => amount,
                            PeriodOutcome::Cooldown { .. } => 0,
                        }),
                    2 => db
                        .claim_period(user, Period::Monthly, Utc::now())
                        .await
                        .map(|o| match o {
                            PeriodOutcome::Claimed { amount, .. } => amount,
                            PeriodOutcome::Cooldown { .. } => 0,
                        }),
                    _ => db.give_rep(user, other).await.map(|o| match o {
                        RepOutcome::Given { .. } => 1,
                        RepOutcome::Cooldown { .. } => 0,
                    }),
                }
            }));
        }
        db.delete_user_data(user, Utc::now()).await.unwrap();
        for claim in claims {
            match claim.await.unwrap() {
                Ok(0) => {}
                Ok(amount) => {
                    paid += 1;
                    shells += amount;
                }
                Err(error) => errors.push(format!("round {round}: {error}")),
            }
        }
        db.delete_user_data(user, Utc::now()).await.unwrap();
        db.delete_user_data(other, Utc::now()).await.unwrap();
    }
    assert_eq!(
        (paid, shells),
        (0, 0),
        "claims that came in during a delete were paid again"
    );
    assert!(errors.is_empty(), "claims failed: {errors:#?}");
}
