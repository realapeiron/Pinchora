//! Clicks through the real slash-command and button handlers without Discord.
//!
//! A mock Discord REST server on localhost stands in for `discord.com`. Anything a handler
//! sends over REST lands in the test, which includes every image: files cannot ride in the
//! HTTP-interaction callback, so the handler defers and then edits the message.
//!
//! Needs `PINCHORA_TEST_DATABASE_URL` and `PINCHORA_TEST_REDIS_URL`; skipped when either is
//! unset. Each step's images are saved as `target/sim/<test>/<step>-<filename>`; run with
//! `--nocapture` to read the steps as they happen.

use std::{
    cell::RefCell,
    collections::HashMap,
    fmt::{self, Write as _},
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock, Mutex, Once,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::SystemTime,
};

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri, header::CONTENT_TYPE},
};
use pinchora_cache::Cache;
use pinchora_commands::{App, Responder, handle_interaction, on_message};
use pinchora_db::Db;
use serde_json::{Value, json};
use tokio::{net::TcpListener, sync::oneshot};
use tracing::{
    Event, Level,
    field::{Field, Visit},
};
use tracing_subscriber::{
    EnvFilter, Layer,
    layer::{Context, SubscriberExt},
    util::SubscriberInitExt,
};
use twilight_http::Client;
use twilight_model::{
    application::interaction::Interaction,
    channel::message::{
        Component, Embed, MessageFlags,
        component::{Button, SelectMenu, SelectMenuOption},
    },
    gateway::payload::incoming::MessageCreate,
    http::interaction::{InteractionResponse, InteractionResponseType},
    id::Id,
};

const APPLICATION_ID: u64 = 1;
const CHANNEL_ID: u64 = 2;
/// The role the mock bot holds; its position is the ceiling for roles the bot can hand out.
const BOT_ROLE_ID: u64 = 9_000_000_000_001;
const BOT_ROLE_POSITION: i64 = 10;
/// Manage Server, as the permission bit Discord sends.
const MANAGE_GUILD: u64 = 1 << 5;

/// An image a handler sent: its filename and bytes.
type File = (String, Vec<u8>);

/// Ids unique per process and, through the clock, per run, so tests can share one database.
fn unique() -> u64 {
    static NEXT: LazyLock<AtomicU64> = LazyLock::new(|| {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        AtomicU64::new((nanos as u64) >> 4)
    });
    NEXT.fetch_add(1, Ordering::Relaxed)
}

// /Mock Discord

/// One webhook call a handler made: an edit of its first answer, or a follow-up message.
struct Call {
    method: Method,
    path: String,
    body: Value,
    files: Vec<File>,
}

type Calls = Arc<Mutex<Vec<Call>>>;

/// What the mock Discord remembers: the webhook calls, and a small server with roles and members.
#[derive(Clone)]
struct Mock {
    calls: Calls,
    /// Roles the server has besides the bot's own, as Discord's JSON.
    roles: Arc<Mutex<Vec<Value>>>,
    /// Roles each member holds; the role routes change them.
    member_roles: Arc<Mutex<HashMap<u64, Vec<u64>>>>,
    /// Role changes seen, as `PUT <member> <role>` or `DELETE <member> <role>`.
    role_calls: Arc<Mutex<Vec<String>>>,
    /// Whether role changes work. Off, Discord answers Missing Permissions.
    roles_allowed: Arc<AtomicBool>,
}

fn role_json(id: u64, name: &str, position: i64, managed: bool) -> Value {
    json!({
        "id": id.to_string(),
        "name": name,
        "color": 0,
        "colors": { "primary_color": 0, "secondary_color": null, "tertiary_color": null },
        "hoist": false,
        "managed": managed,
        "mentionable": false,
        "permissions": "0",
        "position": position,
        "flags": 0,
    })
}

fn user_json(id: u64) -> Value {
    json!({
        "id": id.to_string(),
        "username": format!("sim{id}"),
        "discriminator": "0",
        "global_name": "Sim",
        "avatar": null,
    })
}

fn member_json(user: u64, roles: &[u64]) -> Value {
    json!({
        "user": user_json(user),
        "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
        "joined_at": "2024-01-01T00:00:00.000000+00:00",
        "deaf": false,
        "mute": false,
        "flags": 0,
        "pending": false,
    })
}

/// Answers the REST routes the reward flows use: the server's roles, member lookups, role
/// changes and level-up posts. Records webhook calls and refuses everything else (user lookups,
/// the avatar CDN proxy).
async fn discord(
    State(mock): State<Mock>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, String) {
    let path = uri.path().to_owned();
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let number = |part: &str| part.parse::<u64>().unwrap_or(0);
    match (&method, parts.as_slice()) {
        (&Method::GET, [.., "guilds", _, "roles"]) => {
            let mut roles = vec![role_json(BOT_ROLE_ID, "Pinchora", BOT_ROLE_POSITION, true)];
            roles.extend(mock.roles.lock().unwrap().iter().cloned());
            return (StatusCode::OK, Value::Array(roles).to_string());
        }
        (&Method::GET, [.., "guilds", _, "members", user]) => {
            let user = number(user);
            let roles = if user == APPLICATION_ID {
                vec![BOT_ROLE_ID]
            } else {
                mock.member_roles
                    .lock()
                    .unwrap()
                    .get(&user)
                    .cloned()
                    .unwrap_or_default()
            };
            return (StatusCode::OK, member_json(user, &roles).to_string());
        }
        (&Method::PUT | &Method::DELETE, [.., "guilds", _, "members", user, "roles", role]) => {
            if !mock.roles_allowed.load(Ordering::SeqCst) {
                let refusal = json!({ "message": "Missing Permissions", "code": 50013 });
                return (StatusCode::FORBIDDEN, refusal.to_string());
            }
            let (user, role) = (number(user), number(role));
            mock.role_calls.lock().unwrap().push(format!("{method} {user} {role}"));
            let mut members = mock.member_roles.lock().unwrap();
            let held = members.entry(user).or_default();
            held.retain(|r| *r != role);
            if method == Method::PUT {
                held.push(role);
            }
            return (StatusCode::NO_CONTENT, String::new());
        }
        (&Method::POST, [.., "channels", _, "messages"]) => return (StatusCode::OK, "{}".to_owned()),
        _ => {}
    }
    if method == Method::GET || !path.contains("/webhooks/") {
        return (StatusCode::NOT_FOUND, "{}".to_owned());
    }
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let (body, files) = match content_type.split_once("boundary=") {
        Some((_, boundary)) => multipart(boundary, &body),
        None => (serde_json::from_slice(&body).unwrap_or(Value::Null), Vec::new()),
    };
    mock.calls.lock().unwrap().push(Call {
        method,
        path,
        body,
        files,
    });
    (StatusCode::OK, "{}".to_owned())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Splits twilight's `multipart/form-data` body into its `payload_json` and its files.
fn multipart(boundary: &str, body: &[u8]) -> (Value, Vec<File>) {
    let delimiter = format!("--{boundary}").into_bytes();
    let mut json = Value::Null;
    let mut files = Vec::new();
    let mut rest = body;
    while let Some(at) = find(rest, &delimiter) {
        rest = &rest[at + delimiter.len()..];
        let part = &rest[..find(rest, &delimiter).unwrap_or(rest.len())];
        let Some(part) = part.strip_prefix(b"\r\n") else {
            continue;
        };
        let Some(split) = find(part, b"\r\n\r\n") else { continue };
        let head = String::from_utf8_lossy(&part[..split]);
        let data = &part[split + 4..];
        let data = data.strip_suffix(b"\r\n").unwrap_or(data);
        if head.contains("name=\"payload_json\"") {
            json = serde_json::from_slice(data).unwrap_or(Value::Null);
        } else if let Some((_, name)) = head.split_once("filename=\"")
            && let Some((name, _)) = name.split_once('"')
        {
            files.push((name.to_owned(), data.to_vec()));
        }
    }
    (json, files)
}

// /Errors

thread_local! {
    /// `error!` events from this thread. The handlers swallow their errors into a generic
    /// reply, so a test reads the real one here. Tests run on one thread each.
    static ERRORS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct CaptureErrors;

struct Fields<'a>(&'a mut String);

impl Visit for Fields<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let _ = write!(self.0, "{}={value:?} ", field.name());
    }
}

impl<S: tracing::Subscriber> Layer<S> for CaptureErrors {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        if *event.metadata().level() == Level::ERROR {
            let mut line = String::new();
            event.record(&mut Fields(&mut line));
            ERRORS.with_borrow_mut(|errors| errors.push(format!("{}: {line}", event.metadata().target())));
        }
    }
}

fn init_logging() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
        let _ = tracing_subscriber::registry()
            .with(CaptureErrors)
            .with(tracing_subscriber::fmt::layer().with_test_writer().with_filter(filter))
            .try_init();
    });
}

// /Reply

/// A message as the user sees it after a step: what the handler answered, with its REST
/// edits applied.
#[derive(Clone)]
struct Reply {
    /// How the handler first answered; a defer means the real content came by REST.
    kind: InteractionResponseType,
    content: Option<String>,
    embeds: Vec<Embed>,
    components: Vec<Component>,
    ephemeral: bool,
    /// Images sent in this step.
    files: Vec<File>,
    /// Further messages the handler posted.
    followups: Vec<Reply>,
}

impl Reply {
    fn new(kind: InteractionResponseType) -> Self {
        Self {
            kind,
            content: None,
            embeds: Vec::new(),
            components: Vec::new(),
            ephemeral: false,
            files: Vec::new(),
            followups: Vec::new(),
        }
    }

    /// Applies what the handler sent; fields it left out stay as they were.
    fn edit(&mut self, content: Option<String>, embeds: Option<Vec<Embed>>, components: Option<Vec<Component>>) {
        self.content = content.or(self.content.take());
        self.embeds = embeds.unwrap_or(std::mem::take(&mut self.embeds));
        self.components = components.unwrap_or(std::mem::take(&mut self.components));
    }

    /// `first` is the answer that went back to Discord; `on` the message a button was
    /// clicked on, which an update edits in place.
    fn build(first: InteractionResponse, on: Option<&Reply>, calls: Vec<Call>) -> Self {
        let updates = matches!(
            first.kind,
            InteractionResponseType::UpdateMessage | InteractionResponseType::DeferredUpdateMessage
        );
        let mut reply = match on {
            Some(on) if updates => Self {
                kind: first.kind,
                files: Vec::new(),
                followups: Vec::new(),
                ..on.clone()
            },
            _ => Self::new(first.kind),
        };
        if let Some(data) = first.data {
            if let Some(flags) = data.flags {
                reply.ephemeral = flags.contains(MessageFlags::EPHEMERAL);
            }
            reply.edit(data.content, data.embeds, data.components);
            reply.files = data
                .attachments
                .into_iter()
                .flatten()
                .map(|a| (a.filename, a.file))
                .collect();
        }
        for call in calls {
            let body = &call.body;
            let embeds = body
                .get("embeds")
                .and_then(|v| serde_json::from_value::<Vec<Embed>>(v.clone()).ok());
            let components = body
                .get("components")
                .and_then(|v| serde_json::from_value::<Vec<Component>>(v.clone()).ok());
            let content = body.get("content").and_then(Value::as_str).map(str::to_owned);
            if call.method == Method::PATCH && call.path.ends_with("/messages/@original") {
                reply.edit(content, embeds, components);
                if body.get("attachments").is_some() {
                    reply.files = call.files;
                }
            } else {
                let mut followup = Self::new(InteractionResponseType::ChannelMessageWithSource);
                followup.ephemeral = body["flags"].as_u64().is_some_and(|f| f & 64 != 0);
                followup.edit(content, embeds, components);
                followup.files = call.files;
                reply.followups.push(followup);
            }
        }
        reply
    }

    fn title(&self) -> &str {
        self.embeds.first().and_then(|e| e.title.as_deref()).unwrap_or_default()
    }

    fn description(&self) -> &str {
        self.embeds
            .first()
            .and_then(|e| e.description.as_deref())
            .unwrap_or_default()
    }

    fn content(&self) -> &str {
        self.content.as_deref().unwrap_or_default()
    }

    /// The text of the first embed's field called `name`.
    fn field(&self, name: &str) -> &str {
        self.embeds
            .first()
            .and_then(|e| e.fields.iter().find(|f| f.name == name))
            .map_or("", |f| f.value.as_str())
    }

    /// Number of action rows.
    fn rows(&self) -> usize {
        self.components
            .iter()
            .filter(|c| matches!(c, Component::ActionRow(_)))
            .count()
    }

    /// Every button and menu, row by row.
    fn controls(&self) -> impl Iterator<Item = &Component> {
        self.components.iter().flat_map(|c| match c {
            Component::ActionRow(row) => row.components.iter(),
            other => std::slice::from_ref(other).iter(),
        })
    }

    fn button(&self, label: &str) -> &Button {
        self.controls()
            .find_map(|c| match c {
                Component::Button(b) if b.label.as_deref() == Some(label) => Some(b),
                _ => None,
            })
            .unwrap_or_else(|| {
                let labels: Vec<_> = self
                    .controls()
                    .filter_map(|c| match c {
                        Component::Button(b) => b.label.as_deref(),
                        _ => None,
                    })
                    .collect();
                panic!("no button labelled {label:?}; the message has {labels:?}")
            })
    }

    /// The select menu whose custom id starts with `prefix`.
    fn menu(&self, prefix: &str) -> &SelectMenu {
        self.controls()
            .find_map(|c| match c {
                Component::SelectMenu(m) if m.custom_id.starts_with(prefix) => Some(m),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no select menu with a custom id starting {prefix:?}"))
    }

    fn options(&self, prefix: &str) -> &[SelectMenuOption] {
        self.menu(prefix).options.as_deref().unwrap_or_default()
    }

    /// What a click on `custom_id` is, for the step heading.
    fn describe(&self, custom_id: &str, values: &[&str]) -> String {
        self.controls()
            .find_map(|c| match c {
                Component::Button(b) if b.custom_id.as_deref() == Some(custom_id) => {
                    Some(format!("click [{}]", b.label.as_deref().unwrap_or_default()))
                }
                Component::SelectMenu(m) if m.custom_id == custom_id => Some(format!(
                    "pick {values:?} in {:?}",
                    m.placeholder.as_deref().unwrap_or_default()
                )),
                _ => None,
            })
            .unwrap_or_else(|| format!("component {custom_id} {values:?}"))
    }

    /// The message as Discord attaches it to a component interaction.
    fn message_json(&self) -> Value {
        json!({
            "id": unique().to_string(),
            "channel_id": CHANNEL_ID.to_string(),
            "type": 0,
            "author": {
                "id": APPLICATION_ID.to_string(),
                "username": "Pinchora",
                "discriminator": "0",
                "avatar": null,
                "bot": true,
            },
            "content": self.content(),
            "timestamp": "2024-01-01T00:00:00.000000+00:00",
            "edited_timestamp": null,
            "tts": false,
            "mention_everyone": false,
            "mentions": [],
            "mention_roles": [],
            "pinned": false,
            "attachments": [],
            "embeds": self.embeds,
            "components": self.components,
            "flags": if self.ephemeral { 64 } else { 0 },
        })
    }
}

impl fmt::Display for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ephemeral = if self.ephemeral { ", ephemeral" } else { "" };
        writeln!(f, "  answer: {:?}{ephemeral}", self.kind)?;
        if let Some(content) = &self.content {
            writeln!(f, "  content: {content}")?;
        }
        for embed in &self.embeds {
            if let Some(title) = &embed.title {
                writeln!(f, "  title: {title}")?;
            }
            for line in embed.description.iter().flat_map(|d| d.lines()) {
                writeln!(f, "    {line}")?;
            }
        }
        for (row, component) in self.components.iter().enumerate() {
            writeln!(f, "  row {}", row + 1)?;
            let controls: &[Component] = match component {
                Component::ActionRow(row) => &row.components,
                other => std::slice::from_ref(other),
            };
            for control in controls {
                match control {
                    Component::Button(b) => writeln!(
                        f,
                        "    [{}] {}{}",
                        b.label.as_deref().unwrap_or_default(),
                        b.custom_id.as_deref().unwrap_or_default(),
                        if b.disabled { "  (disabled)" } else { "" }
                    )?,
                    Component::SelectMenu(m) => {
                        let options = m.options.as_deref().unwrap_or_default();
                        writeln!(
                            f,
                            "    select {:?} {}{}",
                            m.placeholder.as_deref().unwrap_or_default(),
                            m.custom_id,
                            if m.disabled { "  (disabled)" } else { "" }
                        )?;
                        for o in options.iter().take(8) {
                            let note = o.description.as_deref().map_or(String::new(), |d| format!("  ({d})"));
                            writeln!(f, "      {} = {}{note}", o.label, o.value)?;
                        }
                        if options.len() > 8 {
                            writeln!(f, "      ... {} more", options.len() - 8)?;
                        }
                    }
                    other => writeln!(f, "    {other:?}")?,
                }
            }
        }
        Ok(())
    }
}

// /Sim

/// The real [`App`] on the test databases, with Discord replaced by a local mock.
///
/// Built per test: the pools and the mock server belong to the runtime that made them.
struct Sim {
    app: Arc<App>,
    calls: Calls,
    mock: Mock,
    guild: u64,
    /// Permission bits the invoking member has, as Discord computes them for the channel.
    permissions: u64,
    /// Roles the invoking member holds.
    roles: Vec<u64>,
    name: &'static str,
    dir: PathBuf,
    step: usize,
}

impl Sim {
    async fn new(name: &'static str) -> Option<Self> {
        let (Ok(database), Ok(redis)) = (
            std::env::var("PINCHORA_TEST_DATABASE_URL"),
            std::env::var("PINCHORA_TEST_REDIS_URL"),
        ) else {
            eprintln!("{name}: skipped, set PINCHORA_TEST_DATABASE_URL and PINCHORA_TEST_REDIS_URL");
            return None;
        };
        init_logging();
        let db = Db::connect(&database, 5).await.expect("connect to the database");
        db.migrate().await.expect("migrate");
        let cache = Cache::connect(&redis).await.expect("connect to redis");

        let calls = Calls::default();
        let state = Mock {
            calls: calls.clone(),
            roles: Arc::default(),
            member_roles: Arc::default(),
            role_calls: Arc::default(),
            roles_allowed: Arc::new(AtomicBool::new(true)),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind the mock");
        let mock = listener.local_addr().expect("mock address");
        let router = Router::new().fallback(discord).with_state(state.clone());
        tokio::spawn(axum::serve(listener, router).into_future());
        let rest = Arc::new(
            Client::builder()
                .token("Bot sim".into())
                .proxy(mock.to_string(), true)
                .ratelimiter(None)
                .build(),
        );
        let mut app = App::new(rest.clone(), rest, Id::new(APPLICATION_ID), db, cache, "sim".into());
        // Avatars come from Discord's CDN. The mock refuses the tunnel, so cards get the
        // placeholder avatar, offline and the same every run.
        app.web = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{mock}")).expect("proxy"))
            .build()
            .expect("http client");

        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .parent()
            .expect("target dir")
            .join("sim")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the output dir");
        Some(Self {
            app: Arc::new(app),
            calls,
            mock: state,
            guild: unique(),
            permissions: 0,
            roles: Vec::new(),
            name,
            dir,
            step: 0,
        })
    }

    /// Runs a slash command as `user`. `options` is Discord's `data.options` array, e.g.
    /// `json!([{"name": "edit", "type": 1}])` for `/house edit`.
    async fn command(&mut self, user: u64, name: &str, options: Value) -> Reply {
        let data = json!({ "id": unique().to_string(), "name": name, "type": 1, "options": options });
        self.run(format!("/{name} {options}"), user, 2, data, None).await
    }

    /// Like [`Sim::command`] for options that name users or roles: `resolved` is Discord's
    /// `data.resolved`, e.g. `json!({ "users": {...}, "roles": {...} })`.
    async fn command_resolved(&mut self, user: u64, name: &str, options: Value, resolved: Value) -> Reply {
        let data = json!({
            "id": unique().to_string(),
            "name": name,
            "type": 1,
            "options": options,
            "resolved": resolved,
        });
        self.run(format!("/{name} {options}"), user, 2, data, None).await
    }

    /// Adds a role to the mock server and returns its JSON for `data.resolved.roles`.
    fn guild_role(&self, id: u64, name: &str, position: i64) -> Value {
        let role = role_json(id, name, position, false);
        self.mock.roles.lock().unwrap().push(role.clone());
        role
    }

    /// Switches role changes on or off in the mock (off: Missing Permissions).
    fn allow_role_changes(&self, allowed: bool) {
        self.mock.roles_allowed.store(allowed, Ordering::SeqCst);
    }

    /// Role changes the mock accepted since the last call, as `PUT <member> <role>` or `DELETE ...`.
    fn role_changes(&self) -> Vec<String> {
        std::mem::take(&mut *self.mock.role_calls.lock().unwrap())
    }

    /// A chat message from `user` carrying the roles they hold, as the gateway delivers it.
    fn chat(&self, user: u64, roles: &[u64]) -> MessageCreate {
        let message = json!({
            "id": unique().to_string(),
            "channel_id": CHANNEL_ID.to_string(),
            "guild_id": self.guild.to_string(),
            "type": 0,
            "author": user_json(user),
            "member": {
                "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
                "joined_at": "2024-01-01T00:00:00.000000+00:00",
                "deaf": false,
                "mute": false,
                "flags": 0,
            },
            "content": "hello",
            "timestamp": "2024-01-01T00:00:00.000000+00:00",
            "edited_timestamp": null,
            "tts": false,
            "mention_everyone": false,
            "mention_roles": [],
            "mentions": [],
            "pinned": false,
            "attachments": [],
            "embeds": [],
            "flags": 0,
        });
        serde_json::from_value(message).expect("message JSON")
    }

    /// Delivers a chat message to the XP handler, which must not fail.
    async fn say(&self, user: u64, roles: &[u64]) {
        ERRORS.with_borrow_mut(Vec::clear);
        on_message(self.app.clone(), self.chat(user, roles))
            .await
            .expect("the message handler failed");
        println!("\n== {}: {user} chatted", self.name);
    }

    /// Clicks a button (no `values`) or picks from a select menu on the message `on`.
    async fn component(&mut self, user: u64, custom_id: &str, values: &[&str], on: &Reply) -> Reply {
        let data = json!({
            "custom_id": custom_id,
            "component_type": if values.is_empty() { 2 } else { 3 },
            "values": values,
        });
        let label = on.describe(custom_id, values);
        self.run(label, user, 3, data, Some(on)).await
    }

    /// Clicks the button labelled `label`, as long as the message offers it enabled.
    async fn click(&mut self, user: u64, on: &Reply, label: &str) -> Reply {
        let button = on.button(label);
        assert!(
            !button.disabled,
            "button {label:?} is disabled, so nobody could click it"
        );
        let id = button.custom_id.clone().expect("button without a custom id");
        self.component(user, &id, &[], on).await
    }

    /// Picks `value` from the select menu whose custom id starts with `menu`.
    async fn pick(&mut self, user: u64, on: &Reply, menu: &str, value: &str) -> Reply {
        let select = on.menu(menu);
        assert!(!select.disabled, "select {menu:?} is disabled");
        assert!(
            on.options(menu).iter().any(|o| o.value == value),
            "select {menu:?} does not offer {value:?}"
        );
        self.component(user, &select.custom_id.clone(), &[value], on).await
    }

    fn interaction(&self, user: u64, kind: u8, data: Value, on: Option<&Reply>) -> Interaction {
        let id = unique();
        let mut interaction = json!({
            "id": id.to_string(),
            "application_id": APPLICATION_ID.to_string(),
            "type": kind,
            "token": format!("sim-{id}"),
            "version": 1,
            "guild_id": self.guild.to_string(),
            "channel": { "id": CHANNEL_ID.to_string(), "type": 0, "guild_id": self.guild.to_string() },
            "locale": "en-US",
            "guild_locale": "en-US",
            "context": 0,
            "attachment_size_limit": 26_214_400,
            "authorizing_integration_owners": {},
            "entitlements": [],
            "member": {
                "user": {
                    "id": user.to_string(),
                    "username": format!("sim{user}"),
                    "discriminator": "0",
                    "global_name": "Sim",
                    "avatar": null,
                },
                "roles": self.roles.iter().map(u64::to_string).collect::<Vec<_>>(),
                "joined_at": "2024-01-01T00:00:00.000000+00:00",
                "deaf": false,
                "mute": false,
                "flags": 0,
                "permissions": self.permissions.to_string(),
            },
            "data": data,
        });
        if let Some(on) = on {
            interaction["message"] = on.message_json();
        }
        serde_json::from_value(interaction).expect("interaction JSON")
    }

    async fn run(&mut self, label: String, user: u64, kind: u8, data: Value, on: Option<&Reply>) -> Reply {
        self.step += 1;
        let step = self.step;
        let interaction = self.interaction(user, kind, data, on);
        self.calls.lock().unwrap().clear();
        ERRORS.with_borrow_mut(Vec::clear);

        let (tx, mut rx) = oneshot::channel();
        let responder = Arc::new(Responder::http(self.app.clone(), &interaction, tx));
        handle_interaction(self.app.clone(), interaction, responder).await;

        let errors = ERRORS.take();
        assert!(
            errors.is_empty(),
            "step {step} ({label}): the handler failed:\n{}",
            errors.join("\n")
        );
        let first = rx
            .try_recv()
            .unwrap_or_else(|_| panic!("step {step} ({label}): the handler never answered"));
        let calls = std::mem::take(&mut *self.calls.lock().unwrap());
        let reply = Reply::build(first, on, calls);
        self.report(step, &label, &reply);
        reply
    }

    /// Prints the step and saves its images.
    fn report(&self, step: usize, label: &str, reply: &Reply) {
        println!("\n== {} step {step:02}: {label}", self.name);
        print!("{reply}");
        for (n, message) in std::iter::once(reply).chain(&reply.followups).enumerate() {
            if n > 0 {
                println!("  follow-up {n}:\n{message}");
            }
            for (filename, bytes) in &message.files {
                let prefix = if n == 0 { String::new() } else { format!("followup{n}-") };
                let path = self.dir.join(format!("{step:02}-{prefix}{filename}"));
                std::fs::write(&path, bytes).expect("save the image");
                let image = image::load_from_memory(bytes)
                    .unwrap_or_else(|e| panic!("step {step}: {filename} is not a valid image: {e}"));
                println!(
                    "  image: {} ({}x{}, {} KB)",
                    path.display(),
                    image.width(),
                    image.height(),
                    bytes.len() / 1024
                );
            }
        }
    }
}

// /Tests

#[tokio::test]
async fn house_editor_flow() {
    let Some(mut sim) = Sim::new("house_editor_flow").await else {
        return;
    };
    let user = unique();

    // /house edit: a fresh player gets a house with a starter kit.
    let edit = sim.command(user, "house", json!([{ "name": "edit", "type": 1 }])).await;
    assert_eq!(edit.rows(), 5, "two menus, arrows, actions, cursor modes");
    assert!(edit.title().starts_with("Editing room 1"), "title: {}", edit.title());
    assert!(
        edit.description().contains("Holding: **nothing**"),
        "{}",
        edit.description()
    );
    assert!(edit.button("Place").disabled, "nothing held, nothing to place");
    assert_eq!(edit.files.len(), 1, "the room picture");

    // Pick the first piece of furniture; it jumps to a free spot.
    let furniture = edit
        .options("he:s:")
        .iter()
        .find(|o| !o.value.starts_with('<') && o.value != "none")
        .expect("the starter kit is in the inventory")
        .value
        .clone();
    let held = sim.pick(user, &edit, "he:s:", &furniture).await;
    assert!(held.description().contains("Holding **"), "{}", held.description());
    assert!(!held.button("Place").disabled, "{}", held.description());

    let placed = sim.click(user, &held, "Place").await;
    assert!(placed.description().contains("Placed **"), "{}", placed.description());

    // An empty hand lists the room's pieces (`#id`); pick one and take it up with Move.
    let piece = placed
        .options("he:j:")
        .first()
        .expect("the placed piece is listed")
        .value
        .clone();
    assert!(piece.starts_with('#'), "piece value {piece:?}");
    let picked = sim.pick(user, &placed, "he:j:", &piece).await;
    assert!(
        picked.description().contains("Under cursor:"),
        "{}",
        picked.description()
    );
    let moving = sim.click(user, &picked, "Move").await;
    assert!(moving.description().contains("Moving **"), "{}", moving.description());

    // Now the same menu offers free spots (`x.y`); go to the furthest one and place it there.
    let spot = moving
        .options("he:j:")
        .last()
        .expect("a free spot for the held piece")
        .value
        .clone();
    assert!(spot.contains('.') && !spot.starts_with('#'), "spot value {spot:?}");
    let at_spot = sim.pick(user, &moving, "he:j:", &spot).await;
    assert!(
        at_spot.description().contains("Holding: **"),
        "{}",
        at_spot.description()
    );
    assert!(!at_spot.button("Place").disabled, "{}", at_spot.description());
    let placed_again = sim.click(user, &at_spot, "Place").await;
    assert!(
        placed_again.description().contains("Placed **"),
        "{}",
        placed_again.description()
    );

    // The cursor stayed on the piece, so Store takes it back.
    let stored = sim.click(user, &placed_again, "Store").await;
    assert!(stored.description().contains("Stored **"), "{}", stored.description());

    let done = sim.click(user, &stored, "Done").await;
    assert!(done.content().contains("Saved!"), "{}", done.content());
    assert!(done.components.is_empty() && done.embeds.is_empty());
}

#[tokio::test]
async fn profile_and_daily() {
    let Some(mut sim) = Sim::new("profile_and_daily").await else {
        return;
    };
    let user = unique();

    let daily = sim.command(user, "daily", json!([])).await;
    assert!(daily.title().ends_with("shells"), "title: {}", daily.title());
    assert!(daily.description().contains("Day **1**"), "{}", daily.description());

    let profile = sim.command(user, "profile", json!([])).await;
    assert_eq!(profile.files.len(), 1);
    assert_eq!(profile.files[0].0, "profile.png");
    assert!(sim.app.db.user(user).await.expect("user row").credits > 0);
}

/// `data.resolved` for options that name `users` and `roles` (as made by [`Sim::guild_role`]).
fn resolved(users: &[u64], roles: &[&Value]) -> Value {
    json!({
        "users": users.iter().map(|u| (u.to_string(), user_json(*u))).collect::<serde_json::Map<_, _>>(),
        "roles": roles
            .iter()
            .map(|r| (r["id"].as_str().expect("role id").to_owned(), (*r).clone()))
            .collect::<serde_json::Map<_, _>>(),
    })
}

fn user_option(name: &str, id: u64) -> Value {
    json!({ "name": name, "type": 6, "value": id.to_string() })
}

fn role_option(name: &str, role: &Value) -> Value {
    json!({ "name": name, "type": 8, "value": role["id"] })
}

fn int_option(name: &str, value: i64) -> Value {
    json!({ "name": name, "type": 4, "value": value })
}

fn text_option(name: &str, value: &str) -> Value {
    json!({ "name": name, "type": 3, "value": value })
}

fn sub(name: &str, options: Vec<Value>) -> Value {
    json!({ "name": name, "type": 1, "options": options })
}

fn group(name: &str, sub: Value) -> Value {
    json!({ "name": name, "type": 2, "options": [sub] })
}

#[tokio::test]
async fn level_roles_follow_level_ups() {
    let Some(mut sim) = Sim::new("level_roles_follow_level_ups").await else {
        return;
    };
    let (admin, early, late, middle) = (unique(), unique(), unique(), unique());
    let (r1, r3) = (unique(), unique());
    let novice = sim.guild_role(r1, "Novice", 2);
    let veteran = sim.guild_role(r3, "Reef Veteran", 3);
    sim.permissions = MANAGE_GUILD;

    // Rewards: level 1 and level 3. A role above the bot, a managed one and @everyone are refused.
    let add = |level: i64, role: &Value| {
        json!([group(
            "level-roles",
            sub("add", vec![int_option("level", level), role_option("role", role)])
        )])
    };
    let reply = sim
        .command_resolved(admin, "config", add(1, &novice), resolved(&[], &[&novice]))
        .await;
    assert!(reply.content().contains("level **1**"), "{}", reply.content());
    let reply = sim
        .command_resolved(admin, "config", add(3, &veteran), resolved(&[], &[&veteran]))
        .await;
    assert!(reply.content().contains("level **3**"), "{}", reply.content());
    let above = role_json(unique(), "Boss", BOT_ROLE_POSITION + 5, false);
    let reply = sim
        .command_resolved(admin, "config", add(5, &above), resolved(&[], &[&above]))
        .await;
    assert!(reply.content().contains("above my highest role"), "{}", reply.content());
    let managed = role_json(unique(), "SomeBot", 1, true);
    let reply = sim
        .command_resolved(admin, "config", add(5, &managed), resolved(&[], &[&managed]))
        .await;
    assert!(
        reply.content().contains("managed by an integration"),
        "{}",
        reply.content()
    );
    let everyone = role_json(sim.guild, "@everyone", 0, false);
    let reply = sim
        .command_resolved(admin, "config", add(5, &everyone), resolved(&[], &[&everyone]))
        .await;
    assert!(reply.content().contains("@everyone"), "{}", reply.content());

    let list = sim
        .command(admin, "config", json!([group("level-roles", sub("list", vec![]))]))
        .await;
    assert!(
        list.description().contains(&format!("Level **1** · <@&{r1}>"))
            && list.description().contains(&format!("Level **3** · <@&{r3}>")),
        "{}",
        list.description()
    );

    // Stacking: reaching level 1 gives the level 1 role.
    sim.app.db.add_message_xp(sim.guild, early, 99, 0).await.unwrap();
    sim.say(early, &[]).await;
    assert_eq!(sim.role_changes(), vec![format!("PUT {early} {r1}")]);

    // Replacing: reaching level 3 gives that role and takes the lower one.
    let reply = sim
        .command(
            admin,
            "config",
            json!([group("level-roles", sub("mode", vec![text_option("mode", "replace")]))]),
        )
        .await;
    assert!(reply.content().contains("replace"), "{}", reply.content());
    sim.app.db.add_message_xp(sim.guild, late, 460, 0).await.unwrap();
    sim.say(late, &[r1]).await;
    assert_eq!(
        sim.role_changes(),
        vec![format!("PUT {late} {r3}"), format!("DELETE {late} {r1}")]
    );

    // A refusal from Discord is logged, not fatal: the member still levels up.
    sim.allow_role_changes(false);
    sim.app.db.add_message_xp(sim.guild, middle, 99, 0).await.unwrap();
    sim.say(middle, &[]).await;
    assert!(sim.role_changes().is_empty());
    sim.allow_role_changes(true);

    // Sync catches up members who levelled before the roles existed: `middle` is past level 1
    // and holds nothing; `early` and `late` already hold the right role.
    let sync = sim
        .command(admin, "config", json!([group("level-roles", sub("sync", vec![]))]))
        .await;
    assert!(
        sync.content().contains("Checked **3** members") && sync.content().contains("updated the level roles of **1**"),
        "{}",
        sync.content()
    );
    assert_eq!(sim.role_changes(), vec![format!("PUT {middle} {r1}")]);

    // /rank shows the next reward.
    let rank = sim.command(early, "rank", json!([])).await;
    assert_eq!(rank.files.len(), 1, "the rank card");

    // Removing a reward stops it being given.
    let reply = sim
        .command(
            admin,
            "config",
            json!([group("level-roles", sub("remove", vec![role_option("role", &veteran)]))]),
        )
        .await;
    assert!(
        reply.content().contains("no longer a level role"),
        "{}",
        reply.content()
    );
    assert_eq!(sim.app.db.level_roles(sim.guild).await.unwrap().len(), 1);
}

#[tokio::test]
async fn points_and_store_flow() {
    let Some(mut sim) = Sim::new("points_and_store_flow").await else {
        return;
    };
    let (admin, buyer) = (unique(), unique());
    let (vip_id, pricey_id) = (unique(), unique());
    let vip = sim.guild_role(vip_id, "VIP", 2);
    let pricey = sim.guild_role(pricey_id, "Whale", 3);

    // /config points renames the currency; an upside-down range is refused.
    sim.permissions = MANAGE_GUILD;
    let config = sim
        .command(
            admin,
            "config",
            json!([sub(
                "points",
                vec![
                    text_option("name", "coins"),
                    text_option("emoji", "🪙"),
                    int_option("min", 2),
                    int_option("max", 4),
                ]
            )]),
        )
        .await;
    let line = config.field("Server points");
    assert!(line.contains("🪙 **coins**") && line.contains("2 to 4"), "{line}");
    let refused = sim
        .command(admin, "config", json!([sub("points", vec![int_option("min", 9)])]))
        .await;
    assert!(refused.content().contains("can't be above"), "{}", refused.content());

    // Only managers hand out points.
    let give = |user: u64, amount: i64| {
        json!([sub(
            "give",
            vec![user_option("user", user), int_option("amount", amount)]
        )])
    };
    sim.permissions = 0;
    let denied = sim
        .command_resolved(buyer, "points", give(buyer, 100), resolved(&[buyer], &[]))
        .await;
    assert!(denied.content().contains("Manage Server"), "{}", denied.content());
    sim.permissions = MANAGE_GUILD;
    let given = sim
        .command_resolved(admin, "points", give(buyer, 100), resolved(&[buyer], &[]))
        .await;
    assert!(
        given.description().contains("Gave **100** 🪙 coins"),
        "{}",
        given.description()
    );
    let taken = sim
        .command_resolved(
            admin,
            "points",
            json!([sub("take", vec![user_option("user", buyer), int_option("amount", 5)])]),
            resolved(&[buyer], &[]),
        )
        .await;
    assert!(taken.description().contains("Took **5**") && taken.description().contains("**95** left"));
    sim.permissions = 0;
    let shown = sim.command(buyer, "points", json!([sub("show", vec![])])).await;
    assert_eq!(shown.title(), "🪙 Coins");
    assert!(
        shown.description().contains("**95** 🪙 coins"),
        "{}",
        shown.description()
    );

    // /leaderboard has a points board.
    let board = sim
        .command(buyer, "leaderboard", json!([text_option("board", "points")]))
        .await;
    assert_eq!(board.files.len(), 1, "the points board card");

    // Store setup: only roles the bot can hand out.
    sim.permissions = MANAGE_GUILD;
    let sell = |role: &Value, price: i64, stock: Option<i64>| {
        let mut options = vec![role_option("role", role), int_option("price", price)];
        options.extend(stock.map(|s| int_option("stock", s)));
        json!([group("store", sub("add", options))])
    };
    let above = role_json(unique(), "Boss", BOT_ROLE_POSITION, false);
    let refused = sim
        .command_resolved(admin, "config", sell(&above, 10, None), resolved(&[], &[&above]))
        .await;
    assert!(
        refused.content().contains("above my highest role"),
        "{}",
        refused.content()
    );
    let added = sim
        .command_resolved(admin, "config", sell(&vip, 60, Some(1)), resolved(&[], &[&vip]))
        .await;
    assert!(
        added.content().contains(&format!("<@&{vip_id}>"))
            && added.content().contains("**60** 🪙")
            && added.content().contains("1 left"),
        "{}",
        added.content()
    );
    sim.command_resolved(admin, "config", sell(&pricey, 500, None), resolved(&[], &[&pricey]))
        .await;

    // /store lists both and offers a menu.
    sim.permissions = 0;
    let store = sim.command(buyer, "store", json!([])).await;
    assert_eq!(store.title(), "Server store");
    assert!(
        store
            .description()
            .contains(&format!("<@&{vip_id}> · **60** 🪙 · 1 left"))
            && store.description().contains(&format!("<@&{pricey_id}> · **500** 🪙")),
        "{}",
        store.description()
    );
    let vip_item = store
        .options("st")
        .iter()
        .find(|o| o.label == "VIP")
        .expect("VIP is on sale")
        .value
        .clone();
    let pricey_item = store
        .options("st")
        .iter()
        .find(|o| o.label == "Whale")
        .expect("Whale is on sale")
        .value
        .clone();

    // Discord refuses the role: the purchase is undone and says why.
    sim.allow_role_changes(false);
    let failed = sim.pick(buyer, &store, "st", &vip_item).await;
    assert!(
        failed.content().contains("I couldn't give you")
            && failed.content().contains("You weren't charged")
            && failed.content().contains("**95**"),
        "{}",
        failed.content()
    );
    assert_eq!(
        sim.app.db.member_points(sim.guild, buyer).await.unwrap().0,
        95,
        "refunded"
    );
    assert_eq!(
        sim.app
            .db
            .store_item(sim.guild, vip_item.parse().unwrap())
            .await
            .unwrap()
            .unwrap()
            .stock,
        Some(1)
    );

    // Not enough points for the pricey one.
    let poor = sim.pick(buyer, &store, "st", &pricey_item).await;
    assert!(
        poor.content().contains("costs **500**") && poor.content().contains("you have **95**"),
        "{}",
        poor.content()
    );

    // Discord accepts: the points go, the role arrives, the store shows the last unit gone.
    sim.allow_role_changes(true);
    let bought = sim.pick(buyer, &store, "st", &vip_item).await;
    let receipt = bought.followups.first().expect("a private receipt");
    assert!(receipt.ephemeral);
    assert!(
        receipt.description().contains("You bought")
            && receipt.description().contains("**60** 🪙 coins")
            && receipt.description().contains("**35** left"),
        "{}",
        receipt.description()
    );
    assert!(bought.description().contains("sold out"), "{}", bought.description());
    assert_eq!(sim.role_changes(), vec![format!("PUT {buyer} {vip_id}")]);
    assert_eq!(sim.app.db.member_points(sim.guild, buyer).await.unwrap().0, 35);

    // A member who already has the role is turned away without paying.
    sim.permissions = MANAGE_GUILD;
    let restocked = sell(&vip, 20, Some(5));
    sim.command_resolved(admin, "config", restocked, resolved(&[], &[&vip]))
        .await;
    sim.permissions = 0;
    sim.roles = vec![vip_id];
    let store = sim.command(buyer, "store", json!([])).await;
    let again = sim.pick(buyer, &store, "st", &vip_item).await;
    assert!(again.content().contains("already have"), "{}", again.content());
    assert_eq!(sim.app.db.member_points(sim.guild, buyer).await.unwrap().0, 35);
    assert!(sim.role_changes().is_empty());

    // /config store remove takes it off sale.
    sim.permissions = MANAGE_GUILD;
    let removed = sim
        .command(
            admin,
            "config",
            json!([group("store", sub("remove", vec![role_option("role", &pricey)]))]),
        )
        .await;
    assert!(removed.content().contains("off sale"), "{}", removed.content());
    let list = sim
        .command(admin, "config", json!([group("store", sub("list", vec![]))]))
        .await;
    assert!(
        !list.description().contains(&format!("<@&{pricey_id}>")),
        "{}",
        list.description()
    );
}
