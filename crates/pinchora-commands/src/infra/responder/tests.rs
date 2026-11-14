use std::{
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{Method, Uri},
};
use pinchora_cache::Cache;
use pinchora_db::Db;
use serde_json::{Value, json};
use tokio::{net::TcpListener, sync::oneshot, time::sleep};
use twilight_http::Client;

use crate::infra::defer::defer_slow;
use crate::infra::responder::*;

/// One REST call the mock Discord saw.
struct Call {
    method: Method,
    path: String,
    body: Value,
}

type Calls = Arc<StdMutex<Vec<Call>>>;

async fn record(State(calls): State<Calls>, method: Method, uri: Uri, body: Bytes) -> &'static str {
    calls.lock().unwrap().push(Call {
        method,
        path: uri.path().to_owned(),
        body: serde_json::from_slice(&body).unwrap_or(Value::Null),
    });
    "{}"
}

/// The real [`App`] on the test databases with Discord replaced by a mock that writes down every call.
/// `None` without the databases, like the simulator.
struct Harness {
    app: Arc<App>,
    calls: Calls,
}

impl Harness {
    async fn new() -> Option<Self> {
        let (Ok(database), Ok(redis)) = (
            std::env::var("PINCHORA_TEST_DATABASE_URL"),
            std::env::var("PINCHORA_TEST_REDIS_URL"),
        ) else {
            eprintln!("skipped, set PINCHORA_TEST_DATABASE_URL and PINCHORA_TEST_REDIS_URL");
            return None;
        };
        let calls = Calls::default();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind the mock");
        let mock = listener.local_addr().expect("mock address");
        let router = Router::new().fallback(record).with_state(calls.clone());
        tokio::spawn(axum::serve(listener, router).into_future());
        let rest = Arc::new(
            Client::builder()
                .token("Bot test".into())
                .proxy(mock.to_string(), true)
                .ratelimiter(None)
                .build(),
        );
        crate::fixture_isolation::shared().await;
        let db = Db::connect_waiting(&database, 1, std::time::Duration::from_secs(60))
            .await
            .expect("connect to the database");
        let cache = Cache::connect(&redis).await.expect("connect to redis");
        let app = App::new(rest.clone(), rest, Id::new(1), db, cache, "test".into());
        Some(Self {
            app: Arc::new(app),
            calls,
        })
    }

    /// A gateway responder for `interaction`.
    fn responder(&self, interaction: &Interaction) -> Arc<Responder> {
        Arc::new(Responder::rest(self.app.clone(), interaction))
    }

    /// What Discord was asked to do so far: `callback <type>`, `edit` or `followup`, each
    /// followed by `ephemeral` when it was private.
    fn seen(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| {
                let flags = call.body["data"]["flags"]
                    .as_u64()
                    .or(call.body["flags"].as_u64())
                    .unwrap_or(0);
                let private = if flags & 64 != 0 { " ephemeral" } else { "" };
                if call.path.ends_with("/callback") {
                    format!("callback {}{private}", call.body["type"])
                } else if call.method == Method::PATCH {
                    format!("edit{private}")
                } else {
                    format!("followup{private}")
                }
            })
            .collect()
    }

    /// Waits until Discord has been asked `n` times.
    async fn wait_for(&self, n: usize) {
        for _ in 0..2_000 {
            if self.calls.lock().unwrap().len() >= n {
                return;
            }
            sleep(Duration::from_millis(5)).await;
        }
        panic!("only {:?} after waiting for {n} calls", self.seen());
    }
}

static NEXT: AtomicU64 = AtomicU64::new(1_000);

/// An interaction as Discord sends it; `message` is the one a component sits on.
fn interaction(kind: u8, data: Value, message: bool) -> Interaction {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut interaction = json!({
        "id": id.to_string(),
        "application_id": "1",
        "type": kind,
        "token": format!("test-{id}"),
        "version": 1,
        "guild_id": "5",
        "channel": { "id": "2", "type": 0, "guild_id": "5" },
        "locale": "en-US",
        "guild_locale": "en-US",
        "context": 0,
        "attachment_size_limit": 26_214_400,
        "authorizing_integration_owners": {},
        "entitlements": [],
        "member": {
            "user": { "id": "7", "username": "tester", "discriminator": "0", "global_name": "Tester", "avatar": null },
            "roles": [],
            "joined_at": "2024-01-01T00:00:00.000000+00:00",
            "deaf": false,
            "mute": false,
            "flags": 0,
            "permissions": "0",
        },
        "data": data,
    });
    if message {
        interaction["message"] = json!({
            "id": "3",
            "channel_id": "2",
            "type": 0,
            "author": { "id": "1", "username": "Pinchora", "discriminator": "0", "avatar": null, "bot": true },
            "content": "",
            "timestamp": "2024-01-01T00:00:00.000000+00:00",
            "edited_timestamp": null,
            "tts": false,
            "mention_everyone": false,
            "mentions": [],
            "mention_roles": [],
            "pinned": false,
            "attachments": [],
            "embeds": [],
            "components": [],
            "flags": 0,
        });
    }
    serde_json::from_value(interaction).expect("interaction JSON")
}

/// `/name` or, with `sub`, `/name sub`.
fn command(name: &str, sub: Option<&str>) -> Interaction {
    let options = sub.map_or_else(|| json!([]), |sub| json!([{ "name": sub, "type": 1, "options": [] }]));
    interaction(
        2,
        json!({ "id": "9", "name": name, "type": 1, "options": options }),
        false,
    )
}

/// A click on a button of a message.
fn click(custom_id: &str) -> Interaction {
    interaction(
        3,
        json!({ "custom_id": custom_id, "component_type": 2, "values": [] }),
        true,
    )
}

fn say(text: &str) -> InteractionResponseData {
    InteractionResponseData {
        content: Some(text.to_owned()),
        ..Default::default()
    }
}

fn say_privately(text: &str) -> InteractionResponseData {
    InteractionResponseData {
        flags: Some(MessageFlags::EPHEMERAL),
        ..say(text)
    }
}

/// Runs a handler that answers once `gate` opens, under a 20 ms timer, and gives back the handle.
/// The test sees Discord's side while the handler is still waiting.
fn slow(
    responder: &Arc<Responder>,
    answer: InteractionResponseData,
    update: bool,
) -> (oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let (open, gate) = oneshot::channel::<()>();
    let responder = responder.clone();
    let task = tokio::spawn(async move {
        let handler = async {
            let _ = gate.await;
            responder.respond(answer, update).await.expect("answer");
        };
        defer_slow(&responder, Duration::from_millis(20), handler).await;
    });
    (open, task)
}

#[tokio::test]
async fn a_fast_handler_is_answered_without_a_defer() {
    let Some(harness) = Harness::new().await else { return };
    let responder = harness.responder(&command("profile", None));
    let handler = async { responder.respond(say("quick"), false).await.expect("answer") };
    defer_slow(&responder, Duration::from_secs(5), handler).await;
    assert_eq!(harness.seen(), ["callback 4"]);
}

#[tokio::test]
async fn a_slow_handler_gets_one_defer_and_then_an_edit() {
    let Some(harness) = Harness::new().await else { return };
    let responder = harness.responder(&command("profile", None));
    assert_eq!(responder.deferral(), Deferral::Public);
    let (open, task) = slow(&responder, say("late"), false);
    harness.wait_for(1).await;
    assert_eq!(harness.seen(), ["callback 5"], "a public thinking message");
    open.send(()).unwrap();
    task.await.unwrap();
    assert_eq!(harness.seen(), ["callback 5", "edit"]);
}

#[tokio::test]
async fn a_slow_private_command_is_deferred_privately() {
    let Some(harness) = Harness::new().await else { return };
    let responder = harness.responder(&command("house", Some("edit")));
    assert_eq!(responder.deferral(), Deferral::Private);
    let (open, task) = slow(&responder, say_privately("late"), false);
    harness.wait_for(1).await;
    assert_eq!(harness.seen(), ["callback 5 ephemeral"]);
    open.send(()).unwrap();
    task.await.unwrap();
    assert_eq!(harness.seen(), ["callback 5 ephemeral", "edit"]);
}

#[tokio::test]
async fn a_slow_click_keeps_the_clicked_message() {
    let Some(harness) = Harness::new().await else { return };
    let responder = harness.responder(&click("qc:1"));
    let (open, task) = slow(&responder, say("updated"), true);
    harness.wait_for(1).await;
    assert_eq!(harness.seen(), ["callback 6"], "deferred update, nothing shown");
    open.send(()).unwrap();
    task.await.unwrap();
    assert_eq!(harness.seen(), ["callback 6", "edit"]);
}

#[tokio::test]
async fn a_slow_click_answers_a_private_notice_beside_the_message() {
    let Some(harness) = Harness::new().await else { return };
    let responder = harness.responder(&click("qc:1"));
    let (open, task) = slow(&responder, say_privately("Not yours."), false);
    harness.wait_for(1).await;
    open.send(()).unwrap();
    task.await.unwrap();
    assert_eq!(
        harness.seen(),
        ["callback 6", "followup ephemeral"],
        "an edit would put the notice on the clicked message for everyone"
    );
}

#[tokio::test]
async fn a_click_that_may_open_a_modal_is_never_deferred() {
    let Some(harness) = Harness::new().await else { return };
    let responder = harness.responder(&click("wo:g:daily:5"));
    assert_eq!(responder.deferral(), Deferral::Never);
    let handler = async {
        sleep(Duration::from_millis(80)).await;
        let form = InteractionResponseData {
            custom_id: Some("wo:m:daily:5".to_owned()),
            title: Some("Wordle guess".to_owned()),
            ..Default::default()
        };
        responder.modal(form).await.expect("modal");
    };
    defer_slow(&responder, Duration::from_millis(20), handler).await;
    assert_eq!(harness.seen(), ["callback 9"]);
}

/// The handler answers about when the timer fires, over and over: whichever gets the response
/// slot first, Discord must see exactly one first answer and exactly one message from it.
#[tokio::test]
async fn the_timer_and_the_answer_never_both_take_the_first_slot() {
    let Some(harness) = Harness::new().await else { return };
    let mut deferred = 0;
    for round in 0..40u64 {
        harness.calls.lock().unwrap().clear();
        let responder = harness.responder(&command("profile", None));
        let handler = async {
            sleep(Duration::from_millis(round / 2)).await;
            responder.respond(say("answer"), false).await.expect("answer");
        };
        defer_slow(&responder, Duration::from_millis(10), handler).await;
        let seen = harness.seen();
        match seen.iter().map(String::as_str).collect::<Vec<_>>()[..] {
            ["callback 4"] => {}
            ["callback 5", "edit"] => deferred += 1,
            _ => panic!("round {round}: {seen:?}"),
        }
    }
    eprintln!("the timer won {deferred} of 40 rounds");
}
