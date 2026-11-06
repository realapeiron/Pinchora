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
    cell::{Cell, RefCell},
    collections::HashMap,
    fmt::{self, Write as _},
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock, Mutex, Once,
        atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicUsize, Ordering},
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
    App, Deferral, Responder, handle_interaction, on_ban_add, on_ban_remove, on_guild_create, on_guild_delete,
    on_member_add, on_member_remove, on_member_update, on_message, on_voice_state_update, reminder_tick, sweep_games,
};
use pinchora_db::{Counter, Db};
use pinchora_game::{
    Content,
    character::Character,
    play::{PlayContent, hangman::Hangman, trivia, wordle},
};
use pinchora_render::{
    BadgeCollectionCard,
    character::{Paint, Slot},
};
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
        BanAdd, BanRemove, GuildCreate, GuildDelete, MemberAdd, MemberRemove, MemberUpdate, MessageCreate,
        VoiceStateUpdate,
    },
    http::interaction::{InteractionResponse, InteractionResponseType},
    id::Id,
};

// The everyday commands (ping, help, bio, rep, give, the shops, stalls, the plaza, quests, house and config basics).
#[path = "simulate/basics.rs"]
mod basics;
#[path = "simulate/board.rs"]
mod board;
// Welcome and goodbye images from the dashboard's editor.
#[path = "simulate/cards.rs"]
mod cards;
// The collection games' flows (Critter Drop, bottles, Beachcomb, Tide Pool Dex, Crab Pot).
#[path = "simulate/collection.rs"]
mod collection;
// Moderation commands, cases and the mod-log; the detailed server log.
#[path = "simulate/moderation.rs"]
mod moderation;
// Correctness follow-ups: stale buttons, forfeits, repeated requests, forged ids, voice XP at scale.
#[path = "simulate/w9a.rs"]
mod w9a;
// Text commands with a server's prefix, through the same handlers.
#[path = "simulate/text.rs"]
mod text;
// The party games' flows (Hot Pearl, Acronym Tide, Caption the Postcard, Wordle Race, Would You Rather, Truth or Dare).
#[path = "simulate/turkish.rs"]
mod turkish;
#[path = "simulate/words.rs"]
mod words;
// Custom commands (/customcommand, /tag) and auto-responders.
#[path = "simulate/tags.rs"]
mod tags;
// What a switched-off module still did: timers, the chat XP minute, quests, the level-up channel.
#[path = "simulate/carryover.rs"]
mod carryover;
// Who may set a role up to be handed out.
#[path = "simulate/grants.rs"]
mod grants;
// What reaches the (still empty) feature hooks.
#[path = "simulate/hooks.rs"]
mod hooks;

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

/// The seconds a countdown such as "2 h 5 min" or "1 h" names at the start of `text`: the game writes
/// remaining time rounded up to the minute.
fn span_secs(text: &str) -> i64 {
    let words: Vec<&str> = text.split_whitespace().collect();
    let (mut secs, mut at) = (0, 0);
    while let (Some(number), Some(unit)) = (words.get(at), words.get(at + 1)) {
        let Ok(number) = number.parse::<i64>() else { break };
        match unit.trim_end_matches(['.', ',']) {
            "h" => secs += number * 3600,
            "min" => secs += number * 60,
            _ => break,
        }
        at += 2;
    }
    secs
}

/// Checks that `text` has a countdown after `marker` that started at `full` seconds and has run down by
/// no more than this test has been running since `began` (plus the minute it is rounded up to). The
/// game counts down in real time, so on a slow machine "1 h" is a minute short by the time it is read.
fn assert_counting_down(text: &str, marker: &str, full: i64, began: std::time::Instant) {
    let (_, rest) = text
        .split_once(marker)
        .unwrap_or_else(|| panic!("no {marker:?} in {text:?}"));
    let left = span_secs(rest);
    let ran = began.elapsed().as_secs() as i64;
    assert!(
        left <= full && left >= full - ran - 60,
        "{marker}{left} s left of {full} s after {ran} s: {text}"
    );
}

/// Closes the games earlier runs left late in the database. The janitor takes the oldest fifty late
/// games of the whole database, so enough leftovers would keep it from ever reaching a test's own
/// game; and what it does to them is nobody's business here. Call it with the sweep turn held.
async fn forget_late_games(sim: &Sim) {
    sqlx::query("UPDATE game_sessions SET status = 'done' WHERE status <> 'done' AND expires_at < now()")
        .execute(sim.app.db.pool())
        .await
        .expect("close the late games of earlier runs");
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
    /// Channels the bot may not post in: posting answers Missing Permissions.
    blocked: Arc<Mutex<Vec<u64>>>,
    /// Channels `GET /channels/{id}` knows, as Discord's JSON; any other answers Unknown Channel.
    channels: Arc<Mutex<HashMap<u64, Value>>>,
    /// How often each channel was looked up.
    channel_lookups: Arc<Mutex<HashMap<u64, usize>>>,
    /// Whether looking up the server itself fails (Discord is down).
    guild_lookup_fails: Arc<AtomicBool>,
    /// Who owns the server, as `GET /guilds/{id}` says (the bot, 1, until a test hands it to someone).
    owner: Arc<AtomicU64>,
    /// Users the server has banned.
    bans: Arc<Mutex<Vec<u64>>>,
    /// Whether bans are refused (Missing Permissions).
    bans_refused: Arc<AtomicBool>,
    /// Moderation calls seen: bans, kicks, member edits, deletes, channel edits and overwrites.
    mod_calls: Arc<Mutex<Vec<String>>>,
    /// A channel's recent messages, newest first, as `GET /channels/{id}/messages` answers them.
    history: Arc<Mutex<HashMap<u64, Vec<Value>>>>,
    /// The bot's global commands, as Discord's JSON.
    global_commands: Arc<Mutex<Vec<Value>>>,
    /// What the server set for the bot's commands under Integrations, as Discord's JSON (a list of
    /// guild application command permissions); `None` answers Unknown Application.
    command_permissions: Arc<Mutex<Option<Value>>>,
    /// An HTTP status the command permissions route answers instead (0: none).
    command_permissions_status: Arc<AtomicU16>,
    /// How often the command permissions were asked for.
    command_permission_lookups: Arc<AtomicUsize>,
    /// Messages somebody deleted through the API, as (channel, message).
    deleted_messages: Arc<Mutex<Vec<(u64, u64)>>>,
    /// Whether the server's role list has `@everyone` (Discord's always does; most tests never ask).
    everyone_role: Arc<AtomicBool>,
}

/// A message a handler posted to a channel (`POST`) or edited there (`PATCH`).
struct Post {
    method: Method,
    channel: u64,
    message: u64,
    body: Value,
    /// Images that came with it.
    files: Vec<File>,
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

    fn description(&self) -> &str {
        self.embed()["description"].as_str().unwrap_or_default()
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
        "afk_channel_id": afk_channel(id).map(|c| c.to_string()),
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
        (&Method::GET, [.., "guilds", guild, "roles"]) => {
            let mut roles = vec![role_json(BOT_ROLE_ID, "Pinchora", BOT_ROLE_POSITION, true)];
            if mock.everyone_role.load(Ordering::SeqCst) {
                // View Channel, Send Messages and Use Application Commands: what a new server gives everyone.
                let mut everyone = role_json(number(guild), "@everyone", 0, false);
                everyone["permissions"] = json!((1u64 << 10 | 1 << 11 | 1 << 31).to_string());
                roles.push(everyone);
            }
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
        (&Method::PUT, [.., "guilds", _, "bans", user]) => {
            if mock.bans_refused.load(Ordering::SeqCst) {
                let refusal = json!({ "message": "Missing Permissions", "code": 50013 });
                return (StatusCode::FORBIDDEN, refusal.to_string());
            }
            let (body, _) = request_body(&headers, &body);
            mock.bans.lock().unwrap().push(number(user));
            let seconds = body["delete_message_seconds"].as_u64().unwrap_or(0);
            mock.mod_calls.lock().unwrap().push(format!("ban {user} {seconds}"));
            return (StatusCode::NO_CONTENT, String::new());
        }
        (&Method::GET, [.., "guilds", _, "bans", user]) => {
            if !mock.bans.lock().unwrap().contains(&number(user)) {
                let gone = json!({ "message": "Unknown Ban", "code": 10026 });
                return (StatusCode::NOT_FOUND, gone.to_string());
            }
            let ban = json!({ "reason": null, "user": person_json(number(user), "banned", None, false) });
            return (StatusCode::OK, ban.to_string());
        }
        (&Method::DELETE, [.., "guilds", _, "bans", user]) => {
            let mut bans = mock.bans.lock().unwrap();
            if !bans.contains(&number(user)) {
                let gone = json!({ "message": "Unknown Ban", "code": 10026 });
                return (StatusCode::NOT_FOUND, gone.to_string());
            }
            bans.retain(|b| *b != number(user));
            mock.mod_calls.lock().unwrap().push(format!("unban {user}"));
            return (StatusCode::NO_CONTENT, String::new());
        }
        (&Method::DELETE, [.., "guilds", _, "members", user]) => {
            mock.mod_calls.lock().unwrap().push(format!("kick {user}"));
            return (StatusCode::NO_CONTENT, String::new());
        }
        (&Method::PATCH, [.., "guilds", _, "members", user]) => {
            let (body, _) = request_body(&headers, &body);
            mock.mod_calls.lock().unwrap().push(format!("member {user} {body}"));
            let roles = mock
                .member_roles
                .lock()
                .unwrap()
                .get(&number(user))
                .cloned()
                .unwrap_or_default();
            return (StatusCode::OK, member_json(number(user), &roles).to_string());
        }
        (&Method::GET, [.., "channels", channel, "messages"]) => {
            let history = mock
                .history
                .lock()
                .unwrap()
                .get(&number(channel))
                .cloned()
                .unwrap_or_default();
            return (StatusCode::OK, Value::Array(history).to_string());
        }
        (&Method::POST, [.., "channels", _, "messages", "bulk-delete"]) => {
            let (body, _) = request_body(&headers, &body);
            let n = body["messages"].as_array().map_or(0, Vec::len);
            mock.mod_calls.lock().unwrap().push(format!("bulk-delete {n}"));
            return (StatusCode::NO_CONTENT, String::new());
        }
        (&Method::PATCH, [.., "channels", channel]) => {
            let (body, _) = request_body(&headers, &body);
            mock.mod_calls.lock().unwrap().push(format!("channel {channel} {body}"));
            let known = mock.channels.lock().unwrap().get(&number(channel)).cloned();
            let answer = known.unwrap_or_else(|| json!({ "id": channel, "type": 0, "name": "sim-channel" }));
            return (StatusCode::OK, answer.to_string());
        }
        (&Method::PUT, [.., "channels", channel, "permissions", id]) => {
            let (body, _) = request_body(&headers, &body);
            mock.mod_calls.lock().unwrap().push(format!(
                "overwrite {channel} {id} allow={} deny={}",
                body["allow"], body["deny"]
            ));
            if let Some(known) = mock.channels.lock().unwrap().get_mut(&number(channel)) {
                let mut overwrites: Vec<Value> = known["permission_overwrites"].as_array().cloned().unwrap_or_default();
                overwrites.retain(|o| o["id"] != json!(id));
                overwrites
                    .push(json!({ "id": id, "type": body["type"], "allow": body["allow"], "deny": body["deny"] }));
                known["permission_overwrites"] = Value::Array(overwrites);
            }
            return (StatusCode::NO_CONTENT, String::new());
        }
        (&Method::DELETE, [.., "channels", channel, "permissions", id]) => {
            mock.mod_calls
                .lock()
                .unwrap()
                .push(format!("overwrite-delete {channel} {id}"));
            if let Some(known) = mock.channels.lock().unwrap().get_mut(&number(channel))
                && let Some(list) = known["permission_overwrites"].as_array_mut()
            {
                list.retain(|o| o["id"] != json!(id));
            }
            return (StatusCode::NO_CONTENT, String::new());
        }
        (&Method::GET, [.., "applications", _, "commands"]) => {
            let commands = mock.global_commands.lock().unwrap().clone();
            return (StatusCode::OK, Value::Array(commands).to_string());
        }
        (&Method::GET, [.., "applications", _, "guilds", _, "commands", "permissions"]) => {
            mock.command_permission_lookups.fetch_add(1, Ordering::SeqCst);
            let status = mock.command_permissions_status.load(Ordering::SeqCst);
            if status != 0 {
                let down = json!({ "message": "Missing Access", "code": 50001 });
                return (StatusCode::from_u16(status).expect("status"), down.to_string());
            }
            return match mock.command_permissions.lock().unwrap().clone() {
                Some(list) => (StatusCode::OK, list.to_string()),
                None => {
                    let gone = json!({ "message": "Unknown application command permissions", "code": 10066 });
                    (StatusCode::NOT_FOUND, gone.to_string())
                }
            };
        }
        (&Method::DELETE, [.., "channels", channel, "messages", id]) => {
            mock.mod_calls.lock().unwrap().push(format!("delete {id}"));
            mock.deleted_messages
                .lock()
                .unwrap()
                .push((number(channel), number(id)));
            return (StatusCode::NO_CONTENT, String::new());
        }
        (&Method::GET, [.., "guilds", _]) if mock.guild_lookup_fails.load(Ordering::SeqCst) => {
            let down = json!({ "message": "Service unavailable", "code": 0 });
            return (StatusCode::INTERNAL_SERVER_ERROR, down.to_string());
        }
        (&Method::GET, [.., "guilds", guild]) => {
            let mut answer = guild_json(number(guild));
            answer["owner_id"] = json!(mock.owner.load(Ordering::SeqCst).to_string());
            return (StatusCode::OK, answer.to_string());
        }
        (&Method::GET, [.., "channels", channel]) => {
            *mock.channel_lookups.lock().unwrap().entry(number(channel)).or_default() += 1;
            return match mock.channels.lock().unwrap().get(&number(channel)) {
                Some(channel) => (StatusCode::OK, channel.to_string()),
                None => {
                    let gone = json!({ "message": "Unknown Channel", "code": 10003 });
                    (StatusCode::NOT_FOUND, gone.to_string())
                }
            };
        }
        (&Method::POST, [.., "users", "@me", "channels"]) => {
            let (body, _) = request_body(&headers, &body);
            return open_direct_message(number(body["recipient_id"].as_str().unwrap_or_default()));
        }
        (&Method::POST, [.., "channels", channel, "messages"]) => {
            let (body, files) = request_body(&headers, &body);
            let (channel, id) = (number(channel), unique());
            if mock.blocked.lock().unwrap().contains(&channel) {
                let refusal = json!({ "message": "Missing Permissions", "code": 50013 });
                return (StatusCode::FORBIDDEN, refusal.to_string());
            }
            mock.posts.lock().unwrap().push(Post {
                method: method.clone(),
                channel,
                message: id,
                body,
                files,
            });
            return (StatusCode::OK, message_json(channel, id).to_string());
        }
        (&Method::PATCH, [.., "channels", channel, "messages", id]) => {
            let (channel, id) = (number(channel), number(id));
            if mock.deleted.lock().unwrap().contains(&id) {
                let gone = json!({ "message": "Unknown Message", "code": 10008 });
                return (StatusCode::NOT_FOUND, gone.to_string());
            }
            let (body, files) = request_body(&headers, &body);
            mock.posts.lock().unwrap().push(Post {
                method: method.clone(),
                channel,
                message: id,
                body,
                files,
            });
            return (StatusCode::OK, message_json(channel, id).to_string());
        }
        _ => {}
    }
    // A handler asking which message it just posted (`GET .../messages/@original`).
    if method == Method::GET && path.contains("/webhooks/") && path.ends_with("/messages/@original") {
        return (StatusCode::OK, message_json(CHANNEL_ID, unique()).to_string());
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
    /// How many statements the database ran for this thread so far (sqlx logs each one).
    static QUERIES: Cell<usize> = const { Cell::new(0) };
}

/// The statements the database has run for this test so far.
fn queries() -> usize {
    QUERIES.get()
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
        if event.metadata().target() == "sqlx::query" {
            QUERIES.set(QUERIES.get() + 1);
        }
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
    /// The form the handler opened instead of answering, as its custom id and title.
    modal: Option<(String, String)>,
    /// The text boxes of that form as (custom id, prefilled value).
    modal_inputs: Vec<(String, Option<String>)>,
    /// The `allowed_mentions` the handler sent with its last answer, if it sent any.
    mentions: Option<Value>,
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
            modal_inputs: Vec::new(),
            mentions: None,
        }
    }

    /// Whether the handler told Discord to ping nobody: `allowed_mentions` is there and parses nothing.
    fn pings_nobody(&self) -> bool {
        self.mentions == Some(json!({ "parse": [] }))
    }

    /// Whether the handler told Discord to ping `user` and nobody else.
    fn pings_only(&self, user: u64) -> bool {
        self.mentions == Some(json!({ "parse": [], "users": [user.to_string()] }))
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
                modal_inputs: Vec::new(),
                mentions: None,
                ..on.clone()
            },
            _ => Self::new(first.kind),
        };
        if let Some(data) = first.data {
            if first.kind == InteractionResponseType::Modal {
                reply.modal = data.custom_id.clone().zip(data.title.clone());
                reply.modal_inputs = data
                    .components
                    .iter()
                    .flatten()
                    .filter_map(|component| match component {
                        Component::Label(label) => match label.component.as_ref() {
                            Component::TextInput(input) => Some((input.custom_id.clone(), input.value.clone())),
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect();
                return reply;
            }
            if let Some(flags) = data.flags {
                reply.ephemeral = flags.contains(MessageFlags::EPHEMERAL);
            }
            reply.mentions = data
                .allowed_mentions
                .map(|m| serde_json::to_value(m).expect("mentions as JSON"));
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
                reply.mentions = body.get("allowed_mentions").cloned();
                if body.get("attachments").is_some() {
                    reply.files = call.files;
                }
            } else {
                let mut followup = Self::new(InteractionResponseType::ChannelMessageWithSource);
                followup.ephemeral = body["flags"].as_u64().is_some_and(|f| f & 64 != 0);
                followup.mentions = body.get("allowed_mentions").cloned();
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

    /// Whether the message has a button with this label.
    fn has_button(&self, label: &str) -> bool {
        self.controls()
            .any(|c| matches!(c, Component::Button(b) if b.label.as_deref() == Some(label)))
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
        if let Some((custom_id, title)) = &self.modal {
            writeln!(f, "  form: {title} ({custom_id})")?;
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
    /// The channel interactions come from; tests that need channel-wide cooldowns to themselves
    /// pick their own.
    channel: u64,
    /// The invoker's Discord client language.
    locale: &'static str,
}

impl Sim {
    async fn new(name: &'static str) -> Option<Self> {
        Self::with(name, |_| {}).await
    }

    /// [`Sim::new`] with a last change to the [`App`] before it is shared, like a config switch.
    async fn with(name: &'static str, configure: impl FnOnce(&mut App)) -> Option<Self> {
        let (Ok(database), Ok(redis)) = (
            std::env::var("PINCHORA_TEST_DATABASE_URL"),
            std::env::var("PINCHORA_TEST_REDIS_URL"),
        ) else {
            eprintln!("{name}: skipped, set PINCHORA_TEST_DATABASE_URL and PINCHORA_TEST_REDIS_URL");
            return None;
        };
        init_logging();
        let db = Db::connect_waiting(&database, 5, std::time::Duration::from_secs(60))
            .await
            .expect("connect to the database");
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
            blocked: Arc::default(),
            channels: Arc::default(),
            channel_lookups: Arc::default(),
            guild_lookup_fails: Arc::default(),
            owner: Arc::new(AtomicU64::new(APPLICATION_ID)),
            bans: Arc::default(),
            bans_refused: Arc::default(),
            mod_calls: Arc::default(),
            history: Arc::default(),
            global_commands: Arc::default(),
            command_permissions: Arc::default(),
            command_permissions_status: Arc::default(),
            command_permission_lookups: Arc::default(),
            deleted_messages: Arc::default(),
            everyone_role: Arc::default(),
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
        // As on a bot with the Message Content intent: text commands answer to the server's prefix.
        app.message_content = true;
        // Avatars come from Discord's CDN. The mock refuses the tunnel, so cards get the
        // placeholder avatar, offline and the same every run.
        app.web = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{mock}")).expect("proxy"))
            .build()
            .expect("http client");
        configure(&mut app);

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
            channel: CHANNEL_ID,
            locale: "en-US",
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

    /// Makes `user` the owner of the server, who may set up any role the bot can hand out.
    fn own_server(&self, user: u64) {
        self.mock.owner.store(user, Ordering::SeqCst);
    }

    /// Like [`Sim::guild_role`] with permission bits.
    fn guild_role_with(&self, id: u64, name: &str, position: i64, permissions: u64) -> Value {
        let mut role = role_json(id, name, position, false);
        role["permissions"] = json!(permissions.to_string());
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
            for (filename, bytes) in &post.files {
                let path = self.dir.join(format!("post-{}-{filename}", post.message));
                std::fs::write(&path, bytes).expect("save the image");
                let image =
                    image::load_from_memory(bytes).unwrap_or_else(|e| panic!("{filename} is not a valid image: {e}"));
                println!("  image: {} ({}x{})", path.display(), image.width(), image.height());
            }
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

    /// Takes away the bot's right to post in `channel`: posting there answers Missing Permissions.
    fn block_channel(&self, channel: u64) {
        self.mock.blocked.lock().unwrap().push(channel);
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
    async fn submit_fields(&mut self, user: u64, on: &Reply, fields: &[(&str, &str)]) -> Reply {
        let (custom_id, _) = on.modal.clone().expect("the step opened no modal");
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

    /// Submits the form `modal` opened with `text` typed into its only text input, from the
    /// message `on` the button was on.
    async fn submit(&mut self, user: u64, modal: &Reply, text: &str, on: &Reply) -> Reply {
        let (custom_id, title) = modal.modal.clone().expect("the reply is not a form");
        let data = json!({
            "custom_id": custom_id,
            "components": [{
                "type": 18,
                "id": 1,
                "component": { "type": 4, "id": 2, "custom_id": "guess", "value": text },
            }],
        });
        self.run(format!("submit [{title}] {text:?}"), user, 5, data, Some(on))
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
            "channel": { "id": self.channel.to_string(), "type": 0, "guild_id": self.guild.to_string() },
            "locale": self.locale,
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
        let deferral = responder.deferral();
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
        // What the gateway's slow-handler defer assumes: a command it makes private never answers
        // publicly, and a form can only follow an answer that is no defer.
        if kind == 2 && deferral == Deferral::Private {
            assert!(
                reply.ephemeral,
                "step {step} ({label}): listed as answering privately, but answered publicly"
            );
        }
        if reply.modal.is_some() {
            assert_eq!(
                deferral,
                Deferral::Never,
                "step {step} ({label}): opened a form, which a defer would have broken"
            );
        }
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

/// A player whose Discord is in Turkish gets Turkish replies and Turkish cards; `/language set`
/// overrides the client, and "Automatic" goes back to it.
#[tokio::test]
async fn turkish_players_get_turkish_text_and_cards() {
    let Some(mut sim) = Sim::new("turkish_players_get_turkish_text_and_cards").await else {
        return;
    };
    let user = unique();
    let set = |value: &str| json!([{ "name": "set", "type": 1, "options": [{ "name": "language", "type": 3, "value": value }] }]);
    let english = card_pixels(&sim.command(user, "profile", json!([])).await, (960, 540));
    let english_rank = card_pixels(&sim.command(user, "rank", json!([])).await, (934, 282));

    sim.locale = "tr";
    let profile = sim.command(user, "profile", json!([])).await;
    assert_eq!(profile.files[0].0, "profile.png");
    let turkish = card_pixels(&profile, (960, 540));
    assert!(
        differ_in(&english, &turkish, 60..264, 296..316),
        "the LEVEL label is SEVİYE"
    );
    let rank = sim.command(user, "rank", json!([])).await;
    assert!(
        differ_in(&english_rank, &card_pixels(&rank, (934, 282)), 600..900, 56..78),
        "RANK and LEVEL"
    );
    let help = sim.command(user, "help", json!([])).await;
    assert!(help.description().contains("Menüden"), "{}", help.description());
    assert_eq!(help.files[0].0, "help.png");

    // The player's own pick beats the client.
    let picked = sim.command(user, "language", set("en")).await;
    assert!(picked.content().contains("**English**"), "{}", picked.content());
    let help = sim.command(user, "help", json!([])).await;
    assert!(help.description().contains("Pick an area"), "{}", help.description());
    sim.locale = "en-GB";
    let picked = sim.command(user, "language", set("tr")).await;
    assert!(picked.content().contains("**Türkçe**"), "{}", picked.content());
    let help = sim.command(user, "help", json!([])).await;
    assert!(help.description().contains("Menüden"), "{}", help.description());
    // Automatic: the client's language again.
    let auto = sim.command(user, "language", set("auto")).await;
    assert!(auto.content().contains("Discord"), "{}", auto.content());
    assert_eq!(sim.app.db.user_language(user).await.expect("read"), None);
    let help = sim.command(user, "help", json!([])).await;
    assert!(help.description().contains("Pick an area"), "{}", help.description());

    // A server default answers nobody in particular; the reply is still in the player's language.
    sim.locale = "tr";
    sim.permissions = 0x20;
    let config =
        json!([{ "name": "language", "type": 1, "options": [{ "name": "language", "type": 3, "value": "tr" }] }]);
    let config = sim.command(user, "config", config).await;
    assert!(
        config.content().contains("varsayılan dili artık **Türkçe**"),
        "{}",
        config.content()
    );
    assert_eq!(sim.app.guild_lang(sim.guild).await, pinchora_core::i18n::Lang::Tr);

    // The rest of the everyday commands, in Turkish.
    let daily = sim.command(user, "daily", json!([])).await;
    assert!(daily.title().ends_with("kabuk"), "{}", daily.title());
    assert!(
        daily.description().contains("Serinin **1**. günü"),
        "{}",
        daily.description()
    );
    let wallet = sim.command(user, "balance", json!([])).await;
    assert_eq!(wallet.files[0].0, "wallet.png");
    let privacy = sim.command(user, "privacy", json!([])).await;
    assert_eq!(privacy.title(), "🔒 Gizlilik");
    assert!(!privacy.button("Verilerimi sil…").disabled);
    let character = sim.command(user, "character", json!([])).await;
    assert_eq!(character.title(), "Karakterin");
    assert!(!character.button("Rastgele").disabled);
    let backgrounds = sim
        .command(user, "customize", json!([{ "name": "background", "type": 1 }]))
        .await;
    assert_eq!(backgrounds.title(), "Kart arka planları");

    // The house editor: its title, hints and buttons.
    let edit = sim.command(user, "house", json!([{ "name": "edit", "type": 1 }])).await;
    assert!(edit.title().starts_with("1. oda düzenleniyor"), "{}", edit.title());
    assert!(
        edit.description().contains("Elinde: **hiçbir şey**"),
        "{}",
        edit.description()
    );
    assert!(edit.button("Yerleştir").disabled, "nothing held, nothing to place");
    let furniture = edit
        .options("he:s:")
        .iter()
        .find(|o| !o.value.starts_with('<') && o.value != "none")
        .expect("the starter kit is in the inventory")
        .value
        .clone();
    let held = sim.pick(user, &edit, "he:s:", &furniture).await;
    assert!(held.description().contains("Elinde **"), "{}", held.description());
    let placed = sim.click(user, &held, "Yerleştir").await;
    assert!(
        placed.description().contains("yerleştirildi"),
        "{}",
        placed.description()
    );
    let done = sim.click(user, &placed, "Bitti").await;
    assert!(done.content().starts_with("Kaydedildi!"), "{}", done.content());
}

/// Emoji and CJK in a name draw as letters on the profile card, where they once drew as empty boxes: the name
/// differs from the same name written in a script no font has, which draws as boxes.
#[tokio::test]
async fn names_with_emoji_and_cjk_draw_on_the_profile_card() {
    let Some(mut sim) = Sim::new("names_with_emoji_and_cjk_draw_on_the_profile_card").await else {
        return;
    };
    let (user, other) = (unique(), unique());
    let mut profile_of = async |name: &str| {
        let resolved = json!({ "users": { other.to_string(): person_json(other, "zoe", Some(name), false) } });
        let reply = sim
            .command_resolved(user, "profile", json!([user_option("user", other)]), resolved)
            .await;
        assert_eq!(reply.files[0].0, "profile.png");
        card_pixels(&reply, (960, 540))
    };
    let glyphs = profile_of("Zoë ✨ 李 😀").await;
    let boxes = profile_of("Zoë አ አ አ").await;
    assert!(
        differ_in(&glyphs, &boxes, 290..924, 44..96),
        "the emoji and the Chinese letter are drawn, not boxes"
    );
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

// /Games

/// Today as the games count it: days since 1970-01-01, UTC.
fn today() -> i64 {
    chrono::Utc::now().timestamp().div_euclid(86_400)
}

async fn fund(sim: &Sim, user: u64, shells: i64) {
    sim.app.db.earn_shells(user, shells, "sim").await.expect("fund");
}

async fn shells(sim: &Sim, user: u64) -> i64 {
    sim.app.db.user(user).await.expect("user row").credits
}

/// What `/play stats` keeps for a player: games played and won.
async fn record_of(sim: &Sim, user: u64, game: &str) -> (i32, i32) {
    sim.app
        .db
        .game_stats(user)
        .await
        .expect("stats")
        .iter()
        .find(|s| s.game == game)
        .map_or((0, 0), |s| (s.played, s.won))
}

/// `/duel <game> user [options]`.
async fn duel(sim: &mut Sim, host: u64, guest: u64, game: &str, extra: Vec<Value>) -> Reply {
    let mut options = vec![user_option("user", guest)];
    options.extend(extra);
    sim.command_resolved(host, "duel", json!([sub(game, options)]), resolved(&[guest], &[]))
        .await
}

/// The session id inside a message's buttons (`c4:m:<id>:<col>`, `tv:<id>:<slot>`, ...).
fn session_of(reply: &Reply) -> i64 {
    reply
        .controls()
        .find_map(|c| match c {
            Component::Button(b) => b.custom_id.as_deref(),
            Component::SelectMenu(m) => Some(m.custom_id.as_str()),
            _ => None,
        })
        .and_then(|id| {
            let parts: Vec<&str> = id.split(':').collect();
            // `tv:<id>:<slot>` and `hm:<id>:<half>` lead with the id; the duels' `c4:m:<id>:<col>` follow an action.
            let at = if matches!(parts[0], "tv" | "hm") { 1 } else { 2 };
            parts.get(at)?.parse().ok()
        })
        .expect("a control with a session id")
}

/// The janitor ends every late game in the database, whoever's it is, so tests that make games
/// late take turns.
static SWEEPS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The message a session is shown in, as the database knows it.
async fn message_of(sim: &Sim, session: i64) -> u64 {
    sim.app
        .db
        .game(session)
        .await
        .expect("session")
        .and_then(|g| g.message_id)
        .expect("the session knows its message") as u64
}

/// Makes a session's clock run out, as if its deadline had passed.
async fn expire(sim: &Sim, session: i64) {
    sqlx::query("UPDATE game_sessions SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(session)
        .execute(sim.app.db.pool())
        .await
        .expect("expire the session");
}

#[tokio::test]
async fn tic_tac_toe_counts_in_the_stats() {
    let Some(mut sim) = Sim::new("tic_tac_toe_counts_in_the_stats").await else {
        return;
    };
    let (x, o) = (unique(), unique());
    let mut game = duel(&mut sim, x, o, "tictactoe", vec![]).await;
    let click = |game: &Reply, cell: usize| match game.controls().nth(cell) {
        Some(Component::Button(b)) => b.custom_id.clone().expect("custom id"),
        other => panic!("cell {cell} is {other:?}"),
    };
    // X takes the top row while O answers in the middle row.
    for (player, cell) in [(x, 0), (o, 3), (x, 1), (o, 4), (x, 2)] {
        let id = click(&game, cell);
        game = sim.component(player, &id, &[], &game).await;
    }
    assert!(game.content().contains("wins"), "{}", game.content());
    assert_eq!(record_of(&sim, x, "tictactoe").await, (1, 1));
    assert_eq!(record_of(&sim, o, "tictactoe").await, (1, 0));
}

#[tokio::test]
async fn connect_four_plays_to_a_win_and_pays_once() {
    let Some(mut sim) = Sim::new("connect_four_plays_to_a_win_and_pays_once").await else {
        return;
    };
    let (a, b, bystander) = (unique(), unique(), unique());

    let challenge = duel(&mut sim, a, b, "connect4", vec![]).await;
    assert!(
        !challenge.ephemeral && challenge.content().contains("challenges"),
        "{}",
        challenge.content()
    );
    assert!(challenge.content().contains("No wager"), "{}", challenge.content());
    let refused = sim.click(a, &challenge, "Accept").await;
    assert!(
        refused.ephemeral && refused.content().contains("You sent this challenge"),
        "{}",
        refused.content()
    );
    let refused = sim.click(bystander, &challenge, "Accept").await;
    assert!(
        refused.ephemeral && refused.content().contains("isn't for you"),
        "{}",
        refused.content()
    );

    let board = sim.click(b, &challenge, "Accept").await;
    assert_eq!((board.files.len(), board.files[0].0.as_str()), (1, "connect4.png"));
    assert_eq!(board.rows(), 2, "columns 1-4 and 5-7");
    assert!(
        board.content().contains(&format!("<@{a}>'s turn")),
        "{}",
        board.content()
    );
    let refused = sim.click(b, &board, "3").await;
    assert!(
        refused.ephemeral && refused.content().contains("Not your turn"),
        "{}",
        refused.content()
    );
    let refused = sim.click(bystander, &board, "3").await;
    assert!(
        refused.ephemeral && refused.content().contains("not in this game"),
        "{}",
        refused.content()
    );

    // A stacks column 1 while B answers in column 2: A's fourth disc wins.
    let mut board = board;
    for (player, label) in [(a, "1"), (b, "2"), (a, "1"), (b, "2"), (a, "1"), (b, "2")] {
        board = sim.click(player, &board, label).await;
        assert!(board.components.len() == 2, "the game is still on");
    }
    assert!(
        board.content().contains(&format!("<@{a}>'s turn")),
        "{}",
        board.content()
    );
    let won = sim.click(a, &board, "1").await;
    assert!(
        won.content().contains(&format!("<@{a}> wins!")) && won.content().contains("+**25** shells"),
        "{}",
        won.content()
    );
    assert!(
        won.components.is_empty() && won.files.len() == 1,
        "the final board has no buttons"
    );
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (25, 0));
    assert_eq!(record_of(&sim, a, "connect4").await, (1, 1));
    assert_eq!(record_of(&sim, b, "connect4").await, (1, 0));
    let late = sim
        .component(b, &board.button("2").custom_id.clone().expect("id"), &[], &board)
        .await;
    assert!(late.ephemeral && late.content().contains("over"), "{}", late.content());
    assert_eq!(shells(&sim, a).await, 25, "a finished game pays no second time");

    let stats = sim.command(a, "play", json!([sub("stats", vec![])])).await;
    assert!(
        stats.field("🔴 Connect four").contains("**1** won of **1**"),
        "{}",
        stats.field("🔴 Connect four")
    );
    assert!(
        stats.description().contains("**1** wins in **1** games"),
        "{}",
        stats.description()
    );
}

#[tokio::test]
async fn connect_four_rewards_stop_at_the_daily_limit() {
    let Some(mut sim) = Sim::new("connect_four_rewards_stop_at_the_daily_limit").await else {
        return;
    };
    let (a, b) = (unique(), unique());
    // The cap counts paid wins per player per day; five are already on the counter.
    for _ in 0..5 {
        sim.app
            .cache
            .incr(
                &format!("gamewin:connect4:{}:{a}", today()),
                std::time::Duration::from_secs(3600),
            )
            .await
            .expect("counter");
    }
    let challenge = duel(&mut sim, a, b, "connect4", vec![]).await;
    let mut board = sim.click(b, &challenge, "Accept").await;
    for (player, label) in [(a, "4"), (b, "5"), (a, "4"), (b, "5"), (a, "4"), (b, "5"), (a, "4")] {
        board = sim.click(player, &board, label).await;
    }
    assert!(
        board.content().contains("wins!") && board.content().contains("limit"),
        "{}",
        board.content()
    );
    assert_eq!(shells(&sim, a).await, 0);
    assert_eq!(record_of(&sim, a, "connect4").await, (1, 1), "the win still counts");
}

#[tokio::test]
async fn connect_four_wagers_are_held_paid_and_refunded() {
    let Some(mut sim) = Sim::new("connect_four_wagers_are_held_paid_and_refunded").await else {
        return;
    };
    let (a, b, broke, rich) = (unique(), unique(), unique(), unique());
    fund(&sim, a, 500).await;
    fund(&sim, b, 500).await;
    fund(&sim, rich, 500).await;
    fund(&sim, broke, 30).await;

    // A challenge holds nothing; a player who cannot cover it cannot accept, and nothing is taken.
    let challenge = duel(&mut sim, rich, broke, "connect4", vec![int_option("bet", 100)]).await;
    assert!(
        challenge.content().contains("Wager: **100** shells each"),
        "{}",
        challenge.content()
    );
    assert_eq!(shells(&sim, rich).await, 500);
    let refused = sim.click(broke, &challenge, "Accept").await;
    assert!(
        refused.ephemeral && refused.content().contains("only have **30**"),
        "{}",
        refused.content()
    );
    assert_eq!((shells(&sim, rich).await, shells(&sim, broke).await), (500, 30));
    let declined = sim.click(broke, &challenge, "Decline").await;
    assert!(
        declined.content().contains("declined") && declined.components.is_empty(),
        "{}",
        declined.content()
    );
    let after = sim.click(broke, &challenge, "Accept").await;
    assert!(
        after.ephemeral && after.content().contains("over"),
        "{}",
        after.content()
    );

    // Nobody can wager what they do not have.
    let too_much = duel(&mut sim, broke, a, "connect4", vec![int_option("bet", 100)]).await;
    assert!(
        too_much.ephemeral && too_much.content().contains("only have **30**"),
        "{}",
        too_much.content()
    );

    // Accepted: both 100 are held. A wins and takes the pot of 200, and no system reward on top.
    let challenge = duel(&mut sim, a, b, "connect4", vec![int_option("bet", 100)]).await;
    let mut board = sim.click(b, &challenge, "Accept").await;
    assert_eq!(
        (shells(&sim, a).await, shells(&sim, b).await),
        (400, 400),
        "both bets are held"
    );
    assert!(board.description().contains("Pot: **200**"), "{}", board.description());
    for (player, label) in [(a, "1"), (b, "2"), (a, "1"), (b, "2"), (a, "1"), (b, "2"), (a, "1")] {
        board = sim.click(player, &board, label).await;
    }
    assert!(
        board.content().contains("take the pot of **200**"),
        "{}",
        board.content()
    );
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (600, 400));
}

#[tokio::test]
async fn connect_four_timeouts_forfeit_the_pot_even_after_a_restart() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("connect_four_timeouts_forfeit_the_pot_even_after_a_restart").await else {
        return;
    };
    forget_late_games(&sim).await;
    let (a, b, c, d, e) = (unique(), unique(), unique(), unique(), unique());
    for user in [a, b, c, d, e] {
        fund(&sim, user, 300).await;
    }

    // The player to move does nothing: the janitor ends the game, the other takes the pot, and the message says so.
    let challenge = duel(&mut sim, a, b, "connect4", vec![int_option("bet", 50)]).await;
    let board = sim.click(b, &challenge, "Accept").await;
    let session = session_of(&board);
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (250, 250));
    sweep_games(&sim.app).await.expect("sweep");
    assert_eq!(shells(&sim, a).await, 250, "a game with time left is not touched");
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    assert_eq!(
        (shells(&sim, a).await, shells(&sim, b).await),
        (250, 350),
        "the player who ran out of time loses the wager to the other"
    );
    let message = message_of(&sim, session).await;
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the message says the game timed out");
    assert!(edit.content().contains("ran out of time"), "{}", edit.content());
    assert!(
        edit.content().contains(&format!("<@{a}> ran out of time and forfeits")),
        "A was to move: {}",
        edit.content()
    );
    assert!(
        edit.content()
            .contains(&format!("<@{b}> takes the pot of **100** shells")),
        "{}",
        edit.content()
    );
    assert_eq!(
        edit.body["components"].as_array().map(Vec::len),
        Some(0),
        "the buttons are gone"
    );
    sweep_games(&sim.app).await.expect("sweep again");
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (250, 350), "paid once");
    let late = sim.click(a, &board, "4").await;
    assert!(late.ephemeral && late.content().contains("over"), "{}", late.content());

    // A click on a late game ends it in place.
    let challenge = duel(&mut sim, c, d, "connect4", vec![int_option("bet", 50)]).await;
    let board = sim.click(d, &challenge, "Accept").await;
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (250, 250));
    expire(&sim, session_of(&board)).await;
    let ended = sim.click(c, &board, "4").await;
    assert!(
        ended.content().contains("ran out of time") && ended.components.is_empty(),
        "{}",
        ended.content()
    );
    assert_eq!(
        (shells(&sim, c).await, shells(&sim, d).await),
        (250, 350),
        "C was to move, so D takes the pot"
    );

    // A restart loses nothing: the wager lives in the database, so a fresh process settles it.
    let challenge = duel(&mut sim, e, b, "connect4", vec![int_option("bet", 200)]).await;
    let board = sim.click(b, &challenge, "Accept").await;
    assert_eq!((shells(&sim, e).await, shells(&sim, b).await), (100, 150));
    expire(&sim, session_of(&board)).await;
    drop(sim);
    let Some(restarted) = Sim::new("connect_four_after_a_restart").await else {
        return;
    };
    sweep_games(&restarted.app).await.expect("sweep");
    assert_eq!(
        (shells(&restarted, e).await, shells(&restarted, b).await),
        (100, 550),
        "E was to move and forfeits to B"
    );
}

#[tokio::test]
async fn unanswered_challenges_expire_without_touching_shells() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("unanswered_challenges_expire_without_touching_shells").await else {
        return;
    };
    forget_late_games(&sim).await;
    let (a, b) = (unique(), unique());
    fund(&sim, a, 100).await;
    fund(&sim, b, 100).await;
    let challenge = duel(&mut sim, a, b, "rps", vec![int_option("bet", 50)]).await;
    // The challenge may not know its message yet; give it one, as Discord's lookup would.
    let session = session_of(&challenge);
    let message = unique();
    sim.app
        .db
        .set_game_message(session, CHANNEL_ID, message)
        .await
        .expect("message");
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the challenge message was edited");
    assert!(edit.content().contains("didn't answer in time"), "{}", edit.content());
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (100, 100));
    let late = sim.click(b, &challenge, "Accept").await;
    assert!(late.ephemeral && late.content().contains("over"), "{}", late.content());
}

#[tokio::test]
async fn rock_paper_scissors_is_picked_in_secret_and_revealed_per_round() {
    let Some(mut sim) = Sim::new("rock_paper_scissors_is_picked_in_secret_and_revealed_per_round").await else {
        return;
    };
    let (a, b, bystander) = (unique(), unique(), unique());

    let challenge = duel(&mut sim, a, b, "rps", vec![int_option("rounds", 3)]).await;
    assert!(challenge.content().contains("best of 3"), "{}", challenge.content());
    let board = sim.click(b, &challenge, "Accept").await;
    assert!(board.files.is_empty(), "no picture, just the scores");
    assert!(
        board.content().contains("Round 1") && board.title().contains("best of 3"),
        "{}",
        board.content()
    );
    assert!(board.content().contains("⏳"), "{}", board.content());

    let refused = sim.click(bystander, &board, "Choose your move").await;
    assert!(
        refused.ephemeral && refused.content().contains("not in this match"),
        "{}",
        refused.content()
    );

    // Each player gets a private picker.
    let a_picker = sim.click(a, &board, "Choose your move").await;
    let b_picker = sim.click(b, &board, "Choose your move").await;
    assert!(a_picker.ephemeral && b_picker.ephemeral);
    assert_eq!(a_picker.controls().count(), 3, "rock, paper, scissors");

    // A locks in rock: only A sees it, the public message just shows that A is ready.
    let locked = sim.click(a, &a_picker, "🪨 Rock").await;
    assert!(
        locked.content().contains("Locked in 🪨 Rock") && locked.components.is_empty(),
        "{}",
        locked.content()
    );
    let posts = sim.posts();
    let shown = posts.last().expect("the public message was refreshed");
    assert!(
        shown.content().contains("✅") && shown.content().contains("⏳"),
        "{}",
        shown.content()
    );
    assert!(!shown.content().contains("Rock") && !shown.embed()["description"].as_str().unwrap_or("").contains("🪨"));
    let again = sim.click(a, &board, "Choose your move").await;
    assert!(
        again.ephemeral && again.content().contains("You locked in"),
        "{}",
        again.content()
    );
    let repeat = sim.click(a, &a_picker, "📄 Paper").await;
    assert!(
        repeat.content().contains("You locked in 🪨"),
        "no changing your mind: {}",
        repeat.content()
    );

    // B picks scissors: the round is revealed to the channel, rock breaks scissors.
    let revealed = sim.click(b, &b_picker, "✂️ Scissors").await;
    assert!(
        revealed.content().contains("You lose the round"),
        "{}",
        revealed.content()
    );
    let posts = sim.posts();
    let public = posts.last().expect("the reveal");
    assert!(public.content().contains("Round 2"), "{}", public.content());
    let description = public.embed()["description"].as_str().unwrap_or_default();
    assert!(
        description.contains("1 - 0") && description.contains("1. 🪨 vs ✂️"),
        "{description}"
    );
    let stale = sim.click(b, &b_picker, "🪨 Rock").await;
    assert!(
        stale.content().contains("already over"),
        "buttons of round 1 are dead: {}",
        stale.content()
    );

    // Round 2 ties and is played again; round 3 gives A the match.
    let board2 = Reply {
        content: Some(public.content().to_owned()),
        components: serde_json::from_value(public.body["components"].clone()).expect("components"),
        embeds: serde_json::from_value(public.body["embeds"].clone()).expect("embeds"),
        ..board.clone()
    };
    let a_picker = sim.click(a, &board2, "Choose your move").await;
    let b_picker = sim.click(b, &board2, "Choose your move").await;
    sim.click(a, &a_picker, "📄 Paper").await;
    let tie = sim.click(b, &b_picker, "📄 Paper").await;
    assert!(tie.content().contains("A tie"), "{}", tie.content());
    let posts = sim.posts();
    let public = posts.last().expect("the tie");
    assert!(
        public.content().contains("Round 2"),
        "a tie replays the round: {}",
        public.content()
    );
    assert!(
        public.embed()["description"]
            .as_str()
            .unwrap_or_default()
            .contains("played again")
    );
    let board3 = Reply {
        content: Some(public.content().to_owned()),
        components: serde_json::from_value(public.body["components"].clone()).expect("components"),
        embeds: serde_json::from_value(public.body["embeds"].clone()).expect("embeds"),
        ..board.clone()
    };
    let a_picker = sim.click(a, &board3, "Choose your move").await;
    let b_picker = sim.click(b, &board3, "Choose your move").await;
    sim.click(b, &b_picker, "📄 Paper").await;
    let won = sim.click(a, &a_picker, "✂️ Scissors").await;
    assert!(won.content().contains("You win the round"), "{}", won.content());
    let posts = sim.posts();
    let end = posts.last().expect("the final message");
    assert!(
        end.content().contains(&format!("<@{a}> wins the match 2 - 0")) && end.content().contains("+**15** shells"),
        "{}",
        end.content()
    );
    assert_eq!(end.body["components"].as_array().map(Vec::len), Some(0));
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (15, 0));
    assert_eq!(record_of(&sim, a, "rps").await, (1, 1));
    assert_eq!(record_of(&sim, b, "rps").await, (1, 0));
}

#[tokio::test]
async fn rock_paper_scissors_wagers_go_to_the_match_winner() {
    let Some(mut sim) = Sim::new("rock_paper_scissors_wagers_go_to_the_match_winner").await else {
        return;
    };
    let (a, b) = (unique(), unique());
    fund(&sim, a, 300).await;
    fund(&sim, b, 300).await;
    let challenge = duel(
        &mut sim,
        a,
        b,
        "rps",
        vec![int_option("bet", 150), int_option("rounds", 1)],
    )
    .await;
    let board = sim.click(b, &challenge, "Accept").await;
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (150, 150));
    let a_picker = sim.click(a, &board, "Choose your move").await;
    let b_picker = sim.click(b, &board, "Choose your move").await;
    sim.click(a, &a_picker, "🪨 Rock").await;
    sim.click(b, &b_picker, "📄 Paper").await;
    let posts = sim.posts();
    let end = posts.last().expect("the final message");
    assert!(
        end.content().contains(&format!("<@{b}> wins the match")) && end.content().contains("pot of **300**"),
        "{}",
        end.content()
    );
    assert_eq!(
        (shells(&sim, a).await, shells(&sim, b).await),
        (150, 450),
        "the winner takes both wagers"
    );
}

#[tokio::test]
async fn hangman_is_won_and_lost_one_letter_at_a_time() {
    let Some(mut sim) = Sim::new("hangman_is_won_and_lost_one_letter_at_a_time").await else {
        return;
    };
    let (player, other, loser) = (unique(), unique(), unique());

    let start = sim.command(player, "play", json!([sub("hangman", vec![])])).await;
    assert_eq!((start.files.len(), start.files[0].0.as_str()), (1, "hangman.png"));
    assert!(start.content().contains("6"), "{}", start.content());
    assert!(
        !start.options("hm:").is_empty() && start.rows() == 2,
        "two letter menus"
    );
    let id = session_of(&start);
    let state = sim.app.db.game(id).await.expect("session").expect("a game").state;
    let game = Hangman::decode(&state).expect("a hangman state");
    let word = game.word().to_owned();

    let again = sim.command(player, "play", json!([sub("hangman", vec![])])).await;
    assert!(
        again.ephemeral && again.content().contains("One sandcastle at a time"),
        "{}",
        again.content()
    );

    // Somebody else cannot play your sandcastle.
    let letter = word.chars().find(|c| *c != ' ').expect("a letter");
    let half = |c: char| if c <= 'm' { "a" } else { "n" };
    let intruder = sim
        .component(
            other,
            &format!("hm:{id}:{}", half(letter)),
            &[&letter.to_string()],
            &start,
        )
        .await;
    assert!(
        intruder.ephemeral && intruder.content().contains("sandcastle"),
        "{}",
        intruder.content()
    );

    // A wrong guess first, then every letter of the word.
    let wrong = ('a'..='z')
        .find(|c| !word.contains(*c))
        .expect("a letter not in the word");
    let mut board = sim
        .pick(player, &start, &format!("hm:{id}:{}", half(wrong)), &wrong.to_string())
        .await;
    assert!(board.content().contains("5"), "one miss used: {}", board.content());
    assert!(
        !board
            .options(&format!("hm:{id}:{}", half(wrong)))
            .iter()
            .any(|o| o.value == wrong.to_string()),
        "a guessed letter leaves its menu"
    );
    let mut letters: Vec<char> = word.chars().filter(|c| *c != ' ').collect();
    letters.sort_unstable();
    letters.dedup();
    for letter in letters {
        board = sim
            .pick(
                player,
                &board,
                &format!("hm:{id}:{}", half(letter)),
                &letter.to_string(),
            )
            .await;
    }
    assert!(
        board.content().contains("saved the sandcastle") && board.content().contains("+**15** shells"),
        "{}",
        board.content()
    );
    assert!(board.components.is_empty());
    assert_eq!(shells(&sim, player).await, 15);
    assert_eq!(record_of(&sim, player, "hangman").await, (1, 1));

    // Six misses lose the castle; no shells, but the game counts.
    let start = sim.command(loser, "play", json!([sub("hangman", vec![])])).await;
    let id = session_of(&start);
    let word = Hangman::decode(&sim.app.db.game(id).await.unwrap().unwrap().state)
        .unwrap()
        .word()
        .to_owned();
    let mut board = start;
    for letter in ('a'..='z').filter(|c| !word.contains(*c)).take(6) {
        board = sim
            .pick(loser, &board, &format!("hm:{id}:{}", half(letter)), &letter.to_string())
            .await;
    }
    assert!(
        board.content().contains("tide won") && board.content().contains(&word),
        "{}",
        board.content()
    );
    assert!(board.components.is_empty() && board.files.len() == 1);
    assert_eq!(shells(&sim, loser).await, 0);
    assert_eq!(record_of(&sim, loser, "hangman").await, (1, 0));
}

#[tokio::test]
async fn an_abandoned_hangman_game_is_closed_by_the_janitor() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("an_abandoned_hangman_game_is_closed_by_the_janitor").await else {
        return;
    };
    forget_late_games(&sim).await;
    let player = unique();
    let start = sim.command(player, "play", json!([sub("hangman", vec![])])).await;
    let id = session_of(&start);
    let message = unique();
    sim.app
        .db
        .set_game_message(id, CHANNEL_ID, message)
        .await
        .expect("message");
    expire(&sim, id).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the game message was edited");
    assert!(edit.content().contains("tide came in"), "{}", edit.content());
    let late = sim.pick(player, &start, &format!("hm:{id}:a"), "a").await;
    assert!(late.ephemeral && late.content().contains("over"), "{}", late.content());
}

#[tokio::test]
async fn the_daily_wordle_is_typed_into_a_form_and_shared_without_spoilers() {
    let Some(mut sim) = Sim::new("the_daily_wordle_is_typed_into_a_form_and_shared_without_spoilers").await else {
        return;
    };
    let player = unique();
    let words = &PlayContent::get().words;
    let day = today();
    let answer = wordle::daily_word(&words.answers, day).to_owned();

    let board = sim.command(player, "play", json!([sub("wordle", vec![])])).await;
    assert!(board.ephemeral, "the board is private, so nobody sees your letters");
    assert_eq!((board.files.len(), board.files[0].0.as_str()), (1, "wordle.png"));
    assert!(board.title().starts_with("Daily wordle #"), "{}", board.title());

    let form = sim.click(player, &board, "Guess").await;
    assert_eq!(
        form.modal.as_ref().map(|m| m.0.clone()),
        Some(format!("wo:m:daily:{day}")),
        "{form}"
    );

    let short = sim.submit(player, &form, "abc", &board).await;
    assert!(
        short.ephemeral && short.content().contains("five letters"),
        "{}",
        short.content()
    );
    let nonsense = sim.submit(player, &form, "zzzzz", &board).await;
    assert!(
        nonsense.ephemeral && nonsense.content().contains("isn't in my word list"),
        "{}",
        nonsense.content()
    );

    let miss = ["crane", "slate", "pious"]
        .into_iter()
        .find(|w| *w != answer && words.is_allowed(w))
        .expect("an allowed word that is not the answer");
    let board = sim.submit(player, &form, &miss.to_uppercase(), &board).await;
    assert!(
        board.description().contains("Guess the five-letter word"),
        "{}",
        board.description()
    );
    assert_eq!(board.files.len(), 1);
    let db_board = sim
        .app
        .db
        .wordle_game(player, "daily", day as i32)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (db_board.guesses.clone(), db_board.status.as_str()),
        (vec![miss.to_owned()], "playing")
    );

    let won = sim.submit(player, &form, &answer, &board).await;
    assert!(
        won.description().contains("You got it in **2**") && won.description().contains("+**40** shells"),
        "{}",
        won.description()
    );
    assert_eq!(shells(&sim, player).await, 40);
    assert_eq!(record_of(&sim, player, "wordle").await, (1, 1));
    won.button("Share result");

    // The form still opens (nothing is read before it does); a board that is over says so when the guess
    // is sent.
    let late_form = sim.click(player, &board, "Guess").await;
    assert!(late_form.modal.is_some(), "{late_form}");
    let late = sim.submit(player, &late_form, miss, &won).await;
    assert!(
        late.ephemeral && late.content().contains("finished"),
        "{}",
        late.content()
    );

    // Sharing posts the result for the channel: no letters, and only once.
    let shared = sim.click(player, &won, "Share result").await;
    assert!(
        !shared.ephemeral && shared.content().contains("Pinchora Wordle #"),
        "{}",
        shared.content()
    );
    assert!(
        shared.content().contains("2/6") && shared.content().contains("🟩🟩🟩🟩🟩"),
        "{}",
        shared.content()
    );
    assert!(
        !shared.content().contains(&answer) && !shared.content().contains(miss),
        "no spoilers"
    );
    let twice = sim.click(player, &won, "Share result").await;
    assert!(
        twice.ephemeral && twice.content().contains("already shared"),
        "{}",
        twice.content()
    );

    // The day's result stays: asking again shows the finished board.
    let again = sim.command(player, "play", json!([sub("wordle", vec![])])).await;
    assert!(
        again.description().contains("You got it in **2**"),
        "{}",
        again.description()
    );
    assert_eq!(shells(&sim, player).await, 40, "the daily word pays once");
}

#[tokio::test]
async fn practice_wordle_has_unlimited_words() {
    let Some(mut sim) = Sim::new("practice_wordle_has_unlimited_words").await else {
        return;
    };
    let player = unique();
    let words = &PlayContent::get().words;
    let board = sim
        .command(
            player,
            "play",
            json!([sub("wordle", vec![text_option("mode", "practice")])]),
        )
        .await;
    assert_eq!(board.title(), "Practice wordle");
    let first = sim
        .app
        .db
        .wordle_game(player, "practice", 0)
        .await
        .unwrap()
        .expect("a practice board");
    let resumed = sim
        .command(
            player,
            "play",
            json!([sub("wordle", vec![text_option("mode", "practice")])]),
        )
        .await;
    assert_eq!(resumed.title(), "Practice wordle");
    assert_eq!(
        sim.app
            .db
            .wordle_game(player, "practice", 0)
            .await
            .unwrap()
            .unwrap()
            .word,
        first.word,
        "an unfinished practice word is resumed, not rerolled"
    );

    // Six wrong guesses lose it and show the word.
    let form = sim.click(player, &board, "Guess").await;
    let mut board = board;
    let misses: Vec<&str> = ["crane", "slate", "pious", "tough", "vivid", "quirk", "blown"]
        .into_iter()
        .filter(|w| *w != first.word && words.is_allowed(w))
        .take(6)
        .collect();
    assert_eq!(misses.len(), 6);
    for miss in &misses {
        board = sim.submit(player, &form, miss, &board).await;
    }
    assert!(board.description().contains("Out of tries"), "{}", board.description());
    assert!(
        board.description().contains(&first.word),
        "the word is shown once it is lost"
    );
    assert_eq!(record_of(&sim, player, "wordle").await, (1, 0));
    assert_eq!(shells(&sim, player).await, 0);

    // A new word is one click away and winning pays a little.
    let fresh = sim.click(player, &board, "New word").await;
    assert_eq!(fresh.title(), "Practice wordle");
    let word = sim
        .app
        .db
        .wordle_game(player, "practice", 0)
        .await
        .unwrap()
        .unwrap()
        .word;
    let form = sim.click(player, &fresh, "Guess").await;
    let won = sim.submit(player, &form, &word, &fresh).await;
    assert!(
        won.description().contains("You got it in **1**") && won.description().contains("+**10** shells"),
        "{}",
        won.description()
    );
    assert_eq!(record_of(&sim, player, "wordle").await, (2, 1));
}

#[tokio::test]
async fn a_trivia_round_collects_answers_then_reveals_and_pays() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("a_trivia_round_collects_answers_then_reveals_and_pays").await else {
        return;
    };
    forget_late_games(&sim).await;
    // Rounds are limited per channel, so this test has a channel to itself.
    sim.channel = unique();
    let (starter, right, wrong, late) = (unique(), unique(), unique(), unique());
    let round = sim
        .command(
            starter,
            "play",
            json!([sub("trivia", vec![text_option("category", "ocean")])]),
        )
        .await;
    assert!(!round.ephemeral && round.title().contains("Ocean"), "{}", round.title());
    assert_eq!(round.controls().count(), 4, "four answers");
    assert!(
        round.content().contains("started a trivia round"),
        "{}",
        round.content()
    );
    let id = session_of(&round);

    // The round is in the database: its question and the order its options are shown in.
    let state = sim.app.db.game(id).await.unwrap().unwrap().state;
    let (question_id, order) = state.split_once(':').expect("question:order");
    let question = PlayContent::get().question(question_id).expect("a real question");
    let order = trivia::decode_order(order).expect("an order");
    let correct = question.correct_slot(order).expect("the answer is shown");
    let incorrect = (correct + 1) % 4;
    for slot in 0..4 {
        assert!(
            round.description().contains(question.shown(order)[slot]),
            "option {slot} is on the message"
        );
    }
    let again = sim.command(unique(), "play", json!([sub("trivia", vec![])])).await;
    assert!(
        again.ephemeral && again.content().contains("just ran"),
        "{}",
        again.content()
    );

    let click = |slot: usize| format!("tv:{id}:{slot}");
    let locked = sim.component(right, &click(correct), &[], &round).await;
    assert!(
        locked.ephemeral && locked.content().contains("Locked in"),
        "{}",
        locked.content()
    );
    assert!(!locked.content().contains("right"), "the answer is not given away");
    let twice = sim.component(right, &click(incorrect), &[], &round).await;
    assert!(
        twice.ephemeral && twice.content().contains("already locked in"),
        "{}",
        twice.content()
    );
    sim.component(wrong, &click(incorrect), &[], &round).await;

    // Time runs out: the janitor reveals the answer and pays the right answer only.
    expire(&sim, id).await;
    let after = sim.component(late, &click(correct), &[], &round).await;
    assert!(
        after.ephemeral && after.content().contains("Time's up"),
        "{}",
        after.content()
    );
    sweep_games(&sim.app).await.expect("sweep");
    let message = message_of(&sim, id).await;
    let posts = sim.posts();
    let reveal = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the round was revealed");
    assert!(
        reveal.content().contains("**2** answered") && reveal.content().contains(&question.answer),
        "{}",
        reveal.content()
    );
    let embed = reveal.embed();
    assert!(
        embed["fields"][0]["value"]
            .as_str()
            .unwrap_or_default()
            .contains(&format!("<@{right}> +{}", question.difficulty.reward()))
    );
    assert!(
        !embed["fields"][0]["value"]
            .as_str()
            .unwrap_or_default()
            .contains(&wrong.to_string())
    );
    assert!(
        embed["description"]
            .as_str()
            .unwrap_or_default()
            .contains(&format!("✅ **{}.**", ['A', 'B', 'C', 'D'][correct]))
    );
    assert_eq!(
        reveal.body["components"][0]["components"][correct]["style"], 3,
        "the right button is green"
    );
    assert_eq!(shells(&sim, right).await, question.difficulty.reward());
    assert_eq!(shells(&sim, wrong).await, 0);
    assert_eq!(record_of(&sim, right, "trivia").await, (1, 1));
    assert_eq!(record_of(&sim, wrong, "trivia").await, (1, 0));
    assert_eq!(
        record_of(&sim, late, "trivia").await,
        (0, 0),
        "a late answer does not count"
    );
    sweep_games(&sim.app).await.expect("sweep again");
    assert_eq!(shells(&sim, right).await, question.difficulty.reward(), "paid once");
}

#[tokio::test]
async fn level_roles_follow_level_ups() {
    let Some(mut sim) = Sim::new("level_roles_follow_level_ups").await else {
        return;
    };
    let (admin, early, late, middle) = (unique(), unique(), unique(), unique());
    // The owner may set up any role the bot can hand out.
    sim.own_server(admin);
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
    // The owner may set up any role the bot can hand out.
    sim.own_server(admin);
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

/// A pixel of a card.
fn pixel(card: &image::RgbaImage, x: u32, y: u32) -> [u8; 4] {
    card.get_pixel(x, y).0
}

/// The left border of the profile card's avatar frame, which is drawn in the accent colour.
fn profile_accent(reply: &Reply) -> [u8; 4] {
    pixel(&card_pixels(reply, (960, 540)), 44, 150)
}

/// The same border on the rank card.
fn rank_accent(reply: &Reply) -> [u8; 4] {
    pixel(&card_pixels(reply, (934, 282)), 37, 140)
}

#[tokio::test]
async fn the_character_editor_steps_dyes_buys_keeps_outfits_and_saves() {
    let Some(mut sim) = Sim::new("the_character_editor_steps_dyes_buys_keeps_outfits_and_saves").await else {
        return;
    };
    let user = unique();
    let db = sim.app.db.clone();
    let content = Content::get();
    let default_value = |reply: &Reply, menu: &str| {
        reply
            .options(menu)
            .iter()
            .find(|o| o.default)
            .map(|o| o.value.clone())
            .unwrap_or_default()
    };

    // One private message: the card, a category menu, a jump menu, the step row and Save/Cancel.
    let editor = sim.command(user, "character", json!([])).await;
    assert!(editor.ephemeral, "the editor is private");
    assert_eq!(editor.title(), "Your character");
    card_image(&editor, (960, 540));
    assert_eq!(editor.rows(), 4);
    assert_eq!(editor.options("ca:tab").len(), 10);
    assert_eq!(editor.options("ca:pick").len(), content.looks.len());
    assert_eq!(default_value(&editor, "ca:pick"), content.looks[0].id);
    assert!(editor.button("Undo").disabled, "nothing to undo yet");

    // ◀ ▶ step the look one click at a time; Undo goes back a step.
    let next = sim.click(user, &editor, "▶").await;
    assert_eq!(default_value(&next, "ca:pick"), content.looks[1].id);
    let first = sim.click(user, &next, "◀").await;
    assert_eq!(default_value(&first, "ca:pick"), content.looks[0].id);
    let wrapped = sim.click(user, &first, "◀").await;
    assert_eq!(default_value(&wrapped, "ca:pick"), content.looks.last().unwrap().id);
    let undone = sim.click(user, &wrapped, "Undo").await;
    assert_eq!(default_value(&undone, "ca:pick"), content.looks[0].id);
    assert!(undone.description().contains("Undone"), "{}", undone.description());

    // A colour tab: twenty swatches, ◀ ▶ and a typed hex code, refused when it is not one and
    // nudged when it is too dark to shade.
    let hair = sim.pick(user, &undone, "ca:tab", "hair").await;
    assert_eq!(hair.options("ca:pick").len(), 20);
    let hazel = sim.click(user, &hair, "▶").await;
    assert_eq!(default_value(&hazel, "ca:pick"), "2");
    let form = sim.click(user, &hazel, "Hex…").await;
    let (modal, title) = form.modal.clone().expect("a form");
    assert!(modal.starts_with("ca:hexm:"), "{modal}");
    assert_eq!(title, "Hair colour");
    assert_eq!(form.modal_inputs, [("hex".to_owned(), Some("#a0743c".to_owned()))]);
    let green = sim.submit_fields(user, &form, &[("hex", "#2fb87a")]).await;
    assert!(green.description().contains("#2fb87a"), "{}", green.description());
    assert_eq!(default_value(&green, "ca:pick"), "custom");
    let junk = sim.submit_fields(user, &form, &[("hex", "banana")]).await;
    assert!(
        junk.description().contains("not a colour code"),
        "{}",
        junk.description()
    );
    let dark = sim.submit_fields(user, &form, &[("hex", "#050505")]).await;
    assert!(dark.description().contains("nudged"), "{}", dark.description());
    assert_eq!(db.character(user).await.unwrap(), None, "nothing is saved before Save");

    // Accessories: a slot row, free things go straight on, a dye cycles its colour.
    let gear = sim.pick(user, &dark, "ca:tab", "gear").await;
    assert_eq!(gear.rows(), 5, "the slot buttons get a row");
    assert!(gear.button("Dye ▶").disabled, "nothing worn to dye");
    let hat = sim.pick(user, &gear, "ca:pick", "straw_hat").await;
    let dyed = sim.click(user, &hat, "Dye ▶").await;
    assert!(dyed.description().contains("Straw Sun Hat"), "{}", dyed.description());
    // A crown needs level 30: it shows locked and has no Buy button.
    let crown = dyed
        .options("ca:pick")
        .iter()
        .find(|o| o.value == "crown")
        .expect("the crown is listed");
    assert!(crown.emoji.is_some() && crown.description.as_deref() == Some("Reach level 30"));
    // A top hat can be tried on, but Save waits until it is bought.
    let top = sim.pick(user, &dyed, "ca:pick", "top_hat").await;
    let buy = "Buy Top Hat · 900 shells";
    assert!(top.has_button(buy));
    let refused = sim.click(user, &top, "Save").await;
    assert!(refused.description().contains("Not saved"), "{}", refused.description());
    assert_eq!(db.character(user).await.unwrap(), None);
    let poor = sim.click(user, &refused, buy).await;
    assert!(poor.description().contains("you have 0"), "{}", poor.description());
    fund(&sim, user, 1_000).await;
    let bought = sim.click(user, &poor, buy).await;
    assert!(
        bought.description().contains("Bought **Top Hat**"),
        "{}",
        bought.description()
    );
    assert_eq!(shells(&sim, user).await, 100);
    assert!(!bought.has_button(buy), "owned now");
    let hand = sim.click(user, &bought, "Hand").await;
    let balloon = sim.pick(user, &hand, "ca:pick", "balloon").await;
    let red = sim.click(user, &balloon, "Dye ▶").await;

    // Save keeps all of it.
    let saved = sim.click(user, &red, "Save").await;
    assert!(saved.description().contains("Saved!"), "{}", saved.description());
    assert!(saved.button("Save").disabled, "nothing new to save");
    let code = db.character(user).await.unwrap().expect("saved");
    let character = Character::decode(content, &code).expect("a valid code");
    assert_eq!(character.look, content.looks[0].id);
    assert_eq!(character.worn(Slot::Head).map(|w| w.id.as_str()), Some("top_hat"));
    let worn_balloon = character.worn(Slot::Hand).expect("a balloon");
    assert_eq!(
        (worn_balloon.id.as_str(), worn_balloon.tint),
        ("balloon", Some(Paint::Swatch(0)))
    );
    assert!(matches!(character.colors.hair, Paint::Custom(v) if v != 0x050505));

    // Outfits: keep the look in slot 2, name it, try a preset, then put the outfit back on.
    let outfits = sim.pick(user, &saved, "ca:tab", "outfits").await;
    assert!(outfits.button("Rename…").disabled, "nothing to name yet");
    let kept = sim.click(user, &outfits, "Keep in 2").await;
    assert_eq!(db.outfits(user).await.unwrap()[0].code, code);
    let names = sim.click(user, &kept, "Rename…").await;
    assert!(names.modal.as_ref().is_some_and(|m| m.0.starts_with("ca:namem:")));
    assert_eq!(names.modal_inputs, [("slot2".to_owned(), Some("Outfit 2".to_owned()))]);
    let named = sim.submit_fields(user, &names, &[("slot2", "Beach day")]).await;
    assert_eq!(db.outfits(user).await.unwrap()[0].name, "Beach day");
    assert!(
        named
            .options("ca:pick")
            .iter()
            .any(|o| o.label == "2 · Beach day" && o.default)
    );
    let presets = sim.pick(user, &named, "ca:tab", "presets").await;
    assert_eq!(presets.options("ca:pick").len(), 10);
    let surfer = sim.pick(user, &presets, "ca:pick", "beach_surfer").await;
    assert!(
        surfer.description().contains("Beach Surfer"),
        "{}",
        surfer.description()
    );
    let witch = sim.click(user, &surfer, "▶").await;
    assert_eq!(default_value(&witch, "ca:pick"), "ship_captain");
    let back = sim.pick(user, &witch, "ca:tab", "outfits").await;
    let worn = sim.pick(user, &back, "ca:pick", "2").await;
    assert!(worn.description().contains("Beach day"), "{}", worn.description());
    assert!(worn.button("Save").disabled, "the outfit is what is saved");
    let random = sim.click(user, &worn, "Randomise").await;
    assert!(random.description().contains("Randomised"), "{}", random.description());

    // Cancel closes the editor without saving.
    let closed = sim.click(user, &random, "Cancel").await;
    assert_eq!(closed.rows(), 0);
    assert!(closed.description().contains("Closed without saving"));
    assert_eq!(db.character(user).await.unwrap().as_deref(), Some(code.as_str()));
    let stale = sim.click(user, &random, "▶").await;
    assert!(stale.content().contains("expired"), "{}", stale.content());

    // A newer editor makes an older message stale; the saved character opens it.
    let first = sim.command(user, "character", json!([])).await;
    let second = sim.command(user, "character", json!([])).await;
    assert!(second.button("Save").disabled, "it opens on what is saved");
    let stale = sim.click(user, &first, "▶").await;
    assert!(stale.content().contains("expired"), "{}", stale.content());
    let old = sim.component(user, "ch:look", &["tidewalker"], &second).await;
    assert!(old.content().contains("expired"), "{}", old.content());

    // Cancel loses nothing when nothing is unsaved, untouched or right after a Save, and says so.
    let calm = sim.click(user, &second, "Cancel").await;
    assert!(calm.description().starts_with("Closed:"), "{}", calm.description());
    assert!(!calm.description().contains("without saving"), "{}", calm.description());
    let again = sim.command(user, "character", json!([])).await;
    let stepped = sim.click(user, &again, "▶").await;
    let kept_new = sim.click(user, &stepped, "Save").await;
    assert!(kept_new.description().contains("Saved!"), "{}", kept_new.description());
    let stepped_back = sim.click(user, &kept_new, "◀").await;
    let kept_old = sim.click(user, &stepped_back, "Save").await;
    assert!(kept_old.description().contains("Saved!"), "{}", kept_old.description());
    assert_eq!(db.character(user).await.unwrap().as_deref(), Some(code.as_str()));
    let after_save = sim.click(user, &kept_old, "Cancel").await;
    assert!(
        after_save.description().starts_with("Closed:"),
        "{}",
        after_save.description()
    );

    // The saved character, hat and balloon, is on the profile and in the house.
    sim.command(user, "profile", json!([])).await;
    sim.command(user, "house", json!([{ "name": "view", "type": 1 }])).await;
}

#[tokio::test]
async fn the_style_editor_sets_accent_opacity_and_frame_on_every_card() {
    let Some(mut sim) = Sim::new("the_style_editor_sets_accent_opacity_and_frame_on_every_card").await else {
        return;
    };
    let user = unique();
    let db = sim.app.db.clone();
    let coral = [255, 107, 74, 255];
    assert_eq!(profile_accent(&sim.command(user, "profile", json!([])).await), coral);

    // The editor opens on the real profile card with nothing to reset: 12 colours and Auto, 6 frames.
    let panel = sim
        .command(user, "customize", json!([{ "name": "style", "type": 1 }]))
        .await;
    assert_eq!(panel.title(), "Card style");
    assert!(panel.ephemeral, "the editor is private");
    card_image(&panel, (960, 540));
    assert_eq!(panel.rows(), 3, "two menus and a row of buttons");
    assert_eq!(panel.options("cs:a").len(), 13);
    assert!(panel.options("cs:a").iter().any(|o| o.value == "auto" && o.default));
    assert!(
        panel.description().contains("**Accent** Auto (coral)"),
        "{}",
        panel.description()
    );
    assert!(panel.button("Reset style").disabled, "nothing to reset yet");

    // Locked frames show a lock and the level, in the menu and under the card.
    let frames = panel.options("cs:f");
    assert_eq!(
        frames.iter().map(|o| o.value.as_str()).collect::<Vec<_>>(),
        ["plain", "rope", "coral", "pearl", "gold", "neon"]
    );
    let frame = |id: &str| frames.iter().find(|o| o.value == id).expect("a frame");
    assert_eq!(frame("plain").description.as_deref(), Some("On your cards now"));
    assert_eq!(frame("rope").description.as_deref(), Some("Free"));
    assert_eq!(
        frame("pearl").description.as_deref(),
        Some("Locked · reach level 20 (you are level 0)")
    );
    assert!(frame("pearl").emoji.is_some() && frame("rope").emoji.is_none());
    assert!(
        panel.description().contains("🔒 Coral · level 10"),
        "{}",
        panel.description()
    );

    // A preset recolours the avatar frame on the profile and rank cards.
    let blue = sim.pick(user, &panel, "cs:a", "3a9bdc").await;
    assert!(
        blue.description().contains("Your accent is `#3a9bdc`"),
        "{}",
        blue.description()
    );
    assert_eq!(db.user(user).await.unwrap().accent.as_deref(), Some("#3a9bdc"));
    assert!(blue.options("cs:a").iter().any(|o| o.value == "3a9bdc" && o.default));
    assert!(!blue.button("Reset style").disabled);
    assert_eq!(
        pixel(&card_pixels(&blue, (960, 540)), 44, 150),
        [58, 155, 220, 255],
        "the editor previews it"
    );
    let ocean = [58, 155, 220, 255];
    assert_eq!(profile_accent(&sim.command(user, "profile", json!([])).await), ocean);
    assert_eq!(rank_accent(&sim.command(user, "rank", json!([])).await), ocean);

    // The hex button opens a form that holds the current colour; unreadable and malformed codes are
    // refused with a reason, and a short code is widened.
    let form = sim.click(user, &blue, "Hex code…").await;
    assert_eq!(
        form.modal.as_ref().map(|m| (m.0.as_str(), m.1.as_str())),
        Some(("cs:hx", "Accent colour"))
    );
    assert_eq!(
        form.modal_inputs,
        [("hex".to_owned(), Some("#3a9bdc".to_owned()))],
        "the box starts with the colour in use"
    );
    let dark = sim.submit_fields(user, &form, &[("hex", "#0b1029")]).await;
    assert!(dark.description().contains("too dark"), "{}", dark.description());
    assert_eq!(db.user(user).await.unwrap().accent.as_deref(), Some("#3a9bdc"), "kept");
    let junk = sim.submit_fields(user, &form, &[("hex", "banana")]).await;
    assert!(
        junk.description().contains("not a colour code"),
        "{}",
        junk.description()
    );
    let typed = sim.submit_fields(user, &form, &[("hex", "F84")]).await;
    assert!(
        typed.description().contains("Your accent is `#ff8844`"),
        "{}",
        typed.description()
    );
    assert_eq!(db.user(user).await.unwrap().accent.as_deref(), Some("#ff8844"));
    assert_eq!(
        profile_accent(&sim.command(user, "profile", json!([])).await),
        [255, 136, 68, 255]
    );
    // A hand-made menu value gets the same check.
    let sneaky = sim.component(user, "cs:a", &["000000"], &typed).await;
    assert!(
        sneaky.content().contains("isn't available any more"),
        "{}",
        sneaky.content()
    );
    assert_eq!(db.user(user).await.unwrap().accent.as_deref(), Some("#ff8844"));

    // Auto gives the background's (here: coral) back.
    let auto = sim.pick(user, &typed, "cs:a", "auto").await;
    assert_eq!(db.user(user).await.unwrap().accent, None);
    assert_eq!(profile_accent(&sim.command(user, "profile", json!([])).await), coral);

    // Panel opacity only shows over a picture: without one the buttons are left out and the editor
    // says where to get one. The others stay.
    assert!(
        !auto.has_button("Light") && !auto.has_button("Normal") && !auto.has_button("Solid"),
        "no picture, no opacity choice"
    );
    assert!(
        auto.description().contains("`/customize background`"),
        "{}",
        auto.description()
    );
    assert!(auto.has_button("Hex code…") && auto.has_button("Reset style"));
    db.earn_shells(user, 5_000, "test").await.unwrap();
    db.buy_background(user, "shellwash_morning", 800).await.unwrap();
    db.equip_background(user, "shellwash_morning").await.unwrap();
    let scenic = sim
        .command(user, "customize", json!([{ "name": "style", "type": 1 }]))
        .await;
    assert!(scenic.has_button("Light") && scenic.has_button("Normal") && scenic.has_button("Solid"));
    assert!(
        !scenic.description().contains("`/customize background`"),
        "{}",
        scenic.description()
    );
    // The plain cards over the picture: what the style is measured against from here on.
    let plain_profile = sim.command(user, "profile", json!([])).await;
    let plain_rank = sim.command(user, "rank", json!([])).await;
    let plain_wallet = sim.command(user, "balance", json!([])).await;

    // Opacity: Light makes the panels see-through on every card, Solid darkens the picture.
    let light = sim.click(user, &scenic, "Light").await;
    assert_eq!(db.user(user).await.unwrap().panel_opacity, 0);
    assert!(light.button("Light").style == twilight_model::channel::message::component::ButtonStyle::Primary);
    assert!(light.button("Normal").style == twilight_model::channel::message::component::ButtonStyle::Secondary);
    assert!(differ_in(
        &card_pixels(&light, (960, 540)),
        &card_pixels(&plain_profile, (960, 540)),
        150..190,
        340..360
    ));
    let solid = sim.click(user, &light, "Solid").await;
    assert_eq!(db.user(user).await.unwrap().panel_opacity, 2);
    assert!(differ_in(
        &card_pixels(&sim.click(user, &solid, "Light").await, (960, 540)),
        &card_pixels(&solid, (960, 540)),
        150..190,
        340..360
    ));
    assert!(
        differ_in(
            &card_pixels(&solid, (960, 540)),
            &card_pixels(&plain_profile, (960, 540)),
            0..960,
            0..540
        ),
        "over a picture Solid is not Normal"
    );
    let normal = sim.click(user, &solid, "Normal").await;
    assert_eq!(db.user(user).await.unwrap().panel_opacity, 1);

    // Frames: the free ones apply at once; the others wait for their level, however they are asked for.
    let rope = sim.pick(user, &normal, "cs:f", "rope").await;
    assert!(
        rope.description().contains("The **Rope** frame is on your cards"),
        "{}",
        rope.description()
    );
    assert_eq!(db.user(user).await.unwrap().frame, "rope");
    let framed = [
        (
            sim.command(user, "profile", json!([])).await,
            &plain_profile,
            (960, 540),
        ),
        (sim.command(user, "rank", json!([])).await, &plain_rank, (934, 282)),
        (sim.command(user, "balance", json!([])).await, &plain_wallet, (900, 300)),
    ];
    for (card, plain, size) in &framed {
        assert!(
            differ_in(&card_pixels(card, *size), &card_pixels(plain, *size), 0..8, 0..size.1),
            "the rope band is on the {size:?} card"
        );
    }
    let refused = sim.pick(user, &rope, "cs:f", "pearl").await;
    assert!(
        refused
            .description()
            .contains("The **Pearl** frame unlocks at level 20. You are level 0."),
        "{}",
        refused.description()
    );
    assert_eq!(
        db.user(user).await.unwrap().frame,
        "rope",
        "a locked frame is not saved"
    );
    let bogus = sim.component(user, "cs:f", &["diamond"], &refused).await;
    assert!(
        bogus.content().contains("isn't available any more"),
        "{}",
        bogus.content()
    );
    // Level 20 opens the pearl frame (23,850 XP in all); a frame the level no longer allows would
    // simply not be drawn.
    db.add_message_xp(sim.guild, user, 30_000, 0).await.unwrap();
    let reopened = sim
        .command(user, "customize", json!([{ "name": "style", "type": 1 }]))
        .await;
    let pearl = reopened
        .options("cs:f")
        .iter()
        .find(|o| o.value == "pearl")
        .expect("a frame");
    assert_eq!(pearl.description.as_deref(), Some("Unlocked at level 20"));
    assert!(pearl.emoji.is_none(), "unlocked, so no lock");
    assert!(
        reopened
            .options("cs:f")
            .iter()
            .any(|o| o.value == "gold" && o.emoji.is_some()),
        "gold is still locked"
    );
    let pearly = sim.pick(user, &reopened, "cs:f", "pearl").await;
    assert_eq!(db.user(user).await.unwrap().frame, "pearl");
    assert!(
        pearly.description().contains("**Frame** Pearl"),
        "{}",
        pearly.description()
    );

    // Reset puts everything back, and the plain card has its bare corners again.
    let reset = sim.click(user, &pearly, "Reset style").await;
    let row = db.user(user).await.unwrap();
    assert_eq!((row.accent, row.panel_opacity, row.frame.as_str()), (None, 1, "plain"));
    assert!(reset.button("Reset style").disabled);
    let profile = sim.command(user, "profile", json!([])).await;
    assert!(!differ_in(
        &card_pixels(&profile, (960, 540)),
        &card_pixels(&plain_profile, (960, 540)),
        0..8,
        0..540
    ));

    // A frame saved above the player's level is not drawn, but it is still saved: Reset is not
    // greyed out because the card looks plain, and it clears the frame.
    db.set_frame(user, "gold").await.unwrap();
    let stale = sim
        .command(user, "customize", json!([{ "name": "style", "type": 1 }]))
        .await;
    assert!(
        !differ_in(
            &card_pixels(&stale, (960, 540)),
            &card_pixels(&plain_profile, (960, 540)),
            0..8,
            0..540
        ),
        "the locked gold frame is not drawn"
    );
    assert!(
        !stale.button("Reset style").disabled,
        "there is a stored frame to reset"
    );
    let unstuck = sim.click(user, &stale, "Reset style").await;
    assert_eq!(db.user(user).await.unwrap().frame, "plain");
    assert!(unstuck.button("Reset style").disabled);
}

/// A long display name, to see how the profile card keeps it clear of everything else.
const LONG_NAME: &str = "Bartholomew Fizzlebottom Smythe!";

/// `/profile user:<target>` as `viewer`, with the target's display name set to `name`.
async fn profile_of(sim: &mut Sim, viewer: u64, target: u64, name: &str) -> Reply {
    let resolved = json!({
        "users": { target.to_string(): person_json(target, "bartholomew.fizzlebottom", Some(name), false) },
    });
    sim.command_resolved(viewer, "profile", json!([user_option("user", target)]), resolved)
        .await
}

#[tokio::test]
async fn the_showcase_editor_picks_fish_pet_furniture_and_house() {
    let Some(mut sim) = Sim::new("the_showcase_editor_picks_fish_pet_furniture_and_house").await else {
        return;
    };
    let (user, other, viewer) = (unique(), unique(), unique());
    let db = sim.app.db.clone();
    let content = Content::get();
    let (squid, mackerel) = (&content.fish[0], &content.fish[1]);
    db.log_catch(user, &squid.id, 31.5).await.unwrap();
    db.log_catch(user, &mackerel.id, 12.0).await.unwrap();
    let pet = db.adopt(user, &content.pets[0].id, "Sir Clicks").await.unwrap();
    let others_pet = db.adopt(other, &content.pets[0].id, "Not Yours").await.unwrap();
    let lamp = content
        .items()
        .find(|i| i.base.id.contains("lamp"))
        .expect("a lamp in the catalog");
    db.grant_item(user, &lamp.id(), 1).await.unwrap();
    let plain = sim.command(user, "profile", json!([])).await;

    // Nothing chosen: the editor says so and offers nothing to clear; fish are picked first.
    let panel = sim
        .command(user, "customize", json!([{ "name": "showcase", "type": 1 }]))
        .await;
    assert_eq!(panel.title(), "Profile showcase");
    assert!(panel.ephemeral);
    card_image(&panel, (960, 540));
    assert_eq!(panel.rows(), 3);
    assert!(
        panel.description().contains("Nothing is on your showcase"),
        "{}",
        panel.description()
    );
    assert!(panel.button("Clear slot").disabled && panel.button("Clear all").disabled);
    assert_eq!(panel.options("sw:k").len(), 3);
    let fish_values: Vec<_> = panel.options("sw:s:f").iter().map(|o| o.value.as_str()).collect();
    let mut expected = vec![squid.name.as_str(), mackerel.name.as_str()];
    expected.sort_by_key(|n| n.to_lowercase());
    let labels: Vec<_> = panel.options("sw:s:f").iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, expected, "the fish book, by name");
    assert_eq!(fish_values.len(), 2);
    assert!(
        panel.options("sw:s:f")[0]
            .description
            .as_deref()
            .is_some_and(|d| d.contains("best") && d.contains("cm")),
        "{:?}",
        panel.options("sw:s:f")[0].description
    );
    assert!(panel.button("Show house").disabled, "no house to show yet");
    let no_house = sim.component(user, "sw:h:f", &[], &panel).await;
    assert!(
        no_house.description().contains("no house yet"),
        "{}",
        no_house.description()
    );
    assert!(!db.user(user).await.unwrap().showcase_house);

    // A fish goes on the card: the layout makes room for the row.
    let fish = sim.pick(user, &panel, "sw:s:f", &squid.id).await;
    assert!(
        fish.description().contains(&format!("Saved. **{}**", squid.name)),
        "{}",
        fish.description()
    );
    assert_eq!(
        db.user(user).await.unwrap().showcase_fish.as_deref(),
        Some(squid.id.as_str())
    );
    assert!(fish.options("sw:s:f").iter().any(|o| o.value == squid.id && o.default));
    assert!(!fish.button("Clear slot").disabled && !fish.button("Clear all").disabled);
    assert!(
        differ_in(
            &card_pixels(&fish, (960, 540)),
            &card_pixels(&plain, (960, 540)),
            296..920,
            260..356
        ),
        "a tile stands in the showcase row"
    );

    // The pet and the furniture, through the slot menu.
    let pets = sim.pick(user, &fish, "sw:k", "p").await;
    assert!(pets.menu("sw:s:p").placeholder.as_deref() == Some("Pick a pet…"));
    assert_eq!(pets.options("sw:s:p").len(), 1);
    assert_eq!(pets.options("sw:s:p")[0].label, "Sir Clicks");
    let with_pet = sim.pick(user, &pets, "sw:s:p", &pet.id.to_string()).await;
    assert_eq!(db.user(user).await.unwrap().showcase_pet, Some(pet.id));
    let furniture = sim.pick(user, &with_pet, "sw:k", "i").await;
    assert!(
        furniture.options("sw:s:i").iter().any(|o| o.value == lamp.id()),
        "owned furniture is listed"
    );
    let with_lamp = sim.pick(user, &furniture, "sw:s:i", &lamp.id()).await;
    assert_eq!(
        db.user(user).await.unwrap().showcase_item.as_deref(),
        Some(lamp.id().as_str())
    );
    assert!(
        with_lamp.description().contains(&squid.name) && with_lamp.description().contains("Sir Clicks"),
        "{}",
        with_lamp.description()
    );

    // Hand-made picks of things that are not theirs are refused: a fish never caught, somebody
    // else's pet, furniture nobody owns.
    for (menu, value) in [
        ("sw:s:f:0", content.fish.last().expect("fish").id.clone()),
        ("sw:s:p:0", others_pet.id.to_string()),
        ("sw:s:i:0", "no_such_item".to_owned()),
    ] {
        let refused = sim.component(user, menu, &[&value], &with_lamp).await;
        assert!(
            refused.content().contains("isn't available any more"),
            "{menu}: {}",
            refused.content()
        );
    }
    let row = db.user(user).await.unwrap();
    assert_eq!(row.showcase_pet, Some(pet.id), "the pet is still theirs");
    assert_eq!(row.showcase_fish.as_deref(), Some(squid.id.as_str()));

    // The house thumbnail needs a house; once there is one, a button shows it.
    sim.command(user, "house", json!([{ "name": "edit", "type": 1 }])).await;
    let house_panel = sim
        .command(user, "customize", json!([{ "name": "showcase", "type": 1 }]))
        .await;
    assert!(!house_panel.button("Show house").disabled);
    let before_house = card_pixels(&house_panel, (960, 540));
    let house = sim.click(user, &house_panel, "Show house").await;
    assert!(
        house.description().contains("Your house is on your card."),
        "{}",
        house.description()
    );
    assert!(
        house.description().contains("**House** shown"),
        "{}",
        house.description()
    );
    assert!(db.user(user).await.unwrap().showcase_house);
    // The house tile is the right-hand one of the four.
    assert!(differ_in(
        &card_pixels(&house, (960, 540)),
        &before_house,
        760..912,
        270..350
    ));

    // A click costs what the editor shows and nothing twice. All four tiles are on the card here;
    // the pick (2), the player (1), each tile's check (3) and the house (3), the menu's list (1),
    // the worn badges (2) and the card's ranks and partner (3) come to 15. It was 23, with the
    // showcase read twice and whole lists fetched to check one item.
    let before = queries();
    let repicked = sim.pick(user, &house, "sw:s:f", &squid.id).await;
    let spent = queries() - before;
    assert!(repicked.description().contains("Saved."), "{}", repicked.description());
    assert!(spent > 0, "the statements are being counted");
    assert!(spent <= 16, "a showcase click ran {spent} statements");

    // Anyone looking at the profile sees the showcase, and the card is the editor's picture.
    let seen = profile_of(&mut sim, viewer, user, "Sim").await;
    assert!(differ_in(
        &card_pixels(&seen, (960, 540)),
        &card_pixels(&plain, (960, 540)),
        296..920,
        260..356
    ));
    let again = profile_of(&mut sim, viewer, user, "Sim").await;
    // Only the showcase is compared: the card also shows a global rank, which moves while the other tests
    // in this binary earn XP.
    assert!(
        !differ_in(
            &card_pixels(&seen, (960, 540)),
            &card_pixels(&again, (960, 540)),
            296..920,
            260..356
        ),
        "drawn from the same things, the showcase is drawn the same"
    );

    // Hiding the house, clearing one slot and clearing everything, each undoes its part.
    let hidden = sim.click(user, &house, "Hide house").await;
    assert!(
        hidden.description().contains("**House** hidden"),
        "{}",
        hidden.description()
    );
    let fish_slot = sim.pick(user, &hidden, "sw:k", "f").await;
    let cleared = sim.click(user, &fish_slot, "Clear slot").await;
    let row = db.user(user).await.unwrap();
    assert_eq!(row.showcase_fish, None);
    assert_eq!(row.showcase_pet, Some(pet.id), "the other slots stay");
    assert!(cleared.button("Clear slot").disabled, "the fish slot is empty now");
    let nothing = sim.click(user, &cleared, "Clear all").await;
    let row = db.user(user).await.unwrap();
    assert_eq!(
        (
            row.showcase_fish,
            row.showcase_pet,
            row.showcase_item,
            row.showcase_house
        ),
        (None, None, None, false)
    );
    assert!(
        nothing.description().contains("Nothing is on your showcase"),
        "{}",
        nothing.description()
    );

    // With nothing chosen the card is the card it always was: the same bio, stats and footer.
    let profile = sim.command(user, "profile", json!([])).await;
    let (now, was) = (card_pixels(&profile, (960, 540)), card_pixels(&plain, (960, 540)));
    for (xs, ys) in [(0..960, 0..288), (0..960, 396..540)] {
        assert!(!differ_in(&now, &was, xs, ys));
    }
    // Stale choices drop out of the card quietly: a pet that left leaves no hole in the editor either.
    db.set_showcase_pet(user, Some(pet.id + 1_000_000)).await.unwrap();
    let stale = sim.command(user, "profile", json!([])).await;
    assert!(!differ_in(&card_pixels(&stale, (960, 540)), &was, 0..960, 0..288));
}

#[tokio::test]
async fn a_busy_profile_card_stays_clean_with_a_long_name_and_a_full_bio() {
    let Some(mut sim) = Sim::new("a_busy_profile_card_stays_clean_with_a_long_name_and_a_full_bio").await else {
        return;
    };
    let (user, viewer) = (unique(), unique());
    let db = sim.app.db.clone();
    let content = Content::get();
    let bio = "Collector of shiny shells, builder of tiny pixel houses, feeder of crabs and chaser of rare fish. \
               Ask me about Sir Clicks or my 4 AM fishing trips!";
    assert!(bio.chars().count() <= 160);
    db.set_bio(user, bio).await.unwrap();
    db.add_message_xp(sim.guild, user, 70_000, 0).await.unwrap();
    db.earn_shells(user, 1_284_500, "test").await.unwrap();
    for fish in content.fish.iter().take(12) {
        db.log_catch(user, &fish.id, 20.0).await.unwrap();
    }
    db.set_title(user, Some("lighthouse_keeper")).await.unwrap();
    let pet = db.adopt(user, &content.pets[1].id, "Sir Clicks-a-lot").await.unwrap();
    let sofa = content.items().find(|i| i.base.id.contains("bed")).expect("a bed");
    db.grant_item(user, &sofa.id(), 1).await.unwrap();
    sim.command(user, "house", json!([{ "name": "edit", "type": 1 }])).await;
    db.set_character(user, &Character::starter(content).encode())
        .await
        .unwrap();

    // Every slot filled, a gold frame and a teal accent.
    db.set_showcase_fish(user, Some(&content.fish[3].id)).await.unwrap();
    db.set_showcase_pet(user, Some(pet.id)).await.unwrap();
    db.set_showcase_item(user, Some(&sofa.id())).await.unwrap();
    db.set_showcase_house(user, true).await.unwrap();
    db.set_frame(user, "gold").await.unwrap();
    db.set_accent(user, Some("#06d6a0")).await.unwrap();
    let busy = profile_of(&mut sim, viewer, user, LONG_NAME).await;
    card_image(&busy, (960, 540));
    assert_eq!(busy.files[0].0, "profile.png");

    // The same player with nothing on the showcase and no frame: today's layout, same size.
    db.clear_showcase(user).await.unwrap();
    db.reset_card_style(user).await.unwrap();
    let classic = profile_of(&mut sim, viewer, user, LONG_NAME).await;
    card_image(&classic, (960, 540));
    assert!(differ_in(
        &card_pixels(&busy, (960, 540)),
        &card_pixels(&classic, (960, 540)),
        296..920,
        260..420
    ));

    // The same busy card over a background picture, with light panels.
    db.earn_shells(user, 5_000, "test").await.unwrap();
    db.buy_background(user, "shellwash_morning", 800).await.unwrap();
    db.equip_background(user, "shellwash_morning").await.unwrap();
    db.set_showcase_fish(user, Some(&content.fish[3].id)).await.unwrap();
    db.set_showcase_pet(user, Some(pet.id)).await.unwrap();
    db.set_showcase_item(user, Some(&sofa.id())).await.unwrap();
    db.set_showcase_house(user, true).await.unwrap();
    db.set_frame(user, "rope").await.unwrap();
    db.set_panel_opacity(user, 0).await.unwrap();
    let scenic = profile_of(&mut sim, viewer, user, LONG_NAME).await;
    card_image(&scenic, (960, 540));
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
    // The owner may set up any role the bot can hand out.
    sim.own_server(admin);
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
    // The owner may set up any role the bot can hand out.
    sim.own_server(admin);
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
        reply.content().contains("Members: 3/5 on")
            && reply.content().contains("Voice: 1/5 on")
            && reply.content().contains("Moderation: 4/4 on"),
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
    assert_eq!(
        form.modal.as_ref().map(|m| m.0.as_str()),
        Some(format!("gb:n:{host}").as_str())
    );
    let signed = sim
        .submit_fields(
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
        sim.submit_fields(visitor, &form, &[("note", &format!("Visitor {n} was here. Cozy!"))])
            .await;
        visitors.push(visitor);
    }
    let stranger_visit = sim
        .command_resolved(stranger, "house", visit_options(host), resolved(&[host], &[]))
        .await;
    let form = sim.click(stranger, &stranger_visit, "Sign guestbook").await;
    sim.submit_fields(stranger, &form, &[("note", "buy my stuff, cheap cheap")])
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
    // And the card draws it: "♥ Sim" in the corner of the about-me panel, which stays empty for
    // somebody without a partner.
    let corner = |card: &Reply| card_pixels(card, (960, 540));
    let partnered = sim.command(a, "profile", json!([])).await;
    let loner = sim.command(c, "profile", json!([])).await;
    assert!(
        differ_in(&corner(&partnered), &corner(&loner), 640..900, 156..178),
        "the partner line is on the card"
    );
    // Whoever looks at the profile sees the partner of the player it belongs to.
    let looked_at = profile_of(&mut sim, c, a, "Sim").await;
    assert!(differ_in(&corner(&looked_at), &corner(&loner), 640..900, 156..178));

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
    // The same player cannot be asked again for a minute (Redis keeps real time: end the wait by hand).
    sim.app
        .cache
        .release_cooldown(&format!("ask:partner:{e}:{f}"))
        .await
        .unwrap();
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
    let single = sim.command(a, "profile", json!([])).await;
    assert!(
        !differ_in(&corner(&single), &corner(&loner), 640..900, 156..178),
        "the corner is empty again"
    );
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
    // A request stops being good after a day. (The same player cannot be asked again for a minute; Redis keeps
    // real time, so the wait is ended by hand.)
    sim.app
        .cache
        .release_cooldown(&format!("ask:friend:{a}:{c}"))
        .await
        .unwrap();
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

// /Leveling toolkit

/// Whether the mock refuses to open direct messages (the member closed them).
static DMS_CLOSED: AtomicBool = AtomicBool::new(false);
/// Members whose direct messages are open (`false`) or closed (`true`) whatever [`DMS_CLOSED`] says, so
/// a test of its own can rely on them.
static DM_OVERRIDES: LazyLock<Mutex<HashMap<u64, bool>>> = LazyLock::new(Mutex::default);
/// Voice ticks pay every server Redis lists, so tests that tick take turns.
static VOICE_TICKS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The AFK voice channel of each mock server.
static AFK_CHANNELS: LazyLock<Mutex<HashMap<u64, u64>>> = LazyLock::new(Mutex::default);

fn afk_channel(guild: u64) -> Option<u64> {
    AFK_CHANNELS.lock().unwrap().get(&guild).copied()
}

/// `POST /users/@me/channels`.
fn open_direct_message(recipient: u64) -> (StatusCode, String) {
    let closed = DM_OVERRIDES
        .lock()
        .unwrap()
        .get(&recipient)
        .copied()
        .unwrap_or_else(|| DMS_CLOSED.load(Ordering::SeqCst));
    if closed {
        let refusal = json!({ "message": "Cannot send messages to this user", "code": 50007 });
        return (StatusCode::FORBIDDEN, refusal.to_string());
    }
    // The channel is the recipient's own id, so a test can tell whose messages are whose.
    let channel = json!({ "id": recipient.to_string(), "type": 1, "recipients": [] });
    (StatusCode::OK, channel.to_string())
}

fn number_option(name: &str, value: f64) -> Value {
    json!({ "name": name, "type": 10, "value": value })
}

/// `data.resolved` for a user option whose member holds `roles` here.
fn resolved_member(user: u64, roles: &[u64]) -> Value {
    json!({
        "users": { user.to_string(): user_json(user) },
        "members": {
            user.to_string(): {
                "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
                "joined_at": "2024-01-01T00:00:00.000000+00:00",
                "flags": 0,
                "permissions": "0",
                "pending": false,
            },
        },
    })
}

impl Sim {
    /// A chat message from `user` in `channel` carrying the roles they hold.
    fn chat_in(&self, user: u64, channel: u64, roles: &[u64]) -> MessageCreate {
        let mut message = serde_json::to_value(self.chat(user, roles)).expect("message JSON");
        message["channel_id"] = json!(channel.to_string());
        serde_json::from_value(message).expect("message JSON")
    }

    async fn say_in(&self, user: u64, channel: u64, roles: &[u64]) {
        ERRORS.with_borrow_mut(Vec::clear);
        on_message(self.app.clone(), self.chat_in(user, channel, roles))
            .await
            .expect("the message handler failed");
    }

    /// `user` is now in voice `channel` (`None`: left) holding `roles`, muted and deafened as given.
    async fn voice_in(&self, user: u64, channel: Option<u64>, roles: &[u64], mute: bool, deaf: bool, bot: bool) {
        let event: VoiceStateUpdate = serde_json::from_value(json!({
            "guild_id": self.guild.to_string(),
            "channel_id": channel.map(|c| c.to_string()),
            "user_id": user.to_string(),
            "member": {
                "user": person_json(user, "voice-user", Some("voice-user"), bot),
                "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
                "joined_at": "2024-01-01T00:00:00.000000+00:00",
                "deaf": false,
                "mute": false,
                "flags": 0,
                "pending": false,
            },
            "session_id": "sim",
            "deaf": false,
            "mute": false,
            "self_deaf": deaf,
            "self_mute": mute,
            "self_stream": false,
            "self_video": false,
            "suppress": false,
            "request_to_speak_timestamp": null,
        }))
        .expect("voice state JSON");
        on_voice_state_update(self.app.clone(), event)
            .await
            .expect("the voice handler failed");
    }

    /// Tells the mock about a channel: its type (0 text, 2 voice, 4 category, 11 thread, 15 forum) and
    /// what it sits under.
    fn channel_is(&self, id: u64, kind: u8, parent: Option<u64>) {
        let channel = json!({
            "id": id.to_string(),
            "type": kind,
            "guild_id": self.guild.to_string(),
            "name": "sim-channel",
            "parent_id": parent.map(|p| p.to_string()),
        });
        self.mock.channels.lock().unwrap().insert(id, channel);
    }

    /// How often the mock was asked about `channel`.
    fn channel_lookups(&self, channel: u64) -> usize {
        self.mock
            .channel_lookups
            .lock()
            .unwrap()
            .get(&channel)
            .copied()
            .unwrap_or(0)
    }

    /// A GUILD_CREATE for this server: who sits in voice as (user, channel, muted, deafened), the
    /// members Discord lists with their bot flag and roles, and the AFK channel.
    fn guild_create(
        &self,
        voice: &[(u64, u64, bool, bool)],
        members: &[(u64, bool, &[u64])],
        afk: Option<u64>,
    ) -> GuildCreate {
        let mut guild = guild_json(self.guild);
        guild["afk_channel_id"] = json!(afk.map(|c| c.to_string()));
        guild["voice_states"] = voice
            .iter()
            .map(|(user, channel, mute, deaf)| {
                json!({
                    "channel_id": channel.to_string(),
                    "user_id": user.to_string(),
                    "session_id": "sim",
                    "deaf": false,
                    "mute": false,
                    "self_deaf": deaf,
                    "self_mute": mute,
                    "self_video": false,
                    "suppress": false,
                    "request_to_speak_timestamp": null,
                })
            })
            .collect::<Vec<_>>()
            .into();
        guild["members"] = members
            .iter()
            .map(|(user, bot, roles)| {
                let mut member = member_json(*user, roles);
                member["user"] = person_json(*user, "snapshot-user", Some("snapshot-user"), *bot);
                member
            })
            .collect::<Vec<_>>()
            .into();
        serde_json::from_value(guild).expect("guild create JSON")
    }

    /// Hands a GUILD_CREATE to the event handler.
    async fn snapshot(&self, voice: &[(u64, u64, bool, bool)], members: &[(u64, bool, &[u64])], afk: Option<u64>) {
        on_guild_create(self.app.clone(), self.guild_create(voice, members, afk))
            .await
            .expect("the guild create handler failed");
    }

    /// Who voice XP remembers in this server, by user id. A member who left is only a note in channel 0 (see
    /// `voice_xp::LEFT_KEPT`): not somebody in voice.
    async fn voice_members(&self) -> Vec<u64> {
        let mut users: Vec<u64> = self
            .app
            .cache
            .map_all(&format!("vc:{}", self.guild))
            .await
            .expect("voice hash")
            .iter()
            .filter(|(_, record)| !record.starts_with("0|"))
            .map(|(user, _)| user.parse().expect("a user id"))
            .collect();
        users.sort_unstable();
        users
    }

    /// Whether the voice ticker still looks at this server.
    async fn ticks_voice(&self) -> bool {
        self.app
            .cache
            .set_members("vc:guilds")
            .await
            .expect("voice guilds")
            .contains(&self.guild.to_string())
    }

    /// A member's server XP, 0 before they have a row.
    async fn server_xp(&self, user: u64) -> i64 {
        self.app
            .db
            .member_rank(self.guild, user)
            .await
            .expect("rank")
            .map_or(0, |r| r.xp)
    }

    async fn global_xp(&self, user: u64) -> i64 {
        self.app.db.user(user).await.expect("user row").xp
    }

    async fn points(&self, user: u64) -> i64 {
        self.app.db.member_points(self.guild, user).await.expect("points").0
    }

    /// Runs `/config xp ...`.
    async fn xp(&mut self, admin: u64, sub_command: Value) -> Reply {
        self.command(admin, "config", json!([group("xp", sub_command)])).await
    }
}

/// The server's PNG height, to tell a card with a podium from one without.
fn png_height(reply: &Reply) -> u32 {
    image::load_from_memory(&reply.files[0].1).expect("a png").height()
}

#[tokio::test]
async fn xp_multipliers_and_voice_xp_are_configured_with_config() {
    let Some(mut sim) = Sim::new("xp_multipliers_and_voice_xp_are_configured_with_config").await else {
        return;
    };
    let admin = unique();
    let (channel, lounge) = (unique(), unique());
    let (booster_id, muted_id) = (unique(), unique());
    let booster = sim.guild_role(booster_id, "Booster", 2);
    let muted = sim.guild_role(muted_id, "Muted", 2);
    sim.permissions = MANAGE_GUILD;

    let by_channel = |value: f64, id: u64| {
        sub(
            "multiplier",
            vec![number_option("value", value), channel_option("channel", id)],
        )
    };
    let by_role = |value: f64, role: &Value| {
        sub(
            "multiplier",
            vec![number_option("value", value), role_option("role", role)],
        )
    };

    // Nothing set yet.
    let shown = sim.xp(admin, sub("show", vec![])).await;
    assert!(
        shown.description().contains("No multipliers yet"),
        "{}",
        shown.description()
    );
    assert!(
        shown.field("Voice XP").contains("**off**"),
        "{}",
        shown.field("Voice XP")
    );

    // A channel, a voice channel, a booster role and a no-XP role.
    let reply = sim
        .command_resolved(
            admin,
            "config",
            json!([group("xp", by_channel(2.0, channel))]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(
        reply.content().contains(&format!("<#{channel}> now earns **×2**")),
        "{}",
        reply.content()
    );
    sim.command_resolved(
        admin,
        "config",
        json!([group("xp", by_channel(1.5, lounge))]),
        resolved_channels(&[lounge]),
    )
    .await;
    let reply = sim.xp(admin, by_role(3.0, &booster)).await;
    assert!(
        reply.content().contains(&format!("<@&{booster_id}> now earns **×3**")),
        "{}",
        reply.content()
    );
    let reply = sim.xp(admin, by_role(0.0, &muted)).await;
    assert!(
        reply.content().contains("**no XP and no points**"),
        "{}",
        reply.content()
    );

    // Exactly one of channel and role.
    let neither = sim
        .xp(admin, sub("multiplier", vec![number_option("value", 2.0)]))
        .await;
    assert!(
        neither.content().contains("channel **or** a role"),
        "{}",
        neither.content()
    );
    let both = sim
        .command_resolved(
            admin,
            "config",
            json!([group(
                "xp",
                sub(
                    "multiplier",
                    vec![
                        number_option("value", 2.0),
                        channel_option("channel", channel),
                        role_option("role", &booster)
                    ]
                )
            )]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(both.content().contains("channel **or** a role"), "{}", both.content());

    let shown = sim.xp(admin, sub("show", vec![])).await;
    assert_eq!(shown.title(), "XP rules");
    let lists = shown.description();
    for line in [
        format!("<#{channel}> · **×2**"),
        format!("<#{lounge}> · **×1.5**"),
        format!("<@&{booster_id}> · **×3**"),
        format!("<@&{muted_id}> · **no XP**"),
    ] {
        assert!(lists.contains(&line), "{line} in {lists}");
    }
    assert_eq!(sim.app.db.xp_rules(sim.guild).await.unwrap().roles.len(), 2);

    // A multiplier of 1 is the default, so it drops the rule; clear removes one or all.
    let reply = sim
        .command_resolved(
            admin,
            "config",
            json!([group("xp", by_channel(1.0, lounge))]),
            resolved_channels(&[lounge]),
        )
        .await;
    assert!(reply.content().contains("earns normal XP"), "{}", reply.content());
    assert!(
        !sim.app
            .db
            .xp_rules(sim.guild)
            .await
            .unwrap()
            .channels
            .contains_key(&lounge)
    );
    let reply = sim.xp(admin, sub("clear", vec![role_option("role", &muted)])).await;
    assert!(
        reply.content().contains("earns normal XP and points again"),
        "{}",
        reply.content()
    );
    let reply = sim.xp(admin, sub("clear", vec![role_option("role", &muted)])).await;
    assert!(reply.content().contains("had no multiplier"), "{}", reply.content());
    let reply = sim.xp(admin, sub("clear", vec![])).await;
    assert!(
        reply.content().contains("Cleared **2** XP rules"),
        "{}",
        reply.content()
    );
    let reply = sim.xp(admin, sub("clear", vec![])).await;
    assert!(reply.content().contains("no XP rules to clear"), "{}", reply.content());

    // Voice XP: off until switched on; the amount is per minute.
    let voice = sim
        .xp(
            admin,
            sub("voice", vec![bool_option("enabled", true), int_option("amount", 15)]),
        )
        .await;
    assert!(
        voice.description().contains("**on** · 15 XP a minute"),
        "{}",
        voice.description()
    );
    let voice = sim.xp(admin, sub("voice", vec![])).await;
    assert!(
        voice.description().contains("**on** · 15 XP"),
        "no options only shows: {}",
        voice.description()
    );
    let settings = sim.app.db.guild_settings(sim.guild).await.unwrap();
    assert!((settings.voice_xp_enabled, settings.voice_xp_amount) == (true, 15));

    // The level-up mode, and the placeholders in the option text.
    let leveling = |options: Vec<Value>| json!([sub("leveling", options)]);
    for (mode, wording) in [
        ("dm", "arrive by direct message"),
        ("off", "are off"),
        ("current", "in the channel the member chatted in"),
        ("channel", "in the channel the member chatted in"),
    ] {
        let reply = sim
            .command(admin, "config", leveling(vec![text_option("mode", mode)]))
            .await;
        assert!(
            reply.field("Leveling").contains(wording),
            "{mode}: {}",
            reply.field("Leveling")
        );
    }
    let reply = sim
        .command_resolved(
            admin,
            "config",
            leveling(vec![channel_option("channel", channel)]),
            resolved_channels(&[channel]),
        )
        .await;
    assert!(
        reply.field("Leveling").contains(&format!("in <#{channel}>")),
        "{}",
        reply.field("Leveling")
    );
    assert!(
        reply.field("XP rules and voice XP").contains("**on**"),
        "{}",
        reply.field("XP rules and voice XP")
    );
    let definitions = pinchora_commands::command_definitions();
    let config = serde_json::to_value(definitions.iter().find(|c| c.name == "config").expect("config")).unwrap();
    let leveling_message = config["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "leveling")
        .and_then(|o| o["options"].as_array())
        .and_then(|options| options.iter().find(|o| o["name"] == "message"))
        .expect("the message option");
    let doc = leveling_message["description"].as_str().unwrap();
    assert!(
        ["{user}", "{level}", "{server}", "{role}"]
            .iter()
            .all(|p| doc.contains(p)),
        "the placeholders are documented: {doc}"
    );
}

#[tokio::test]
async fn chat_multipliers_scale_server_xp_and_points_but_not_global_xp() {
    let Some(mut sim) = Sim::new("chat_multipliers_scale_server_xp_and_points_but_not_global_xp").await else {
        return;
    };
    let admin = unique();
    let (double, silent, plain) = (unique(), unique(), unique());
    let (booster_id, hush_id) = (unique(), unique());
    let booster = sim.guild_role(booster_id, "Booster", 2);
    let hush = sim.guild_role(hush_id, "Hushed", 2);
    sim.permissions = MANAGE_GUILD;
    for (id, value) in [(double, 2.0), (silent, 0.0)] {
        sim.command_resolved(
            admin,
            "config",
            json!([group(
                "xp",
                sub(
                    "multiplier",
                    vec![number_option("value", value), channel_option("channel", id)]
                )
            )]),
            resolved_channels(&[id]),
        )
        .await;
    }
    for (role, value) in [(&booster, 3.0), (&hush, 0.0)] {
        sim.xp(
            admin,
            sub(
                "multiplier",
                vec![number_option("value", value), role_option("role", role)],
            ),
        )
        .await;
    }
    // Points are on by default, 1 to 3 a message.
    sim.command(
        admin,
        "config",
        json!([sub("points", vec![int_option("min", 4), int_option("max", 4)])]),
    )
    .await;

    // Nothing set for this channel and these members: the usual 15 to 25 XP, global XP the same.
    let normal = unique();
    sim.say_in(normal, plain, &[]).await;
    let xp = sim.server_xp(normal).await;
    assert!((15..=25).contains(&xp), "{xp}");
    assert_eq!(sim.global_xp(normal).await, xp);
    assert_eq!(sim.points(normal).await, 4);

    // A x2 channel: twice the server XP and points, the global XP stays the base amount.
    let a = unique();
    sim.say_in(a, double, &[]).await;
    let global = sim.global_xp(a).await;
    assert!((15..=25).contains(&global), "{global}");
    assert_eq!(sim.server_xp(a).await, global * 2);
    assert_eq!(sim.points(a).await, 8);

    // A x3 role in a normal channel; the channel and the role multiply.
    let b = unique();
    sim.say_in(b, plain, &[booster_id]).await;
    assert_eq!(sim.server_xp(b).await, sim.global_xp(b).await * 3);
    assert_eq!(sim.points(b).await, 12);
    let c = unique();
    sim.say_in(c, double, &[booster_id, unique()]).await;
    assert_eq!(sim.server_xp(c).await, sim.global_xp(c).await * 6);
    assert_eq!(sim.points(c).await, 24);

    // A no-XP channel and a no-XP role: nothing for the server (even next to a booster role), the
    // global XP carries on as usual.
    for (user, channel, roles) in [
        (unique(), silent, vec![]),
        (unique(), plain, vec![hush_id]),
        (unique(), plain, vec![hush_id, booster_id]),
        (unique(), silent, vec![booster_id]),
    ] {
        sim.say_in(user, channel, &roles).await;
        assert_eq!(sim.server_xp(user).await, 0, "channel {channel} roles {roles:?}");
        assert_eq!(sim.points(user).await, 0);
        assert!((15..=25).contains(&sim.global_xp(user).await));
    }
    assert!(
        sim.app
            .db
            .guild_leaderboard(sim.guild, 20, 0)
            .await
            .unwrap()
            .iter()
            .all(|r| r.value > 0),
        "members who earned nothing stay off the board"
    );

    // Clearing the rules puts everything back to normal; @everyone is a server-wide multiplier.
    sim.xp(admin, sub("clear", vec![])).await;
    let everyone = role_json(sim.guild, "@everyone", 0, false);
    sim.xp(
        admin,
        sub(
            "multiplier",
            vec![number_option("value", 5.0), role_option("role", &everyone)],
        ),
    )
    .await;
    let d = unique();
    sim.say_in(d, plain, &[]).await;
    assert_eq!(sim.server_xp(d).await, sim.global_xp(d).await * 5, "@everyone x5");
    assert_eq!(sim.points(d).await, 20);
}

#[tokio::test]
async fn voice_xp_pays_once_a_minute_to_members_with_company() {
    let Some(mut sim) = Sim::new("voice_xp_pays_once_a_minute_to_members_with_company").await else {
        return;
    };
    let Some(mut restarted) = Sim::new("voice_xp_pays_once_a_minute_after_a_restart").await else {
        return;
    };
    let _turn = VOICE_TICKS.lock().await;
    // Minute-long locks live in Redis: a clock of its own keeps repeated runs apart.
    sim.app.shift_clock(chrono::Duration::days((unique() % 36_500) as i64));
    let admin = unique();
    let (lounge, den, quiet, afk, tavern) = (unique(), unique(), unique(), unique(), unique());
    let (a, b, alone, idle, listener, robot, sleeper1, sleeper2) = (
        unique(),
        unique(),
        unique(),
        unique(),
        unique(),
        unique(),
        unique(),
        unique(),
    );
    let (booster_id, hush_id) = (unique(), unique());
    sim.guild_role(booster_id, "Booster", 2);
    sim.guild_role(hush_id, "Hushed", 2);
    AFK_CHANNELS.lock().unwrap().insert(sim.guild, afk);
    sim.permissions = MANAGE_GUILD;

    let seated = [
        (a, lounge, false, false, false),
        (b, lounge, false, false, false),
        (robot, lounge, false, false, true),
        (alone, quiet, false, false, false),
        (idle, den, true, true, false),
        (listener, den, false, false, false),
        (sleeper1, afk, false, false, false),
        (sleeper2, afk, false, false, false),
    ];
    for (user, channel, mute, deaf, bot) in seated {
        sim.voice_in(user, Some(channel), &[], mute, deaf, bot).await;
    }

    // Voice XP is off until a server turns it on: nobody is even remembered, and a minute passes
    // without anyone earning.
    assert!(
        sim.voice_members().await.is_empty(),
        "nothing is tracked while voice XP is off"
    );
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!(sim.server_xp(a).await, 0);

    let reply = sim
        .xp(
            admin,
            sub("voice", vec![bool_option("enabled", true), int_option("amount", 20)]),
        )
        .await;
    assert!(
        reply.description().contains("20 XP a minute"),
        "{}",
        reply.description()
    );
    assert!(
        reply.description().contains("already in a voice channel"),
        "switching it on says who is missed: {}",
        reply.description()
    );
    // Discord cannot list who is in voice, so everyone is noticed when they next change state.
    for (user, channel, mute, deaf, bot) in seated {
        sim.voice_in(user, Some(channel), &[], mute, deaf, bot).await;
    }

    // The next minute: members with company earn; the lone member, the muted-and-deafened one, the
    // bot and the AFK channel do not. A deafened member next to a listener still keeps them company.
    sim.app.shift_clock(chrono::Duration::seconds(60));
    let paid = pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert!(paid >= 3, "{paid}");
    for earner in [a, b, listener] {
        assert_eq!(sim.server_xp(earner).await, 20, "{earner}");
        assert_eq!(sim.global_xp(earner).await, 0, "voice earns server XP only");
    }
    for nobody in [alone, idle, robot, sleeper1, sleeper2] {
        assert_eq!(sim.server_xp(nobody).await, 0, "{nobody}");
    }

    // The same minute again, from this process or from a restarted one, pays nobody twice.
    assert_eq!(pinchora_commands::voice_tick(&sim.app).await.expect("tick"), 0);
    restarted.guild = sim.guild;
    let ahead = sim.app.now() - restarted.app.now();
    restarted.app.shift_clock(ahead);
    assert_eq!(pinchora_commands::voice_tick(&restarted.app).await.expect("tick"), 0);
    assert_eq!(sim.server_xp(a).await, 20, "paid once for the minute");

    // The restarted process knows who is in voice (it lives in Redis) and pays the next minute.
    sim.app.shift_clock(chrono::Duration::seconds(60));
    restarted.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&restarted.app).await.expect("tick");
    assert_eq!((sim.server_xp(a).await, sim.server_xp(listener).await), (40, 40));

    // Multipliers apply to voice too, by the voice channel and the member's roles.
    sim.command_resolved(
        admin,
        "config",
        json!([group(
            "xp",
            sub(
                "multiplier",
                vec![number_option("value", 2.0), channel_option("channel", lounge)]
            )
        )]),
        resolved_channels(&[lounge]),
    )
    .await;
    for (role, value) in [(hush_id, 0.0), (booster_id, 3.0)] {
        let role = role_json(role, "role", 2, false);
        sim.xp(
            admin,
            sub(
                "multiplier",
                vec![number_option("value", value), role_option("role", &role)],
            ),
        )
        .await;
    }
    sim.voice_in(a, Some(lounge), &[booster_id], false, false, false).await;
    sim.voice_in(b, Some(lounge), &[hush_id], false, false, false).await;
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!(sim.server_xp(a).await, 40 + 120, "x2 channel, x3 role");
    assert_eq!(sim.server_xp(b).await, 40, "a no-XP role earns nothing");

    // Leaving, and muting and deafening both at once, stop the XP.
    sim.voice_in(a, None, &[], false, false, false).await;
    sim.voice_in(b, Some(lounge), &[], true, true, false).await;
    sim.voice_in(listener, None, &[], false, false, false).await;
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!(
        (sim.server_xp(a).await, sim.server_xp(b).await),
        (160, 40),
        "b is alone and idle"
    );
    assert_eq!(sim.server_xp(listener).await, 60, "gone before this minute");

    // A level-up in voice is announced in the voice channel's chat when no channel is set.
    let (climber, partner) = (unique(), unique());
    sim.app.db.add_message_xp(sim.guild, climber, 90, 0).await.unwrap();
    sim.voice_in(climber, Some(tavern), &[], false, false, false).await;
    sim.voice_in(partner, Some(tavern), &[], false, false, false).await;
    sim.posts();
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    let posts = sim.posts();
    assert!(
        in_channel(&posts, tavern)
            .iter()
            .any(|p| p.content().contains(&format!("<@{climber}> reached **level 1**"))),
        "the level-up lands in the voice channel's chat"
    );

    // The week's board counts voice XP too.
    let week = pinchora_core::leveling::week_start(sim.app.now());
    let board = sim.app.db.weekly_leaderboard(sim.guild, week, 10, 0).await.unwrap();
    assert_eq!(board.iter().find(|r| r.user_id as u64 == a).map(|r| r.value), Some(160));

    // A member can sit in voice for hours without a state change and keep earning...
    let before = sim.server_xp(partner).await;
    sim.app.shift_clock(chrono::Duration::hours(9));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!(sim.server_xp(partner).await, before + 20);
    assert_eq!(sim.server_xp(listener).await, 60);

    // ...but nobody earns on a record nothing refreshed for a day (a leave that was lost), and the
    // stale records are dropped.
    sim.app.shift_clock(chrono::Duration::hours(25));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!(sim.server_xp(partner).await, before + 20);
    assert!(sim.voice_members().await.is_empty());
}

/// Shifts a clock to half past an hour (and `hours` on), so two processes with clocks a second apart
/// still agree on which hour it is.
fn to_half_past(app: &App, hours: i64) {
    let into_hour = app.now().timestamp().rem_euclid(3600);
    app.shift_clock(chrono::Duration::seconds(1800 - into_hour + hours * 3600));
}

#[tokio::test]
async fn a_guild_create_snapshot_replaces_who_is_remembered_in_voice() {
    let Some(mut sim) = Sim::new("a_guild_create_snapshot_replaces_who_is_remembered_in_voice").await else {
        return;
    };
    let _turn = VOICE_TICKS.lock().await;
    sim.app.shift_clock(chrono::Duration::days((unique() % 36_500) as i64));
    let admin = unique();
    let (lounge, den, afk) = (unique(), unique(), unique());
    let (a, b, ghost, robot, stranger, sleeper1, sleeper2) =
        (unique(), unique(), unique(), unique(), unique(), unique(), unique());
    let booster_id = unique();
    let booster = sim.guild_role(booster_id, "Booster", 2);
    sim.permissions = MANAGE_GUILD;
    sim.xp(
        admin,
        sub("voice", vec![bool_option("enabled", true), int_option("amount", 20)]),
    )
    .await;
    sim.xp(
        admin,
        sub(
            "multiplier",
            vec![number_option("value", 3.0), role_option("role", &booster)],
        ),
    )
    .await;

    // What the bot remembered before it went away: a ghost who left meanwhile, and `a` in another
    // channel than where they are now.
    sim.voice_in(ghost, Some(lounge), &[], false, false, false).await;
    sim.voice_in(a, Some(den), &[], false, false, false).await;
    let mut before = vec![ghost, a];
    before.sort_unstable();
    assert_eq!(sim.voice_members().await, before);

    // The snapshot is the truth: it lists who is in voice now; its member list says who is a bot and
    // who holds which role. `stranger` has no member there, so they wait for their next voice event.
    // The bot was away for a moment: what it remembered was written before the snapshot was taken (a voice
    // event after the snapshot stays as the event left it).
    sim.app.shift_clock(chrono::Duration::seconds(1));
    let boosted: &[u64] = &[booster_id];
    sim.snapshot(
        &[
            (a, lounge, false, false),
            (b, lounge, false, false),
            (robot, lounge, false, false),
            (stranger, lounge, false, false),
            (sleeper1, afk, false, false),
            (sleeper2, afk, false, false),
        ],
        &[
            (a, false, boosted),
            (b, false, &[]),
            (robot, true, &[]),
            (sleeper1, false, &[]),
            (sleeper2, false, &[]),
        ],
        Some(afk),
    )
    .await;
    let mut now = vec![a, b, sleeper1, sleeper2];
    now.sort_unstable();
    assert_eq!(
        sim.voice_members().await,
        now,
        "the ghost is gone, the bot and the member-less are not remembered"
    );
    assert!(sim.ticks_voice().await);

    // The next minute pays from the snapshot: the booster role comes from its member list, and its
    // AFK channel (the mock server says there is none) keeps the sleepers from earning.
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!(sim.server_xp(a).await, 60, "x3 from the role the snapshot gave");
    assert_eq!(sim.server_xp(b).await, 20);
    for nobody in [ghost, robot, stranger, sleeper1, sleeper2] {
        assert_eq!(sim.server_xp(nobody).await, 0, "{nobody}");
    }

    // A server that is only unavailable says nothing about voice.
    let unavailable: GuildCreate =
        serde_json::from_value(json!({ "id": sim.guild.to_string(), "unavailable": true })).expect("guild JSON");
    on_guild_create(sim.app.clone(), unavailable).await.expect("handler");
    assert_eq!(sim.voice_members().await, now);

    // Leaves that happened while the bot was away disappear with the next snapshot...
    sim.app.shift_clock(chrono::Duration::seconds(1));
    sim.snapshot(&[(b, lounge, false, false)], &[(b, false, &[])], None)
        .await;
    assert_eq!(sim.voice_members().await, vec![b]);
    // ...all of them when the server is empty...
    sim.app.shift_clock(chrono::Duration::seconds(1));
    sim.snapshot(&[], &[], None).await;
    assert!(sim.voice_members().await.is_empty());
    assert!(
        !sim.ticks_voice().await,
        "nobody in voice: the ticker lets the server go"
    );
    // ...and losing the server forgets everyone.
    sim.voice_in(a, Some(lounge), &[], false, false, false).await;
    assert!(sim.ticks_voice().await);
    let gone: GuildDelete =
        serde_json::from_value(json!({ "id": sim.guild.to_string(), "unavailable": true })).expect("guild JSON");
    on_guild_delete(sim.app.clone(), gone).await.expect("handler");
    assert!(sim.voice_members().await.is_empty());
    assert!(!sim.ticks_voice().await);
}

#[tokio::test]
async fn servers_with_voice_xp_off_are_not_tracked_and_are_dropped_when_it_goes_off() {
    let Some(mut sim) = Sim::new("servers_with_voice_xp_off_are_not_tracked_and_are_dropped_when_it_goes_off").await
    else {
        return;
    };
    let _turn = VOICE_TICKS.lock().await;
    sim.app.shift_clock(chrono::Duration::days((unique() % 36_500) as i64));
    let admin = unique();
    let lounge = unique();
    let (a, b) = (unique(), unique());
    let mut both = vec![a, b];
    both.sort_unstable();
    sim.permissions = MANAGE_GUILD;

    // Off (the default): voice events and a GUILD_CREATE snapshot leave Redis alone.
    sim.voice_in(a, Some(lounge), &[], false, false, false).await;
    sim.voice_in(b, Some(lounge), &[], false, false, false).await;
    sim.snapshot(
        &[(a, lounge, false, false), (b, lounge, false, false)],
        &[(a, false, &[]), (b, false, &[])],
        None,
    )
    .await;
    assert!(sim.voice_members().await.is_empty());
    assert!(!sim.ticks_voice().await);

    // On: the same events are remembered, and the minute pays.
    let voice = |enabled: bool| sub("voice", vec![bool_option("enabled", enabled), int_option("amount", 20)]);
    sim.xp(admin, voice(true)).await;
    sim.voice_in(a, Some(lounge), &[], false, false, false).await;
    sim.voice_in(b, Some(lounge), &[], false, false, false).await;
    assert_eq!(sim.voice_members().await, both);
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!((sim.server_xp(a).await, sim.server_xp(b).await), (20, 20));

    // Leveling off pays nobody but forgets nobody either.
    sim.app
        .db
        .update_leveling(sim.guild, Some(false), None, None)
        .await
        .unwrap();
    sim.app.forget_guild_settings(sim.guild).await;
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!(sim.server_xp(a).await, 20);
    assert_eq!(sim.voice_members().await, both);
    sim.app
        .db
        .update_leveling(sim.guild, Some(true), None, None)
        .await
        .unwrap();
    sim.app.forget_guild_settings(sim.guild).await;
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!(
        sim.server_xp(a).await,
        40,
        "leveling back on, the same people earn again"
    );

    // Off again: the next tick pays nothing and drops the server and its records, so a later
    // "on" does not resurrect members who left in between.
    sim.xp(admin, voice(false)).await;
    assert!(
        sim.voice_members().await.is_empty(),
        "switching it off forgets everyone at once, so no pod with an older view pays"
    );
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!(sim.server_xp(a).await, 40);
    assert!(sim.voice_members().await.is_empty());
    assert!(!sim.ticks_voice().await);
}

#[tokio::test]
async fn a_failed_afk_lookup_never_pays_the_afk_channel() {
    let Some(mut sim) = Sim::new("a_failed_afk_lookup_never_pays_the_afk_channel").await else {
        return;
    };
    let _turn = VOICE_TICKS.lock().await;
    sim.app.shift_clock(chrono::Duration::days((unique() % 36_500) as i64));
    let admin = unique();
    let (lounge, afk) = (unique(), unique());
    let (p1, p2, s1, s2) = (unique(), unique(), unique(), unique());
    AFK_CHANNELS.lock().unwrap().insert(sim.guild, afk);
    sim.permissions = MANAGE_GUILD;
    sim.xp(
        admin,
        sub("voice", vec![bool_option("enabled", true), int_option("amount", 20)]),
    )
    .await;
    for (user, channel) in [(p1, lounge), (p2, lounge), (s1, afk), (s2, afk)] {
        sim.voice_in(user, Some(channel), &[], false, false, false).await;
    }

    // Discord is down and the AFK channel was never heard of: nobody in the server is paid, rather
    // than guessing there is none and paying the sleepers.
    sim.mock.guild_lookup_fails.store(true, Ordering::SeqCst);
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    for user in [p1, p2, s1, s2] {
        assert_eq!(sim.server_xp(user).await, 0, "{user}");
    }

    // Discord is back: the lounge earns, the AFK channel does not.
    sim.mock.guild_lookup_fails.store(false, Ordering::SeqCst);
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!((sim.server_xp(p1).await, sim.server_xp(p2).await), (20, 20));
    assert_eq!((sim.server_xp(s1).await, sim.server_xp(s2).await), (0, 0));

    // Down again once the fresh answer has expired: the last known AFK channel stands in.
    sim.app.cache.forget(&format!("voice-afk:{}", sim.guild)).await.unwrap();
    sim.mock.guild_lookup_fails.store(true, Ordering::SeqCst);
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!((sim.server_xp(p1).await, sim.server_xp(p2).await), (40, 40));
    assert_eq!((sim.server_xp(s1).await, sim.server_xp(s2).await), (0, 0));
    sim.mock.guild_lookup_fails.store(false, Ordering::SeqCst);
}

#[tokio::test]
async fn one_pod_prunes_old_weekly_xp_per_hour() {
    let Some(sim) = Sim::new("one_pod_prunes_old_weekly_xp_per_hour").await else {
        return;
    };
    let Some(other) = Sim::new("one_pod_prunes_old_weekly_xp_per_hour_other_pod").await else {
        return;
    };
    // The hour locks live two hours in Redis, and this test sends its clocks to hours ahead: the ones
    // earlier runs took would be found taken.
    let mut redis = sim.app.cache.conn();
    let taken: Vec<String> = redis::cmd("KEYS")
        .arg("pinchora:once:weekly-prune:*")
        .query_async(&mut redis)
        .await
        .expect("list the locks");
    if !taken.is_empty() {
        let _: i64 = redis::cmd("DEL")
            .arg(&taken)
            .query_async(&mut redis)
            .await
            .expect("forget the locks");
    }
    let guild = sim.guild;
    let (old_user, recent_user, later_user) = (unique(), unique(), unique());
    let this_week = pinchora_core::leveling::week_start(chrono::Utc::now());
    let grant = pinchora_db::XpGrant {
        server_xp: 10,
        global_xp: 0,
        points: 0,
    };
    let old_week = this_week - chrono::Duration::weeks(10);
    for (user, week) in [(old_user, old_week), (recent_user, this_week), (later_user, old_week)] {
        sim.app.db.add_xp(guild, user, grant, week).await.unwrap();
    }
    let rows = |week| {
        let db = sim.app.db.clone();
        async move { db.weekly_leaderboard(guild, week, 10, 0).await.unwrap().len() }
    };
    assert_eq!((rows(old_week).await, rows(this_week).await), (2, 1));

    // Both pods are in the same hour (some hours ahead, so a rerun finds a fresh lock): the first to
    // ask prunes, the second finds the hour taken.
    to_half_past(&sim.app, unique() as i64 % 24);
    to_half_past(&other.app, unique() as i64 % 24);
    let ahead = sim.app.now() - other.app.now();
    other.app.shift_clock(ahead);
    let first = loop {
        if let Some(removed) = pinchora_commands::voice_prune(&sim.app).await.expect("prune") {
            break removed;
        }
        // An earlier run took this hour.
        sim.app.shift_clock(chrono::Duration::hours(1));
        other.app.shift_clock(chrono::Duration::hours(1));
    };
    assert!(first >= 2, "both old rows go: {first}");
    assert_eq!(
        pinchora_commands::voice_prune(&other.app).await.expect("prune"),
        None,
        "another pod in the same hour leaves it alone"
    );
    assert_eq!((rows(old_week).await, rows(this_week).await), (0, 1));

    // The next hour it is somebody's turn again, and the other pod may be the one.
    sim.app.db.add_xp(guild, later_user, grant, old_week).await.unwrap();
    sim.app.shift_clock(chrono::Duration::hours(1));
    other.app.shift_clock(chrono::Duration::hours(1));
    assert!(
        pinchora_commands::voice_prune(&other.app)
            .await
            .expect("prune")
            .is_some_and(|removed| removed >= 1)
    );
    assert_eq!(pinchora_commands::voice_prune(&sim.app).await.expect("prune"), None);
    assert_eq!(rows(old_week).await, 0);
}

#[tokio::test]
async fn multipliers_reach_threads_forum_posts_and_channels_in_a_category() {
    let Some(mut sim) = Sim::new("multipliers_reach_threads_forum_posts_and_channels_in_a_category").await else {
        return;
    };
    let Some(mut plain) = Sim::new("multipliers_without_channel_rules_never_ask_discord").await else {
        return;
    };
    let _turn = VOICE_TICKS.lock().await;
    sim.app.shift_clock(chrono::Duration::days((unique() % 36_500) as i64));
    let admin = unique();
    let (category, text, thread, own_thread, sibling) = (unique(), unique(), unique(), unique(), unique());
    let (forum, post, loose, loose_thread, unknown) = (unique(), unique(), unique(), unique(), unique());
    let (hush, hush_child, hush_thread, voice, loose_voice) = (unique(), unique(), unique(), unique(), unique());
    sim.channel_is(category, 4, None);
    sim.channel_is(text, 0, Some(category));
    sim.channel_is(thread, 11, Some(text));
    sim.channel_is(own_thread, 11, Some(text));
    sim.channel_is(sibling, 0, Some(category));
    sim.channel_is(forum, 15, Some(category));
    sim.channel_is(post, 11, Some(forum));
    sim.channel_is(loose, 0, None);
    sim.channel_is(loose_thread, 11, Some(loose));
    sim.channel_is(hush, 4, None);
    sim.channel_is(hush_child, 0, Some(hush));
    sim.channel_is(hush_thread, 11, Some(hush_child));
    sim.channel_is(voice, 2, Some(category));
    sim.channel_is(loose_voice, 2, None);
    sim.permissions = MANAGE_GUILD;
    // The category doubles XP, `text` triples it, `own_thread` has a x5 of its own, `hush` earns nothing.
    for (channel, value) in [(category, 2.0), (text, 3.0), (own_thread, 5.0), (hush, 0.0)] {
        let reply = sim
            .command_resolved(
                admin,
                "config",
                json!([group(
                    "xp",
                    sub(
                        "multiplier",
                        vec![number_option("value", value), channel_option("channel", channel)]
                    )
                )]),
                resolved_channels(&[channel]),
            )
            .await;
        assert!(
            reply.content().contains(&format!("<#{channel}>")),
            "{}",
            reply.content()
        );
    }

    // (channel, what a message in it earns times): the nearest rule up the chain decides.
    for (channel, times, why) in [
        (text, 3, "its own rule beats its category"),
        (thread, 3, "a thread follows its channel"),
        (own_thread, 5, "a thread's own rule beats its channel's"),
        (sibling, 2, "a channel follows its category"),
        (post, 2, "a forum post follows its forum, which follows its category"),
        (loose, 1, "no rule anywhere"),
        (loose_thread, 1, "no rule anywhere, thread or not"),
        (unknown, 1, "a channel Discord does not know is just itself"),
        (hush_child, 0, "a no-XP category"),
        (hush_thread, 0, "a no-XP category reaches threads in its channels"),
    ] {
        let user = unique();
        sim.say_in(user, channel, &[]).await;
        let global = sim.global_xp(user).await;
        assert!((15..=25).contains(&global), "{why}: {global}");
        assert_eq!(sim.server_xp(user).await, global * times, "{why}");
    }

    // Discord is asked once per channel, and not at all where the channel has a rule of its own.
    for _ in 0..3 {
        sim.say_in(unique(), thread, &[]).await;
    }
    assert_eq!(sim.channel_lookups(thread), 1);
    assert_eq!(sim.channel_lookups(text), 0, "its own rule: nothing to look up");
    assert_eq!((sim.channel_lookups(post), sim.channel_lookups(forum)), (1, 1));
    assert_eq!(sim.channel_lookups(category), 0, "a category's parent is never asked");

    // Voice follows the category too: the lounge with a category earns double, one without does not.
    let (v1, v2, w1, w2) = (unique(), unique(), unique(), unique());
    sim.xp(
        admin,
        sub("voice", vec![bool_option("enabled", true), int_option("amount", 20)]),
    )
    .await;
    for (user, channel) in [(v1, voice), (v2, voice), (w1, loose_voice), (w2, loose_voice)] {
        sim.voice_in(user, Some(channel), &[], false, false, false).await;
    }
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!((sim.server_xp(v1).await, sim.server_xp(v2).await), (40, 40));
    assert_eq!((sim.server_xp(w1).await, sim.server_xp(w2).await), (20, 20));

    // A server with role rules only never asks Discord about channels.
    let everyone = role_json(plain.guild, "@everyone", 0, false);
    plain.permissions = MANAGE_GUILD;
    plain
        .xp(
            admin,
            sub(
                "multiplier",
                vec![number_option("value", 2.0), role_option("role", &everyone)],
            ),
        )
        .await;
    plain.channel_is(text, 0, Some(category));
    let user = unique();
    plain.say_in(user, text, &[]).await;
    assert_eq!(plain.server_xp(user).await, plain.global_xp(user).await * 2);
    assert_eq!(plain.channel_lookups(text), 0);
}

#[tokio::test]
async fn level_ups_follow_the_mode_and_fill_their_placeholders() {
    let Some(mut sim) = Sim::new("level_ups_follow_the_mode_and_fill_their_placeholders").await else {
        return;
    };
    let admin = unique();
    // The owner may set up any role the bot can hand out.
    sim.own_server(admin);
    let (announce, chatty) = (unique(), unique());
    let novice_id = unique();
    let novice = sim.guild_role(novice_id, "Reef Novice", 2);
    sim.permissions = MANAGE_GUILD;
    let level_up = |sim: &Sim, user: u64| {
        let (db, guild) = (sim.app.db.clone(), sim.guild);
        async move {
            db.add_message_xp(guild, user, 99, 0).await.unwrap();
        }
    };

    // The default message when nothing is set: where the member chatted, with no {role} to fill.
    let first = unique();
    level_up(&sim, first).await;
    sim.say_in(first, chatty, &[]).await;
    let posts = sim.posts();
    assert!(
        in_channel(&posts, chatty)
            .iter()
            .any(|p| p.content().contains(&format!("<@{first}> reached **level 1**"))),
        "the default message, in the channel they chatted in"
    );

    // Placeholders: {user} {level} {server} {role}; with no reward role the {role} is empty.
    let template = "GG {user}: level {level} in {server}! New role: [{role}]";
    let reply = sim
        .command_resolved(
            admin,
            "config",
            json!([sub(
                "leveling",
                vec![text_option("message", template), channel_option("channel", announce)]
            )]),
            resolved_channels(&[announce]),
        )
        .await;
    assert!(
        reply.field("Leveling").contains(&format!("in <#{announce}>")),
        "{}",
        reply.field("Leveling")
    );
    let second = unique();
    level_up(&sim, second).await;
    sim.say_in(second, chatty, &[]).await;
    let posts = sim.posts();
    let text = in_channel(&posts, announce)
        .first()
        .map(|p| p.content().to_owned())
        .expect("a post in the level-up channel");
    assert_eq!(text, format!("GG <@{second}>: level 1 in {GUILD_NAME}! New role: []"));

    // With a reward role, {role} is the role they just earned; the role itself is given first.
    sim.command_resolved(
        admin,
        "config",
        json!([group(
            "level-roles",
            sub("add", vec![int_option("level", 1), role_option("role", &novice)])
        )]),
        resolved(&[], &[&novice]),
    )
    .await;
    let third = unique();
    level_up(&sim, third).await;
    sim.say_in(third, chatty, &[]).await;
    assert_eq!(sim.role_changes(), vec![format!("PUT {third} {novice_id}")]);
    let posts = sim.posts();
    let text = in_channel(&posts, announce)
        .first()
        .map(|p| p.content().to_owned())
        .expect("a post");
    assert_eq!(
        text,
        format!("GG <@{third}>: level 1 in {GUILD_NAME}! New role: [<@&{novice_id}>]")
    );

    // Mode `current`: always where they chatted, whatever channel is set.
    sim.command(
        admin,
        "config",
        json!([sub("leveling", vec![text_option("mode", "current")])]),
    )
    .await;
    let fourth = unique();
    level_up(&sim, fourth).await;
    sim.say_in(fourth, chatty, &[]).await;
    let posts = sim.posts();
    assert_eq!(in_channel(&posts, chatty).len(), 1, "posted where they chatted");
    assert!(in_channel(&posts, announce).is_empty());
    sim.role_changes();

    // Mode `dm`: a direct message, with the role's name, since a mention does not render there.
    sim.command(
        admin,
        "config",
        json!([sub("leveling", vec![text_option("mode", "dm")])]),
    )
    .await;
    let fifth = unique();
    level_up(&sim, fifth).await;
    sim.say_in(fifth, chatty, &[]).await;
    let posts = sim.posts();
    assert!(in_channel(&posts, chatty).is_empty() && in_channel(&posts, announce).is_empty());
    let dm = in_channel(&posts, fifth);
    assert_eq!(dm.len(), 1, "one direct message");
    assert_eq!(
        dm[0].content(),
        format!("GG <@{fifth}>: level 1 in {GUILD_NAME}! New role: [Reef Novice]")
    );
    assert_eq!(sim.role_changes(), vec![format!("PUT {fifth} {novice_id}")]);

    // Closed direct messages are not an error, and the level still counts.
    let sixth = unique();
    DM_OVERRIDES.lock().unwrap().insert(sixth, true);
    level_up(&sim, sixth).await;
    sim.say_in(sixth, chatty, &[]).await;
    DM_OVERRIDES.lock().unwrap().remove(&sixth);
    assert!(sim.posts().is_empty());
    assert!(sim.server_xp(sixth).await >= 100);
    assert_eq!(
        sim.role_changes(),
        vec![format!("PUT {sixth} {novice_id}")],
        "the role does not depend on the DM"
    );

    // Mode `off`: no message anywhere, the reward role still arrives.
    sim.command(
        admin,
        "config",
        json!([sub("leveling", vec![text_option("mode", "off")])]),
    )
    .await;
    let seventh = unique();
    level_up(&sim, seventh).await;
    sim.say_in(seventh, chatty, &[]).await;
    assert!(sim.posts().is_empty());
    assert_eq!(sim.role_changes(), vec![format!("PUT {seventh} {novice_id}")]);

    // Back to `channel`: the level-up channel again.
    sim.command(
        admin,
        "config",
        json!([sub("leveling", vec![text_option("mode", "channel")])]),
    )
    .await;
    let eighth = unique();
    level_up(&sim, eighth).await;
    sim.say_in(eighth, chatty, &[]).await;
    let posts = sim.posts();
    assert_eq!(in_channel(&posts, announce).len(), 1);
}

#[tokio::test]
async fn the_weekly_board_counts_this_week_and_shows_last_weeks_podium() {
    let Some(mut sim) = Sim::new("the_weekly_board_counts_this_week_and_shows_last_weeks_podium").await else {
        return;
    };
    let (chatter, early, mid, late, fourth) = (unique(), unique(), unique(), unique(), unique());
    let week = pinchora_core::leveling::week_start(sim.app.now());
    let grant = |xp| pinchora_db::XpGrant {
        server_xp: xp,
        global_xp: 0,
        points: 0,
    };

    // This week: one chat tick, one big grant.
    sim.say(chatter, &[]).await;
    sim.app.db.add_xp(sim.guild, mid, grant(900), week).await.unwrap();
    let all_time = sim.app.db.guild_leaderboard(sim.guild, 10, 0).await.unwrap();
    assert_eq!(all_time[0].user_id as u64, mid);

    // Nobody earned last week yet: no podium, the usual card size.
    let board = sim
        .command(chatter, "leaderboard", json!([text_option("board", "weekly")]))
        .await;
    assert_eq!(board.files.len(), 1);
    assert_eq!(png_height(&board), 716, "no podium without a last week");
    assert!(board.button("▶").disabled && board.button("◀").disabled);

    // Last week's results (stored under last week's key) show up under the board.
    let last = week - chrono::Duration::weeks(1);
    for (user, xp) in [(early, 5_200), (late, 800), (fourth, 90), (mid, 3_100)] {
        sim.app.db.add_xp(sim.guild, user, grant(xp), last).await.unwrap();
    }
    let board = sim
        .command(chatter, "leaderboard", json!([text_option("board", "weekly")]))
        .await;
    assert_eq!(png_height(&board), 826, "the podium adds a panel");
    // Its page buttons keep the weekly board.
    let id = board.button("◀").custom_id.clone().unwrap();
    assert_eq!(id, "lb:weekly:0");
    let again = sim.component(chatter, &id, &[], &board).await;
    assert_eq!(png_height(&again), 826);

    // The server board is untouched by weeks.
    let server = sim.command(chatter, "leaderboard", json!([])).await;
    assert_eq!(png_height(&server), 716);

    // A week later, this week's results are last week's: the podium shows them, the board starts again.
    sim.app.shift_clock(chrono::Duration::weeks(1));
    let next = sim
        .command(chatter, "leaderboard", json!([text_option("board", "weekly")]))
        .await;
    assert_eq!(png_height(&next), 826);
    let new_week = pinchora_core::leveling::week_start(sim.app.now());
    assert_eq!(new_week, week + chrono::Duration::weeks(1));
    assert!(
        sim.app
            .db
            .weekly_leaderboard(sim.guild, new_week, 10, 0)
            .await
            .unwrap()
            .is_empty()
    );
    let podium = sim.app.db.weekly_leaderboard(sim.guild, week, 3, 0).await.unwrap();
    assert_eq!(
        podium.iter().map(|r| r.user_id as u64).collect::<Vec<_>>(),
        vec![mid, chatter]
    );
}

/// `/rep target` from `from`, where the target holds `held` here.
async fn give_rep(sim: &mut Sim, from: u64, target: u64, held: &[u64]) -> Reply {
    let options = json!([user_option("user", target)]);
    sim.command_resolved(from, "rep", options, resolved_member(target, held))
        .await
}

#[tokio::test]
async fn reputation_roles_are_given_when_a_point_lands() {
    let Some(mut sim) = Sim::new("reputation_roles_are_given_when_a_point_lands").await else {
        return;
    };
    let admin = unique();
    // The owner may set up any role the bot can hand out.
    sim.own_server(admin);
    let (target, g1, g2, g3, g4, g5) = (unique(), unique(), unique(), unique(), unique(), unique());
    let (trusted_id, legend_id, gone_id) = (unique(), unique(), unique());
    let trusted = sim.guild_role(trusted_id, "Trusted", 2);
    let legend = sim.guild_role(legend_id, "Legend", 3);
    let gone = sim.guild_role(gone_id, "Doomed", 4);
    sim.permissions = MANAGE_GUILD;
    let add = |reputation: i64, role: &Value| {
        json!([group(
            "rep-roles",
            sub(
                "add",
                vec![int_option("reputation", reputation), role_option("role", role)]
            )
        )])
    };

    // The same safety checks as level roles.
    let above = role_json(unique(), "Boss", BOT_ROLE_POSITION + 5, false);
    let reply = sim
        .command_resolved(admin, "config", add(3, &above), resolved(&[], &[&above]))
        .await;
    assert!(reply.content().contains("above my highest role"), "{}", reply.content());
    let managed = role_json(unique(), "SomeBot", 1, true);
    let reply = sim
        .command_resolved(admin, "config", add(3, &managed), resolved(&[], &[&managed]))
        .await;
    assert!(
        reply.content().contains("managed by an integration"),
        "{}",
        reply.content()
    );
    let everyone = role_json(sim.guild, "@everyone", 0, false);
    let reply = sim
        .command_resolved(admin, "config", add(3, &everyone), resolved(&[], &[&everyone]))
        .await;
    assert!(reply.content().contains("@everyone"), "{}", reply.content());
    let empty = sim
        .command(admin, "config", json!([group("rep-roles", sub("list", vec![]))]))
        .await;
    assert!(
        empty.content().contains("No reputation roles yet"),
        "{}",
        empty.content()
    );

    for (reputation, role) in [(3, &trusted), (5, &legend), (4, &gone)] {
        let reply = sim
            .command_resolved(admin, "config", add(reputation, role), resolved(&[], &[role]))
            .await;
        assert!(
            reply.content().contains(&format!("**{reputation}** reputation")),
            "{}",
            reply.content()
        );
    }
    let list = sim
        .command(admin, "config", json!([group("rep-roles", sub("list", vec![]))]))
        .await;
    assert_eq!(
        list.description(),
        format!(
            "**3** reputation · <@&{trusted_id}>\n**4** reputation · <@&{gone_id}>\n**5** reputation · <@&{legend_id}>"
        )
    );

    sqlx::query("INSERT INTO users (id, reputation) VALUES ($1, 1) ON CONFLICT (id) DO UPDATE SET reputation = 1")
        .bind(target as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();

    // 1 -> 2: no role yet. 2 -> 3: Trusted, announced under the point.
    let reply = give_rep(&mut sim, g1, target, &[]).await;
    assert!(
        reply.description().contains("They now have **2**") && !reply.description().contains("earned"),
        "{}",
        reply.description()
    );
    assert!(sim.role_changes().is_empty());
    let reply = give_rep(&mut sim, g2, target, &[]).await;
    assert!(
        reply.description().contains("They now have **3**"),
        "{}",
        reply.description()
    );
    assert!(
        reply.description().contains(&format!("earned <@&{trusted_id}>!")),
        "{}",
        reply.description()
    );
    assert_eq!(sim.role_changes(), vec![format!("PUT {target} {trusted_id}")]);

    // 3 -> 4: Doomed, a role somebody deletes; the point still lands and the role leaves the list.
    sim.delete_role(gone_id);
    let reply = give_rep(&mut sim, g3, target, &[trusted_id]).await;
    assert!(
        reply.description().contains("They now have **4**") && !reply.description().contains("earned"),
        "{}",
        reply.description()
    );
    assert!(sim.role_changes().is_empty());
    assert_eq!(
        sim.app.db.rep_roles(sim.guild).await.unwrap().len(),
        2,
        "the deleted role is forgotten"
    );

    // 4 -> 5: Legend, while Trusted (already held) is not given twice. Discord refusing a role does
    // not stop the point.
    sim.allow_role_changes(false);
    let reply = give_rep(&mut sim, g4, target, &[trusted_id]).await;
    assert!(
        reply.description().contains("They now have **5**") && !reply.description().contains("earned"),
        "{}",
        reply.description()
    );
    sim.allow_role_changes(true);
    let reply = give_rep(&mut sim, g5, target, &[trusted_id]).await;
    assert!(
        reply.description().contains(&format!("earned <@&{legend_id}>!")),
        "{}",
        reply.description()
    );
    assert!(!reply.description().contains(&format!("earned <@&{trusted_id}>")));
    assert_eq!(sim.role_changes(), vec![format!("PUT {target} {legend_id}")]);

    // Removing a reputation role stops it being given.
    let removed = sim
        .command(
            admin,
            "config",
            json!([group("rep-roles", sub("remove", vec![role_option("role", &legend)]))]),
        )
        .await;
    assert!(
        removed.content().contains("no longer a reputation role"),
        "{}",
        removed.content()
    );
    let removed = sim
        .command(
            admin,
            "config",
            json!([group("rep-roles", sub("remove", vec![role_option("role", &legend)]))]),
        )
        .await;
    assert!(
        removed.content().contains("isn't a reputation role"),
        "{}",
        removed.content()
    );
    let _ = gone;
}

// /Pets

fn pet_command(sub: &str) -> Value {
    json!([{ "name": sub, "type": 1 }])
}

/// `/pet trip go destination [pet]`.
fn trip_go(destination: &str, pet: Option<&str>) -> Value {
    let mut options = vec![json!({ "name": "destination", "type": 3, "value": destination })];
    if let Some(pet) = pet {
        options.push(json!({ "name": "pet", "type": 3, "value": pet }));
    }
    json!([{ "name": "trip", "type": 2, "options": [{ "name": "go", "type": 1, "options": options }] }])
}

/// `/pet trip claim`.
fn trip_claim() -> Value {
    json!([{ "name": "trip", "type": 2, "options": [{ "name": "claim", "type": 1 }] }])
}

async fn pet_row(db: &Db, user: u64, id: i64) -> pinchora_db::PetRow {
    db.pets(user)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.id == id)
        .expect("the pet")
}

fn assert_png(reply: &Reply, width: u32, height: u32) {
    let (name, bytes) = reply.files.first().expect("a picture");
    let image = image::load_from_memory(bytes).unwrap_or_else(|e| panic!("{name} is not a png: {e}"));
    assert_eq!((image.width(), image.height()), (width, height), "{name}");
}

/// Stores needs, level and XP on a pet the way a care write does, from the row as it is stored now.
async fn set_pet(
    db: &Db,
    user: u64,
    pet_id: i64,
    needs: [i16; 4],
    level: i32,
    xp: i32,
    at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<()> {
    let seen = pet_row(db, user, pet_id).await;
    anyhow::ensure!(
        db.save_pet(user, &seen, needs, level, xp, at).await?,
        "the pet could not be saved"
    );
    Ok(())
}

/// Care costs XP-free needs; a ready pet with every need at `v`.
fn fresh_needs(v: i16) -> [i16; 4] {
    [v, v, v, v]
}

#[tokio::test]
async fn pets_level_up_and_the_top_level_stops_earning() {
    let Some(mut sim) = Sim::new("pets_level_up_and_the_top_level_stops_earning").await else {
        return;
    };
    let (user, db, app) = (unique(), sim.app.db.clone(), sim.app.clone());
    let pet = db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap();

    // A new pet: level 1, care buttons ready, the second row has a trip and the nursery.
    let card = sim.command(user, "pet", pet_command("view")).await;
    assert_png(&card, 860, 420);
    assert!(!card.button("Play").disabled && !card.button("Bath").disabled);
    card.button("Trip");
    card.button("Nursery");
    assert!(
        card.controls()
            .all(|c| !matches!(c, Component::Button(b) if b.label.as_deref() == Some("Make active"))),
        "the active pet needs no Make active"
    );

    // Level 3 → 4 on a Play, which opens the next trip.
    set_pet(&db, user, pet.id, fresh_needs(80), 3, 178, app.now())
        .await
        .unwrap();
    let played = sim.click(user, &card, "Play").await;
    assert!(played.description().contains("+15 XP"), "{}", played.description());
    assert!(
        played.description().contains("Pip reached level 4!"),
        "{}",
        played.description()
    );
    assert!(
        played.description().contains("New trip unlocked: **Driftwood Cove**"),
        "{}",
        played.description()
    );
    assert_png(&played, 860, 420);
    let row = pet_row(&db, user, pet.id).await;
    assert_eq!((row.level, row.xp), (4, 13), "178 + 15 - 180 carried into level 4");
    // A new player is told about their first badge under the reply.
    assert_eq!(played.followups.len(), 1);
    assert_eq!(played.followups[0].title(), "Badge unlocked: Pawshake");

    // Walks find more at a higher level: level 20 pays 25 to 70 shells.
    set_pet(&db, user, pet.id, fresh_needs(80), 20, 0, app.now())
        .await
        .unwrap();
    let before = db.user(user).await.unwrap().credits;
    let walked = sim.click(user, &played, "Walk").await;
    let after = db.user(user).await.unwrap().credits;
    assert!((25..=70).contains(&(after - before)), "found {}", after - before);
    assert!(
        walked
            .description()
            .contains(&format!("Found **{}** shells", after - before)),
        "{}",
        walked.description()
    );

    // Level 29 with the next level five XP away; a bath is worth ten.
    set_pet(&db, user, pet.id, fresh_needs(40), 29, 1_215, app.now())
        .await
        .unwrap();
    let bathed = sim.click(user, &walked, "Bath").await;
    assert!(
        bathed.description().contains("Pip reached level 30!"),
        "{}",
        bathed.description()
    );
    let row = pet_row(&db, user, pet.id).await;
    assert_eq!((row.level, row.xp), (30, 0), "the top level holds no XP");

    // At the top there is nothing to earn: no XP line, no level-up, the bar stays full.
    let fed = sim.click(user, &bathed, "Feed (20 shells)").await;
    assert!(!fed.description().contains("XP"), "{}", fed.description());
    assert!(!fed.description().contains("reached"), "{}", fed.description());
    assert_png(&fed, 860, 420);
    let row = pet_row(&db, user, pet.id).await;
    assert_eq!((row.level, row.xp), (30, 0));

    // An unknown action is an error, not a bath (it used to be one).
    set_pet(&db, user, pet.id, [60, 60, 60, 30], 30, 0, app.now())
        .await
        .unwrap();
    let odd = sim.component(user, &format!("pc:rub:{}", pet.id), &[], &fed).await;
    assert!(
        odd.ephemeral && odd.content().contains("doesn't do anything"),
        "{}",
        odd.content()
    );
    let none = sim.component(user, "pc", &[], &fed).await;
    assert!(none.content().contains("doesn't do anything"), "{}", none.content());
    let row = pet_row(&db, user, pet.id).await;
    assert_eq!(row.hygiene, 30, "nothing was washed");

    // Somebody else's pet is not yours to look after.
    let stranger = unique();
    let theirs = sim.component(stranger, &format!("pc:play:{}", pet.id), &[], &fed).await;
    assert!(theirs.content().contains("not your pet"), "{}", theirs.content());
}

#[tokio::test]
async fn a_sleeping_pet_lies_in_the_house_instead_of_wandering() {
    let Some(mut sim) = Sim::new("a_sleeping_pet_lies_in_the_house_instead_of_wandering").await else {
        return;
    };
    let (user, db, app) = (unique(), sim.app.db.clone(), sim.app.clone());
    let view = || json!([{ "name": "view", "type": 1 }]);
    let (pip, bub, nod) = (
        db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap(),
        db.adopt(user, "ribbonfin_guppy", "Bub").await.unwrap(),
        db.adopt(user, "noodlenose_seahorse", "Nod").await.unwrap(),
    );
    set_pet(&db, user, pip.id, fresh_needs(100), 1, 0, app.now())
        .await
        .unwrap();
    // Bub (a wide one) and Nod (a tall one) have been left alone for days: they fell asleep, so they
    // lie in the house with a "z z" while Pip wanders.
    for pet in [&bub, &nod] {
        set_pet(
            &db,
            user,
            pet.id,
            [0, 0, 100, 0],
            4,
            10,
            app.now() - chrono::Duration::days(3),
        )
        .await
        .unwrap();
    }
    let asleep = sim.command(user, "house", view()).await;
    assert_eq!(asleep.files.len(), 1, "the room picture");

    // Woken, they are up and about again, which is drawn differently.
    for pet in [&bub, &nod] {
        set_pet(&db, user, pet.id, fresh_needs(100), 4, 10, app.now())
            .await
            .unwrap();
    }
    let awake = sim.command(user, "house", view()).await;
    assert_ne!(
        asleep.files[0].1, awake.files[0].1,
        "a sleeper is not drawn like a wanderer"
    );
}

#[tokio::test]
async fn a_neglected_pet_falls_asleep_and_waking_it_is_free_once_a_day() {
    let Some(mut sim) = Sim::new("a_neglected_pet_falls_asleep_and_waking_it_is_free_once_a_day").await else {
        return;
    };
    let (user, db, app) = (unique(), sim.app.db.clone(), sim.app.clone());
    let pet = db.adopt(user, "ribbonfin_guppy", "Bub").await.unwrap();

    // Everything has run dry: a day later, it is asleep. Until then it only looks grumpy.
    set_pet(&db, user, pet.id, [0, 0, 100, 0], 5, 40, app.now())
        .await
        .unwrap();
    app.shift_clock(chrono::Duration::hours(23));
    let drowsy = sim.command(user, "pet", pet_command("view")).await;
    assert!(!drowsy.button("Play").disabled, "23 hours is not a day yet");
    assert!(
        drowsy.controls().all(
            |c| !matches!(c, Component::Button(b) if b.label.as_deref().is_some_and(|l| l.starts_with("Wake up")))
        ),
        "nothing to wake up yet"
    );
    app.shift_clock(chrono::Duration::hours(2));
    let asleep = sim.command(user, "pet", pet_command("view")).await;
    assert_png(&asleep, 860, 420);
    assert!(asleep.description().contains("dozed off"), "{}", asleep.description());
    for label in ["Play", "Walk", "Bath", "Feed (20 shells)"] {
        assert!(asleep.button(label).disabled, "{label} is for a pet that is up");
    }
    assert!(asleep.button("Wake up (free today)").custom_id.is_some());
    assert!(
        asleep
            .controls()
            .all(|c| !matches!(c, Component::Button(b) if b.label.as_deref() == Some("Trip"))),
        "a sleeper takes no trips"
    );

    // An old message's care button does nothing for a sleeper, and earns no XP.
    let nudge = sim.component(user, &format!("pc:play:{}", pet.id), &[], &asleep).await;
    assert!(
        nudge.ephemeral && nudge.content().contains("fast asleep"),
        "{}",
        nudge.content()
    );
    let trip = sim.command(user, "pet", trip_go("tidepool_meadow", None)).await;
    assert!(
        trip.ephemeral && trip.content().contains("fast asleep"),
        "{}",
        trip.content()
    );
    let row = pet_row(&db, user, pet.id).await;
    assert_eq!((row.level, row.xp), (5, 40));
    // The nursery shows it asleep.
    let nursery = sim.command(user, "pet", pet_command("list")).await;
    assert!(nursery.description().contains("1 asleep"), "{}", nursery.description());
    assert_png(&nursery, 1000, 352);

    // The day's first wake-up is free. The pet is up, a bit hungry, and rested.
    let woke = sim.click(user, &asleep, "Wake up (free today)").await;
    assert!(woke.description().contains("free wake-up"), "{}", woke.description());
    assert!(!woke.button("Play").disabled && !woke.button("Feed (20 shells)").disabled);
    assert_png(&woke, 860, 420);
    let row = pet_row(&db, user, pet.id).await;
    assert_eq!((row.hunger, row.fun, row.energy, row.hygiene), (50, 50, 100, 50));
    assert_eq!(db.user(user).await.unwrap().credits, 0, "free means free");
    // Waking it a second time from the same old message does nothing.
    let again = sim.component(user, &format!("pc:wake:{}", pet.id), &[], &asleep).await;
    assert!(again.content().contains("already awake"), "{}", again.content());

    // It dozes off again the same day: a second wake-up costs 75 shells, which the player lacks.
    set_pet(
        &db,
        user,
        pet.id,
        [0, 0, 100, 0],
        5,
        40,
        app.now() - chrono::Duration::hours(30),
    )
    .await
    .unwrap();
    let again_asleep = sim.command(user, "pet", pet_command("view")).await;
    let fee = again_asleep.button("Wake up (75 shells)").clone();
    assert!(fee.custom_id.is_some());
    let broke = sim.click(user, &again_asleep, "Wake up (75 shells)").await;
    assert!(
        broke.ephemeral && broke.content().contains("free wake-up for today is used"),
        "{}",
        broke.content()
    );
    assert!(
        broke.content().contains("can sleep as long as it likes"),
        "never permanent trouble"
    );
    db.earn_shells(user, 100, "test").await.unwrap();
    let paid = sim.click(user, &again_asleep, "Wake up (75 shells)").await;
    assert!(
        paid.description().contains("cost 75 shells (25 left)"),
        "{}",
        paid.description()
    );
    assert_eq!(db.user(user).await.unwrap().credits, 25);
    assert!(!paid.button("Play").disabled);

    // Tomorrow the free wake-up is back.
    set_pet(
        &db,
        user,
        pet.id,
        [0, 0, 100, 0],
        5,
        40,
        app.now() - chrono::Duration::hours(30),
    )
    .await
    .unwrap();
    app.shift_clock(chrono::Duration::hours(24));
    let next_day = sim.command(user, "pet", pet_command("view")).await;
    let free = sim.click(user, &next_day, "Wake up (free today)").await;
    assert!(free.description().contains("free wake-up"), "{}", free.description());
    assert_eq!(db.user(user).await.unwrap().credits, 25);
}

#[tokio::test]
async fn a_pet_goes_on_a_trip_and_its_souvenir_is_claimed_once() {
    let Some(mut sim) = Sim::new("a_pet_goes_on_a_trip_and_its_souvenir_is_claimed_once").await else {
        return;
    };
    let began = std::time::Instant::now();
    let (user, db, app) = (unique(), sim.app.db.clone(), sim.app.clone());
    let pet = db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap();
    let second = db.adopt(user, "ribbonfin_guppy", "Bub").await.unwrap();

    // A new player's first care click seeds the badge notices (their first badge is announced).
    let card = sim.command(user, "pet", pet_command("view")).await;
    let cared = sim.click(user, &card, "Play").await;
    assert_eq!(cared.followups.len(), 1);

    // Places open with the pet's level.
    let locked = sim.command(user, "pet", trip_go("driftwood_cove", None)).await;
    assert!(locked.ephemeral, "a refusal is private");
    assert!(
        locked
            .content()
            .contains("**Driftwood Cove** opens when Pip reaches level 4"),
        "{}",
        locked.content()
    );
    let nobody = sim
        .command(user, "pet", trip_go("tidepool_meadow", Some("Nobody")))
        .await;
    assert!(
        nobody.content().contains("None of your pets is called **Nobody**"),
        "{}",
        nobody.content()
    );

    // The Trip button lists the places and offers a menu; the locked ones say what they need.
    let picker = sim.click(user, &cared, "Trip").await;
    assert!(picker.title().contains("Where should Pip go?"), "{}", picker.title());
    assert!(
        picker.description().contains("Tidepool Meadow"),
        "{}",
        picker.description()
    );
    assert!(
        picker.description().contains("🔒 **Kelp Forest Library**"),
        "{}",
        picker.description()
    );
    assert_eq!(picker.options("py:s:").len(), 8);
    assert_png(&picker, 860, 420);
    let stay = sim.click(user, &picker, "Stay home").await;
    stay.button("Trip");

    // Off to the first meadow, by the menu.
    let gone = sim.pick(user, &picker, "py:s:", "tidepool_meadow").await;
    assert!(
        gone.description()
            .contains("sets off for **Tidepool Meadow**. Back in 1 h."),
        "{}",
        gone.description()
    );
    assert_png(&gone, 860, 420);
    for label in ["Play", "Walk", "Bath", "Feed (20 shells)"] {
        assert!(gone.button(label).disabled, "{label}");
    }
    // The countdown button is the hour, less what the test has used since the pet left.
    let countdown = gone
        .controls()
        .find_map(|c| match c {
            Component::Button(b) if b.label.as_deref().is_some_and(|l| l.starts_with("Back in ")) => {
                Some((b.label.clone().unwrap_or_default(), b.disabled))
            }
            _ => None,
        })
        .expect("a countdown button");
    assert!(countdown.1, "the countdown is not a button to press");
    assert_counting_down(&countdown.0, "Back in ", 3600, began);
    let row = pet_row(&db, user, pet.id).await;
    assert_eq!(row.trip_dest.as_deref(), Some("tidepool_meadow"));

    // While it is away it cannot be looked after, sent again, or claimed.
    let care = sim.component(user, &format!("pc:walk:{}", pet.id), &[], &gone).await;
    assert!(
        care.ephemeral && care.content().contains("is away at Tidepool Meadow"),
        "{}",
        care.content()
    );
    let twice = sim.command(user, "pet", trip_go("tidepool_meadow", Some("pip"))).await;
    assert!(
        twice.content().contains("already away at **Tidepool Meadow**"),
        "{}",
        twice.content()
    );
    let early = sim.command(user, "pet", trip_claim()).await;
    assert!(
        early.ephemeral && early.content().contains("**Pip** is at Tidepool Meadow, back in "),
        "{}",
        early.content()
    );
    assert_counting_down(early.content(), "back in ", 3600, began);
    let impatient = sim.component(user, &format!("py:cl:{}", pet.id), &[], &gone).await;
    assert!(
        impatient.ephemeral && impatient.content().contains("still at Tidepool Meadow"),
        "{}",
        impatient.content()
    );

    // Its needs wait for it. The other pet can go too, and the nursery shows who is away.
    let bub = sim.command(user, "pet", trip_go("tidepool_meadow", Some("bub"))).await;
    assert!(
        bub.description().contains("**Bub** packs a tiny bag"),
        "{}",
        bub.description()
    );
    let nursery = sim.command(user, "pet", pet_command("list")).await;
    assert!(nursery.description().contains("2 away"), "{}", nursery.description());
    assert_png(&nursery, 1000, 352);

    // An hour on, both are back and the souvenirs wait. Care works again; a new trip waits for the claim.
    app.shift_clock(chrono::Duration::minutes(61));
    let back = sim.command(user, "pet", pet_command("view")).await;
    assert!(
        back.description()
            .contains("is back from **Tidepool Meadow** with a souvenir"),
        "{}",
        back.description()
    );
    assert_png(&back, 860, 420);
    assert!(!back.button("Play").disabled);
    back.button("Claim souvenir");
    let busy = sim.command(user, "pet", trip_go("tidepool_meadow", Some("Pip"))).await;
    assert!(busy.content().contains("Claim it first"), "{}", busy.content());

    // Claim: shells, XP and a story, once. The badge for 10,000 shells follows.
    db.earn_shells(user, 9_980, "test").await.unwrap();
    let before = db.user(user).await.unwrap().credits;
    let claimed = sim.click(user, &back, "Claim souvenir").await;
    let after = db.user(user).await.unwrap().credits;
    assert!((25..=45).contains(&(after - before)), "found {}", after - before);
    assert!(
        claimed
            .description()
            .contains("**Pip** is back from **Tidepool Meadow**!"),
        "{}",
        claimed.description()
    );
    assert!(
        claimed
            .description()
            .contains(&format!("**+{}** shells", after - before)),
        "{}",
        claimed.description()
    );
    assert!(
        claimed.description().contains("**+25** XP"),
        "{}",
        claimed.description()
    );
    assert_png(&claimed, 860, 420);
    claimed.button("Trip");
    let row = pet_row(&db, user, pet.id).await;
    assert_eq!(
        (row.trip_dest.clone(), row.level, row.xp),
        (None, 1, 15 + 25),
        "the care XP and the trip's"
    );
    assert_eq!(claimed.followups.len(), 1);
    assert_eq!(claimed.followups[0].title(), "Badge unlocked: Pocket Rattle");

    // Claiming again, from the old message or the command, pays nothing.
    let stale = sim.click(user, &back, "Claim souvenir").await;
    assert!(
        stale.description().contains("no souvenir waiting"),
        "{}",
        stale.description()
    );
    assert_eq!(db.user(user).await.unwrap().credits, after);
    // The command collects the one left (Bub's) with a card of its own.
    let last = sim.command(user, "pet", trip_claim()).await;
    assert!(
        last.description().contains("**Bub** is back from **Tidepool Meadow**!"),
        "{}",
        last.description()
    );
    assert_png(&last, 860, 420);
    let none = sim.command(user, "pet", trip_claim()).await;
    assert!(
        none.ephemeral && none.content().contains("None of your pets is on a trip"),
        "{}",
        none.content()
    );
    let row = pet_row(&db, user, second.id).await;
    assert_eq!((row.trip_dest, row.xp), (None, 25));
}

#[tokio::test]
async fn a_high_level_trip_can_bring_home_furniture_and_several_pets_claim_together() {
    let Some(mut sim) = Sim::new("a_high_level_trip_can_bring_home_furniture_and_several_pets_claim_together").await
    else {
        return;
    };
    let began = std::time::Instant::now();
    let (user, db, app) = (unique(), sim.app.db.clone(), sim.app.clone());
    let content = Content::get();
    let (pip, bub) = (
        db.adopt(user, "crumbclaw_crab", "Pip").await.unwrap(),
        db.adopt(user, "ribbonfin_guppy", "Bub").await.unwrap(),
    );
    for pet in [&pip, &bub] {
        set_pet(&db, user, pet.id, fresh_needs(100), 30, 0, app.now())
            .await
            .unwrap();
    }

    // Level 30 brings the most: 320 to 520 shells at level 1, up to 1.87 times that.
    let sent = sim.command(user, "pet", trip_go("starfall_sandbar", Some("Pip"))).await;
    assert!(sent.description().contains("Back in 8 h"), "{}", sent.description());
    let ready = sim.command(user, "pet", trip_go("starfall_sandbar", Some("Bub"))).await;
    assert!(
        ready.description().contains("sets off for **Starfall Sandbar**"),
        "{}",
        ready.description()
    );
    app.shift_clock(chrono::Duration::hours(7));
    let early = sim.command(user, "pet", trip_claim()).await;
    assert_counting_down(early.content(), "back in ", 3600, began);
    app.shift_clock(chrono::Duration::hours(1) + chrono::Duration::minutes(1));

    let before = db.user(user).await.unwrap().credits;
    let both = sim.command(user, "pet", trip_claim()).await;
    assert_eq!(both.title(), "2 pets are home");
    assert!(
        both.field("Pip the Crumbclaw Crab")
            .contains("is back from **Starfall Sandbar**")
    );
    assert!(
        both.field("Bub the Ribbonfin Guppy")
            .contains("is back from **Starfall Sandbar**")
    );
    let gained = db.user(user).await.unwrap().credits - before;
    // Two pets, each between 320 * 1.87 and 520 * 1.87 shells.
    assert!((2 * 598..=2 * 972).contains(&gained), "gained {gained}");
    for pet in [&pip, &bub] {
        let row = pet_row(&db, user, pet.id).await;
        assert_eq!(
            (row.level, row.xp, row.trip_dest),
            (30, 0, None),
            "the top level stays at the top"
        );
    }

    // Keep sending Pip until a trip brings a piece of furniture (about one in two at level 30).
    let mut found = None;
    for _ in 0..40 {
        sim.command(user, "pet", trip_go("starfall_sandbar", Some("Pip"))).await;
        app.shift_clock(chrono::Duration::hours(9));
        let claim = sim.command(user, "pet", trip_claim()).await;
        let line = claim
            .description()
            .lines()
            .find(|l| l.starts_with("It also found a **"))
            .map(str::to_owned);
        if line.is_some() {
            found = line;
            break;
        }
    }
    let line = found.expect("a trip with a find in 40 tries");
    assert!(line.ends_with("now in your `/inventory`."), "{line}");
    let inventory = db.inventory(user).await.unwrap();
    assert!(!inventory.is_empty(), "the find is in the inventory");
    let item = content.item(&inventory[0].item_id).expect("a real item");
    assert!(line.contains(&item.name()), "{line} vs {}", item.name());
    assert_eq!(
        item.base.availability,
        pinchora_game::Availability::Reward,
        "never what the shop sells"
    );
}

#[tokio::test]
async fn the_nursery_is_a_paged_grid_that_opens_one_pet() {
    let Some(mut sim) = Sim::new("the_nursery_is_a_paged_grid_that_opens_one_pet").await else {
        return;
    };
    let began = std::time::Instant::now();
    let (user, stranger, db, app) = (unique(), unique(), sim.app.db.clone(), sim.app.clone());
    let content = Content::get();

    let nobody = sim.command(stranger, "pet", pet_command("list")).await;
    assert!(
        nobody.ephemeral && nobody.content().contains("No pets yet"),
        "{}",
        nobody.content()
    );

    // Ten pets: a few levels, one asleep, one away, one back with a souvenir, one at the top.
    let names = [
        "Pip", "Bub", "Moss", "Dot", "Fennel", "Sorrel", "Tansy", "Wisp", "Nib", "Orla",
    ];
    let mut pets = Vec::new();
    for (n, name) in names.iter().enumerate() {
        pets.push(db.adopt(user, &content.pets[n * 3].id, name).await.unwrap());
    }
    let now = app.now();
    let hour = chrono::Duration::hours(1);
    set_pet(&db, user, pets[1].id, [30, 20, 60, 90], 7, 150, now)
        .await
        .unwrap();
    set_pet(
        &db,
        user,
        pets[2].id,
        [0, 0, 100, 0],
        12,
        300,
        now - chrono::Duration::hours(40),
    )
    .await
    .unwrap();
    db.start_trip(user, &pets[3], "kelp_forest_library", fresh_needs(70), now + hour * 3)
        .await
        .unwrap();
    db.start_trip(user, &pets[4], "tidepool_meadow", fresh_needs(70), now - hour)
        .await
        .unwrap();
    set_pet(&db, user, pets[5].id, [100, 95, 90, 100], 30, 0, now)
        .await
        .unwrap();
    set_pet(&db, user, pets[6].id, [10, 5, 2, 8], 2, 99, now).await.unwrap();
    set_pet(&db, user, pets[9].id, [80, 70, 60, 50], 18, 600, now)
        .await
        .unwrap();

    let page1 = sim.command(user, "pet", pet_command("list")).await;
    assert!(page1.ephemeral, "the nursery is private");
    assert_png(&page1, 1000, 600);
    assert_eq!(page1.options("pn:o:").len(), 8, "eight to a page");
    assert!(page1.description().contains("**10** pets"), "{}", page1.description());
    assert!(
        page1.description().contains("1 away") && page1.description().contains("1 asleep"),
        "{}",
        page1.description()
    );
    assert!(
        page1.description().contains("1 back with a souvenir"),
        "{}",
        page1.description()
    );
    assert!(page1.button("◀ Page").disabled && !page1.button("Page ▶").disabled);
    assert!(page1.button("1/2").disabled);
    assert!(
        page1.options("pn:o:")[0].default && page1.options("pn:o:")[0].label == "Pip",
        "the active pet comes first"
    );
    let moss = page1.options("pn:o:").iter().find(|o| o.label == "Moss").unwrap();
    assert!(
        moss.description.as_deref().unwrap().contains("Lv 12")
            && moss.description.as_deref().unwrap().contains("asleep")
    );

    let page2 = sim.click(user, &page1, "Page ▶").await;
    // Two pets fill one row, so the card is only as tall as that row.
    assert_png(&page2, 1000, 352);
    assert_eq!(page2.options("pn:o:").len(), 2);
    assert!(!page2.button("◀ Page").disabled && page2.button("Page ▶").disabled);
    assert_eq!(page2.options("pn:o:")[0].label, "Nib");
    let back = sim.click(user, &page2, "◀ Page").await;
    assert_eq!(back.options("pn:o:").len(), 8);

    // Open a pet from the second page: its card, with Make active and a way back to that page.
    let nib = &pets[8];
    let card = sim.pick(user, &page2, "pn:o:", &nib.id.to_string()).await;
    assert_eq!(
        card.title(),
        "Nib the ".to_owned() + &content.pet(&nib.species).unwrap().name
    );
    assert_png(&card, 860, 420);
    let active = sim.click(user, &card, "Make active").await;
    assert!(
        active.description().contains("Active pet changed"),
        "{}",
        active.description()
    );
    assert_eq!(db.active_pet(user).await.unwrap().unwrap().id, nib.id);
    assert!(
        active
            .controls()
            .all(|c| !matches!(c, Component::Button(b) if b.label.as_deref() == Some("Make active"))),
        "it is the active one now"
    );
    let home = sim.click(user, &active, "Nursery").await;
    // The active pet leads the first page now.
    assert_eq!(home.options("pn:o:").len(), 8);
    assert!(home.options("pn:o:")[0].default && home.options("pn:o:")[0].label == "Nib");
    assert_png(&home, 1000, 600);

    // The cards of the away, sleeping and returned pets.
    let away = sim.pick(user, &page1, "pn:o:", &pets[3].id.to_string()).await;
    // Three hours out, less what the test has used since the trip was sent.
    assert!(
        away.description()
            .contains("is away at **Kelp Forest Library**, back in "),
        "{}",
        away.description()
    );
    assert_counting_down(away.description(), "back in ", 3 * 3600, began);
    assert_png(&away, 860, 420);
    let asleep = sim.pick(user, &page1, "pn:o:", &pets[2].id.to_string()).await;
    assert!(asleep.description().contains("dozed off"), "{}", asleep.description());
    assert_png(&asleep, 860, 420);
    let returned = sim.pick(user, &page1, "pn:o:", &pets[4].id.to_string()).await;
    returned.button("Claim souvenir");
    let top = sim.pick(user, &page1, "pn:o:", &pets[5].id.to_string()).await;
    assert_png(&top, 860, 420);

    // Another player's menu pick of your pet is refused.
    let theirs = sim
        .component(stranger, "pn:o:0", &[&pets[0].id.to_string()], &page1)
        .await;
    assert!(
        theirs.ephemeral && theirs.content().contains("isn't yours"),
        "{}",
        theirs.content()
    );
    let sneaky = sim
        .component(stranger, &format!("pn:a:{}", pets[0].id), &[], &page1)
        .await;
    assert!(sneaky.content().contains("isn't yours"), "{}", sneaky.content());
    assert_eq!(db.active_pet(user).await.unwrap().unwrap().id, nib.id);
    // Another player's Nursery click on your public card does not swap it for their own nursery.
    db.adopt(stranger, &content.pets[1].id, "Rex").await.unwrap();
    let public = sim.command(user, "pet", pet_command("view")).await;
    assert!(!public.ephemeral, "/pet view is a public card");
    let swap = sim.component(stranger, &format!("pn:f:{}", nib.id), &[], &public).await;
    assert!(
        swap.ephemeral && swap.content().contains("isn't yours"),
        "{}",
        swap.content()
    );

    // The owner's own Nursery click on the public card opens the nursery as a private message of its
    // own: the card stays what everybody sees.
    let opened = sim.click(user, &public, "Nursery").await;
    assert!(opened.ephemeral, "the nursery is for its owner's eyes only");
    assert!(
        !matches!(
            opened.kind,
            InteractionResponseType::UpdateMessage | InteractionResponseType::DeferredUpdateMessage
        ),
        "a new message beside the card, not the card replaced: {:?}",
        opened.kind
    );
    assert_png(&opened, 1000, 600);
    assert_eq!(opened.options("pn:o:").len(), 8);
    // Paging works on that private copy, and so do the pet cards inside it.
    let paged = sim.click(user, &opened, "Page ▶").await;
    assert!(
        paged.ephemeral && paged.options("pn:o:").len() == 2,
        "{}",
        paged.description()
    );
    assert_png(&paged, 1000, 352);
    let inside = sim.pick(user, &paged, "pn:o:", &pets[9].id.to_string()).await;
    assert!(
        inside.ephemeral,
        "a pet card opened inside the private nursery stays private"
    );
    // Its own Nursery button turns that message back into the nursery instead of opening another.
    let again = sim.click(user, &inside, "Nursery").await;
    assert!(
        again.ephemeral
            && matches!(
                again.kind,
                InteractionResponseType::UpdateMessage | InteractionResponseType::DeferredUpdateMessage
            ),
        "{:?}",
        again.kind
    );
    assert_png(&again, 1000, 352);
}

// /Rewards, reminders and privacy

use chrono::{Datelike as _, NaiveDate, TimeZone as _, Utc};

/// `/remind me when <what> <on|off>`.
fn remind_when(what: &str, on: bool) -> Value {
    json!([group(
        "me",
        sub(
            "when",
            vec![
                text_option("what", what),
                text_option("state", if on { "on" } else { "off" })
            ]
        )
    )])
}

/// `/remind me in <duration> <text>`.
fn remind_in(duration: &str, text: &str) -> Value {
    json!([group(
        "me",
        sub("in", vec![text_option("duration", duration), text_option("text", text)])
    )])
}

/// The ticker sends every reminder in the database that is due, so the tests that make some due take
/// turns, and each starts by sending what an earlier run left behind.
static REMINDERS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The direct messages `user` got among `posts`, oldest first: the mock opens a channel named after
/// the person it is for.
fn dms_of(posts: &[Post], user: u64) -> Vec<&Post> {
    in_channel(posts, user)
}

/// Moves the game clock of `sim` to `at`.
fn set_clock(sim: &Sim, at: chrono::DateTime<Utc>) {
    sim.app.shift_clock(at - sim.app.now());
}

#[tokio::test]
async fn weekly_and_monthly_rewards_have_cooldowns_and_show_on_the_wallet() {
    let Some(mut sim) = Sim::new("weekly_and_monthly_rewards_have_cooldowns_and_show_on_the_wallet").await else {
        return;
    };
    let user = unique();
    let day = chrono::Duration::days;

    let fresh = sim.command(user, "balance", json!([])).await;
    assert_png(&fresh, 900, 300);

    let weekly = sim.command(user, "weekly", json!([])).await;
    assert_eq!(weekly.title(), "+1,500 shells");
    assert!(
        weekly.description().contains("Your weekly reward"),
        "{}",
        weekly.description()
    );
    let again = sim.command(user, "weekly", json!([])).await;
    assert_eq!(again.title(), "Already collected this week");
    assert!(again.description().contains("<t:"), "{}", again.description());
    let monthly = sim.command(user, "monthly", json!([])).await;
    assert_eq!(monthly.title(), "+6,500 shells");
    assert_eq!(shells(&sim, user).await, 8_000);
    let again = sim.command(user, "monthly", json!([])).await;
    assert_eq!(again.title(), "Already collected this month");

    // The wallet shows what is left of each: the daily is ready, the others are counting down.
    sim.command(user, "daily", json!([])).await;
    let wallet = sim.command(user, "balance", json!([])).await;
    assert_png(&wallet, 900, 300);

    // A week less a day on: still waiting. A day later the weekly pays again and the monthly does not.
    sim.app.shift_clock(day(6));
    assert_eq!(
        sim.command(user, "weekly", json!([])).await.title(),
        "Already collected this week"
    );
    sim.app.shift_clock(day(1));
    let wallet = sim.command(user, "balance", json!([])).await;
    assert_png(&wallet, 900, 300);
    assert_eq!(sim.command(user, "weekly", json!([])).await.title(), "+1,500 shells");
    assert_eq!(
        sim.command(user, "monthly", json!([])).await.title(),
        "Already collected this month"
    );

    // Thirty days after the first monthly it pays again.
    sim.app.shift_clock(day(23));
    assert_eq!(sim.command(user, "monthly", json!([])).await.title(), "+6,500 shells");
    assert_eq!(sim.command(user, "weekly", json!([])).await.title(), "+1,500 shells");
    let total = shells(&sim, user).await;
    assert_eq!(total, 8_000 + 250 + 1_500 + 6_500 + 1_500);

    // Every one of those shells shows in this month's numbers for /wrapped.
    let year = sim.app.db.activity_year(user, Utc::now().year()).await.unwrap();
    assert_eq!(year.iter().map(|(_, m)| m.shells).sum::<i64>(), total);

    // Nobody else's cooldown moved.
    let other = unique();
    assert_eq!(sim.command(other, "weekly", json!([])).await.title(), "+1,500 shells");
}

#[tokio::test]
async fn reminders_arrive_once_after_their_time() {
    let Some(mut sim) = Sim::new("reminders_arrive_once_after_their_time").await else {
        return;
    };
    let _turn = REMINDERS.lock().await;
    let user = unique();
    // The level-up test closes everyone's DMs for a while; this member's stay open.
    DM_OVERRIDES.lock().unwrap().insert(user, false);
    let hours = chrono::Duration::hours;
    let minutes = chrono::Duration::minutes;
    reminder_tick(&sim.app).await.unwrap();
    sim.posts();

    // The daily reward: switched on while it is ready, scheduled when it is collected.
    let on = sim.command(user, "remind", remind_when("daily", true)).await;
    assert!(
        on.ephemeral && on.content().contains("ready to collect now"),
        "{}",
        on.content()
    );
    sim.command(user, "daily", json!([])).await;
    assert!(sim.app.db.scheduled_reminder(user, "daily", 0).await.unwrap().is_some());
    assert_eq!(reminder_tick(&sim.app).await.unwrap(), 0, "nothing is due yet");
    assert!(dms_of(&sim.posts(), user).is_empty());

    sim.app.shift_clock(hours(25));
    reminder_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let dms = dms_of(&posts, user);
    assert_eq!(dms.len(), 1, "one message when the daily is ready");
    assert_eq!(dms[0].title(), "Your daily is ready");
    assert!(dms[0].description().contains("/daily"), "{}", dms[0].description());
    assert_eq!(
        dms[0].body["allowed_mentions"]["parse"],
        json!([]),
        "a direct message never pings anyone"
    );
    reminder_tick(&sim.app).await.unwrap();
    assert!(dms_of(&sim.posts(), user).is_empty(), "and only one");
    assert!(sim.app.db.scheduled_reminder(user, "daily", 0).await.unwrap().is_none());

    // Collected again a day later (the daily keeps real time, so its clock is turned back in the
    // database): the next message is scheduled by the claim itself.
    sqlx::query("UPDATE users SET last_daily_at = now() - interval '25 hours' WHERE id = $1")
        .bind(user as i64)
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(sim.command(user, "daily", json!([])).await.title(), "+275 shells");
    assert!(sim.app.db.scheduled_reminder(user, "daily", 0).await.unwrap().is_some());
    // Switched off: what was scheduled goes, and nothing is sent when the time comes.
    sim.command(user, "remind", remind_when("daily", false)).await;
    assert!(sim.app.db.scheduled_reminder(user, "daily", 0).await.unwrap().is_none());
    sim.app.shift_clock(hours(25));
    reminder_tick(&sim.app).await.unwrap();
    assert!(dms_of(&sim.posts(), user).is_empty());

    // The weekly reward: collected first, then asked for, so the time is worked out from the claim.
    sim.command(user, "weekly", json!([])).await;
    let on = sim.command(user, "remind", remind_when("weekly", true)).await;
    assert!(on.content().contains("Your next weekly is ready"), "{}", on.content());
    sim.app.shift_clock(chrono::Duration::days(6));
    reminder_tick(&sim.app).await.unwrap();
    assert!(dms_of(&sim.posts(), user).is_empty(), "six days is not a week");
    sim.app.shift_clock(hours(25));
    reminder_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    assert_eq!(dms_of(&posts, user).len(), 1);
    assert_eq!(dms_of(&posts, user)[0].title(), "Your weekly is ready");

    // A reminder written by the player: mentions are left out, the list shows it with a menu.
    let set = sim
        .command(user, "remind", remind_in("2h30m", "call mum <@123456> and @everyone"))
        .await;
    assert!(set.ephemeral, "reminders are private");
    assert_eq!(set.title(), "⏰ Reminder set");
    assert!(
        set.description().contains("call mum and")
            && !set.description().contains("123456")
            && !set.description().contains("everyone"),
        "{}",
        set.description()
    );
    let list = sim.command(user, "remind", json!([sub("list", vec![])])).await;
    assert!(
        list.ephemeral && list.field("Reminders").contains("call mum and"),
        "{}",
        list.field("Reminders")
    );
    assert!(
        list.field("Messages when something is ready")
            .contains("🔔 Weekly reward")
    );
    assert_eq!(list.options("rm:c").len(), 1);
    reminder_tick(&sim.app).await.unwrap();
    assert!(dms_of(&sim.posts(), user).is_empty());
    sim.app.shift_clock(hours(2) + minutes(31));
    reminder_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let dms = dms_of(&posts, user);
    assert_eq!(
        (dms.len(), dms[0].title(), dms[0].description()),
        (1, "⏰ Reminder", "call mum and")
    );
    reminder_tick(&sim.app).await.unwrap();
    assert!(dms_of(&sim.posts(), user).is_empty(), "sent once");
    assert!(sim.app.db.text_reminders(user).await.unwrap().is_empty());

    // Cancelling: from the list's menu, and by number.
    sim.command(user, "remind", remind_in("1h", "tea")).await;
    sim.command(user, "remind", remind_in("2 hours", "toast")).await;
    let list = sim.command(user, "remind", json!([sub("list", vec![])])).await;
    let tea = list
        .options("rm:c")
        .iter()
        .find(|o| o.label == "tea")
        .expect("tea")
        .value
        .clone();
    let toast = list
        .options("rm:c")
        .iter()
        .find(|o| o.label == "toast")
        .expect("toast")
        .value
        .clone();
    let list = sim.pick(user, &list, "rm:c", &tea).await;
    assert!(list.description().contains("Cancelled"), "{}", list.description());
    assert_eq!(list.options("rm:c").len(), 1);
    let gone = sim
        .command(
            user,
            "remind",
            json!([sub("cancel", vec![int_option("id", toast.parse().unwrap())])]),
        )
        .await;
    assert!(gone.content().contains("Cancelled reminder"), "{}", gone.content());
    let none = sim
        .command(
            user,
            "remind",
            json!([sub("cancel", vec![int_option("id", 987_654_321)])]),
        )
        .await;
    assert!(none.content().contains("no reminder"), "{}", none.content());
    sim.app.shift_clock(hours(3));
    reminder_tick(&sim.app).await.unwrap();
    assert!(
        dms_of(&sim.posts(), user).is_empty(),
        "cancelled reminders never arrive"
    );

    // Closed direct messages: logged quietly, the reminder is dropped and not tried again.
    sim.command(user, "remind", remind_in("5m", "closed door")).await;
    DM_OVERRIDES.lock().unwrap().insert(user, true);
    sim.app.shift_clock(minutes(6));
    reminder_tick(&sim.app).await.unwrap();
    DM_OVERRIDES.lock().unwrap().insert(user, false);
    assert!(dms_of(&sim.posts(), user).is_empty());
    assert!(sim.app.db.text_reminders(user).await.unwrap().is_empty(), "dropped");
    reminder_tick(&sim.app).await.unwrap();
    assert!(dms_of(&sim.posts(), user).is_empty());

    // Times that make no sense, and text that is only mentions.
    for (duration, text, answer) in [
        ("soon", "x", "couldn't read"),
        ("30s", "x", "too soon"),
        ("400d", "x", "a year"),
        ("5m", "<@1> @here", "Write what to remind you of"),
    ] {
        let refused = sim.command(user, "remind", remind_in(duration, text)).await;
        assert!(
            refused.ephemeral && refused.content().contains(answer),
            "{duration}: {}",
            refused.content()
        );
    }

    // At most 25 are open at a time.
    let busy = unique();
    for n in 0..25 {
        let text = format!("note {n} {}", "long ".repeat(38));
        let set = sim.command(busy, "remind", remind_in("1d", &text)).await;
        assert_eq!(set.title(), "⏰ Reminder set", "{n}");
    }
    let full = sim.command(busy, "remind", remind_in("1d", "one too many")).await;
    assert!(full.content().contains("already have 25"), "{}", full.content());
    assert_eq!(sim.app.db.text_reminders(busy).await.unwrap().len(), 25);
    // 25 notes of 200 characters still fit the list: an embed field takes 1,024 at most.
    let list = sim.command(busy, "remind", json!([sub("list", vec![])])).await;
    let field = list.field("Reminders");
    assert!(field.chars().count() <= 1024, "{} characters", field.chars().count());
    assert!(field.contains("more in the menu below"), "{field}");
    assert_eq!(list.options("rm:c").len(), 25);
    sim.app.db.delete_user_data(busy, sim.app.now()).await.unwrap();
}

#[tokio::test]
async fn pet_and_trip_reminders_follow_the_pet() {
    let Some(mut sim) = Sim::new("pet_and_trip_reminders_follow_the_pet").await else {
        return;
    };
    let _turn = REMINDERS.lock().await;
    let (user, db) = (unique(), sim.app.db.clone());
    DM_OVERRIDES.lock().unwrap().insert(user, false);
    reminder_tick(&sim.app).await.unwrap();
    sim.posts();
    let content = Content::get();
    let pet = db.adopt(user, &content.pets[0].id, "Pinchy").await.unwrap();

    // Switched on with a pet at home: told around when it will need looking after.
    let on = sim.command(user, "remind", remind_when("pet", true)).await;
    assert!(
        on.content()
            .contains("I'll message you when **Pinchy** needs looking after, around <t:"),
        "{}",
        on.content()
    );
    let first = db
        .scheduled_reminder(user, "pet", pet.id)
        .await
        .unwrap()
        .expect("scheduled");
    // Needs of 80 run low (30) after 10 hours, the fun first.
    let left = (first - pet.needs_at).num_minutes();
    assert!((595..=605).contains(&left), "{left} minutes");
    let none = sim.command(user, "remind", remind_when("trip", true)).await;
    assert!(
        none.content().contains("None of your pets is on a trip"),
        "{}",
        none.content()
    );

    // Off it goes on a one-hour trip: the pet is not at home, so it is not told about; the trip is.
    let trip = sim.command(user, "pet", trip_go("tidepool_meadow", None)).await;
    assert!(trip.description().contains("sets off"), "{}", trip.description());
    assert!(db.scheduled_reminder(user, "pet", pet.id).await.unwrap().is_none());
    let home_at = db
        .scheduled_reminder(user, "trip", pet.id)
        .await
        .unwrap()
        .expect("trip scheduled");
    assert_eq!(
        home_at,
        db.active_pet(user).await.unwrap().unwrap().trip_ends_at.unwrap()
    );
    assert_eq!(reminder_tick(&sim.app).await.unwrap(), 0);
    assert!(dms_of(&sim.posts(), user).is_empty());

    set_clock(&sim, home_at + chrono::Duration::minutes(1));
    reminder_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let dms = dms_of(&posts, user);
    assert_eq!(dms.len(), 1, "told when the pet is home");
    assert_eq!(dms[0].title(), "Pinchy is home");
    assert!(
        dms[0].description().contains("/pet trip claim"),
        "{}",
        dms[0].description()
    );

    // Looking at a pet that is back (the trip not claimed yet) does not announce the trip again.
    sim.command(user, "pet", pet_command("view")).await;
    assert!(
        db.scheduled_reminder(user, "trip", pet.id).await.unwrap().is_none(),
        "a trip that is over is not scheduled again"
    );
    reminder_tick(&sim.app).await.unwrap();
    assert!(dms_of(&sim.posts(), user).is_empty());

    // Claimed: the pet is at home again, and its next message is about its needs.
    let claim = sim
        .command(
            user,
            "pet",
            json!([{ "name": "trip", "type": 2, "options": [{ "name": "claim", "type": 1 }] }]),
        )
        .await;
    assert!(claim.description().contains("is back from"), "{}", claim.description());
    let due = db
        .scheduled_reminder(user, "pet", pet.id)
        .await
        .unwrap()
        .expect("the pet is watched again");
    assert!(due > home_at);
    set_clock(&sim, due + chrono::Duration::minutes(1));
    reminder_tick(&sim.app).await.unwrap();
    let posts = sim.posts();
    let dms = dms_of(&posts, user);
    assert_eq!(dms.len(), 1, "told when the pet needs looking after");
    assert_eq!(dms[0].title(), "Pinchy needs you");
    assert!(
        dms[0].description().contains("someone to play with"),
        "{}",
        dms[0].description()
    );
    reminder_tick(&sim.app).await.unwrap();
    assert!(dms_of(&sim.posts(), user).is_empty(), "once");

    // A pet that is already low, looked at again: the message that was sent is not scheduled and sent
    // again every time the card is shown.
    let card = sim.command(user, "pet", pet_command("view")).await;
    sim.command(user, "pet", pet_command("view")).await;
    assert!(
        db.scheduled_reminder(user, "pet", pet.id).await.unwrap().is_none(),
        "an overdue message is not scheduled again"
    );
    reminder_tick(&sim.app).await.unwrap();
    assert!(
        dms_of(&sim.posts(), user).is_empty(),
        "nothing arrives for a pet that was told about"
    );

    // Looking after the pet schedules the next message, for later.
    sim.click(user, &card, "Play").await;
    let after = db
        .scheduled_reminder(user, "pet", pet.id)
        .await
        .unwrap()
        .expect("scheduled for the next time it needs looking after");
    assert!(after > sim.app.now(), "{after} is still to come");

    // Turned off: nothing is waiting, and nothing arrives.
    sim.command(user, "remind", remind_when("pet", false)).await;
    sim.command(user, "remind", remind_when("trip", false)).await;
    assert!(db.scheduled_reminder(user, "pet", pet.id).await.unwrap().is_none());
    assert!(db.reminder_optins(user).await.unwrap().is_empty());
}

#[tokio::test]
async fn privacy_choices_are_seen_by_other_players_and_data_can_be_deleted() {
    let Some(mut sim) = Sim::new("privacy_choices_are_seen_by_other_players_and_data_can_be_deleted").await else {
        return;
    };
    let (owner, viewer, db) = (unique(), unique(), sim.app.db.clone());
    fund(&sim, owner, 700).await;
    db.befriend(owner, viewer).await.unwrap();
    let both = resolved(&[owner], &[]);
    let by_owner = |what: &str| json!([user_option(what, owner)]);

    // Public to begin with.
    let public = profile_of(&mut sim, viewer, owner, "Marlowe").await;
    assert_eq!((public.files.len(), public.ephemeral), (1, false));
    let panel = sim.command(owner, "privacy", json!([])).await;
    assert!(
        panel.ephemeral && panel.description().contains("Public."),
        "{}",
        panel.description()
    );
    assert_eq!(panel.rows(), 2);

    // Profile: private. Every card of theirs says so to others, and still opens for them.
    let panel = sim.click(owner, &panel, "Profile: public").await;
    assert!(
        panel.description().contains("Saved.") && panel.has_button("Profile: private"),
        "{}",
        panel.description()
    );
    assert!(db.privacy(owner).await.unwrap().hide_profile);
    let hidden = profile_of(&mut sim, viewer, owner, "Marlowe").await;
    assert!(hidden.ephemeral && hidden.content() == "This profile is private." && hidden.files.is_empty());
    for (command, options) in [
        ("rank", by_owner("user")),
        ("balance", by_owner("user")),
        ("friends", json!([sub("list", vec![user_option("user", owner)])])),
        ("partner", json!([sub("view", vec![user_option("user", owner)])])),
        ("play", json!([sub("stats", vec![user_option("user", owner)])])),
    ] {
        let reply = sim.command_resolved(viewer, command, options, both.clone()).await;
        assert!(
            reply.ephemeral && reply.content() == "This profile is private.",
            "/{command}: {:?}",
            reply.content()
        );
    }
    let own = sim.command(owner, "profile", json!([])).await;
    assert_eq!(own.files.len(), 1, "they see their own");
    let own = sim
        .command_resolved(owner, "balance", by_owner("user"), both.clone())
        .await;
    assert_eq!(own.files.len(), 1, "even when they name themselves");
    let panel = sim.click(owner, &panel, "Profile: private").await;
    assert!(panel.has_button("Profile: public"));
    assert_eq!(
        profile_of(&mut sim, viewer, owner, "Marlowe").await.files.len(),
        1,
        "public again"
    );

    // Global boards: the owner is first on the XP board until they hide from it.
    sqlx::query("UPDATE users SET xp = $2 WHERE id = $1")
        .bind(owner as i64)
        .bind(owner as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let leader = |rows: Vec<pinchora_db::LeaderboardRow>| rows.first().map(|r| r.user_id as u64);
    assert_eq!(leader(db.global_xp_leaderboard(1, 0).await.unwrap()), Some(owner));
    let panel = sim.click(owner, &panel, "Boards: listed").await;
    assert!(panel.has_button("Boards: hidden"), "{}", panel.description());
    assert_ne!(leader(db.global_xp_leaderboard(1, 0).await.unwrap()), Some(owner));
    let board = sim
        .command(viewer, "leaderboard", json!([text_option("board", "xp")]))
        .await;
    assert_eq!(board.files.len(), 1, "the board still draws");
    let panel = sim.click(owner, &panel, "Boards: hidden").await;
    assert_eq!(leader(db.global_xp_leaderboard(1, 0).await.unwrap()), Some(owner));

    // House: friends find it in /house visit; hidden, it is not offered and cannot be visited by name.
    let offered = sim.command(viewer, "house", json!([sub("visit", vec![])])).await;
    assert!(
        offered.options("fd:v:").iter().any(|o| o.value == owner.to_string()),
        "{offered}"
    );
    let panel = sim.click(owner, &panel, "House: open").await;
    assert!(panel.has_button("House: hidden"));
    let none = sim.command(viewer, "house", json!([sub("visit", vec![])])).await;
    assert!(
        none.content().contains("Name whose house to visit") && none.rows() == 0,
        "no shortlist: {none}"
    );
    let refused = sim
        .command_resolved(
            viewer,
            "house",
            json!([sub("visit", vec![user_option("user", owner)])]),
            both.clone(),
        )
        .await;
    assert!(
        refused.ephemeral && refused.content().contains("is private"),
        "{}",
        refused.content()
    );
    assert_eq!(db.visitable_friends(viewer, 10).await.unwrap().len(), 0);
    let panel = sim.click(owner, &panel, "House: hidden").await;
    let visit = sim
        .command_resolved(
            viewer,
            "house",
            json!([sub("visit", vec![user_option("user", owner)])]),
            both.clone(),
        )
        .await;
    assert_eq!(visit.files.len(), 1, "open again");

    // Delete my data: two steps, and the second one times out.
    let confirm = sim.click(owner, &panel, "Delete my data…").await;
    assert_eq!(confirm.title(), "Delete all your data?");
    let kept = sim.click(owner, &confirm, "Keep my data").await;
    assert!(
        kept.description().contains("Nothing was deleted"),
        "{}",
        kept.description()
    );
    assert_eq!(shells(&sim, owner).await, 700);
    let confirm = sim.click(owner, &kept, "Delete my data…").await;
    sim.app.shift_clock(chrono::Duration::minutes(3));
    let late = sim.click(owner, &confirm, "Yes, delete everything").await;
    assert!(late.description().contains("timed out"), "{}", late.description());
    assert_eq!(shells(&sim, owner).await, 700, "a late confirmation deletes nothing");

    // Somebody else's click on the confirm button deletes their own data, never the owner's.
    let confirm = sim.click(owner, &late, "Delete my data…").await;
    let stranger = unique();
    fund(&sim, stranger, 50).await;
    sim.click(stranger, &confirm, "Yes, delete everything").await;
    assert_eq!(shells(&sim, owner).await, 700);
    assert_eq!(shells(&sim, stranger).await, 0);

    db.set_privacy(owner, pinchora_db::Hide::House, true).await.unwrap();
    let done = sim.click(owner, &confirm, "Yes, delete everything").await;
    assert_eq!(done.title(), "Your data is deleted");
    assert!(done.components.is_empty(), "nothing left to press");
    assert!(done.description().contains("Removed"), "{}", done.description());
    for (table, column) in pinchora_db::USER_COLUMNS {
        let (left,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE {column} = $1"
        )))
        .bind(owner as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert_eq!(left, 0, "{table}.{column}");
    }
    assert!(
        !db.are_friends(viewer, owner).await.unwrap(),
        "the friendship went with them"
    );
    assert_eq!(shells(&sim, owner).await, 0, "they start over");
    assert!(!db.privacy(owner).await.unwrap().hide_house);
}

/// Deletes `user`'s data through the panel's two steps.
async fn delete_through_the_panel(sim: &mut Sim, user: u64) -> Reply {
    let panel = sim.command(user, "privacy", json!([])).await;
    let confirm = sim.click(user, &panel, "Delete my data…").await;
    sim.click(user, &confirm, "Yes, delete everything").await
}

#[tokio::test]
async fn deleting_data_does_not_hand_out_a_second_daily_weekly_monthly_or_walk() {
    let Some(mut sim) = Sim::new("deleting_data_does_not_hand_out_a_second_daily_weekly_monthly_or_walk").await else {
        return;
    };
    let (user, alt) = (unique(), unique());
    let (db, day) = (sim.app.db.clone(), chrono::Duration::days);

    // Everything there is to collect, and a pet that has walked.
    assert!(sim.command(user, "daily", json!([])).await.title().starts_with("+"));
    assert_eq!(sim.command(user, "weekly", json!([])).await.title(), "+1,500 shells");
    assert_eq!(sim.command(user, "monthly", json!([])).await.title(), "+6,500 shells");
    let collected = shells(&sim, user).await;
    db.adopt(user, &Content::get().pets[0].id, "Pinchy").await.unwrap();

    // The loop: give it all to an alt, delete, come back for more.
    for round in 0..3 {
        let balance = shells(&sim, user).await;
        if balance > 0 {
            db.transfer_credits(user, alt, balance).await.unwrap();
        }
        let done = delete_through_the_panel(&mut sim, user).await;
        assert_eq!(done.title(), "Your data is deleted", "round {round}");
        assert_eq!(shells(&sim, user).await, 0, "they start over with nothing");
        assert_eq!(
            sim.command(user, "daily", json!([])).await.title(),
            "Already collected today",
            "round {round}"
        );
        assert_eq!(
            sim.command(user, "weekly", json!([])).await.title(),
            "Already collected this week",
            "round {round}"
        );
        assert_eq!(
            sim.command(user, "monthly", json!([])).await.title(),
            "Already collected this month",
            "round {round}"
        );
    }
    assert_eq!(
        shells(&sim, alt).await,
        collected,
        "one cycle's worth, however often they deleted"
    );

    // The wallet counts the same waiting times down again.
    let wallet = sim.command(user, "balance", json!([])).await;
    assert_png(&wallet, 900, 300);

    // A new pet is held to the old pet's walk cooldown.
    db.adopt(user, &Content::get().pets[0].id, "Pinchy").await.unwrap();
    let card = sim.command(user, "pet", pet_command("view")).await;
    let walk = sim.click(user, &card, "Walk").await;
    assert!(
        walk.ephemeral && walk.content().contains("needs a break"),
        "{}",
        walk.content()
    );
    assert_eq!(shells(&sim, user).await, 0, "no shells were found");
    sim.app.shift_clock(chrono::Duration::minutes(50));
    let walk = sim.click(user, &card, "Walk").await;
    assert!(walk.description().contains("Found"), "{}", walk.description());

    // Waiting still works: a week and a day later the weekly pays again, the monthly does not.
    sim.app.shift_clock(day(8));
    assert_eq!(sim.command(user, "weekly", json!([])).await.title(), "+1,500 shells");
    assert_eq!(
        sim.command(user, "monthly", json!([])).await.title(),
        "Already collected this month"
    );
    sim.app.shift_clock(day(23));
    assert_eq!(sim.command(user, "monthly", json!([])).await.title(), "+6,500 shells");
}

#[tokio::test]
async fn a_hidden_house_and_a_private_profile_close_every_door() {
    let Some(mut sim) = Sim::new("a_hidden_house_and_a_private_profile_close_every_door").await else {
        return;
    };
    let (owner, viewer, db) = (unique(), unique(), sim.app.db.clone());
    let both = resolved(&[owner], &[]);
    let guestbook = json!([{ "name": "guestbook", "type": 1, "options": [user_option("user", owner)] }]);
    let like = json!([sub("like", vec![user_option("user", owner)])]);
    let points = json!([sub("show", vec![user_option("user", owner)])]);
    let stall = json!([sub("view", vec![user_option("user", owner)])]);
    let likes = |db: pinchora_db::Db| async move {
        let (likes,): (i32,) = sqlx::query_as("SELECT COALESCE(max(likes), 0) FROM houses WHERE user_id = $1")
            .bind(owner as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
        likes
    };

    // A book with two pages, so there is a page to turn.
    for n in 0..6 {
        sqlx::query(
            "INSERT INTO guestbook_notes (host_id, author_id, author_name, day, body)
             VALUES ($1, $2, 'Guest', current_date, 'hello')",
        )
        .bind(owner as i64)
        .bind(unique() as i64 + n)
        .execute(db.pool())
        .await
        .unwrap();
    }

    // Open: the book reads, the house can be liked, the points and the stall show.
    let book = sim
        .command_resolved(viewer, "house", guestbook.clone(), both.clone())
        .await;
    assert!(book.title().contains("guestbook"), "{}", book.title());
    let liked = sim.command_resolved(viewer, "house", like.clone(), both.clone()).await;
    assert!(liked.content().contains("liked"), "{}", liked.content());
    assert!(
        liked.pings_only(owner),
        "the owner is told, nobody else: {:?}",
        liked.mentions
    );
    assert_eq!(likes(db.clone()).await, 1);
    assert_eq!(
        sim.command_resolved(viewer, "points", points.clone(), both.clone())
            .await
            .title(),
        "⭐ Points"
    );
    assert_png(
        &sim.command_resolved(viewer, "stall", stall.clone(), both.clone()).await,
        1000,
        600,
    );

    // The house is hidden: no book (a page from before does not turn), no like, no signing.
    db.set_privacy(owner, pinchora_db::Hide::House, true).await.unwrap();
    for (name, options) in [("guestbook", guestbook.clone()), ("like", like.clone())] {
        let refused = sim.command_resolved(viewer, "house", options, both.clone()).await;
        assert!(
            refused.ephemeral && refused.content() == "This house is private.",
            "{name}: {}",
            refused.content()
        );
    }
    let turned = sim.click(viewer, &book, "Older ▶").await;
    assert_eq!(turned.content(), "This house is private.");
    assert_eq!(likes(db.clone()).await, 1, "nothing was liked");
    assert_eq!(
        sim.command_resolved(owner, "house", guestbook.clone(), both.clone())
            .await
            .title(),
        "📖 Your guestbook",
        "the owner still reads their own book"
    );

    // A private profile hides the points and the stall as well.
    db.set_privacy(owner, pinchora_db::Hide::House, false).await.unwrap();
    db.set_privacy(owner, pinchora_db::Hide::Profile, true).await.unwrap();
    for (command, options) in [("points", points), ("stall", stall)] {
        let refused = sim.command_resolved(viewer, command, options, both.clone()).await;
        assert!(
            refused.ephemeral && refused.content() == "This profile is private.",
            "/{command}: {}",
            refused.content()
        );
    }
}

#[tokio::test]
async fn wrapped_opens_in_december_and_stays_open_through_january() {
    let Some(mut sim) = Sim::new("wrapped_opens_in_december_and_stays_open_through_january").await else {
        return;
    };
    let (user, host, newcomer, db) = (unique(), unique(), unique(), sim.app.db.clone());
    let year = Utc::now().year();
    let at = |y: i32, m: u32, d: u32| Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap();

    // A year of numbers: busy in the autumn, a favourite fish and a friend's house visited often.
    let counted = [
        (
            Counter::Messages,
            [(1, 120), (3, 300), (5, 80), (7, 560), (9, 410), (11, 980)],
        ),
        (
            Counter::Xp,
            [
                (1, 2_400),
                (3, 6_100),
                (5, 1_500),
                (7, 11_200),
                (9, 8_300),
                (11, 19_500),
            ],
        ),
        (
            Counter::Shells,
            [(1, 900), (3, 4_000), (5, 700), (7, 9_000), (9, 5_500), (11, 12_000)],
        ),
        (Counter::PetCare, [(1, 10), (3, 30), (5, 8), (7, 50), (9, 40), (11, 90)]),
        (Counter::LikesGiven, [(1, 1), (3, 4), (5, 0), (7, 6), (9, 3), (11, 9)]),
        (
            Counter::LikesReceived,
            [(1, 2), (3, 3), (5, 1), (7, 8), (9, 2), (11, 12)],
        ),
    ];
    for (counter, months) in counted {
        for (month, n) in months {
            db.add_activity_in(user, NaiveDate::from_ymd_opt(year, month, 1).unwrap(), counter, n)
                .await
                .unwrap();
        }
    }
    let fish = &Content::get().fish[2];
    for size in [10.0, 12.0, 14.0] {
        db.log_catch(user, &fish.id, size).await.unwrap();
    }
    db.log_catch(user, &Content::get().fish[5].id, 20.0).await.unwrap();
    db.befriend(user, host).await.unwrap();
    db.remember_name(host, "Marlowe the Mapmaker").await.unwrap();
    for day in [3, 9, 15, 21, 27] {
        db.record_visit(host, user, NaiveDate::from_ymd_opt(year, 6, day).unwrap(), 0, 10)
            .await
            .unwrap();
    }

    // Outside the window it says when it opens.
    set_clock(&sim, at(year, 6, 15));
    let closed = sim.command(user, "wrapped", json!([])).await;
    let opens = Utc.with_ymd_and_hms(year, 12, 1, 0, 0, 0).unwrap().timestamp();
    assert!(
        closed.ephemeral && closed.content().contains(&format!("<t:{opens}:D>")) && closed.files.is_empty(),
        "{}",
        closed.content()
    );
    set_clock(&sim, at(year, 11, 30));
    assert!(
        sim.command(user, "wrapped", json!([])).await.files.is_empty(),
        "not the day before"
    );

    // December 1 opens it: the card of the year, and a quiet one for somebody with nothing.
    set_clock(&sim, Utc.with_ymd_and_hms(year, 12, 1, 0, 0, 5).unwrap());
    let card = sim.command(user, "wrapped", json!([])).await;
    assert_png(&card, 640, 1164);
    assert!(!card.ephemeral, "the card is for sharing");
    let saved = sim.dir.join(format!("{:02}-wrapped.png", sim.step));
    let samples = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .parent()
        .unwrap()
        .join("render-samples");
    std::fs::create_dir_all(&samples).unwrap();
    std::fs::copy(&saved, samples.join("wrapped-test-user.png")).expect("copy the sample");
    let quiet = sim.command(newcomer, "wrapped", json!([])).await;
    assert_png(&quiet, 640, 1164);
    std::fs::copy(
        sim.dir.join(format!("{:02}-wrapped.png", sim.step)),
        samples.join("wrapped-test-user-quiet.png"),
    )
    .expect("copy the quiet sample");

    // Years: the one that just ended, an earlier one with nothing, a later one that is not over.
    let same = sim
        .command(user, "wrapped", json!([int_option("year", year.into())]))
        .await;
    assert_png(&same, 640, 1164);
    let early = sim
        .command(user, "wrapped", json!([int_option("year", (year - 1).into())]))
        .await;
    assert!(
        early.ephemeral && early.content().contains("Nothing was counted"),
        "{}",
        early.content()
    );
    let later = sim
        .command(user, "wrapped", json!([int_option("year", (year + 1).into())]))
        .await;
    assert!(
        later.ephemeral && later.content().contains("hasn't been wrapped up yet"),
        "{}",
        later.content()
    );

    // January still shows the year that ended; February closes it again.
    set_clock(&sim, at(year + 1, 1, 31));
    assert_png(&sim.command(user, "wrapped", json!([])).await, 640, 1164);
    set_clock(&sim, at(year + 1, 2, 1));
    let shut = sim.command(user, "wrapped", json!([])).await;
    let reopens = Utc.with_ymd_and_hms(year + 1, 12, 1, 0, 0, 0).unwrap().timestamp();
    assert!(
        shut.content().contains(&format!("<t:{reopens}:D>")),
        "{}",
        shut.content()
    );
}

// /Server events

/// The events tests take turns: a tick ends every giveaway and poll that is due in the database, and
/// these tests shift their clock far enough to end each other's.
static EVENTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A message the handlers posted, as a member sees it: its text, embeds and buttons.
fn reply_of(post: &Post) -> Reply {
    let mut reply = Reply::new(InteractionResponseType::ChannelMessageWithSource);
    reply.content = post.body["content"].as_str().map(str::to_owned);
    reply.embeds = post
        .body
        .get("embeds")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    reply.components = post
        .body
        .get("components")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    reply
}

/// Whether a posted message told Discord to ping nobody.
fn pings_nobody(post: &Post) -> bool {
    post.body["allowed_mentions"] == json!({ "parse": [] })
}

/// The one message of `method` a step left in `channel`.
fn only(posts: &[Post], channel: u64, method: Method) -> &Post {
    let found: Vec<&Post> = posts
        .iter()
        .filter(|p| p.channel == channel && p.method == method)
        .collect();
    match found.as_slice() {
        [post] => post,
        other => panic!("expected one {method} in channel {channel}, found {}", other.len()),
    }
}

fn giveaway_start(duration: &str, winners: i64, prize: &str, shells: Option<i64>) -> Vec<Value> {
    let mut options = vec![
        text_option("duration", duration),
        int_option("winners", winners),
        text_option("prize", prize),
    ];
    options.extend(shells.map(|s| int_option("shells", s)));
    options
}

/// `/giveaway <sub> options`.
async fn giveaway_cmd(sim: &mut Sim, user: u64, name: &str, options: Vec<Value>) -> Reply {
    sim.command(user, "giveaway", json!([sub(name, options)])).await
}

async fn running_giveaways(sim: &Sim) -> usize {
    sim.app.db.running_giveaways(sim.guild).await.expect("giveaways").len()
}

#[tokio::test]
async fn giveaways_run_pay_once_and_survive_restarts() {
    let Some(mut sim) = Sim::new("giveaways_run_pay_once_and_survive_restarts").await else {
        return;
    };
    let _turn = EVENTS.lock().await;
    sim.channel = unique();
    let channel = sim.channel;
    let (host, poor, alice, bob, carol, dave, eve) =
        (unique(), unique(), unique(), unique(), unique(), unique(), unique());
    fund(&sim, host, 1_000).await;
    fund(&sim, poor, 10).await;
    let tick = |sim: &Sim| {
        let app = sim.app.clone();
        async move { pinchora_commands::events_tick(&app).await.expect("tick") }
    };

    // Only managers start giveaways.
    let denied = giveaway_cmd(
        &mut sim,
        host,
        "start",
        giveaway_start("2h", 2, "Golden shell", Some(50)),
    )
    .await;
    assert!(denied.content().contains("Manage Server"), "{}", denied.content());
    assert!(denied.pings_nobody(), "every answer sets allowed_mentions");
    assert_eq!(running_giveaways(&sim).await, 0);
    sim.permissions = MANAGE_GUILD;

    // Bad durations and an empty prize.
    for (duration, expected) in [
        ("soon", "couldn't read"),
        ("10s", "1 minute to 30 days"),
        ("45d", "1 minute to 30 days"),
    ] {
        let bad = giveaway_cmd(&mut sim, host, "start", giveaway_start(duration, 1, "x", None)).await;
        assert!(bad.content().contains(expected), "{duration}: {}", bad.content());
    }
    let empty = giveaway_cmd(&mut sim, host, "start", giveaway_start("1h", 1, "<@1>", None)).await;
    assert!(empty.content().contains("a prize"), "{}", empty.content());
    assert_eq!(running_giveaways(&sim).await, 0);

    // A shells prize is held from the host, and the prize text cannot carry a mention.
    let started = giveaway_cmd(
        &mut sim,
        host,
        "start",
        giveaway_start("2h", 2, "Golden <@42> shell @everyone", Some(50)),
    )
    .await;
    assert!(
        started.ephemeral && started.content().contains("is live in"),
        "{}",
        started.content()
    );
    assert!(
        started.content().contains("100 shells were taken from you"),
        "{}",
        started.content()
    );
    assert_eq!(shells(&sim, host).await, 900);
    let posts = sim.posts();
    let post = only(&posts, channel, Method::POST);
    assert_eq!(post.title(), "Golden shell everyone");
    assert!(pings_nobody(post), "{}", post.body);
    let text = post.embed()["description"].as_str().unwrap();
    assert!(
        text.contains("Winners: **2**") && text.contains("**50** shells each"),
        "{text}"
    );
    let first = sim.app.db.running_giveaways(sim.guild).await.unwrap()[0].clone();
    assert_eq!(first.message_id, Some(post.message as i64));
    let message = reply_of(post);

    // A host who cannot cover the prize starts nothing and loses nothing.
    let broke = giveaway_cmd(&mut sim, poor, "start", giveaway_start("1h", 1, "Big prize", Some(50))).await;
    assert!(
        broke.content().contains("You have 10") && broke.content().contains("**50** shells"),
        "{}",
        broke.content()
    );
    assert_eq!((shells(&sim, poor).await, running_giveaways(&sim).await), (10, 1));
    assert!(sim.posts().is_empty());

    // A channel the bot cannot post in: the host gets the held shells back.
    let walled = unique();
    sim.block_channel(walled);
    let mut options = giveaway_start("1h", 1, "Walled prize", Some(40));
    options.push(channel_option("channel", walled));
    let refused = sim
        .command_resolved(
            host,
            "giveaway",
            json!([sub("start", options)]),
            resolved_channels(&[walled]),
        )
        .await;
    assert!(refused.content().contains("I can't post in"), "{}", refused.content());
    assert_eq!((shells(&sim, host).await, running_giveaways(&sim).await), (900, 1));

    // Entering: a toggle, an ephemeral answer, and one message edit per five seconds.
    let hosting = sim.click(host, &message, "Enter").await;
    assert!(hosting.content().contains("can't enter"), "{}", hosting.content());
    let entered = sim.click(alice, &message, "Enter").await;
    assert!(
        entered.ephemeral && entered.pings_nobody() && entered.content().contains("1 entry so far"),
        "{}",
        entered.content()
    );
    let posts = sim.posts();
    let edit = only(&posts, channel, Method::PATCH);
    assert_eq!(edit.message, post.message);
    assert_eq!(edit.field("Entries"), "1", "the first entry shows at once");
    // The five seconds are real time, and a loaded machine can spend more than that between two clicks.
    // So the window is held open by hand from here to the tick, and closed by hand to end it.
    let window = format!("gw-refresh:{}", first.id);
    sim.app.cache.release_cooldown(&window).await.unwrap();
    assert!(
        sim.app
            .cache
            .try_cooldown(&window, std::time::Duration::from_secs(600))
            .await
            .unwrap()
            .is_none(),
        "the window was taken by nobody else"
    );
    let (second, third) = (
        sim.click(bob, &message, "Enter").await,
        sim.click(carol, &message, "Enter").await,
    );
    assert!(second.content().contains("2 entries so far"), "{}", second.content());
    assert!(third.content().contains("3 entries so far"), "{}", third.content());
    let left = sim.click(alice, &message, "Enter").await;
    assert!(left.content().contains("You left"), "{}", left.content());
    let back = sim.click(alice, &message, "Enter").await;
    assert!(back.content().contains("3 entries so far"), "{}", back.content());
    tick(&sim).await;
    assert!(
        in_channel(&sim.posts(), channel).is_empty(),
        "inside the five seconds nothing is edited, not even by the ticker"
    );
    sim.app.cache.release_cooldown(&window).await.unwrap();
    tick(&sim).await;
    let posts = sim.posts();
    assert_eq!(
        only(&posts, channel, Method::PATCH).field("Entries"),
        "3",
        "the ticker catches up"
    );
    tick(&sim).await;
    assert!(in_channel(&sim.posts(), channel).is_empty(), "and only once");

    // A required role is checked when a member presses Enter.
    let vip_id = unique();
    let vip = sim.guild_role(vip_id, "VIP", 2);
    let mut options = giveaway_start("5h", 2, "VIP cake", Some(10));
    options.push(role_option("required_role", &vip));
    let started = sim
        .command_resolved(host, "giveaway", json!([sub("start", options)]), resolved(&[], &[&vip]))
        .await;
    assert!(started.content().contains("20 shells"), "{}", started.content());
    let posts = sim.posts();
    let vip_post = only(&posts, channel, Method::POST);
    assert!(
        vip_post.embed()["description"]
            .as_str()
            .unwrap()
            .contains(&format!("Requires <@&{vip_id}>")),
        "{}",
        vip_post.embed()
    );
    let vip_message = reply_of(vip_post);
    let vip_giveaway = sim
        .app
        .db
        .running_giveaways(sim.guild)
        .await
        .unwrap()
        .into_iter()
        .find(|g| g.prize == "VIP cake")
        .expect("the VIP giveaway");
    let shut_out = sim.click(dave, &vip_message, "Enter").await;
    assert!(
        shut_out.content().contains(&format!("<@&{vip_id}>")),
        "{}",
        shut_out.content()
    );
    sim.roles = vec![vip_id];
    let welcomed = sim.click(eve, &vip_message, "Enter").await;
    sim.roles.clear();
    assert!(welcomed.content().contains("You're in"), "{}", welcomed.content());
    assert_eq!(shells(&sim, host).await, 880);
    let posts = sim.posts();
    assert_eq!(
        only(&posts, channel, Method::PATCH).field("Entries"),
        "1",
        "its own five seconds"
    );

    // /giveaway list shows what is running.
    let list = giveaway_cmd(&mut sim, host, "list", vec![]).await;
    assert!(list.ephemeral && list.pings_nobody());
    let lines = list.description();
    assert!(
        lines.contains(&format!("**#{}**", first.id))
            && lines.contains("Golden shell everyone")
            && lines.contains(&format!("**#{}**", vip_giveaway.id))
            && lines.contains("3 entries")
            && lines.contains("1 entry"),
        "{lines}"
    );

    // Time passes: the first giveaway (2 hours) is over, the VIP one (5 hours) is not.
    sim.app.shift_clock(chrono::Duration::hours(3));
    let late = sim.click(dave, &message, "Enter").await;
    assert!(late.content().contains("has ended"), "{}", late.content());
    tick(&sim).await;
    let ended = sim.app.db.giveaway(first.id).await.unwrap().unwrap();
    assert_eq!(ended.status, "ended");
    assert!(ended.announced);
    assert_eq!(ended.winner_ids.len(), 2, "two winners from three entrants");
    let entrants = [alice, bob, carol];
    assert!(ended.winner_ids.iter().all(|w| entrants.contains(&(*w as u64))));
    for entrant in entrants {
        let won = ended.winner_ids.contains(&(entrant as i64));
        assert_eq!(
            shells(&sim, entrant).await,
            if won { 50 } else { 0 },
            "payout of {entrant}"
        );
    }
    assert_eq!(
        shells(&sim, host).await,
        880,
        "both prizes were won, nothing comes back"
    );
    let posts = sim.posts();
    let result = only(&posts, channel, Method::POST);
    assert!(pings_nobody(result), "{}", result.body);
    for winner in &ended.winner_ids {
        assert!(
            result.content().contains(&format!("<@{winner}>")),
            "{}",
            result.content()
        );
    }
    assert!(
        result.content().contains("Congratulations") && result.content().contains("50 shells were paid"),
        "{}",
        result.content()
    );
    let locked = only(&posts, channel, Method::PATCH);
    assert_eq!(locked.message, post.message);
    assert_eq!(locked.body["components"][0]["components"][0]["disabled"], json!(true));
    assert_eq!(locked.field("Winners").matches("<@").count(), 2);
    assert_eq!(
        sim.app.db.giveaway(vip_giveaway.id).await.unwrap().unwrap().status,
        "running"
    );
    tick(&sim).await;
    assert!(
        in_channel(&sim.posts(), channel).is_empty(),
        "a giveaway ends exactly once"
    );
    for entrant in entrants {
        assert!(shells(&sim, entrant).await <= 50, "nobody was paid twice");
    }

    // A reroll picks someone who has not won, pays nothing, and runs out of people.
    let leftover = entrants
        .into_iter()
        .find(|e| !ended.winner_ids.contains(&(*e as i64)))
        .expect("one entrant did not win");
    let id = int_option("id", first.id);
    let rerolled = giveaway_cmd(&mut sim, host, "reroll", vec![id.clone()]).await;
    assert!(
        rerolled.content().contains(&format!("<@{leftover}>")),
        "{}",
        rerolled.content()
    );
    let posts = sim.posts();
    let announced = only(&posts, channel, Method::POST);
    assert!(
        announced.content().contains(&format!("<@{leftover}>")) && pings_nobody(announced),
        "{}",
        announced.body
    );
    assert!(
        announced.content().contains("(no shells on a reroll)"),
        "the shells prize says nothing is paid again: {}",
        announced.content()
    );
    assert_eq!(shells(&sim, leftover).await, 0, "a reroll does not pay shells");
    let none_left = giveaway_cmd(&mut sim, host, "reroll", vec![id]).await;
    assert!(
        none_left.content().contains("Nobody is left"),
        "{}",
        none_left.content()
    );
    let running = giveaway_cmd(&mut sim, host, "reroll", vec![int_option("id", vip_giveaway.id)]).await;
    assert!(running.content().contains("still running"), "{}", running.content());

    // Ending early: the winner is paid and the prize nobody won goes back to the host.
    let early = giveaway_cmd(&mut sim, host, "end", vec![int_option("id", vip_giveaway.id)]).await;
    assert!(
        early.content().contains("Ended giveaway") && early.content().contains(&format!("<@{eve}>")),
        "{}",
        early.content()
    );
    assert_eq!((shells(&sim, eve).await, shells(&sim, host).await), (10, 890));
    let posts = sim.posts();
    let result = only(&posts, channel, Method::POST);
    assert!(
        result.content().contains("10 unclaimed shells went back"),
        "{}",
        result.content()
    );
    let again = giveaway_cmd(&mut sim, host, "end", vec![int_option("id", vip_giveaway.id)]).await;
    assert!(again.content().contains("already ended"), "{}", again.content());
    let other = giveaway_cmd(&mut sim, host, "end", vec![int_option("id", 999_999_999)]).await;
    assert!(other.content().contains("no giveaway"), "{}", other.content());

    // Another server's giveaway cannot be reached by its number.
    let elsewhere = match sim
        .app
        .db
        .create_giveaway(pinchora_db::NewGiveaway {
            guild_id: unique(),
            channel_id: unique(),
            host: unique(),
            prize: "Elsewhere",
            shells: 0,
            winners: 1,
            required_role: None,
            ends_at: sim.app.now() + chrono::Duration::hours(1),
        })
        .await
        .unwrap()
    {
        pinchora_db::GiveawayOpen::Started(g) => *g,
        other => panic!("{other:?}"),
    };
    let foreign = giveaway_cmd(&mut sim, host, "end", vec![int_option("id", elsewhere.id)]).await;
    assert!(foreign.content().contains("no giveaway"), "{}", foreign.content());
    sim.app.db.abort_giveaway(elsewhere.id).await.unwrap();
    let forged = sim
        .component(alice, &format!("gw:e:{}", elsewhere.id), &[], &message)
        .await;
    assert!(forged.content().contains("no longer exists"), "{}", forged.content());
    let junk = sim.component(alice, "gw:e:x", &[], &message).await;
    assert!(junk.content().contains("expired"), "{}", junk.content());

    // Two ticks at once pay a giveaway once and announce it once.
    let (x, y) = (unique(), unique());
    giveaway_cmd(&mut sim, host, "start", giveaway_start("1h", 1, "Race", Some(25))).await;
    let posts = sim.posts();
    let race = reply_of(only(&posts, channel, Method::POST));
    sim.click(x, &race, "Enter").await;
    sim.click(y, &race, "Enter").await;
    assert_eq!(shells(&sim, host).await, 865);
    sim.app.shift_clock(chrono::Duration::hours(2));
    let (a, b) = tokio::join!(tick(&sim), tick(&sim));
    assert!(a + b >= 1, "somebody ended it");
    let balances = (shells(&sim, x).await, shells(&sim, y).await);
    assert!(balances == (25, 0) || balances == (0, 25), "paid once: {balances:?}");
    assert_eq!(shells(&sim, host).await, 865, "nothing came back: it was won");
    let posts = sim.posts();
    let announcements = posts
        .iter()
        .filter(|p| p.channel == channel && p.method == Method::POST)
        .count();
    assert_eq!(announcements, 1, "announced once");

    // Nobody entered: the whole prize goes back.
    giveaway_cmd(&mut sim, host, "start", giveaway_start("1h", 1, "Lonely", Some(30))).await;
    assert_eq!(shells(&sim, host).await, 835);
    sim.posts();
    sim.app.shift_clock(chrono::Duration::hours(2));
    tick(&sim).await;
    let posts = sim.posts();
    let result = only(&posts, channel, Method::POST);
    assert!(
        result.content().contains("Nobody entered") && result.content().contains("30 unclaimed shells"),
        "{}",
        result.content()
    );
    assert_eq!(shells(&sim, host).await, 865);

    // A pod that ended a giveaway and died before announcing it: the next tick announces it.
    giveaway_cmd(&mut sim, host, "start", giveaway_start("1h", 1, "Orphan", Some(5))).await;
    let posts = sim.posts();
    let orphan = reply_of(only(&posts, channel, Method::POST));
    let z = unique();
    sim.click(z, &orphan, "Enter").await;
    sim.posts();
    sim.app.shift_clock(chrono::Duration::hours(2));
    let mine = sim.app.db.running_giveaways(sim.guild).await.unwrap().remove(0);
    assert_eq!(mine.prize, "Orphan");
    let ended = sim
        .app
        .db
        .end_giveaway(mine.id, sim.app.now(), false, |pool, n| {
            pool.iter().copied().take(n).collect()
        })
        .await
        .unwrap()
        .expect("ended by the pod that then died");
    assert_eq!(ended.winners, [z]);
    assert!(in_channel(&sim.posts(), channel).is_empty());
    assert!(!sim.app.db.giveaway(mine.id).await.unwrap().unwrap().announced);
    tick(&sim).await;
    let posts = sim.posts();
    let late_news = only(&posts, channel, Method::POST);
    assert!(
        late_news.content().contains(&format!("<@{z}>")),
        "{}",
        late_news.content()
    );
    assert_eq!(shells(&sim, z).await, 5);
    tick(&sim).await;
    assert!(in_channel(&sim.posts(), channel).is_empty(), "announced once");
}

/// The ids of the 👍 and 👎 buttons of a suggestion message.
fn vote_buttons(reply: &Reply) -> (String, String) {
    let ids: Vec<String> = reply
        .controls()
        .filter_map(|c| match c {
            Component::Button(b) => b.custom_id.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(ids.len(), 2, "a suggestion has two buttons: {ids:?}");
    (ids[0].clone(), ids[1].clone())
}

/// The counts on the 👍 and 👎 buttons.
fn vote_counts(reply: &Reply) -> (String, String) {
    let labels: Vec<String> = reply
        .controls()
        .filter_map(|c| match c {
            Component::Button(b) => b.label.clone(),
            _ => None,
        })
        .collect();
    (labels[0].clone(), labels[1].clone())
}

#[tokio::test]
async fn suggestions_collect_votes_and_managers_decide() {
    let Some(mut sim) = Sim::new("suggestions_collect_votes_and_managers_decide").await else {
        return;
    };
    let (admin, author, second_author, alice, bob) = (unique(), unique(), unique(), unique(), unique());
    let (ideas, walled) = (unique(), unique());
    // The mock opens these members' DMs, whatever the level-up test does to the shared flag; the
    // second author has closed theirs.
    DM_OVERRIDES.lock().unwrap().insert(author, false);
    DM_OVERRIDES.lock().unwrap().insert(second_author, true);
    let suggest = |text: &str| json!([text_option("text", text)]);

    // Nothing to post to until a manager picks a channel.
    let unset = sim.command(author, "suggest", suggest("Add a garden")).await;
    assert!(unset.content().contains("aren't set up"), "{}", unset.content());
    assert!(unset.pings_nobody());

    sim.permissions = MANAGE_GUILD;
    let shown = sim.command(admin, "config", json!([sub("suggestions", vec![])])).await;
    assert!(shown.content().contains("**off**"), "{}", shown.content());
    let both = sim
        .command_resolved(
            admin,
            "config",
            json!([sub(
                "suggestions",
                vec![channel_option("channel", ideas), bool_option("off", true)]
            )]),
            resolved_channels(&[ideas]),
        )
        .await;
    assert!(both.content().contains("not both"), "{}", both.content());
    let set = sim
        .command_resolved(
            admin,
            "config",
            json!([sub("suggestions", vec![channel_option("channel", ideas)])]),
            resolved_channels(&[ideas]),
        )
        .await;
    assert!(
        set.content().contains(&format!("**on** · <#{ideas}>")),
        "{}",
        set.content()
    );
    let settings = sim.command(admin, "config", json!([sub("show", vec![])])).await;
    assert!(
        settings.field("Suggestions").contains(&format!("<#{ideas}>")),
        "{}",
        settings.field("Suggestions")
    );
    sim.permissions = 0;

    // A suggestion: mentions stripped, 👍 and 👎 buttons at zero, nobody pinged.
    let posted = sim
        .command(author, "suggest", suggest("Add a <@42> garden\n@everyone please"))
        .await;
    assert!(
        posted.ephemeral && posted.content().contains("up as **#1**"),
        "{}",
        posted.content()
    );
    let posts = sim.posts();
    let post = only(&posts, ideas, Method::POST);
    assert_eq!(post.title(), "Suggestion #1");
    assert_eq!(post.embed()["description"], "Add a garden\neveryone please");
    assert!(pings_nobody(post), "{}", post.body);
    let message = reply_of(post);
    assert_eq!(vote_counts(&message), ("0".to_owned(), "0".to_owned()));
    let (up, down) = vote_buttons(&message);
    assert_eq!(
        message.field("Suggested by"),
        format!("<@{author}>"),
        "the author is named, not pinged"
    );
    let slow = sim.command(author, "suggest", suggest("And a pond")).await;
    assert!(slow.content().contains("Give it a moment"), "{}", slow.content());

    // Votes are live on the buttons, one per member, switchable and withdrawable.
    let one = sim.component(alice, &up, &[], &message).await;
    assert_eq!(vote_counts(&one), ("1".to_owned(), "0".to_owned()));
    assert!(!one.ephemeral && one.pings_nobody());
    assert_eq!(one.title(), "Suggestion #1", "the text stays");
    let two = sim.component(bob, &up, &[], &one).await;
    assert_eq!(vote_counts(&two), ("2".to_owned(), "0".to_owned()));
    let switched = sim.component(alice, &down, &[], &two).await;
    assert_eq!(
        vote_counts(&switched),
        ("1".to_owned(), "1".to_owned()),
        "alice moved sides"
    );
    let again = sim.component(alice, &down, &[], &switched).await;
    assert_eq!(
        vote_counts(&again),
        ("1".to_owned(), "0".to_owned()),
        "the same side again takes it back"
    );
    let bob_again = sim.component(bob, &up, &[], &again).await;
    assert_eq!(vote_counts(&bob_again), ("0".to_owned(), "0".to_owned()));
    let revote = sim.component(bob, &up, &[], &bob_again).await;
    assert_eq!(vote_counts(&revote), ("1".to_owned(), "0".to_owned()));

    // Only managers decide.
    let refused = sim
        .command(alice, "suggestion", json!([sub("approve", vec![int_option("id", 1)])]))
        .await;
    assert!(refused.content().contains("Manage Server"), "{}", refused.content());
    sim.permissions = MANAGE_GUILD;
    let missing = sim
        .command(admin, "suggestion", json!([sub("approve", vec![int_option("id", 77)])]))
        .await;
    assert!(missing.content().contains("no suggestion #77"), "{}", missing.content());

    // Approving edits the message (colour, who, why, locked buttons) and DMs the author.
    let approved = sim
        .command(
            admin,
            "suggestion",
            json!([sub(
                "approve",
                vec![int_option("id", 1), text_option("reason", "Great <@5> idea")]
            )]),
        )
        .await;
    assert!(
        approved.content().contains("approved") && approved.content().contains("I told them"),
        "{}",
        approved.content()
    );
    let posts = sim.posts();
    let edit = only(&posts, ideas, Method::PATCH);
    assert_eq!(edit.message, post.message);
    assert_eq!(edit.embed()["color"], json!(0x06d6a0));
    assert_eq!(edit.field("Approved by"), format!("<@{admin}>"));
    assert_eq!(edit.field("Reason"), "Great idea");
    let locked: Vec<&Value> = edit.body["components"][0]["components"]
        .as_array()
        .unwrap()
        .iter()
        .collect();
    assert!(locked.iter().all(|b| b["disabled"] == json!(true)), "{locked:?}");
    assert_eq!(locked[0]["label"], "1", "the final count stays on the button");
    let dm = only(&posts, author, Method::POST);
    assert_eq!(dm.title(), "Your suggestion #1 was approved");
    assert!(
        dm.embed()["description"].as_str().unwrap().contains("Add a garden") && pings_nobody(dm),
        "{}",
        dm.body
    );
    assert_eq!(dm.field("Reason"), "Great idea");
    let late = sim.component(alice, &up, &[], &revote).await;
    assert!(
        late.ephemeral && late.content().contains("already decided"),
        "{}",
        late.content()
    );
    let twice = sim
        .command(admin, "suggestion", json!([sub("deny", vec![int_option("id", 1)])]))
        .await;
    assert!(twice.content().contains("already approved"), "{}", twice.content());

    // Denying with closed DMs still works; the manager is told the author was not reached.
    sim.permissions = 0;
    let second = sim
        .command(second_author, "suggest", suggest("Remove the garden"))
        .await;
    assert!(second.content().contains("up as **#2**"), "{}", second.content());
    sim.posts();
    sim.permissions = MANAGE_GUILD;
    let denied = sim
        .command(admin, "suggestion", json!([sub("deny", vec![int_option("id", 2)])]))
        .await;
    assert!(
        denied.content().contains("denied") && denied.content().contains("DMs are closed"),
        "{}",
        denied.content()
    );
    let posts = sim.posts();
    let edit = only(&posts, ideas, Method::PATCH);
    assert_eq!(edit.embed()["color"], json!(0xef476f));
    assert_eq!(edit.field("Denied by"), format!("<@{admin}>"));
    assert!(in_channel(&posts, second_author).is_empty());

    // Another server's suggestion cannot be voted on from here.
    let foreign = sim
        .app
        .db
        .create_suggestion(unique(), author, "Elsewhere")
        .await
        .unwrap();
    let forged = sim
        .component(alice, &format!("sg:v:{}:u", foreign.id), &[], &message)
        .await;
    assert!(forged.content().contains("no longer exists"), "{}", forged.content());
    let junk = sim.component(alice, "sg:v:x:u", &[], &message).await;
    assert!(junk.content().contains("expired"), "{}", junk.content());

    // A channel the bot cannot post in leaves no half-made suggestion behind.
    sim.permissions = MANAGE_GUILD;
    sim.command_resolved(
        admin,
        "config",
        json!([sub("suggestions", vec![channel_option("channel", walled)])]),
        resolved_channels(&[walled]),
    )
    .await;
    sim.block_channel(walled);
    sim.permissions = 0;
    let third = unique();
    let blocked = sim.command(third, "suggest", suggest("Anything at all")).await;
    assert!(blocked.content().contains("I can't post in"), "{}", blocked.content());
    let (stuck,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM suggestions WHERE guild_id = $1 AND message_id IS NULL")
            .bind(sim.guild as i64)
            .fetch_one(sim.app.db.pool())
            .await
            .unwrap();
    assert_eq!(stuck, 0);
}

/// `/poll question options [duration]`.
async fn poll_cmd(sim: &mut Sim, user: u64, question: &str, options: &str, duration: Option<&str>) -> Reply {
    let mut all = vec![text_option("question", question), text_option("options", options)];
    all.extend(duration.map(|d| text_option("duration", d)));
    sim.command(user, "poll", json!(all)).await
}

#[tokio::test]
async fn polls_collect_one_vote_each_and_close_into_a_chart() {
    let Some(mut sim) = Sim::new("polls_collect_one_vote_each_and_close_into_a_chart").await else {
        return;
    };
    let _turn = EVENTS.lock().await;
    sim.channel = unique();
    let channel = sim.channel;
    let author = unique();
    let [alice, bob, carol, dave, erin, frank, grace, zed] = std::array::from_fn(|_| unique());
    let tick = |sim: &Sim| {
        let app = sim.app.clone();
        async move { pinchora_commands::events_tick(&app).await.expect("tick") }
    };

    // Bad polls are refused before anything is posted.
    let long = format!("a | {}", "x".repeat(81));
    for (options, duration, expected) in [
        ("Only one", None, "two to five answers"),
        ("a | b | c | d | e | f", None, "two to five answers"),
        ("a | <@1> | ", None, "two to five answers"),
        ("Same | same", None, "different"),
        (long.as_str(), None, "80 characters"),
        ("a | b", Some("later"), "couldn't read"),
        ("a | b", Some("30s"), "1 minute to 14 days"),
        ("a | b", Some("20d"), "1 minute to 14 days"),
    ] {
        let bad = poll_cmd(&mut sim, author, "Question?", options, duration).await;
        assert!(bad.content().contains(expected), "{options:?}: {}", bad.content());
        assert!(bad.ephemeral && bad.pings_nobody());
    }
    let nothing = poll_cmd(&mut sim, author, "<@1>", "a | b", None).await;
    assert!(nothing.content().contains("Ask a question"), "{}", nothing.content());
    assert!(sim.posts().is_empty());

    // A poll: question and answers with mentions stripped, a button per answer and Results.
    let opened = poll_cmd(
        &mut sim,
        author,
        "Pizza <@7> or tacos? @everyone",
        "Pizza | Tacos | Sushi",
        Some("1h"),
    )
    .await;
    assert!(
        opened.ephemeral && opened.content().contains("is open in"),
        "{}",
        opened.content()
    );
    let posts = sim.posts();
    let post = only(&posts, channel, Method::POST);
    assert_eq!(post.title(), "Pizza or tacos? everyone");
    assert!(pings_nobody(post), "{}", post.body);
    let text = post.embed()["description"].as_str().unwrap();
    assert!(
        text.starts_with("**1** · Pizza\n**2** · Tacos\n**3** · Sushi")
            && text.contains(&format!("Asked by <@{author}>")),
        "{text}"
    );
    let message = reply_of(post);
    let labels: Vec<_> = message
        .controls()
        .filter_map(|c| match c {
            Component::Button(b) => b.label.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(labels, ["Pizza", "Tacos", "Sushi", "Results"]);
    let poll_id: i64 = message
        .button("Pizza")
        .custom_id
        .as_deref()
        .and_then(|id| id.split(':').nth(2))
        .and_then(|id| id.parse().ok())
        .expect("a poll id in the button");

    // One vote each: a first vote, a move, taking it back.
    let cast = sim.click(alice, &message, "Pizza").await;
    assert!(cast.ephemeral && cast.pings_nobody(), "answers are private and quiet");
    assert!(
        cast.content().contains("vote for **Pizza** is in"),
        "{}",
        cast.content()
    );
    let moved = sim.click(alice, &message, "Tacos").await;
    assert!(
        moved.content().contains("Moved your vote to **Tacos**"),
        "{}",
        moved.content()
    );
    sim.click(bob, &message, "Pizza").await;
    let withdrawn = sim.click(bob, &message, "Pizza").await;
    assert!(
        withdrawn.content().contains("Took your vote back"),
        "{}",
        withdrawn.content()
    );
    for (voter, answer) in [
        (carol, "Tacos"),
        (dave, "Sushi"),
        (erin, "Pizza"),
        (frank, "Pizza"),
        (grace, "Tacos"),
    ] {
        sim.click(voter, &message, answer).await;
    }
    assert_eq!(sim.app.db.poll_tally(poll_id, 3).await.unwrap(), [2, 3, 1]);

    // Results while it runs: a private card, drawn at most every ten seconds.
    let first = sim.click(zed, &message, "Results").await;
    assert!(first.ephemeral && first.pings_nobody(), "the card is private and quiet");
    assert_eq!(
        first.kind,
        InteractionResponseType::DeferredChannelMessageWithSource,
        "a new private message, not an edit of the poll"
    );
    assert_eq!((first.files.len(), first.files[0].0.as_str()), (1, "poll.png"));
    assert_png(&first, 900, 432);
    sim.click(zed, &message, "Sushi").await;
    let cached = sim.click(erin, &message, "Results").await;
    assert_eq!(cached.files[0].1, first.files[0].1, "the same card inside ten seconds");
    sim.app.shift_clock(chrono::Duration::seconds(11));
    let fresh = sim.click(erin, &message, "Results").await;
    assert_ne!(
        fresh.files[0].1, first.files[0].1,
        "a new card once ten seconds are over"
    );

    // Members have at most three polls open at once.
    let coffee = poll_cmd(&mut sim, author, "Coffee or tea?", "Coffee | Tea", Some("30m")).await;
    assert!(coffee.content().contains("is open in"), "{}", coffee.content());
    let cats = poll_cmd(&mut sim, author, "Cats or dogs?", "Cats | Dogs", Some("45m")).await;
    assert!(cats.content().contains("is open in"), "{}", cats.content());
    let posts = sim.posts();
    let cats_message = reply_of(
        posts
            .iter()
            .rfind(|p| p.channel == channel && p.title() == "Cats or dogs?")
            .expect("the cats poll"),
    );
    sim.click(alice, &cats_message, "Cats").await;
    sim.click(bob, &cats_message, "Dogs").await;
    let capped = poll_cmd(&mut sim, author, "One more?", "Yes | No", None).await;
    assert!(
        capped.content().contains("already have 3 polls open"),
        "{}",
        capped.content()
    );
    assert!(sim.posts().is_empty());

    // Time passes: the tick closes all three, once, each with its chart and a spoken result.
    sim.app.shift_clock(chrono::Duration::hours(2));
    let late = sim.click(zed, &message, "Tacos").await;
    assert!(late.content().contains("closed"), "{}", late.content());
    tick(&sim).await;
    let posts = sim.posts();
    let results: Vec<&Post> = posts
        .iter()
        .filter(|p| p.channel == channel && p.method == Method::POST)
        .collect();
    assert_eq!(results.len(), 3, "one result per poll");
    let by_title = |title: &str| {
        results
            .iter()
            .find(|p| p.title() == title)
            .unwrap_or_else(|| panic!("no result titled {title:?}"))
    };
    let pizza = by_title("Poll closed: Pizza or tacos? everyone");
    assert_eq!(pizza.embed()["description"], "**Tacos** wins with 3 votes (43%).");
    assert_eq!((pizza.files.len(), pizza.files[0].0.as_str()), (1, "poll.png"));
    assert!(pings_nobody(pizza), "{}", pizza.body);
    assert_eq!(
        by_title("Poll closed: Coffee or tea?").embed()["description"],
        "Nobody voted."
    );
    assert_eq!(
        by_title("Poll closed: Cats or dogs?").embed()["description"],
        "A tie between **Cats** and **Dogs** with 1 vote each."
    );
    let closed: Vec<&Post> = posts
        .iter()
        .filter(|p| p.channel == channel && p.method == Method::PATCH)
        .collect();
    assert_eq!(closed.len(), 3, "each poll message loses its buttons");
    for edit in &closed {
        assert_eq!(edit.body["components"], json!([]));
        assert!(
            edit.embed()["description"]
                .as_str()
                .unwrap()
                .contains("This poll is closed."),
            "{}",
            edit.embed()
        );
    }
    tick(&sim).await;
    assert!(
        in_channel(&sim.posts(), channel).is_empty(),
        "a poll closes exactly once"
    );

    // After closing: no votes, and Results is the final card.
    let after = sim.click(zed, &message, "Tacos").await;
    assert!(after.content().contains("has closed"), "{}", after.content());
    assert_eq!(sim.app.db.poll_tally(poll_id, 3).await.unwrap(), [2, 3, 2]);
    let final_card = sim.click(erin, &message, "Results").await;
    assert!(
        final_card.description().contains("closed"),
        "{}",
        final_card.description()
    );
    assert_eq!(final_card.files.len(), 1);

    // A forged id from another server's poll gives nothing.
    let foreign = match sim
        .app
        .db
        .create_poll(pinchora_db::NewPoll {
            guild_id: unique(),
            channel_id: unique(),
            author: unique(),
            question: "Elsewhere?",
            options: &["a".to_owned(), "b".to_owned()],
            ends_at: sim.app.now() + chrono::Duration::hours(1),
        })
        .await
        .unwrap()
    {
        pinchora_db::PollOpen::Started(p) => p,
        other => panic!("{other:?}"),
    };
    let forged = sim
        .component(alice, &format!("pl:v:{}:0", foreign.id), &[], &message)
        .await;
    assert!(forged.content().contains("no longer exists"), "{}", forged.content());
    let junk = sim.component(alice, "pl:v:x:0", &[], &message).await;
    assert!(junk.content().contains("expired"), "{}", junk.content());
    let beyond = sim.component(alice, &format!("pl:v:{poll_id}:4"), &[], &message).await;
    assert!(
        beyond.content().contains("expired"),
        "an option the poll does not have: {}",
        beyond.content()
    );

    // A pod that closed a poll and died before announcing it: the next tick announces it.
    poll_cmd(&mut sim, author, "Tea or juice?", "Tea | Juice", Some("1h")).await;
    let posts = sim.posts();
    let orphan = reply_of(only(&posts, channel, Method::POST));
    sim.click(alice, &orphan, "Tea").await;
    sim.app.shift_clock(chrono::Duration::hours(2));
    let orphan_id: i64 = orphan
        .button("Tea")
        .custom_id
        .as_deref()
        .and_then(|id| id.split(':').nth(2))
        .and_then(|id| id.parse().ok())
        .expect("a poll id");
    assert!(sim.app.db.close_poll(orphan_id, sim.app.now()).await.unwrap().is_some());
    assert!(in_channel(&sim.posts(), channel).is_empty());
    tick(&sim).await;
    let posts = sim.posts();
    let news = only(&posts, channel, Method::POST);
    assert_eq!(news.embed()["description"], "**Tea** wins with 1 vote (100%).");
    tick(&sim).await;
    assert!(in_channel(&sim.posts(), channel).is_empty(), "announced once");

    // A channel the bot cannot post in leaves no half-made poll behind.
    let walled = unique();
    sim.block_channel(walled);
    sim.channel = walled;
    let refused = poll_cmd(&mut sim, author, "Hello?", "Yes | No", None).await;
    assert!(refused.content().contains("I can't post in"), "{}", refused.content());
    let (stuck,): (i64,) = sqlx::query_as("SELECT count(*) FROM polls WHERE channel_id = $1")
        .bind(walled as i64)
        .fetch_one(sim.app.db.pool())
        .await
        .unwrap();
    assert_eq!(stuck, 0);
}

#[tokio::test]
async fn replies_after_a_defer_keep_allowed_mentions() {
    let Some(sim) = Sim::new("replies_after_a_defer_keep_allowed_mentions").await else {
        return;
    };
    let data = json!({ "id": unique().to_string(), "name": "x", "type": 1 });
    let interaction = sim.interaction(unique(), 2, data, None);
    let (tx, _rx) = oneshot::channel();
    let responder = Responder::http(sim.app.clone(), &interaction, tx);
    let answer = |text: &str, quiet: bool| twilight_model::http::interaction::InteractionResponseData {
        content: Some(text.to_owned()),
        allowed_mentions: quiet.then(twilight_model::channel::message::AllowedMentions::default),
        ..Default::default()
    };
    sim.calls.lock().unwrap().clear();
    responder.defer(false).await.expect("defer");
    responder
        .respond(answer("edit", true), false)
        .await
        .expect("edit the deferred answer");
    responder
        .respond(answer("follow-up", true), false)
        .await
        .expect("follow-up");
    responder
        .respond(answer("loud", false), false)
        .await
        .expect("follow-up");
    let calls = std::mem::take(&mut *sim.calls.lock().unwrap());
    assert_eq!(calls.len(), 3);
    assert!(calls[0].method == Method::PATCH && calls[0].path.ends_with("/messages/@original"));
    assert_eq!(
        calls[0].body["allowed_mentions"],
        json!({ "parse": [] }),
        "the edit of the deferred answer"
    );
    assert!(calls[1].method == Method::POST);
    assert_eq!(
        calls[1].body["allowed_mentions"],
        json!({ "parse": [] }),
        "the follow-up"
    );
    assert_eq!(
        calls[2].body["allowed_mentions"],
        json!({ "parse": [] }),
        "unset pings nobody: {}",
        calls[2].body
    );
}

/// A reply that says nothing about `allowed_mentions` pings nobody, however it reaches Discord; one that
/// names somebody to ping keeps that.
#[tokio::test]
async fn replies_ping_nobody_unless_they_say_whom() {
    let Some(sim) = Sim::new("replies_ping_nobody_unless_they_say_whom").await else {
        return;
    };
    let data = json!({ "id": unique().to_string(), "name": "x", "type": 1 });
    let interaction = sim.interaction(unique(), 2, data, None);
    let (tx, rx) = oneshot::channel();
    let responder = Responder::http(sim.app.clone(), &interaction, tx);
    let loud = |text: &str, whom: Option<u64>| twilight_model::http::interaction::InteractionResponseData {
        content: Some(format!("{text} <@5> <@&6> @everyone")),
        allowed_mentions: whom.map(|user| twilight_model::channel::message::AllowedMentions {
            users: vec![Id::new(user)],
            ..Default::default()
        }),
        ..Default::default()
    };
    sim.calls.lock().unwrap().clear();
    responder
        .respond(loud("first", None), false)
        .await
        .expect("the first answer");
    let first = rx.await.expect("the answer to Discord").data.expect("its data");
    responder
        .respond(loud("follow-up", None), false)
        .await
        .expect("a follow-up");
    responder
        .respond(loud("to one", Some(5)), false)
        .await
        .expect("a follow-up that pings");
    responder.edit(&loud("edit", None)).await.expect("an edit");
    responder
        .edit(&loud("edit to one", Some(5)))
        .await
        .expect("an edit that pings");
    let calls = std::mem::take(&mut *sim.calls.lock().unwrap());
    assert_eq!(
        calls.len(),
        4,
        "{:?}",
        calls.iter().map(|c| &c.path).collect::<Vec<_>>()
    );
    let quiet = json!({ "parse": [] });
    let to_five = json!({ "parse": [], "users": ["5"] });
    let first = serde_json::to_value(first.allowed_mentions).expect("mentions as JSON");
    assert_eq!(first, quiet, "the first answer");
    assert_eq!(calls[0].body["allowed_mentions"], quiet, "a follow-up");
    assert_eq!(calls[1].body["allowed_mentions"], to_five, "a follow-up that pings");
    assert_eq!(calls[2].body["allowed_mentions"], quiet, "an edit");
    assert_eq!(calls[3].body["allowed_mentions"], to_five, "an edit that pings");
}

// /Fishing

/// `/fish cast [spot]`.
fn fish_cast(spot: Option<&str>) -> Value {
    json!([sub("cast", spot.map(|s| text_option("spot", s)).into_iter().collect())])
}

fn fish_sub(name: &str) -> Value {
    json!([sub(name, Vec::new())])
}

/// The ids of every species in the game, as the catch counts the bucket by.
fn known_fish() -> Vec<&'static str> {
    Content::get().fish.iter().map(|f| f.id.as_str()).collect()
}

/// Total XP at the start of `level`.
fn xp_for_level(level: u32) -> i64 {
    (0..level).map(|l| pinchora_core::leveling::xp_to_next(l) as i64).sum()
}

fn footer(reply: &Reply) -> &str {
    reply
        .embeds
        .first()
        .and_then(|e| e.footer.as_ref())
        .map_or("", |f| f.text.as_str())
}

/// Puts a cast of `user` in the cache as if it had bitten `late_ms` ago. `state` is the part of the
/// value after the time (`spot:rod:bait`), or empty for a cast that carries none.
async fn seed_cast(sim: &Sim, user: u64, nonce: u32, late_ms: i64, state: &str) -> String {
    let at = chrono::Utc::now().timestamp_millis() - late_ms;
    let value = if state.is_empty() {
        at.to_string()
    } else {
        format!("{at}:{state}")
    };
    sim.app
        .cache
        .put(
            &format!("cast:{user}:{nonce}"),
            &value,
            std::time::Duration::from_secs(30),
        )
        .await
        .unwrap();
    format!("fr:{user}:{nonce}")
}

/// The reel button as the cast message carries it.
fn casting() -> Reply {
    Reply::new(InteractionResponseType::ChannelMessageWithSource)
}

/// The fish a catch embed names.
fn caught_fish(reply: &Reply) -> &'static pinchora_game::Fish {
    let title = reply.title();
    Content::get()
        .fish
        .iter()
        .find(|f| title.ends_with(&format!("{}!", f.name)))
        .unwrap_or_else(|| panic!("no fish in {title:?}"))
}

#[tokio::test]
async fn fishing_spots_open_by_level_and_the_last_one_is_remembered() {
    let Some(mut sim) = Sim::new("fishing_spots_open_by_level_and_the_last_one_is_remembered").await else {
        return;
    };
    let db = sim.app.db.clone();

    // A new player: the Reef is out of reach, and a refused cast costs nothing.
    let angler = unique();
    fund(&sim, angler, 500).await;
    let locked = sim.command(angler, "fish", fish_cast(Some("reef"))).await;
    assert!(locked.ephemeral);
    assert!(
        locked.content().contains("opens at level 5") && locked.content().contains("you are level 0"),
        "{}",
        locked.content()
    );
    assert_eq!(shells(&sim, angler).await, 500);
    assert_eq!(db.fishing_gear(angler).await.unwrap().spot, "shore");

    // Every deeper spot is shut one level short, whatever else the player has.
    for (level, spot) in [(4, "reef"), (11, "kelp_forest"), (19, "deep_sea"), (29, "abyss")] {
        let visitor = unique();
        fund(&sim, visitor, 50).await;
        db.add_message_xp(sim.guild, visitor, xp_for_level(level), 0)
            .await
            .unwrap();
        let shut = sim.command(visitor, "fish", fish_cast(Some(spot))).await;
        assert!(shut.content().contains("opens at level"), "{spot}: {}", shut.content());
        assert_eq!(shells(&sim, visitor).await, 50, "{spot}");
    }

    // The shore is open to everyone and costs the plain 10 shells of bait.
    let shore = sim.command(angler, "fish", fish_cast(None)).await;
    assert_eq!(shore.title(), "🎣 Line in the water…");
    assert_eq!(footer(&shore), "Shore · Driftwood Rod · Plain bait");
    assert_eq!(shells(&sim, angler).await, 490);

    // Level 5 opens the reef; choosing it remembers it.
    let reef_fan = unique();
    fund(&sim, reef_fan, 50).await;
    db.add_message_xp(sim.guild, reef_fan, xp_for_level(5), 0)
        .await
        .unwrap();
    let reef = sim.command(reef_fan, "fish", fish_cast(Some("reef"))).await;
    assert_eq!(footer(&reef), "Reef · Driftwood Rod · Plain bait");
    assert_eq!(db.fishing_gear(reef_fan).await.unwrap().spot, "reef");

    // A bare /fish cast goes where the player went last.
    let regular = unique();
    fund(&sim, regular, 50).await;
    db.add_message_xp(sim.guild, regular, xp_for_level(12), 0)
        .await
        .unwrap();
    db.set_fishing_spot(regular, "kelp_forest").await.unwrap();
    let kelp = sim.command(regular, "fish", fish_cast(None)).await;
    assert_eq!(footer(&kelp), "Kelp Forest · Driftwood Rod · Plain bait");

    // A remembered spot the player has no level for falls back to the deepest one they do have.
    let strayed = unique();
    fund(&sim, strayed, 50).await;
    db.set_fishing_spot(strayed, "abyss").await.unwrap();
    let back = sim.command(strayed, "fish", fish_cast(None)).await;
    assert_eq!(footer(&back), "Shore · Driftwood Rod · Plain bait");
    assert_eq!(db.fishing_gear(strayed).await.unwrap().spot, "shore");

    // Level 30 opens the abyss.
    let diver = unique();
    fund(&sim, diver, 50).await;
    db.add_message_xp(sim.guild, diver, xp_for_level(30), 0).await.unwrap();
    let abyss = sim.command(diver, "fish", fish_cast(Some("abyss"))).await;
    assert_eq!(footer(&abyss), "Abyss · Driftwood Rod · Plain bait");

    // The gear panel lists every spot, the closed ones with a lock and the level they need.
    let panel = sim.command(angler, "fish", fish_sub("gear")).await;
    assert!(panel.ephemeral);
    let spots: Vec<_> = panel
        .options("fx:s:")
        .iter()
        .map(|o| {
            (
                o.label.as_str(),
                o.description.as_deref().unwrap_or_default(),
                o.default,
            )
        })
        .collect();
    assert_eq!(
        spots.iter().map(|(label, ..)| *label).collect::<Vec<_>>(),
        ["Shore", "🔒 Reef", "🔒 Kelp Forest", "🔒 Deep Sea", "🔒 Abyss"]
    );
    assert_eq!(spots[3].1, "Opens at level 20");
    assert!(spots[0].2 && !spots[1].2, "the spot in use is the default");
    assert!(panel.field("Spot").contains("**Shore**"), "{}", panel.field("Spot"));
    let refused = sim.pick(angler, &panel, "fx:s:", "deep_sea").await;
    assert!(
        refused.description().contains("opens at level 20 and you are level 0"),
        "{}",
        refused.description()
    );
    assert_eq!(db.fishing_gear(angler).await.unwrap().spot, "shore");
    // At level 20 the same pick works.
    db.add_message_xp(sim.guild, angler, xp_for_level(20), 0).await.unwrap();
    let opened = sim.pick(angler, &refused, "fx:s:", "deep_sea").await;
    assert!(
        opened.description().contains("Fishing at the **Deep Sea**"),
        "{}",
        opened.description()
    );
    assert_eq!(db.fishing_gear(angler).await.unwrap().spot, "deep_sea");
    assert!(
        opened
            .options("fx:s:")
            .iter()
            .any(|o| o.label == "Deep Sea" && o.default)
    );
    assert!(opened.options("fx:s:").iter().any(|o| o.label == "🔒 Abyss"));
    // Somebody else's panel is not theirs to use.
    let stranger = sim
        .component(unique(), &opened.menu("fx:s:").custom_id.clone(), &["reef"], &opened)
        .await;
    assert!(
        stranger.content().contains("someone else's panel"),
        "{}",
        stranger.content()
    );
    assert_eq!(db.fishing_gear(angler).await.unwrap().spot, "deep_sea");

    // The bites come on their own a few seconds after each cast; let them land before the test ends.
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
}

#[tokio::test]
async fn rods_and_bait_are_bought_equipped_and_used_up() {
    let Some(mut sim) = Sim::new("rods_and_bait_are_bought_equipped_and_used_up").await else {
        return;
    };
    let db = sim.app.db.clone();
    let angler = unique();
    fund(&sim, angler, 1_000).await;

    // The shop: five rods (the free one is not for sale) and three baits on a card.
    let shop = sim.command(angler, "fish", fish_sub("shop")).await;
    assert!(shop.ephemeral);
    let card = image::load_from_memory(&shop.files[0].1).expect("a png");
    assert_eq!((card.width(), card.height()), (1_000, 600));
    let values = |reply: &Reply| -> Vec<String> { reply.options("fx:p:").iter().map(|o| o.value.clone()).collect() };
    assert_eq!(
        values(&shop),
        [
            "r:reedline_rod",
            "r:pearlwhip_rod",
            "r:tidecaller_rod",
            "r:starweave_rod",
            "b:glow_worm",
            "b:silver_lure",
            "b:tide_crumbs"
        ]
    );

    // Buying a rod: the shells go, the rod is theirs, and it leaves the menu.
    let bought = sim.pick(angler, &shop, "fx:p:", "r:reedline_rod").await;
    assert!(
        bought
            .description()
            .contains("Bought the **Reedline Rod** for 800 shells"),
        "{}",
        bought.description()
    );
    assert_eq!(shells(&sim, angler).await, 200);
    assert!(!values(&bought).contains(&"r:reedline_rod".to_owned()));
    assert_eq!(db.owned_rods(angler).await.unwrap(), ["reedline_rod"]);
    // Too dear: nothing is taken.
    let poor = sim.pick(angler, &bought, "fx:p:", "r:tidecaller_rod").await;
    assert!(
        poor.description().contains("costs 12,000 shells and you have 200"),
        "{}",
        poor.description()
    );
    assert_eq!(shells(&sim, angler).await, 200);
    // A rod bought twice (an old menu) is not charged twice.
    let again = sim
        .component(
            angler,
            &bought.menu("fx:p:").custom_id.clone(),
            &["r:reedline_rod"],
            &bought,
        )
        .await;
    assert!(again.description().contains("already own"), "{}", again.description());
    assert_eq!(shells(&sim, angler).await, 200);
    // Neither is a rod that does not exist, nor the free one.
    for value in ["r:driftwood_rod", "r:gold_rod", "b:dragon_bait", "x:y"] {
        let nothing = sim
            .component(angler, &bought.menu("fx:p:").custom_id.clone(), &[value], &bought)
            .await;
        assert!(
            nothing.description().contains("not in the shop") || nothing.description().contains("expired"),
            "{value}: {}",
            nothing.description()
        );
    }
    assert_eq!(shells(&sim, angler).await, 200);

    // Bait comes in packs of five and adds up.
    let worms = sim.pick(angler, &bought, "fx:p:", "b:glow_worm").await;
    assert!(
        worms
            .description()
            .contains("Bought 5 **Glow Worm** for 65 shells; you have 5"),
        "{}",
        worms.description()
    );
    let worms = sim.pick(angler, &worms, "fx:p:", "b:glow_worm").await;
    assert!(worms.description().contains("you have 10"), "{}", worms.description());
    assert_eq!(shells(&sim, angler).await, 70);
    let broke = sim.pick(angler, &worms, "fx:p:", "b:silver_lure").await;
    assert!(
        broke.description().contains("costs 140 shells and you have 70"),
        "{}",
        broke.description()
    );
    // The shop is the player's own.
    let stranger = sim
        .component(
            unique(),
            &broke.menu("fx:p:").custom_id.clone(),
            &["b:glow_worm"],
            &broke,
        )
        .await;
    assert!(
        stranger.content().contains("someone else's panel"),
        "{}",
        stranger.content()
    );
    assert_eq!(shells(&sim, angler).await, 70);

    // /fish gear: only owned rods and bait are offered; putting them on is saved.
    let gear = sim.command(angler, "fish", fish_sub("gear")).await;
    let labels = |reply: &Reply, prefix: &str| -> Vec<String> {
        reply.options(prefix).iter().map(|o| o.label.clone()).collect()
    };
    assert_eq!(labels(&gear, "fx:r:"), ["Driftwood Rod", "Reedline Rod"]);
    assert_eq!(labels(&gear, "fx:b:"), ["Plain bait", "Glow Worm ×10"]);
    assert!(gear.field("Rod").contains("**Driftwood Rod**"), "{}", gear.field("Rod"));
    assert!(gear.field("Bait").contains("Plain bait"), "{}", gear.field("Bait"));
    let rod = sim.pick(angler, &gear, "fx:r:", "reedline_rod").await;
    assert!(
        rod.description().contains("**Reedline Rod** is on the line"),
        "{}",
        rod.description()
    );
    assert!(rod.field("Rod").contains("reel 2.8 s") && rod.field("Rod").contains("rare luck +6%"));
    let hook = sim.pick(angler, &rod, "fx:b:", "glow_worm").await;
    assert!(
        hook.description().contains("**Glow Worm** is on the hook"),
        "{}",
        hook.description()
    );
    assert!(hook.field("Bait").contains("×10"), "{}", hook.field("Bait"));
    let on_line = db.fishing_gear(angler).await.unwrap();
    assert_eq!(
        (on_line.rod.as_str(), on_line.bait.as_deref()),
        ("reedline_rod", Some("glow_worm"))
    );
    // A hand-made pick of gear they do not have is refused.
    let fake = sim
        .component(angler, &hook.menu("fx:r:").custom_id.clone(), &["starweave_rod"], &hook)
        .await;
    assert!(fake.description().contains("not yours yet"), "{}", fake.description());
    let fake = sim
        .component(angler, &hook.menu("fx:b:").custom_id.clone(), &["tide_crumbs"], &hook)
        .await;
    assert!(
        fake.description().contains("no Tide Crumbs left"),
        "{}",
        fake.description()
    );
    assert_eq!(db.fishing_gear(angler).await.unwrap().rod, "reedline_rod");

    // A cast with bait on the hook uses one piece and no plain bait.
    let cast = sim.command(angler, "fish", fish_cast(None)).await;
    assert_eq!(footer(&cast), "Shore · Reedline Rod · Glow Worm ×9 left");
    assert_eq!(shells(&sim, angler).await, 70, "the worm paid for the cast");
    assert_eq!(db.bait_stock(angler).await.unwrap(), [("glow_worm".to_owned(), 9)]);
    // Back to plain bait.
    let plain = sim.pick(angler, &hook, "fx:b:", "plain").await;
    assert!(
        plain.description().contains("Plain bait it is"),
        "{}",
        plain.description()
    );
    assert_eq!(db.fishing_gear(angler).await.unwrap().bait, None);

    // The last piece of bait is used and the hook goes back to plain bait, which costs shells.
    let thrifty = unique();
    fund(&sim, thrifty, 100).await;
    db.buy_bait(thrifty, "glow_worm", 1, 1).await.unwrap();
    db.equip_bait(thrifty, Some("glow_worm")).await.unwrap();
    let last = sim.command(thrifty, "fish", fish_cast(None)).await;
    assert_eq!(footer(&last), "Shore · Driftwood Rod · Glow Worm ×0 left");
    assert_eq!(db.fishing_gear(thrifty).await.unwrap().bait, None);
    let broke_bait = unique();
    let none = sim.command(broke_bait, "fish", fish_cast(None)).await;
    assert!(
        none.content().contains("Plain bait costs 10 shells"),
        "no shells, no cast: {}",
        none.content()
    );

    // The window to Reel! is the rod's. 2.6 s after the bite is too slow for the starter rod...
    let slow = unique();
    let id = seed_cast(&sim, slow, 1, 2_600, "").await;
    let missed = sim.component(slow, &id, &[], &casting()).await;
    assert_eq!(missed.title(), "Too slow!");
    // ...but fine for the Reedline Rod, whichever way the cast carries it.
    let id = seed_cast(&sim, angler, 2, 2_600, "").await;
    let caught = sim.component(angler, &id, &[], &casting()).await;
    assert!(caught.title().contains("You caught"), "{}", caught.title());
    // A cast remembers the rod it was made with: a rod swapped in afterwards changes nothing.
    let swapper = unique();
    let id = seed_cast(&sim, swapper, 3, 3_200, "shore:starweave_rod:-").await;
    let caught = sim.component(swapper, &id, &[], &casting()).await;
    assert!(caught.title().contains("You caught"), "{}", caught.title());
    let id = seed_cast(&sim, swapper, 4, 2_600, "shore:driftwood_rod:-").await;
    assert_eq!(sim.component(swapper, &id, &[], &casting()).await.title(), "Too slow!");
    // Too early is still too early, and somebody else's line is theirs.
    let id = seed_cast(&sim, swapper, 5, -800, "shore:driftwood_rod:-").await;
    assert_eq!(sim.component(swapper, &id, &[], &casting()).await.title(), "Too early!");
    let id = seed_cast(&sim, swapper, 6, 100, "").await;
    let theirs = sim.component(unique(), &id, &[], &casting()).await;
    assert!(theirs.content().contains("someone else's line"), "{}", theirs.content());

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
}

#[tokio::test]
async fn a_catch_has_a_size_waits_in_the_bucket_and_beats_records() {
    let Some(mut sim) = Sim::new("a_catch_has_a_size_waits_in_the_bucket_and_beats_records").await else {
        return;
    };
    let db = sim.app.db.clone();
    let content = Content::get();

    // A first catch: a fish of the shore, sized inside its range, put in the bucket and not sold.
    let angler = unique();
    fund(&sim, angler, 100).await;
    // The reel is this player's first interaction: read their language now, as their cast would
    // have, so the reaction time measures the click and not that read.
    sim.app.user_lang_choice(angler).await;
    let id = seed_cast(&sim, angler, 1, 400, "shore:driftwood_rod:-").await;
    let catch = sim.component(angler, &id, &[], &casting()).await;
    let fish = caught_fish(&catch);
    assert!(
        fish.spots.contains_key("shore"),
        "{} does not live at the shore",
        fish.id
    );
    assert!(catch.title().starts_with("🆕 You caught a"), "{}", catch.title());
    assert!(
        catch.title().contains(&fish.rarity.label().to_lowercase()),
        "{}",
        catch.title()
    );
    assert_eq!(catch.files[0].0, "catch.png");
    let bucket = db.bucket(angler).await.unwrap();
    assert_eq!(bucket.len(), 1);
    let size = bucket[0].size_cm;
    assert!(
        size >= fish.min_cm && size <= fish.max_cm,
        "{} cm for {}",
        size,
        fish.id
    );
    assert!(
        catch.description().contains(&format!("**{size:.1} cm**")) && catch.description().contains("in your bucket"),
        "{}",
        catch.description()
    );
    assert!(catch.description().contains("A new species for your fish book!"));
    assert!(
        footer(&catch).starts_with("Shore · Reaction 0.") && footer(&catch).ends_with("Fish book 1/72 · Bucket 1/60"),
        "{}",
        footer(&catch)
    );
    assert_eq!(shells(&sim, angler).await, 100, "a catch pays nothing until it is sold");
    let book = db.fish_book(angler).await.unwrap();
    assert_eq!(
        (book[0].fish_id.as_str(), book[0].caught, book[0].best_cm),
        (fish.id.as_str(), 1, size)
    );
    // The same fish worked out by hand: what it will sell for.
    let value = pinchora_game::fishing::sale_value(fish, size);
    assert!(
        catch.description().contains(&format!("worth **{value}** shells")),
        "{}",
        catch.description()
    );
    // Boots and tins are no catch for the weekly board; everything else is.
    let week = pinchora_core::leveling::week_start(sim.app.now());
    let board = db.weekly_catch_board(sim.guild, week, 10, 0).await.unwrap();
    assert_eq!(board.len(), usize::from(!fish.junk), "{}", fish.id);

    // A bigger one of a species they know is a record: every shore fish is in this player's book,
    // each at a tiny size, so whatever bites is a record.
    let veteran = unique();
    for fish in content.fish.iter().filter(|f| f.spots.contains_key("shore")) {
        db.log_catch(veteran, &fish.id, 0.1).await.unwrap();
    }
    let id = seed_cast(&sim, veteran, 2, 400, "shore:driftwood_rod:-").await;
    let record = sim.component(veteran, &id, &[], &casting()).await;
    let fish = caught_fish(&record);
    assert!(!record.title().starts_with("🆕"), "{}", record.title());
    assert!(
        record
            .description()
            .contains("🏆 A new record! Your biggest was 0.1 cm."),
        "{}",
        record.description()
    );
    let entry = db
        .fish_book(veteran)
        .await
        .unwrap()
        .into_iter()
        .find(|e| e.fish_id == fish.id)
        .unwrap();
    assert_eq!(entry.caught, 2);
    assert!(entry.best_cm > 0.1);

    // A catch is a catch even with no server (a DM has none to rank in): nothing breaks without one.
    // The bucket holds 60 fish; a full one stops the next cast, for free, until something is sold.
    let hoarder = unique();
    fund(&sim, hoarder, 100).await;
    for n in 0..60 {
        db.record_catch(
            hoarder,
            pinchora_db::NewCatch {
                fish_id: &Content::get().fish[n % 7].id,
                size_cm: 10.0,
                ratio: 1.0,
                weekly: None,
                milestones: &[],
                known: &known_fish(),
            },
        )
        .await
        .unwrap();
    }
    let full = sim.command(hoarder, "fish", fish_cast(None)).await;
    assert!(
        full.ephemeral && full.content().contains("bucket is full (60/60)"),
        "{}",
        full.content()
    );
    assert_eq!(shells(&sim, hoarder).await, 100);
    // Fish of a species the game dropped do not count towards the 60: a bucket of them casts freely.
    let strayer = unique();
    fund(&sim, strayer, 100).await;
    for n in 0..60 {
        db.record_catch(
            strayer,
            pinchora_db::NewCatch {
                fish_id: &format!("vanished{}", n % 7),
                size_cm: 10.0,
                ratio: 1.0,
                weekly: None,
                milestones: &[],
                known: &known_fish(),
            },
        )
        .await
        .unwrap();
    }
    assert_eq!(db.bucket(strayer).await.unwrap().len(), 60);
    let freely = sim.command(strayer, "fish", fish_cast(None)).await;
    assert_eq!(freely.title(), "🎣 Line in the water…", "{}", freely.content());
    // Their bucket is the player's own: it fills with a catch made right at the brim, then warns.
    let last = unique();
    for n in 0..59 {
        db.record_catch(
            last,
            pinchora_db::NewCatch {
                fish_id: &Content::get().fish[n % 7].id,
                size_cm: 10.0,
                ratio: 1.0,
                weekly: None,
                milestones: &[],
                known: &known_fish(),
            },
        )
        .await
        .unwrap();
    }
    let id = seed_cast(&sim, last, 3, 300, "shore:driftwood_rod:-").await;
    let brim = sim.component(last, &id, &[], &casting()).await;
    assert!(
        brim.description()
            .contains("Your bucket is full. Sell some fish with `/fish sell`."),
        "{}",
        brim.description()
    );
    assert!(footer(&brim).ends_with("Bucket 60/60"), "{}", footer(&brim));
}

#[tokio::test]
async fn book_milestones_pay_shells_and_a_cosmetic_under_the_catch() {
    let Some(mut sim) = Sim::new("book_milestones_pay_shells_and_a_cosmetic_under_the_catch").await else {
        return;
    };
    let db = sim.app.db.clone();

    // The tenth species pays 300 shells and a badge, in a second embed under the catch.
    let tenth = unique();
    for n in 0..9 {
        db.log_catch(tenth, &format!("old{n}"), 5.0).await.unwrap();
    }
    let id = seed_cast(&sim, tenth, 1, 400, "shore:driftwood_rod:-").await;
    let catch = sim.component(tenth, &id, &[], &casting()).await;
    assert_eq!(catch.embeds.len(), 2, "the catch and the milestone");
    assert_eq!(
        catch.embeds[1].title.as_deref(),
        Some("📖 Fish book milestone: 10 species!")
    );
    let text = catch.embeds[1].description.as_deref().unwrap_or_default();
    assert!(
        text.contains("**+300 shells**")
            && text.contains("**Pocket Puffer** badge")
            && text.contains("/customize badges"),
        "{text}"
    );
    assert_eq!(shells(&sim, tenth).await, 300);
    assert_eq!(db.owned_badges(tenth).await.unwrap(), ["pocket_puffer"]);
    // The next catch pays nothing more.
    let id = seed_cast(&sim, tenth, 2, 400, "shore:driftwood_rod:-").await;
    let next = sim.component(tenth, &id, &[], &casting()).await;
    assert_eq!(
        next.embeds.len(),
        1,
        "{:?}",
        next.embeds.iter().map(|e| e.title.clone()).collect::<Vec<_>>()
    );
    assert_eq!(shells(&sim, tenth).await, 300);

    // The whole book: 71 species and a 72nd. A player who never claimed anything is paid the lot.
    let complete = unique();
    for n in 0..71 {
        db.log_catch(complete, &format!("old{n}"), 5.0).await.unwrap();
    }
    let id = seed_cast(&sim, complete, 3, 400, "shore:driftwood_rod:-").await;
    let all = sim.component(complete, &id, &[], &casting()).await;
    let titles: Vec<_> = all.embeds[1..].iter().filter_map(|e| e.title.as_deref()).collect();
    assert_eq!(
        titles,
        [
            "📖 Fish book milestone: 10 species!",
            "📖 Fish book milestone: 25 species!",
            "📖 Fish book milestone: 50 species!",
            "📖 Fish book milestone: 72 species!"
        ]
    );
    assert_eq!(shells(&sim, complete).await, 300 + 1_000 + 3_000 + 10_000);
    assert_eq!(
        db.owned_badges(complete).await.unwrap(),
        ["lantern_jelly", "pocket_puffer"]
    );
    assert_eq!(
        db.owned_backgrounds(complete).await.unwrap(),
        ["brinebell_market", "moonwhale_crossing"]
    );
    assert!(
        all.embeds[2]
            .description
            .as_deref()
            .unwrap_or_default()
            .contains("**Brinebell Market** profile background"),
        "{:?}",
        all.embeds[2].description
    );
    // The rewards are the player's to wear: the background editor lists them as owned.
    let editor = sim
        .command(complete, "customize", json!([sub("background", Vec::new())]))
        .await;
    assert!(
        editor
            .options("cz:")
            .iter()
            .any(|o| o.value.contains("moonwhale_crossing")),
        "{:?}",
        editor.options("cz:").iter().map(|o| &o.value).collect::<Vec<_>>()
    );

    // A reward they already own (bought earlier) is said so, and not given twice.
    let owner = unique();
    fund(&sim, owner, 5_000).await;
    db.buy_badge(owner, "pocket_puffer", 2_000).await.unwrap();
    for n in 0..9 {
        db.log_catch(owner, &format!("old{n}"), 5.0).await.unwrap();
    }
    let id = seed_cast(&sim, owner, 4, 400, "shore:driftwood_rod:-").await;
    let had = sim.component(owner, &id, &[], &casting()).await;
    assert!(
        had.embeds[1]
            .description
            .as_deref()
            .unwrap_or_default()
            .contains("which you already had"),
        "{:?}",
        had.embeds[1].description
    );
    assert_eq!(shells(&sim, owner).await, 3_000 + 300);

    // /fish book: every species with its record, the unknown ones as shadows with where they live.
    let reader = unique();
    let content = Content::get();
    for (n, fish) in content.fish.iter().take(12).enumerate() {
        db.log_catch(
            reader,
            &fish.id,
            fish.min_cm + (fish.max_cm - fish.min_cm) * (n as f32 / 12.0),
        )
        .await
        .unwrap();
    }
    db.log_catch(reader, &content.fish[2].id, content.fish[2].max_cm)
        .await
        .unwrap();
    let book = sim.command(reader, "fish", fish_sub("book")).await;
    assert!(!book.ephemeral, "a book is for showing off");
    assert_eq!(book.title(), "📖 Fish book");
    let lines: Vec<_> = book.description().lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(
        lines[0].starts_with("✅ **10 species**") && lines[1].starts_with("▫️ **25 species**"),
        "{lines:?}"
    );
    assert!(lines[3].contains("Moonwhale Crossing background"), "{lines:?}");
    assert!(book.button("◀ Page").disabled && !book.button("Page ▶").disabled);
    let page_2 = sim.click(reader, &book, "Page ▶").await;
    assert!(!page_2.button("◀ Page").disabled);
    let (first, second) = (
        image::load_from_memory(&book.files[0].1).unwrap(),
        image::load_from_memory(&page_2.files[0].1).unwrap(),
    );
    assert_eq!((first.width(), first.height()), (1_000, 600));
    assert_ne!(first.to_rgba8(), second.to_rgba8());
    // Somebody else's book is not theirs to page through.
    let stranger = sim.click(unique(), &page_2, "◀ Page").await;
    assert!(
        stranger.content().contains("someone else's book"),
        "{}",
        stranger.content()
    );
    // Pages past the end stop at the last one.
    let far = sim.component(reader, &format!("fz:b:{reader}:99"), &[], &page_2).await;
    assert!(far.button("Page ▶").disabled, "the last page has no next");
}

/// A catch in the weekly board's terms, put away with the real size rules.
async fn catch_for_board(sim: &Sim, user: u64, fish: &str, size: f32, week: chrono::NaiveDate) {
    let fish = Content::get().fish(fish).expect("a fish");
    sim.app
        .db
        .record_catch(
            user,
            pinchora_db::NewCatch {
                fish_id: &fish.id,
                size_cm: size,
                ratio: f64::from(pinchora_game::fishing::size_ratio(fish, size)),
                weekly: Some((sim.guild, week)),
                milestones: &[],
                known: &known_fish(),
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn selling_empties_the_bucket_for_shells_and_keeps_the_book() {
    let Some(mut sim) = Sim::new("selling_empties_the_bucket_for_shells_and_keeps_the_book").await else {
        return;
    };
    let db = sim.app.db.clone();
    let content = Content::get();
    let value = |fish: &str, size: f32| pinchora_game::fishing::sale_value(content.fish(fish).unwrap(), size);

    // Nine species in the bucket, three of them more than once: two pages of the card.
    let seller = unique();
    fund(&sim, seller, 9_990).await;
    let catches: [(&str, f32); 14] = [
        ("pearlpatch_koi", 66.0),
        ("pearlpatch_koi", 50.0),
        ("pearlpatch_koi", 31.0),
        ("reefruff_lionfish", 36.0),
        ("reefruff_lionfish", 20.0),
        ("toaststripe_minnow", 7.9),
        ("toaststripe_minnow", 4.0),
        ("pebblelip_dace", 9.0),
        ("crumbfin_bream", 15.0),
        ("honeypatch_tench", 30.0),
        ("orbitglow_oarfish", 300.0),
        ("aurorawoven_leviathan", 600.0),
        ("siltstep_boot", 25.0),
        ("mossmosaic_lobster", 40.0),
    ];
    for (fish, size) in catches {
        catch_for_board(
            &sim,
            seller,
            fish,
            size,
            chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(),
        )
        .await;
    }
    let everything: i64 = catches.iter().map(|(f, s)| value(f, *s)).sum();
    let spare: i64 = [
        ("pearlpatch_koi", 50.0),
        ("pearlpatch_koi", 31.0),
        ("reefruff_lionfish", 20.0),
        ("toaststripe_minnow", 4.0),
    ]
    .iter()
    .map(|(f, s)| value(f, *s))
    .sum();

    let bucket = sim.command(seller, "fish", fish_sub("sell")).await;
    assert!(bucket.ephemeral);
    assert!(
        bucket
            .description()
            .contains("Sell it all, or keep the biggest of each kind"),
        "{}",
        bucket.description()
    );
    assert_eq!(image::load_from_memory(&bucket.files[0].1).unwrap().height(), 600);
    let all_label = format!("Sell all (14 · {} shells)", pinchora_render::thousands(everything));
    let dup_label = format!("Sell duplicates (4 · {} shells)", pinchora_render::thousands(spare));
    assert!(!bucket.button(&all_label).disabled && !bucket.button(&dup_label).disabled);
    assert!(
        bucket.button("◀").disabled && !bucket.button("▶").disabled,
        "ten species make two pages"
    );
    let page_2 = sim.click(seller, &bucket, "▶").await;
    assert!(!page_2.button("◀").disabled && page_2.button("▶").disabled);
    assert_ne!(bucket.files[0].1, page_2.files[0].1);
    let back = sim.click(seller, &page_2, "◀").await;
    assert!(back.button("◀").disabled);

    // Duplicates go, the biggest of each kind stays.
    let before = shells(&sim, seller).await;
    let sold = sim.click(seller, &back, &dup_label).await;
    assert!(
        sold.description().contains(&format!(
            "Sold **4 fish** for **{} shells**. You have {}.",
            pinchora_render::thousands(spare),
            pinchora_render::thousands(before + spare)
        )),
        "{}",
        sold.description()
    );
    assert_eq!(shells(&sim, seller).await, before + spare);
    let left = db.bucket(seller).await.unwrap();
    assert_eq!(left.len(), 10);
    let koi = left.iter().find(|r| r.fish_id == "pearlpatch_koi").unwrap();
    assert_eq!(koi.size_cm, 66.0, "the biggest koi is the one kept");
    assert_eq!(
        db.fish_book(seller).await.unwrap().len(),
        10,
        "the book keeps every species"
    );
    let book_koi = db
        .fish_book(seller)
        .await
        .unwrap()
        .into_iter()
        .find(|b| b.fish_id == "pearlpatch_koi")
        .unwrap();
    assert_eq!((book_koi.caught, book_koi.best_cm), (3, 66.0));
    // Pocket Rattle: the shells took the wallet over 10,000.
    assert!(
        sold.followups
            .iter()
            .flat_map(|f| f.embeds.iter())
            .any(|e| e.title.as_deref() == Some("Badge unlocked: Pocket Rattle")),
        "{:?}",
        sold.followups
            .iter()
            .flat_map(|f| f.embeds.iter())
            .map(|e| e.title.clone())
            .collect::<Vec<_>>()
    );
    // Nothing is a duplicate any more, and an old duplicates button sells nothing.
    assert!(sold.button("Sell duplicates (0 · 0 shells)").disabled);
    let stale = sim.component(seller, &format!("fz:s:{seller}:dup"), &[], &sold).await;
    assert!(
        stale.description().contains("Nothing to sell right now."),
        "{}",
        stale.description()
    );
    assert_eq!(db.bucket_count(seller, &known_fish()).await.unwrap(), 10);

    // Somebody else's bucket is not theirs to sell.
    let thief = sim
        .click(
            unique(),
            &sold,
            &format!(
                "Sell all (10 · {} shells)",
                pinchora_render::thousands(everything - spare)
            ),
        )
        .await;
    assert!(thief.content().contains("someone else's bucket"), "{}", thief.content());
    assert_eq!(db.bucket_count(seller, &known_fish()).await.unwrap(), 10);

    // Sell the rest.
    let rest = everything - spare;
    let all = sim
        .click(
            seller,
            &sold,
            &format!("Sell all (10 · {} shells)", pinchora_render::thousands(rest)),
        )
        .await;
    assert!(all.description().contains("Sold **10 fish**"), "{}", all.description());
    assert_eq!(shells(&sim, seller).await, before + everything);
    assert_eq!(db.bucket_count(seller, &known_fish()).await.unwrap(), 0);
    assert!(all.button("Sell all (0 · 0 shells)").disabled);
    assert_eq!(db.fish_book(seller).await.unwrap().len(), 10);
    assert_eq!(
        db.user(seller).await.unwrap().credits,
        9_990 + everything,
        "every shell came from a sale"
    );

    // An empty bucket says so.
    let empty = sim.command(seller, "fish", fish_sub("sell")).await;
    assert!(
        empty.description().contains("The bucket is empty"),
        "{}",
        empty.description()
    );

    // Fish of a species the game dropped later sit in the bucket for nothing: they are not counted,
    // and Sell all clears them away at 0 shells.
    for n in 0..3 {
        db.record_catch(
            seller,
            pinchora_db::NewCatch {
                fish_id: &format!("vanished{n}"),
                size_cm: 12.0,
                ratio: 1.0,
                weekly: None,
                milestones: &[],
                known: &known_fish(),
            },
        )
        .await
        .unwrap();
    }
    assert_eq!(db.bucket_count(seller, &known_fish()).await.unwrap(), 0);
    let strays = sim.command(seller, "fish", fish_sub("sell")).await;
    assert!(
        strays.description().contains("no longer holds"),
        "{}",
        strays.description()
    );
    assert!(strays.button("Sell duplicates (0 · 0 shells)").disabled);
    let credits = shells(&sim, seller).await;
    let cleared = sim.click(seller, &strays, "Sell all (3 · 0 shells)").await;
    assert!(
        cleared.description().contains("Sold **3 fish** for **0 shells**"),
        "{}",
        cleared.description()
    );
    assert_eq!(shells(&sim, seller).await, credits);
    assert!(db.bucket(seller).await.unwrap().is_empty());
    assert!(cleared.button("Sell all (0 · 0 shells)").disabled);
}

#[tokio::test]
async fn the_weekly_catch_board_ranks_by_size_for_the_species() {
    let Some(mut sim) = Sim::new("the_weekly_catch_board_ranks_by_size_for_the_species").await else {
        return;
    };
    let week = pinchora_core::leveling::week_start(sim.app.now());
    let (viewer, dave, alice, bob, carol, erin) = (unique(), unique(), unique(), unique(), unique(), unique());

    // Nobody has fished this week: an empty board of the usual size.
    let empty = sim
        .command(viewer, "leaderboard", json!([text_option("board", "catch")]))
        .await;
    assert_eq!(png_height(&empty), 716);

    // A koi of 66 cm is big for a koi (usual 47.5), a 7.9 cm minnow is bigger for a minnow (usual 5.5),
    // and 200 cm is nothing for a sailfish. Centimetres do not decide it; the size for the species does.
    for (user, fish, size) in [
        (alice, "pearlpatch_koi", 66.0),
        (bob, "reefruff_lionfish", 36.0),
        (carol, "coralchorus_sailfish", 200.0),
        (dave, "toaststripe_minnow", 7.9),
        (erin, "velvetwake_catfish", 40.0),
    ] {
        catch_for_board(&sim, user, fish, size, week).await;
    }
    // A smaller second catch does not replace a player's best.
    catch_for_board(&sim, alice, "pearlpatch_koi", 30.0, week).await;
    let board = sim
        .command(viewer, "leaderboard", json!([text_option("board", "catch")]))
        .await;
    assert_eq!(png_height(&board), 716, "no podium without a last week");
    let ranked: Vec<u64> = sim
        .app
        .db
        .weekly_catch_board(sim.guild, week, 10, 0)
        .await
        .unwrap()
        .iter()
        .map(|r| r.user_id as u64)
        .collect();
    assert_eq!(ranked, [dave, alice, bob, carol, erin]);
    assert!(board.button("◀").disabled && board.button("▶").disabled);
    assert_eq!(
        board.button("◀").custom_id.as_deref(),
        Some("lb:catch:0"),
        "its buttons keep the catch board"
    );

    // Last week's winners show under the board as a podium.
    let last = week - chrono::Duration::weeks(1);
    for (user, fish, size) in [
        (erin, "moonbutton_sunfish", 80.0),
        (bob, "hushbell_angler", 50.0),
        (alice, "petalveil_betta", 8.0),
    ] {
        catch_for_board(&sim, user, fish, size, last).await;
    }
    let with_podium = sim
        .command(viewer, "leaderboard", json!([text_option("board", "catch")]))
        .await;
    assert_eq!(png_height(&with_podium), 826);
    let again = sim.component(viewer, "lb:catch:0", &[], &with_podium).await;
    assert_eq!(png_height(&again), 826);

    // Other boards and other servers do not see it.
    let weekly_xp = sim
        .command(viewer, "leaderboard", json!([text_option("board", "weekly")]))
        .await;
    assert_eq!(png_height(&weekly_xp), 716);
    assert!(
        sim.app
            .db
            .weekly_catch_board(unique(), week, 10, 0)
            .await
            .unwrap()
            .is_empty()
    );

    // A week later: this week's catches are the podium and the board starts again.
    sim.app.shift_clock(chrono::Duration::weeks(1));
    let next = sim
        .command(viewer, "leaderboard", json!([text_option("board", "catch")]))
        .await;
    assert_eq!(png_height(&next), 826);
    let new_week = pinchora_core::leveling::week_start(sim.app.now());
    assert!(
        sim.app
            .db
            .weekly_catch_board(sim.guild, new_week, 10, 0)
            .await
            .unwrap()
            .is_empty()
    );

    // A reeled catch lands on the board of the week it was caught in, unless it is junk.
    let angler = unique();
    let id = seed_cast(&sim, angler, 1, 400, "shore:driftwood_rod:-").await;
    let catch = sim.component(angler, &id, &[], &casting()).await;
    let fish = caught_fish(&catch);
    let row = sim.app.db.weekly_catch_board(sim.guild, new_week, 10, 0).await.unwrap();
    assert_eq!(row.len(), usize::from(!fish.junk), "{}", fish.id);
    if let Some(row) = row.first() {
        assert_eq!(row.user_id as u64, angler);
        assert_eq!(row.fish_id, fish.id);
    }
}

// /Admin permissions

/// Administrator, as the permission bit Discord sends.
const ADMINISTRATOR: u64 = 1 << 3;

/// What the admin settings of a server hold, as text, to see that a refused command changed nothing.
async fn admin_state(sim: &Sim, target: u64, suggestion: i64) -> String {
    let (db, guild) = (&sim.app.db, sim.guild);
    format!(
        "{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
        db.log_config(guild).await.unwrap(),
        db.mod_settings(guild).await.unwrap(),
        db.warn_thresholds(guild).await.unwrap(),
        db.guild_settings(guild).await.unwrap(),
        db.module_states(guild).await.unwrap(),
        db.level_roles(guild).await.unwrap(),
        db.rep_roles(guild).await.unwrap(),
        db.store_items(guild).await.unwrap(),
        db.xp_rules(guild).await.unwrap(),
        db.running_giveaways(guild).await.unwrap(),
        db.find_panel(guild, "Gated").await.unwrap(),
        db.suggestion_channel(guild).await.unwrap(),
        db.collection_settings(guild).await.unwrap(),
        db.gambling_enabled(guild).await.unwrap(),
        db.party_settings(guild).await.unwrap(),
        db.member_points(guild, target).await.unwrap(),
        db.suggestion(suggestion).await.unwrap(),
    )
}

/// A server can show any command to anyone in Discord's integration settings, so every admin path
/// asks for Manage Server itself. The table is the audit: each command and subcommand that changes
/// how a server runs, tried by a member with no permissions and by one with every permission but
/// Manage Server and Administrator.
#[tokio::test]
async fn admin_paths_refuse_members_without_manage_server() {
    let Some(mut sim) = Sim::new("admin_paths_refuse_members_without_manage_server").await else {
        return;
    };
    let (member, target, author) = (unique(), unique(), unique());
    let (role_id, channel) = (unique(), unique());
    let role = sim.guild_role(role_id, "Gated", 2);
    let suggestion = sim
        .app
        .db
        .create_suggestion(sim.guild, author, "Add a garden")
        .await
        .unwrap();
    // Resolved data that no option names is ignored, so one blob serves every row.
    let known = json!({
        "users": resolved(&[target], &[])["users"],
        "roles": resolved(&[], &[&role])["roles"],
        "channels": resolved_channels(&[channel])["channels"],
    });
    let rows: Vec<(&str, Value)> = vec![
        (
            "config welcome",
            json!([sub("welcome", vec![bool_option("enabled", true)])]),
        ),
        (
            "config leveling",
            json!([sub("leveling", vec![bool_option("enabled", false)])]),
        ),
        ("config show", json!([sub("show", vec![])])),
        (
            "config welcome-card",
            json!([sub("welcome-card", vec![text_option("title", "HELLO")])]),
        ),
        (
            "config level-roles add",
            json!([group(
                "level-roles",
                sub("add", vec![int_option("level", 3), role_option("role", &role)])
            )]),
        ),
        (
            "config level-roles remove",
            json!([group("level-roles", sub("remove", vec![role_option("role", &role)]))]),
        ),
        (
            "config level-roles list",
            json!([group("level-roles", sub("list", vec![]))]),
        ),
        (
            "config level-roles mode",
            json!([group("level-roles", sub("mode", vec![text_option("mode", "replace")]))]),
        ),
        (
            "config level-roles sync",
            json!([group("level-roles", sub("sync", vec![]))]),
        ),
        (
            "config xp multiplier",
            json!([group(
                "xp",
                sub(
                    "multiplier",
                    vec![number_option("value", 2.0), role_option("role", &role)]
                )
            )]),
        ),
        (
            "config xp clear",
            json!([group("xp", sub("clear", vec![role_option("role", &role)]))]),
        ),
        ("config xp show", json!([group("xp", sub("show", vec![]))])),
        (
            "config xp voice",
            json!([group("xp", sub("voice", vec![bool_option("enabled", true)]))]),
        ),
        (
            "config rep-roles add",
            json!([group(
                "rep-roles",
                sub("add", vec![int_option("reputation", 5), role_option("role", &role)])
            )]),
        ),
        (
            "config rep-roles remove",
            json!([group("rep-roles", sub("remove", vec![role_option("role", &role)]))]),
        ),
        (
            "config rep-roles list",
            json!([group("rep-roles", sub("list", vec![]))]),
        ),
        (
            "config points",
            json!([sub("points", vec![text_option("name", "coins")])]),
        ),
        (
            "config store add",
            json!([group(
                "store",
                sub("add", vec![role_option("role", &role), int_option("price", 50)])
            )]),
        ),
        (
            "config store remove",
            json!([group("store", sub("remove", vec![role_option("role", &role)]))]),
        ),
        ("config store list", json!([group("store", sub("list", vec![]))])),
        (
            "config join-role",
            json!([sub("join-role", vec![role_option("role", &role)])]),
        ),
        (
            "config goodbye",
            json!([sub("goodbye", vec![channel_option("channel", channel)])]),
        ),
        (
            "config log",
            json!([sub("log", vec![channel_option("channel", channel)])]),
        ),
        (
            "config log-events",
            json!([sub("log-events", vec![bool_option("voice", false)])]),
        ),
        (
            "config suggestions",
            json!([sub("suggestions", vec![channel_option("channel", channel)])]),
        ),
        (
            "config critters",
            json!([sub("critters", vec![bool_option("enabled", true)])]),
        ),
        (
            "config gambling",
            json!([sub("gambling", vec![bool_option("enabled", true)])]),
        ),
        (
            "config surge",
            json!([sub("surge", vec![bool_option("enabled", true)])]),
        ),
        ("config modules show", json!([group("modules", sub("show", vec![]))])),
        (
            "config modules toggle",
            json!([group(
                "modules",
                sub(
                    "toggle",
                    vec![text_option("module", "pets"), bool_option("enabled", false)]
                )
            )]),
        ),
        (
            "config modules channels",
            json!([group(
                "modules",
                sub(
                    "channels",
                    vec![
                        text_option("module", "pets"),
                        text_option("where", "only"),
                        channel_option("channel", channel),
                    ]
                )
            )]),
        ),
        (
            "config partygames",
            json!([sub("partygames", vec![bool_option("strict", true)])]),
        ),
        (
            "config log-ignore",
            json!([sub(
                "log-ignore",
                vec![user_option("user", target), bool_option("bots", true)]
            )]),
        ),
        (
            "config moderation",
            json!([sub(
                "moderation",
                vec![bool_option("dm", false), channel_option("channel", channel)]
            )]),
        ),
        (
            "config warn-ladder",
            json!([sub(
                "warn-ladder",
                vec![int_option("warns", 3), text_option("action", "kick")]
            )]),
        ),
        (
            "roles panel",
            json!([sub("panel", vec![text_option("title", "Gated")])]),
        ),
        (
            "roles add",
            json!([sub(
                "add",
                vec![
                    text_option("panel", "Gated"),
                    role_option("role", &role),
                    text_option("label", "Gate"),
                ]
            )]),
        ),
        (
            "roles remove",
            json!([sub(
                "remove",
                vec![text_option("panel", "Gated"), role_option("role", &role)]
            )]),
        ),
        (
            "roles post",
            json!([sub(
                "post",
                vec![text_option("panel", "Gated"), channel_option("channel", channel)]
            )]),
        ),
        (
            "points give",
            json!([sub(
                "give",
                vec![user_option("user", target), int_option("amount", 100)]
            )]),
        ),
        (
            "points take",
            json!([sub(
                "take",
                vec![user_option("user", target), int_option("amount", 100)]
            )]),
        ),
        (
            "giveaway start",
            json!([sub(
                "start",
                vec![
                    text_option("duration", "1h"),
                    int_option("winners", 1),
                    text_option("prize", "A cake"),
                ]
            )]),
        ),
        ("giveaway end", json!([sub("end", vec![int_option("id", 1)])])),
        ("giveaway reroll", json!([sub("reroll", vec![int_option("id", 1)])])),
        ("giveaway list", json!([sub("list", vec![])])),
        (
            "suggestion approve",
            json!([sub("approve", vec![int_option("id", i64::from(suggestion.number))])]),
        ),
        (
            "suggestion deny",
            json!([sub("deny", vec![int_option("id", i64::from(suggestion.number))])]),
        ),
        ("plaza edit", json!([sub("edit", vec![])])),
    ];

    let before = admin_state(&sim, target, suggestion.id).await;
    let all_but_manage = ((1u64 << 50) - 1) & !(MANAGE_GUILD | ADMINISTRATOR);
    for permissions in [0, all_but_manage] {
        sim.permissions = permissions;
        for (label, options) in &rows {
            let name = label.split(' ').next().expect("a command name");
            let reply = sim.command_resolved(member, name, options.clone(), known.clone()).await;
            let wanted = if name == "plaza" {
                "server managers"
            } else {
                "Manage Server"
            };
            assert!(
                reply.ephemeral && reply.content().contains(wanted),
                "/{label} with permissions {permissions}: {:?}",
                reply.content()
            );
        }
    }
    assert_eq!(
        admin_state(&sim, target, suggestion.id).await,
        before,
        "a refused command changed something"
    );

    // The plaza editor's own controls ask too: a manager opens it, somebody else presses its buttons.
    sim.permissions = MANAGE_GUILD;
    let editor = sim.command(member, "plaza", json!([sub("edit", vec![])])).await;
    let controls: Vec<(String, bool)> = editor
        .controls()
        .filter_map(|c| match c {
            Component::Button(b) => b.custom_id.clone().map(|id| (id, false)),
            Component::SelectMenu(m) => Some((m.custom_id.clone(), true)),
            _ => None,
        })
        .filter(|(id, _)| id.starts_with("pe:"))
        .collect();
    assert!(controls.len() > 3, "the plaza editor has controls: {controls:?}");
    sim.permissions = all_but_manage;
    for (id, menu) in &controls {
        let values: &[&str] = if *menu { &["x"] } else { &[] };
        let refused = sim.component(target, id, values, &editor).await;
        assert!(
            refused.ephemeral && refused.content().contains("server managers"),
            "{id}: {:?}",
            refused.content()
        );
    }

    // Manage Server and Administrator get through.
    for permissions in [MANAGE_GUILD, ADMINISTRATOR] {
        sim.permissions = permissions;
        let shown = sim.command(member, "config", json!([sub("show", vec![])])).await;
        assert_eq!(shown.title(), "Server settings", "permissions {permissions}");
        let listed = sim.command(member, "giveaway", json!([sub("list", vec![])])).await;
        assert!(!listed.content().contains("Manage Server"), "{}", listed.content());
        let gave = sim
            .command_resolved(
                member,
                "points",
                json!([sub("give", vec![user_option("user", target), int_option("amount", 5)])]),
                known.clone(),
            )
            .await;
        assert!(gave.description().contains("Gave **5**"), "{}", gave.description());
    }
}

// /Party games

fn play_sub(name: &str, options: Vec<Value>) -> Value {
    json!([sub(name, options)])
}

/// The seed a derby or heist lobby was opened with, which decides how it will go.
async fn seed_of(sim: &Sim, session: i64) -> u64 {
    sim.app
        .db
        .game(session)
        .await
        .expect("session")
        .expect("the lobby exists")
        .state
        .parse()
        .expect("a lobby's state is its seed")
}

/// The race a derby lobby will run.
async fn race_of(sim: &Sim, session: i64) -> pinchora_game::party::derby::Race {
    pinchora_game::party::derby::run(seed_of(sim, session).await)
}

/// Gives a lobby a message of its own to be edited, as `track_message` does a moment after a command.
async fn give_message(sim: &Sim, session: i64) -> u64 {
    let message = unique();
    sim.app
        .db
        .set_game_message(session, sim.channel, message)
        .await
        .expect("message");
    message
}

/// Opens a derby lobby as `host` in the sim's channel.
async fn open_derby(sim: &mut Sim, host: u64, bet: Option<i64>) -> Reply {
    let options = bet.map(|b| vec![int_option("bet", b)]).unwrap_or_default();
    sim.command(host, "play", play_sub("derby", options)).await
}

/// The edit of `message` among the posts, which must carry `file`.
fn edit_with<'a>(posts: &'a [Post], message: u64, file: &str) -> &'a Post {
    let edit = posts
        .iter()
        .find(|p| p.message == message)
        .unwrap_or_else(|| panic!("message {message} was not edited"));
    assert!(edit.files.iter().any(|(name, _)| name == file), "no {file} on the edit");
    edit
}

#[tokio::test]
async fn a_free_derby_runs_as_a_gif_and_rewards_the_backers_of_the_winner() {
    let Some(mut sim) = Sim::new("a_free_derby_runs_as_a_gif_and_rewards_the_backers_of_the_winner").await else {
        return;
    };
    sim.channel = unique();
    let (host, a, b, stranger) = (unique(), unique(), unique(), unique());
    let lobby = open_derby(&mut sim, host, None).await;
    assert!(
        !lobby.ephemeral && lobby.content().contains("Hermit Crab Derby") && lobby.content().contains("No stakes"),
        "{}",
        lobby.content()
    );
    assert!(lobby.pings_nobody());
    assert_png(&lobby, 600, 470);
    assert_eq!(lobby.controls().count(), 8, "six crabs, start and leave");
    let session = session_of(&lobby);
    let race = race_of(&sim, session).await;
    let (winner, other) = (race.winner(), (race.winner() + 1) % 6);
    let names = pinchora_game::party::derby::ROSTER.map(|c| c.name);

    let second = open_derby(&mut sim, stranger, None).await;
    assert!(
        second.ephemeral && second.content().contains("already open"),
        "{}",
        second.content()
    );

    let after_a = sim.click(a, &lobby, names[winner]).await;
    assert_png(&after_a, 600, 470);
    let after_b = sim.click(b, &after_a, names[other]).await;
    assert_png(&after_b, 600, 470);
    let backers = sim.app.db.party_members(session).await.unwrap();
    assert_eq!(
        backers
            .iter()
            .map(|m| (m.user_id as u64, m.choice as usize))
            .collect::<Vec<_>>(),
        vec![(a, winner), (b, other)]
    );

    let early = sim.click(stranger, &after_b, "Start the race").await;
    assert!(
        early.ephemeral && early.content().contains("Only"),
        "{}",
        early.content()
    );
    let nobody = sim.click(stranger, &after_b, "Leave").await;
    assert!(
        nobody.ephemeral && nobody.content().contains("haven't picked"),
        "{}",
        nobody.content()
    );

    let finished = sim.click(host, &after_b, "Start the race").await;
    assert!(finished.components.is_empty(), "the buttons are gone");
    assert!(
        finished.content().contains("||") && finished.content().contains(&format!("**{}**", names[winner])),
        "{}",
        finished.content()
    );
    assert!(
        finished.content().contains(&format!("{} +20", mention(a))),
        "{}",
        finished.content()
    );
    let frames = gif_frames(&finished, "derby.gif");
    assert!(frames.len() > 45, "{} frames", frames.len());
    assert_eq!((frames[0].width(), frames[0].height()), (480, 288));
    save_frames(&sim, "derby", &frames);
    frames[frames.len() - 1]
        .save(sim.dir.join("derby-last.png"))
        .expect("save the last frame");
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (20, 0));
    assert_eq!(record_of(&sim, a, "derby").await, (1, 1));
    assert_eq!(record_of(&sim, b, "derby").await, (1, 0));
    assert_eq!(record_of(&sim, host, "derby").await, (0, 0), "the host only watched");

    let late = sim.click(a, &after_b, names[other]).await;
    assert!(late.ephemeral && late.content().contains("over"), "{}", late.content());
    // The channel is free for another race now.
    let next = open_derby(&mut sim, stranger, None).await;
    assert!(!next.ephemeral, "{}", next.content());
}

#[tokio::test]
async fn a_staked_derby_shares_the_pool_between_the_backers_and_the_janitor_runs_a_forgotten_race() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) =
        Sim::new("a_staked_derby_shares_the_pool_between_the_backers_and_the_janitor_runs_a_forgotten_race").await
    else {
        return;
    };
    forget_late_games(&sim).await;
    sim.channel = unique();
    let (host, a, b, c, broke) = (unique(), unique(), unique(), unique(), unique());
    gambling(&mut sim, unique(), true).await;
    for user in [a, b, c] {
        fund(&sim, user, 100).await;
    }
    fund(&sim, broke, 10).await;
    let lobby = open_derby(&mut sim, host, Some(40)).await;
    assert!(lobby.content().contains("**40** shells"), "{}", lobby.content());
    let session = session_of(&lobby);
    let race = race_of(&sim, session).await;
    let names = pinchora_game::party::derby::ROSTER.map(|c| c.name);
    let (win, lose, other) = (race.winner(), (race.winner() + 1) % 6, (race.winner() + 2) % 6);

    let short = sim.click(broke, &lobby, names[win]).await;
    assert!(
        short.ephemeral && short.content().contains("only have **10** shells"),
        "{}",
        short.content()
    );
    sim.click(a, &lobby, names[win]).await;
    sim.click(b, &lobby, names[win]).await;
    let mut view = sim.click(c, &lobby, names[lose]).await;
    assert_eq!(
        (shells(&sim, a).await, shells(&sim, b).await, shells(&sim, c).await),
        (60, 60, 60),
        "the stakes are held"
    );
    // Changing crab costs nothing more; leaving gives the stake back, once.
    view = sim.click(c, &view, names[other]).await;
    assert_eq!(shells(&sim, c).await, 60);
    view = sim.click(c, &view, "Leave").await;
    assert_eq!(shells(&sim, c).await, 100);
    let gone = sim.click(c, &view, "Leave").await;
    assert!(
        gone.ephemeral && gone.content().contains("haven't picked"),
        "{}",
        gone.content()
    );
    sim.click(c, &view, names[lose]).await;
    assert_eq!(shells(&sim, c).await, 60);
    assert_eq!(sim.app.db.party_members(session).await.unwrap().len(), 3);

    // Nobody starts it: the janitor runs the race when the lobby's time is up and edits its message.
    let message = give_message(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    assert_eq!(shells(&sim, a).await, 60, "a lobby with time left is not touched");
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = edit_with(&posts, message, "derby.gif");
    assert!(
        edit.content().contains("share the pool of **120** shells")
            && edit.content().contains(&format!("{} +60", mention(a)))
            && edit.content().contains(&format!("{} +60", mention(b))),
        "{}",
        edit.content()
    );
    assert!(!edit.content().contains(&mention(c)), "the loser is not named");
    assert_eq!(edit.body["components"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        (shells(&sim, a).await, shells(&sim, b).await, shells(&sim, c).await),
        (120, 120, 60),
        "the pool went to the two who backed the winner"
    );
    assert_eq!(record_of(&sim, a, "derby").await, (1, 1));
    assert_eq!(record_of(&sim, c, "derby").await, (1, 0));
    sweep_games(&sim.app).await.expect("sweep again");
    assert_eq!(shells(&sim, a).await, 120, "paid once");
    assert!(sim.posts().is_empty());
}

#[tokio::test]
async fn a_derby_nobody_won_or_too_few_joined_gives_every_stake_back() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("a_derby_nobody_won_or_too_few_joined_gives_every_stake_back").await else {
        return;
    };
    forget_late_games(&sim).await;
    sim.channel = unique();
    let (host, a, b) = (unique(), unique(), unique());
    gambling(&mut sim, unique(), true).await;
    for user in [a, b] {
        fund(&sim, user, 100).await;
    }
    let names = pinchora_game::party::derby::ROSTER.map(|c| c.name);

    // Nobody backed the winner.
    let lobby = open_derby(&mut sim, host, Some(25)).await;
    let race = race_of(&sim, session_of(&lobby)).await;
    sim.click(a, &lobby, names[(race.winner() + 1) % 6]).await;
    sim.click(b, &lobby, names[(race.winner() + 2) % 6]).await;
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (75, 75));
    let finished = sim.click(host, &lobby, "Start the race").await;
    assert!(
        finished.content().contains("every stake went back"),
        "{}",
        finished.content()
    );
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (100, 100));

    // A race with shells on it needs two racers: the host cannot start it early with one, and a race
    // that runs out of time with one is called off and the stake goes back.
    let lobby = open_derby(&mut sim, a, Some(30)).await;
    let session = session_of(&lobby);
    let message = give_message(&sim, session).await;
    sim.click(a, &lobby, names[0]).await;
    assert_eq!(shells(&sim, a).await, 70);
    let early = sim.click(a, &lobby, "Start the race").await;
    assert!(
        early.ephemeral && early.content().contains("at least 2 racers"),
        "{}",
        early.content()
    );
    assert_eq!(shells(&sim, a).await, 70, "the lobby is still open");
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the lobby message says the race was called off");
    assert!(
        edit.content().contains("called off") && edit.content().contains("The stake went back"),
        "{}",
        edit.content()
    );
    assert_eq!(shells(&sim, a).await, 100, "the stake went back");
    assert_eq!(
        record_of(&sim, a, "derby").await,
        (1, 0),
        "the race before counted once; a called-off one adds nothing"
    );

    // An empty lobby cannot be started either.
    let lobby = open_derby(&mut sim, b, None).await;
    let empty = sim.click(b, &lobby, "Start the race").await;
    assert!(
        empty.ephemeral && empty.content().contains("Pick a crab"),
        "{}",
        empty.content()
    );
}

/// What a player has won less staked at the derby tables today.
async fn derby_net(sim: &Sim, user: u64) -> i64 {
    sim.app.db.table_today(user, "derby").await.expect("the day's table").1
}

#[tokio::test]
async fn staked_derbies_need_the_gambling_switch_and_stop_at_the_daily_loss_limit() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("staked_derbies_need_the_gambling_switch_and_stop_at_the_daily_loss_limit").await
    else {
        return;
    };
    forget_late_games(&sim).await;
    let (manager, rich, other) = (unique(), unique(), unique());
    fund(&sim, rich, 5_000).await;
    fund(&sim, other, 5_000).await;
    let names = pinchora_game::party::derby::ROSTER.map(|c| c.name);

    // Off by default: a race with shells on it is refused, a race for fun is not.
    sim.channel = unique();
    let refused = open_derby(&mut sim, unique(), Some(40)).await;
    assert!(
        refused.ephemeral
            && refused.content().contains("**off**")
            && refused.content().contains("/config gambling")
            && refused.files.is_empty(),
        "{}",
        refused.content()
    );
    let free = open_derby(&mut sim, unique(), None).await;
    assert!(
        !free.ephemeral && free.content().contains("No stakes"),
        "{}",
        free.content()
    );
    gambling(&mut sim, manager, true).await;

    // Three lobbies of 500: the limit is 1,000 shells down, so two fit and the third is refused.
    let mut lobbies = Vec::new();
    for _ in 0..3 {
        sim.channel = unique();
        let lobby = open_derby(&mut sim, unique(), Some(500)).await;
        assert!(!lobby.ephemeral, "{}", lobby.content());
        lobbies.push(lobby);
    }
    sim.click(rich, &lobbies[0], names[0]).await;
    sim.click(rich, &lobbies[1], names[0]).await;
    assert_eq!((shells(&sim, rich).await, derby_net(&sim, rich).await), (4_000, -1_000));
    let third = sim.click(rich, &lobbies[2], names[0]).await;
    assert!(
        third.ephemeral && third.content().contains("limit of **1,000**") && third.content().contains("-1,000"),
        "{}",
        third.content()
    );
    assert_eq!(shells(&sim, rich).await, 4_000, "a refused stake takes nothing");
    let sessions: Vec<i64> = lobbies.iter().map(session_of).collect();
    assert!(sim.app.db.party_members(sessions[2]).await.unwrap().is_empty());
    sim.click(other, &lobbies[2], names[0]).await;
    assert_eq!(derby_net(&sim, other).await, -500, "the limit is each player's own");

    // Leaving a lobby gives the room under the limit back.
    sim.click(rich, &lobbies[0], "Leave").await;
    assert_eq!((shells(&sim, rich).await, derby_net(&sim, rich).await), (4_500, -500));
    sim.click(rich, &lobbies[2], names[0]).await;
    assert_eq!((shells(&sim, rich).await, derby_net(&sim, rich).await), (4_000, -1_000));

    // The race of the third lobby: `other` backs the winner and `rich` does not. The second lobby has
    // only one racer, so it is called off and its stake comes back.
    let race = race_of(&sim, sessions[2]).await;
    sim.click(other, &lobbies[2], names[race.winner()]).await;
    sim.click(rich, &lobbies[2], names[(race.winner() + 1) % 6]).await;
    expire(&sim, sessions[1]).await;
    expire(&sim, sessions[2]).await;
    sweep_games(&sim.app).await.expect("sweep");
    assert_eq!((shells(&sim, rich).await, derby_net(&sim, rich).await), (4_500, -500));
    assert_eq!((shells(&sim, other).await, derby_net(&sim, other).await), (5_500, 500));
}

#[tokio::test]
async fn share_or_grab_hides_the_picks_and_pays_the_sea_s_pool_by_what_they_picked() {
    let Some(mut sim) = Sim::new("share_or_grab_hides_the_picks_and_pays_the_sea_s_pool_by_what_they_picked").await
    else {
        return;
    };
    let (a, b, bystander) = (unique(), unique(), unique());
    let challenge = duel(&mut sim, a, b, "share", vec![]).await;
    assert!(
        !challenge.ephemeral
            && challenge.content().contains("Share or Grab")
            && challenge.content().contains("**60** shells in the pool"),
        "{}",
        challenge.content()
    );
    // A duel's Accept with this id is not a way in (nor past the party games' module switch).
    let forged = format!("c4:y:{}", session_of(&challenge));
    let forged = sim.component(b, &forged, &[], &challenge).await;
    assert!(
        forged.ephemeral && forged.content().contains("expired"),
        "{}",
        forged.content()
    );
    let board = sim.click(b, &challenge, "Accept").await;
    assert_png(&board, 580, 350);
    assert!(
        board.content().contains("⏳") && board.has_button("Choose in secret"),
        "{}",
        board.content()
    );
    // Nor is a trivia answer or a hangman letter with this id: those buttons pass the word games' switch.
    let id = session_of(&challenge);
    let forged = sim.component(a, &format!("tv:{id}:0"), &[], &board).await;
    assert!(!forged.content().contains("Locked in"), "{}", forged.content());
    let forged = sim.component(a, &format!("hm:{id}:0"), &["a"], &board).await;
    assert!(forged.content().contains("over"), "{}", forged.content());
    let refused = sim.click(bystander, &board, "Choose in secret").await;
    assert!(
        refused.ephemeral && refused.content().contains("not in this game"),
        "{}",
        refused.content()
    );

    let a_picker = sim.click(a, &board, "Choose in secret").await;
    let b_picker = sim.click(b, &board, "Choose in secret").await;
    assert!(a_picker.ephemeral && b_picker.ephemeral);
    assert_eq!(a_picker.controls().count(), 2, "share or grab");
    let locked = sim.click(a, &a_picker, "🤝 Share").await;
    assert!(
        locked.content().contains("Locked in 🤝 Share") && locked.components.is_empty(),
        "{}",
        locked.content()
    );
    let posts = sim.posts();
    let shown = posts.last().expect("the public message was refreshed");
    assert!(
        shown.content().contains("✅") && shown.content().contains("⏳") && !shown.content().contains("Share"),
        "{}",
        shown.content()
    );
    assert!(shown.files.iter().any(|(name, _)| name == "share.png"));
    let repeat = sim.click(a, &a_picker, "🦀 Grab").await;
    assert!(
        repeat.content().contains("You locked in 🤝"),
        "no changing your mind: {}",
        repeat.content()
    );

    let done = sim.click(b, &b_picker, "🤝 Share").await;
    assert!(done.content().contains("Both shared"), "{}", done.content());
    let posts = sim.posts();
    let end = posts.last().expect("the result");
    assert!(
        end.content().contains("split: **30** shells each")
            && end.body["components"].as_array().map(Vec::len) == Some(0),
        "{}",
        end.content()
    );
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (30, 30));
    assert_eq!(record_of(&sim, a, "sharegrab").await, (1, 1));
    assert_eq!(record_of(&sim, b, "sharegrab").await, (1, 1));
    let late = sim.click(a, &a_picker, "🤝 Share").await;
    assert!(late.content().contains("over"), "{}", late.content());

    // The same pair is not paid from the pool twice in an hour.
    let challenge = duel(&mut sim, b, a, "share", vec![]).await;
    let board = sim.click(a, &challenge, "Accept").await;
    let (a_picker, b_picker) = (
        sim.click(a, &board, "Choose in secret").await,
        sim.click(b, &board, "Choose in secret").await,
    );
    sim.click(a, &a_picker, "🤝 Share").await;
    sim.click(b, &b_picker, "🤝 Share").await;
    let posts = sim.posts();
    assert!(
        posts.last().expect("result").content().contains("within the hour"),
        "{}",
        posts.last().expect("result").content()
    );
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (30, 30));

    // A grabber takes the whole pool from a sharer; two grabbers get nothing.
    let (c, d, e, f) = (unique(), unique(), unique(), unique());
    let challenge = duel(&mut sim, c, d, "share", vec![]).await;
    let board = sim.click(d, &challenge, "Accept").await;
    let (c_picker, d_picker) = (
        sim.click(c, &board, "Choose in secret").await,
        sim.click(d, &board, "Choose in secret").await,
    );
    sim.click(d, &d_picker, "🤝 Share").await;
    let grabbed = sim.click(c, &c_picker, "🦀 Grab").await;
    assert!(
        grabbed.content().contains("🦀 Grab against 🤝 Share"),
        "{}",
        grabbed.content()
    );
    let posts = sim.posts();
    let end = posts.last().expect("the result");
    assert!(
        end.content()
            .contains(&format!("{} grabbed the whole pool of **60** shells", mention(c))),
        "{}",
        end.content()
    );
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (60, 0));
    assert_eq!(record_of(&sim, c, "sharegrab").await, (1, 1));
    assert_eq!(record_of(&sim, d, "sharegrab").await, (1, 0));

    let challenge = duel(&mut sim, e, f, "share", vec![]).await;
    let board = sim.click(f, &challenge, "Accept").await;
    let (e_picker, f_picker) = (
        sim.click(e, &board, "Choose in secret").await,
        sim.click(f, &board, "Choose in secret").await,
    );
    sim.click(e, &e_picker, "🦀 Grab").await;
    sim.click(f, &f_picker, "🦀 Grab").await;
    let posts = sim.posts();
    assert!(
        posts.last().expect("result").content().contains("washes away"),
        "{}",
        posts.last().unwrap().content()
    );
    assert_eq!((shells(&sim, e).await, shells(&sim, f).await), (0, 0));
    assert_eq!(record_of(&sim, e, "sharegrab").await, (1, 0));
}

#[tokio::test]
async fn share_or_grab_wagers_go_by_the_picks_and_a_player_who_never_picks_forfeits() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("share_or_grab_wagers_go_by_the_picks_and_a_player_who_never_picks_forfeits").await
    else {
        return;
    };
    forget_late_games(&sim).await;
    let users: Vec<u64> = (0..8).map(|_| unique()).collect();
    for user in &users {
        fund(&sim, *user, 200).await;
    }
    let (a, b, c, d, e, f, g, h) = (
        users[0], users[1], users[2], users[3], users[4], users[5], users[6], users[7],
    );

    let challenge = duel(&mut sim, a, b, "share", vec![int_option("bet", 50)]).await;
    assert!(
        challenge.content().contains("**100** in the pot") && challenge.content().contains("sea keeps it all"),
        "{}",
        challenge.content()
    );
    let board = sim.click(b, &challenge, "Accept").await;
    assert!(board.content().contains("**100** shells"), "{}", board.content());
    assert_eq!(
        (shells(&sim, a).await, shells(&sim, b).await),
        (150, 150),
        "the stakes are held"
    );
    // One grabs while the other shares: the grabber takes the pot.
    let (a_picker, b_picker) = (
        sim.click(a, &board, "Choose in secret").await,
        sim.click(b, &board, "Choose in secret").await,
    );
    sim.click(a, &a_picker, "🦀 Grab").await;
    sim.click(b, &b_picker, "🤝 Share").await;
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (250, 150));
    assert_eq!(record_of(&sim, a, "sharegrab").await, (1, 1));

    // Both share: the stakes come back. Both grab: the sea keeps them.
    let challenge = duel(&mut sim, c, d, "share", vec![int_option("bet", 50)]).await;
    let board = sim.click(d, &challenge, "Accept").await;
    let (c_picker, d_picker) = (
        sim.click(c, &board, "Choose in secret").await,
        sim.click(d, &board, "Choose in secret").await,
    );
    sim.click(c, &c_picker, "🤝 Share").await;
    sim.click(d, &d_picker, "🤝 Share").await;
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (200, 200));
    let challenge = duel(&mut sim, d, c, "share", vec![int_option("bet", 50)]).await;
    let board = sim.click(c, &challenge, "Accept").await;
    let (c_picker, d_picker) = (
        sim.click(c, &board, "Choose in secret").await,
        sim.click(d, &board, "Choose in secret").await,
    );
    sim.click(c, &c_picker, "🦀 Grab").await;
    sim.click(d, &d_picker, "🦀 Grab").await;
    let posts = sim.posts();
    assert!(
        posts.last().expect("result").content().contains("sink into the sea"),
        "{}",
        posts.last().unwrap().content()
    );
    assert_eq!((shells(&sim, c).await, shells(&sim, d).await), (150, 150));

    // A player who never picks forfeits the pot to the one who did.
    let challenge = duel(&mut sim, e, f, "share", vec![int_option("bet", 50)]).await;
    let board = sim.click(f, &challenge, "Accept").await;
    let session = session_of(&board);
    let e_picker = sim.click(e, &board, "Choose in secret").await;
    sim.click(e, &e_picker, "🦀 Grab").await;
    sim.posts();
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    // Found by what it says: the challenge's own tracker may have swapped the message id meanwhile.
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.content().contains("forfeits"))
        .expect("the forfeit is announced");
    assert!(edit.files.iter().any(|(name, _)| name == "share.png"));
    assert!(
        edit.content()
            .contains(&format!("{} ran out of time and forfeits", mention(f)))
            && edit
                .content()
                .contains(&format!("{} takes the pot of **100**", mention(e))),
        "{}",
        edit.content()
    );
    assert_eq!((shells(&sim, e).await, shells(&sim, f).await), (250, 150));
    assert_eq!(record_of(&sim, e, "sharegrab").await, (1, 1));
    assert_eq!(record_of(&sim, f, "sharegrab").await, (1, 0));
    sweep_games(&sim.app).await.expect("sweep again");
    assert_eq!(shells(&sim, e).await, 250, "paid once");

    // Nobody picks: both stakes go back, and a late click ends the game in place.
    let challenge = duel(&mut sim, g, h, "share", vec![int_option("bet", 50)]).await;
    let board = sim.click(h, &challenge, "Accept").await;
    assert_eq!((shells(&sim, g).await, shells(&sim, h).await), (150, 150));
    expire(&sim, session_of(&board)).await;
    let ended = sim.click(g, &board, "Choose in secret").await;
    assert!(ended.content().contains("Nobody picked in time"), "{}", ended.content());
    assert_eq!((shells(&sim, g).await, shells(&sim, h).await), (200, 200));
    assert_eq!(record_of(&sim, g, "sharegrab").await, (0, 0));

    // A declined challenge holds nothing.
    let challenge = duel(&mut sim, h, c, "share", vec![int_option("bet", 50)]).await;
    let before = (shells(&sim, h).await, shells(&sim, c).await);
    let declined = sim.click(c, &challenge, "Decline").await;
    assert!(declined.content().contains("declined"), "{}", declined.content());
    assert_eq!((shells(&sim, h).await, shells(&sim, c).await), before);
}

#[tokio::test]
async fn a_pair_s_hour_starts_when_the_pool_paid_them_and_a_double_click_pays_once() {
    let Some(mut sim) = Sim::new("a_pair_s_hour_starts_when_the_pool_paid_them_and_a_double_click_pays_once").await
    else {
        return;
    };
    let pair_key = |x: u64, y: u64| format!("rest:sharepair:{}:{}", x.min(y), x.max(y));

    // Players who used up the day's pool: the game pays nothing, so it must not start their hour.
    let (a, b) = (unique(), unique());
    for user in [a, b] {
        assert_eq!(
            sim.app
                .db
                .grant_capped(user, "sharegrab", 180, 180, "test")
                .await
                .unwrap(),
            180
        );
    }
    let challenge = duel(&mut sim, a, b, "share", vec![]).await;
    let board = sim.click(b, &challenge, "Accept").await;
    let (a_picker, b_picker) = (
        sim.click(a, &board, "Choose in secret").await,
        sim.click(b, &board, "Choose in secret").await,
    );
    sim.click(a, &a_picker, "🤝 Share").await;
    sim.click(b, &b_picker, "🤝 Share").await;
    let posts = sim.posts();
    assert!(
        posts.last().expect("result").content().contains("limit for this game"),
        "{}",
        posts.last().unwrap().content()
    );
    assert_eq!((shells(&sim, a).await, shells(&sim, b).await), (180, 180));
    assert!(
        sim.app.cache.get(&pair_key(a, b)).await.unwrap().is_none(),
        "nothing was paid, so the pair is not resting"
    );

    // The last pick arriving twice at once: the game ends once, and it pays.
    let (c, d) = (unique(), unique());
    let challenge = duel(&mut sim, c, d, "share", vec![]).await;
    let board = sim.click(d, &challenge, "Accept").await;
    let (c_picker, d_picker) = (
        sim.click(c, &board, "Choose in secret").await,
        sim.click(d, &board, "Choose in secret").await,
    );
    sim.click(c, &c_picker, "🤝 Share").await;
    let share = d_picker.button("🤝 Share").custom_id.clone().expect("a custom id");
    let data = json!({ "custom_id": share, "component_type": 2, "values": [] });
    let first = sim.interaction(d, 3, data.clone(), Some(&d_picker));
    let second = sim.interaction(d, 3, data, Some(&d_picker));
    let (first_tx, _first_rx) = oneshot::channel();
    let (second_tx, _second_rx) = oneshot::channel();
    let first_responder = Arc::new(Responder::http(sim.app.clone(), &first, first_tx));
    let second_responder = Arc::new(Responder::http(sim.app.clone(), &second, second_tx));
    ERRORS.with_borrow_mut(Vec::clear);
    tokio::join!(
        Box::pin(handle_interaction(sim.app.clone(), first, first_responder)),
        Box::pin(handle_interaction(sim.app.clone(), second, second_responder))
    );
    let errors = ERRORS.take();
    assert!(errors.is_empty(), "a handler failed:\n{}", errors.join("\n"));
    assert_eq!(
        (shells(&sim, c).await, shells(&sim, d).await),
        (30, 30),
        "paid once, in full"
    );
    assert_eq!(record_of(&sim, c, "sharegrab").await, (1, 1));
    assert!(
        sim.app.cache.get(&pair_key(c, d)).await.unwrap().is_some(),
        "a payout went through, so the hour has started"
    );
}

/// `/config gambling enabled:<on>` as a manager.
async fn gambling(sim: &mut Sim, manager: u64, on: bool) -> Reply {
    sim.permissions = MANAGE_GUILD;
    let reply = sim
        .command(
            manager,
            "config",
            json!([sub("gambling", vec![bool_option("enabled", on)])]),
        )
        .await;
    sim.permissions = 0;
    reply
}

#[tokio::test]
async fn the_shell_game_is_off_until_a_manager_turns_it_on_and_pays_a_right_guess() {
    let Some(mut sim) = Sim::new("the_shell_game_is_off_until_a_manager_turns_it_on_and_pays_a_right_guess").await
    else {
        return;
    };
    let (manager, player) = (unique(), unique());
    fund(&sim, player, 100).await;
    let shells_cmd = |stake: i64| play_sub("shells", vec![int_option("bet", stake)]);
    let off = sim.command(player, "play", shells_cmd(20)).await;
    assert!(off.ephemeral && off.content().contains("**off**"), "{}", off.content());
    assert_eq!(shells(&sim, player).await, 100, "nothing was taken");
    let member = sim
        .command(
            player,
            "config",
            json!([sub("gambling", vec![bool_option("enabled", true)])]),
        )
        .await;
    assert!(
        member.ephemeral && member.content().contains("Manage Server"),
        "{}",
        member.content()
    );
    assert!(
        !sim.app.db.gambling_enabled(sim.guild).await.unwrap(),
        "a member cannot switch it on"
    );
    let on = gambling(&mut sim, manager, true).await;
    assert!(on.ephemeral && on.content().contains("**on**"), "{}", on.content());
    assert!(sim.app.db.gambling_enabled(sim.guild).await.unwrap());
    let broke = sim.command(unique(), "play", shells_cmd(20)).await;
    assert!(
        broke.ephemeral && broke.content().contains("only have **0** shells"),
        "{}",
        broke.content()
    );

    // A right guess pays two and a half times the stake.
    let table = sim.command(player, "play", shells_cmd(20)).await;
    assert!(
        !table.ephemeral && table.content().contains("sits down at the shell table"),
        "{}",
        table.content()
    );
    assert_png(&table, 520, 320);
    assert_eq!(table.controls().count(), 3);
    assert_eq!(shells(&sim, player).await, 80, "the stake is held");
    let session = session_of(&table);
    let pearl: usize = sim.app.db.game(session).await.unwrap().unwrap().state.parse().unwrap();
    let stranger = sim.click(unique(), &table, &format!("Shell {}", pearl + 1)).await;
    assert!(
        stranger.ephemeral && stranger.content().contains("isn't your table"),
        "{}",
        stranger.content()
    );
    let found = sim.click(player, &table, &format!("Shell {}", pearl + 1)).await;
    assert!(
        found.components.is_empty() && found.content().contains("found the pearl"),
        "{}",
        found.content()
    );
    assert_png(&found, 520, 320);
    assert_eq!(shells(&sim, player).await, 130, "20 staked, 50 back");
    assert_eq!(record_of(&sim, player, "shellgame").await, (1, 1));
    let again = sim.click(player, &table, "Shell 1").await;
    assert!(
        again.ephemeral && again.content().contains("over"),
        "{}",
        again.content()
    );

    // A wrong guess loses the stake and shows where the pearl was.
    sim.app
        .cache
        .forget(&format!("rest:shellsstart:{player}"))
        .await
        .unwrap();
    let table = sim.command(player, "play", shells_cmd(30)).await;
    let pearl: usize = sim
        .app
        .db
        .game(session_of(&table))
        .await
        .unwrap()
        .unwrap()
        .state
        .parse()
        .unwrap();
    let wrong = (pearl + 1) % 3;
    let lost = sim.click(player, &table, &format!("Shell {}", wrong + 1)).await;
    assert!(
        lost.content().contains(&format!("pearl was under shell {}", pearl + 1)),
        "{}",
        lost.content()
    );
    assert_eq!(shells(&sim, player).await, 100);
    assert_eq!(record_of(&sim, player, "shellgame").await, (2, 1));
    let again = sim.command(player, "play", shells_cmd(30)).await;
    assert!(
        again.ephemeral && again.content().contains("still shuffling"),
        "{}",
        again.content()
    );

    // Switched off again, the table closes.
    gambling(&mut sim, manager, false).await;
    sim.app
        .cache
        .forget(&format!("rest:shellsstart:{player}"))
        .await
        .unwrap();
    let closed = sim.command(player, "play", shells_cmd(20)).await;
    assert!(
        closed.ephemeral && closed.content().contains("**off**"),
        "{}",
        closed.content()
    );
}

#[tokio::test]
async fn a_forgotten_shell_game_gives_the_stake_back_and_the_tables_have_daily_limits() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("a_forgotten_shell_game_gives_the_stake_back_and_the_tables_have_daily_limits").await
    else {
        return;
    };
    forget_late_games(&sim).await;
    let (manager, player, late) = (unique(), unique(), unique());
    fund(&sim, player, 100).await;
    fund(&sim, late, 100).await;
    gambling(&mut sim, manager, true).await;
    let shells_cmd = |stake: i64| play_sub("shells", vec![int_option("bet", stake)]);

    let table = sim.command(player, "play", shells_cmd(40)).await;
    let session = session_of(&table);
    let message = give_message(&sim, session).await;
    assert_eq!(shells(&sim, player).await, 60);
    sweep_games(&sim.app).await.expect("sweep");
    assert_eq!(shells(&sim, player).await, 60, "a table with time left is left alone");
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    assert_eq!(shells(&sim, player).await, 100, "the stake is back");
    let posts = sim.posts();
    let edit = edit_with(&posts, message, "shells.png");
    assert!(edit.content().contains("never picked a shell"), "{}", edit.content());
    assert_eq!(edit.body["components"].as_array().map(Vec::len), Some(0));
    sweep_games(&sim.app).await.expect("sweep again");
    assert_eq!(shells(&sim, player).await, 100, "refunded once");
    assert_eq!(record_of(&sim, player, "shellgame").await, (0, 0));

    // A click on a late table ends it in place.
    let table = sim.command(late, "play", shells_cmd(25)).await;
    expire(&sim, session_of(&table)).await;
    let ended = sim.click(late, &table, "Shell 2").await;
    assert!(ended.content().contains("never picked a shell"), "{}", ended.content());
    assert_eq!(shells(&sim, late).await, 100);

    // The loss limit: 300 down is the end of the day. Three lost games of 100 would pass it.
    let rich = unique();
    fund(&sim, rich, 1_000).await;
    for round in 0..3 {
        sim.app.cache.forget(&format!("rest:shellsstart:{rich}")).await.unwrap();
        let table = sim.command(rich, "play", shells_cmd(90)).await;
        assert!(!table.ephemeral, "round {round}: {}", table.content());
        let pearl: usize = sim
            .app
            .db
            .game(session_of(&table))
            .await
            .unwrap()
            .unwrap()
            .state
            .parse()
            .unwrap();
        sim.click(rich, &table, &format!("Shell {}", (pearl + 1) % 3 + 1)).await;
    }
    sim.app.cache.forget(&format!("rest:shellsstart:{rich}")).await.unwrap();
    let stop = sim.command(rich, "play", shells_cmd(50)).await;
    assert!(
        stop.ephemeral && stop.content().contains("loss limit of **300**"),
        "{}",
        stop.content()
    );
    assert_eq!(shells(&sim, rich).await, 730);
}

/// Opens a heist lobby as `host` in the sim's channel.
async fn open_heist(sim: &mut Sim, host: u64) -> Reply {
    sim.command(host, "play", play_sub("heist", vec![])).await
}

#[tokio::test]
async fn a_heist_crew_picks_roles_and_the_vault_pays_what_the_seed_says() {
    use pinchora_game::party::heist::{self, Role, Tier};
    let Some(mut sim) = Sim::new("a_heist_crew_picks_roles_and_the_vault_pays_what_the_seed_says").await else {
        return;
    };
    sim.channel = unique();
    let (host, a, b, stranger) = (unique(), unique(), unique(), unique());
    let lobby = open_heist(&mut sim, host).await;
    assert!(
        !lobby.ephemeral
            && lobby.content().contains("Treasure Heist")
            && lobby.content().contains("Nobody loses shells"),
        "{}",
        lobby.content()
    );
    assert!(lobby.pings_nobody());
    assert_png(&lobby, 600, 476);
    for label in ["Lookout", "Locksmith", "Diver", "Set off now", "Leave"] {
        assert!(lobby.has_button(label), "{label}");
    }
    let session = session_of(&lobby);
    let second = open_heist(&mut sim, stranger).await;
    assert!(
        second.ephemeral && second.content().contains("already being planned"),
        "{}",
        second.content()
    );

    // The seed decides the stages; pick one that opens the vault for this crew.
    let crew_roles = [Role::Lookout, Role::Locksmith, Role::Diver];
    let seed = (0..10_000u64)
        .find(|s| heist::run(*s, &crew_roles).tier == Tier::Vault)
        .expect("some seed opens the vault");
    sqlx::query("UPDATE game_sessions SET state = $2 WHERE id = $1")
        .bind(session)
        .bind(seed.to_string())
        .execute(sim.app.db.pool())
        .await
        .unwrap();

    let view = sim.click(host, &lobby, "Lookout").await;
    assert_png(&view, 600, 476);
    let view = sim.click(a, &view, "Diver").await;
    let view = sim.click(a, &view, "Locksmith").await;
    sim.click(b, &view, "Diver").await;
    let crew = sim.app.db.party_members(session).await.unwrap();
    assert_eq!(
        crew.iter().map(|m| (m.user_id as u64, m.choice)).collect::<Vec<_>>(),
        vec![(host, 0), (a, 1), (b, 2)],
        "a changed their mind and kept their place"
    );
    let early = sim.click(stranger, &view, "Set off now").await;
    assert!(
        early.ephemeral && early.content().contains("Only"),
        "{}",
        early.content()
    );
    let nobody = sim.click(stranger, &view, "Leave").await;
    assert!(
        nobody.ephemeral && nobody.content().contains("not in this crew"),
        "{}",
        nobody.content()
    );

    let done = sim.click(host, &view, "Set off now").await;
    assert_png(&done, 600, 504);
    assert!(
        done.content().contains("the vault is open") && done.components.is_empty(),
        "{}",
        done.content()
    );
    for (user, icon) in [(host, "🔭"), (a, "🔑"), (b, "🤿")] {
        assert!(
            done.content().contains(&format!("{icon} {} **+60**", mention(user))),
            "{}",
            done.content()
        );
        assert_eq!(shells(&sim, user).await, 60);
        assert_eq!(record_of(&sim, user, "heist").await, (1, 1));
    }
    let late = sim.click(a, &view, "Lookout").await;
    assert!(late.ephemeral && late.content().contains("over"), "{}", late.content());

    // The crew lies low after a heist, and cannot plan another at once.
    let resting = open_heist(&mut sim, host).await;
    assert!(
        resting.ephemeral && resting.content().contains("laying low"),
        "{}",
        resting.content()
    );
    let fresh = unique();
    let lobby = open_heist(&mut sim, fresh).await;
    let refused = sim.click(a, &lobby, "Diver").await;
    assert!(
        refused.ephemeral && refused.content().contains("laying low"),
        "{}",
        refused.content()
    );
}

#[tokio::test]
async fn a_busted_heist_pays_nobody_and_a_lone_crew_member_is_called_off_by_the_janitor() {
    use pinchora_game::party::heist::{self, Role, Tier};
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) =
        Sim::new("a_busted_heist_pays_nobody_and_a_lone_crew_member_is_called_off_by_the_janitor").await
    else {
        return;
    };
    forget_late_games(&sim).await;
    sim.channel = unique();
    let (host, a, loner) = (unique(), unique(), unique());
    let lobby = open_heist(&mut sim, host).await;
    let session = session_of(&lobby);
    let crew_roles = [Role::Diver, Role::Diver];
    let seed = (0..10_000u64)
        .find(|s| heist::run(*s, &crew_roles).tier == Tier::Busted)
        .expect("some seed fails this crew");
    sqlx::query("UPDATE game_sessions SET state = $2 WHERE id = $1")
        .bind(session)
        .bind(seed.to_string())
        .execute(sim.app.db.pool())
        .await
        .unwrap();
    let view = sim.click(host, &lobby, "Diver").await;
    sim.click(a, &view, "Diver").await;
    let message = give_message(&sim, session).await;
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = edit_with(&posts, message, "heist.png");
    assert!(edit.content().contains("busted"), "{}", edit.content());
    assert_eq!((shells(&sim, host).await, shells(&sim, a).await), (0, 0));
    assert_eq!(record_of(&sim, host, "heist").await, (1, 0));

    // One crew member alone is not a heist.
    sim.channel = unique();
    let lobby = open_heist(&mut sim, loner).await;
    let session = session_of(&lobby);
    sim.click(loner, &lobby, "Lookout").await;
    let early = sim.click(loner, &lobby, "Set off now").await;
    assert!(
        early.ephemeral && early.content().contains("at least 2 crew"),
        "{}",
        early.content()
    );
    let message = give_message(&sim, session).await;
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = posts
        .iter()
        .find(|p| p.message == message)
        .expect("the lobby message was edited");
    assert!(
        edit.content().contains("called off") && edit.content().contains("at least 2 crew"),
        "{}",
        edit.content()
    );
    assert_eq!(edit.body["components"].as_array().map(Vec::len), Some(0));
    assert_eq!(record_of(&sim, loner, "heist").await, (0, 0));
}

/// `/surge` as a manager.
async fn call_storm(sim: &mut Sim, manager: u64) -> Reply {
    sim.permissions = MANAGE_GUILD;
    let reply = sim.command(manager, "surge", json!([])).await;
    sim.permissions = 0;
    reply
}

#[tokio::test]
async fn a_storm_surge_is_held_back_by_the_whole_server_and_pays_everyone_who_helped() {
    let Some(mut sim) = Sim::new("a_storm_surge_is_held_back_by_the_whole_server_and_pays_everyone_who_helped").await
    else {
        return;
    };
    sim.channel = unique();
    let (manager, member) = (unique(), unique());
    let denied = sim.command(member, "surge", json!([])).await;
    assert!(
        denied.ephemeral && denied.content().contains("Manage Server"),
        "{}",
        denied.content()
    );

    let storm = call_storm(&mut sim, manager).await;
    assert!(
        !storm.ephemeral && storm.content().contains("Storm surge") && storm.has_button("Stack a sandbag"),
        "{}",
        storm.content()
    );
    assert!(storm.pings_nobody());
    assert_png(&storm, 640, 380);
    let session = session_of(&storm);
    let again = call_storm(&mut sim, manager).await;
    assert!(
        again.ephemeral && again.content().contains("breather"),
        "{}",
        again.content()
    );
    sim.app
        .cache
        .forget(&format!("rest:surge:{}", sim.guild))
        .await
        .unwrap();
    let raging = call_storm(&mut sim, manager).await;
    assert!(
        raging.ephemeral && raging.content().contains("already raging"),
        "{}",
        raging.content()
    );

    let helpers: Vec<u64> = (0..3).map(|_| unique()).collect();
    let stack = |sim: &Sim, user: u64| {
        let key = format!("rest:surgehelp:{session}:{user}");
        let cache = sim.app.cache.clone();
        async move { cache.forget(&key).await.unwrap() }
    };
    let first = sim.click(helpers[0], &storm, "Stack a sandbag").await;
    assert!(
        first.ephemeral && first.content().contains("Sandbag **1** stacked"),
        "{}",
        first.content()
    );
    let eager = sim.click(helpers[0], &storm, "Stack a sandbag").await;
    assert!(
        eager.ephemeral && eager.content().contains("claws need a rest"),
        "{}",
        eager.content()
    );

    // Two helpers stack 18 each, the third finishes the wall: 24 + 8 * 3 = 48 sandbags.
    for round in 1..18 {
        stack(&sim, helpers[0]).await;
        let reply = sim.click(helpers[0], &storm, "Stack a sandbag").await;
        assert!(
            reply.content().contains(&format!("Sandbag **{}** stacked", round + 1)),
            "{}",
            reply.content()
        );
    }
    for _ in 0..18 {
        stack(&sim, helpers[1]).await;
        sim.click(helpers[1], &storm, "Stack a sandbag").await;
    }
    let mut last = None;
    for n in 0..12 {
        stack(&sim, helpers[2]).await;
        let reply = sim.click(helpers[2], &storm, "Stack a sandbag").await;
        if n < 11 {
            assert!(
                reply.ephemeral && reply.content().contains("The wall stands at"),
                "{}",
                reply.content()
            );
        } else {
            last = Some(reply);
        }
    }
    let held = last.expect("the last sandbag");
    assert!(
        held.content().contains("The wall held") && held.components.is_empty(),
        "{}",
        held.content()
    );
    assert_png(&held, 640, 380);
    // 20 + 2 shells for each of the first 15 sandbags, then 12 sandbags for the third.
    for (user, bags, shells_paid) in [
        (helpers[0], 18, 20 + 2 * 15),
        (helpers[1], 18, 20 + 2 * 15),
        (helpers[2], 12, 20 + 2 * 12),
    ] {
        assert!(
            held.content()
                .contains(&format!("{} ×{bags} **+{shells_paid}**", mention(user))),
            "{}",
            held.content()
        );
        assert_eq!(shells(&sim, user).await, shells_paid);
        assert_eq!(record_of(&sim, user, "surge").await, (1, 1));
    }
    sim.app
        .cache
        .forget(&format!("rest:surgehelp:{session}:{}", helpers[1]))
        .await
        .unwrap();
    let late = sim.click(helpers[1], &storm, "Stack a sandbag").await;
    assert!(
        late.ephemeral && late.content().contains("passed"),
        "{}",
        late.content()
    );
    assert_eq!(record_of(&sim, member, "surge").await, (0, 0));
}

#[tokio::test]
async fn the_janitor_keeps_the_storm_card_fresh_and_the_sea_wins_when_time_runs_out() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("the_janitor_keeps_the_storm_card_fresh_and_the_sea_wins_when_time_runs_out").await
    else {
        return;
    };
    forget_late_games(&sim).await;
    sim.channel = unique();
    let (manager, a, b) = (unique(), unique(), unique());
    let storm = call_storm(&mut sim, manager).await;
    let session = session_of(&storm);
    let message = give_message(&sim, session).await;
    sim.click(a, &storm, "Stack a sandbag").await;
    sim.click(b, &storm, "Stack a sandbag").await;
    sim.app
        .cache
        .forget(&format!("rest:surgehelp:{session}:{a}"))
        .await
        .unwrap();
    sim.click(a, &storm, "Stack a sandbag").await;

    // The card catches up with the wall once, and not again until something changes.
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let refreshed = edit_with(&posts, message, "surge.png");
    assert!(
        refreshed.content().contains("Storm surge") && !refreshed.body["components"].as_array().unwrap().is_empty()
    );
    sweep_games(&sim.app).await.expect("sweep again");
    assert!(
        sim.posts().iter().all(|p| p.message != message),
        "nothing changed, nothing redrawn"
    );
    sim.app
        .cache
        .forget(&format!("rest:surgehelp:{session}:{b}"))
        .await
        .unwrap();
    sim.click(b, &storm, "Stack a sandbag").await;
    sweep_games(&sim.app).await.expect("sweep once more");
    assert!(
        sim.posts().iter().any(|p| p.message == message),
        "a new sandbag redraws the card"
    );

    // Time runs out with the wall too low: the sea wins and the helpers get a small thanks.
    expire(&sim, session).await;
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let edit = edit_with(&posts, message, "surge.png");
    assert!(
        edit.content().contains("The sea broke through the wall"),
        "{}",
        edit.content()
    );
    assert_eq!(edit.body["components"].as_array().map(Vec::len), Some(0));
    // Each stacked two sandbags: 6 shells for trying and 1 for each sandbag.
    assert_eq!(shells(&sim, a).await, 6 + 2);
    assert_eq!(shells(&sim, b).await, 6 + 2);
    assert_eq!(record_of(&sim, a, "surge").await, (1, 0));
    sweep_games(&sim.app).await.expect("sweep after");
    assert_eq!(shells(&sim, a).await, 8, "paid once");
    sim.app
        .cache
        .forget(&format!("rest:surgehelp:{session}:{a}"))
        .await
        .unwrap();
    let late = sim.click(a, &storm, "Stack a sandbag").await;
    assert!(
        late.ephemeral && late.content().contains("passed"),
        "{}",
        late.content()
    );
}

#[tokio::test]
async fn timed_storm_surges_are_switched_on_per_server_and_come_when_they_are_due() {
    let _serial = SWEEPS.lock().await;
    let Some(mut sim) = Sim::new("timed_storm_surges_are_switched_on_per_server_and_come_when_they_are_due").await
    else {
        return;
    };
    forget_late_games(&sim).await;
    let (manager, helper, lounge) = (unique(), unique(), unique());
    sim.permissions = MANAGE_GUILD;
    let config = |options: Vec<Value>| json!([sub("surge", options)]);
    let off = sim.command(manager, "config", config(vec![])).await;
    assert!(off.ephemeral && off.content().contains("**off**"), "{}", off.content());
    let needs = sim
        .command(manager, "config", config(vec![bool_option("enabled", true)]))
        .await;
    assert!(needs.content().contains("need a channel"), "{}", needs.content());
    let on = sim
        .command_resolved(
            manager,
            "config",
            config(vec![channel_option("channel", lounge), int_option("every", 6)]),
            resolved_channels(&[lounge]),
        )
        .await;
    assert!(
        on.content().contains(&format!("<#{lounge}>")) && on.content().contains("every **6** hours"),
        "{}",
        on.content()
    );
    let next = sim.app.db.party_settings(sim.guild).await.unwrap().surge_next_at;
    let shown = sim.command(manager, "config", config(vec![])).await;
    assert!(shown.content().contains("every **6** hours"), "{}", shown.content());
    assert_eq!(
        sim.app.db.party_settings(sim.guild).await.unwrap().surge_next_at,
        next,
        "looking at the setting does not move the next storm"
    );
    sim.permissions = 0;
    let refused = sim
        .command(helper, "config", config(vec![bool_option("enabled", false)]))
        .await;
    assert!(refused.content().contains("Manage Server"), "{}", refused.content());
    sweep_games(&sim.app).await.expect("sweep");
    assert!(
        sim.posts().iter().all(|p| p.channel != lounge),
        "nothing before it is due"
    );
    sim.app.shift_clock(chrono::Duration::hours(7));
    sweep_games(&sim.app).await.expect("sweep");
    let posts = sim.posts();
    let storm = posts
        .iter()
        .find(|p| p.channel == lounge && p.method == Method::POST)
        .expect("the storm came to the channel");
    assert!(storm.content().contains("Storm surge") && storm.files.iter().any(|(n, _)| n == "surge.png"));
    let reply = posted_reply(storm);
    assert!(reply.has_button("Stack a sandbag"));
    let running = sim.app.db.running_surges(1_000).await.unwrap();
    let mine = running
        .iter()
        .find(|s| s.guild_id == sim.guild as i64)
        .expect("a storm is running here");
    assert_eq!(mine.channel_id, lounge as i64);
    assert!(mine.message_id.is_some(), "the janitor knows the message it posted");
    sweep_games(&sim.app).await.expect("sweep");
    assert!(
        sim.posts().iter().all(|p| p.method != Method::POST),
        "one storm per slot: the next comes six hours later"
    );
    let helped = sim.click(helper, &reply, "Stack a sandbag").await;
    assert!(
        helped.content().contains("Sandbag **1** stacked"),
        "{}",
        helped.content()
    );

    // Switched off, no more storms; clean up so no stale slot is left in the database.
    sim.permissions = MANAGE_GUILD;
    let off = sim
        .command(manager, "config", config(vec![bool_option("enabled", false)]))
        .await;
    assert!(off.content().contains("**off**"), "{}", off.content());
    expire(&sim, mine.id).await;
    sweep_games(&sim.app).await.expect("sweep");
    assert!(
        sim.app
            .db
            .party_settings(sim.guild)
            .await
            .unwrap()
            .surge_next_at
            .is_none()
    );
}

#[tokio::test]
async fn the_party_games_are_in_the_stats_and_the_help() {
    let Some(mut sim) = Sim::new("the_party_games_are_in_the_stats_and_the_help").await else {
        return;
    };
    let player = unique();
    let stats = sim.command(player, "play", json!([sub("stats", vec![])])).await;
    let fields: Vec<&str> = stats.embeds[0].fields.iter().map(|f| f.name.as_str()).collect();
    for game in [
        "Crab Derby",
        "Share or Grab",
        "Shell Game",
        "Treasure Heist",
        "Storm Surge",
    ] {
        assert!(fields.iter().any(|f| f.contains(game)), "{game} in {fields:?}");
    }
    // /help lists them on its pages (every command is checked against the registered tree by a unit
    // test); here the pages that hold them exist and open.
    let help = sim.command(player, "help", json!([])).await;
    let areas = help_areas(&help);
    for area in ["Crowd games & collecting", "Duels", "Game & module setup"] {
        assert!(areas.iter().any(|a| a == area), "{area} in {areas:?}");
    }
}

/// The areas the help menu lists, once /help is seen to answer privately with a card and controls.
fn help_areas(help: &Reply) -> Vec<String> {
    assert!(help.ephemeral, "the help is for the one who asked");
    assert_eq!(help.files.len(), 1, "one card");
    assert!(help.button("Next ▶").custom_id.is_some());
    help.options("hp:s").iter().map(|o| o.label.clone()).collect()
}

// /Modules

/// `/config modules <sub_command>` as a manager; `channels` are what the options name.
async fn modules(sim: &mut Sim, manager: u64, sub_command: Value, channels: &[u64]) -> Reply {
    let before = sim.permissions;
    sim.permissions = MANAGE_GUILD;
    let reply = sim
        .command_resolved(
            manager,
            "config",
            json!([group("modules", sub_command)]),
            resolved_channels(channels),
        )
        .await;
    sim.permissions = before;
    reply
}

fn toggle(module: &str, on: bool) -> Value {
    sub(
        "toggle",
        vec![text_option("module", module), bool_option("enabled", on)],
    )
}

fn limit(module: &str, place: &str, channel: u64, remove: bool) -> Value {
    let mut options = vec![
        text_option("module", module),
        text_option("where", place),
        channel_option("channel", channel),
    ];
    if remove {
        options.push(bool_option("remove", true));
    }
    sub("channels", options)
}

#[tokio::test]
async fn module_off_refuses_commands_and_xp() {
    let Some(mut sim) = Sim::new("module_off_refuses_commands_and_xp").await else {
        return;
    };
    let _turn = VOICE_TICKS.lock().await;
    sim.app.shift_clock(chrono::Duration::days((unique() % 36_500) as i64));
    let (manager, member) = (unique(), unique());
    let (category, here, there, thread, loose) = (unique(), unique(), unique(), unique(), unique());
    let (voice, loose_voice) = (unique(), unique());
    sim.channel_is(category, 4, None);
    sim.channel_is(here, 0, Some(category));
    sim.channel_is(there, 0, Some(category));
    sim.channel_is(thread, 11, Some(here));
    sim.channel_is(loose, 0, None);
    sim.channel_is(voice, 2, Some(category));
    sim.channel_is(loose_voice, 2, None);
    sim.channel = here;
    let pet = || json!([sub("view", vec![])]);

    // Everything is on to begin with.
    let first = sim.command(member, "pet", pet()).await;
    assert!(!first.content().contains("turned off"), "{}", first.content());

    // A manager switches pets off for the whole server: every way in is shut, privately.
    let off = modules(&mut sim, manager, toggle("pets", false), &[]).await;
    assert!(
        off.ephemeral && off.content().contains("**Pets**") && off.content().contains("off"),
        "{}",
        off.content()
    );
    let shown = modules(&mut sim, manager, sub("show", vec![]), &[]).await;
    assert!(shown.ephemeral);
    assert!(
        shown.field("Home, pets and fishing").contains("**Pets** · off"),
        "{:?}",
        shown.embeds[0].fields
    );
    assert!(
        shown.field("Server tools").contains("**Leveling** · on"),
        "{:?}",
        shown.embeds[0].fields
    );
    assert!(
        shown.field("Games").contains("**Gambling-style games** · off"),
        "{:?}",
        shown.embeds[0].fields
    );

    let refused = sim.command(member, "pet", pet()).await;
    assert!(refused.ephemeral, "a refusal is private");
    assert_eq!(refused.content(), "This feature is turned off on this server.");
    let button = sim.component(member, "pc:feed:1", &[], &refused).await;
    assert!(
        button.ephemeral && button.content() == "This feature is turned off on this server.",
        "{}",
        button.content()
    );
    let menu = sim.component(member, "pa", &["kitten"], &refused).await;
    assert_eq!(menu.content(), "This feature is turned off on this server.");
    // A manager is told the way back.
    sim.permissions = MANAGE_GUILD;
    let manager_sees = sim.command(manager, "pet", pet()).await;
    assert!(
        manager_sees.content().contains("/config modules") && manager_sees.content().contains("**Pets**"),
        "{}",
        manager_sees.content()
    );
    sim.permissions = 0;

    // Never gated: the way back in, the basics. Other modules carry on.
    let help = sim.command(member, "help", json!([])).await;
    assert!(!help.content().contains("turned off"), "{}", help.content());
    let ping = sim.command(member, "ping", json!([])).await;
    assert!(!ping.content().contains("turned off"), "{}", ping.content());
    let daily = sim.command(member, "daily", json!([])).await;
    assert!(!daily.content().contains("turned off"), "{}", daily.content());
    let hangman = sim.command(member, "play", json!([sub("hangman", vec![])])).await;
    assert!(!hangman.content().contains("turned off"), "{}", hangman.content());

    // Written down, with who did it and from where; switching back on is just as quick.
    let rows = sim.app.db.audit_page(sim.guild, None, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (
            rows[0].source.as_str(),
            rows[0].action.as_str(),
            rows[0].target.as_deref()
        ),
        ("discord", "module.set", Some("pets"))
    );
    assert_eq!(rows[0].actor_id as u64, manager);
    assert_eq!(rows[0].before.as_deref(), Some(r#"{"enabled": true}"#));
    assert_eq!(rows[0].after.as_deref(), Some(r#"{"enabled": false}"#));
    modules(&mut sim, manager, toggle("pets", false), &[]).await;
    assert_eq!(
        sim.app.db.audit_page(sim.guild, None, 10).await.unwrap().len(),
        1,
        "no change, no row"
    );
    modules(&mut sim, manager, toggle("pets", true), &[]).await;
    let back = sim.command(member, "pet", pet()).await;
    assert!(!back.content().contains("turned off"), "{}", back.content());

    // Where it works: a channel list, a category covering what is under it, threads included.
    let only = modules(&mut sim, manager, limit("pets", "only", there, false), &[there]).await;
    assert!(
        only.content().contains("only in 1 channel") && only.content().contains(&format!("<#{there}>")),
        "{}",
        only.content()
    );
    let blocked = sim.command(member, "pet", pet()).await;
    assert_eq!(blocked.content(), "This feature is turned off in this channel.");
    sim.channel = there;
    let allowed = sim.command(member, "pet", pet()).await;
    assert!(!allowed.content().contains("turned off"), "{}", allowed.content());
    sim.channel = here;
    let everywhere_but = modules(&mut sim, manager, limit("pets", "except", category, false), &[category]).await;
    assert!(
        everywhere_but.content().contains("off in 1 channel"),
        "{}",
        everywhere_but.content()
    );
    for (channel, why) in [
        (here, "a channel in the category"),
        (thread, "a thread of a channel in the category"),
        (there, "its sibling"),
    ] {
        sim.channel = channel;
        let reply = sim.command(member, "pet", pet()).await;
        assert_eq!(reply.content(), "This feature is turned off in this channel.", "{why}");
    }
    sim.channel = loose;
    let loose_reply = sim.command(member, "pet", pet()).await;
    assert!(!loose_reply.content().contains("turned off"), "outside the category");
    // The last channel taken off the list opens it up again.
    modules(&mut sim, manager, limit("pets", "except", category, true), &[category]).await;
    sim.channel = here;
    let open = sim.command(member, "pet", pet()).await;
    assert!(!open.content().contains("turned off"), "{}", open.content());
    let shown = modules(&mut sim, manager, sub("show", vec![]), &[]).await;
    assert!(
        shown.field("Home, pets and fishing").contains("**Pets** · on"),
        "{:?}",
        shown.embeds[0].fields
    );

    // The slash command's own refusals are the dashboard's sentences.
    let whole = modules(&mut sim, manager, limit("welcome", "only", here, false), &[here]).await;
    assert!(
        whole.content().contains("works for the whole server"),
        "{}",
        whole.content()
    );
    let nothing = modules(
        &mut sim,
        manager,
        sub(
            "channels",
            vec![text_option("module", "pets"), text_option("where", "only")],
        ),
        &[],
    )
    .await;
    assert!(nothing.content().contains("Pick a channel"), "{}", nothing.content());
    assert!(
        !sim.app
            .db
            .module_states(sim.guild)
            .await
            .unwrap()
            .iter()
            .any(|(m, r)| *m == pinchora_core::modules::Module::Welcome
                && r.mode != pinchora_core::modules::ChannelMode::All)
    );

    // The party and the duel games are different modules, and the shell game's switch is the
    // gambling column.
    modules(&mut sim, manager, toggle("party", false), &[]).await;
    for (command, sub_name) in [("play", "derby"), ("play", "heist"), ("duel", "share")] {
        let reply = sim.command(member, command, json!([sub(sub_name, vec![])])).await;
        assert_eq!(
            reply.content(),
            "This feature is turned off on this server.",
            "/{command} {sub_name}"
        );
    }
    let stats = sim.command(member, "play", json!([sub("stats", vec![])])).await;
    assert!(!stats.content().contains("turned off"), "stats belong to profiles");
    modules(&mut sim, manager, toggle("party", true), &[]).await;
    assert!(!sim.app.db.gambling_enabled(sim.guild).await.unwrap());
    modules(&mut sim, manager, toggle("gambling", true), &[]).await;
    assert!(
        sim.app.db.gambling_enabled(sim.guild).await.unwrap(),
        "the module is the /config gambling switch"
    );

    // Chat XP and points stop in the channels leveling and points are off in; the rest earns.
    modules(
        &mut sim,
        manager,
        limit("leveling", "except", category, false),
        &[category],
    )
    .await;
    let (muted, free) = (unique(), unique());
    sim.say_in(muted, here, &[]).await;
    sim.say_in(free, loose, &[]).await;
    assert_eq!(sim.server_xp(muted).await, 0, "leveling is off under the category");
    assert!(sim.points(muted).await > 0, "points are a module of their own");
    assert!(
        (15..=25).contains(&sim.server_xp(free).await),
        "{}",
        sim.server_xp(free).await
    );
    modules(
        &mut sim,
        manager,
        limit("leveling", "except", category, true),
        &[category],
    )
    .await;
    modules(&mut sim, manager, limit("points", "except", there, false), &[there]).await;
    let (no_points, points) = (unique(), unique());
    sim.say_in(no_points, there, &[]).await;
    sim.say_in(points, here, &[]).await;
    assert!(sim.server_xp(no_points).await > 0, "XP is a different module");
    assert_eq!(sim.points(no_points).await, 0);
    assert!(sim.points(points).await > 0);
    // A switch kept in the settings is the settings' switch.
    modules(&mut sim, manager, toggle("leveling", false), &[]).await;
    assert!(!sim.app.db.guild_settings(sim.guild).await.unwrap().leveling_enabled);
    let silent = unique();
    sim.say_in(silent, here, &[]).await;
    assert_eq!(sim.server_xp(silent).await, 0);
    modules(&mut sim, manager, toggle("leveling", true), &[]).await;

    // Voice XP follows the module rule of the voice channel's category.
    modules(
        &mut sim,
        manager,
        limit("leveling", "except", category, false),
        &[category],
    )
    .await;
    sim.permissions = MANAGE_GUILD;
    sim.xp(
        manager,
        sub("voice", vec![bool_option("enabled", true), int_option("amount", 20)]),
    )
    .await;
    let (v1, v2, w1, w2) = (unique(), unique(), unique(), unique());
    for (user, channel) in [(v1, voice), (v2, voice), (w1, loose_voice), (w2, loose_voice)] {
        sim.voice_in(user, Some(channel), &[], false, false, false).await;
    }
    sim.app.shift_clock(chrono::Duration::seconds(60));
    pinchora_commands::voice_tick(&sim.app).await.expect("tick");
    assert_eq!(
        (sim.server_xp(v1).await, sim.server_xp(v2).await),
        (0, 0),
        "no voice XP in a channel it is off in"
    );
    assert_eq!((sim.server_xp(w1).await, sim.server_xp(w2).await), (20, 20));
}

#[tokio::test]
async fn module_off_stops_chat_drops_goodbyes_and_the_log() {
    let Some(mut sim) = Sim::new("module_off_stops_chat_drops_goodbyes_and_the_log").await else {
        return;
    };
    let manager = unique();
    let (category, here, loose, bye, log) = (unique(), unique(), unique(), unique(), unique());
    sim.channel_is(category, 4, None);
    sim.channel_is(here, 0, Some(category));
    sim.channel_is(loose, 0, None);
    sim.permissions = MANAGE_GUILD;

    // Critter drops: counted only where the collection module works.
    sim.command(
        manager,
        "config",
        json!([sub("critters", vec![bool_option("enabled", true)])]),
    )
    .await;
    modules(
        &mut sim,
        manager,
        limit("collection", "except", category, false),
        &[category],
    )
    .await;
    for _ in 0..30 {
        sim.say_in(unique(), here, &[]).await;
    }
    assert!(sim.posts().is_empty(), "nothing washes up where the module is off");
    for _ in 0..25 {
        sim.say_in(unique(), loose, &[]).await;
    }
    let posts = sim.posts();
    assert_eq!(in_channel(&posts, loose).len(), 1, "the other channels count as usual");

    // Goodbye and the log are modules of their own.
    sim.command_resolved(
        manager,
        "config",
        json!([sub("goodbye", vec![channel_option("channel", bye)])]),
        resolved_channels(&[bye]),
    )
    .await;
    sim.command_resolved(
        manager,
        "config",
        json!([sub("log", vec![channel_option("channel", log)])]),
        resolved_channels(&[log]),
    )
    .await;
    sim.leave(unique(), "Ana", false).await;
    let posts = sim.posts();
    assert_eq!((in_channel(&posts, bye).len(), in_channel(&posts, log).len()), (1, 1));

    modules(&mut sim, manager, toggle("goodbye", false), &[]).await;
    sim.leave(unique(), "Ben", false).await;
    let posts = sim.posts();
    assert_eq!(
        (in_channel(&posts, bye).len(), in_channel(&posts, log).len()),
        (0, 1),
        "the log carries on"
    );

    modules(&mut sim, manager, toggle("log", false), &[]).await;
    sim.leave(unique(), "Cy", false).await;
    sim.join(unique(), "Di", false, None).await;
    sim.ban(unique(), "Ed", true).await;
    assert!(sim.posts().is_empty(), "switched off, the log says nothing");

    modules(&mut sim, manager, toggle("log", true), &[]).await;
    sim.leave(unique(), "Flo", false).await;
    assert_eq!(in_channel(&sim.posts(), log).len(), 1);

    // Welcome cards are the settings' `welcome_enabled`.
    modules(&mut sim, manager, toggle("welcome", true), &[]).await;
    assert!(sim.app.db.guild_settings(sim.guild).await.unwrap().welcome_enabled);
    modules(&mut sim, manager, toggle("welcome", false), &[]).await;
    assert!(!sim.app.db.guild_settings(sim.guild).await.unwrap().welcome_enabled);
}

/// What Discord would suggest while `typed` is in the `module` box of `/config modules <sub_command>`.
async fn suggestions(sim: &Sim, user: u64, sub_command: &str, typed: &str) -> Vec<(String, String)> {
    let data = json!({
        "id": unique().to_string(),
        "name": "config",
        "type": 1,
        "options": [{ "name": "modules", "type": 2, "options": [{
            "name": sub_command,
            "type": 1,
            "options": [{ "name": "module", "type": 3, "value": typed, "focused": true }],
        }] }],
    });
    let interaction = sim.interaction(user, 4, data, None);
    let (tx, mut rx) = oneshot::channel();
    let responder = Arc::new(Responder::http(sim.app.clone(), &interaction, tx));
    assert_eq!(responder.deferral(), Deferral::Never, "suggestions are never deferred");
    handle_interaction(sim.app.clone(), interaction, responder).await;
    let first = rx.try_recv().expect("the autocomplete was never answered");
    assert_eq!(
        first.kind,
        InteractionResponseType::ApplicationCommandAutocompleteResult
    );
    first
        .data
        .and_then(|data| data.choices)
        .expect("suggestions")
        .into_iter()
        .map(|choice| match choice.value {
            twilight_model::application::command::CommandOptionChoiceValue::String(key) => (choice.name, key),
            other => panic!("a suggestion that is not text: {other:?}"),
        })
        .collect()
}

#[tokio::test]
async fn module_names_autocomplete_and_a_typed_name_works() {
    let Some(mut sim) = Sim::new("module_names_autocomplete_and_a_typed_name_works").await else {
        return;
    };
    let (manager, member) = (unique(), unique());

    // More modules than Discord's 25 choices: the box suggests as you type, 25 at most.
    assert!(pinchora_core::modules::Module::ALL.len() > 25);
    let all = suggestions(&sim, member, "toggle", "").await;
    assert_eq!(all.len(), 25);
    let chess = suggestions(&sim, member, "channels", "chess").await;
    assert_eq!(chess, [("Board and puzzle games".to_owned(), "board_games".to_owned())]);
    let party = suggestions(&sim, member, "toggle", "party").await;
    let keys: Vec<&str> = party.iter().map(|(_, key)| key.as_str()).collect();
    assert!(keys.contains(&"party") && keys.contains(&"party_games"), "{keys:?}");
    assert!(suggestions(&sim, member, "toggle", "zzzz").await.is_empty());
    for module in pinchora_core::modules::Module::ALL {
        let found = suggestions(&sim, member, "toggle", module.label()).await;
        assert!(found.iter().any(|(_, key)| key == module.key()), "{}", module.key());
    }

    // The name as typed, or the key as picked, both name the module; anything else is told so privately.
    let by_name = modules(&mut sim, manager, toggle("Board and puzzle games", false), &[]).await;
    assert!(
        by_name.ephemeral && by_name.content().contains("**Board and puzzle games** · off"),
        "{}",
        by_name.content()
    );
    let by_key = modules(&mut sim, manager, toggle("board_games", true), &[]).await;
    assert!(
        by_key.content().contains("**Board and puzzle games** · on"),
        "{}",
        by_key.content()
    );
    let unknown = modules(&mut sim, manager, toggle("boards", false), &[]).await;
    assert!(
        unknown.ephemeral && unknown.content().contains("isn't a module"),
        "{}",
        unknown.content()
    );
    let channel = sim.channel;
    let unknown = modules(&mut sim, manager, limit("chess", "only", channel, false), &[channel]).await;
    assert!(unknown.content().contains("isn't a module"), "{}", unknown.content());
}

#[tokio::test]
async fn module_off_refuses_the_board_and_word_party_games() {
    let Some(mut sim) = Sim::new("module_off_refuses_the_board_and_word_party_games").await else {
        return;
    };
    let (manager, member) = (unique(), unique());
    let off = "This feature is turned off on this server.";

    modules(&mut sim, manager, toggle("board_games", false), &[]).await;
    let mut refused = None;
    for (command, options) in [
        ("duel", json!([sub("seafleet", vec![])])),
        ("duel", json!([sub("chess", vec![])])),
        ("duel", json!([sub("checkers", vec![])])),
        ("play", json!([sub("picross", vec![])])),
        ("play", json!([sub("2048", vec![])])),
    ] {
        let reply = sim.command(member, command, options.clone()).await;
        assert!(
            reply.ephemeral && reply.content() == off,
            "/{command} {options}: {}",
            reply.content()
        );
        refused = Some(reply);
    }
    let refused = refused.unwrap();
    for id in ["bs:y:1", "ck:y:1", "cc:y:1", "pz:r:1", "tz:l:1"] {
        let click = sim.component(member, id, &[], &refused).await;
        assert!(click.ephemeral && click.content() == off, "{id}: {}", click.content());
    }
    let form = sim.component(member, "pz:m:1", &[], &refused).await;
    assert_eq!(form.content(), off, "a picross form passes the same switch");
    // The other games carry on: another module, and the stats that belong to profiles.
    for (command, options) in [
        ("play", json!([sub("hangman", vec![])])),
        ("play", json!([sub("stats", vec![])])),
        ("party", json!([sub("stop", vec![])])),
    ] {
        let reply = sim.command(member, command, options).await;
        assert!(
            !reply.content().contains("turned off"),
            "/{command}: {}",
            reply.content()
        );
    }

    modules(&mut sim, manager, toggle("board_games", true), &[]).await;
    modules(&mut sim, manager, toggle("party_games", false), &[]).await;
    let mut refused = None;
    for name in [
        "hotpearl",
        "acronym",
        "postcard",
        "wordrace",
        "wouldyourrather",
        "truthordare",
        "stop",
    ] {
        let reply = sim.command(member, "party", json!([sub(name, vec![])])).await;
        assert!(
            reply.ephemeral && reply.content() == off,
            "/party {name}: {}",
            reply.content()
        );
        refused = Some(reply);
    }
    let refused = refused.unwrap();
    for id in ["hl:j:1", "ac:v:1:1", "cn:v:1:1", "wx:g:1", "wy:a:1", "td:t:1"] {
        let click = sim.component(member, id, &[], &refused).await;
        assert!(click.ephemeral && click.content() == off, "{id}: {}", click.content());
    }
    let hangman = sim.command(member, "play", json!([sub("hangman", vec![])])).await;
    assert!(!hangman.content().contains("turned off"), "{}", hangman.content());
    // The crab party games are a module of their own.
    let derby = sim.command(member, "play", json!([sub("derby", vec![])])).await;
    assert!(!derby.content().contains("turned off"), "{}", derby.content());
}
