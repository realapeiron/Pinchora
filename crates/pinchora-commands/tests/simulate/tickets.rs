//! Tickets: a panel posted and pressed, the ticket channel (or private thread) it opens, the controls
//! inside, closing with a transcript, the log and the opener's rating, the ticker that closes idle
//! tickets, and who may set any of it up.
//!
//! The shared mock knows no channels being made, so these tests put a small Discord of their own in front
//! of it: it makes channels and threads, keeps permission and thread-member changes, answers a channel's
//! history, and keeps the transcript pages handlers send (the harness reads every other file as a picture).

use pinchora_commands::{on_ticket_place_deleted, tickets_tick};
use pinchora_core::tickets::{Question, Topic};
use pinchora_db::{NewTicket, PanelWrite, TicketOpen, TicketPanelSaved};

use super::*;

// /Ticket mock

/// A page a handler sent as a file: where it went and what it is.
#[derive(Clone)]
struct Page {
    path: String,
    name: String,
    html: String,
}

/// What the ticket routes remember on top of the shared mock.
#[derive(Clone, Default)]
struct World {
    /// Channels the handlers made, as (id, the body they sent).
    made: Arc<Mutex<Vec<(u64, Value)>>>,
    /// Private threads, as (parent, id, body).
    threads: Arc<Mutex<Vec<(u64, u64, Value)>>>,
    /// Changes to channels: `PUT perm <channel> <target> <body>`, `DELETE perm …`, `PATCH <channel> <body>`,
    /// `DELETE channel <channel>`, `PUT member <thread> <user>`, `DELETE member …`.
    changes: Arc<Mutex<Vec<String>>>,
    /// What each channel's history holds, oldest first.
    history: Arc<Mutex<HashMap<u64, Vec<Value>>>>,
    thread_members: Arc<Mutex<HashMap<u64, Vec<u64>>>>,
    /// How many channels `GET /guilds/{id}/channels` lists.
    channel_count: Arc<AtomicUsize>,
    /// Refuse to make channels, as Discord does for a bot without Manage Channels.
    refuse_channels: Arc<AtomicBool>,
    refuse_uploads: Arc<AtomicBool>,
    refuse_message_edits: Arc<AtomicBool>,
    pages: Arc<Mutex<Vec<Page>>>,
}

#[derive(Clone)]
struct TicketMock {
    mock: Mock,
    world: World,
}

fn missing_permissions() -> (StatusCode, String) {
    let refusal = json!({ "message": "Missing Permissions", "code": 50013 });
    (StatusCode::FORBIDDEN, refusal.to_string())
}

fn channel_json(id: u64, guild: &str, kind: u8, name: &str, parent: Option<&str>) -> Value {
    json!({
        "id": id.to_string(),
        "type": kind,
        "guild_id": guild,
        "name": name,
        "position": 0,
        "parent_id": parent,
        "permission_overwrites": [],
    })
}

async fn ticket_discord(
    State(state): State<TicketMock>,
    method: Method,
    uri: Uri,
    mut headers: HeaderMap,
    mut body: Bytes,
) -> (StatusCode, String) {
    let path = uri.path().to_owned();
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let number = |part: &str| part.parse::<u64>().unwrap_or(0);
    let world = &state.world;
    // Transcript pages ride as files: keep them here and pass the rest on as plain JSON.
    let (json, files) = request_body(&headers, &body);
    if !files.is_empty() && world.refuse_uploads.load(Ordering::SeqCst) {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({ "message": "Request entity too large", "code": 40005 }).to_string(),
        );
    }
    if files.iter().any(|(name, _)| name.ends_with(".html")) {
        for (name, bytes) in files {
            assert!(
                name.ends_with(".html"),
                "a transcript and a picture in one message: {name}"
            );
            world.pages.lock().unwrap().push(Page {
                path: path.clone(),
                name,
                html: String::from_utf8(bytes).expect("a page is text"),
            });
        }
        headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
        body = Bytes::from(json.to_string());
    }
    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    match (&method, parts.as_slice()) {
        (&Method::PATCH, [.., "channels", _, "messages", _]) if world.refuse_message_edits.load(Ordering::SeqCst) => {
            return missing_permissions();
        }
        (&Method::POST, [.., "guilds", guild, "channels"]) => {
            if world.refuse_channels.load(Ordering::SeqCst) {
                return missing_permissions();
            }
            let id = unique();
            let mut channel = channel_json(
                id,
                guild,
                0,
                json["name"].as_str().unwrap_or_default(),
                json["parent_id"].as_str(),
            );
            channel["permission_overwrites"] = json["permission_overwrites"].clone();
            state.mock.channels.lock().unwrap().insert(id, channel.clone());
            world.made.lock().unwrap().push((id, json));
            return (StatusCode::OK, channel.to_string());
        }
        (&Method::GET, [.., "guilds", guild, "channels"]) => {
            let count = world.channel_count.load(Ordering::SeqCst);
            let list: Vec<Value> = (0..count)
                .map(|n| channel_json(n as u64 + 1, guild, 0, "chat", None))
                .collect();
            return (StatusCode::OK, Value::Array(list).to_string());
        }
        (&Method::POST, [.., "channels", parent, "threads"]) => {
            let id = unique();
            let thread = json!({
                "id": id.to_string(),
                "type": 12,
                "guild_id": "1",
                "name": json["name"],
                "parent_id": parent.to_string(),
            });
            world.threads.lock().unwrap().push((number(parent), id, json));
            return (StatusCode::OK, thread.to_string());
        }
        (&Method::PUT | &Method::DELETE, [.., "channels", channel, "thread-members", user]) => {
            let mut members = world.thread_members.lock().unwrap();
            let list = members.entry(number(channel)).or_default();
            if method == Method::PUT && !list.contains(&number(user)) {
                list.push(number(user));
            } else if method == Method::DELETE {
                list.retain(|id| *id != number(user));
            }
            world
                .changes
                .lock()
                .unwrap()
                .push(format!("{method} member {channel} {user}"));
            return (StatusCode::NO_CONTENT, String::new());
        }
        (&Method::GET, [.., "channels", channel, "thread-members"]) => {
            let members = world.thread_members.lock().unwrap();
            let list: Vec<Value> = members
                .get(&number(channel))
                .into_iter()
                .flatten()
                .map(|id| {
                    json!({
                        "id": channel,
                        "user_id": id.to_string(),
                        "join_timestamp": "2024-01-01T00:00:00.000000+00:00",
                        "flags": 0,
                    })
                })
                .collect();
            return (StatusCode::OK, Value::Array(list).to_string());
        }
        (&Method::PUT | &Method::DELETE, [.., "channels", channel, "permissions", target]) => {
            world
                .changes
                .lock()
                .unwrap()
                .push(format!("{method} perm {channel} {target} {json}"));
            return (StatusCode::NO_CONTENT, String::new());
        }
        (&Method::GET, [.., "channels", channel, "messages"]) => {
            let mut lines = world
                .history
                .lock()
                .unwrap()
                .get(&number(channel))
                .cloned()
                .unwrap_or_default();
            lines.reverse();
            return (StatusCode::OK, Value::Array(lines).to_string());
        }
        (&Method::PATCH, [.., "channels", channel]) => {
            world.changes.lock().unwrap().push(format!("PATCH {channel} {json}"));
            let known = state.mock.channels.lock().unwrap().get(&number(channel)).cloned();
            let answer = known.unwrap_or_else(|| channel_json(number(channel), "1", 12, "thread", None));
            return (StatusCode::OK, answer.to_string());
        }
        (&Method::DELETE, [.., "channels", channel]) => {
            world.changes.lock().unwrap().push(format!("DELETE channel {channel}"));
            let gone = state.mock.channels.lock().unwrap().remove(&number(channel));
            let answer = gone.unwrap_or_else(|| channel_json(number(channel), "1", 0, "gone", None));
            return (StatusCode::OK, answer.to_string());
        }
        _ => {}
    }
    discord(State(state.mock), method, uri, headers, body).await
}

/// A simulator whose Discord also makes ticket channels and threads.
async fn ticket_sim(name: &'static str) -> Option<(Sim, World)> {
    let mut sim = Sim::new(name).await?;
    let world = World::default();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind the ticket mock");
    let address = listener.local_addr().expect("ticket mock address");
    let router = Router::new().fallback(ticket_discord).with_state(TicketMock {
        mock: sim.mock.clone(),
        world: world.clone(),
    });
    tokio::spawn(axum::serve(listener, router).into_future());
    let rest = Arc::new(
        Client::builder()
            .token("Bot sim".into())
            .proxy(address.to_string(), true)
            .ratelimiter(None)
            .build(),
    );
    let app = Arc::get_mut(&mut sim.app).expect("nobody holds the new app yet");
    app.http = rest.clone();
    app.interaction_http = rest;
    Some((sim, world))
}

/// A message in a ticket's history, `seconds` from now, as Discord lists it.
fn history_message(channel: u64, author: Value, content: &str, seconds: i64, files: &[&str]) -> Value {
    let at = chrono::Utc::now() + chrono::Duration::seconds(seconds);
    json!({
        "id": unique().to_string(),
        "channel_id": channel.to_string(),
        "type": 0,
        "author": author,
        "content": content,
        "timestamp": at.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string(),
        "edited_timestamp": null,
        "tts": false,
        "mention_everyone": false,
        "mentions": [],
        "mention_roles": [],
        "pinned": false,
        "attachments": files.iter().map(|name| json!({
            "id": unique().to_string(),
            "filename": name,
            "size": 10,
            "url": "https://cdn.example/file",
            "proxy_url": "https://cdn.example/file",
            "ephemeral": false,
        })).collect::<Vec<_>>(),
        "embeds": [],
        "flags": 0,
    })
}

fn sub_group(group_name: &str, sub_name: &str, options: Vec<Value>) -> Value {
    json!([group(group_name, sub(sub_name, options))])
}

/// The posts made since the last call, without the image checks of [`Sim::posts`] (no ticket post
/// carries a picture).
fn take_posts(sim: &Sim) -> Vec<Post> {
    std::mem::take(&mut *sim.mock.posts.lock().unwrap())
}

fn changes(world: &World) -> Vec<String> {
    std::mem::take(&mut *world.changes.lock().unwrap())
}

/// A panel straight into the database, as the dashboard saves one.
async fn save_panel(
    sim: &Sim,
    name: &str,
    channel: u64,
    topics: &[Topic],
    questions: &[Question],
    roles: &[u64],
) -> i64 {
    let saved = sim
        .app
        .db
        .save_ticket_panel(
            None,
            &PanelWrite {
                guild_id: sim.guild,
                name,
                channel_id: channel,
                title: "Need help?",
                description: "Press a button.",
                color: 0xff6b4a,
                style: pinchora_core::tickets::PanelStyle::Buttons,
                topics,
                questions,
                support_roles: roles,
                welcome_text: "",
                open_category_id: None,
                log_channel_id: None,
                enabled: true,
            },
        )
        .await
        .expect("save a panel");
    let TicketPanelSaved::Saved(panel) = saved else {
        panic!("the panel was not saved: {saved:?}")
    };
    panel.id
}

/// The panel message the bot posted in `channel`, ready to be clicked.
async fn posted_panel(sim: &mut Sim, admin: u64, panel: &str, channel: u64) -> Reply {
    sim.permissions = MANAGE_GUILD;
    let posted = sim
        .command(
            admin,
            "tickets",
            sub_group("panel", "post", vec![text_option("panel", panel)]),
        )
        .await;
    assert!(
        posted.ephemeral && posted.content().contains("https://discord.com/channels/"),
        "{}",
        posted.content()
    );
    sim.permissions = 0;
    let posts = take_posts(sim);
    let post = posts
        .iter()
        .rev()
        .find(|p| p.channel == channel)
        .expect("the panel was posted in its channel");
    posted_reply(post)
}

// Both tests expose a board to the guild-wide ticker; keep their Discord worlds isolated.
static TICKET_TICK_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// /Tests

#[tokio::test]
async fn deleted_ticket_is_recovered_on_next_open() {
    let Some((mut sim, world)) = ticket_sim("deleted_ticket_is_recovered_on_next_open").await else {
        return;
    };
    let (admin, member, channel) = (unique(), unique(), unique());
    save_panel(&sim, "Help", channel, &[Topic::new("Support")], &[], &[]).await;
    let panel = posted_panel(&mut sim, admin, "Help", channel).await;
    sim.click(member, &panel, "Support").await;
    let first = world.made.lock().unwrap()[0].0;
    sim.mock.channels.lock().unwrap().remove(&first);
    sim.app
        .cache
        .release_cooldown(&format!("tk:{}:{member}", sim.guild))
        .await
        .unwrap();
    // Expire the persistent cooldown too; deleting a cache key must not bypass it.
    sim.app.shift_clock(chrono::Duration::seconds(61));
    let again = sim.click(member, &panel, "Support").await;
    assert!(
        again
            .followups
            .last()
            .unwrap()
            .content()
            .contains("Your ticket is open")
    );
    assert_eq!(world.made.lock().unwrap().len(), 2);
    let old = sim.app.db.ticket_at(sim.guild, first).await.unwrap().unwrap();
    assert!(!old.is_open());
    assert_eq!(old.close_reason.as_deref(), Some("The ticket channel was deleted."));
}

#[tokio::test]
async fn deleted_ticket_gateway_close_is_idempotent() {
    let Some((mut sim, world)) = ticket_sim("deleted_ticket_gateway_close_is_idempotent").await else {
        return;
    };
    let (admin, member, channel) = (unique(), unique(), unique());
    save_panel(&sim, "Help", channel, &[Topic::new("Support")], &[], &[]).await;
    let panel = posted_panel(&mut sim, admin, "Help", channel).await;
    sim.click(member, &panel, "Support").await;
    let place = world.made.lock().unwrap()[0].0;
    sim.mock.channels.lock().unwrap().remove(&place);
    on_ticket_place_deleted(&sim.app, sim.guild, place).await.unwrap();
    on_ticket_place_deleted(&sim.app, sim.guild, place).await.unwrap();
    let ticket = sim.app.db.ticket_at(sim.guild, place).await.unwrap().unwrap();
    assert!(!ticket.is_open());
    assert_eq!(ticket.close_reason.as_deref(), Some("The ticket channel was deleted."));
}

/// The whole life of a ticket: set up and post a panel, open a ticket from it, claim it, close it with a
/// transcript (hostile text and all), rate it, reopen it, read its transcript, close and delete it.
#[tokio::test]
async fn a_ticket_opens_from_a_panel_is_claimed_closed_rated_and_reopened() {
    let Some((mut sim, world)) = ticket_sim("a_ticket_opens_from_a_panel_is_claimed_closed_rated_and_reopened").await
    else {
        return;
    };
    let (admin, member, staff) = (unique(), unique(), unique());
    let (panel_channel, log_channel, support) = (unique(), unique(), unique());
    let role = sim.guild_role(support, "Support", 2);
    DM_OVERRIDES.lock().unwrap().insert(member, false);

    sim.permissions = MANAGE_GUILD;
    let created = sim
        .command_resolved(
            admin,
            "tickets",
            sub_group(
                "panel",
                "create",
                vec![
                    text_option("name", "Help desk"),
                    channel_option("channel", panel_channel),
                    text_option("topics", "General, Billing"),
                    role_option("support_role", &role),
                ],
            ),
            json!({
                "channels": resolved_channels(&[panel_channel])["channels"],
                "roles": resolved(&[], &[&role])["roles"],
            }),
        )
        .await;
    assert!(
        created.ephemeral && created.content().contains("Help desk"),
        "{}",
        created.content()
    );
    let set = sim
        .command_resolved(
            admin,
            "tickets",
            json!([sub(
                "settings",
                vec![channel_option("log_channel", log_channel), bool_option("ratings", true)]
            )]),
            resolved_channels(&[log_channel]),
        )
        .await;
    assert_eq!(set.title(), "Ticket settings");
    assert!(set.description().contains("Saved"), "{}", set.description());

    let panel = posted_panel(&mut sim, admin, "Help desk", panel_channel).await;
    assert_eq!(panel.title(), "Need help?");
    assert!(panel.has_button("General") && panel.has_button("Billing"));

    // A member opens one about billing.
    let opened = sim.click(member, &panel, "Billing").await;
    let told = opened.followups.last().expect("the member is told where");
    let made = world.made.lock().unwrap().clone();
    assert_eq!(made.len(), 1, "one channel");
    let (place, body) = &made[0];
    assert!(
        told.ephemeral && told.content().contains(&format!("<#{place}>")),
        "{told}"
    );
    assert_eq!(body["name"], "ticket-0001");
    let overwrites = body["permission_overwrites"].as_array().expect("overwrites");
    let find = |id: u64| {
        overwrites
            .iter()
            .find(|o| o["id"].as_str().and_then(|s| s.parse::<u64>().ok()) == Some(id))
            .unwrap_or_else(|| panic!("no overwrite for {id}: {overwrites:?}"))
    };
    assert_eq!(find(sim.guild)["deny"], "1024", "everyone else cannot see it");
    for who in [member, APPLICATION_ID, support] {
        let allow: u64 = find(who)["allow"].as_str().unwrap().parse().unwrap();
        assert!(allow & 1024 != 0 && allow & 2048 != 0, "{who} sees and writes");
    }
    let posts = take_posts(&sim);
    let welcome = posts
        .iter()
        .find(|p| p.channel == *place)
        .expect("a welcome in the ticket");
    assert_eq!(welcome.title(), "Ticket #0001 · Billing");
    assert_eq!(welcome.content(), format!("<@{member}> <@&{support}>"));
    assert_eq!(
        welcome.body["allowed_mentions"],
        json!({ "parse": [], "users": [member.to_string()], "roles": [support.to_string()] })
    );
    let logged = posts.iter().find(|p| p.channel == log_channel).expect("logged");
    assert_eq!(logged.title(), "Ticket #0001 opened");
    let welcome = posted_reply(welcome);
    for label in ["Close", "Claim", "Transcript", "Lock"] {
        assert!(welcome.has_button(label), "{label}");
    }

    // A second ticket right away waits a minute; with the wait over, one open ticket is the limit.
    let again = sim.click(member, &panel, "General").await;
    assert!(again.content().contains("Try again in"), "{again}");
    sim.app
        .cache
        .release_cooldown(&format!("tk:{}:{member}", sim.guild))
        .await
        .unwrap();
    let again = sim.click(member, &panel, "General").await;
    let said = again.followups.last().map_or(again.content(), Reply::content);
    assert!(said.contains("already have an open ticket"), "{again}");
    assert_eq!(world.made.lock().unwrap().len(), 1, "no second channel");

    // Inside the ticket: the member cannot claim it, staff with the support role can.
    sim.channel = *place;
    let refused = sim.click(member, &welcome, "Claim").await;
    assert!(
        refused.ephemeral && refused.content().contains("Only the ticket's staff"),
        "{refused}"
    );
    sim.roles = vec![support];
    let claimed = sim.click(staff, &welcome, "Claim").await;
    assert!(claimed.content().contains("You claimed ticket #0001"), "{claimed}");
    let extra = unique();
    sim.mock.channels.lock().unwrap().get_mut(place).unwrap()["permission_overwrites"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": extra.to_string(), "type": 1, "allow": "3072", "deny": "0"}));
    sim.click(staff, &welcome, "Lock").await;
    let locked = changes(&world);
    for who in [sim.guild, member, extra] {
        let change = locked
            .iter()
            .find(|c| c.starts_with(&format!("PUT perm {place} {who} ")))
            .unwrap();
        let overwrite: Value = serde_json::from_str(&change[change.find('{').unwrap()..]).unwrap();
        let deny: u64 = overwrite["deny"].as_str().unwrap().parse().unwrap();
        assert_ne!(deny & 2048, 0, "{who} cannot send: {change}");
    }
    let twice = sim.click(staff, &welcome, "Claim").await;
    assert!(
        twice.content().contains("back in the queue"),
        "the button hands it back: {twice}"
    );
    sim.click(staff, &welcome, "Claim").await;
    let posts = take_posts(&sim);
    assert!(
        posts
            .iter()
            .any(|p| p.channel == *place && p.description().contains("is handling this ticket")),
        "the ticket hears who took it"
    );
    sim.roles = Vec::new();

    // What was said in the ticket, hostile text included.
    let bot = json!({ "id": APPLICATION_ID.to_string(), "username": "Pinchora", "discriminator": "0", "avatar": null, "bot": true });
    let mentioned = unique();
    let mut hostile = history_message(
        *place,
        person_json(member, "sam", Some("Sam <b>"), false),
        &format!("<script>alert('x')</script> my bill is wrong & here's proof <@{mentioned}>"),
        2,
        &["bill \"1\".png"],
    );
    let mut mention = person_json(mentioned, "private-name", Some("Private Name"), false);
    mention["public_flags"] = json!(0);
    hostile["mentions"] = json!([mention]);
    world.history.lock().unwrap().insert(
        *place,
        vec![
            history_message(*place, bot, "", 1, &[]),
            hostile,
            history_message(
                *place,
                person_json(staff, "kai", Some("Kai"), false),
                "On it!",
                120,
                &[],
            ),
        ],
    );
    let closing = sim
        .command(
            member,
            "ticket",
            json!([sub("close", vec![text_option("reason", "Solved <@&1> **thanks**")])]),
        )
        .await;
    assert!(
        closing.ephemeral && closing.content().contains("Closing ticket #0001"),
        "{closing}"
    );
    let ticket = sim
        .app
        .db
        .ticket_at(sim.guild, *place)
        .await
        .unwrap()
        .expect("the ticket");
    assert!(!ticket.is_open());
    assert_eq!(ticket.closed_by, Some(member as i64));
    assert_eq!(ticket.close_reason.as_deref(), Some("Solved **thanks**"));
    let reply_at = ticket.first_reply_at.expect("the first reply was found");
    assert!(
        reply_at > ticket.opened_at + chrono::Duration::seconds(60),
        "Kai answered, not Sam"
    );
    let changed = changes(&world);
    assert!(
        changed
            .iter()
            .any(|c| c.starts_with(&format!("DELETE perm {place} {member}"))),
        "the member loses access: {changed:?}"
    );
    let posts = take_posts(&sim);
    let notice = posts
        .iter()
        .find(|p| p.channel == *place && p.description().contains("Closed by"))
        .expect("the ticket says it closed");
    let closed_controls = posted_reply(notice);
    for label in ["Reopen", "Transcript", "Delete"] {
        assert!(closed_controls.has_button(label), "{label}");
    }
    let log = posts
        .iter()
        .find(|p| p.channel == log_channel && p.title() == "Ticket #0001 closed")
        .expect("the close is logged");
    assert!(log.description().contains("3 messages"), "{}", log.description());
    let dm = posts
        .iter()
        .find(|p| p.channel == member)
        .expect("the member gets a DM");
    assert!(
        dm.content().contains("was closed") && dm.content().contains("How did we do"),
        "{}",
        dm.content()
    );
    let pages = world.pages.lock().unwrap().clone();
    assert_eq!(pages.len(), 2, "one page for the log, one for the member");
    for to in [log_channel, member] {
        assert!(
            pages
                .iter()
                .any(|p| p.path.ends_with(&format!("/channels/{to}/messages"))),
            "a page went to {to}"
        );
    }
    for page in &pages {
        assert_eq!(page.name, "ticket-0001.html");
        assert!(
            page.html.contains("&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"),
            "{}",
            page.html
        );
        assert!(!page.html.contains("<script"), "nothing becomes markup");
        assert!(page.html.contains("bill &quot;1&quot;.png"));
        assert!(page.html.contains("Sam &lt;b&gt;") && page.html.contains("Kai"));
        assert!(page.html.contains("@private-name"));
        assert!(!page.html.contains(&format!("&lt;@{mentioned}&gt;")));
    }
    std::fs::write(sim.dir.join("transcript.html"), &pages[0].html).unwrap();
    let stored = sim.app.db.transcript(ticket.id).await.unwrap().expect("kept");
    assert_eq!(stored.lines.len(), 3);
    assert!(stored.with_content);
    assert!(stored.lines.iter().any(|line| {
        line.c
            .as_deref()
            .unwrap_or_default()
            .contains(&format!("<@{mentioned}>"))
    }));
    sim.app
        .db
        .delete_user_data(mentioned, chrono::Utc::now())
        .await
        .unwrap();
    let anonymised = sim.app.db.transcript(ticket.id).await.unwrap().unwrap();
    assert!(!anonymised.author_ids.iter().any(|(_, id)| *id == mentioned));
    assert_eq!(
        pinchora_core::tickets::render_mentions(
            &format!("<@{mentioned}>"),
            &std::collections::HashMap::new(),
            pinchora_core::i18n::Lang::En
        ),
        "@Deleted user"
    );

    // The member rates it once.
    let rating = posted_reply(dm);
    let rated = sim.click(member, &rating, "★★★★").await;
    assert!(rated.content().contains("You gave ★★★★"), "{rated}");
    let again = sim.click(member, &rating, "★").await;
    assert!(again.content().contains("rated already"), "{again}");
    assert_eq!(sim.app.db.ticket(ticket.id).await.unwrap().unwrap().rating, Some(4));

    // A closed ticket cannot be closed or claimed again; staff reopen it.
    let closed_again = sim.click(member, &welcome, "Close").await;
    assert!(closed_again.content().contains("is closed"), "{closed_again}");
    let not_staff = sim.click(member, &closed_controls, "Reopen").await;
    assert!(not_staff.content().contains("Only the ticket's staff"), "{not_staff}");
    sim.roles = vec![support];
    sim.click(staff, &closed_controls, "Reopen").await;
    let reopened = sim.app.db.ticket(ticket.id).await.unwrap().unwrap();
    assert!(reopened.is_open() && reopened.rating.is_none() && reopened.claimed_by.is_none());
    assert!(
        sim.app.db.transcript(ticket.id).await.unwrap().is_none(),
        "a new one is kept next time"
    );
    let changed = changes(&world);
    assert!(
        changed
            .iter()
            .any(|c| c.starts_with(&format!("PUT perm {place} {member}"))),
        "the member sees it again: {changed:?}"
    );
    let posts = take_posts(&sim);
    assert!(
        posts
            .iter()
            .any(|p| p.channel == *place && p.description().contains("reopened"))
    );

    // Staff read the transcript privately; the member is not staff but may too.
    let transcript = sim.click(staff, &welcome, "Transcript").await;
    let said = transcript.followups.last().map_or(transcript.content(), Reply::content);
    assert!(said.contains("transcript"), "{transcript}");
    assert_eq!(world.pages.lock().unwrap().len(), 3);

    // Closed by button (asked first), then the channel is deleted.
    let ask = sim.click(staff, &welcome, "Close").await;
    assert!(ask.ephemeral && ask.content().contains("Close ticket #0001?"), "{ask}");
    world.refuse_uploads.store(true, Ordering::SeqCst);
    let closing = sim.click(staff, &ask, "Close ticket").await;
    assert!(closing.content().contains("Closing"), "{closing}");
    let posts = take_posts(&sim);
    let notice = posts
        .iter()
        .find(|p| p.channel == *place && p.description().contains("Closed by"))
        .expect("closed again");
    let controls = posted_reply(notice);
    assert!(
        posts
            .iter()
            .any(|p| p.channel == log_channel && p.title() == "Ticket #0001 closed")
    );
    assert!(
        posts
            .iter()
            .any(|p| p.channel == member && p.content().contains("was closed"))
    );
    let ask = sim.click(staff, &controls, "Delete").await;
    sim.click(staff, &ask, "Delete channel").await;
    assert!(changes(&world).contains(&format!("DELETE channel {place}")));
    assert!(
        sim.app.db.ticket(ticket.id).await.unwrap().is_some(),
        "the ticket stays"
    );
}

/// Two members opening at once get two numbers; one member opening twice at once gets one ticket.
#[tokio::test]
async fn ticket_numbers_never_collide_and_one_member_gets_one_open_ticket() {
    let Some((sim, _world)) = ticket_sim("ticket_numbers_never_collide_and_one_member_gets_one_open_ticket").await
    else {
        return;
    };
    let (a, b) = (unique(), unique());
    let open = |user: u64| {
        let db = sim.app.db.clone();
        let guild = sim.guild;
        async move {
            db.open_ticket(&NewTicket {
                guild_id: guild,
                panel_id: None,
                opener: user,
                topic: "Help",
                answers: &[],
                max_open: 1,
                at: chrono::Utc::now(),
            })
            .await
            .expect("open")
        }
    };
    let (first, second) = tokio::join!(open(a), open(b));
    let number = |o: &TicketOpen| match o {
        TicketOpen::Opened(t) => t.number,
        TicketOpen::TooMany { .. } | TicketOpen::Cooldown { .. } => panic!("both may open one"),
    };
    let mut numbers = [number(&first), number(&second)];
    numbers.sort_unstable();
    assert_eq!(numbers, [1, 2]);

    let c = unique();
    let (one, two, three) = tokio::join!(open(c), open(c), open(c));
    let opened = [&one, &two, &three]
        .iter()
        .filter(|o| matches!(o, TicketOpen::Opened(_)))
        .count();
    assert_eq!(opened, 1, "one member, one ticket");
    assert!(
        [one, two, three]
            .iter()
            .any(|o| matches!(o, TicketOpen::TooMany { open: 1 }))
    );
}

/// A panel with a form asks first and puts the answers in the ticket; past 400 channels tickets are
/// private threads, which closing archives.
#[tokio::test]
async fn a_form_asks_first_and_big_servers_get_private_threads() {
    let Some((mut sim, world)) = ticket_sim("a_form_asks_first_and_big_servers_get_private_threads").await else {
        return;
    };
    let (admin, member) = (unique(), unique());
    let panel_channel = unique();
    DM_OVERRIDES.lock().unwrap().insert(member, false);
    let questions = [
        Question {
            label: "Your order number".into(),
            placeholder: "#1234".into(),
            long: false,
            required: true,
            choices: Vec::new(),
            upload: false,
        },
        Question {
            label: "What happened?".into(),
            placeholder: String::new(),
            long: true,
            required: false,
            choices: Vec::new(),
            upload: false,
        },
    ];
    save_panel(
        &sim,
        "Orders",
        panel_channel,
        &[Topic::new("Order problem")],
        &questions,
        &[],
    )
    .await;
    world.channel_count.store(450, Ordering::SeqCst);
    let panel = posted_panel(&mut sim, admin, "Orders", panel_channel).await;

    let form = sim.click(member, &panel, "Order problem").await;
    let (id, title) = form.modal.clone().expect("a form first");
    assert!(id.starts_with("tf:"), "{id}");
    assert_eq!(title, "Need help?");
    assert_eq!(form.modal_inputs.len(), 2);
    let opened = sim
        .submit_fields(member, &form, &[("q0", "#4242 <@&5>"), ("q1", "It arrived **broken**")])
        .await;
    let told = opened.followups.last().map_or(opened.content(), Reply::content);
    assert!(world.made.lock().unwrap().is_empty(), "no channel: a thread");
    let threads = world.threads.lock().unwrap().clone();
    assert_eq!(threads.len(), 1);
    let (parent, thread, body) = &threads[0];
    assert_eq!(*parent, panel_channel);
    assert_eq!(body["type"], 12, "private");
    assert!(told.contains(&format!("<#{thread}>")), "{told}");
    assert!(changes(&world).contains(&format!("PUT member {thread} {member}")));
    let posts = take_posts(&sim);
    let welcome = posts
        .iter()
        .find(|p| p.channel == *thread)
        .expect("welcome in the thread");
    assert_eq!(welcome.field("Your order number"), "\\#4242", "answers are scrubbed");
    assert_eq!(welcome.field("What happened?"), "It arrived \\*\\*broken\\*\\*");
    let ticket = sim.app.db.ticket_at(sim.guild, *thread).await.unwrap().expect("ticket");
    assert_eq!(ticket.answers().len(), 2);

    let added = unique();
    world
        .thread_members
        .lock()
        .unwrap()
        .entry(*thread)
        .or_default()
        .push(added);

    sim.channel = *thread;
    sim.command(member, "ticket", json!([sub("close", vec![])])).await;
    let changed = changes(&world);
    assert!(
        changed.contains(&format!("DELETE member {thread} {member}")),
        "{changed:?}"
    );
    assert!(
        changed.contains(&format!("DELETE member {thread} {added}")),
        "{changed:?}"
    );
    assert!(
        changed.iter().any(|c| c.starts_with(&format!("PATCH {thread}"))
            && c.contains("\"archived\":true")
            && c.contains("\"locked\":true")),
        "{changed:?}"
    );
    let stored = sim.app.db.transcript(ticket.id).await.unwrap().expect("kept");
    assert!(stored.lines.is_empty(), "an empty thread makes an empty transcript");
}

/// A deleted panel's button, and a bot that cannot make channels, both say exactly what is wrong and leave
/// nothing behind.
#[tokio::test]
async fn deleted_panels_and_missing_permissions_are_explained() {
    let Some((mut sim, world)) = ticket_sim("deleted_panels_and_missing_permissions_are_explained").await else {
        return;
    };
    let (admin, member) = (unique(), unique());
    let panel_channel = unique();
    save_panel(&sim, "Support", panel_channel, &[Topic::new("Help")], &[], &[]).await;
    let panel = posted_panel(&mut sim, admin, "Support", panel_channel).await;

    world.refuse_channels.store(true, Ordering::SeqCst);
    let refused = sim.click(member, &panel, "Help").await;
    let said = refused.followups.last().map_or(refused.content(), Reply::content);
    assert!(
        said.contains("Manage Channels") && said.contains("Manage Roles"),
        "{refused}"
    );
    let list = sim
        .app
        .db
        .ticket_list(sim.guild, &Default::default(), 10)
        .await
        .unwrap();
    assert!(list.is_empty(), "the half-made ticket was taken back");
    world.refuse_channels.store(false, Ordering::SeqCst);
    let retried = sim.click(member, &panel, "Help").await;
    let said = retried.followups.last().map_or(retried.content(), Reply::content);
    assert!(
        said.contains("Your ticket is open"),
        "no wait after a failure: {retried}"
    );
    let ticket = &sim
        .app
        .db
        .ticket_list(sim.guild, &Default::default(), 10)
        .await
        .unwrap()[0];
    assert_eq!(ticket.number, 2, "the refused one used number 1");

    sim.permissions = MANAGE_GUILD;
    let deleted = sim
        .command(
            admin,
            "tickets",
            sub_group("panel", "delete", vec![text_option("panel", "Support")]),
        )
        .await;
    assert!(deleted.content().contains("deleted"), "{deleted}");
    sim.permissions = 0;
    let gone = sim.click(unique(), &panel, "Help").await;
    assert!(gone.ephemeral && gone.content().contains("panel was deleted"), "{gone}");
    assert!(
        sim.app.db.ticket(ticket.id).await.unwrap().is_some(),
        "its tickets stay"
    );
    // Outside a ticket, /ticket says so.
    sim.channel = CHANNEL_ID;
    let nowhere = sim.command(member, "ticket", json!([sub("claim", vec![])])).await;
    assert!(
        nowhere.ephemeral && nowhere.content().contains("not a ticket"),
        "{nowhere}"
    );
}

/// The snowflake of a message sent at `at`.
fn snowflake_at(at: chrono::DateTime<chrono::Utc>) -> u64 {
    ((at.timestamp_millis() - 1_420_070_400_000) as u64) << 22
}

/// A ticket nobody wrote in closes by itself once the server's hours are up; one with a newer message
/// stays open.
#[tokio::test]
async fn idle_tickets_close_themselves() {
    let _ticker = TICKET_TICK_LOCK.lock().await;
    let Some((mut sim, world)) = ticket_sim("idle_tickets_close_themselves").await else {
        return;
    };
    let (admin, quiet_member, busy_member) = (unique(), unique(), unique());
    let panel_channel = unique();
    save_panel(&sim, "Support", panel_channel, &[Topic::new("Help")], &[], &[]).await;
    sim.permissions = MANAGE_GUILD;
    sim.command(
        admin,
        "tickets",
        json!([sub("settings", vec![int_option("auto_close_hours", 1)])]),
    )
    .await;
    let panel = posted_panel(&mut sim, admin, "Support", panel_channel).await;
    sim.click(quiet_member, &panel, "Help").await;
    sim.click(busy_member, &panel, "Help").await;
    let made = world.made.lock().unwrap().clone();
    let (quiet_place, busy_place) = (made[0].0, made[1].0);

    sim.app.shift_clock(chrono::Duration::hours(2));
    // The busy one had a message a moment ago.
    let recent = snowflake_at(sim.app.now() - chrono::Duration::minutes(5));
    sim.mock.channels.lock().unwrap().get_mut(&busy_place).unwrap()["last_message_id"] = json!(recent.to_string());
    let closed = tickets_tick(&sim.app).await.expect("tick");
    assert!(closed >= 1);
    let quiet = sim.app.db.ticket_at(sim.guild, quiet_place).await.unwrap().unwrap();
    let busy = sim.app.db.ticket_at(sim.guild, busy_place).await.unwrap().unwrap();
    assert!(!quiet.is_open(), "the quiet one closed");
    assert_eq!(quiet.close_reason.as_deref(), Some("Nobody wrote for 1 hour."));
    assert_eq!(quiet.closed_by, None, "the bot closed it");
    assert!(busy.is_open(), "the busy one stays open");
    let posts = take_posts(&sim);
    assert!(
        posts
            .iter()
            .any(|p| p.channel == quiet_place && p.description().contains("Closed by the bot"))
    );
    // A second tick has nothing left to do.
    tickets_tick(&sim.app).await.expect("tick");
    assert!(
        sim.app
            .db
            .ticket_at(sim.guild, busy_place)
            .await
            .unwrap()
            .unwrap()
            .is_open()
    );
}

/// Every `/community tickets` subcommand asks for Manage Server itself (a server can show the command to anyone), and
/// a refused one changes nothing.
#[tokio::test]
async fn ticket_admin_paths_refuse_members_without_manage_server() {
    let Some((mut sim, _world)) = ticket_sim("ticket_admin_paths_refuse_members_without_manage_server").await else {
        return;
    };
    let member = unique();
    let channel = unique();
    save_panel(&sim, "Support", channel, &[Topic::new("Help")], &[], &[]).await;
    let state = |sim: &Sim| {
        let (db, guild) = (sim.app.db.clone(), sim.guild);
        async move {
            format!(
                "{:?}\n{:?}",
                db.ticket_settings(guild).await.unwrap(),
                db.ticket_panels(guild).await.unwrap()
            )
        }
    };
    let before = state(&sim).await;
    let rows: Vec<(&str, Value)> = vec![
        (
            "panel create",
            sub_group(
                "panel",
                "create",
                vec![text_option("name", "Other"), channel_option("channel", channel)],
            ),
        ),
        (
            "panel edit",
            sub_group(
                "panel",
                "edit",
                vec![text_option("panel", "Support"), text_option("title", "Hacked")],
            ),
        ),
        (
            "panel post",
            sub_group("panel", "post", vec![text_option("panel", "Support")]),
        ),
        (
            "panel delete",
            sub_group("panel", "delete", vec![text_option("panel", "Support")]),
        ),
        ("panel list", sub_group("panel", "list", vec![])),
        (
            "settings",
            json!([sub("settings", vec![int_option("auto_close_hours", 5)])]),
        ),
    ];
    let all_but_manage = ((1u64 << 50) - 1) & !(MANAGE_GUILD | ADMINISTRATOR);
    for permissions in [0, all_but_manage] {
        sim.permissions = permissions;
        for (label, options) in &rows {
            let reply = sim
                .command_resolved(member, "tickets", options.clone(), resolved_channels(&[channel]))
                .await;
            assert!(
                reply.ephemeral && reply.content().contains("Manage Server"),
                "/community tickets {label} with {permissions}: {reply}"
            );
        }
    }
    assert_eq!(state(&sim).await, before, "a refused command changed something");
    assert!(take_posts(&sim).is_empty(), "nothing was posted");
}

/// Turkish managers and members read the panel, the controls and the refusals in Turkish.
#[tokio::test]
async fn tickets_speak_turkish() {
    let Some((mut sim, world)) = ticket_sim("tickets_speak_turkish").await else {
        return;
    };
    let (admin, member) = (unique(), unique());
    let panel_channel = unique();
    sim.app
        .set_guild_lang(sim.guild, Some(pinchora_core::i18n::Lang::Tr))
        .await
        .unwrap();
    sim.locale = "tr";
    save_panel(&sim, "Destek", panel_channel, &[Topic::new("Yardım")], &[], &[]).await;
    let panel = posted_panel(&mut sim, admin, "Destek", panel_channel).await;
    assert_eq!(
        panel.embeds[0].footer.as_ref().map(|f| f.text.as_str()),
        Some("Gizli bir talep açmak için bir konu seç.")
    );
    let opened = sim.click(member, &panel, "Yardım").await;
    let said = opened.followups.last().expect("told").content().to_owned();
    assert!(said.starts_with("Talebin açıldı"), "{said}");
    let place = world.made.lock().unwrap()[0].0;
    let posts = take_posts(&sim);
    let welcome = posted_reply(posts.iter().find(|p| p.channel == place).expect("welcome"));
    assert!(welcome.has_button("Kapat") && welcome.has_button("Üstlen"));
    sim.channel = place;
    let refused = sim.click(member, &welcome, "Üstlen").await;
    assert_eq!(refused.content(), "Bunu yalnızca talebin yetkilileri yapabilir.");
}

#[tokio::test]
async fn ticket_message_context_keeps_the_source_link() {
    let Some((mut sim, world)) = ticket_sim("ticket_message_context_keeps_the_source_link").await else {
        return;
    };
    let member = unique();
    let channel = unique();
    let source = unique();
    let panel = save_panel(&sim, "Context", channel, &[Topic::new("Help")], &[], &[]).await;
    sim.channel = channel;
    let mut message = message_json(channel, source);
    message["content"] = json!("");
    let data = json!({
        "id": unique().to_string(), "name": "Open ticket", "type": 3,
        "target_id": source.to_string(),
        "resolved": { "messages": { source.to_string(): message } },
        "options": []
    });
    let panel_menu = sim.run("message Open ticket".into(), member, 2, data, None).await;
    assert!(panel_menu.ephemeral);
    let topic_menu = sim.pick(member, &panel_menu, "tk:m", &panel.to_string()).await;
    let opened = sim.pick(member, &topic_menu, "tk:q", "0").await;
    assert!(
        opened
            .followups
            .last()
            .map_or(opened.content(), Reply::content)
            .contains("Your ticket is open")
    );
    let place = world.made.lock().unwrap()[0].0;
    let ticket = sim.app.db.ticket_at(sim.guild, place).await.unwrap().unwrap();
    assert_eq!(
        ticket.answers()[0].answer,
        format!("https://discord.com/channels/{}/{channel}/{source}", sim.guild)
    );
}

async fn open_multistep_fixture(sim: &mut Sim, world: &World, admin: u64, member: u64) -> u64 {
    let panel_channel = unique();
    let questions: Vec<Question> = (1..=6)
        .map(|n| Question {
            label: format!("Question {n}"),
            placeholder: String::new(),
            long: n == 6,
            required: true,
            choices: Vec::new(),
            upload: false,
        })
        .collect();
    let panel_id = save_panel(sim, "V2", panel_channel, &[Topic::new("Help")], &questions, &[]).await;
    let mut ops = sim.app.db.ticket_panel_operations(panel_id).await.unwrap();
    ops.faq.push(pinchora_db::TicketFaq {
        question: "Where are the hours?".into(),
        answer: "On the site.".into(),
    });
    ops.faq.push(pinchora_db::TicketFaq {
        question: "Where are updates?".into(),
        answer: "*".repeat(1000),
    });
    sim.app.db.save_ticket_panel_operations(panel_id, &ops).await.unwrap();
    let panel = posted_panel(sim, admin, "V2", panel_channel).await;
    let faq = sim.click(member, &panel, "Help").await;
    assert!(faq.ephemeral && faq.content().contains("Where are the hours?"));
    assert!(world.made.lock().unwrap().is_empty(), "FAQ deflects before opening");
    let next_faq = sim.click(member, &faq, "Next answer").await;
    assert!(next_faq.content().contains("Where are updates?"));
    assert!(next_faq.content().chars().count() <= 2000);
    let first = sim.click(member, &next_faq, "Continue").await;
    assert_eq!(first.modal_inputs.len(), 5);
    let invalid = sim
        .submit_fields(member, &first, &[("q0", "missing required fields")])
        .await;
    assert!(invalid.ephemeral && world.made.lock().unwrap().is_empty());
    let next = sim
        .submit_fields(
            member,
            &first,
            &[("q0", "A1"), ("q1", "A2"), ("q2", "A3"), ("q3", "A4"), ("q4", "A5")],
        )
        .await;
    assert!(next.content().contains("next step"));
    let second = sim.click(member, &next, "Continue").await;
    assert_eq!(second.modal_inputs.len(), 1);
    let stolen = sim.submit_fields(unique(), &second, &[("q0", "stolen")]).await;
    assert!(stolen.ephemeral && world.made.lock().unwrap().is_empty());
    let oversized = sim.submit_fields(member, &second, &[("q0", &"x".repeat(1001))]).await;
    assert!(oversized.ephemeral && world.made.lock().unwrap().is_empty());
    let opened = sim.submit_fields(member, &second, &[("q0", "A6")]).await;
    let replay = sim.submit_fields(member, &second, &[("q0", "replayed")]).await;
    assert!(replay.ephemeral);
    assert_eq!(world.made.lock().unwrap().len(), 1);
    assert!(
        opened
            .followups
            .last()
            .map_or(opened.content(), Reply::content)
            .contains("Your ticket is open")
    );
    world.made.lock().unwrap()[0].0
}

#[tokio::test]
async fn ticket_operations_faq_multistep_private_note_and_close_request() {
    let Some((mut sim, world)) = ticket_sim("ticket_operations_faq_multistep_private_note_and_close_request").await
    else {
        return;
    };
    let (admin, member, staff) = (unique(), unique(), unique());
    let place = open_multistep_fixture(&mut sim, &world, admin, member).await;
    let ticket = sim.app.db.ticket_at(sim.guild, place).await.unwrap().unwrap();
    assert_eq!(ticket.answers().len(), 6);
    assert!(
        take_posts(&sim)
            .iter()
            .any(|post| post.channel == place && post.body.to_string().contains("A6")),
        "staff see answers from later form steps"
    );
    sim.channel = place;
    sim.permissions = MANAGE_GUILD;
    let saved = sim
        .command(
            staff,
            "ticket",
            json!([sub("note", vec![text_option("text", "internal only")])]),
        )
        .await;
    assert!(saved.ephemeral && saved.content().contains("Private staff note saved"));
    assert!(
        take_posts(&sim)
            .iter()
            .all(|post| !post.body.to_string().contains("internal only"))
    );
    assert_eq!(
        sim.app.db.ticket_staff_notes(ticket.id).await.unwrap()[0].body,
        "internal only"
    );
    sim.app
        .db
        .save_ticket_snippet(
            sim.guild,
            "Welcome",
            "Hello {user} in ticket {number} @everyone <@&1234>",
        )
        .await
        .unwrap();
    let sent = sim
        .command(
            staff,
            "ticket",
            json!([sub("snippet", vec![text_option("name", "Welcome")])]),
        )
        .await;
    assert!(sent.content().contains("Saved reply sent"));
    assert!(take_posts(&sim).iter().any(|post| {
        post.channel == place
            && post.body["allowed_mentions"]["parse"] == json!([])
            && post.body.to_string().contains("ticket 1")
            && post.body.to_string().contains(&format!("<@{member}>"))
    }));
    let requested = sim
        .command(staff, "ticket", json!([sub("request-close", vec![])]))
        .await;
    assert!(requested.content().contains("Staff asked to close"));
    let posted = take_posts(&sim);
    let request = posted
        .iter()
        .find(|post| post.channel == place && post.body.to_string().contains("Confirm close"))
        .expect("close confirmation posted");
    sim.permissions = 0;
    let confirmed = sim.click(member, &posted_reply(request), "Confirm close").await;
    assert!(confirmed.content().contains("Closing"));
    assert!(!sim.app.db.ticket(ticket.id).await.unwrap().unwrap().is_open());
    let transcript = sim.app.db.transcript(ticket.id).await.unwrap().unwrap();
    assert!(
        !serde_json::to_string(&transcript.lines)
            .unwrap()
            .contains("internal only")
    );
}

#[tokio::test]
async fn ticket_operations_faq_and_support_hours_speak_turkish() {
    let Some((mut sim, world)) = ticket_sim("ticket_operations_faq_and_support_hours_speak_turkish").await else {
        return;
    };
    let (admin, member, panel_channel) = (unique(), unique(), unique());
    sim.app
        .set_guild_lang(sim.guild, Some(pinchora_core::i18n::Lang::Tr))
        .await
        .unwrap();
    sim.locale = "tr";
    let id = save_panel(&sim, "Yardım", panel_channel, &[Topic::new("Destek")], &[], &[]).await;
    let mut ops = sim.app.db.ticket_panel_operations(id).await.unwrap();
    ops.faq.push(pinchora_db::TicketFaq {
        question: "Saatler?".into(),
        answer: "Hafta içi.".into(),
    });
    sim.app.db.save_ticket_panel_operations(id, &ops).await.unwrap();
    let mut settings = sim.app.db.ticket_operation_settings(sim.guild).await.unwrap();
    settings.support_hours.push(pinchora_db::SupportWindow {
        day: 1,
        start: 1,
        end: 2,
    });
    settings.out_of_hours_message = "Yarın döneceğiz.".into();
    sim.app
        .db
        .save_ticket_operation_settings(sim.guild, &settings)
        .await
        .unwrap();
    let panel = posted_panel(&mut sim, admin, "Yardım", panel_channel).await;
    let faq = sim.click(member, &panel, "Destek").await;
    assert!(faq.content().contains("Talep açmadan önce"));
    assert!(world.made.lock().unwrap().is_empty());
    let opened = sim.click(member, &faq, "Devam et").await;
    assert!(
        opened
            .followups
            .last()
            .map_or(opened.content(), Reply::content)
            .contains("Talebin açıldı")
    );
    assert!(
        take_posts(&sim)
            .iter()
            .any(|post| post.body.to_string().contains("Yarın döneceğiz"))
    );
}

#[tokio::test]
async fn status_board_parallel_refreshes_are_throttled_without_pool_starvation() {
    let _ticker = TICKET_TICK_LOCK.lock().await;
    let Some((sim, world)) = ticket_sim("ticket_status_throttle").await else {
        return;
    };
    let channel = unique();
    sim.app
        .db
        .save_ticket_operation_settings(
            sim.guild,
            &pinchora_db::TicketOperationSettings {
                status_channel_id: Some(channel as i64),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let refreshes = (0..12).map(|_| pinchora_commands::refresh_ticket_status_board(&sim.app, sim.guild));
    let results = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        futures_util::future::join_all(refreshes),
    )
    .await
    .expect("parallel board refresh must not exhaust the connection pool");
    for result in results {
        result.unwrap();
    }
    let posts = take_posts(&sim);
    assert_eq!(posts.iter().filter(|p| p.channel == channel).count(), 1);
    pinchora_commands::refresh_ticket_status_board(&sim.app, sim.guild)
        .await
        .unwrap();
    assert!(take_posts(&sim).is_empty(), "edits also share the throttle");
    sim.app
        .cache
        .release_cooldown(&format!("ticket-status:{}", sim.guild))
        .await
        .unwrap();
    world.refuse_message_edits.store(true, Ordering::SeqCst);
    assert!(
        pinchora_commands::refresh_ticket_status_board(&sim.app, sim.guild)
            .await
            .is_err()
    );
    assert!(
        take_posts(&sim).is_empty(),
        "a failed edit must not create a duplicate board"
    );
    sim.app
        .db
        .save_ticket_operation_settings(sim.guild, &Default::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn ticket_staff_operations_deny_members_and_force_open_records_only_success() {
    let Some((mut sim, world)) = ticket_sim("ticket_staff_operations_guards").await else {
        return;
    };
    let (owner, member, staff, channel) = (unique(), unique(), unique(), unique());
    save_panel(&sim, "Guards", channel, &[Topic::new("Help")], &[], &[]).await;
    let panel = posted_panel(&mut sim, owner, "Guards", channel).await;
    sim.click(member, &panel, "Help").await;
    sim.channel = world.made.lock().unwrap()[0].0;
    let ticket = sim.app.db.ticket_at(sim.guild, sim.channel).await.unwrap().unwrap();
    for (action, options) in [
        ("priority", vec![text_option("level", "high")]),
        ("escalate", vec![]),
        ("request-close", vec![]),
        ("force-open", vec![]),
        ("note", vec![text_option("text", "private")]),
        ("transfer", vec![]),
        ("snippet", vec![text_option("name", "hello")]),
    ] {
        let result = sim.command(member, "ticket", json!([sub(action, options)])).await;
        assert!(
            result.ephemeral && result.content().contains("staff"),
            "{action}: {}",
            result.content()
        );
    }
    assert!(sim.app.db.ticket_staff_notes(ticket.id).await.unwrap().is_empty());
    sim.permissions = MANAGE_GUILD;
    sim.command(staff, "ticket", json!([sub("close", vec![])])).await;
    let reopened = sim.command(staff, "ticket", json!([sub("force-open", vec![])])).await;
    assert!(reopened.content().contains("reopened"), "{}", reopened.content());
    let operations = sim.app.db.ticket_operations(ticket.id).await.unwrap();
    assert_eq!(operations.force_opened_by, Some(staff as i64));
    sim.command(owner, "ticket", json!([sub("force-open", vec![])])).await;
    assert_eq!(
        sim.app.db.ticket_operations(ticket.id).await.unwrap().force_opened_by,
        Some(staff as i64)
    );
}

#[tokio::test]
async fn transfer_rejects_nonstaff_users_and_deleted_support_roles() {
    let Some((mut sim, _)) = ticket_sim("ticket_transfer_validation").await else {
        return;
    };
    let (staff, opener, deleted_role) = (unique(), unique(), unique());
    let panel = save_panel(
        &sim,
        "Transfer",
        sim.channel,
        &[Topic::new("Help")],
        &[],
        &[deleted_role],
    )
    .await;
    let TicketOpen::Opened(ticket) = sim
        .app
        .db
        .open_ticket(&NewTicket {
            guild_id: sim.guild,
            panel_id: Some(panel),
            opener,
            topic: "Help",
            answers: &[],
            max_open: 1,
            at: sim.app.now(),
        })
        .await
        .unwrap()
    else {
        panic!("opens")
    };
    sim.app.db.set_ticket_place(ticket.id, sim.channel, None).await.unwrap();
    sim.permissions = MANAGE_GUILD;
    let role = role_json(deleted_role, "Deleted support", 1, false);
    let response = sim
        .command_resolved(
            staff,
            "ticket",
            json!([sub("transfer", vec![role_option("role", &role)])]),
            json!({"roles": {deleted_role.to_string():role}}),
        )
        .await;
    assert!(response.ephemeral);
    assert!(
        sim.app
            .db
            .ticket_operations(ticket.id)
            .await
            .unwrap()
            .transferred_to_role
            .is_none()
    );
    sim.command(
        staff,
        "ticket",
        json!([sub("transfer", vec![user_option("user", opener)])]),
    )
    .await;
    assert!(
        sim.app
            .db
            .ticket(ticket.id)
            .await
            .unwrap()
            .unwrap()
            .claimed_by
            .is_none()
    );
}

#[tokio::test]
async fn ticket_claim_mode_message_guard_respects_the_module_switch() {
    let Some((sim, _)) = ticket_sim("ticket_claim_mode_module").await else {
        return;
    };
    let (staff, support) = (unique(), unique());
    let panel = save_panel(&sim, "Claim mode", sim.channel, &[Topic::new("Help")], &[], &[support]).await;
    sim.app
        .db
        .save_ticket_panel_operations(
            panel,
            &pinchora_db::TicketPanelOperations {
                claim_mode: "required".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let TicketOpen::Opened(ticket) = sim
        .app
        .db
        .open_ticket(&NewTicket {
            guild_id: sim.guild,
            panel_id: Some(panel),
            opener: unique(),
            topic: "Help",
            answers: &[],
            max_open: 1,
            at: sim.app.now(),
        })
        .await
        .unwrap()
    else {
        panic!("opens")
    };
    sim.app.db.set_ticket_place(ticket.id, sim.channel, None).await.unwrap();
    for enabled in [false, true] {
        sim.app
            .db
            .set_module(sim.guild, pinchora_core::modules::Module::Tickets, Some(enabled), None)
            .await
            .unwrap();
        sim.app.forget_guild_settings(sim.guild).await;
        let mut message = sim.chat(staff, &[support]);
        message.channel_id = Id::new(sim.channel);
        let id = message.id.get();
        on_message(sim.app.clone(), message).await.unwrap();
        assert_eq!(
            sim.mock
                .deleted_messages
                .lock()
                .unwrap()
                .iter()
                .any(|(_, deleted)| *deleted == id),
            enabled
        );
    }
}
