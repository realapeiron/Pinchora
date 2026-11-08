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
