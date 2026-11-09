use crate::privacy::*;

#[test]
fn fixture_id_batches_never_overlap() {
    let first: [u64; 1000] = ids();
    let second: [u64; 1000] = ids();
    assert_eq!(
        first
            .into_iter()
            .chain(second)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        2000
    );
}

#[tokio::test]
async fn deleting_a_player_leaves_nothing_in_any_table() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
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
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT table_name::TEXT FROM information_schema.tables
         WHERE table_schema = current_schema() AND table_type = 'BASE TABLE' ORDER BY table_name",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    let listed: Vec<&str> = USER_COLUMNS
        .iter()
        .map(|(t, _)| *t)
        .chain(DAILY_LIMITS.iter().map(|(t, _, _)| *t))
        .chain(ANONYMISED.iter().map(|(t, _, _)| *t))
        .chain(ZEROED_COLUMNS.iter().map(|(t, _)| *t))
        .collect();
    let unknown: Vec<&str> = tables
        .iter()
        .map(|(t,)| t.as_str())
        .filter(|t| !listed.contains(t) && !OTHER_TABLES.contains(t))
        .collect();
    assert!(
        unknown.is_empty(),
        "tables {unknown:?} are in none of USER_COLUMNS, DAILY_LIMITS, ANONYMISED, ZEROED_COLUMNS or OTHER_TABLES \
         (crates/pinchora-db/src/privacy.rs): \"delete my data\" would miss them"
    );
    let named: Vec<(&str, &str)> = USER_COLUMNS
        .iter()
        .copied()
        .chain(CLEARED_COLUMNS.iter().copied())
        .chain(ID_LISTS.iter().map(|(t, c, _)| (*t, *c)))
        .chain(DAILY_LIMITS.iter().map(|(t, c, _)| (*t, *c)))
        .chain(ANONYMISED.iter().map(|(t, c, _)| (*t, *c)))
        .chain(ZEROED_COLUMNS.iter().copied())
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
        "taker",
        "actor",
        "pick",
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
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db(8).await else { return };
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
async fn reminders_go_out_once_and_survive_a_crash() {
    let _isolation = crate::support::exclusive().await;
    let Some(db) = db(8).await else { return };
    // The exclusive guard makes old fixtures safe to clear before the bounded worker scan.
    sqlx::query("DELETE FROM reminders").execute(db.pool()).await.unwrap();
    let [user, other] = ids();
    let now = micros(Utc::now());
    let past = now - Duration::minutes(5);

    let first = db
        .add_text_reminder_in(user, "tea", "tr", past)
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
    let texts = db.text_reminders(user).await.unwrap();
    assert_eq!(texts.len(), 2);
    assert_eq!(texts[0].language, "tr");

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
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
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
    let _isolation = crate::support::shared().await;
    let Some(db) = db(8).await else { return };
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
