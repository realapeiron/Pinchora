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
use pinchora_commands::{
    App, Responder, handle_interaction, on_ban_add, on_ban_remove, on_member_add, on_member_remove, on_member_update,
    on_message, on_voice_state_update,
};
use pinchora_db::Db;
use pinchora_game::Content;
use pinchora_render::BadgeCollectionCard;
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
    gateway::payload::incoming::{
        BanAdd, BanRemove, MemberAdd, MemberRemove, MemberUpdate, MessageCreate, VoiceStateUpdate,
    },
    http::interaction::{InteractionResponse, InteractionResponseType},
    id::Id,
};

const APPLICATION_ID: u64 = 1;
const CHANNEL_ID: u64 = 2;
/// The role the mock bot holds; its position is the ceiling for roles the bot can hand out.
const BOT_ROLE_ID: u64 = 9_000_000_000_001;
const BOT_ROLE_POSITION: i64 = 10;
/// What the mock says about the server when asked: its name and member count.
const GUILD_NAME: &str = "Pinchora Reef";
const GUILD_MEMBERS: u64 = 42;
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
    /// Messages posted to or edited in channels, oldest first.
    posts: Arc<Mutex<Vec<Post>>>,
    /// Messages somebody deleted: editing one answers Unknown Message.
    deleted: Arc<Mutex<Vec<u64>>>,
    /// Roles somebody deleted: changing one answers Unknown Role.
    deleted_roles: Arc<Mutex<Vec<u64>>>,
}

/// A message a handler posted to a channel (`POST`) or edited there (`PATCH`).
struct Post {
    method: Method,
    channel: u64,
    message: u64,
    body: Value,
}

impl Post {
    fn embed(&self) -> &Value {
        &self.body["embeds"][0]
    }

    fn title(&self) -> &str {
        self.embed()["title"].as_str().unwrap_or_default()
    }

    /// The text of the first embed's field called `name`.
    fn field(&self, name: &str) -> &str {
        self.embed()["fields"]
            .as_array()
            .and_then(|fields| fields.iter().find(|f| f["name"] == name))
            .and_then(|f| f["value"].as_str())
            .unwrap_or_default()
    }

    fn content(&self) -> &str {
        self.body["content"].as_str().unwrap_or_default()
    }
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

/// A user who can be a bot and have any name.
fn person_json(id: u64, name: &str, global_name: Option<&str>, bot: bool) -> Value {
    json!({
        "id": id.to_string(),
        "username": name,
        "discriminator": "0",
        "global_name": global_name,
        "avatar": null,
        "bot": bot,
    })
}

/// The message Discord answers when one is posted or edited.
fn message_json(channel: u64, id: u64) -> Value {
    json!({
        "id": id.to_string(),
        "channel_id": channel.to_string(),
        "type": 0,
        "author": user_json(APPLICATION_ID),
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
        "flags": 0,
    })
}

/// `GET /guilds/{id}?with_counts=true`.
fn guild_json(id: u64) -> Value {
    json!({
        "id": id.to_string(),
        "name": GUILD_NAME,
        "icon": null,
        "splash": null,
        "discovery_splash": null,
        "owner_id": "1",
        "afk_channel_id": null,
        "afk_timeout": 300,
        "application_id": null,
        "banner": null,
        "default_message_notifications": 0,
        "description": null,
        "emojis": [],
        "explicit_content_filter": 0,
        "features": [],
        "large": false,
        "mfa_level": 0,
        "nsfw_level": 0,
        "preferred_locale": "en-US",
        "premium_progress_bar_enabled": false,
        "public_updates_channel_id": null,
        "roles": [],
        "rules_channel_id": null,
        "system_channel_flags": 0,
        "system_channel_id": null,
        "vanity_url_code": null,
        "verification_level": 0,
        "approximate_member_count": GUILD_MEMBERS,
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
            if mock.deleted_roles.lock().unwrap().contains(&number(role)) {
                let gone = json!({ "message": "Unknown Role", "code": 10011 });
                return (StatusCode::NOT_FOUND, gone.to_string());
            }
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
        (&Method::GET, [.., "guilds", guild]) => return (StatusCode::OK, guild_json(number(guild)).to_string()),
        (&Method::POST, [.., "channels", channel, "messages"]) => {
            let (body, _) = request_body(&headers, &body);
            let (channel, id) = (number(channel), unique());
            mock.posts.lock().unwrap().push(Post {
                method: method.clone(),
                channel,
                message: id,
                body,
            });
            return (StatusCode::OK, message_json(channel, id).to_string());
        }
        (&Method::PATCH, [.., "channels", channel, "messages", id]) => {
            let (channel, id) = (number(channel), number(id));
            if mock.deleted.lock().unwrap().contains(&id) {
                let gone = json!({ "message": "Unknown Message", "code": 10008 });
                return (StatusCode::NOT_FOUND, gone.to_string());
            }
            let (body, _) = request_body(&headers, &body);
            mock.posts.lock().unwrap().push(Post {
                method: method.clone(),
                channel,
                message: id,
                body,
            });
            return (StatusCode::OK, message_json(channel, id).to_string());
        }
        _ => {}
    }
    if method == Method::GET || !path.contains("/webhooks/") {
        return (StatusCode::NOT_FOUND, "{}".to_owned());
    }
    let (body, files) = request_body(&headers, &body);
    mock.calls.lock().unwrap().push(Call {
        method,
        path,
        body,
        files,
    });
    (StatusCode::OK, "{}".to_owned())
}

/// A request's JSON, and the files that came with it as multipart.
fn request_body(headers: &HeaderMap, body: &[u8]) -> (Value, Vec<File>) {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    match content_type.split_once("boundary=") {
        Some((_, boundary)) => multipart(boundary, body),
        None => (serde_json::from_slice(body).unwrap_or(Value::Null), Vec::new()),
    }
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
    /// The custom id of the modal the handler opened instead of answering.
    modal: Option<String>,
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
            modal: None,
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
                modal: None,
                ..on.clone()
            },
            _ => Self::new(first.kind),
        };
        if let Some(data) = first.data {
            if first.kind == InteractionResponseType::Modal {
                reply.modal = data.custom_id.clone();
            }
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

    /// The text of every field of the first embed whose name starts with `prefix`, one after the
    /// other: a long list is split into several fields.
    fn fields_starting(&self, prefix: &str) -> String {
        self.embeds
            .first()
            .map(|e| {
                e.fields
                    .iter()
                    .filter(|f| f.name.starts_with(prefix))
                    .map(|f| f.value.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
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
        if let Some(modal) = &self.modal {
            writeln!(f, "  modal: {modal}")?;
        }
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
            for field in &embed.fields {
                writeln!(f, "  field {}:", field.name)?;
                for line in field.value.lines() {
                    writeln!(f, "    {line}")?;
                }
            }
            if let Some(footer) = &embed.footer {
                writeln!(f, "  footer: {}", footer.text)?;
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
            posts: Arc::default(),
            deleted: Arc::default(),
            deleted_roles: Arc::default(),
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

    /// Messages the handlers posted to or edited in channels since the last call.
    fn posts(&self) -> Vec<Post> {
        let posts = std::mem::take(&mut *self.mock.posts.lock().unwrap());
        for post in &posts {
            let text = [post.title(), post.content()]
                .iter()
                .filter(|t| !t.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" | ");
            println!(
                "  {} in channel {}: {text} {}",
                post.method,
                post.channel,
                post.embed()["description"]
                    .as_str()
                    .unwrap_or_default()
                    .replace('\n', " / ")
            );
        }
        posts
    }

    /// Makes a role disappear, as if a moderator had deleted it.
    fn delete_role(&self, id: u64) {
        self.mock.deleted_roles.lock().unwrap().push(id);
    }

    /// Makes a posted message disappear, as if a moderator had deleted it.
    fn delete_message(&self, id: u64) {
        self.mock.deleted.lock().unwrap().push(id);
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

    /// Delivers a member joining to the handler, which must not fail.
    async fn join(&self, user: u64, name: &str, bot: bool, nick: Option<&str>) {
        let event: MemberAdd = serde_json::from_value(json!({
            "guild_id": self.guild.to_string(),
            "user": person_json(user, name, Some(name), bot),
            "nick": nick,
            "roles": [],
            "joined_at": "2024-01-01T00:00:00.000000+00:00",
            "deaf": false,
            "mute": false,
            "flags": 0,
            "pending": false,
        }))
        .expect("member add JSON");
        println!("\n== {}: {name} joined", self.name);
        Box::pin(on_member_add(self.app.clone(), event))
            .await
            .expect("the join handler failed");
    }

    async fn leave(&self, user: u64, name: &str, bot: bool) {
        let event: MemberRemove = serde_json::from_value(json!({
            "guild_id": self.guild.to_string(),
            "user": person_json(user, name, Some(name), bot),
        }))
        .expect("member remove JSON");
        println!("\n== {}: {name} left", self.name);
        Box::pin(on_member_remove(self.app.clone(), event))
            .await
            .expect("the leave handler failed");
    }

    /// A member update: the names the gateway reports for `user` now.
    async fn update_member(&self, user: u64, name: &str, nick: Option<&str>) {
        let event: MemberUpdate = serde_json::from_value(json!({
            "guild_id": self.guild.to_string(),
            "user": person_json(user, name, Some(name), false),
            "nick": nick,
            "roles": [],
            "avatar": null,
            "communication_disabled_until": null,
            "joined_at": null,
            "deaf": null,
            "mute": null,
            "pending": false,
            "premium_since": null,
        }))
        .expect("member update JSON");
        println!("\n== {}: {name} updated (nick {nick:?})", self.name);
        on_member_update(self.app.clone(), event)
            .await
            .expect("the update handler failed");
    }

    async fn ban(&self, user: u64, name: &str, banned: bool) {
        let event = json!({ "guild_id": self.guild.to_string(), "user": person_json(user, name, Some(name), false) });
        println!(
            "\n== {}: {name} {}",
            self.name,
            if banned { "banned" } else { "unbanned" }
        );
        if banned {
            let event: BanAdd = serde_json::from_value(event).expect("ban JSON");
            on_ban_add(self.app.clone(), event)
                .await
                .expect("the ban handler failed");
        } else {
            let event: BanRemove = serde_json::from_value(event).expect("unban JSON");
            on_ban_remove(self.app.clone(), event)
                .await
                .expect("the unban handler failed");
        }
    }

    /// A voice state: `user` is now in `channel` (`None`: in none); `mute` flips the mute flag.
    async fn voice(&self, user: u64, name: &str, bot: bool, channel: Option<u64>, mute: bool) {
        let event: VoiceStateUpdate = serde_json::from_value(json!({
            "guild_id": self.guild.to_string(),
            "channel_id": channel.map(|c| c.to_string()),
            "user_id": user.to_string(),
            "member": {
                "user": person_json(user, name, Some(name), bot),
                "roles": [],
                "joined_at": "2024-01-01T00:00:00.000000+00:00",
                "deaf": false,
                "mute": false,
                "flags": 0,
                "pending": false,
            },
            "session_id": "sim",
            "deaf": false,
            "mute": false,
            "self_deaf": false,
            "self_mute": mute,
            "self_stream": false,
            "self_video": false,
            "suppress": false,
            "request_to_speak_timestamp": null,
        }))
        .expect("voice state JSON");
        println!("\n== {}: {name} voice {channel:?} (muted: {mute})", self.name);
        on_voice_state_update(self.app.clone(), event)
            .await
            .expect("the voice handler failed");
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

    /// Submits the modal that `on` opened, with its text inputs set to `fields`.
    async fn submit(&mut self, user: u64, on: &Reply, fields: &[(&str, &str)]) -> Reply {
        let custom_id = on.modal.clone().expect("the step opened no modal");
        let components: Vec<Value> = fields
            .iter()
            .enumerate()
            .map(|(n, (id, value))| {
                json!({
                    "type": 18,
                    "id": n * 2 + 1,
                    "component": { "type": 4, "id": n * 2 + 2, "custom_id": id, "value": value },
                })
            })
            .collect();
        let data = json!({ "custom_id": custom_id, "components": components });
        self.run(format!("submit {custom_id} {fields:?}"), user, 5, data, None)
            .await
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

    // An empty hand lists the room's pieces (`#id`), after the clear-room option; pick one and
    // take it up with Move.
    assert_eq!(
        placed.options("he:j:").first().map(|o| o.value.as_str()),
        Some("<clear>")
    );
    let piece = placed
        .options("he:j:")
        .iter()
        .find(|o| o.value.starts_with('#'))
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

/// The characters Discord counts against an embed's 6,000 limit.
fn embed_chars(reply: &Reply) -> usize {
    let embed = &reply.embeds[0];
    embed.title.as_deref().map_or(0, |t| t.chars().count())
        + embed.description.as_deref().map_or(0, |d| d.chars().count())
        + embed
            .fields
            .iter()
            .map(|f| f.name.chars().count() + f.value.chars().count())
            .sum::<usize>()
}

/// The picture of a step's first image, which must be a card of `size`.
fn card_image(reply: &Reply, size: (u32, u32)) -> &[u8] {
    let (filename, bytes) = &reply.files[0];
    let image = image::load_from_memory(bytes).expect("a png");
    assert_eq!((image.width(), image.height()), size, "{filename}");
    bytes
}

/// A step's first image, decoded; it must be a card of `size`.
fn card_pixels(reply: &Reply, size: (u32, u32)) -> image::RgbaImage {
    image::load_from_memory(card_image(reply, size))
        .expect("a png")
        .to_rgba8()
}

/// Whether two cards differ anywhere in the `x0..x1`, `y0..y1` box. Whole-image comparisons would
/// also notice the global rank moving while the other tests in this binary earn XP.
fn differ_in(a: &image::RgbaImage, b: &image::RgbaImage, x: std::ops::Range<u32>, y: std::ops::Range<u32>) -> bool {
    y.clone()
        .any(|y| x.clone().any(|x| a.get_pixel(x, y) != b.get_pixel(x, y)))
}

#[tokio::test]
async fn buying_and_wearing_a_background() {
    let Some(mut sim) = Sim::new("buying_and_wearing_a_background").await else {
        return;
    };
    let user = unique();
    sim.app.db.earn_shells(user, 5_000, "test").await.unwrap();
    let plain_profile = sim.command(user, "profile", json!([])).await;
    let plain_rank = sim.command(user, "rank", json!([])).await;
    let plain_wallet = sim.command(user, "balance", json!([])).await;

    // The panel starts on the real profile card, with nothing picked: nothing to buy, wear or remove.
    let panel = sim
        .command(user, "customize", json!([{ "name": "background", "type": 1 }]))
        .await;
    assert_eq!(panel.title(), "Card backgrounds");
    assert!(panel.ephemeral, "the editor is private");
    card_image(&panel, (960, 540));
    assert!(
        panel.description().contains("**5,000** shells"),
        "{}",
        panel.description()
    );
    assert_eq!(panel.options("cz:bs").len(), 25, "Discord's page of options");
    assert!(panel.button("Buy").disabled && panel.button("Equip").disabled && panel.button("Remove").disabled);
    assert!(panel.button("◀ Page").disabled && !panel.button("Page ▶").disabled);

    // Each option says its rarity and price; a pick previews the background and offers Buy.
    let option = panel
        .options("cz:bs")
        .iter()
        .find(|o| o.value == "shellwash_morning")
        .expect("the cheapest backgrounds are on page one");
    assert_eq!(option.label, "Shellwash Morning");
    assert_eq!(option.description.as_deref(), Some("Common · 800 shells"));
    let picked = sim.pick(user, &panel, "cz:bs", "shellwash_morning").await;
    assert!(
        picked
            .description()
            .contains("**Shellwash Morning** · Common · 800 shells"),
        "{}",
        picked.description()
    );
    assert!(!picked.button("Buy").disabled && picked.button("Equip").disabled);
    assert!(
        picked
            .options("cz:bs")
            .iter()
            .any(|o| o.default && o.value == "shellwash_morning")
    );
    assert_ne!(
        card_image(&picked, (960, 540)),
        card_image(&panel, (960, 540)),
        "the card changes"
    );
    assert!(
        sim.app.db.owned_backgrounds(user).await.unwrap().is_empty(),
        "a preview is free"
    );

    // Wearing what you do not own is refused, even with a hand-made click.
    let sneaky = sim.component(user, "cz:be:shellwash_morning", &[], &picked).await;
    assert!(sneaky.description().contains("don't own"), "{}", sneaky.description());
    assert_eq!(sim.app.db.user(user).await.unwrap().background, None);

    // Buying takes the shells once; the background moves to the front of the menu.
    let bought = sim.click(user, &picked, "Buy").await;
    assert!(
        bought
            .description()
            .contains("Bought **Shellwash Morning** for 800 shells. 4,200 left."),
        "{}",
        bought.description()
    );
    assert!(bought.button("Buy").disabled && !bought.button("Equip").disabled);
    let first = &bought.options("cz:bs")[0];
    assert_eq!(
        (first.value.as_str(), first.description.as_deref()),
        ("shellwash_morning", Some("Common · Owned"))
    );
    let again = sim.component(user, "cz:bb:shellwash_morning", &[], &bought).await;
    assert!(again.description().contains("already own"), "{}", again.description());
    assert_eq!(sim.app.db.user(user).await.unwrap().credits, 4_200, "no second charge");

    // Too expensive: page two holds the dearest backgrounds; the refusal quotes both numbers.
    let page_two = sim.click(user, &bought, "Page ▶").await;
    assert_eq!(page_two.options("cz:bs").len(), 36 - 25, "the rest of the shop");
    let last = page_two.options("cz:bs").last().expect("an option").value.clone();
    let looking = sim.pick(user, &page_two, "cz:bs", &last).await;
    let refused = sim.click(user, &looking, "Buy").await;
    assert!(
        refused.description().contains("shells; you have 4,200"),
        "{}",
        refused.description()
    );
    assert_eq!(
        sim.app.db.owned_backgrounds(user).await.unwrap(),
        vec!["shellwash_morning"]
    );

    // Equip: every card changes, and the accent follows the background.
    let back = sim.click(user, &refused, "◀ Page").await;
    let again = sim.pick(user, &back, "cz:bs", "shellwash_morning").await;
    let worn = sim.click(user, &again, "Equip").await;
    assert!(worn.description().contains("Saved."), "{}", worn.description());
    assert!(
        worn.description().contains("on your cards now"),
        "{}",
        worn.description()
    );
    assert!(worn.button("Equip").disabled && !worn.button("Remove").disabled);
    assert_eq!(
        sim.app.db.user(user).await.unwrap().background.as_deref(),
        Some("shellwash_morning")
    );
    let profile = sim.command(user, "profile", json!([])).await;
    let rank = sim.command(user, "rank", json!([])).await;
    let wallet = sim.command(user, "balance", json!([])).await;
    // Every card has the background in its corner now.
    for (now, before, size) in [
        (&profile, &plain_profile, (960, 540)),
        (&rank, &plain_rank, (934, 282)),
        (&wallet, &plain_wallet, (900, 300)),
    ] {
        assert!(differ_in(
            &card_pixels(now, size),
            &card_pixels(before, size),
            0..24,
            0..24
        ));
    }
    // The panel opens on what is worn.
    let reopened = sim
        .command(user, "customize", json!([{ "name": "background", "type": 1 }]))
        .await;
    assert!(
        reopened.description().contains("on your cards now"),
        "{}",
        reopened.description()
    );

    // Remove: back to the plain look.
    let removed = sim.click(user, &reopened, "Remove").await;
    assert!(
        removed.description().contains("plain again"),
        "{}",
        removed.description()
    );
    assert!(removed.button("Remove").disabled);
    assert_eq!(sim.app.db.user(user).await.unwrap().background, None);
    // The plain card has its night-sea corner back.
    let plain_again = card_pixels(&sim.command(user, "profile", json!([])).await, (960, 540));
    assert!(!differ_in(
        &plain_again,
        &card_pixels(&plain_profile, (960, 540)),
        0..24,
        0..24
    ));
    assert!(differ_in(
        &plain_again,
        &card_pixels(&profile, (960, 540)),
        0..24,
        0..24
    ));
}

/// Where the badges stand along the foot of the profile card.
const BADGE_FOOT: (std::ops::Range<u32>, std::ops::Range<u32>) = (450..780, 482..534);

/// The collection card's height for the 40 badges.
fn collection_height() -> u32 {
    BadgeCollectionCard::height_for(Content::get().badges.len())
}

/// Whether the collection card marks the badge `id` NEW: a gold tag at the top-right of its plate.
fn has_new_marker(card: &image::RgbaImage, id: &str) -> bool {
    let index = Content::get()
        .badges
        .iter()
        .position(|b| b.id == id)
        .expect("a badge in the content");
    let (xs, ys) = BadgeCollectionCard::new_tag_box(index);
    let gold = image::Rgba([255, 209, 102, 255]);
    let marker = ys
        .flat_map(|y| xs.clone().map(move |x| (x, y)))
        .filter(|&(x, y)| *card.get_pixel(x, y) == gold)
        .count();
    marker > 150
}

#[tokio::test]
async fn earning_buying_and_wearing_badges() {
    let Some(mut sim) = Sim::new("earning_buying_and_wearing_badges").await else {
        return;
    };
    let user = unique();
    let db = sim.app.db.clone();
    let foot = BADGE_FOOT;

    // Nothing earned yet: nothing on the card.
    let empty = sim.command(user, "profile", json!([])).await;

    // 1,200 XP is level 5; 12 reputation; three fish species; 1,500 shells.
    db.earn_shells(user, 1_500, "test").await.unwrap();
    db.add_message_xp(sim.guild, user, 1_200, 0).await.unwrap();
    for _ in 0..12 {
        db.give_rep(unique(), user).await.unwrap();
    }
    for fish in ["a", "b", "c"] {
        db.log_catch(user, fish, 10.0).await.unwrap();
    }

    // They never opened /customize, yet the card already shows the best of what they earned.
    let auto = sim.command(user, "profile", json!([])).await;
    assert!(
        differ_in(
            &card_pixels(&auto, (960, 540)),
            &card_pixels(&empty, (960, 540)),
            foot.0.clone(),
            foot.1.clone()
        ),
        "earned badges show without any setup"
    );
    assert!(!db.user(user).await.unwrap().badges_set);

    let panel = sim
        .command(user, "customize", json!([{ "name": "badges", "type": 1 }]))
        .await;
    assert_eq!(panel.title(), "Your badges");
    assert!(panel.ephemeral);
    let collection = card_pixels(&panel, (960, collection_height()));
    assert!(embed_chars(&panel) < 6_000, "{} characters", embed_chars(&panel));
    assert!(
        panel
            .description()
            .contains("Your card shows your best badges on its own (2/6): **Little Beacon**, **Kind Current**."),
        "{}",
        panel.description()
    );
    // The first time, what they earned is NEW on the collection.
    assert!(has_new_marker(&collection, "little_beacon") && has_new_marker(&collection, "kind_current"));
    assert!(
        !has_new_marker(&collection, "bright_beacon"),
        "a locked badge is not new"
    );

    // Earned badges are ticked, locked ones show real progress, shop badges show prices.
    let level = panel.field("Level");
    assert!(
        level.contains("✅ **Little Beacon** · Reach level 5. *(worn)*"),
        "automatic badges count as worn: {level}"
    );
    assert!(level.contains("🔒 **Bright Beacon** · 5/10 levels"), "{level}");
    assert!(
        panel
            .field("Fish book")
            .contains("🔒 **Fin Finder** · 3/10 fish species")
    );
    assert!(panel.field("Reputation").contains("✅ **Kind Current**"));
    assert!(
        panel
            .field("Reputation")
            .contains("🔒 **Harbour Helper** · 12/50 reputation")
    );
    assert!(
        panel
            .field("Shells")
            .contains("🔒 **Pocket Rattle** · 1,500/10,000 shells")
    );
    assert!(panel.field("Shop").contains("🛒 **Tiny Tide** · 1,000 shells"));
    let wearable: Vec<_> = panel.options("cz:ws").iter().map(|o| o.value.as_str()).collect();
    assert_eq!(wearable, ["little_beacon", "kind_current"], "only what is earned");
    assert!(
        panel.options("cz:ws").iter().all(|o| o.default),
        "the automatic badges are ticked"
    );
    assert_eq!(panel.options("cz:wb").len(), 12, "every shop badge is for sale");
    assert!(
        !panel.button("Take all off").disabled,
        "no badges at all is a choice too"
    );
    assert!(
        panel.button("Show my best automatically").disabled,
        "nothing to hand back yet"
    );

    // A badge that is not earned cannot be worn, even with a hand-made pick.
    let sneaky = sim.component(user, "cz:ws:0", &["guiding_light"], &panel).await;
    assert!(
        sneaky.content().contains("isn't available any more"),
        "{}",
        sneaky.content()
    );
    assert!(!db.user(user).await.unwrap().badges_set, "nothing was saved");

    // Wear the two earned ones: now it is their choice.
    let ws = panel.menu("cz:ws").custom_id.clone();
    let worn = sim
        .component(user, &ws, &["little_beacon", "kind_current"], &panel)
        .await;
    assert!(worn.description().contains("Saved."), "{}", worn.description());
    assert!(
        worn.description()
            .contains("Wearing 2/6: **Little Beacon**, **Kind Current**"),
        "{}",
        worn.description()
    );
    assert!(
        worn.field("Level")
            .contains("✅ **Little Beacon** · Reach level 5. *(worn)*")
    );
    let row = db.user(user).await.unwrap();
    assert_eq!(row.badges, ["little_beacon", "kind_current"]);
    assert!(row.badges_set);
    assert!(
        has_new_marker(&card_pixels(&worn, (960, collection_height())), "little_beacon"),
        "the NEW markers stay while the player works in the panel"
    );
    // Opening the collection again: they have seen those badges now.
    let again = sim
        .command(user, "customize", json!([{ "name": "badges", "type": 1 }]))
        .await;
    assert!(!has_new_marker(
        &card_pixels(&again, (960, collection_height())),
        "little_beacon"
    ));

    // The same two, in the same order, look the same as what the card picked by itself.
    let with_badges = sim.command(user, "profile", json!([])).await;
    assert!(!differ_in(
        &card_pixels(&with_badges, (960, 540)),
        &card_pixels(&auto, (960, 540)),
        foot.0.clone(),
        foot.1.clone()
    ));

    // A shop badge: too dear for a poor player, then bought once.
    db.spend_shells(user, 1_000, "test").await.unwrap();
    let poor = sim.pick(user, &worn, "cz:wb", "tiny_tide").await;
    assert!(
        poor.description()
            .contains("**Tiny Tide** costs 1,000 shells; you have 500."),
        "{}",
        poor.description()
    );
    assert!(db.owned_badges(user).await.unwrap().is_empty());
    db.earn_shells(user, 1_000, "test").await.unwrap();
    let bought = sim.pick(user, &poor, "cz:wb", "tiny_tide").await;
    assert!(
        bought
            .description()
            .contains("Bought **Tiny Tide** for 1,000 shells. 500 left."),
        "{}",
        bought.description()
    );
    assert!(
        bought.field("Shop").contains("✅ **Tiny Tide** · bought"),
        "{}",
        bought.field("Shop")
    );
    assert_eq!(bought.options("cz:wb").len(), 11);
    assert_eq!(bought.options("cz:ws").len(), 3, "the bought badge can be worn now");
    assert_eq!(db.user(user).await.unwrap().credits, 500);
    let ws = bought.menu("cz:ws").custom_id.clone();
    let worn = sim
        .component(user, &ws, &["little_beacon", "kind_current", "tiny_tide"], &bought)
        .await;
    assert_eq!(
        db.user(user).await.unwrap().badges,
        ["little_beacon", "kind_current", "tiny_tide"],
        "new badges go last"
    );

    // Unticking one takes just that one off, and the order of the rest stays.
    let ws = worn.menu("cz:ws").custom_id.clone();
    let fewer = sim.component(user, &ws, &["little_beacon", "tiny_tide"], &worn).await;
    assert!(fewer.description().contains("Wearing 2/6"), "{}", fewer.description());
    assert_eq!(db.user(user).await.unwrap().badges, ["little_beacon", "tiny_tide"]);

    // Never more than six: earn more, then ask for seven.
    db.add_message_xp(sim.guild, user, 3_800, 0).await.unwrap();
    for _ in 0..48 {
        db.give_rep(unique(), user).await.unwrap();
    }
    db.adopt(user, "crumbclaw_crab", "Sir Clicks").await.unwrap();
    let starter = pinchora_db::NewRoom {
        room_type: "living".into(),
        wallpaper: "w".into(),
        floor: "f".into(),
    };
    db.house(user, &starter).await.unwrap();
    let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
    for _ in 0..10 {
        db.like_house(user, unique(), day).await.unwrap();
    }
    let panel = sim
        .command(user, "customize", json!([{ "name": "badges", "type": 1 }]))
        .await;
    let seven = [
        "little_beacon",
        "bright_beacon",
        "warm_welcome",
        "pawshake",
        "kind_current",
        "harbour_helper",
        "tiny_tide",
    ];
    assert_eq!(panel.options("cz:ws").len(), seven.len());
    let new_ones = card_pixels(&panel, (960, collection_height()));
    assert!(
        has_new_marker(&new_ones, "bright_beacon") && has_new_marker(&new_ones, "harbour_helper"),
        "what they earned since last time is new"
    );
    assert!(
        !has_new_marker(&new_ones, "little_beacon"),
        "and what they had seen is not"
    );
    let ws = panel.menu("cz:ws").custom_id.clone();
    let refused = sim.component(user, &ws, &seven, &panel).await;
    assert!(
        refused.description().contains("6 badges at most"),
        "{}",
        refused.description()
    );
    assert_eq!(
        db.user(user).await.unwrap().badges,
        ["little_beacon", "tiny_tide"],
        "nothing changed"
    );
    let six = sim.component(user, &ws, &seven[..6], &panel).await;
    assert!(six.description().contains("Wearing 6/6"), "{}", six.description());
    assert_eq!(db.user(user).await.unwrap().badges.len(), 6);
    let profile = sim.command(user, "profile", json!([])).await;
    card_image(&profile, (960, 540));

    // Take them all off: an explicit empty choice means no badges, however many they have earned.
    let bare = sim.click(user, &six, "Take all off").await;
    assert!(
        bare.description().contains("not wearing any badges"),
        "{}",
        bare.description()
    );
    assert!(bare.button("Take all off").disabled);
    assert!(!bare.button("Show my best automatically").disabled);
    let row = db.user(user).await.unwrap();
    assert!(row.badges.is_empty() && row.badges_set);
    let plain_again = sim.command(user, "profile", json!([])).await;
    assert!(differ_in(
        &card_pixels(&plain_again, (960, 540)),
        &card_pixels(&profile, (960, 540)),
        foot.0.clone(),
        foot.1.clone()
    ));
    assert!(
        !differ_in(
            &card_pixels(&plain_again, (960, 540)),
            &card_pixels(&empty, (960, 540)),
            foot.0.clone(),
            foot.1.clone()
        ),
        "no badges is no badges"
    );

    // Hand the choice back: the card shows the six best again, the rarest first.
    let automatic = sim.click(user, &bare, "Show my best automatically").await;
    assert!(
        automatic.description().contains(
            "(6/6): **Bright Beacon**, **Harbour Helper**, **Little Beacon**, **Warm Welcome**, **Pawshake**, **Kind Current**."
        ),
        "two uncommon badges first, then commons in the order of the list: {}",
        automatic.description()
    );
    assert!(automatic.button("Show my best automatically").disabled);
    let row = db.user(user).await.unwrap();
    assert!(row.badges.is_empty() && !row.badges_set);
    let best = sim.command(user, "profile", json!([])).await;
    assert!(differ_in(
        &card_pixels(&best, (960, 540)),
        &card_pixels(&empty, (960, 540)),
        foot.0,
        foot.1
    ));
}

/// Every badge on the collection card: some earned and worn, some locked part-way, some for
/// sale, and the NEW tag on whatever came since the last look.
#[tokio::test]
async fn the_badge_collection_card_shows_progress_and_what_is_new() {
    let Some(mut sim) = Sim::new("the_badge_collection_card_shows_progress_and_what_is_new").await else {
        return;
    };
    let user = unique();
    let db = sim.app.db.clone();

    // Level 25, 14 fish species, a pet, a day of streak, two shop badges, 12,500 shells.
    db.add_message_xp(sim.guild, user, 42_000, 0).await.unwrap();
    db.earn_shells(user, 15_000, "test").await.unwrap();
    for n in 0..14 {
        db.log_catch(user, &format!("fish{n}"), 10.0).await.unwrap();
    }
    db.adopt(user, "crumbclaw_crab", "Sir Clicks").await.unwrap();
    db.buy_badge(user, "tiny_tide", 1_000).await.unwrap();
    db.buy_badge(user, "crab_cap", 1_500).await.unwrap();
    db.claim_daily(user).await.unwrap();
    for _ in 0..30 {
        db.give_rep(unique(), user).await.unwrap();
    }
    db.set_badges(
        user,
        &["harbour_beacon", "pocket_rattle", "tiny_tide", "kind_current"].map(str::to_owned),
    )
    .await
    .unwrap();

    let panel = sim
        .command(user, "customize", json!([{ "name": "badges", "type": 1 }]))
        .await;
    let first = card_pixels(&panel, (960, collection_height()));
    for earned in [
        "little_beacon",
        "bright_beacon",
        "harbour_beacon",
        "fin_finder",
        "pawshake",
        "kind_current",
        "pocket_rattle",
        "tiny_tide",
        "crab_cap",
    ] {
        assert!(has_new_marker(&first, earned), "{earned} was earned and never seen");
    }
    for not_yet in [
        "starlit_beacon",
        "reef_regular",
        "week_of_waves",
        "harbour_helper",
        "pearl_wyrm",
    ] {
        assert!(!has_new_marker(&first, not_yet), "{not_yet} is not earned");
    }
    assert!(panel.description().contains("Wearing 4/6"), "{}", panel.description());

    // Opened again, nothing is new. Earning one more makes just that one new.
    let second = sim
        .command(user, "customize", json!([{ "name": "badges", "type": 1 }]))
        .await;
    let calm = card_pixels(&second, (960, collection_height()));
    assert!(!has_new_marker(&calm, "little_beacon") && !has_new_marker(&calm, "crab_cap"));
    for _ in 0..25 {
        db.give_rep(unique(), user).await.unwrap();
    }
    let third = sim
        .command(user, "customize", json!([{ "name": "badges", "type": 1 }]))
        .await;
    let latest = card_pixels(&third, (960, collection_height()));
    assert!(
        has_new_marker(&latest, "harbour_helper"),
        "55 reputation earns Harbour Helper"
    );
    assert!(!has_new_marker(&latest, "little_beacon"), "the rest was seen");
    assert!(differ_in(&latest, &calm, 0..960, 0..collection_height()));
}

#[tokio::test]
async fn badge_unlocks_are_announced_once_under_the_command_that_earned_them() {
    let Some(mut sim) = Sim::new("badge_unlocks_are_announced_once_under_the_command_that_earned_them").await else {
        return;
    };
    let db = sim.app.db.clone();
    let thumbnail = |notice: &Reply, id: &str| {
        assert_eq!(notice.files.len(), 1, "the badge art goes along as the thumbnail");
        assert_eq!(notice.files[0].0, format!("badge-{id}.png"));
        let art = image::load_from_memory(&notice.files[0].1).expect("a png");
        assert_eq!(art.width(), 96, "drawn 4x");
    };

    // /daily: the shells it pays take a wallet over 10,000 and earn Pocket Rattle.
    let saver = unique();
    db.earn_shells(saver, 9_800, "test").await.unwrap();
    let daily = sim.command(saver, "daily", json!([])).await;
    assert!(daily.title().starts_with("+250"), "{}", daily.title());
    assert_eq!(daily.followups.len(), 1, "one notice under the answer");
    let notice = &daily.followups[0];
    assert_eq!(notice.title(), "Badge unlocked: Pocket Rattle");
    assert!(notice.description().contains("Hold 10,000 shells at once."));
    assert!(notice.description().contains("Uncommon"));
    thumbnail(notice, "pocket_rattle");
    assert!(!notice.ephemeral, "a public answer, a public notice");
    // Never twice: the cooldown answer carries no notice, and neither does another command.
    let cooldown = sim.command(saver, "daily", json!([])).await;
    assert!(cooldown.followups.is_empty());
    let other = (unique(), unique());
    let given = sim
        .command_resolved(
            saver,
            "give",
            json!([user_option("user", other.0), int_option("amount", 1)]),
            resolved(&[other.0], &[]),
        )
        .await;
    assert!(given.followups.is_empty(), "Pocket Rattle was announced already");

    // /fish: a tenth species earns Fin Finder.
    let angler = unique();
    db.earn_shells(angler, 500, "test").await.unwrap();
    for n in 0..9 {
        db.log_catch(angler, &format!("fish{n}"), 10.0).await.unwrap();
    }
    let casting = Reply::new(InteractionResponseType::ChannelMessageWithSource);
    let reel = |nonce: u32| format!("fr:{angler}:{nonce}");
    let bite = |nonce: u32| {
        let at = chrono::Utc::now().timestamp_millis() - 400;
        (format!("cast:{angler}:{nonce}"), at.to_string())
    };
    let (key, at) = bite(1);
    sim.app
        .cache
        .put(&key, &at, std::time::Duration::from_secs(30))
        .await
        .unwrap();
    let caught = sim.component(angler, &reel(1), &[], &casting).await;
    assert!(caught.title().contains("You caught"), "{}", caught.title());
    assert_eq!(caught.followups.len(), 1, "{:?}", caught.title());
    assert_eq!(caught.followups[0].title(), "Badge unlocked: Fin Finder");
    thumbnail(&caught.followups[0], "fin_finder");
    let (key, at) = bite(2);
    sim.app
        .cache
        .put(&key, &at, std::time::Duration::from_secs(30))
        .await
        .unwrap();
    let again = sim.component(angler, &reel(2), &[], &casting).await;
    assert!(again.title().contains("You caught"), "{}", again.title());
    assert!(again.followups.is_empty(), "Fin Finder is not announced twice");

    // /pet adopt: the first pet earns Pawshake.
    let keeper = unique();
    let menu = sim
        .command(keeper, "pet", json!([{ "name": "adopt", "type": 1 }]))
        .await;
    assert!(menu.followups.is_empty(), "choosing is not adopting");
    let adopted = sim.pick(keeper, &menu, "pa", "crumbclaw_crab").await;
    assert_eq!(adopted.followups.len(), 1);
    assert_eq!(adopted.followups[0].title(), "Badge unlocked: Pawshake");

    // /give: a badge earned some other way (here, reputation) is announced at the next chance.
    let (kind, friend) = (unique(), unique());
    db.earn_shells(kind, 5, "test").await.unwrap();
    for _ in 0..10 {
        db.give_rep(unique(), kind).await.unwrap();
    }
    let given = sim
        .command_resolved(
            kind,
            "give",
            json!([user_option("user", friend), int_option("amount", 1)]),
            resolved(&[friend], &[]),
        )
        .await;
    assert!(
        given.description().contains("Sent **1** shells"),
        "{}",
        given.description()
    );
    assert_eq!(given.followups.len(), 1);
    assert_eq!(given.followups[0].title(), "Badge unlocked: Kind Current");

    // Chat: reaching level 5 earns Little Beacon, posted under the level-up message.
    let chatter = unique();
    db.add_message_xp(sim.guild, chatter, 1_149, 0).await.unwrap();
    sim.say(chatter, &[]).await;
    let posts = sim.posts();
    assert_eq!(posts.len(), 2, "the level-up and the badge");
    assert!(
        posts[0].content().contains("reached **level 5**"),
        "{}",
        posts[0].content()
    );
    assert_eq!(posts[1].title(), "Badge unlocked: Little Beacon");
    assert!(
        posts[1].content().contains("unlocked a badge!"),
        "{}",
        posts[1].content()
    );
    assert_eq!(
        posts[1].body["allowed_mentions"]["parse"].as_array().map(Vec::len),
        Some(0),
        "named, not pinged"
    );

    // Placing furniture: the 25th piece in the house earns Nest Nudger, privately under the editor.
    let decorator = unique();
    let edit = sim
        .command(decorator, "house", json!([{ "name": "edit", "type": 1 }]))
        .await;
    db.grant_item(decorator, "seed_piece", 24).await.unwrap();
    for x in 0..24 {
        db.place_item(decorator, 0, "seed_piece", x, 0, false, |_| Ok(()))
            .await
            .unwrap();
    }
    let furniture = edit
        .options("he:s:")
        .iter()
        .find(|o| !o.value.starts_with('<') && o.value != "none")
        .expect("the starter kit is in the inventory")
        .value
        .clone();
    let held = sim.pick(decorator, &edit, "he:s:", &furniture).await;
    let placed = sim.click(decorator, &held, "Place").await;
    assert!(placed.description().contains("Placed **"), "{}", placed.description());
    assert_eq!(placed.followups.len(), 1);
    assert_eq!(placed.followups[0].title(), "Badge unlocked: Nest Nudger");
    assert!(placed.followups[0].ephemeral, "the editor is private, so is the notice");
}

#[tokio::test]
async fn titles_sit_under_the_name_on_the_profile_and_rank_cards() {
    let Some(mut sim) = Sim::new("titles_sit_under_the_name_on_the_profile_and_rank_cards").await else {
        return;
    };
    let user = unique();
    let db = sim.app.db.clone();
    let name_block = (296..800, 40..136);
    let rank_block = (280..700, 60..160);

    // Level 5 and 12 reputation: Shoreline Stroller and Kindly Neighbour are theirs; three fish
    // species are short of Tide Angler's five.
    db.add_message_xp(sim.guild, user, 1_200, 0).await.unwrap();
    for _ in 0..12 {
        db.give_rep(unique(), user).await.unwrap();
    }
    for fish in ["a", "b", "c"] {
        db.log_catch(user, fish, 10.0).await.unwrap();
    }
    let plain_profile = sim.command(user, "profile", json!([])).await;
    let plain_rank = sim.command(user, "rank", json!([])).await;

    let panel = sim
        .command(user, "customize", json!([{ "name": "title", "type": 1 }]))
        .await;
    assert_eq!(panel.title(), "Your titles");
    assert!(panel.ephemeral);
    card_image(&panel, (960, 540));
    assert!(embed_chars(&panel) < 6_000, "{} characters", embed_chars(&panel));
    assert!(
        panel.description().contains("You have no title yet."),
        "{}",
        panel.description()
    );
    assert!(panel.description().contains("unlocked 2/30"), "{}", panel.description());
    let unlocked: Vec<_> = panel.options("ti:s").iter().map(|o| o.value.as_str()).collect();
    assert_eq!(unlocked, ["shoreline_stroller", "kindly_neighbour"]);
    assert!(
        panel
            .field("Unlocked")
            .contains("✅ **Kindly Neighbour** · Reach 5 reputation.")
    );
    let locked = panel.fields_starting("Still to earn");
    assert!(locked.contains("🔒 **Tide Angler** · 3/5 fish species"), "{locked}");
    assert!(locked.contains("🔒 **House of the Week**"), "{locked}");
    assert!(panel.button("Remove").disabled, "no title to remove");

    // Hand-made picks of titles that are not theirs are refused.
    for locked in ["tide_angler", "house_of_the_week", "no_such_title"] {
        let refused = sim.component(user, "ti:s:0", &[locked], &panel).await;
        assert!(
            refused.content().contains("isn't available any more"),
            "{locked}: {}",
            refused.content()
        );
    }
    assert_eq!(db.user(user).await.unwrap().title, None);

    // Choosing one puts it on both cards.
    let chosen = sim.pick(user, &panel, "ti:s", "kindly_neighbour").await;
    assert!(
        chosen
            .description()
            .contains("Saved. **Kindly Neighbour** sits under your name"),
        "{}",
        chosen.description()
    );
    assert!(chosen.description().contains("Your title is **Kindly Neighbour**."));
    assert!(
        chosen
            .field("Unlocked")
            .contains("✅ **Kindly Neighbour** · Reach 5 reputation. *(worn)*")
    );
    assert!(
        chosen
            .options("ti:s")
            .iter()
            .any(|o| o.value == "kindly_neighbour" && o.default)
    );
    assert!(!chosen.button("Remove").disabled);
    assert_eq!(db.user(user).await.unwrap().title.as_deref(), Some("kindly_neighbour"));
    // The editor shows the card as it will look.
    assert!(differ_in(
        &card_pixels(&chosen, (960, 540)),
        &card_pixels(&plain_profile, (960, 540)),
        name_block.0.clone(),
        name_block.1.clone()
    ));
    let profile = sim.command(user, "profile", json!([])).await;
    assert!(differ_in(
        &card_pixels(&profile, (960, 540)),
        &card_pixels(&plain_profile, (960, 540)),
        name_block.0.clone(),
        name_block.1.clone()
    ));
    let rank = sim.command(user, "rank", json!([])).await;
    assert!(differ_in(
        &card_pixels(&rank, (934, 282)),
        &card_pixels(&plain_rank, (934, 282)),
        rank_block.0.clone(),
        rank_block.1.clone()
    ));

    // A title the contest grants shows up in the menu and can be worn.
    db.grant_title(user, "house_of_the_week").await.unwrap();
    let panel = sim
        .command(user, "customize", json!([{ "name": "title", "type": 1 }]))
        .await;
    let unlocked: Vec<_> = panel.options("ti:s").iter().map(|o| o.value.as_str()).collect();
    assert_eq!(
        unlocked,
        ["shoreline_stroller", "kindly_neighbour", "house_of_the_week"]
    );
    assert!(!panel.fields_starting("Still to earn").contains("House of the Week"));
    let special = sim.pick(user, &panel, "ti:s", "house_of_the_week").await;
    assert!(
        special.description().contains("Your title is **House of the Week**."),
        "{}",
        special.description()
    );

    // Removing it makes both cards plain again.
    let removed = sim.click(user, &special, "Remove").await;
    assert!(
        removed.description().contains("You have no title yet."),
        "{}",
        removed.description()
    );
    assert!(removed.button("Remove").disabled);
    assert_eq!(db.user(user).await.unwrap().title, None);
    let profile = sim.command(user, "profile", json!([])).await;
    assert!(!differ_in(
        &card_pixels(&profile, (960, 540)),
        &card_pixels(&plain_profile, (960, 540)),
        name_block.0,
        name_block.1
    ));
    let rank = sim.command(user, "rank", json!([])).await;
    assert!(!differ_in(
        &card_pixels(&rank, (934, 282)),
        &card_pixels(&plain_rank, (934, 282)),
        rank_block.0,
        rank_block.1
    ));
}

fn channel_option(name: &str, id: u64) -> Value {
    json!({ "name": name, "type": 7, "value": id.to_string() })
}

fn bool_option(name: &str, value: bool) -> Value {
    json!({ "name": name, "type": 5, "value": value })
}

/// `data.resolved` for options that name channels.
fn resolved_channels(ids: &[u64]) -> Value {
    json!({
        "channels": ids
            .iter()
            .map(|id| {
                (
                    id.to_string(),
                    json!({ "id": id.to_string(), "type": 0, "name": "sim-channel", "permissions": "0" }),
                )
            })
            .collect::<serde_json::Map<_, _>>(),
    })
}

/// A message a handler posted, as the `Reply` a member would see, so its buttons can be clicked.
fn posted_reply(post: &Post) -> Reply {
    let mut reply = Reply::new(InteractionResponseType::ChannelMessageWithSource);
    let embeds = serde_json::from_value(post.body["embeds"].clone()).ok();
    let components = serde_json::from_value::<Vec<Component>>(post.body["components"].clone()).ok();
    reply.edit(post.body["content"].as_str().map(str::to_owned), embeds, components);
    reply
}

/// The posts made in `channel`.
fn in_channel(posts: &[Post], channel: u64) -> Vec<&Post> {
    posts.iter().filter(|p| p.channel == channel).collect()
}

#[tokio::test]
async fn role_panels_post_and_toggle_roles() {
    let Some(mut sim) = Sim::new("role_panels_post_and_toggle_roles").await else {
        return;
    };
    let (admin, alice) = (unique(), unique());
    let (red_id, blue_id) = (unique(), unique());
    let red = sim.guild_role(red_id, "Red", 2);
    let blue = sim.guild_role(blue_id, "Blue", 3);
    let (lounge, elsewhere) = (unique(), unique());

    let panel_cmd = |title: &str, description: Option<&str>| {
        let mut options = vec![text_option("title", title)];
        options.extend(description.map(|d| text_option("description", d)));
        json!([sub("panel", options)])
    };
    let add_cmd = |panel: &str, role: &Value, label: &str, emoji: Option<&str>| {
        let mut options = vec![
            text_option("panel", panel),
            role_option("role", role),
            text_option("label", label),
        ];
        options.extend(emoji.map(|e| text_option("emoji", e)));
        json!([sub("add", options)])
    };
    let post_cmd = |panel: &str, channel: u64| {
        json!([sub(
            "post",
            vec![text_option("panel", panel), channel_option("channel", channel)]
        )])
    };

    // Only managers set panels up.
    let denied = sim.command(alice, "roles", panel_cmd("Colours", None)).await;
    assert!(denied.content().contains("Manage Server"), "{}", denied.content());
    assert!(sim.app.db.find_panel(sim.guild, "Colours").await.unwrap().is_none());
    sim.permissions = MANAGE_GUILD;

    // A draft; the same title again only updates the description.
    let draft = sim
        .command(admin, "roles", panel_cmd("Colours", Some("Pick a colour")))
        .await;
    assert!(
        draft.content().contains("**Colours**") && draft.content().contains("draft"),
        "{}",
        draft.content()
    );
    assert!(draft.ephemeral);
    let panel = sim.app.db.find_panel(sim.guild, "Colours").await.unwrap().unwrap();
    let again = sim
        .command(admin, "roles", panel_cmd("colours", Some("Pick your colour")))
        .await;
    assert!(again.content().contains("already existed"), "{}", again.content());

    // A panel without roles is not posted.
    let empty = sim
        .command_resolved(
            admin,
            "roles",
            post_cmd("Colours", lounge),
            resolved_channels(&[lounge]),
        )
        .await;
    assert!(empty.content().contains("no roles yet"), "{}", empty.content());
    assert!(sim.posts().is_empty());

    // Buttons need a known panel, a readable emoji, and a role the bot can hand out.
    let reply = sim
        .command_resolved(
            admin,
            "roles",
            add_cmd("Nope", &red, "Red", None),
            resolved(&[], &[&red]),
        )
        .await;
    assert!(reply.content().contains("no panel called"), "{}", reply.content());
    let reply = sim
        .command_resolved(
            admin,
            "roles",
            add_cmd("Colours", &red, "Red", Some("red")),
            resolved(&[], &[&red]),
        )
        .await;
    assert!(reply.content().contains("read that emoji"), "{}", reply.content());
    let above = role_json(unique(), "Boss", BOT_ROLE_POSITION + 5, false);
    let reply = sim
        .command_resolved(
            admin,
            "roles",
            add_cmd("Colours", &above, "Boss", None),
            resolved(&[], &[&above]),
        )
        .await;
    assert!(reply.content().contains("above my highest role"), "{}", reply.content());
    let managed = role_json(unique(), "SomeBot", 1, true);
    let reply = sim
        .command_resolved(
            admin,
            "roles",
            add_cmd("Colours", &managed, "Bot", None),
            resolved(&[], &[&managed]),
        )
        .await;
    assert!(
        reply.content().contains("managed by an integration"),
        "{}",
        reply.content()
    );
    let everyone = role_json(sim.guild, "@everyone", 0, false);
    let reply = sim
        .command_resolved(
            admin,
            "roles",
            add_cmd("Colours", &everyone, "All", None),
            resolved(&[], &[&everyone]),
        )
        .await;
    assert!(reply.content().contains("@everyone"), "{}", reply.content());
    assert!(sim.app.db.panel_roles(panel.id).await.unwrap().is_empty());

    // The panel can be named by its number, too.
    let reply = sim
        .command_resolved(
            admin,
            "roles",
            add_cmd("Colours", &red, "Red", Some("🔴")),
            resolved(&[], &[&red]),
        )
        .await;
    assert!(
        reply.content().contains(&format!("<@&{red_id}>")) && reply.content().contains("(1/25)"),
        "{}",
        reply.content()
    );
    let reply = sim
        .command_resolved(
            admin,
            "roles",
            add_cmd(&panel.id.to_string(), &blue, "Blue", None),
            resolved(&[], &[&blue]),
        )
        .await;
    assert!(reply.content().contains("(2/25)"), "{}", reply.content());

    // Posting puts the panel in the channel: an embed and one button per role.
    let reply = sim
        .command_resolved(
            admin,
            "roles",
            post_cmd("colours", lounge),
            resolved_channels(&[lounge]),
        )
        .await;
    assert!(
        reply.content().contains(&format!("Posted **Colours** in <#{lounge}>")),
        "{}",
        reply.content()
    );
    let posts = sim.posts();
    let [post] = &posts[..] else {
        panic!("expected one post, got {}", posts.len());
    };
    assert_eq!((post.method.clone(), post.channel), (Method::POST, lounge));
    assert_eq!(post.title(), "Colours");
    let text = post.embed()["description"].as_str().unwrap();
    assert!(
        text.starts_with("Pick your colour\n\n🔴 **Red** ·") && text.contains(&format!("<@&{blue_id}>")),
        "{text}"
    );
    let buttons = post.body["components"][0]["components"].as_array().unwrap();
    let ids: Vec<_> = buttons.iter().map(|b| b["custom_id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        [
            format!("rr:{}:{red_id}", panel.id),
            format!("rr:{}:{blue_id}", panel.id)
        ]
    );
    assert_eq!(buttons[0]["emoji"]["name"], "🔴");
    let stored = sim.app.db.find_panel(sim.guild, "Colours").await.unwrap().unwrap();
    assert_eq!(
        (stored.channel_id, stored.message_id),
        (Some(lounge as i64), Some(post.message as i64))
    );

    // A click toggles: Added, then Removed. Both answers are private.
    let message = posted_reply(post);
    let added = sim.click(alice, &message, "Red").await;
    assert!(added.ephemeral);
    assert_eq!(added.content(), format!("Added <@&{red_id}>."));
    assert_eq!(sim.role_changes(), vec![format!("PUT {alice} {red_id}")]);
    sim.roles = vec![red_id];
    let removed = sim.click(alice, &message, "Red").await;
    assert!(removed.ephemeral);
    assert_eq!(removed.content(), format!("Removed <@&{red_id}>."));
    assert_eq!(sim.role_changes(), vec![format!("DELETE {alice} {red_id}")]);
    sim.roles = Vec::new();

    // Discord refusing is explained, not swallowed.
    sim.allow_role_changes(false);
    let refused = sim.click(alice, &message, "Blue").await;
    assert!(
        refused.content().contains("couldn't give you") && refused.content().contains("highest role"),
        "{}",
        refused.content()
    );
    sim.allow_role_changes(true);

    // Ids are checked against the panel: another server's panel, a role that has no button,
    // and garbage all give nothing.
    let stranger = unique();
    let (theirs, _) = sim.app.db.save_panel(stranger, "Theirs", None).await.unwrap();
    assert!(
        sim.app
            .db
            .add_panel_role(theirs.id, blue_id, "Blue", None)
            .await
            .unwrap()
    );
    for (forged, expect) in [
        (format!("rr:{}:{blue_id}", theirs.id), "isn't on this panel"),
        (format!("rr:{}:{}", panel.id, unique()), "isn't on this panel"),
        ("rr:x:y".to_owned(), "expired"),
        (format!("rr:{}:{red_id}:extra", panel.id), "expired"),
    ] {
        let reply = sim.component(alice, &forged, &[], &message).await;
        assert!(reply.content().contains(expect), "{forged}: {}", reply.content());
    }
    assert!(sim.role_changes().is_empty(), "forged ids must not change roles");

    // Taking a button off stops its old message working, until the panel is posted again.
    let reply = sim
        .command(
            admin,
            "roles",
            json!([sub(
                "remove",
                vec![text_option("panel", "Colours"), role_option("role", &red)]
            )]),
        )
        .await;
    assert!(reply.content().contains("is off **Colours**"), "{}", reply.content());
    let stale = sim.click(alice, &message, "Red").await;
    assert!(stale.content().contains("isn't on this panel"), "{}", stale.content());
    assert!(sim.role_changes().is_empty());

    // Posting again in the same channel edits the message in place.
    sim.command_resolved(
        admin,
        "roles",
        post_cmd("Colours", lounge),
        resolved_channels(&[lounge]),
    )
    .await;
    let posts = sim.posts();
    let [edit] = &posts[..] else {
        panic!("expected one edit, got {}", posts.len());
    };
    assert_eq!((edit.method.clone(), edit.message), (Method::PATCH, post.message));
    assert_eq!(edit.body["components"][0]["components"].as_array().unwrap().len(), 1);

    // A message somebody deleted is posted afresh.
    sim.delete_message(post.message);
    sim.command_resolved(
        admin,
        "roles",
        post_cmd("Colours", lounge),
        resolved_channels(&[lounge]),
    )
    .await;
    let posts = sim.posts();
    let [fresh] = &posts[..] else {
        panic!("expected one post, got {}", posts.len());
    };
    assert_eq!((fresh.method.clone(), fresh.channel), (Method::POST, lounge));
    assert_ne!(fresh.message, post.message);

    // Another channel gets a new message; the old one keeps working but is no longer updated.
    let moved = sim
        .command_resolved(
            admin,
            "roles",
            post_cmd("Colours", elsewhere),
            resolved_channels(&[elsewhere]),
        )
        .await;
    assert!(
        moved.content().contains(&format!("<#{lounge}>")) && moved.content().contains("still works"),
        "{}",
        moved.content()
    );
    let posts = sim.posts();
    assert_eq!((posts.len(), posts[0].channel), (1, elsewhere));
    let stored = sim.app.db.find_panel(sim.guild, "Colours").await.unwrap().unwrap();
    assert_eq!(stored.channel_id, Some(elsewhere as i64));

    // A role deleted on Discord loses its button when somebody clicks it.
    let ghost_id = unique();
    let ghost = sim.guild_role(ghost_id, "Ghost", 1);
    sim.command_resolved(
        admin,
        "roles",
        add_cmd("Colours", &ghost, "Ghost", None),
        resolved(&[], &[&ghost]),
    )
    .await;
    sim.command_resolved(
        admin,
        "roles",
        post_cmd("Colours", elsewhere),
        resolved_channels(&[elsewhere]),
    )
    .await;
    let posts = sim.posts();
    let latest = posted_reply(&posts[0]);
    sim.delete_role(ghost_id);
    let gone = sim.click(alice, &latest, "Ghost").await;
    assert!(
        gone.content().contains("no longer exists") && gone.content().contains("taken off the panel"),
        "{}",
        gone.content()
    );
    assert!(
        sim.app
            .db
            .panel_role(sim.guild, panel.id, ghost_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn join_role_goodbye_and_server_log() {
    let Some(mut sim) = Sim::new("join_role_goodbye_and_server_log").await else {
        return;
    };
    let admin = unique();
    let (alice, robot, stranger) = (unique(), unique(), unique());
    let (newbie_id, log, bye) = (unique(), unique(), unique());
    let (lounge, den) = (unique(), unique());
    let newbie = sim.guild_role(newbie_id, "Newbie", 2);

    let config = |sub_command: Value| json!([sub_command]);

    // Nothing is configured yet: members come and go unnoticed.
    sim.join(unique(), "Early", false, None).await;
    assert!(sim.role_changes().is_empty() && sim.posts().is_empty());

    // /config join-role: only roles the bot can hand out.
    sim.permissions = MANAGE_GUILD;
    let join_role = |role: &Value| config(sub("join-role", vec![role_option("role", role)]));
    let above = role_json(unique(), "Boss", BOT_ROLE_POSITION + 5, false);
    let reply = sim
        .command_resolved(admin, "config", join_role(&above), resolved(&[], &[&above]))
        .await;
    assert!(reply.content().contains("above my highest role"), "{}", reply.content());
    let managed = role_json(unique(), "SomeBot", 1, true);
    let reply = sim
        .command_resolved(admin, "config", join_role(&managed), resolved(&[], &[&managed]))
        .await;
    assert!(
        reply.content().contains("managed by an integration"),
        "{}",
        reply.content()
    );
    let everyone = role_json(sim.guild, "@everyone", 0, false);
    let reply = sim
        .command_resolved(admin, "config", join_role(&everyone), resolved(&[], &[&everyone]))
        .await;
    assert!(reply.content().contains("@everyone"), "{}", reply.content());
    assert!(
        sim.app
            .db
            .guild_settings(sim.guild)
            .await
            .unwrap()
            .join_role_id
            .is_none()
    );
    let reply = sim
        .command_resolved(admin, "config", join_role(&newbie), resolved(&[], &[&newbie]))
        .await;
    assert!(
        reply.content().contains(&format!("<@&{newbie_id}>")),
        "{}",
        reply.content()
    );

    // /config goodbye and /config log.
    let goodbye = sim
        .command_resolved(
            admin,
            "config",
            config(sub(
                "goodbye",
                vec![
                    channel_option("channel", bye),
                    text_option("message", "Bye {user} from {server} ({members} left) {nope}"),
                ],
            )),
            resolved_channels(&[bye]),
        )
        .await;
    assert!(
        goodbye.content().contains(&format!("<#{bye}>")) && goodbye.content().contains("Bye {user}"),
        "{}",
        goodbye.content()
    );
    let clash = sim
        .command_resolved(
            admin,
            "config",
            config(sub(
                "goodbye",
                vec![channel_option("channel", bye), bool_option("off", true)],
            )),
            resolved_channels(&[bye]),
        )
        .await;
    assert!(clash.content().contains("on its own"), "{}", clash.content());
    let logging = sim
        .command_resolved(
            admin,
            "config",
            config(sub("log", vec![channel_option("channel", log)])),
            resolved_channels(&[log]),
        )
        .await;
    assert!(
        logging.content().contains(&format!("<#{log}>")),
        "{}",
        logging.content()
    );
    let shown = sim.command(admin, "config", config(sub("show", vec![]))).await;
    assert!(
        shown.field("Join role").contains(&format!("<@&{newbie_id}>"))
            && shown.field("Goodbye").contains(&format!("<#{bye}>"))
            && shown.field("Server log").contains(&format!("<#{log}>"))
            && shown.field("Server log").contains("voice on"),
        "{:?}",
        shown.embeds[0].fields
    );

    // A human joins: the role, and a log entry; the entry says when the account was made.
    sim.join(alice, "Alice", false, None).await;
    assert_eq!(sim.role_changes(), vec![format!("PUT {alice} {newbie_id}")]);
    let posts = sim.posts();
    let [joined] = &posts[..] else {
        panic!("expected one log entry, got {}", posts.len());
    };
    assert_eq!((joined.channel, joined.title()), (log, "Member joined"));
    assert!(
        joined.embed()["description"]
            .as_str()
            .unwrap()
            .contains(&format!("<@{alice}>")),
        "{:?}",
        joined.embed()
    );
    assert!(
        joined.field("Account created").starts_with("<t:"),
        "{:?}",
        joined.embed()
    );
    assert!(
        joined.embed()["thumbnail"]["url"]
            .as_str()
            .unwrap()
            .contains("cdn.discordapp.com")
    );
    assert!(joined.embed()["timestamp"].is_string());

    // A bot joining is logged but gets no role.
    sim.join(robot, "RoboBot", true, None).await;
    assert!(sim.role_changes().is_empty());
    let posts = sim.posts();
    assert_eq!(posts.iter().map(Post::title).collect::<Vec<_>>(), ["Bot joined"]);

    // Discord refusing the role does not stop the log.
    sim.allow_role_changes(false);
    sim.join(unique(), "Blocked", false, None).await;
    sim.allow_role_changes(true);
    assert_eq!(
        sim.posts().iter().map(Post::title).collect::<Vec<_>>(),
        ["Member joined"]
    );

    // Leaving: the goodbye fills its placeholders (and pings nobody), and the log has an entry.
    sim.leave(alice, "Alice", false).await;
    let posts = sim.posts();
    let [say] = &in_channel(&posts, bye)[..] else {
        panic!("expected one goodbye, got {}", in_channel(&posts, bye).len());
    };
    assert_eq!(
        say.content(),
        format!("Bye Alice from {GUILD_NAME} ({GUILD_MEMBERS} left) {{nope}}")
    );
    assert_eq!(say.body["allowed_mentions"]["parse"], json!([]));
    assert_eq!(
        in_channel(&posts, log).iter().map(|p| p.title()).collect::<Vec<_>>(),
        ["Member left"]
    );

    // Bots get no goodbye, only the log entry.
    sim.leave(robot, "RoboBot", true).await;
    let posts = sim.posts();
    assert!(in_channel(&posts, bye).is_empty());
    assert_eq!(posts.iter().map(Post::title).collect::<Vec<_>>(), ["Bot left"]);

    // /config goodbye with nothing to change shows the current setting.
    let reply = sim.command(admin, "config", config(sub("goodbye", vec![]))).await;
    assert!(reply.content().contains("Bye {user}"), "{}", reply.content());

    // Nicknames: Alice's names were noted when she joined, so the change shows both sides.
    sim.join(alice, "alice", false, None).await;
    sim.posts();
    sim.role_changes();
    sim.update_member(alice, "alice", Some("Zed")).await;
    let posts = sim.posts();
    let [renamed] = &posts[..] else {
        panic!("expected one entry, got {}", posts.len());
    };
    assert_eq!((renamed.channel, renamed.title()), (log, "Nickname changed"));
    assert_eq!(renamed.field("Nickname"), "*none* → `Zed`");
    sim.update_member(alice, "alice", Some("Zedd")).await;
    assert_eq!(sim.posts()[0].field("Nickname"), "`Zed` → `Zedd`");
    // Roles, timeouts and the like also send member updates; nothing changed, nothing logged.
    sim.update_member(alice, "alice", Some("Zedd")).await;
    assert!(sim.posts().is_empty());
    // A changed username (and with it the display name) is one entry with both.
    sim.update_member(alice, "alice2", Some("Zedd")).await;
    let posts = sim.posts();
    assert_eq!(posts[0].title(), "Names changed");
    assert_eq!(posts[0].field("Username"), "`alice` → `alice2`");
    assert_eq!(posts[0].field("Display name"), "`alice` → `alice2`");
    // A member never seen before: only a set nickname is reported, as the bare new value.
    sim.update_member(stranger, "stranger", Some("Hello")).await;
    let posts = sim.posts();
    assert_eq!(
        (posts[0].title(), posts[0].field("Nickname")),
        ("Nickname changed", "`Hello`")
    );
    sim.update_member(unique(), "nobody", None).await;
    assert!(sim.posts().is_empty());

    // Bans and unbans.
    sim.ban(stranger, "stranger", true).await;
    assert_eq!(
        sim.posts().iter().map(Post::title).collect::<Vec<_>>(),
        ["Member banned"]
    );
    sim.ban(stranger, "stranger", false).await;
    assert_eq!(
        sim.posts().iter().map(Post::title).collect::<Vec<_>>(),
        ["Member unbanned"]
    );

    // Voice: join, mute (nothing to say), move, leave. Bots are ignored.
    sim.voice(alice, "alice2", false, Some(lounge), false).await;
    let posts = sim.posts();
    assert_eq!(posts[0].title(), "Joined voice");
    assert!(
        posts[0].embed()["description"]
            .as_str()
            .unwrap()
            .contains(&format!("<#{lounge}>")),
        "{:?}",
        posts[0].embed()
    );
    sim.voice(alice, "alice2", false, Some(lounge), true).await;
    assert!(sim.posts().is_empty());
    sim.voice(alice, "alice2", false, Some(den), true).await;
    let posts = sim.posts();
    assert_eq!(posts[0].title(), "Moved voice");
    assert!(
        posts[0].embed()["description"]
            .as_str()
            .unwrap()
            .contains(&format!("<#{lounge}> to <#{den}>")),
        "{:?}",
        posts[0].embed()
    );
    sim.voice(alice, "alice2", false, None, false).await;
    assert_eq!(sim.posts()[0].title(), "Left voice");
    sim.voice(robot, "RoboBot", true, Some(lounge), false).await;
    assert!(sim.posts().is_empty());

    // /config log-events switches kinds off; the others keep working.
    let events = |options: Vec<Value>| config(sub("log-events", options));
    let reply = sim
        .command(
            admin,
            "config",
            events(vec![bool_option("voice", false), bool_option("names", false)]),
        )
        .await;
    assert!(
        reply.content().contains("name changes off")
            && reply.content().contains("voice off")
            && reply.content().contains("bans on"),
        "{}",
        reply.content()
    );
    sim.voice(alice, "alice2", false, Some(lounge), false).await;
    sim.update_member(alice, "alice2", Some("Quiet")).await;
    assert!(sim.posts().is_empty(), "switched-off kinds are not logged");
    sim.ban(stranger, "stranger", true).await;
    assert_eq!(sim.posts().len(), 1, "bans are still logged");
    sim.command(admin, "config", events(vec![bool_option("members", false)]))
        .await;
    sim.join(unique(), "Quiet", false, None).await;
    assert_eq!(sim.role_changes().len(), 1, "the join role does not depend on the log");
    assert!(sim.posts().is_empty());

    // Everything off: no log, no goodbye, no join role.
    sim.command(admin, "config", config(sub("log", vec![bool_option("off", true)])))
        .await;
    sim.ban(stranger, "stranger", false).await;
    assert!(sim.posts().is_empty());
    sim.command(admin, "config", config(sub("goodbye", vec![bool_option("off", true)])))
        .await;
    sim.leave(unique(), "Gone", false).await;
    assert!(sim.posts().is_empty());
    let reply = sim
        .command(
            admin,
            "config",
            config(sub("join-role", vec![bool_option("off", true)])),
        )
        .await;
    assert!(reply.content().contains("no longer get a role"), "{}", reply.content());
    sim.join(unique(), "Late", false, None).await;
    assert!(sim.role_changes().is_empty() && sim.posts().is_empty());

    // On a server with a goodbye channel but no message of its own, the default names the server.
    sim.guild = unique();
    sim.command_resolved(
        admin,
        "config",
        config(sub("goodbye", vec![channel_option("channel", bye)])),
        resolved_channels(&[bye]),
    )
    .await;
    sim.leave(unique(), "Zoë", false).await;
    let posts = sim.posts();
    assert_eq!(
        in_channel(&posts, bye)[0].content(),
        format!("👋 **Zoë** has left **{GUILD_NAME}**.")
    );
}

// /House: undo, clear room, guestbook, contest

/// The pieces in a room as (item, x, y, flipped), oldest first.
async fn room_pieces(sim: &Sim, user: u64, room: i16) -> Vec<(String, i16, i16, bool)> {
    sim.app
        .db
        .placements(user, room)
        .await
        .expect("placements")
        .into_iter()
        .map(|p| (p.item_id, p.x, p.y, p.flipped))
        .collect()
}

/// How many of `item` the player has unplaced.
async fn owned(sim: &Sim, user: u64, item: &str) -> i32 {
    sim.app
        .db
        .inventory(user)
        .await
        .expect("inventory")
        .into_iter()
        .find(|row| row.item_id == item)
        .map_or(0, |row| row.quantity)
}

/// The inventory menu value of the first piece whose description names `category`.
fn kit_item(editor: &Reply, category: &str) -> String {
    editor
        .options("he:s:")
        .iter()
        .find(|o| o.description.as_deref().is_some_and(|d| d.contains(category)))
        .unwrap_or_else(|| panic!("no {category} in the inventory menu"))
        .value
        .clone()
}

#[tokio::test]
async fn house_editor_undo() {
    let Some(mut sim) = Sim::new("house_editor_undo").await else {
        return;
    };
    let user = unique();
    let edit = sim.command(user, "house", json!([{ "name": "edit", "type": 1 }])).await;
    assert_eq!(edit.rows(), 5, "the undo button fits the existing rows");
    assert!(edit.button("↶ Undo").disabled, "nothing to undo yet");
    let seat = kit_item(&edit, "Seating");
    let table = kit_item(&edit, "Tables");

    // Place → Undo: the piece goes back to the inventory.
    let held = sim.pick(user, &edit, "he:s:", &seat).await;
    let placed = sim.click(user, &held, "Place").await;
    assert_eq!(room_pieces(&sim, user, 0).await.len(), 1);
    assert_eq!(owned(&sim, user, &seat).await, 0);
    let undone = sim.click(user, &placed, "↶ Undo").await;
    assert!(
        undone.description().contains("Undid placing **"),
        "{}",
        undone.description()
    );
    assert!(room_pieces(&sim, user, 0).await.is_empty());
    assert_eq!(owned(&sim, user, &seat).await, 1);
    assert!(undone.button("↶ Undo").disabled, "that was the only step");

    // Move → Undo: one step, back to the old spot and turn.
    let held = sim.pick(user, &undone, "he:s:", &seat).await;
    let placed = sim.click(user, &held, "Place").await;
    let before = room_pieces(&sim, user, 0).await;
    let moving = sim.click(user, &placed, "Move").await;
    let spot = moving.options("he:j:").last().expect("a free spot").value.clone();
    let at_spot = sim.pick(user, &moving, "he:j:", &spot).await;
    let moved = sim.click(user, &at_spot, "Place").await;
    let after = room_pieces(&sim, user, 0).await;
    assert_ne!(before, after, "the seat moved");
    let undone = sim.click(user, &moved, "↶ Undo").await;
    assert!(
        undone.description().contains("Undid moving **"),
        "{}",
        undone.description()
    );
    assert_eq!(room_pieces(&sim, user, 0).await, before, "back where it was");
    assert!(
        undone.description().contains("Under cursor:"),
        "the cursor follows the piece back: {}",
        undone.description()
    );

    // Turn → Undo.
    let turned = sim.click(user, &undone, "Flip").await;
    assert!(room_pieces(&sim, user, 0).await[0].3, "turned");
    let undone = sim.click(user, &turned, "↶ Undo").await;
    assert!(
        undone.description().contains("Undid turning **"),
        "{}",
        undone.description()
    );
    assert_eq!(room_pieces(&sim, user, 0).await, before);

    // Store → Undo: back on the same spot.
    let stored = sim.click(user, &undone, "Store").await;
    assert!(room_pieces(&sim, user, 0).await.is_empty());
    let undone = sim.click(user, &stored, "↶ Undo").await;
    assert!(
        undone.description().contains("Undid storing **"),
        "{}",
        undone.description()
    );
    assert_eq!(room_pieces(&sim, user, 0).await, before);

    // Clear room: the first option of Go to, a confirm step, then one undo puts it all back.
    let held = sim.pick(user, &undone, "he:s:", &table).await;
    let with_table = sim.click(user, &held, "Place").await;
    let full = room_pieces(&sim, user, 0).await;
    assert_eq!(full.len(), 2);
    assert_eq!(with_table.options("he:j:")[0].value, "<clear>");
    let confirm = sim.pick(user, &with_table, "he:j:", "<clear>").await;
    assert!(confirm.title().starts_with("Clear room 1"), "{}", confirm.title());
    assert_eq!(confirm.rows(), 1, "only confirm or cancel");
    assert!(confirm.files.is_empty(), "the room picture stays as it was");
    let kept = sim.click(user, &confirm, "Cancel").await;
    assert_eq!(room_pieces(&sim, user, 0).await, full, "cancel keeps everything");
    let confirm = sim.pick(user, &kept, "he:j:", "<clear>").await;
    let cleared = sim.click(user, &confirm, "Store all 2").await;
    assert!(
        cleared.description().contains("Stored all **2**"),
        "{}",
        cleared.description()
    );
    assert!(room_pieces(&sim, user, 0).await.is_empty());
    assert_eq!(
        (owned(&sim, user, &seat).await, owned(&sim, user, &table).await),
        (1, 1)
    );
    let undone = sim.click(user, &cleared, "↶ Undo").await;
    assert!(
        undone.description().contains("Undid clearing the room (2 pieces)"),
        "{}",
        undone.description()
    );
    assert_eq!(room_pieces(&sim, user, 0).await, full);

    // The history outlives the message: a new editor can still undo.
    let reopened = sim.command(user, "house", json!([{ "name": "edit", "type": 1 }])).await;
    assert!(!reopened.button("↶ Undo").disabled);

    // An undo that is no longer possible says so and drops that step.
    let seat_id = sim
        .app
        .db
        .placements(user, 0)
        .await
        .expect("placements")
        .into_iter()
        .find(|p| p.item_id == seat)
        .expect("the seat is back")
        .id;
    let picked = sim.pick(user, &reopened, "he:j:", &format!("#{seat_id}")).await;
    let stored = sim.click(user, &picked, "Store").await;
    assert!(stored.description().contains("Stored **"), "{}", stored.description());
    let friend = unique();
    assert!(sim.app.db.gift_item(user, friend, &seat).await.expect("gift"));
    let refused = sim.click(user, &stored, "↶ Undo").await;
    assert!(
        refused.description().contains("Can't undo storing **") && refused.description().contains("left"),
        "{}",
        refused.description()
    );
    assert_eq!(room_pieces(&sim, user, 0).await.len(), 1, "nothing changed");
    assert!(!refused.button("↶ Undo").disabled, "older steps are still there");

    // A change outside the editor drops the history.
    let table_id = sim.app.db.placements(user, 0).await.expect("placements")[0].id;
    sim.app.db.pick_up(user, table_id).await.expect("pick up");
    let moved = sim.click(user, &refused, "↘").await;
    assert!(moved.button("↶ Undo").disabled, "the room changed outside the editor");
}

fn visit_options(host: u64) -> Value {
    json!([{ "name": "visit", "type": 1, "options": [user_option("user", host)] }])
}

#[tokio::test]
async fn guestbook_sign_read_delete_block() {
    let Some(mut sim) = Sim::new("guestbook").await else {
        return;
    };
    let (host, guest, stranger) = (unique(), unique(), unique());
    sim.command(host, "house", json!([{ "name": "view", "type": 1 }])).await;

    // A visit offers the guestbook; the note comes in through a modal and is cleaned.
    let visit = sim
        .command_resolved(guest, "house", visit_options(host), resolved(&[host], &[]))
        .await;
    let form = sim.click(guest, &visit, "Sign guestbook").await;
    assert_eq!(form.modal.as_deref(), Some(format!("gb:n:{host}").as_str()));
    let signed = sim
        .submit(
            guest,
            &form,
            &[(
                "note",
                "Hi @everyone! Love the **sofa** <@123> https://spam.example/x\nsee discord.gg/abc",
            )],
        )
        .await;
    assert!(
        signed.ephemeral && signed.content().contains("Signed!"),
        "{}",
        signed.content()
    );
    let (notes, total) = sim.app.db.guestbook(host, 0, 5).await.expect("guestbook");
    assert_eq!(total, 1);
    assert_eq!(notes[0].body, "Hi ! Love the **sofa** see");
    assert!(
        signed.content().contains("Love the \\*\\*sofa\\*\\* see"),
        "shown as typed: {}",
        signed.content()
    );

    // Once a day, only after a visit.
    let again = sim.click(guest, &visit, "Sign guestbook").await;
    assert!(again.content().contains("already signed"), "{}", again.content());
    let nope = sim.click(stranger, &visit, "Sign guestbook").await;
    assert!(nope.content().contains("Visit the house first"), "{}", nope.content());
    let own = sim.click(host, &visit, "Sign guestbook").await;
    assert!(own.content().contains("your own guestbook"), "{}", own.content());

    // The owner's /house view mentions it until it is read.
    let view = sim.command(host, "house", json!([{ "name": "view", "type": 1 }])).await;
    let footer = |r: &Reply| {
        r.embeds
            .first()
            .and_then(|e| e.footer.as_ref())
            .map(|f| f.text.clone())
            .unwrap_or_default()
    };
    assert!(footer(&view).contains("1 new guestbook note"), "{}", footer(&view));

    // More visitors, so the book has two pages.
    let mut visitors = vec![guest];
    for n in 0..5 {
        let visitor = unique();
        let visit = sim
            .command_resolved(visitor, "house", visit_options(host), resolved(&[host], &[]))
            .await;
        let form = sim.click(visitor, &visit, "Sign guestbook").await;
        sim.submit(visitor, &form, &[("note", &format!("Visitor {n} was here. Cozy!"))])
            .await;
        visitors.push(visitor);
    }
    let stranger_visit = sim
        .command_resolved(stranger, "house", visit_options(host), resolved(&[host], &[]))
        .await;
    let form = sim.click(stranger, &stranger_visit, "Sign guestbook").await;
    sim.submit(stranger, &form, &[("note", "buy my stuff, cheap cheap")])
        .await;

    let book = sim
        .command(host, "house", json!([{ "name": "guestbook", "type": 1 }]))
        .await;
    assert!(book.ephemeral, "the owner's guestbook is private");
    assert_eq!(book.title(), "📖 Your guestbook");
    assert!(book.description().contains("**new**"), "{}", book.description());
    assert!(book.description().contains("buy my stuff"), "newest first");
    assert!(book.field("Recent visitors").contains(&format!("<@{stranger}>")));
    assert_eq!(book.field("Recent visitors").lines().count(), 7);
    assert_eq!(book.rows(), 3, "pages, delete, block");
    let view = sim.command(host, "house", json!([{ "name": "view", "type": 1 }])).await;
    assert!(!footer(&view).contains("new guestbook"), "read now: {}", footer(&view));

    let older = sim.click(host, &book, "Older ▶").await;
    assert!(older.description().contains("Love the"), "{}", older.description());
    assert!(footer(&older).starts_with("Page 2/2"), "{}", footer(&older));
    // The owner turning a public page of their book keeps it public, without their menus.
    let shared = sim
        .command_resolved(
            guest,
            "house",
            json!([{ "name": "guestbook", "type": 1, "options": [user_option("user", host)] }]),
            resolved(&[host], &[]),
        )
        .await;
    let turned = sim.click(host, &shared, "Older ▶").await;
    assert!(footer(&turned).starts_with("Page 2/2"), "{}", footer(&turned));
    assert!(
        turned.controls().all(|c| !matches!(c, Component::SelectMenu(_))),
        "the owner's menus never show on a public page"
    );

    // The owner deletes the first guest's note and blocks the stranger.
    let first = older
        .options("gb:d:")
        .iter()
        .find(|o| o.label.contains("Love the"))
        .expect("the first note can be deleted")
        .value
        .clone();
    let deleted = sim.pick(host, &older, "gb:d:", &first).await;
    assert!(
        deleted.description().contains("Deleted the note"),
        "{}",
        deleted.description()
    );
    assert_eq!(sim.app.db.guestbook(host, 0, 50).await.expect("guestbook").1, 6);
    let page1 = sim.click(host, &deleted, "◀ Newer").await;
    let blocked = sim.pick(host, &page1, "gb:b:", &format!("b{stranger}")).await;
    assert!(
        blocked.description().contains("Blocked **"),
        "{}",
        blocked.description()
    );
    assert!(!blocked.description().contains("buy my stuff"), "their notes are gone");
    assert!(
        blocked
            .options("gb:b:")
            .iter()
            .any(|o| o.value == format!("u{stranger}")),
        "the owner can unblock them"
    );
    let refused = sim.click(stranger, &stranger_visit, "Sign guestbook").await;
    assert!(refused.content().contains("blocked you"), "{}", refused.content());

    // Only the owner manages the book; anyone can read it.
    let meddling = sim.pick(guest, &blocked, "gb:b:", &format!("u{stranger}")).await;
    assert!(meddling.content().contains("Only the owner"), "{}", meddling.content());
    let public = sim
        .command_resolved(
            guest,
            "house",
            json!([{ "name": "guestbook", "type": 1, "options": [user_option("user", host)] }]),
            resolved(&[host], &[]),
        )
        .await;
    assert!(!public.ephemeral);
    assert!(public.title().ends_with("'s guestbook"), "{}", public.title());
    assert!(!public.description().contains("**new**"));
    assert!(
        public.controls().all(|c| !matches!(c, Component::SelectMenu(_))),
        "no owner menus for visitors"
    );

    // A locked house turns signers away.
    sim.command(
        host,
        "house",
        json!([{ "name": "lock", "type": 1, "options": [bool_option("locked", true)] }]),
    )
    .await;
    let locked = sim.click(visitors[1], &visit, "Sign guestbook").await;
    assert!(locked.content().contains("locked"), "{}", locked.content());
}

fn contest(sub_name: &str, options: Vec<Value>) -> Value {
    json!([group("contest", sub(sub_name, options))])
}

/// Places one starter piece of each category in room 1 with the editor.
async fn furnish(sim: &mut Sim, user: u64, categories: &[&str]) {
    let mut editor = sim.command(user, "house", json!([{ "name": "edit", "type": 1 }])).await;
    for category in categories {
        let held = sim.pick(user, &editor, "he:s:", &kit_item(&editor, category)).await;
        editor = sim.click(user, &held, "Place").await;
        assert!(editor.description().contains("Placed **"), "{}", editor.description());
    }
}

/// Opens ballots as `voter`, voting for the rooms of `wanted` and skipping others; returns the last message.
async fn vote_for(sim: &mut Sim, voter: u64, wanted: &[u64]) -> Reply {
    let mut ballot = sim.command(voter, "house", contest("vote", vec![])).await;
    let mut left = wanted.to_vec();
    for _ in 0..20 {
        if left.is_empty() {
            return ballot;
        }
        assert_eq!(ballot.files.len(), 1, "a ballot shows the room");
        let owner: u64 = ballot
            .button("💖 Vote")
            .custom_id
            .as_deref()
            .and_then(|id| id.rsplit(':').next())
            .and_then(|id| id.parse().ok())
            .expect("the vote button names the entry");
        assert_ne!(owner, voter, "a ballot never shows your own room");
        if let Some(i) = left.iter().position(|w| *w == owner) {
            left.remove(i);
            ballot = sim.click(voter, &ballot, "💖 Vote").await;
            let said = format!("{}{}", ballot.content(), ballot.description());
            assert!(said.contains("Vote counted"), "{said}");
        } else {
            ballot = sim.click(voter, &ballot, "Skip").await;
        }
    }
    panic!("{voter} never got to vote for {left:?}");
}

async fn credits(sim: &Sim, user: u64) -> i64 {
    sim.app.db.user(user).await.expect("user").credits
}

#[tokio::test]
async fn room_contest_week() {
    let Some(mut sim) = Sim::new("room_contest_week").await else {
        return;
    };
    let (a, b, c, d) = (unique(), unique(), unique(), unique());
    furnish(
        &mut sim,
        a,
        &["Rugs", "Seating", "Tables", "Plants", "Wall decor", "Beds"],
    )
    .await;
    furnish(&mut sim, b, &["Beds", "Lighting", "Rugs"]).await;
    furnish(&mut sim, c, &["Seating", "Tables"]).await;

    // An empty room can't be entered; furnished ones can, and entering again updates the snapshot.
    let empty = sim.command(d, "house", contest("enter", vec![])).await;
    assert!(
        empty.ephemeral && empty.content().contains("is empty"),
        "{}",
        empty.content()
    );
    for user in [a, b, c] {
        let entered = sim
            .command(user, "house", contest("enter", vec![int_option("room", 1)]))
            .await;
        assert!(
            entered.title().contains("entered the room contest"),
            "{}",
            entered.title()
        );
        assert_eq!(entered.files.len(), 1, "the entry is drawn");
    }
    let again = sim.command(a, "house", contest("enter", vec![])).await;
    assert!(
        again.title().contains("updated their contest entry"),
        "{}",
        again.title()
    );

    let view = sim.command(a, "house", contest("view", vec![])).await;
    assert!(view.title().starts_with("🏆 Room contest: "), "{}", view.title());
    assert_eq!(view.field("Entries"), "3");
    assert!(
        view.field("Your entry").starts_with("Room 1"),
        "{}",
        view.field("Your entry")
    );
    assert!(
        view.field("Last week").contains("No contest"),
        "{}",
        view.field("Last week")
    );

    // Votes: a gets 3, b 2, c 1.
    let done = vote_for(&mut sim, a, &[b, c]).await;
    assert!(done.content().contains("judged every entry"), "{}", done.content());
    vote_for(&mut sim, b, &[a]).await;
    vote_for(&mut sim, c, &[a]).await;
    let stale = vote_for(&mut sim, d, &[a, b]).await;
    assert!(stale.button("Skip").disabled, "only one entry left to judge");
    let already = sim.command(a, "house", contest("vote", vec![])).await;
    assert!(
        already.content().contains("judged every entry"),
        "{}",
        already.content()
    );

    // The week ends: the first contest call after it pays the prizes, once.
    let before = [credits(&sim, a).await, credits(&sim, b).await, credits(&sim, c).await];
    sim.app.shift_clock(chrono::Duration::days(7));
    let next = sim.command(d, "house", contest("view", vec![])).await;
    let podium = next.field("Last week");
    assert!(podium.contains("🥇") && podium.contains("3 votes"), "{podium}");
    assert!(podium.contains("🥈") && podium.contains("2 votes"), "{podium}");
    assert!(podium.contains("🥉") && podium.contains("1 vote "), "{podium}");
    assert_eq!(next.field("Entries"), "0", "a new week");
    let after = [credits(&sim, a).await, credits(&sim, b).await, credits(&sim, c).await];
    assert_eq!(
        [after[0] - before[0], after[1] - before[1], after[2] - before[2]],
        [500, 250, 100]
    );
    let pool = sim.app.db.pool().clone();
    let (title,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM user_titles WHERE user_id = $1 AND title_id = 'house_of_the_week'")
            .bind(a as i64)
            .fetch_one(&pool)
            .await
            .expect("titles");
    assert_eq!(title, 1, "the winner is House of the Week");
    let (results,): (i64,) = sqlx::query_as("SELECT count(*) FROM contest_results WHERE guild_id = $1")
        .bind(sim.guild as i64)
        .fetch_one(&pool)
        .await
        .expect("results");
    assert_eq!(results, 1);

    sim.command(b, "house", contest("view", vec![])).await;
    sim.command(c, "house", contest("winners", vec![])).await;
    let (paid,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM credit_ledger WHERE user_id = $1 AND reason = 'contest_prize'")
            .bind(a as i64)
            .fetch_one(&pool)
            .await
            .expect("ledger");
    assert_eq!(paid, 1, "paid exactly once");
    assert_eq!(credits(&sim, a).await, after[0]);

    let winners = sim.command(b, "house", contest("winners", vec![])).await;
    assert!(
        winners.title().starts_with("🏆 House of the Week"),
        "{}",
        winners.title()
    );
    assert_eq!(winners.files.len(), 1, "the winning room is drawn");

    // A ballot from last week can't vote any more.
    let late = sim.click(d, &stale, "💖 Vote").await;
    assert!(late.content().contains("week is over"), "{}", late.content());
}

// /Social: partners, friends, hug, pat, wave, high five

/// `data.resolved` for users that have names of their own.
fn named(users: &[(u64, &str)]) -> Value {
    json!({
        "users": users
            .iter()
            .map(|(id, name)| (id.to_string(), person_json(*id, name, Some(name), false)))
            .collect::<serde_json::Map<_, _>>(),
    })
}

fn mention(user: u64) -> String {
    format!("<@{user}>")
}

/// Every frame of the GIF named `name` in a reply, as pictures.
fn gif_frames(reply: &Reply, name: &str) -> Vec<image::RgbaImage> {
    use image::AnimationDecoder;
    let (_, bytes) = reply.files.iter().find(|(file, _)| file == name).unwrap_or_else(|| {
        panic!(
            "no file {name}; the reply has {:?}",
            reply.files.iter().map(|f| &f.0).collect::<Vec<_>>()
        )
    });
    assert!(bytes.len() <= 1_000_000, "{name} is {} bytes", bytes.len());
    image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes))
        .expect("a GIF")
        .into_frames()
        .collect_frames()
        .expect("every frame decodes")
        .into_iter()
        .map(image::Frame::into_buffer)
        .collect()
}

/// Saves the first and the middle frame of a GIF next to the step images, to look at.
fn save_frames(sim: &Sim, step_name: &str, frames: &[image::RgbaImage]) {
    for (label, at) in [("first", 0), ("middle", frames.len() / 2)] {
        frames[at]
            .save(sim.dir.join(format!("{step_name}-{label}.png")))
            .expect("save the frame");
    }
}

fn partner_options(sub_name: &str, options: Vec<Value>) -> Value {
    json!([sub(sub_name, options)])
}

#[tokio::test]
async fn partners_propose_accept_view_leave() {
    let Some(mut sim) = Sim::new("partners_flow").await else {
        return;
    };
    let (a, b, c, d, e, f) = (unique(), unique(), unique(), unique(), unique(), unique());
    let propose = |to: u64, name: &str| {
        (
            partner_options("propose", vec![user_option("user", to)]),
            named(&[(to, name)]),
        )
    };

    // Nobody partners themselves.
    let (options, users) = propose(a, "Sim");
    let own = sim.command_resolved(a, "partner", options, users).await;
    assert!(
        own.ephemeral && own.content().contains("somebody else"),
        "{}",
        own.content()
    );

    // a asks b; only b can answer, and the question pings b.
    let (options, users) = propose(b, "Bea");
    let asked = sim.command_resolved(a, "partner", options, users).await;
    assert_eq!(asked.content(), mention(b));
    assert!(
        asked
            .description()
            .contains(&format!("{} asks {}", mention(a), mention(b))),
        "{}",
        asked.description()
    );
    assert_eq!(asked.rows(), 1);
    asked.button("Decline");
    let stolen = sim.click(c, &asked, "Accept").await;
    assert!(
        stolen.ephemeral && stolen.content().contains("isn't for you"),
        "{}",
        stolen.content()
    );
    assert!(sim.app.db.partner(a).await.unwrap().is_none());

    // b accepts: partners, with a picture of the two of them.
    let paired = sim.click(b, &asked, "Accept").await;
    assert_eq!(paired.title(), "💞 New partners");
    assert!(
        paired.components.is_empty() && paired.content().is_empty(),
        "{}",
        paired.content()
    );
    assert_eq!(paired.files.len(), 1);
    assert_eq!(paired.files[0].0, "partners.png");
    let pair = sim.app.db.partner(a).await.unwrap().expect("a has a partner");
    assert_eq!(pair.partner_id, b);
    // A second click on the same Accept changes nothing.
    let twice = sim.click(b, &asked, "Accept").await;
    assert!(twice.content().contains("partners already"), "{}", twice.content());
    assert_eq!(sim.app.db.partner(b).await.unwrap().map(|p| p.partner_id), Some(a));
    // The profile card asks for the partner's name.
    let name = pinchora_commands::partner_name(&sim.app, a).await;
    assert_eq!(name.as_deref(), Some("Sim"), "the name b had when they answered");
    assert_eq!(pinchora_commands::partner_name(&sim.app, c).await, None);

    // Both must be free: a and b are taken, whoever asks.
    let (options, users) = propose(c, "Cy");
    let busy = sim.command_resolved(a, "partner", options, users).await;
    assert!(
        busy.ephemeral && busy.content().contains("already have a partner"),
        "{}",
        busy.content()
    );
    let (options, users) = propose(b, "Bea");
    let taken = sim.command_resolved(c, "partner", options, users).await;
    assert!(
        taken.ephemeral && taken.content().contains("already has a partner"),
        "{}",
        taken.content()
    );

    // The pair's page, counting what they sent each other.
    sim.app.db.record_interaction("hug", a, b).await.unwrap();
    sim.app.db.record_interaction("hug", b, a).await.unwrap();
    sim.app.db.record_interaction("pat", a, b).await.unwrap();
    let view = sim.command(a, "partner", partner_options("view", vec![])).await;
    assert!(
        view.title().starts_with("💞 ") && view.title().contains(" & "),
        "{}",
        view.title()
    );
    assert_eq!(
        view.field("Together since"),
        format!("<t:{}:D>", pair.since.timestamp())
    );
    assert_eq!(view.field("Days together"), "1");
    let sent = view.field("Sent to each other");
    assert!(
        sent.contains("2 hugs") && sent.contains("1 pat") && sent.contains("0 waves") && sent.contains("0 high fives"),
        "{sent}"
    );
    assert!(
        view.field("Their houses").contains("pieces"),
        "{}",
        view.field("Their houses")
    );
    assert_eq!(view.files[0].0, "partners.png");
    // Anybody can look at a pair; somebody without a partner gets told so.
    let seen = sim
        .command_resolved(
            c,
            "partner",
            partner_options("view", vec![user_option("user", b)]),
            named(&[(b, "Bea")]),
        )
        .await;
    assert_eq!(seen.field("Days together"), "1");
    let alone = sim.command(c, "partner", partner_options("view", vec![])).await;
    assert!(
        alone.ephemeral && alone.content().contains("don't have a partner"),
        "{}",
        alone.content()
    );

    // Declining and taking a request back both end it.
    let (options, users) = propose(f, "Fay");
    let asked = sim.command_resolved(e, "partner", options, users).await;
    let said_no = sim.click(f, &asked, "Decline").await;
    assert!(
        said_no.content().contains("said no thanks") && said_no.components.is_empty(),
        "{}",
        said_no.content()
    );
    let (options, users) = propose(f, "Fay");
    let asked = sim.command_resolved(e, "partner", options, users).await;
    let back = sim.click(e, &asked, "Decline").await;
    assert!(
        back.content().contains("took the partner request back"),
        "{}",
        back.content()
    );
    assert!(sim.app.db.partner(e).await.unwrap().is_none());

    // A request nobody answered in ten minutes is dead.
    let (options, users) = propose(d, "Dee");
    let asked = sim.command_resolved(c, "partner", options, users).await;
    sim.app.shift_clock(chrono::Duration::minutes(11));
    let late = sim.click(d, &asked, "Accept").await;
    assert!(
        late.content().contains("expired") && late.components.is_empty(),
        "{}",
        late.content()
    );
    assert!(sim.app.db.partner(c).await.unwrap().is_none() && sim.app.db.partner(d).await.unwrap().is_none());

    // Leaving takes a confirmation; staying keeps the pair.
    let alone = sim.command(c, "partner", partner_options("leave", vec![])).await;
    assert!(
        alone.ephemeral && alone.content().contains("don't have a partner"),
        "{}",
        alone.content()
    );
    let confirm = sim.command(a, "partner", partner_options("leave", vec![])).await;
    assert!(
        confirm.ephemeral && confirm.title().contains("Leave"),
        "{}",
        confirm.title()
    );
    let stayed = sim.click(a, &confirm, "Stay together").await;
    assert!(stayed.content().contains("still partners"), "{}", stayed.content());
    assert!(sim.app.db.partner(a).await.unwrap().is_some());
    let wrong = sim.click(b, &confirm, "Leave").await;
    assert!(
        wrong.ephemeral && wrong.content().contains("isn't yours"),
        "{}",
        wrong.content()
    );
    let left = sim.click(a, &confirm, "Leave").await;
    assert!(left.content().contains("not partners anymore"), "{}", left.content());
    assert!(sim.app.db.partner(a).await.unwrap().is_none() && sim.app.db.partner(b).await.unwrap().is_none());
    assert_eq!(pinchora_commands::partner_name(&sim.app, a).await, None);
    // Both are free to pair again.
    assert!(matches!(
        sim.app.db.pair_partners(b, c).await.unwrap(),
        pinchora_db::PairOutcome::Paired { .. }
    ));
}

#[tokio::test]
async fn a_partner_decorates_the_others_house() {
    let Some(mut sim) = Sim::new("partner_editing").await else {
        return;
    };
    let (a, b, stranger) = (unique(), unique(), unique());
    sim.app.db.pair_partners(a, b).await.unwrap();
    sim.app.db.remember_name(b, "Bea").await.unwrap();
    let edit_options = |owner: u64| json!([sub("edit", vec![user_option("owner", owner)])]);

    // Only a partner gets in.
    let refused = sim
        .command_resolved(stranger, "house", edit_options(b), named(&[(b, "Bea")]))
        .await;
    assert!(
        refused.ephemeral && refused.content().contains("partner"),
        "{}",
        refused.content()
    );
    assert!(room_pieces(&sim, b, 0).await.is_empty());

    // The editor shows b's room and a's inventory, and says whose the pieces become.
    let edit = sim
        .command_resolved(a, "house", edit_options(b), named(&[(b, "Bea")]))
        .await;
    assert!(edit.title().starts_with("Decorating Bea's room 1"), "{}", edit.title());
    let footer = edit.embeds[0]
        .footer
        .as_ref()
        .map(|f| f.text.clone())
        .unwrap_or_default();
    assert!(footer.contains("belongs to Bea from then on"), "{footer}");
    assert_eq!(edit.rows(), 5, "the same rows as the owner's editor");
    assert!(edit.button("Move").disabled, "a piece of Bea's can't go into a's hand");
    let pieces: Vec<&str> = edit
        .options("pt:s:")
        .iter()
        .map(|o| o.value.as_str())
        .filter(|v| !v.starts_with('<') && *v != "none")
        .collect();
    assert!(!pieces.is_empty(), "a's starter kit is in the menu");
    let seat = pieces[0].to_owned();
    let seat_before = owned(&sim, a, &seat).await;
    // Opening b's house made it, with a starter kit of b's own.
    let b_before = owned(&sim, b, &seat).await;

    // Place: it leaves a's inventory and stands in b's house.
    let held = sim.pick(a, &edit, "pt:s:", &seat).await;
    let placed = sim.click(a, &held, "Place").await;
    assert!(placed.description().contains("Placed **"), "{}", placed.description());
    assert_eq!(room_pieces(&sim, b, 0).await.len(), 1);
    assert!(room_pieces(&sim, a, 0).await.is_empty(), "not in a's own room");
    assert_eq!(owned(&sim, a, &seat).await, seat_before - 1);

    // Store: it is b's now, so it goes to b's inventory.
    let stored = sim.click(a, &placed, "Store").await;
    assert!(
        stored.description().contains("Bea's inventory"),
        "{}",
        stored.description()
    );
    assert!(room_pieces(&sim, b, 0).await.is_empty());
    assert_eq!(owned(&sim, b, &seat).await, b_before + 1);
    assert_eq!(owned(&sim, a, &seat).await, seat_before - 1);

    // Undo puts it back from b's inventory; undoing the placement returns it to a, who paid.
    let undone = sim.click(a, &stored, "↶ Undo").await;
    assert!(
        undone.description().contains("Undid storing"),
        "{}",
        undone.description()
    );
    assert_eq!(room_pieces(&sim, b, 0).await.len(), 1);
    assert_eq!(owned(&sim, b, &seat).await, b_before);
    let undone = sim.click(a, &undone, "↶ Undo").await;
    assert!(
        undone.description().contains("Undid placing"),
        "{}",
        undone.description()
    );
    assert!(room_pieces(&sim, b, 0).await.is_empty());
    assert_eq!(owned(&sim, a, &seat).await, seat_before);
    assert_eq!(owned(&sim, b, &seat).await, b_before);

    // Place again and clear the room: everything goes to b's inventory, undo puts it back.
    let held = sim.pick(a, &undone, "pt:s:", &seat).await;
    let placed = sim.click(a, &held, "Place").await;
    let prompt = sim.pick(a, &placed, "pt:j:", "<clear>").await;
    assert!(
        prompt.description().contains("Bea's inventory"),
        "{}",
        prompt.description()
    );
    let cleared = sim.click(a, &prompt, "Store it").await;
    assert!(
        cleared.description().contains("Bea's inventory"),
        "{}",
        cleared.description()
    );
    assert!(room_pieces(&sim, b, 0).await.is_empty());
    assert_eq!(owned(&sim, b, &seat).await, b_before + 1);
    let back = sim.click(a, &cleared, "↶ Undo").await;
    assert_eq!(room_pieces(&sim, b, 0).await.len(), 1);
    assert_eq!(owned(&sim, b, &seat).await, b_before);
    assert!(
        back.description().contains("Undid clearing the room"),
        "{}",
        back.description()
    );
    assert!(!back.button("↶ Undo").disabled, "a's placement can still be undone");

    // A new partnership starts without that history: what a placed under the old one stays b's.
    sim.app.db.leave_partner(a).await.unwrap();
    sim.app.db.pair_partners(a, b).await.unwrap();
    let reopened = sim
        .command_resolved(a, "house", edit_options(b), named(&[(b, "Bea")]))
        .await;
    assert!(
        reopened.button("↶ Undo").disabled,
        "the old partnership's steps are gone"
    );
    let old_undo = back.button("↶ Undo").custom_id.clone().expect("custom id");
    let nothing = sim.component(a, &old_undo, &[], &back).await;
    assert!(
        nothing.description().contains("Nothing to undo"),
        "{}",
        nothing.description()
    );
    assert_eq!(room_pieces(&sim, b, 0).await.len(), 1);

    // Moving is the owner's: the button is off, and a hand-made click is refused too.
    let id = placed
        .button("Store")
        .custom_id
        .clone()
        .expect("custom id")
        .replacen(":k:", ":v:", 1);
    let move_it = sim.component(a, &id, &[], &placed).await;
    assert!(move_it.description().contains("theirs"), "{}", move_it.description());
    assert_eq!(room_pieces(&sim, b, 0).await.len(), 1);

    // b's own editor is b's: the same room, b's inventory, and a history of b's own.
    let own = sim.command(b, "house", json!([{ "name": "edit", "type": 1 }])).await;
    assert!(own.title().starts_with("Editing room 1"), "{}", own.title());
    assert!(own.button("↶ Undo").disabled, "a's steps are not b's to undo");
    assert!(!own.button("Move").disabled || own.description().contains("Holding: **nothing**"));
    let hovered = sim
        .pick(
            b,
            &own,
            "he:j:",
            &own.options("he:j:")
                .iter()
                .find(|o| o.value.starts_with('#'))
                .expect("the piece is listed")
                .value,
        )
        .await;
    let lifted = sim.click(b, &hovered, "Store").await;
    assert!(lifted.description().contains("Stored **"), "{}", lifted.description());
    assert_eq!(owned(&sim, b, &seat).await, b_before + 1);
    // ... and a's editor, still open, now finds b's change under it and has nothing to undo.
    let stale = sim.click(a, &placed, "Store").await;
    assert!(
        stale.description().contains("Nothing under the cursor"),
        "{}",
        stale.description()
    );
    assert!(stale.button("↶ Undo").disabled, "the room changed outside a's editor");
    let seen = stale.options("pt:s:").len();
    assert!(seen > 0);

    // Once they stop being partners, a's open editor stops working.
    sim.app.db.leave_partner(a).await.unwrap();
    let any_click = stale.button("Place").custom_id.clone().expect("custom id");
    let gone = sim.component(a, &any_click, &[], &stale).await;
    assert!(
        gone.ephemeral && gone.content().contains("no longer partners"),
        "{}",
        gone.content()
    );
    let again = sim
        .command_resolved(a, "house", edit_options(b), named(&[(b, "Bea")]))
        .await;
    assert!(
        again.ephemeral && again.content().contains("partner"),
        "{}",
        again.content()
    );
}

#[tokio::test]
async fn friends_add_accept_list_remove_and_visit() {
    let Some(mut sim) = Sim::new("friends_flow").await else {
        return;
    };
    let (a, b, c) = (unique(), unique(), unique());
    let add = |to: u64, name: &str| (json!([sub("add", vec![user_option("user", to)])]), named(&[(to, name)]));

    let (options, users) = add(a, "Sim");
    let own = sim.command_resolved(a, "friends", options, users).await;
    assert!(
        own.ephemeral && own.content().contains("other people"),
        "{}",
        own.content()
    );

    let (options, users) = add(b, "Bea");
    let asked = sim.command_resolved(a, "friends", options, users).await;
    assert_eq!(asked.content(), mention(b));
    assert!(asked.title().contains("Friend request"), "{}", asked.title());
    let stolen = sim.click(c, &asked, "Accept").await;
    assert!(
        stolen.ephemeral && stolen.content().contains("isn't for you"),
        "{}",
        stolen.content()
    );
    assert!(!sim.app.db.are_friends(a, b).await.unwrap());
    let friends = sim.click(b, &asked, "Accept").await;
    assert!(
        friends
            .description()
            .contains(&format!("{} and {} are friends now", mention(a), mention(b))),
        "{}",
        friends.description()
    );
    assert!(friends.components.is_empty());
    assert!(sim.app.db.are_friends(a, b).await.unwrap() && sim.app.db.are_friends(b, a).await.unwrap());
    // Answering twice does nothing more.
    let (options, users) = add(b, "Bea");
    let twice = sim.command_resolved(a, "friends", options, users).await;
    assert!(
        twice.ephemeral && twice.content().contains("already friends"),
        "{}",
        twice.content()
    );

    // The list shows level and house likes; b's house gets a like and a lock to see both.
    sim.command(b, "house", json!([{ "name": "view", "type": 1 }])).await;
    sim.app
        .db
        .like_house(b, c, chrono::Utc::now().date_naive())
        .await
        .unwrap();
    sim.app.db.add_message_xp(sim.guild, b, 99, 0).await.unwrap();
    let list = sim.command(a, "friends", json!([sub("list", vec![])])).await;
    assert_eq!(list.title(), "👥 Your friends");
    assert!(
        list.description().contains(&format!("**1.** {} · Lv ", mention(b))) && list.description().contains("💖 1"),
        "{}",
        list.description()
    );
    assert!(
        list.embeds[0]
            .footer
            .as_ref()
            .is_some_and(|f| f.text == "1 of 100 friends")
    );
    let theirs = sim
        .command_resolved(
            c,
            "friends",
            json!([sub("list", vec![user_option("user", b)])]),
            named(&[(b, "Bea")]),
        )
        .await;
    assert_eq!(theirs.title(), "👥 Bea's friends", "{}", theirs.title());
    assert!(theirs.description().contains(&mention(a)));
    let empty = sim.command(c, "friends", json!([sub("list", vec![])])).await;
    assert!(
        empty.description().contains("No friends yet"),
        "{}",
        empty.description()
    );

    // /house visit without a name offers the friends whose doors are open.
    let shortlist = sim.command(a, "house", json!([sub("visit", vec![])])).await;
    assert!(shortlist.ephemeral);
    let options = shortlist.options("fd:v:");
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].value, b.to_string());
    assert!(
        options[0]
            .description
            .as_deref()
            .is_some_and(|d| d.contains("Lv ") && d.contains("💖 1")),
        "{:?}",
        options[0].description
    );
    let credits = sim.app.db.user(a).await.unwrap().credits;
    let visit = sim.pick(a, &shortlist, "fd:v:", &b.to_string()).await;
    assert!(
        visit.title().contains("house") || visit.title().contains("Shack") || !visit.title().is_empty(),
        "{}",
        visit.title()
    );
    assert_eq!(visit.files[0].0, "room.png");
    assert!(
        visit.embeds[0]
            .footer
            .as_ref()
            .is_some_and(|f| f.text.contains("shells for visiting")),
        "{:?}",
        visit.embeds[0].footer
    );
    assert_eq!(sim.app.db.user(a).await.unwrap().credits, credits + 25);
    visit.button("Sign guestbook");
    // A locked house leaves the shortlist.
    sim.command(b, "house", json!([sub("lock", vec![bool_option("locked", true)])]))
        .await;
    let shortlist = sim.command(a, "house", json!([sub("visit", vec![])])).await;
    assert!(
        shortlist.content().contains("Name whose house"),
        "{}",
        shortlist.content()
    );
    // Somebody without friends is told how to get some.
    let none = sim.command(c, "house", json!([sub("visit", vec![])])).await;
    assert!(
        none.ephemeral && none.content().contains("/friends add"),
        "{}",
        none.content()
    );
    // Naming someone still works as before.
    let named_visit = sim
        .command_resolved(c, "house", visit_options(a), resolved(&[a], &[]))
        .await;
    assert_eq!(named_visit.files[0].0, "room.png");

    // Remove, and decline.
    let removed = sim
        .command_resolved(
            b,
            "friends",
            json!([sub("remove", vec![user_option("user", a)])]),
            named(&[(a, "Amy")]),
        )
        .await;
    assert!(
        removed.ephemeral && removed.content().contains("no longer your friend"),
        "{}",
        removed.content()
    );
    assert!(!sim.app.db.are_friends(a, b).await.unwrap() && !sim.app.db.are_friends(b, a).await.unwrap());
    let again = sim
        .command_resolved(
            b,
            "friends",
            json!([sub("remove", vec![user_option("user", a)])]),
            named(&[(a, "Amy")]),
        )
        .await;
    assert!(
        again.content().contains("isn't one of your friends"),
        "{}",
        again.content()
    );
    let (options, users) = add(c, "Cy");
    let asked = sim.command_resolved(a, "friends", options, users).await;
    let no = sim.click(c, &asked, "Decline").await;
    assert!(no.content().contains("said no thanks"), "{}", no.content());
    assert!(!sim.app.db.are_friends(a, c).await.unwrap());
    // A request stops being good after a day.
    let (options, users) = add(c, "Cy");
    let asked = sim.command_resolved(a, "friends", options, users).await;
    sim.app.shift_clock(chrono::Duration::hours(25));
    let late = sim.click(c, &asked, "Accept").await;
    assert!(late.content().contains("expired"), "{}", late.content());
    assert!(!sim.app.db.are_friends(a, c).await.unwrap());
}

#[tokio::test]
async fn a_hundred_friends_is_the_most_and_the_list_pages() {
    let Some(mut sim) = Sim::new("friends_cap").await else {
        return;
    };
    let (full, extra) = (unique(), unique());
    let crowd: Vec<u64> = (0..100).map(|n| full + 5_000_000 + n).collect();
    for friend in &crowd {
        sim.app.db.befriend(full, *friend).await.unwrap();
    }
    let (options, users) = (
        json!([sub("add", vec![user_option("user", extra)])]),
        named(&[(extra, "Xi")]),
    );
    let refused = sim.command_resolved(full, "friends", options, users).await;
    assert!(
        refused.ephemeral && refused.content().contains("100 friends"),
        "{}",
        refused.content()
    );
    let (options, users) = (
        json!([sub("add", vec![user_option("user", full)])]),
        named(&[(full, "Fu")]),
    );
    let theirs = sim.command_resolved(extra, "friends", options, users).await;
    assert!(
        theirs.ephemeral && theirs.content().contains("has 100 friends already"),
        "{}",
        theirs.content()
    );

    let first = sim.command(full, "friends", json!([sub("list", vec![])])).await;
    assert!(
        first.embeds[0]
            .footer
            .as_ref()
            .is_some_and(|f| f.text == "Page 1/10 · 100 of 100 friends"),
        "{:?}",
        first.embeds[0].footer
    );
    assert_eq!(first.description().lines().count(), 10);
    assert!(first.button("◀ Newer").disabled && !first.button("Older ▶").disabled);
    let second = sim.click(full, &first, "Older ▶").await;
    assert!(second.description().starts_with("**11.**"), "{}", second.description());
    assert!(!second.button("◀ Newer").disabled);
    // Newest first: the last friend added heads page one.
    assert!(
        first.description().contains(&mention(*crowd.last().unwrap())),
        "{}",
        first.description()
    );
}

#[tokio::test]
async fn hug_pat_wave_and_high_five_send_a_gif_of_the_two_players() {
    let Some(mut sim) = Sim::new("interactions").await else {
        return;
    };
    let (a, b, c) = (unique(), unique(), unique());
    // a and b each design a character, so the GIF shows their own looks and colours.
    let content = Content::get();
    for (user, look, skin, top) in [(a, 0, 1, 0), (b, 2, 5, 4)] {
        let code = format!("{}.{skin}.3.{top}.2", content.looks[look].id);
        sim.app.db.set_character(user, &code).await.unwrap();
    }

    let mut last = None;
    for (command, verb, key) in [
        ("hug", "hugs", "hug"),
        ("pat", "pats", "pat"),
        ("wave", "waves at", "wave"),
        ("highfive", "high-fives", "highfive"),
    ] {
        // Each kind from a different sender, so the 5 s wait between two of one player's is not in the way.
        let sender = unique();
        sim.app
            .db
            .set_character(sender, &format!("{}.2.1.1.3", content.looks[3].id))
            .await
            .unwrap();
        let reply = sim
            .command_resolved(sender, command, json!([user_option("user", b)]), named(&[(b, "Bea")]))
            .await;
        let said = format!("{} {} {verb} {}", content_emoji(key), mention(sender), mention(b));
        assert_eq!(reply.content(), said);
        assert!(
            reply
                .description()
                .contains(&format!("from {} to {}", mention(sender), mention(b))),
            "{}",
            reply.description()
        );
        assert!(reply.description().contains("**#1**"), "{}", reply.description());
        let frames = gif_frames(&reply, &format!("{key}.gif"));
        assert!(frames.len() > 1, "{key} is animated");
        assert!((12..=16).contains(&frames.len()), "{key} has {} frames", frames.len());
        assert!(frames.windows(2).any(|w| w[0] != w[1]), "{key} moves");
        save_frames(&sim, key, &frames);
        let footer = reply.embeds[0]
            .footer
            .as_ref()
            .map(|f| f.text.clone())
            .unwrap_or_default();
        assert!(
            footer.contains("1 ") && footer.contains("given") && footer.contains("received"),
            "{footer}"
        );
        assert!(!footer.contains("/character"), "the sender has a character: {footer}");
        reply.button(&format!(
            "{} back",
            match key {
                "highfive" => "High five".to_owned(),
                other => format!("{}{}", other[..1].to_uppercase(), &other[1..]),
            }
        ));
        last = Some((reply, sender));
    }
    let (high_five, _) = last.unwrap();
    assert_eq!(high_five.files.len(), 1);

    // Counted per pair and per user: a second hug from the same player is #2, once the wait is over.
    let (sender, receiver) = (a, c);
    let started = std::time::Instant::now();
    let first = sim
        .command_resolved(
            sender,
            "hug",
            json!([user_option("user", receiver)]),
            named(&[(receiver, "Cy")]),
        )
        .await;
    assert!(first.description().contains("**#1**"), "{}", first.description());
    // The wait is five seconds from the moment the first went out.
    if started.elapsed() < std::time::Duration::from_secs(4) {
        let eager = sim
            .command_resolved(
                sender,
                "pat",
                json!([user_option("user", receiver)]),
                named(&[(receiver, "Cy")]),
            )
            .await;
        assert!(
            eager.ephemeral && eager.content().contains("Easy there"),
            "{}",
            eager.content()
        );
        assert!(eager.files.is_empty());
        assert_eq!(
            sim.app.db.exchanged(a, c).await.unwrap(),
            vec![("hug".to_owned(), 1, 0)]
        );
    }
    // A cooldown that is running refuses, whoever the receiver is.
    let busy = unique();
    assert!(
        sim.app
            .cache
            .try_cooldown(&format!("social:{busy}"), std::time::Duration::from_secs(30))
            .await
            .unwrap()
            .is_none()
    );
    let eager = sim
        .command_resolved(
            busy,
            "wave",
            json!([user_option("user", receiver)]),
            named(&[(receiver, "Cy")]),
        )
        .await;
    assert!(
        eager.ephemeral && eager.content().contains("Easy there"),
        "{}",
        eager.content()
    );
    assert!(eager.files.is_empty() && sim.app.db.exchanged(busy, c).await.unwrap().is_empty());
    tokio::time::sleep(std::time::Duration::from_millis(5_200)).await;
    // Another receiver starts its own pair count while the sender's total goes on.
    let second = sim
        .command_resolved(sender, "hug", json!([user_option("user", b)]), named(&[(b, "Bea")]))
        .await;
    assert!(second.description().contains("**#1**"), "{}", second.description());
    let footer = second.embeds[0]
        .footer
        .as_ref()
        .map(|f| f.text.clone())
        .unwrap_or_default();
    assert!(footer.contains("2 hugs given"), "{footer}");
    tokio::time::sleep(std::time::Duration::from_millis(5_200)).await;
    let third = sim
        .command_resolved(
            sender,
            "hug",
            json!([user_option("user", receiver)]),
            named(&[(receiver, "Cy")]),
        )
        .await;
    assert!(third.description().contains("**#2**"), "{}", third.description());
    let footer = third.embeds[0]
        .footer
        .as_ref()
        .map(|f| f.text.clone())
        .unwrap_or_default();
    assert!(
        footer.contains("3 hugs given") && footer.contains("2 hugs received"),
        "{footer}"
    );
    assert_eq!(first.files.len(), 1);

    // The receiver can send one back from the button, once; anybody else is told it isn't theirs.
    let not_yours = sim.click(b, &third, "Hug back").await;
    assert!(
        not_yours.ephemeral && not_yours.content().contains(&format!("for {}", mention(receiver))),
        "{}",
        not_yours.content()
    );
    let back = sim.click(receiver, &third, "Hug back").await;
    assert!(back.components.is_empty(), "the button is spent");
    assert_eq!(back.followups.len(), 1);
    let answer = &back.followups[0];
    assert_eq!(
        answer.content(),
        format!("🤗 {} hugs {}", mention(receiver), mention(sender))
    );
    assert!(
        answer.description().contains("**#1**")
            && answer
                .description()
                .contains(&format!("from {} to {}", mention(receiver), mention(sender))),
        "{}",
        answer.description()
    );
    assert!(gif_frames(answer, "hug.gif").len() > 1);
    assert_eq!(
        sim.app.db.exchanged(a, c).await.unwrap(),
        vec![("hug".to_owned(), 2, 1)]
    );

    // Nobody hugs themselves or a bot, and a player without a character is still on screen.
    let own = sim
        .command_resolved(unique(), "hug", json!([user_option("user", 77)]), named(&[(77, "x")]))
        .await;
    assert!(own.files.len() == 1, "someone else's id");
    let me = unique();
    let myself = sim
        .command_resolved(me, "hug", json!([user_option("user", me)]), named(&[(me, "Me")]))
        .await;
    assert!(
        myself.ephemeral && myself.content().contains("can't give yourself a hug"),
        "{}",
        myself.content()
    );
    let bot = json!({ "users": { "55": person_json(55, "robo", Some("Robo"), true) } });
    let robot = sim
        .command_resolved(unique(), "wave", json!([user_option("user", 55)]), bot)
        .await;
    assert!(
        robot.ephemeral && robot.content().contains("bots"),
        "{}",
        robot.content()
    );
    let footer = own.embeds[0]
        .footer
        .as_ref()
        .map(|f| f.text.clone())
        .unwrap_or_default();
    assert!(
        footer.contains("Design your own with /character"),
        "no saved character: {footer}"
    );
}

/// The emoji each command starts its message with.
fn content_emoji(key: &str) -> &'static str {
    match key {
        "hug" => "🤗",
        "pat" => "🫳",
        "wave" => "👋",
        _ => "🙌",
    }
}
