//! Durable delivery intent, fencing, backlog coalescing and privacy on the real database.
use chrono::{Duration as Span, Utc};
use pinchora_core::utility::{Schedule, next_run};
use pinchora_db::{ScheduledRow, UtilityStickyRow, timestamp};
use serde_json::json;

// claim_schedules is a global worker scan: it can lease another test's due rows.
// The global scan is exclusive; per-schedule operations use unique guilds.

use crate::support::db;
use crate::support::id as unique;
fn schedule(guild: u64) -> ScheduledRow {
    ScheduledRow {
        id: 0,
        guild_id: guild as i64,
        channel_id: guild as i64,
        content: "note".into(),
        embed: None,
        schedule: json!(Schedule::Interval { seconds: 60 }),
        tz: "UTC".into(),
        next_run_at: Utc::now() - Span::days(30),
        last_run_at: None,
        runs: 0,
        enabled: true,
        created_by: Some(guild as i64),
        ping_allowed: true,
        lease_until: None,
        pause_reason: None,
    }
}
fn sticky(guild: u64) -> UtilityStickyRow {
    UtilityStickyRow {
        guild_id: guild as i64,
        channel_id: guild as i64,
        content: "note".into(),
        embed: None,
        last_message_id: None,
        min_gap_secs: 5,
        min_messages: 2,
        message_count: 0,
        enabled: true,
        updated_by: Some(guild as i64),
        lease_until: None,
        pause_reason: None,
    }
}

#[tokio::test]
async fn delivery_intent_survives_crashes_and_fences_stale_workers() {
    let _isolation = crate::support::exclusive().await;

    let Some(db) = db(5).await else { return };
    let guild = unique();
    let id = db.add_schedule(&schedule(guild)).await.unwrap().unwrap();
    let (a, b) = tokio::join!(db.claim_schedule_now(guild, id), db.claim_schedule_now(guild, id));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.is_some(), b.is_some());
    let row = a.or(b).unwrap();
    assert!(!db.enable_schedule(guild, id, true).await.unwrap());
    assert!(!db.remove_schedule(guild, id).await.unwrap());
    assert!(
        !db.update_schedule(&ScheduledRow { id, ..schedule(guild) })
            .await
            .unwrap()
    );
    assert!(db.begin_schedule(&row).await.unwrap());
    assert!(!db.begin_schedule(&row).await.unwrap());
    // Simulate losing the process after Discord accepted the post, before completion persisted.
    sqlx::query("UPDATE scheduled_messages SET lease_until=now()-INTERVAL '1 second' WHERE id=$1")
        .bind(id)
        .execute(db.pool())
        .await
        .unwrap();
    let crashed = db.schedule(guild, id).await.unwrap().unwrap();
    assert!(!crashed.enabled);
    assert_eq!(crashed.pause_reason.as_deref(), Some("delivery_uncertain"));
    let all = db.claim_schedules(10000).await.unwrap();
    assert!(all.iter().all(|r| r.id != id));
    for other in all {
        db.release_schedule(&other).await.unwrap();
    }
    assert!(db.enable_schedule(guild, id, true).await.unwrap());
    let new = db.claim_schedule_now(guild, id).await.unwrap().unwrap();
    db.release_schedule(&row).await.unwrap();
    assert_eq!(
        db.schedule(guild, id).await.unwrap().unwrap().lease_until,
        new.lease_until
    );
    assert!(!db.begin_schedule(&row).await.unwrap());
    assert!(db.begin_schedule(&new).await.unwrap());
    db.finish_schedule(&row, None).await.unwrap();
    assert_eq!(db.schedule(guild, id).await.unwrap().unwrap().runs, 0);
    db.finish_schedule(&new, Some(Utc::now() + Span::minutes(1)))
        .await
        .unwrap();
    assert_eq!(db.schedule(guild, id).await.unwrap().unwrap().runs, 1);
    db.remove_schedule(guild, id).await.unwrap();
}

#[tokio::test]
async fn overdue_recurrence_coalesces_and_guild_limit_is_atomic() {
    let _isolation = crate::support::shared().await;

    let Some(db) = db(5).await else { return };
    let guild = unique();
    for _ in 0..19 {
        db.add_schedule(&schedule(guild)).await.unwrap().unwrap();
    }
    let draft = schedule(guild);
    let (a, b) = tokio::join!(db.add_schedule(&draft), db.add_schedule(&draft));
    assert_ne!(a.unwrap().is_some(), b.unwrap().is_some());
    let rows = db.schedules(guild).await.unwrap();
    assert_eq!(rows.len(), 20);
    let row = db.claim_schedule_now(guild, rows[0].id).await.unwrap().unwrap();
    assert!(db.begin_schedule(&row).await.unwrap());
    let now = timestamp(Utc::now());
    db.finish_schedule(&row, next_run(&Schedule::Interval { seconds: 60 }, &row.tz, now))
        .await
        .unwrap();
    let saved = db.schedule(guild, row.id).await.unwrap().unwrap();
    assert_eq!(saved.runs, 1);
    assert_eq!(saved.next_run_at, now + Span::minutes(1));
    for row in rows {
        db.remove_schedule(guild, row.id).await.unwrap();
    }
}

#[tokio::test]
async fn recurrence_with_nanoseconds_round_trips_at_postgres_precision() {
    let _isolation = crate::support::shared().await;

    let Some(db) = db(5).await else { return };
    let guild = unique();
    let id = db.add_schedule(&schedule(guild)).await.unwrap().unwrap();
    let row = db.claim_schedule_now(guild, id).await.unwrap().unwrap();
    assert!(db.begin_schedule(&row).await.unwrap());
    // Explicit nanoseconds reproduce a Linux clock on macOS as well.
    let now = chrono::DateTime::from_timestamp(1_800_000_000, 308_657_117).unwrap();
    let next = next_run(&Schedule::Interval { seconds: 60 }, &row.tz, now).unwrap();
    assert_eq!(next, now + Span::minutes(1));
    db.finish_schedule(&row, Some(next)).await.unwrap();
    let saved = db.schedule(guild, id).await.unwrap().unwrap();
    assert_eq!(saved.next_run_at, timestamp(now) + Span::minutes(1));
    assert_eq!(next - saved.next_run_at, Span::nanoseconds(117));
    assert_eq!(saved.runs, 1);
    assert!(saved.enabled);
    assert_eq!(saved.lease_until, None);
    db.remove_schedule(guild, id).await.unwrap();
}

#[tokio::test]
async fn sticky_cleanup_is_atomic_and_delivery_is_fenced() {
    let _isolation = crate::support::shared().await;
    let Some(db) = db(5).await else { return };
    let guild = unique();
    db.upsert_sticky(&sticky(guild)).await.unwrap();
    sqlx::query("UPDATE sticky_messages SET updated_at=now()-INTERVAL '10 seconds',message_count=2,lease_until=now()+INTERVAL '45 seconds',last_message_id=123 WHERE channel_id=$1")
        .bind(guild as i64).execute(db.pool()).await.unwrap();
    let row = db.stickies(guild).await.unwrap().remove(0);
    assert!(db.begin_sticky(&row).await.unwrap());
    assert!(!db.begin_sticky(&row).await.unwrap());
    assert!(db.remove_sticky(guild, guild).await.unwrap().is_none());
    db.finish_sticky(&row, 456).await.unwrap();
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sticky_copy_cleanup WHERE channel_id=$1 AND message_id=123)")
            .bind(guild as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(exists);
    db.remove_sticky(guild, guild).await.unwrap().unwrap();
    let copies: Vec<i64> =
        sqlx::query_scalar("SELECT message_id FROM sticky_copy_cleanup WHERE channel_id=$1 ORDER BY message_id")
            .bind(guild as i64)
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert_eq!(copies, vec![123, 456]);
    for copy in copies {
        db.finish_sticky_cleanup(guild, copy as u64).await.unwrap();
    }
}

#[tokio::test]
async fn privacy_removes_afk_and_anonymises_creators_and_nickname_repeated_set() {
    let _isolation = crate::support::shared().await;

    let Some(db) = db(5).await else { return };
    let user = unique();
    db.set_afk(user, user, "lunch", true).await.unwrap();
    db.set_afk(user, user, "back soon", false).await.unwrap();
    db.set_afk(0, user, "away globally", false).await.unwrap();
    assert!(db.afk_for(user, &[user as i64]).await.unwrap()[0].nick_prefixed);
    let id = db.add_schedule(&schedule(user)).await.unwrap().unwrap();
    db.upsert_sticky(&sticky(user)).await.unwrap();
    db.delete_user_data(user, Utc::now()).await.unwrap();
    assert!(db.afk_for(user, &[user as i64]).await.unwrap().is_empty());
    assert_eq!(db.schedule(user, id).await.unwrap().unwrap().created_by, None);
    assert_eq!(db.stickies(user).await.unwrap()[0].updated_by, None);
    db.remove_schedule(user, id).await.unwrap();
    db.remove_sticky(user, user).await.unwrap();
}
