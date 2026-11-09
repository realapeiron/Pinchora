use chrono::{Duration, Utc};
use pinchora_core::community::events::Recurrence;
use pinchora_core::community::events::Repeat;
use pinchora_core::community::events::default_options;
use pinchora_db::{Db, EventDraft, ReminderRow, RsvpResult};

async fn setup() -> (Db, u64, u64, EventDraft) {
    let db = Db::connect_waiting(
        &std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap(),
        6,
        std::time::Duration::from_secs(5),
    )
    .await
    .unwrap();
    db.migrate().await.unwrap();
    let guild = crate::support::id();
    let start = Utc::now() + Duration::hours(3);
    let mut options = default_options();
    options[0].capacity = 1;
    (
        db,
        guild,
        guild + 1,
        EventDraft {
            title: "@everyone <@123>".into(),
            description: "<@&456>".into(),
            starts_at: start,
            ends_at: start + Duration::hours(1),
            tz: "Europe/Istanbul".into(),
            location: String::new(),
            channel_id: None,
            host_id: Some((guild + 1) as i64),
            options,
            required_role_id: None,
            reminders: vec![60, 15],
            recurrence: None,
        },
    )
}
async fn queued(db: &Db, id: i64, user: u64) -> ReminderRow {
    sqlx::query_as("SELECT id,user_id,kind,ref_id,body,language,due_at FROM reminders WHERE ref_id=$1 AND user_id=$2 AND kind='event_60'")
        .bind(id).bind(user as i64).fetch_one(db.pool()).await.unwrap()
}
#[tokio::test]
async fn capacity_race_promotion_role_scope_and_elapsed_deadline() {
    let _isolation = crate::support::shared().await;
    let (db, g, u, mut d) = setup().await;
    let row = db.create_event(g, &d).await.unwrap();
    let (first, second) = tokio::join!(
        db.rsvp_event(g, row.id, u + 50, "going"),
        db.rsvp_event(g, row.id, u + 50, "going")
    );
    assert!(matches!(
        (first.unwrap(), second.unwrap()),
        (RsvpResult::Confirmed, RsvpResult::Removed) | (RsvpResult::Removed, RsvpResult::Confirmed)
    ));
    assert_eq!(
        db.event_rsvp_count(row.id).await.unwrap(),
        0,
        "a double toggle leaves no phantom seat or queue entry"
    );
    assert!(
        db.scheduled_reminder(u + 50, "event_60", row.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!row.title.contains('@') && !row.description.contains('@'));
    let (a, b) = tokio::join!(
        db.rsvp_event(g, row.id, u, "going"),
        db.rsvp_event(g, row.id, u + 1, "going")
    );
    assert!(matches!(
        (a.unwrap(), b.unwrap()),
        (RsvpResult::Confirmed, RsvpResult::Waitlisted) | (RsvpResult::Waitlisted, RsvpResult::Confirmed)
    ));
    let people = db.event_rsvps(row.id, 0, 25).await.unwrap();
    let first = people.iter().find(|p| !p.waitlisted).unwrap().user_id as u64;
    let waiting = people.iter().find(|p| p.waitlisted).unwrap().user_id as u64;
    let (a, b) = tokio::join!(
        db.rsvp_event(g, row.id, first, "declined"),
        db.rsvp_event(g, row.id, u + 2, "going")
    );
    a.unwrap();
    b.unwrap();
    let counts = db.event_counts(row.id).await.unwrap();
    assert!(counts.contains(&("going".into(), 1)));
    assert!(
        !db.event_rsvps(row.id, 0, 25)
            .await
            .unwrap()
            .iter()
            .find(|p| p.user_id == waiting as i64)
            .unwrap()
            .waitlisted
    );
    assert!(
        db.scheduled_reminder(waiting, "event_60", row.id)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        db.rsvp_event(g + 100, row.id, u + 3, "going").await.unwrap(),
        RsvpResult::Closed
    );
    d.required_role_id = Some(42);
    db.update_event(g, row.id, &d).await.unwrap();
    assert_eq!(
        db.rsvp_event_with_roles(g, row.id, u + 3, "going", &[]).await.unwrap(),
        RsvpResult::RequiredRole
    );
    assert_eq!(
        db.rsvp_event_with_roles(g, row.id, u + 3, "going", &[42])
            .await
            .unwrap(),
        RsvpResult::Waitlisted
    );
    sqlx::query("UPDATE events SET starts_at=now()-interval '2 hours', ends_at=now()-interval '1 hour' WHERE id=$1")
        .bind(row.id)
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(
        db.rsvp_event(g, row.id, u + 4, "going").await.unwrap(),
        RsvpResult::Closed
    );
    db.cancel_event(g, row.id).await.unwrap();
    assert_eq!(
        db.rsvp_event(g, row.id, u + 4, "going").await.unwrap(),
        RsvpResult::Closed
    );
}
#[tokio::test]
async fn reminders_survive_replays_edits_privacy_and_restarts_without_duplicates() {
    let _isolation = crate::support::shared().await;
    let (db, g, u, mut d) = setup().await;
    let row = db.create_event(g, &d).await.unwrap();
    db.rsvp_event(g, row.id, u, "going").await.unwrap();
    let original = queued(&db, row.id, u).await;
    sqlx::query("UPDATE reminders SET lease_until=now()+interval '2 minutes' WHERE id=$1")
        .bind(original.id)
        .execute(db.pool())
        .await
        .unwrap();
    db.reconcile_event_reminders(row.id).await.unwrap();
    let (leased,): (bool,) = sqlx::query_as("SELECT lease_until IS NOT NULL FROM reminders WHERE id=$1")
        .bind(original.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(leased);
    d.starts_at += Duration::hours(1);
    d.ends_at += Duration::hours(1);
    db.update_event(g, row.id, &d).await.unwrap();
    assert!(
        !db.claim_event_delivery(&original, 60).await.unwrap(),
        "stale in-flight queue entry rejected"
    );
    let changed = queued(&db, row.id, u).await;
    let (a, b) = tokio::join!(
        db.claim_event_delivery(&changed, 60),
        db.claim_event_delivery(&changed, 60)
    );
    assert_ne!(a.unwrap(), b.unwrap());
    db.finish_reminder(&changed).await.unwrap();
    db.reconcile_event_reminders(row.id).await.unwrap();
    assert!(db.scheduled_reminder(u, "event_60", row.id).await.unwrap().is_none());
    // Replay through a new pool, after a simulated pod restart.
    let other = Db::connect_waiting(
        &std::env::var("PINCHORA_TEST_DATABASE_URL").unwrap(),
        2,
        std::time::Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert!(!other.claim_event_delivery(&changed, 60).await.unwrap());
    db.set_dm_opt_out(u, true).await.unwrap();
    db.reconcile_event_reminders(row.id).await.unwrap();
    assert!(db.scheduled_reminder(u, "event_15", row.id).await.unwrap().is_none());
    db.cancel_event(g, row.id).await.unwrap();
    db.reconcile_event_reminders(row.id).await.unwrap();
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM reminders WHERE ref_id=$1 AND kind LIKE 'event_%'")
        .bind(row.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
}
#[tokio::test]
async fn bulk_reminders_capacity_edit_and_recurrence_are_serialized() {
    let _isolation = crate::support::exclusive().await;

    let (db, g, u, mut d) = setup().await;
    let row = db.create_event(g, &d).await.unwrap();
    for n in 0..3 {
        db.rsvp_event(g, row.id, u + n, "going").await.unwrap();
    }
    d.options[0].capacity = 3;
    db.update_event(g, row.id, &d).await.unwrap();
    assert!(
        db.event_rsvps(row.id, 0, 25)
            .await
            .unwrap()
            .iter()
            .all(|r| !r.waitlisted)
    );
    sqlx::query(
        "INSERT INTO event_rsvps(event_id,user_id,option) SELECT $1,$2+n,'maybe' FROM generate_series(10,1009) n",
    )
    .bind(row.id)
    .bind(u as i64)
    .execute(db.pool())
    .await
    .unwrap();
    db.reconcile_event_reminders(row.id).await.unwrap();
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM reminders WHERE ref_id=$1 AND kind LIKE 'event_%'")
        .bind(row.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 2006);
    d.recurrence = Some(Recurrence {
        repeat: Repeat::Daily,
        until: (Utc::now() + Duration::days(4)).date_naive(),
    });
    db.update_event(g, row.id, &d).await.unwrap();
    sqlx::query("UPDATE events SET starts_at=now()-interval '2 hours',ends_at=now()-interval '1 hour' WHERE id=$1")
        .bind(row.id)
        .execute(db.pool())
        .await
        .unwrap();
    let now = Utc::now();
    let (a, b) = tokio::join!(db.advance_events(now), db.advance_events(now));
    assert_eq!(
        a.unwrap()
            .iter()
            .chain(b.unwrap().iter())
            .filter(|r| r.parent_id == Some(row.id))
            .count(),
        1
    );
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM reminders WHERE ref_id=$1 AND kind LIKE 'event_%'")
        .bind(row.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn privacy_deletes_rsvp_delivery_history_and_anonymises_host() {
    let _isolation = crate::support::shared().await;
    let (db, g, u, d) = setup().await;
    db.remember_name(u, "Event host").await.unwrap();
    let row = db.create_event(g, &d).await.unwrap();
    db.rsvp_event(g, row.id, u, "going").await.unwrap();
    let reminder = queued(&db, row.id, u).await;
    assert!(db.claim_event_delivery(&reminder, 60).await.unwrap());
    db.delete_user_data(u, Utc::now()).await.unwrap();
    assert!(db.event(g, row.id).await.unwrap().unwrap().host_id.is_none());
    assert_eq!(db.event_rsvp_count(row.id).await.unwrap(), 0);
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM event_reminder_deliveries WHERE user_id=$1")
        .bind(u as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn discord_terminal_updates_drop_queue_and_materialise_recurrence_once() {
    let _isolation = crate::support::exclusive().await;

    let (db, g, u, mut d) = setup().await;
    let cancelled = db.create_event(g, &d).await.unwrap();
    db.set_discord_event(cancelled.id, Some(g)).await.unwrap();
    db.rsvp_event(g, cancelled.id, u, "going").await.unwrap();
    assert!(
        db.update_scheduled_event_status(g + 1, g, "cancelled")
            .await
            .unwrap()
            .is_none()
    );
    db.update_scheduled_event_status(g, g, "cancelled").await.unwrap();
    assert!(
        db.scheduled_reminder(u, "event_60", cancelled.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        db.update_scheduled_event_status(g, g, "cancelled")
            .await
            .unwrap()
            .is_none()
    );
    d.recurrence = Some(Recurrence {
        repeat: Repeat::Daily,
        until: (Utc::now() + Duration::days(3)).date_naive(),
    });
    let row = db.create_event(g, &d).await.unwrap();
    db.set_discord_event(row.id, Some(g + 2)).await.unwrap();
    // Discord can complete the occurrence early: close RSVP even before its planned start.
    db.update_scheduled_event_status(g, g + 2, "ended").await.unwrap();
    assert_eq!(db.rsvp_event(g, row.id, u, "going").await.unwrap(), RsvpResult::Closed);
    assert_eq!(db.event(g, row.id).await.unwrap().unwrap().status, "ended");
    let now = Utc::now();
    let (a, b) = tokio::join!(db.advance_events(now), db.advance_events(now));
    assert_eq!(
        a.unwrap()
            .iter()
            .chain(b.unwrap().iter())
            .filter(|r| r.parent_id == Some(row.id))
            .count(),
        1
    );
    assert!(
        db.update_scheduled_event_status(g, g + 2, "ended")
            .await
            .unwrap()
            .is_none()
    );
}
