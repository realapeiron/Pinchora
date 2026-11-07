//! F21 event form, public RSVP card, capacity, promotion and recurrence.

use super::*;

const MANAGE_GUILD: u64 = 1 << 5;

#[tokio::test]
async fn event_form_rsvp_waitlist_and_recurrence() {
    let Some(mut sim) = Sim::new("event_form_rsvp_waitlist_and_recurrence").await else {
        return;
    };
    sim.permissions = MANAGE_GUILD;
    let manager = unique();
    sim.app
        .db
        .set_event_timezone(sim.guild, "Europe/Istanbul")
        .await
        .unwrap();
    assert_eq!(
        sim.app.db.event_timezone_for(sim.guild, manager).await.unwrap(),
        "Europe/Istanbul"
    );
    let form = sim.command(manager, "event", json!([{"name":"create","type":1}])).await;
    assert_eq!(form.modal.as_ref().map(|m| m.0.as_str()), Some("ev:c"));
    let tomorrow = (chrono::Utc::now().with_timezone(&"Europe/Istanbul".parse::<chrono_tz::Tz>().unwrap())
        + chrono::Duration::days(1))
    .format("%Y-%m-%d 20:00")
    .to_string();
    let created = sim
        .submit_fields(
            manager,
            &form,
            &[
                ("title", "Crab night"),
                ("when", &tomorrow),
                ("duration", "90"),
                ("description", "Bring a friend"),
            ],
        )
        .await;
    assert!(
        created.content.as_deref().unwrap_or_default().contains("posted"),
        "{created}"
    );
    let rows = sim.app.db.events(sim.guild, false).await.unwrap();
    let row = rows.iter().find(|r| r.title == "Crab night").unwrap();
    let id = row.id;
    let posts = sim.posts();
    let card = posts
        .iter()
        .find(|p| p.title().contains("Crab night"))
        .expect("public card");
    assert!(card.description().contains("<t:"));
    let reply = posted_reply(card);
    let first = unique();
    let r = sim.component(first, &format!("ev:r:{id}:going"), &[], &reply).await;
    assert!(r.content.as_deref().unwrap_or_default().contains("confirmed"), "{r}");
    let _ = sim.posts();
    // A manager limits the going option to one place.
    let updated=sim.command(manager,"event",json!([{"name":"edit","type":1,"options":[{"name":"id","type":4,"value":id},{"name":"capacity","type":4,"value":1}]}])).await;
    assert!(
        updated.content.as_deref().unwrap_or_default().contains("updated"),
        "{updated}"
    );
    let second = unique();
    let waiting = sim.component(second, &format!("ev:r:{id}:going"), &[], &reply).await;
    assert!(
        waiting.content.as_deref().unwrap_or_default().contains("waitlist"),
        "{waiting}"
    );
    let _ = sim.component(first, &format!("ev:r:{id}:going"), &[], &reply).await;
    let attendees = sim.app.db.event_rsvps(id, 0, 25).await.unwrap();
    assert_eq!(attendees.len(), 1);
    assert_eq!(attendees[0].user_id, second as i64);
    assert!(!attendees[0].waitlisted);
    assert!(
        sim.app
            .db
            .scheduled_reminder(second, "event_60", id)
            .await
            .unwrap()
            .is_some()
    );
    sim.app.db.set_dm_opt_out(second, true).await.unwrap();
    let _ = sim.component(second, &format!("ev:r:{id}:going"), &[], &reply).await;
    let _ = sim.component(second, &format!("ev:r:{id}:going"), &[], &reply).await;
    assert!(
        sim.app
            .db
            .scheduled_reminder(second, "event_60", id)
            .await
            .unwrap()
            .is_none(),
        "server DMs off suppresses the event reminder"
    );
    let _ = sim
        .command(
            manager,
            "event",
            json!([{"name":"edit","type":1,"options":[
                {"name":"id","type":4,"value":id}, {"name":"capacity","type":4,"value":3}
            ]}]),
        )
        .await;
    let third = unique();
    let fourth = unique();
    let fifth = unique();
    for person in [third, fourth] {
        assert_eq!(
            sim.app.db.rsvp_event(sim.guild, id, person, "going").await.unwrap(),
            pinchora_db::RsvpResult::Confirmed
        );
    }
    assert_eq!(
        sim.app.db.rsvp_event(sim.guild, id, fifth, "going").await.unwrap(),
        pinchora_db::RsvpResult::Waitlisted
    );
    let _ = sim
        .command(
            manager,
            "event",
            json!([{"name":"edit","type":1,"options":[
                {"name":"id","type":4,"value":id}, {"name":"capacity","type":4,"value":1}
            ]}]),
        )
        .await;
    for person in [third, fourth] {
        sim.app.db.rsvp_event(sim.guild, id, person, "going").await.unwrap();
        assert!(
            sim.app
                .db
                .event_rsvps(id, 0, 25)
                .await
                .unwrap()
                .iter()
                .any(|r| r.user_id == fifth as i64 && r.waitlisted)
        );
    }
    sim.app.db.rsvp_event(sim.guild, id, second, "going").await.unwrap();
    assert!(
        sim.app
            .db
            .event_rsvps(id, 0, 25)
            .await
            .unwrap()
            .iter()
            .any(|r| r.user_id == fifth as i64 && !r.waitlisted)
    );
    let role = unique();
    let restricted = sim
        .command(
            manager,
            "event",
            json!([{"name":"edit","type":1,"options":[
                {"name":"id","type":4,"value":id}, {"name":"role","type":4,"value":role}
            ]}]),
        )
        .await;
    assert!(
        restricted.content.as_deref().unwrap_or_default().contains("updated"),
        "{restricted}"
    );
    let outsider = sim.component(unique(), &format!("ev:r:{id}:going"), &[], &reply).await;
    assert!(
        outsider
            .content
            .as_deref()
            .unwrap_or_default()
            .contains(&format!("<@&{role}>")),
        "{outsider}"
    );

    let until = (chrono::Utc::now() + chrono::Duration::days(3))
        .format("%Y-%m-%d")
        .to_string();
    let repeated = sim
        .command(
            manager,
            "event",
            json!([{"name":"edit","type":1,"options":[
                {"name":"id","type":4,"value":id}, {"name":"repeat","type":3,"value":"daily"},
                {"name":"until","type":3,"value":until}
            ]}]),
        )
        .await;
    assert!(
        repeated.content.as_deref().unwrap_or_default().contains("updated"),
        "{repeated}"
    );
    let start = chrono::Utc::now() - chrono::Duration::days(1) + chrono::Duration::hours(2);
    sqlx::query("UPDATE events SET starts_at=$2, ends_at=$3 WHERE id=$1")
        .bind(id)
        .bind(start)
        .bind(start + chrono::Duration::hours(1))
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let now = chrono::Utc::now();
    let (first_pod, second_pod) = tokio::join!(sim.app.db.advance_events(now), sim.app.db.advance_events(now));
    let next: Vec<_> = first_pod.unwrap().into_iter().chain(second_pod.unwrap()).collect();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].parent_id, Some(id));
    assert!(sim.app.db.advance_events(chrono::Utc::now()).await.unwrap().is_empty());
}

async fn scheduled_discord(
    State(mock): State<Mock>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, String) {
    let path = uri.path();
    if path.contains("/scheduled-events") {
        let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
        let guild = parts
            .iter()
            .position(|p| *p == "guilds")
            .and_then(|i| parts.get(i + 1))
            .copied()
            .unwrap_or("0");
        let operation = match method {
            Method::POST => "create",
            Method::PATCH => "update",
            Method::DELETE => "delete",
            _ => "other",
        };
        mock.mod_calls.lock().unwrap().push(format!("scheduled {operation}"));
        if method == Method::DELETE {
            return (StatusCode::NO_CONTENT, String::new());
        }
        let (sent, _) = request_body(&headers, &body);
        let event = json!({
            "id": "90001", "guild_id": guild, "name": sent["name"].as_str().unwrap_or("Crab night"),
            "privacy_level": 2, "entity_type": 3, "scheduled_start_time": sent["scheduled_start_time"],
            "scheduled_end_time": sent["scheduled_end_time"], "status": 1,
            "recurrence_rule": null,
        });
        return (StatusCode::OK, event.to_string());
    }
    discord(State(mock), method, uri, headers, body).await
}

#[tokio::test]
async fn scheduled_event_sync_creates_updates_and_deletes_with_manage_events() {
    let Some(mut sim) = Sim::new("scheduled_event_sync_creates_updates_and_deletes_with_manage_events").await else {
        return;
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(
        axum::serve(
            listener,
            Router::new().fallback(scheduled_discord).with_state(sim.mock.clone()),
        )
        .into_future(),
    );
    let rest = Arc::new(
        Client::builder()
            .token("Bot sim".into())
            .proxy(address.to_string(), true)
            .ratelimiter(None)
            .build(),
    );
    let app = Arc::get_mut(&mut sim.app).unwrap();
    app.http = rest.clone();
    app.interaction_http = rest;
    let start = chrono::Utc::now() + chrono::Duration::days(1);
    let draft = pinchora_db::EventDraft {
        title: "Crab night".into(),
        description: "At the reef".into(),
        starts_at: start,
        ends_at: start + chrono::Duration::hours(1),
        tz: "UTC".into(),
        location: "Reef".into(),
        channel_id: Some(CHANNEL_ID as i64),
        host_id: None,
        options: pinchora_core::events::default_options(),
        required_role_id: None,
        reminders: vec![60, 15],
        recurrence: None,
    };
    let row = sim.app.db.create_event(sim.guild, &draft).await.unwrap();
    let Some(missing) = Sim::new("scheduled_event_sync_missing_permission").await else {
        return;
    };
    let error = pinchora_commands::sync_discord_event(&missing.app, &row, true)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Manage Events"));
    sim.guild_role_with(BOT_ROLE_ID, "Pinchora", BOT_ROLE_POSITION, 1u64 << 33);
    pinchora_commands::sync_discord_event(&sim.app, &row, true)
        .await
        .unwrap();
    let row = sim.app.db.event(sim.guild, row.id).await.unwrap().unwrap();
    assert_eq!(row.discord_event_id, Some(90001));
    pinchora_commands::sync_discord_event(&sim.app, &row, true)
        .await
        .unwrap();
    pinchora_commands::sync_discord_event(&sim.app, &row, false)
        .await
        .unwrap();
    assert_eq!(
        sim.mock.mod_calls.lock().unwrap().as_slice(),
        &["scheduled create", "scheduled update", "scheduled delete"]
    );
}
