//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! Server stats: batches written once, the 90-day sweep that skips rows somebody holds, the counter
//! channels' limit and the claim that keeps two pods from working out the same server.

use chrono::{Duration, Utc};
use pinchora_core::server_stats::{HourTally, MAX_COUNTERS, Pair, RENAME_GAP};
use pinchora_db::{CounterSave, Db};

async fn db() -> Option<Db> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 5, std::time::Duration::from_secs(60))
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

fn tally(messages: i64, channel: u64, user: u64) -> HourTally {
    let mut t = HourTally {
        messages,
        joins: 1,
        ..HourTally::default()
    };
    t.channels.insert(
        channel,
        Pair {
            messages,
            voice_minutes: 0,
        },
    );
    t.members.insert(
        user,
        Pair {
            messages,
            voice_minutes: 3,
        },
    );
    t
}

#[tokio::test]
async fn a_batch_is_written_once_and_adds_to_the_hour() {
    let Some(db) = db().await else { return };
    let [guild, channel, user] = ids();
    let hour = Utc::now() - Duration::hours(1);
    let hour = hour - Duration::seconds(hour.timestamp().rem_euclid(3600));
    let key = format!("test:{guild}:a");
    assert!(
        db.write_stats_batch(&key, guild, hour, &tally(5, channel, user))
            .await
            .unwrap()
    );
    assert!(
        !db.write_stats_batch(&key, guild, hour, &tally(5, channel, user))
            .await
            .unwrap()
    );
    // Another batch of the same hour adds up.
    assert!(
        db.write_stats_batch(&format!("test:{guild}:b"), guild, hour, &tally(2, channel, user))
            .await
            .unwrap()
    );
    let hours = db.guild_hours(guild, hour).await.unwrap();
    assert_eq!(hours.len(), 1);
    assert_eq!((hours[0].messages, hours[0].joins), (7, 2));
    let day = hour.date_naive();
    let member = db.member_days(guild, user, day).await.unwrap();
    assert_eq!((member[0].messages, member[0].voice_minutes), (7, 6));
    assert_eq!(db.channel_days(guild, channel, day).await.unwrap()[0].messages, 7);
    assert_eq!(db.active_members(guild, day).await.unwrap(), 1);
    assert_eq!(db.channel_place(guild, channel, day).await.unwrap(), Some((1, 1)));
}

#[tokio::test]
async fn deletion_rejects_member_rows_from_an_already_sealed_batch() {
    let Some(db) = db().await else { return };
    let [guild, channel, user] = ids();
    let hour = Utc::now() - Duration::minutes(1);
    db.delete_user_data(user, Utc::now()).await.unwrap();
    db.write_stats_batch(&format!("deleted:{guild}"), guild, hour, &tally(1, channel, user))
        .await
        .unwrap();
    assert!(db.member_days(guild, user, hour.date_naive()).await.unwrap().is_empty());
}

#[tokio::test]
async fn late_hourly_batches_recompute_yesterdays_global_messages() {
    let Some(db) = db().await else { return };
    let [guild, channel, user] = ids();
    let yesterday = Utc::now().date_naive() - Duration::days(1);
    db.snapshot_global_stats(yesterday).await.unwrap();
    let hour = yesterday.and_hms_opt(23, 0, 0).unwrap().and_utc();
    let before = db
        .global_days(yesterday)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.day == yesterday)
        .unwrap()
        .messages;
    db.write_stats_batch(&format!("late:{guild}"), guild, hour, &tally(3, channel, user))
        .await
        .unwrap();
    db.refresh_global_message_history(yesterday).await.unwrap();
    let after = db
        .global_days(yesterday)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.day == yesterday)
        .unwrap()
        .messages;
    assert!(after >= before + 3);
}

#[tokio::test]
async fn publication_and_audit_commit_together() {
    let Some(db) = db().await else { return };
    let [guild, actor] = ids();
    db.guild_settings(guild).await.unwrap();
    assert!(
        db.set_stats_public_audited(guild, true, actor, "Test manager")
            .await
            .unwrap()
    );
    assert!(db.stats_public(guild).await.unwrap());
    let audit = db.audit_page(guild, None, 10).await.unwrap();
    assert_eq!(audit[0].action, "stats.public");
    assert_eq!(audit[0].actor_id, actor as i64);
}

#[tokio::test]
async fn public_daily_totals_exclude_hidden_channels() {
    let Some(db) = db().await else { return };
    let [guild, public, hidden, user] = ids();
    let hour = Utc::now() - Duration::minutes(1);
    let mut safe = tally(2, public, user);
    safe.public_channels.insert(
        public,
        Pair {
            messages: 2,
            voice_minutes: 0,
        },
    );
    db.write_stats_batch(&format!("public:{guild}"), guild, hour, &safe)
        .await
        .unwrap();
    db.write_stats_batch(&format!("hidden:{guild}"), guild, hour, &tally(9, hidden, user))
        .await
        .unwrap();
    let days = db
        .public_visible_channel_days(guild, hour.date_naive(), &[public as i64])
        .await
        .unwrap();
    assert_eq!(days.iter().map(|d| d.messages).sum::<i64>(), 2);
    assert!(
        db.public_top_channels(guild, hour.date_naive(), 8)
            .await
            .unwrap()
            .iter()
            .all(|rank| rank.id != hidden as i64)
    );
}

#[tokio::test]
async fn the_sweep_forgets_old_days_and_skips_rows_somebody_holds() {
    let Some(db) = db().await else { return };
    let [guild, channel, held_user, free_user] = ids();
    let old = Utc::now() - Duration::days(120);
    let old = old - Duration::seconds(old.timestamp().rem_euclid(3600));
    let recent = Utc::now() - Duration::days(3);
    let recent = recent - Duration::seconds(recent.timestamp().rem_euclid(3600));
    for (key, at, user) in [
        ("old-a", old, held_user),
        ("old-b", old, free_user),
        ("new", recent, free_user),
    ] {
        db.write_stats_batch(&format!("sweep:{guild}:{key}"), guild, at, &tally(1, channel, user))
            .await
            .unwrap();
    }
    // Somebody holds one old row while the sweep runs: it is left for the next sweep, not waited for.
    let mut holder = db.pool().begin().await.unwrap();
    sqlx::query("SELECT 1 FROM member_activity_daily WHERE guild_id = $1 AND user_id = $2 FOR UPDATE")
        .bind(guild as i64)
        .bind(held_user as i64)
        .execute(&mut *holder)
        .await
        .unwrap();
    let cutoff = (Utc::now() - Duration::days(90)).date_naive();
    let swept = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        let mut gone = 0;
        while let Ok(n) = db.sweep_stats(cutoff).await {
            gone += n;
            if n == 0 {
                break;
            }
        }
        gone
    })
    .await
    .expect("the sweep waited for a held row");
    assert!(
        swept >= 3,
        "the old hour, the old channel day and one old member day: {swept}"
    );
    let count = |sql: &'static str| {
        let db = db.clone();
        async move {
            let (n,): (i64,) = sqlx::query_as(sql)
                .bind(guild as i64)
                .fetch_one(db.pool())
                .await
                .unwrap();
            n
        }
    };
    assert_eq!(
        count("SELECT count(*) FROM member_activity_daily WHERE guild_id = $1").await,
        2
    );
    holder.rollback().await.unwrap();
    db.sweep_stats(cutoff).await.unwrap();
    assert_eq!(
        count("SELECT count(*) FROM member_activity_daily WHERE guild_id = $1").await,
        1,
        "the next sweep takes the row once it is free"
    );
    assert_eq!(
        count("SELECT count(*) FROM guild_activity_hourly WHERE guild_id = $1").await,
        1
    );
    assert_eq!(
        count("SELECT count(*) FROM channel_activity_daily WHERE guild_id = $1").await,
        1
    );
}

#[tokio::test]
async fn a_server_has_ten_counters_and_one_pod_works_them_out_at_a_time() {
    let Some(db) = db().await else { return };
    let ids: [u64; 12] = ids();
    let guild = ids[0];
    for channel in &ids[1..=MAX_COUNTERS] {
        let saved = db
            .save_counter(guild, *channel, "members", None, "M: {count}")
            .await
            .unwrap();
        assert!(matches!(saved, CounterSave::Saved(_)));
    }
    assert_eq!(
        db.save_counter(guild, ids[11], "members", None, "M: {count}")
            .await
            .unwrap(),
        CounterSave::Full
    );
    // The one already there can still change.
    let CounterSave::Saved(row) = db
        .save_counter(guild, ids[1], "role_members", Some(7), "R: {count}")
        .await
        .unwrap()
    else {
        panic!("an existing counter could not be changed");
    };
    assert_eq!((row.kind.as_str(), row.role_id), ("role_members", Some(7)));
    assert_eq!(db.counters(guild).await.unwrap().len(), MAX_COUNTERS);

    // Claimed once, then left alone until the gap has passed.
    let now = Utc::now() + Duration::days(3650);
    let first = db.claim_counters(now, 10_000).await.unwrap();
    assert_eq!(
        first.iter().filter(|r| r.guild_id == guild as i64).count(),
        MAX_COUNTERS
    );
    let again = db.claim_counters(now, 10_000).await.unwrap();
    assert!(again.iter().all(|r| r.guild_id != guild as i64), "claimed twice");
    let later = db.claim_counters(now + RENAME_GAP, 10_000).await.unwrap();
    assert_eq!(
        later.iter().filter(|r| r.guild_id == guild as i64).count(),
        MAX_COUNTERS
    );

    let at = Utc::now();
    db.record_counter(row.id, Some(42), Some(("R: 42", at)), None)
        .await
        .unwrap();
    db.record_counter(row.id, Some(43), None, Some("manage_channels"))
        .await
        .unwrap();
    let stored = db
        .counters(guild)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.id == row.id)
        .unwrap();
    assert_eq!(stored.last_name.as_deref(), Some("R: 42"));
    assert_eq!(stored.last_value, Some(43));
    assert_eq!(stored.problem.as_deref(), Some("manage_channels"));
    assert!(db.remove_counter(guild, ids[1]).await.unwrap());
    assert!(!db.remove_counter(guild, ids[1]).await.unwrap());

    assert!(!db.stats_public(guild).await.unwrap());
    db.set_stats_public(guild, true).await.unwrap();
    assert!(db.stats_public(guild).await.unwrap());
    db.snapshot_global_stats(Utc::now().date_naive()).await.unwrap();
    let days = db.global_days(Utc::now().date_naive()).await.unwrap();
    assert_eq!(days.len(), 1);
    assert!(days[0].servers >= 1);
}
