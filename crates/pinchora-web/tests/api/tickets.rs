//! Tickets over the API: settings and panels saved with the `/setup tickets` rules, a panel posted by the bot,
//! the tickets list with its filters, one ticket with its transcript, and the transcript page.
//! `seed_demo_tickets` (ignored) leaves tickets for the Playwright run to look at.

use chrono::{Duration as Span, Utc};
use pinchora_core::tickets::{Answer, Line};
use pinchora_db::{Db, NewTicket, TicketOpen};

use super::*;

fn t(guild: u64) -> String {
    format!("/api/guilds/{guild}/tickets")
}

fn panel(name: &str) -> Value {
    json!({
        "name": name,
        "channel_id": GENERAL.to_string(),
        "title": "Need help?",
        "description": "Press a button.",
        "color": "#06d6a0",
        "style": "buttons",
        "topics": [{ "label": "Support", "emoji": "🎫", "tone": "primary" }, { "label": "Billing", "tone": "success" }],
        "questions": [{ "label": "Your order number", "placeholder": "#1234", "long": false, "required": true }],
        "support_roles": [FAN.to_string()],
        "welcome_text": "Hi {user}!",
    })
}

/// A member's ticket opened on `panel`, then (when `close`) answered by `staff` and closed with a transcript
/// that holds hostile text, as the bot would leave it.
async fn seed_ticket(
    db: &Db,
    guild: u64,
    panel: Option<i64>,
    opener: (u64, &str),
    staff: u64,
    topic: &str,
    close: bool,
) -> i64 {
    let (opener, opener_name) = opener;
    let opened = db
        .open_ticket(&NewTicket {
            guild_id: guild,
            panel_id: panel,
            opener,
            topic,
            answers: &[Answer {
                question: "Your order number".into(),
                answer: "#4242".into(),
            }],
            max_open: 10,
            at: Utc::now() - Span::hours(3),
        })
        .await
        .expect("open");
    let TicketOpen::Opened(ticket) = opened else {
        panic!("the ticket opens")
    };
    db.set_ticket_place(ticket.id, unique(), None).await.expect("place");
    if !close {
        return ticket.id;
    }
    db.claim_ticket(ticket.id, staff).await.expect("claim");
    let at = |minutes: i64| ticket.opened_at + Span::minutes(minutes);
    let lines = vec![
        Line {
            a: 0,
            t: at(0),
            c: Some(String::new()),
            f: vec![],
            e: 1,
        },
        Line {
            a: 1,
            t: at(1),
            c: Some("<script>alert('x')</script> my order never came".into()),
            f: vec!["receipt.png".into()],
            e: 0,
        },
        Line {
            a: 2,
            t: at(14),
            c: Some(format!("Sorry about that! A new one is on its way. <@{staff}>")),
            f: vec![],
            e: 0,
        },
        Line {
            a: 1,
            t: at(16),
            c: Some("Thank you!!".into()),
            f: vec![],
            e: 0,
        },
    ];
    let authors = vec![
        (APPLICATION_ID, "Pinchora".to_owned(), true),
        (opener, opener_name.to_owned(), false),
        (staff, "Kai".to_owned(), false),
    ];
    db.close_ticket(ticket.id, Some(staff), Some("Solved: replacement sent"), at(20), None)
        .await
        .expect("close")
        .expect("closes once");
    db.save_transcript(ticket.id, &lines, &authors, true, Some(at(14)))
        .await
        .expect("transcript");
    db.rate_ticket(ticket.id, opener, 5).await.expect("rate");
    ticket.id
}

#[tokio::test]
async fn ticket_settings_and_panels_save_with_the_command_rules() {
    let Some(h) = Harness::new("tickets_settings_panels").await else {
        return;
    };
    let (guild, _owner, mut browser) = h.managed_server().await;

    let empty = browser.get(&t(guild)).await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.text());
    let body = empty.json();
    assert_eq!(body["settings"]["naming"], "ticket-{number}");
    assert_eq!(body["settings"]["mode"], "auto");
    assert_eq!(body["panels"], json!([]));
    assert_eq!(body["limits"]["buttons"], 5);
    assert_eq!(body["summary"]["week"]["opened"], 0);

    let settings = |open: u64, naming: &str, hours: i64| {
        json!({
            "open_category_id": open.to_string(),
            "closed_category_id": null,
            "log_channel_id": GENERAL.to_string(),
            "auto_close_hours": hours,
            "dm_transcript": true,
            "rating_enabled": true,
            "naming": naming,
            "mode": "channel",
            "max_open_per_user": 2,
        })
    };
    let refused = browser
        .write(
            Method::PUT,
            &format!("{}/settings", t(guild)),
            settings(GENERAL, "!!!", 999),
        )
        .await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", refused.text());
    let mut fields = fields_of(&refused);
    fields.sort();
    assert_eq!(fields, ["auto_close_hours", "naming", "open_category_id"]);
    let saved = browser
        .write(
            Method::PUT,
            &format!("{}/settings", t(guild)),
            settings(CATEGORY, "{user}-{number}", 48),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let stored = h.state.app.db.ticket_settings(guild).await.unwrap();
    assert_eq!(stored.open_category_id, Some(CATEGORY as i64));
    assert_eq!((stored.auto_close_hours, stored.max_open_per_user), (48, 2));
    assert!(stored.rating_enabled);

    // A panel the bot could not show is refused field by field, in the command's words.
    let mut bad = panel("12");
    bad["topics"] = json!([]);
    bad["support_roles"] = json!([guild.to_string()]);
    bad["channel_id"] = json!(LOUNGE.to_string());
    let refused = browser.write(Method::POST, &format!("{}/panels", t(guild)), bad).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", refused.text());
    let mut fields = fields_of(&refused);
    fields.sort();
    assert_eq!(fields, ["buttons", "channel_id", "name", "support_roles"]);

    let made = browser
        .write(Method::POST, &format!("{}/panels", t(guild)), panel("Help desk"))
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.text());
    let id = made.json()["id"].as_str().unwrap().to_owned();
    assert_eq!(made.json()["questions"][0]["label"], "Your order number");
    assert_eq!(made.json()["color"], "#06d6a0");
    let twice = browser
        .write(Method::POST, &format!("{}/panels", t(guild)), panel("help DESK"))
        .await;
    assert_eq!(fields_of(&twice), ["name"], "names are one per server, any case");

    let mut edit = panel("Help desk");
    edit["style"] = json!("select");
    edit["questions"] = json!([]);
    let edited = browser
        .write(Method::PUT, &format!("{}/panels/{id}", t(guild)), edit)
        .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text());
    assert_eq!(edited.json()["style"], "select");
    assert_eq!(edited.json()["questions"], json!([]), "no questions, no form");

    let posted = browser
        .write(Method::POST, &format!("{}/panels/{id}/post", t(guild)), json!({}))
        .await;
    assert_eq!(posted.status, StatusCode::OK, "{}", posted.text());
    assert_eq!(posted.json()["updated"], false);
    let listed = browser.get(&t(guild)).await.json();
    assert_eq!(listed["panels"][0]["message_id"], posted.json()["message_id"]);

    let audit = browser.get(&format!("/api/guilds/{guild}/audit")).await.json();
    let actions: Vec<&str> = audit["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["action"].as_str())
        .collect();
    for action in [
        "tickets.settings",
        "tickets.panel.create",
        "tickets.panel.update",
        "tickets.panel.post",
    ] {
        assert!(actions.contains(&action), "{action} in {actions:?}");
    }

    let gone = browser
        .write(Method::DELETE, &format!("{}/panels/{id}", t(guild)), json!({}))
        .await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    let again = browser
        .write(Method::DELETE, &format!("{}/panels/{id}", t(guild)), json!({}))
        .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tickets_list_filter_and_show_their_transcript() {
    let Some(h) = Harness::new("tickets_list_transcript").await else {
        return;
    };
    let (guild, _owner, mut browser) = h.managed_server().await;
    let db = &h.state.app.db;
    let (sam, kai, mia) = (unique(), unique(), unique());
    let open = seed_ticket(db, guild, None, (sam, "Sam"), kai, "Billing", false).await;
    let closed = seed_ticket(db, guild, None, (mia, "Mia"), kai, "Order problem", true).await;

    let all = browser.get(&format!("{}/list", t(guild))).await;
    assert_eq!(all.status, StatusCode::OK, "{}", all.text());
    let items = all.json()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], closed.to_string(), "newest first");
    assert_eq!(items[0]["state"], "closed");
    assert_eq!(items[0]["rating"], 5);
    assert_eq!(items[1]["state"], "open");
    for (query, wanted) in [
        ("state=closed".to_owned(), vec![closed]),
        ("state=open".to_owned(), vec![open]),
        ("state=claimed".to_owned(), vec![]),
        ("q=billing".to_owned(), vec![open]),
        (format!("user={mia}"), vec![closed]),
        (format!("staff={kai}"), vec![closed]),
    ] {
        let page = browser.get(&format!("{}/list?{query}", t(guild))).await.json();
        let ids: Vec<String> = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_owned())
            .collect();
        let wanted: Vec<String> = wanted.iter().map(i64::to_string).collect();
        assert_eq!(ids, wanted, "{query}");
    }

    let detail = browser.get(&format!("{}/ticket/{closed}", t(guild))).await;
    assert_eq!(detail.status, StatusCode::OK, "{}", detail.text());
    let detail = detail.json();
    assert_eq!(detail["ticket"]["close_reason"], "Solved: replacement sent");
    assert_eq!(detail["transcript"]["messages"], 4);
    assert_eq!(detail["transcript"]["authors"][1]["name"], "Mia");
    assert!(detail["transcript"]["lines"][2]["c"].as_str().unwrap().contains("@Kai"));
    assert_eq!(detail["ticket"]["answers"][0]["answer"], "#4242");
    let summary = browser.get(&t(guild)).await.json()["summary"]["week"].clone();
    assert_eq!(summary["opened"], 2);
    assert_eq!(summary["closed"], 1);
    assert_eq!(summary["median_first_reply_secs"], 14.0 * 60.0);
    assert_eq!(summary["rating_avg"], 5.0);

    let page = browser
        .get(&format!("{}/ticket/{closed}/transcript.html", t(guild)))
        .await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.header("content-type").starts_with("text/html"));
    assert!(page.header("content-disposition").contains("attachment"));
    let html = page.text();
    assert!(html.contains("&lt;script&gt;") && !html.contains("<script"), "{html}");
    assert!(html.contains("Kai") && html.contains("Mia") && html.contains("receipt.png"));
    assert!(html.contains("@Kai"));

    let closed_from_web = browser
        .write(Method::POST, &format!("{}/ticket/{open}/close", t(guild)), json!({}))
        .await;
    assert_eq!(closed_from_web.status, StatusCode::OK, "{}", closed_from_web.text());
    assert_eq!(closed_from_web.json()["state"], "closed");
    assert_eq!(db.ticket(open).await.unwrap().unwrap().closed_by, Some(_owner as i64));
    let repeated = browser
        .write(Method::POST, &format!("{}/ticket/{open}/close", t(guild)), json!({}))
        .await;
    assert_eq!(repeated.status, StatusCode::CONFLICT);
    let audit = browser.get(&format!("/api/guilds/{guild}/audit")).await.json();
    assert!(
        audit["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["action"] == "tickets.close")
    );

    // Somebody else's server's ticket is not here.
    let (other, _, _) = h.managed_server().await;
    let stranger = seed_ticket(db, other, None, (sam, "Sam"), kai, "Elsewhere", true).await;
    let missing = browser.get(&format!("{}/ticket/{stranger}", t(guild))).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let missing = browser
        .get(&format!("{}/ticket/{stranger}/transcript.html", t(guild)))
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    db.delete_user_data(kai, Utc::now()).await.unwrap();
    let after_delete = browser.get(&format!("{}/ticket/{closed}", t(guild))).await.json();
    assert!(
        after_delete["transcript"]["lines"][2]["c"]
            .as_str()
            .unwrap()
            .contains("@Deleted user")
    );
    let after_html = browser
        .get(&format!("{}/ticket/{closed}/transcript.html", t(guild)))
        .await
        .text();
    assert!(after_html.contains("@Deleted user") && !after_html.contains("@Kai"));
}

/// Leaves tickets on the panel the browser test made, for its screenshots: `TICKETS_SEED_GUILD` names the
/// server and `TICKETS_SEED_PANEL` the panel. Run by `web/tests/e2e/tickets.spec.ts`.
#[tokio::test]
#[ignore = "seeds the test database for the browser tests"]
async fn seed_demo_tickets() {
    let Ok(database) = std::env::var("PINCHORA_TEST_DATABASE_URL") else {
        return;
    };
    let guild: u64 = std::env::var("TICKETS_SEED_GUILD")
        .expect("TICKETS_SEED_GUILD")
        .parse()
        .expect("a server id");
    let name = std::env::var("TICKETS_SEED_PANEL").unwrap_or_else(|_| "Help desk".into());
    let db = Db::connect_waiting(&database, 2, Duration::from_secs(60))
        .await
        .expect("connect");
    db.migrate().await.expect("migrate");
    let panel = db
        .find_ticket_panel(guild, &name)
        .await
        .expect("panels")
        .expect("the browser test made the panel first");
    let (sam, mia, kai, leo) = (
        770_000_000_000_000_101,
        770_000_000_000_000_102,
        770_000_000_000_000_103,
        770_000_000_000_000_104,
    );
    seed_ticket(&db, guild, Some(panel.id), (mia, "Mia"), kai, "Order problem", true).await;
    let claimed = seed_ticket(&db, guild, Some(panel.id), (leo, "Leo"), kai, "Billing", false).await;
    db.claim_ticket(claimed, kai).await.expect("claim");
    seed_ticket(&db, guild, Some(panel.id), (sam, "Sam"), kai, "Support", false).await;
}
