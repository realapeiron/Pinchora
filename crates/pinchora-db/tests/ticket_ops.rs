//! Ticket v2 operation state against the assigned test PostgreSQL.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use pinchora_db::{Db, NewTicket, TicketOpen, TicketPanelOperations};

#[tokio::test]
async fn limits_claim_close_and_private_notes_are_atomic() {
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect_waiting(&url, 8, Duration::from_secs(60)).await.unwrap();
    db.migrate().await.unwrap();
    let base = ((SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64) >> 4) as i64;
    let guild = base as u64;
    let panel: i64 = sqlx::query_scalar(
        "INSERT INTO ticket_panels(guild_id,name,channel_id,title)
        VALUES($1,$2,$3,'Support') RETURNING id",
    )
    .bind(base)
    .bind(format!("v2-{base}"))
    .bind(base + 1)
    .fetch_one(db.pool())
    .await
    .unwrap();
    let ops = TicketPanelOperations {
        max_open_per_panel: 1,
        staff_open_limit: 1,
        claim_mode: "exclusive".into(),
        ..Default::default()
    };
    db.save_ticket_panel_operations(panel, &ops).await.unwrap();
    let open = |opener| NewTicket {
        guild_id: guild,
        panel_id: Some(panel),
        opener,
        topic: "Help",
        answers: &[],
        max_open: 10,
        at: Utc::now(),
    };
    let TicketOpen::Opened(ticket) = db.open_ticket(&open(guild + 10)).await.unwrap() else {
        panic!("first opens")
    };
    assert!(matches!(
        db.open_ticket(&open(guild + 11)).await.unwrap(),
        TicketOpen::TooMany { .. }
    ));
    let (first, second) = tokio::join!(
        db.claim_ticket(ticket.id, guild + 20),
        db.claim_ticket(ticket.id, guild + 21)
    );
    assert_eq!(usize::from(first.unwrap()) + usize::from(second.unwrap()), 1);
    let winner = db.ticket(ticket.id).await.unwrap().unwrap().claimed_by.unwrap() as u64;
    assert!(!db.unclaim_ticket_by(ticket.id, winner + 1).await.unwrap());
    assert!(db.unclaim_ticket_by(ticket.id, winner).await.unwrap());
    assert!(db.request_ticket_close(ticket.id, winner, 24).await.unwrap());
    assert!(!db.request_ticket_close(ticket.id, winner, 24).await.unwrap());
    db.add_ticket_staff_note(ticket.id, winner, "internal only")
        .await
        .unwrap();
    assert_eq!(db.ticket_staff_notes(ticket.id).await.unwrap()[0].body, "internal only");
    let (first, second) = tokio::join!(
        db.close_ticket(ticket.id, Some(winner), None, Utc::now(), None),
        db.close_ticket(ticket.id, Some(winner), None, Utc::now(), None)
    );
    assert_eq!(
        usize::from(first.unwrap().is_some()) + usize::from(second.unwrap().is_some()),
        1
    );
    db.save_transcript(ticket.id, &[], &[], true, None).await.unwrap();
    assert!(db.transcript(ticket.id).await.unwrap().unwrap().lines.is_empty());
    db.reopen_ticket(ticket.id, Utc::now()).await.unwrap().unwrap();
    assert!(
        db.ticket_operations(ticket.id)
            .await
            .unwrap()
            .close_request_expires_at
            .is_none()
    );
    assert!(db.ticket_staff_notes(ticket.id).await.unwrap().len() == 1);
}

async fn operation_fixture() -> Option<(Db, u64, i64)> {
    let url = std::env::var("PINCHORA_TEST_DATABASE_URL").ok()?;
    let db = Db::connect_waiting(&url, 8, Duration::from_secs(60)).await.unwrap();
    db.migrate().await.unwrap();
    let guild = (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() >> 4) as u64;
    let panel = sqlx::query_scalar(
        "INSERT INTO ticket_panels(guild_id,name,channel_id,title,open_category_id)
        VALUES($1,'Operations',$1,'Support',$1) RETURNING id",
    )
    .bind(guild as i64)
    .fetch_one(db.pool())
    .await
    .unwrap();
    Some((db, guild, panel))
}

#[tokio::test]
async fn cooldown_survives_cache_loss_and_parallel_open_attempts() {
    let Some((db, guild, panel)) = operation_fixture().await else {
        return;
    };
    let request = NewTicket {
        guild_id: guild,
        panel_id: Some(panel),
        opener: guild + 1,
        topic: "Support",
        answers: &[],
        max_open: 10,
        at: Utc::now(),
    };
    let (a, b) = tokio::join!(db.open_ticket(&request), db.open_ticket(&request));
    let results = [a.unwrap(), b.unwrap()];
    assert_eq!(results.iter().filter(|r| matches!(r, TicketOpen::Opened(_))).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, TicketOpen::Cooldown { .. }))
            .count(),
        1
    );
    let ticket = results
        .iter()
        .find_map(|r| match r {
            TicketOpen::Opened(t) => Some(t.id),
            _ => None,
        })
        .unwrap();
    db.close_ticket(ticket, None, None, Utc::now(), None).await.unwrap();
    assert!(matches!(
        db.open_ticket(&request).await.unwrap(),
        TicketOpen::Cooldown { .. }
    ));
    let later = NewTicket {
        at: request.at + chrono::Duration::seconds(61),
        ..request
    };
    assert!(matches!(db.open_ticket(&later).await.unwrap(), TicketOpen::Opened(_)));
}

#[tokio::test]
async fn parallel_panel_and_category_limits_have_one_winner() {
    let Some((db, guild, panel)) = operation_fixture().await else {
        return;
    };
    for category in [false, true] {
        db.save_ticket_panel_operations(
            panel,
            &TicketPanelOperations {
                max_open_per_panel: if category { 0 } else { 1 },
                max_open_per_category: if category { 1 } else { 0 },
                cooldown_seconds: 0,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let a = NewTicket {
            guild_id: guild,
            panel_id: Some(panel),
            opener: guild + 1,
            topic: "Support",
            answers: &[],
            max_open: 10,
            at: Utc::now(),
        };
        let b = NewTicket {
            opener: guild + 2,
            ..a.clone()
        };
        let (a, b) = tokio::join!(db.open_ticket(&a), db.open_ticket(&b));
        let results = [a.unwrap(), b.unwrap()];
        assert_eq!(results.iter().filter(|r| matches!(r, TicketOpen::Opened(_))).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|r| matches!(r, TicketOpen::TooMany { .. }))
                .count(),
            1
        );
        for result in results {
            if let TicketOpen::Opened(t) = result {
                db.close_ticket(t.id, None, None, Utc::now(), None).await.unwrap();
            }
        }
    }
}

#[tokio::test]
async fn form_pages_are_user_bound_expiring_size_capped_and_single_use() {
    let Some((db, guild, panel)) = operation_fixture().await else {
        return;
    };
    let user = guild + 1;
    let answers = serde_json::json!([{"question":"Name", "answer":"Ada"}]);
    db.save_ticket_form_draft(guild, user, panel, 0, &serde_json::json!([]), 0)
        .await
        .unwrap();
    assert!(db.ticket_form_draft(guild, user + 1, panel, 0).await.unwrap().is_none());
    let (a, b) = tokio::join!(
        db.advance_ticket_form_draft(guild, user, panel, 0, &answers, 0),
        db.advance_ticket_form_draft(guild, user, panel, 0, &answers, 0)
    );
    assert_eq!(usize::from(a.unwrap()) + usize::from(b.unwrap()), 1);
    assert_eq!(
        db.ticket_form_draft(guild, user, panel, 0).await.unwrap().unwrap(),
        (answers.clone(), 1)
    );
    let oversized = serde_json::json!([{"question":"Name", "answer":"x".repeat(1001)}]);
    assert!(
        db.advance_ticket_form_draft(guild, user, panel, 0, &oversized, 1)
            .await
            .is_err()
    );
    sqlx::query("UPDATE ticket_form_drafts SET expires_at=now()-interval '1 second' WHERE panel_id=$1")
        .bind(panel)
        .execute(db.pool())
        .await
        .unwrap();
    assert!(db.ticket_form_draft(guild, user, panel, 0).await.unwrap().is_none());
    assert!(
        !db.advance_ticket_form_draft(guild, user, panel, 0, &answers, 1)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn close_request_deadline_is_persistent_and_private_data_is_deleted() {
    let Some((db, guild, panel)) = operation_fixture().await else {
        return;
    };
    let user = guild + 1;
    let TicketOpen::Opened(ticket) = db
        .open_ticket(&NewTicket {
            guild_id: guild,
            panel_id: Some(panel),
            opener: user,
            topic: "Support",
            answers: &[],
            max_open: 10,
            at: Utc::now(),
        })
        .await
        .unwrap()
    else {
        panic!("opens")
    };
    db.request_ticket_close(ticket.id, guild + 2, 1).await.unwrap();
    assert!(
        db.due_ticket_close_requests(Utc::now() + chrono::Duration::hours(2), 10000)
            .await
            .unwrap()
            .contains(&ticket.id)
    );
    db.add_ticket_staff_note(ticket.id, guild + 2, "secret").await.unwrap();
    db.save_ticket_form_draft(guild, user, panel, 0, &serde_json::json!([]), 0)
        .await
        .unwrap();
    db.delete_user_data(user, Utc::now()).await.unwrap();
    assert!(db.ticket_staff_notes(ticket.id).await.unwrap().is_empty());
    assert!(db.ticket_form_draft(guild, user, panel, 0).await.unwrap().is_none());
}

#[tokio::test]
async fn timeout_close_rechecks_requests_and_competes_with_manual_close() {
    let Some((db, guild, panel)) = operation_fixture().await else {
        return;
    };
    let TicketOpen::Opened(ticket) = db
        .open_ticket(&NewTicket {
            guild_id: guild,
            panel_id: Some(panel),
            opener: guild + 1,
            topic: "Support",
            answers: &[],
            max_open: 1,
            at: Utc::now(),
        })
        .await
        .unwrap()
    else {
        panic!("opens")
    };
    let later = Utc::now() + chrono::Duration::hours(2);
    db.request_ticket_close(ticket.id, guild + 2, 1).await.unwrap();
    db.cancel_ticket_close_request(ticket.id).await.unwrap();
    assert!(
        db.close_requested_ticket(ticket.id, "timeout", later)
            .await
            .unwrap()
            .is_none()
    );
    db.request_ticket_close(ticket.id, guild + 2, 1).await.unwrap();
    let (manual, timeout) = tokio::join!(
        db.close_ticket(ticket.id, Some(guild + 2), None, later, None),
        db.close_requested_ticket(ticket.id, "timeout", later)
    );
    assert_eq!(
        usize::from(manual.unwrap().is_some()) + usize::from(timeout.unwrap().is_some()),
        1
    );
    db.reopen_ticket(ticket.id, later).await.unwrap().unwrap();
    assert!(
        db.close_requested_ticket(ticket.id, "stale timeout", later)
            .await
            .unwrap()
            .is_none()
    );
    assert!(db.ticket(ticket.id).await.unwrap().unwrap().is_open());
}

#[tokio::test]
async fn force_open_attribution_belongs_to_the_single_reopen_winner() {
    let Some((db, guild, panel)) = operation_fixture().await else {
        return;
    };
    let TicketOpen::Opened(ticket) = db
        .open_ticket(&NewTicket {
            guild_id: guild,
            panel_id: Some(panel),
            opener: guild + 1,
            topic: "Support",
            answers: &[],
            max_open: 1,
            at: Utc::now(),
        })
        .await
        .unwrap()
    else {
        panic!("opens")
    };
    db.close_ticket(ticket.id, None, None, Utc::now(), None).await.unwrap();
    let (a, b) = tokio::join!(
        db.force_reopen_ticket(ticket.id, Utc::now(), guild + 2),
        db.force_reopen_ticket(ticket.id, Utc::now(), guild + 3)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(usize::from(a.is_some()) + usize::from(b.is_some()), 1);
    let winner = if a.is_some() { guild + 2 } else { guild + 3 };
    assert_eq!(
        db.ticket_operations(ticket.id).await.unwrap().force_opened_by,
        Some(winner as i64)
    );
}

#[tokio::test]
async fn settings_edits_preserve_concurrently_created_board_ids() {
    let Some((db, guild, _)) = operation_fixture().await else {
        return;
    };
    let mut stale = pinchora_db::TicketOperationSettings {
        status_channel_id: Some((guild + 1) as i64),
        ..Default::default()
    };
    db.save_ticket_operation_settings(guild, &stale).await.unwrap();
    assert!(db.set_ticket_status_message(guild, guild + 1, guild + 2).await.unwrap());
    stale.out_of_hours_message = "Back tomorrow".into();
    db.save_ticket_operation_settings(guild, &stale).await.unwrap();
    assert_eq!(
        db.ticket_operation_settings(guild).await.unwrap().status_message_id,
        Some((guild + 2) as i64)
    );
    stale.status_channel_id = Some((guild + 3) as i64);
    db.save_ticket_operation_settings(guild, &stale).await.unwrap();
    assert!(
        db.ticket_operation_settings(guild)
            .await
            .unwrap()
            .status_message_id
            .is_none()
    );
    assert!(!db.set_ticket_status_message(guild, guild + 1, guild + 4).await.unwrap());
    db.save_ticket_operation_settings(guild, &Default::default())
        .await
        .unwrap();
}
