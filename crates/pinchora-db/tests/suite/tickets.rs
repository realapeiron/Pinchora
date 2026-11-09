//! Ticket transactions against the dedicated test PostgreSQL.

use std::time::Duration;

use chrono::Utc;
use pinchora_db::{Db, NewTicket, TicketOpen};

#[tokio::test]
async fn reopening_waits_for_transcript_author_lock() {
    let _isolation = crate::support::shared().await;
    let Ok(url) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect_waiting(&url, 8, Duration::from_secs(60)).await.unwrap();
    db.migrate().await.unwrap();
    let base = crate::support::id();
    let (guild, opener, author) = (base, base + 1, base + 2);
    let TicketOpen::Opened(ticket) = db
        .open_ticket(&NewTicket {
            guild_id: guild,
            panel_id: None,
            opener,
            topic: "Help",
            answers: &[],
            max_open: 1,
            at: Utc::now(),
        })
        .await
        .unwrap()
    else {
        panic!("ticket opens")
    };
    db.close_ticket(ticket.id, None, None, Utc::now(), None).await.unwrap();
    db.save_transcript(ticket.id, &[], &[(author, "Writer".into(), false)], true, None)
        .await
        .unwrap();

    let mut held = db.pool().begin().await.unwrap();
    sqlx::query("SELECT lock_user($1)")
        .bind(author as i64)
        .execute(&mut *held)
        .await
        .unwrap();
    let mut reopen = Box::pin(db.reopen_ticket(ticket.id, Utc::now()));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut reopen)
            .await
            .is_err()
    );
    held.commit().await.unwrap();
    assert!(reopen.await.unwrap().is_some());
    assert!(db.transcript(ticket.id).await.unwrap().is_none());
}
