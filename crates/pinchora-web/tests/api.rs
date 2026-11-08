//! Drives the dashboard like a browser would, with Discord replaced by a local mock.
//!
//! The mock answers the OAuth token endpoint, the user-token routes (`/users/@me`, its guilds) and,
//! through twilight's proxy setting, the bot-token routes. A `Browser` keeps cookies and the CSRF
//! token and sends requests straight into the router.
//!
//! Needs `PINCHORA_TEST_DATABASE_URL` and `PINCHORA_TEST_REDIS_URL`; skipped when either is unset.

use std::{
    collections::{HashMap, HashSet},
    net::{Ipv6Addr, SocketAddr},
    path::PathBuf,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use axum::{
    Form, Router,
    body::{Body, Bytes},
    extract::{ConnectInfo, State},
    http::{HeaderMap, Method, Request, StatusCode, Uri, header},
    routing::post,
};
use base64::Engine;
use futures_util::StreamExt;
use pinchora_cache::{Cache, Invalidation};
use pinchora_commands::{
    App, CardSubject, PreviewError, card_png, listen_for_invalidations, on_member_add, on_message,
};
use pinchora_core::{card_design::CardKind, config::WebConfig, i18n::Lang};
use pinchora_db::{Db, XpGrant};
use pinchora_web::{WebSettings, WebState};
use reqwest::Url;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tower::ServiceExt;
use twilight_http::Client;
use twilight_model::{
    gateway::payload::incoming::{MemberAdd, MessageCreate},
    id::Id,
};

#[path = "api/achievements.rs"]
mod achievements;
#[path = "api/web_i18n.rs"]
mod web_i18n;

// Custom commands and auto-responders.
#[path = "api/stats.rs"]
mod stats;
#[path = "api/tags.rs"]
mod tags;
// The roles page: auto roles, sticky roles, role panels and colour roles.
#[path = "api/roles.rs"]
mod roles;
// Tickets: settings, panels, the tickets list and transcripts.
#[path = "api/invites.rs"]
mod invites;
#[path = "api/starboard.rs"]
mod starboard;
#[path = "api/tickets.rs"]
mod tickets;
// The Feeds page.
#[path = "api/feeds.rs"]
mod feeds;
// Temp voice: creators and the live list of rooms.
#[path = "api/anti_nuke.rs"]
mod anti_nuke;
#[path = "api/birthdays.rs"]
mod birthdays;
#[path = "api/branding.rs"]
mod branding;
#[path = "api/events.rs"]
mod events;
#[path = "api/tempvoice.rs"]
mod tempvoice;
#[path = "api/utility.rs"]
mod utility;
#[path = "api/verification.rs"]
mod verification;
mod xp_admin;

const APPLICATION_ID: u64 = 1;
const PUBLIC_URL: &str = "http://dash.test";
const MANAGE_GUILD: u64 = 1 << 5;
const ADMINISTRATOR: u64 = 1 << 3;
const SEND_MESSAGES: u64 = 1 << 11;
const MANAGE_ROLES: u64 = 1 << 28;

// The server every mocked guild looks like: a category with a text channel, an announcement and a
// voice channel; roles below the bot's, one above it, and one an integration manages.
const CATEGORY: u64 = 7_004;
const GENERAL: u64 = 7_001;
const NEWS: u64 = 7_002;
const LOUNGE: u64 = 7_003;
const BOT_ROLE: u64 = 9_000;
const BOT_ROLE_POSITION: i64 = 10;
const FAN: u64 = 9_001;
const REGULAR: u64 = 9_002;
const ADMIN: u64 = 9_003;
const BOOSTER: u64 = 9_004;
/// Below the bot, with Manage Messages: a role only the owner or an administrator may set up.
const HELPER: u64 = 9_005;
const MANAGE_MESSAGES: u64 = 1 << 13;

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

/// A user Discord knows: their JSON and their server list.
type Account = (Value, Vec<Value>);

/// What the mock Discord knows: users by access token, the codes it handed out, where the bot is.
#[derive(Clone, Default)]
struct Mock {
    /// Access token → the account.
    users: Arc<Mutex<HashMap<String, Account>>>,
    /// Authorization code → (access token, the PKCE challenge it was issued for).
    codes: Arc<Mutex<HashMap<String, (String, String)>>>,
    /// Servers the bot is in.
    bot_guilds: Arc<Mutex<HashSet<u64>>>,
    profile_patches: Arc<Mutex<Vec<Value>>>,
    refuse_banner: Arc<AtomicBool>,
    refuse_avatar: Arc<AtomicBool>,
    refuse_profile: Arc<AtomicBool>,
    guild_owners: Arc<Mutex<HashMap<u64, u64>>>,
    /// Forms posted to the token endpoint, oldest first.
    token_forms: Arc<Mutex<Vec<HashMap<String, String>>>>,
    /// Tokens revoked at logout.
    revoked: Arc<Mutex<Vec<String>>>,
    /// Hand out tokens that are about to expire, so the next use refreshes them.
    short_tokens: Arc<AtomicBool>,
    /// Refuse refresh tokens, as Discord does once the user removed the app.
    refuse_refresh: Arc<AtomicBool>,
    /// Hand out a new refresh token with every refresh and refuse a used one, as Discord does.
    rotate: Arc<AtomicBool>,
    /// Refresh tokens already traded in.
    used_refresh: Arc<Mutex<HashSet<String>>>,
    /// Makes the token endpoint slow, to widen races.
    token_delay_ms: Arc<AtomicU64>,
    /// Roles each member holds.
    member_roles: Arc<Mutex<HashMap<u64, Vec<u64>>>>,
    /// Role changes, as `PUT user role` / `DELETE user role`.
    role_calls: Arc<Mutex<Vec<String>>>,
    /// How many times a server's channels were asked for.
    channel_lookups: Arc<AtomicUsize>,
    guild_lookups: Arc<AtomicUsize>,
    guild_delay_ms: Arc<AtomicU64>,
    /// Give @everyone View Channel but deny it on GENERAL for public-stats tests.
    stats_public_channels: Arc<AtomicBool>,
    /// Where the bot's own role sits, so a test can move it between two saves.
    bot_position: Arc<AtomicI64>,
    /// Messages the bot posted: the channel and the request body as text (JSON, or multipart with files).
    posts: Arc<Mutex<Vec<(u64, String)>>>,
}

fn user_json(id: u64, name: &str) -> Value {
    json!({
        "id": id.to_string(),
        "username": name,
        "discriminator": "0",
        "global_name": format!("{name} G"),
        "avatar": null,
    })
}

fn guild_entry(id: u64, name: &str, owner: bool, permissions: u64) -> Value {
    json!({
        "id": id.to_string(),
        "name": name,
        "icon": "a1b2c3",
        "owner": owner,
        "permissions": permissions.to_string(),
        "features": [],
    })
}

fn role_json(id: u64, name: &str, position: i64, managed: bool, permissions: u64) -> Value {
    json!({
        "id": id.to_string(),
        "name": name,
        "color": 0,
        "colors": { "primary_color": 0xff6b4a, "secondary_color": null, "tertiary_color": null },
        "hoist": false,
        "managed": managed,
        "mentionable": false,
        "permissions": permissions.to_string(),
        "position": position,
        "flags": 0,
    })
}

/// `GET /guilds/{id}/roles`: `@everyone` (the server's id) and the fixed roles.
fn roles_json(guild: u64, bot_position: i64) -> Value {
    json!([
        role_json(guild, "@everyone", 0, false, SEND_MESSAGES),
        role_json(BOT_ROLE, "Pinchora", bot_position, true, MANAGE_ROLES),
        role_json(FAN, "Fan", 3, false, 0),
        role_json(REGULAR, "Regular", 5, false, 0),
        role_json(ADMIN, "Admin", 20, false, ADMINISTRATOR),
        role_json(BOOSTER, "Server Booster", 2, true, 0),
        role_json(HELPER, "Helper", 4, false, MANAGE_MESSAGES),
    ])
}

fn channel_json(guild: u64, id: u64, name: &str, kind: u8, position: i32, parent: Option<u64>) -> Value {
    json!({
        "id": id.to_string(),
        "guild_id": guild.to_string(),
        "type": kind,
        "name": name,
        "position": position,
        "parent_id": parent.map(|p| p.to_string()),
        "permission_overwrites": [],
        "nsfw": false,
    })
}

/// `GET /guilds/{id}/channels`.
fn channels_json(guild: u64) -> Value {
    json!([
        channel_json(guild, CATEGORY, "Chat", 4, 0, None),
        channel_json(guild, GENERAL, "general", 0, 1, Some(CATEGORY)),
        channel_json(guild, NEWS, "news", 5, 2, Some(CATEGORY)),
        channel_json(guild, LOUNGE, "Lounge", 2, 3, Some(CATEGORY)),
    ])
}

fn member_json(user: u64, roles: &[u64]) -> Value {
    json!({
        "user": user_json(user, &format!("user{user}")),
        "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
        "joined_at": "2024-01-01T00:00:00.000000+00:00",
        "deaf": false,
        "mute": false,
        "flags": 0,
        "pending": false,
    })
}

/// `GET /guilds/{id}` as the bot sees it.
fn guild_json(id: u64) -> Value {
    json!({
        "id": id.to_string(),
        "name": "Pinchora Reef",
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
    })
}

fn message_json(channel: u64, id: u64) -> Value {
    json!({
        "id": id.to_string(),
        "channel_id": channel.to_string(),
        "type": 0,
        "author": user_json(APPLICATION_ID, "pinchora"),
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

fn tokens_json(mock: &Mock, access: &str) -> Value {
    let expires_in = if mock.short_tokens.load(Ordering::SeqCst) {
        60
    } else {
        604_800
    };
    json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": expires_in,
        "refresh_token": format!("r-{access}"),
        "scope": "identify guilds",
    })
}

/// `POST /oauth2/token`: codes for tokens (checking the redirect and the PKCE verifier), and refreshes.
async fn token(State(mock): State<Mock>, Form(form): Form<HashMap<String, String>>) -> (StatusCode, String) {
    mock.token_forms.lock().unwrap().push(form.clone());
    let delay = mock.token_delay_ms.load(Ordering::SeqCst);
    if delay > 0 {
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    let refused = (StatusCode::BAD_REQUEST, json!({ "error": "invalid_grant" }).to_string());
    if form.get("client_secret").map(String::as_str) != Some("test-secret") {
        return (
            StatusCode::UNAUTHORIZED,
            json!({ "error": "invalid_client" }).to_string(),
        );
    }
    match form.get("grant_type").map(String::as_str) {
        Some("authorization_code") => {
            let Some((access, challenge)) = form.get("code").and_then(|c| mock.codes.lock().unwrap().remove(c)) else {
                return refused;
            };
            let verifier = form.get("code_verifier").cloned().unwrap_or_default();
            let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
            if expected != challenge || form.get("redirect_uri").map(String::as_str) != Some(&redirect_uri()) {
                return refused;
            }
            (StatusCode::OK, tokens_json(&mock, &access).to_string())
        }
        Some("refresh_token") => {
            let access = form
                .get("refresh_token")
                .and_then(|r| r.strip_prefix("r-"))
                .unwrap_or_default()
                .to_owned();
            if mock.refuse_refresh.load(Ordering::SeqCst) || !mock.users.lock().unwrap().contains_key(&access) {
                return refused;
            }
            if mock.rotate.load(Ordering::SeqCst) {
                let used = form.get("refresh_token").cloned().unwrap_or_default();
                if !mock.used_refresh.lock().unwrap().insert(used) {
                    return refused;
                }
                let renewed = format!("{access}-{}", unique());
                let mut users = mock.users.lock().unwrap();
                let account = users[&access].clone();
                users.insert(renewed.clone(), account);
                return (StatusCode::OK, tokens_json(&mock, &renewed).to_string());
            }
            (StatusCode::OK, tokens_json(&mock, &access).to_string())
        }
        _ => refused,
    }
}

async fn revoke(State(mock): State<Mock>, Form(form): Form<HashMap<String, String>>) -> StatusCode {
    mock.revoked
        .lock()
        .unwrap()
        .push(form.get("token").cloned().unwrap_or_default());
    StatusCode::OK
}

/// Everything else: user-token routes by bearer token, bot-token routes, message posts.
async fn discord(
    State(mock): State<Mock>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, String) {
    let path = uri.path().to_owned();
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned);
    let unauthorized = (
        StatusCode::UNAUTHORIZED,
        json!({ "message": "401: Unauthorized", "code": 0 }).to_string(),
    );
    match (&method, parts.as_slice()) {
        (&Method::PATCH, [.., "guilds", _, "members", "@me"]) => {
            let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            mock.profile_patches.lock().unwrap().push(value.clone());
            if mock.refuse_profile.load(Ordering::SeqCst) {
                return (
                    StatusCode::UNAUTHORIZED,
                    json!({"message":"Unauthorized","code":0}).to_string(),
                );
            }
            if (mock.refuse_banner.load(Ordering::SeqCst) && value.get("banner").is_some())
                || (mock.refuse_avatar.load(Ordering::SeqCst) && value.get("avatar").is_some())
            {
                return (
                    StatusCode::FORBIDDEN,
                    json!({"message":"Missing Access","code":50001}).to_string(),
                );
            }
            (StatusCode::OK, "{}".to_owned())
        }
        (&Method::GET, [.., "users", "@me"]) => {
            let users = mock.users.lock().unwrap();
            match bearer.and_then(|t| users.get(&t).cloned()) {
                Some((user, _)) => (StatusCode::OK, user.to_string()),
                None => unauthorized,
            }
        }
        (&Method::GET, [.., "users", "@me", "guilds"]) => {
            let users = mock.users.lock().unwrap();
            match bearer.and_then(|t| users.get(&t).cloned()) {
                Some((_, guilds)) => (StatusCode::OK, Value::Array(guilds).to_string()),
                None => unauthorized,
            }
        }
        (&Method::GET, [.., "guilds", guild, "channels"]) => {
            mock.channel_lookups.fetch_add(1, Ordering::SeqCst);
            let id = guild.parse::<u64>().unwrap_or(0);
            let mut channels = channels_json(id);
            if mock.stats_public_channels.load(Ordering::SeqCst) {
                channels[1]["permission_overwrites"] = json!([{
                    "id": id.to_string(), "type": 0, "allow": "0", "deny": (1u64 << 10).to_string()
                }]);
            }
            (StatusCode::OK, channels.to_string())
        }
        (&Method::GET, [.., "guilds", guild, "roles"]) => {
            let id = guild.parse::<u64>().unwrap_or(0);
            let mut roles = roles_json(id, mock.bot_position.load(Ordering::SeqCst));
            if mock.stats_public_channels.load(Ordering::SeqCst) {
                roles[0]["permissions"] = json!((SEND_MESSAGES | (1u64 << 10)).to_string());
            }
            (StatusCode::OK, roles.to_string())
        }
        (&Method::GET, [.., "guilds", _, "members", user]) => {
            let user = user.parse::<u64>().unwrap_or(0);
            let roles = if user == APPLICATION_ID {
                vec![BOT_ROLE]
            } else {
                mock.member_roles
                    .lock()
                    .unwrap()
                    .get(&user)
                    .cloned()
                    .unwrap_or_default()
            };
            (StatusCode::OK, member_json(user, &roles).to_string())
        }
        (&Method::PUT | &Method::DELETE, [.., "guilds", _, "members", user, "roles", role]) => {
            let (user, role) = (user.parse::<u64>().unwrap_or(0), role.parse::<u64>().unwrap_or(0));
            mock.role_calls.lock().unwrap().push(format!("{method} {user} {role}"));
            let mut members = mock.member_roles.lock().unwrap();
            let held = members.entry(user).or_default();
            held.retain(|r| *r != role);
            if method == Method::PUT {
                held.push(role);
            }
            (StatusCode::NO_CONTENT, String::new())
        }
        (&Method::GET, [.., "guilds", guild]) => {
            mock.guild_lookups.fetch_add(1, Ordering::SeqCst);
            let delay = mock.guild_delay_ms.load(Ordering::SeqCst);
            if delay > 0 {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            let id = guild.parse::<u64>().unwrap_or(0);
            if mock.bot_guilds.lock().unwrap().contains(&id) {
                let mut guild = guild_json(id);
                if let Some(owner) = mock.guild_owners.lock().unwrap().get(&id) {
                    guild["owner_id"] = json!(owner.to_string());
                }
                (StatusCode::OK, guild.to_string())
            } else {
                let gone = json!({ "message": "Unknown Guild", "code": 10004 });
                (StatusCode::NOT_FOUND, gone.to_string())
            }
        }
        (&Method::POST, [.., "channels", channel, "messages"]) => {
            let channel = channel.parse::<u64>().unwrap_or(1);
            let text = String::from_utf8_lossy(&body).into_owned();
            mock.posts.lock().unwrap().push((channel, text));
            (StatusCode::OK, message_json(channel, unique()).to_string())
        }
        _ => (
            StatusCode::NOT_FOUND,
            json!({ "message": "404: Not Found", "code": 0 }).to_string(),
        ),
    }
}

fn redirect_uri() -> String {
    format!("{PUBLIC_URL}/auth/callback")
}

// /Harness

/// One dashboard on the test databases, its Discord mocked.
struct Harness {
    state: WebState,
    router: Router,
    mock: Mock,
    mock_addr: SocketAddr,
    database: String,
    redis: String,
}

impl Harness {
    async fn new(name: &str) -> Option<Self> {
        Self::with(name, |_| {}).await
    }

    /// A dashboard whose configuration `configure` adjusts first.
    async fn with(name: &str, configure: impl FnOnce(&mut WebConfig)) -> Option<Self> {
        let (Ok(database), Ok(redis)) = (
            std::env::var("PINCHORA_TEST_DATABASE_URL"),
            std::env::var("PINCHORA_TEST_REDIS_URL"),
        ) else {
            eprintln!("{name}: skipped, set PINCHORA_TEST_DATABASE_URL and PINCHORA_TEST_REDIS_URL");
            return None;
        };
        let mock = Mock::default();
        mock.bot_position.store(BOT_ROLE_POSITION, Ordering::SeqCst);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind the mock");
        let mock_addr = listener.local_addr().expect("mock address");
        let routes = Router::new()
            .route("/api/v10/oauth2/token", post(token))
            .route("/api/v10/oauth2/token/revoke", post(revoke))
            .fallback(discord)
            .with_state(mock.clone());
        tokio::spawn(axum::serve(listener, routes).into_future());

        let app = app(&database, &redis, mock_addr).await;
        let mut web = WebConfig {
            public_url: PUBLIC_URL.into(),
            client_secret: Some(String::from("test-secret").into()),
            discord_api_base: format!("http://{mock_addr}/api/v10"),
            ..WebConfig::default()
        };
        configure(&mut web);
        let settings = WebSettings::resolve(&web, APPLICATION_ID).expect("settings");
        let state = WebState::new(app, settings);
        Some(Self {
            router: pinchora_web::router(state.clone()),
            state,
            mock,
            mock_addr,
            database,
            redis,
        })
    }

    fn browser(&self) -> Browser {
        Browser {
            router: self.router.clone(),
            cookies: HashMap::new(),
            csrf: None,
            // Keep the whole per-run ID: modulo pairs reused login rate-limit buckets in large suites.
            address: SocketAddr::new(Ipv6Addr::from((0xfd_u128 << 120) | u128::from(unique())).into(), 4000),
        }
    }

    /// Puts the bot into a server.
    fn add_bot(&self, guild: u64) {
        self.mock.bot_guilds.lock().unwrap().insert(guild);
    }

    /// A user Discord knows, with their server list; returns their access token.
    fn add_user(&self, user: u64, guilds: Vec<Value>) -> String {
        let token = format!("tok-{user}");
        let json = user_json(user, &format!("user{user}"));
        self.mock.users.lock().unwrap().insert(token.clone(), (json, guilds));
        token
    }

    /// A server with the bot, and a browser logged in as its owner.
    async fn managed_server(&self) -> (u64, u64, Browser) {
        let (guild, owner) = (unique(), unique());
        self.add_bot(guild);
        let mut browser = self.browser();
        browser
            .login(self, owner, vec![guild_entry(guild, "Pinchora Reef", true, 0)])
            .await;
        (guild, owner, browser)
    }

    /// Gives a member server XP, as chatting would.
    async fn seed_xp(&self, guild: u64, user: u64, xp: i64) {
        let grant = XpGrant {
            server_xp: xp,
            global_xp: 0,
            points: 0,
        };
        let week = pinchora_core::leveling::week_start(chrono::Utc::now());
        self.state
            .app
            .db
            .add_xp(guild, user, grant, week)
            .await
            .expect("seed XP");
    }

    /// Another pod of the bot on the same databases, as the gateway would run it.
    async fn bot_pod(&self) -> Arc<App> {
        app(&self.database, &self.redis, self.mock_addr).await
    }
}

/// The real [`App`] on the test databases, its REST client pointed at the mock.
async fn app(database: &str, redis: &str, mock: SocketAddr) -> Arc<App> {
    let db = Db::connect_waiting(database, 5, std::time::Duration::from_secs(60))
        .await
        .expect("connect to the database");
    db.migrate().await.expect("migrate");
    let cache = Cache::connect(redis).await.expect("connect to redis");
    let rest = Arc::new(
        Client::builder()
            .token("Bot web-test".into())
            .proxy(mock.to_string(), true)
            .ratelimiter(None)
            .build(),
    );
    Arc::new(App::new(
        rest.clone(),
        rest,
        Id::new(APPLICATION_ID),
        db,
        cache,
        "web-test".into(),
    ))
}

/// A response, its body read.
struct Response {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl Response {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", self.text()))
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn header(&self, name: &str) -> &str {
        self.headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or_default()
    }

    /// The `error.code` of an API error.
    fn code(&self) -> String {
        self.json()["error"]["code"].as_str().unwrap_or_default().to_owned()
    }
}

/// Cookies and the CSRF token, like a browser tab on the dashboard.
#[derive(Clone)]
struct Browser {
    router: Router,
    cookies: HashMap<String, String>,
    csrf: Option<String>,
    /// The client address, so each browser has its own login rate limit.
    address: SocketAddr,
}

impl Browser {
    async fn send(&mut self, request: Request<Body>) -> Response {
        let mut request = request;
        if !self.cookies.is_empty() {
            let jar = self
                .cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ");
            request
                .headers_mut()
                .insert(header::COOKIE, jar.parse().expect("cookie header"));
        }
        request.extensions_mut().insert(ConnectInfo(self.address));
        let response = self.router.clone().oneshot(request).await.expect("router");
        for set in response.headers().get_all(header::SET_COOKIE) {
            let set = set.to_str().expect("cookie");
            let (pair, attributes) = set.split_once(';').unwrap_or((set, ""));
            let (name, value) = pair.split_once('=').expect("name=value");
            if attributes.contains("Max-Age=0") {
                self.cookies.remove(name);
            } else {
                self.cookies.insert(name.to_owned(), value.to_owned());
            }
        }
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        Response { status, headers, body }
    }

    async fn get(&mut self, path: &str) -> Response {
        self.send(Request::get(path).body(Body::empty()).unwrap()).await
    }

    /// A write as the dashboard's own script sends it: JSON, same origin, with the CSRF token.
    async fn write(&mut self, method: Method, path: &str, body: Value) -> Response {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, PUBLIC_URL)
            .header("sec-fetch-site", "same-origin");
        if let Some(csrf) = &self.csrf {
            request = request.header("x-csrf-token", csrf);
        }
        self.send(request.body(Body::from(body.to_string())).unwrap()).await
    }

    /// Logs in through the whole OAuth dance: our redirect, Discord's consent (mocked), the callback.
    /// Returns `GET /api/me`.
    async fn login(&mut self, harness: &Harness, user: u64, guilds: Vec<Value>) -> Value {
        let token = harness.add_user(user, guilds);
        let start = self.get("/auth/login?next=/servers").await;
        assert_eq!(start.status, StatusCode::SEE_OTHER, "{}", start.text());
        let location = Url::parse(start.header("location")).expect("authorize URL");
        assert_eq!(location.path(), "/oauth2/authorize");
        let query: HashMap<String, String> = location.query_pairs().into_owned().collect();
        assert_eq!(query["client_id"], APPLICATION_ID.to_string());
        assert_eq!(query["scope"], "identify guilds");
        assert_eq!(query["redirect_uri"], redirect_uri());
        assert_eq!(query["code_challenge_method"], "S256");
        let code = format!("code-{}", unique());
        harness
            .mock
            .codes
            .lock()
            .unwrap()
            .insert(code.clone(), (token, query["code_challenge"].clone()));

        let back = self
            .get(&format!("/auth/callback?code={code}&state={}", query["state"]))
            .await;
        assert_eq!(back.status, StatusCode::SEE_OTHER, "{}", back.text());
        assert_eq!(back.header("location"), "/servers");
        assert!(self.cookies.contains_key("pinchora_sid"), "the session cookie is set");
        assert!(
            !self.cookies.contains_key("pinchora_oauth"),
            "the login cookie is cleared"
        );
        let me = self.get("/api/me").await;
        assert_eq!(me.status, StatusCode::OK, "{}", me.text());
        let me = me.json();
        self.csrf = me["csrf"].as_str().map(str::to_owned);
        me
    }
}

/// Saves a PNG the dashboard served to `target/web-sim/<test>/<step>.png`, for a person (or an agent)
/// to look at.
fn save_png(test: &str, step: &str, bytes: &[u8]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/web-sim/{test}"));
    std::fs::create_dir_all(&dir).expect("png folder");
    let path = dir.join(format!("{step}.png"));
    std::fs::write(&path, bytes).expect("write png");
    path
}

/// The fields a 422 names, in order.
fn fields_of(response: &Response) -> Vec<String> {
    response.json()["error"]["errors"]
        .as_array()
        .map(|errors| {
            errors
                .iter()
                .filter_map(|e| e["field"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Waits for `what` to be announced on Redis.
async fn announced(heard: &mut (impl futures_util::Stream<Item = Invalidation> + Unpin), what: Invalidation) -> bool {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(got) = heard.next().await {
            if got == what {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false)
}

/// A chat message from `user` in `guild`, as the gateway delivers it.
fn chat(guild: u64, user: u64) -> MessageCreate {
    chat_in(guild, user, 2)
}

/// [`chat`] in a given channel.
fn chat_in(guild: u64, user: u64, channel: u64) -> MessageCreate {
    serde_json::from_value(json!({
        "id": unique().to_string(),
        "channel_id": channel.to_string(),
        "guild_id": guild.to_string(),
        "type": 0,
        "author": user_json(user, "chatter"),
        "member": {
            "roles": [],
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
    }))
    .expect("message JSON")
}

async fn server_xp(app: &App, guild: u64, user: u64) -> i64 {
    app.db.member_rank(guild, user).await.expect("rank").map_or(0, |r| r.xp)
}

// /Tests

#[tokio::test]
async fn oauth_login_lists_only_the_servers_the_user_manages() {
    let Some(h) = Harness::new("oauth_login").await else {
        return;
    };
    let (owned, managed, admin, member, absent) = (unique(), unique(), unique(), unique(), unique());
    for g in [owned, managed, admin, member] {
        h.add_bot(g);
    }
    let user = unique();
    let mut browser = h.browser();
    let me = browser
        .login(
            &h,
            user,
            vec![
                guild_entry(owned, "Owned Reef", true, 0),
                guild_entry(managed, "Managed Bay", false, MANAGE_GUILD | SEND_MESSAGES),
                guild_entry(admin, "admin cove", false, ADMINISTRATOR),
                guild_entry(member, "Just Chatting", false, SEND_MESSAGES),
                guild_entry(absent, "Botless Lagoon", false, MANAGE_GUILD),
            ],
        )
        .await;
    assert_eq!(me["user"]["id"], user.to_string(), "ids are strings");
    assert_eq!(me["user"]["global_name"], format!("user{user} G"));
    assert_eq!(me["csrf"].as_str().map(str::len), Some(43));

    let forms = h.mock.token_forms.lock().unwrap().clone();
    let exchange = forms.last().expect("the code was exchanged");
    assert_eq!(exchange["grant_type"], "authorization_code");
    assert_eq!(exchange["client_id"], APPLICATION_ID.to_string());

    let list = browser.get("/api/me/guilds").await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.text());
    assert_eq!(list.header("cache-control"), "no-store");
    let items = list.json()["items"].as_array().cloned().expect("items");
    let names: Vec<&str> = items.iter().map(|g| g["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["admin cove", "Managed Bay", "Owned Reef", "Botless Lagoon"],
        "managers only, the bot's servers first, then by name"
    );
    let botless = &items[3];
    assert_eq!(botless["bot_present"], false);
    let invite = botless["invite_url"]
        .as_str()
        .expect("an invite for the botless server");
    assert!(invite.contains(&format!("guild_id={absent}")) && invite.contains("scope=bot+applications.commands"));
    assert_eq!(items[2]["owner"], true);
    assert_eq!(items[2]["invite_url"], Value::Null);
    assert_eq!(
        items[2]["icon_url"],
        format!("https://cdn.discordapp.com/icons/{owned}/a1b2c3.png?size=128")
    );

    // A refresh asks Discord again, then waits ten seconds before it may again.
    let first = browser.write(Method::POST, "/api/me/guilds/refresh", json!({})).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    let again = browser.write(Method::POST, "/api/me/guilds/refresh", json!({})).await;
    assert_eq!(again.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(!again.header("retry-after").is_empty());
}

#[tokio::test]
async fn a_callback_must_match_the_login_it_came_from() {
    let Some(h) = Harness::new("callback_state").await else {
        return;
    };
    let mut browser = h.browser();
    let start = browser.get("/auth/login").await;
    let location = Url::parse(start.header("location")).unwrap();
    let state = location
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .unwrap();

    // Another state than the cookie's: someone else's login link.
    let other = "ab".repeat(32);
    let forged = browser.get(&format!("/auth/callback?code=x&state={other}")).await;
    assert_eq!(forged.status, StatusCode::BAD_REQUEST);
    assert_eq!(forged.code(), "malformed");

    // No cookie at all: the login was not started in this browser.
    let mut stranger = h.browser();
    let stray = stranger.get(&format!("/auth/callback?code=x&state={state}")).await;
    assert_eq!(stray.status, StatusCode::BAD_REQUEST);

    // Cancelled on Discord: back to the login page, the state used up.
    let denied = browser
        .get(&format!("/auth/callback?error=access_denied&state={state}"))
        .await;
    assert_eq!(denied.status, StatusCode::SEE_OTHER);
    assert_eq!(denied.header("location"), "/login?error=denied");
    browser.cookies.insert("pinchora_oauth".into(), state.clone());
    let replay = browser.get(&format!("/auth/callback?code=x&state={state}")).await;
    assert_eq!(replay.status, StatusCode::BAD_REQUEST, "a state works once");

    // A code Discord does not know.
    let start = browser.get("/auth/login").await;
    let state = Url::parse(start.header("location"))
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .unwrap();
    let bad_code = browser.get(&format!("/auth/callback?code=nope&state={state}")).await;
    assert_eq!(bad_code.status, StatusCode::SEE_OTHER);
    assert_eq!(bad_code.header("location"), "/login?error=denied");
    assert!(!browser.cookies.contains_key("pinchora_sid"));

    // `next` never leaves the site.
    let outside = browser.get("/auth/login?next=//evil.example/x").await;
    let state = Url::parse(outside.header("location"))
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .unwrap();
    let pending = h
        .state
        .app
        .cache
        .get(&format!("web:oauth:{state}"))
        .await
        .unwrap()
        .expect("the login is pending");
    assert!(pending.contains("\"next\":\"/servers\""), "{pending}");
}

#[tokio::test]
async fn writes_need_the_csrf_token_and_our_origin() {
    let Some(h) = Harness::new("csrf").await else { return };
    let guild = unique();
    h.add_bot(guild);
    let mut browser = h.browser();
    browser
        .login(&h, unique(), vec![guild_entry(guild, "Reef", true, 0)])
        .await;
    let path = format!("/api/guilds/{guild}/settings");
    let body = json!({ "leveling_enabled": false });

    let token = browser.csrf.take();
    let missing = browser.write(Method::PATCH, &path, body.clone()).await;
    assert_eq!(
        (missing.status, missing.code().as_str()),
        (StatusCode::FORBIDDEN, "csrf")
    );

    browser.csrf = Some("x".repeat(43));
    let wrong = browser.write(Method::PATCH, &path, body.clone()).await;
    assert_eq!((wrong.status, wrong.code().as_str()), (StatusCode::FORBIDDEN, "csrf"));
    browser.csrf = token.clone();

    let cross_site = browser
        .send(
            Request::patch(&path)
                .header(header::CONTENT_TYPE, "application/json")
                .header("sec-fetch-site", "cross-site")
                .header("x-csrf-token", token.clone().unwrap())
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(cross_site.code(), "csrf", "another site's page cannot write");

    let other_origin = browser
        .send(
            Request::patch(&path)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::ORIGIN, "http://evil.example")
                .header("x-csrf-token", token.clone().unwrap())
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await;
    assert_eq!(other_origin.code(), "csrf");

    let form = browser
        .send(
            Request::patch(&path)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header("x-csrf-token", token.clone().unwrap())
                .body(Body::from("leveling_enabled=false"))
                .unwrap(),
        )
        .await;
    assert_eq!(
        (form.status, form.code().as_str()),
        (StatusCode::BAD_REQUEST, "malformed")
    );
    let settings = h.state.app.db.guild_settings(guild).await.unwrap();
    assert!(settings.leveling_enabled, "nothing above changed anything");

    let ok = browser.write(Method::PATCH, &path, body).await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.text());
    assert_eq!(ok.json()["leveling_enabled"], false);
}

#[tokio::test]
async fn only_managers_with_the_bot_open_a_server() {
    let Some(h) = Harness::new("managed_guild").await else {
        return;
    };
    let (with_bot, without_bot, chatting, elsewhere) = (unique(), unique(), unique(), unique());
    h.add_bot(with_bot);
    h.add_bot(chatting);
    h.add_bot(elsewhere);

    let mut anonymous = h.browser();
    let no_login = anonymous.get(&format!("/api/guilds/{with_bot}")).await;
    assert_eq!(
        (no_login.status, no_login.code().as_str()),
        (StatusCode::UNAUTHORIZED, "unauthorized")
    );

    let mut browser = h.browser();
    browser
        .login(
            &h,
            unique(),
            vec![
                guild_entry(with_bot, "Reef", false, MANAGE_GUILD),
                guild_entry(without_bot, "Lagoon", true, 0),
                guild_entry(chatting, "Cafe", false, SEND_MESSAGES),
            ],
        )
        .await;

    let open = browser.get(&format!("/api/guilds/{with_bot}")).await;
    assert_eq!(open.status, StatusCode::OK, "{}", open.text());
    let overview = open.json();
    assert_eq!(overview["guild"]["id"], with_bot.to_string());
    assert_eq!(overview["guild"]["name"], "Reef");
    assert_eq!(overview["settings"]["leveling_enabled"], true);
    assert_eq!(overview["audit"], json!([]));

    let botless = browser.get(&format!("/api/guilds/{without_bot}")).await;
    assert_eq!(
        (botless.status, botless.code().as_str()),
        (StatusCode::FORBIDDEN, "bot_missing")
    );
    let invite = botless.json()["error"]["invite_url"].as_str().unwrap().to_owned();
    assert!(invite.contains(&format!("guild_id={without_bot}")));

    for guild in [chatting, elsewhere] {
        let refused = browser.get(&format!("/api/guilds/{guild}")).await;
        assert_eq!(
            (refused.status, refused.code().as_str()),
            (StatusCode::FORBIDDEN, "not_manager")
        );
        let write = browser
            .write(
                Method::PATCH,
                &format!("/api/guilds/{guild}/settings"),
                json!({ "leveling_enabled": false }),
            )
            .await;
        assert_eq!(write.code(), "not_manager");
    }
    assert!(h.state.app.db.guild_settings(chatting).await.unwrap().leveling_enabled);

    let bad = browser.get("/api/guilds/not-a-number").await;
    assert_eq!(
        (bad.status, bad.code().as_str()),
        (StatusCode::BAD_REQUEST, "malformed")
    );
    let unknown_field = browser
        .write(
            Method::PATCH,
            &format!("/api/guilds/{with_bot}/settings"),
            json!({ "leveling_enabled": false, "owner": "me" }),
        )
        .await;
    assert_eq!(unknown_field.status, StatusCode::BAD_REQUEST);
    assert!(unknown_field.text().contains("owner"), "{}", unknown_field.text());
    let wrong_type = browser
        .write(
            Method::PATCH,
            &format!("/api/guilds/{with_bot}/settings"),
            json!({ "leveling_enabled": "off" }),
        )
        .await;
    assert_eq!(wrong_type.status, StatusCode::BAD_REQUEST);
}

/// The P1 slice end to end: a manager turns leveling off on the web, the audit log records it, the
/// change is announced on Redis, and a bot pod that had the old settings cached gives the very next
/// chat message no XP, well inside the cache's minute.
#[tokio::test]
async fn turning_leveling_off_on_the_web_reaches_the_bot_at_once() {
    let Some(h) = Harness::new("leveling_toggle").await else {
        return;
    };
    let guild = unique();
    h.add_bot(guild);
    let bot = h.bot_pod().await;
    listen_for_invalidations(&bot).await;
    let mut heard = Box::pin(h.state.app.cache.invalidations().await.expect("subscribe"));

    // The bot pays XP and keeps the server's settings (leveling on) cached.
    let first = unique();
    on_message(bot.clone(), chat(guild, first)).await.expect("chat");
    assert!(server_xp(&bot, guild, first).await > 0);
    assert!(bot.guild_settings(guild).await.unwrap().leveling_enabled);

    let manager = unique();
    let mut browser = h.browser();
    browser
        .login(&h, manager, vec![guild_entry(guild, "Reef", false, MANAGE_GUILD)])
        .await;
    let started = Instant::now();
    let off = browser
        .write(
            Method::PATCH,
            &format!("/api/guilds/{guild}/settings"),
            json!({ "leveling_enabled": false }),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    assert_eq!(off.json()["leveling_enabled"], false);
    assert!(!h.state.app.db.guild_settings(guild).await.unwrap().leveling_enabled);

    // The announcement went out on Redis.
    let announced = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(what) = heard.next().await {
            if what == Invalidation::Guild(guild) {
                return true;
            }
        }
        false
    })
    .await;
    assert_eq!(announced, Ok(true), "guild:{guild} was published");

    // The bot pod dropped its copy: no XP for the next message, and no 60 s wait.
    let second = unique();
    let mut earned = 1;
    while started.elapsed() < Duration::from_secs(5) {
        let user = unique();
        on_message(bot.clone(), chat(guild, user)).await.expect("chat");
        earned = server_xp(&bot, guild, user).await;
        if earned == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(earned, 0, "chat earns no XP once leveling is off");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the bot saw the change in {:?}",
        started.elapsed()
    );
    on_message(bot.clone(), chat(guild, second)).await.expect("chat");
    assert_eq!(server_xp(&bot, guild, second).await, 0);

    // The overview shows who did it.
    let overview = browser.get(&format!("/api/guilds/{guild}")).await.json();
    let audit = overview["audit"].as_array().expect("audit rows");
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0]["actor_id"], manager.to_string());
    assert_eq!(audit[0]["actor_name"], format!("user{manager} G"));
    assert_eq!(audit[0]["source"], "web");
    assert_eq!(audit[0]["action"], "settings.patch");
    assert_eq!(audit[0]["before"], json!({ "leveling_enabled": true }));
    assert_eq!(audit[0]["after"], json!({ "leveling_enabled": false }));

    // Saving the same value again changes nothing and records nothing.
    let same = browser
        .write(
            Method::PATCH,
            &format!("/api/guilds/{guild}/settings"),
            json!({ "leveling_enabled": false }),
        )
        .await;
    assert_eq!(same.status, StatusCode::OK);
    assert_eq!(h.state.app.db.audit_page(guild, None, 10).await.unwrap().len(), 1);

    // And back on: XP flows again straight away.
    let on = browser
        .write(
            Method::PATCH,
            &format!("/api/guilds/{guild}/settings"),
            json!({ "leveling_enabled": true }),
        )
        .await;
    assert_eq!(on.json()["leveling_enabled"], true);
    let mut earned = 0;
    let back_on = Instant::now();
    while back_on.elapsed() < Duration::from_secs(5) {
        let user = unique();
        on_message(bot.clone(), chat(guild, user)).await.expect("chat");
        earned = server_xp(&bot, guild, user).await;
        if earned > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(earned > 0, "chat earns XP again");
    let rows = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].after.as_deref(), Some(r#"{"leveling_enabled": true}"#));
}

#[tokio::test]
async fn logout_ends_the_session_and_revokes_the_token() {
    let Some(h) = Harness::new("logout").await else { return };
    let user = unique();
    let mut browser = h.browser();
    browser.login(&h, user, vec![]).await;

    let unsigned = browser
        .send(Request::post("/auth/logout").body(Body::empty()).unwrap())
        .await;
    assert_eq!(unsigned.code(), "csrf", "logging someone out needs the token too");

    let out = browser.write(Method::POST, "/auth/logout", json!({})).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT, "{}", out.text());
    assert!(!browser.cookies.contains_key("pinchora_sid"));
    let me = browser.get("/api/me").await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);

    let revoked = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if h.mock.revoked.lock().unwrap().contains(&format!("tok-{user}")) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(revoked.is_ok(), "the Discord token was revoked");
}

#[tokio::test]
async fn an_expiring_token_is_refreshed_and_a_refused_one_ends_the_session() {
    let Some(h) = Harness::new("token_refresh").await else {
        return;
    };
    h.mock.short_tokens.store(true, Ordering::SeqCst);
    let guild = unique();
    h.add_bot(guild);
    let mut browser = h.browser();
    browser
        .login(&h, unique(), vec![guild_entry(guild, "Reef", true, 0)])
        .await;

    let list = browser.get("/api/me/guilds").await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.text());
    let grants: Vec<String> = h
        .mock
        .token_forms
        .lock()
        .unwrap()
        .iter()
        .map(|f| f["grant_type"].clone())
        .collect();
    assert_eq!(grants.last().map(String::as_str), Some("refresh_token"));

    h.mock.refuse_refresh.store(true, Ordering::SeqCst);
    let refused = browser.write(Method::POST, "/api/me/guilds/refresh", json!({})).await;
    assert_eq!(
        (refused.status, refused.code().as_str()),
        (StatusCode::UNAUTHORIZED, "session_expired")
    );
    assert_eq!(browser.get("/api/me").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn without_a_client_secret_the_setup_page_is_served() {
    let Some(h) = Harness::with("setup_page", |web| web.client_secret = None).await else {
        return;
    };
    let mut browser = h.browser();
    let page = browser.get("/servers").await;
    assert_eq!(page.status, StatusCode::OK);
    let html = page.text();
    assert!(html.contains("PINCHORA_WEB__CLIENT_SECRET"), "{html}");
    assert!(html.contains(&redirect_uri()), "the redirect to register is shown");
    assert_eq!(
        browser.get("/g/1/overview").await.text(),
        html,
        "every app page shows it"
    );
    assert_ne!(
        browser.get("/").await.text(),
        html,
        "the public front page needs no login, so it is not the setup page"
    );
    assert_eq!(browser.get("/setup.css").await.status, StatusCode::OK);

    let login = browser.get("/auth/login").await;
    assert_eq!(
        (login.status, login.code().as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, "not_configured")
    );
}

#[tokio::test]
async fn the_app_fonts_and_api_answer_with_the_right_headers() {
    let Some(h) = Harness::new("static_files").await else {
        return;
    };
    let mut browser = h.browser();

    let page = browser.get("/g/123/overview").await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(
        page.text().to_lowercase().starts_with("<!doctype html>"),
        "index.html for app routes"
    );
    assert_eq!(page.header("cache-control"), "no-cache");
    let csp = page.header("content-security-policy");
    assert!(
        csp.contains("script-src 'self'") && csp.contains("frame-ancestors 'none'"),
        "{csp}"
    );
    assert_eq!(page.header("x-content-type-options"), "nosniff");

    let font = browser.get("/assets/fonts/Silkscreen-Regular.ttf").await;
    assert_eq!(font.status, StatusCode::OK);
    assert_eq!(font.header("content-type"), "font/ttf");
    assert_eq!(font.body.as_ref(), pinchora_render::FONT_FILES[0].1);
    let etag = font.header("etag").to_owned();
    let cached = browser
        .send(
            Request::get("/assets/fonts/Silkscreen-Regular.ttf")
                .header(header::IF_NONE_MATCH, &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(cached.status, StatusCode::NOT_MODIFIED);

    assert_eq!(
        browser.get("/assets/fonts/nope.ttf").await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        browser.get("/assets/missing-abc.js").await.status,
        StatusCode::NOT_FOUND
    );
    let api = browser.get("/api/nothing/here").await;
    assert_eq!((api.status, api.code().as_str()), (StatusCode::NOT_FOUND, "not_found"));
    let me = browser.get("/api/me").await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    assert_eq!(me.header("cache-control"), "no-store");
}

#[tokio::test]
async fn dev_login_exists_only_when_switched_on_at_a_loopback_address() {
    let Some(h) = Harness::new("dev_login_off").await else {
        return;
    };
    let body = json!({ "user_id": "5", "username": "dev", "guilds": [] });
    let mut browser = h.browser();
    let off = browser.write(Method::POST, "/auth/dev-login", body.clone()).await;
    assert_eq!(off.status, StatusCode::NOT_FOUND);

    let Some(h) = Harness::with("dev_login_on", |web| {
        web.dev_login = true;
        web.bind = ([127, 0, 0, 1], 8090).into();
    })
    .await
    else {
        return;
    };
    let guild = unique();
    h.add_bot(guild);
    let mut browser = h.browser();
    let on = browser
        .write(
            Method::POST,
            "/auth/dev-login",
            json!({
                "user_id": "5",
                "username": "dev",
                "guilds": [{ "id": guild.to_string(), "name": "Dev Reef", "owner": true, "permissions": "0" }],
            }),
        )
        .await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.text());
    browser.csrf = on.json()["csrf"].as_str().map(str::to_owned);
    let list = browser.get("/api/me/guilds").await.json();
    assert_eq!(list["items"][0]["name"], "Dev Reef");
    assert_eq!(list["items"][0]["bot_present"], true);
    let toggled = browser
        .write(
            Method::PATCH,
            &format!("/api/guilds/{guild}/settings"),
            json!({ "leveling_enabled": false }),
        )
        .await;
    assert_eq!(toggled.status, StatusCode::OK, "{}", toggled.text());

    let Some(h) = Harness::with("dev_login_public", |web| web.dev_login = true).await else {
        return;
    };
    let public = h.browser().write(Method::POST, "/auth/dev-login", body).await;
    assert_eq!(public.status, StatusCode::NOT_FOUND, "never on 0.0.0.0");
}

#[tokio::test]
async fn login_attempts_are_rate_limited_per_address() {
    let Some(h) = Harness::new("auth_rate_limit").await else {
        return;
    };
    let mut browser = h.browser();
    let mut statuses = Vec::new();
    // 21 tries: whichever way the minute boundary falls, one window gets more than ten.
    for _ in 0..21 {
        statuses.push(browser.get("/auth/login").await.status);
    }
    assert_eq!(statuses[0], StatusCode::SEE_OTHER);
    assert!(statuses.contains(&StatusCode::TOO_MANY_REQUESTS), "{statuses:?}");
    let mut other = h.browser();
    assert_eq!(
        other.get("/auth/login").await.status,
        StatusCode::SEE_OTHER,
        "per address"
    );
}

// /Settings pages (P2)

#[tokio::test]
async fn the_server_lists_mark_what_the_bot_can_hand_out() {
    let Some(h) = Harness::new("discord_lists").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let lists = browser.get(&format!("/api/guilds/{guild}/discord")).await;
    assert_eq!(lists.status, StatusCode::OK, "{}", lists.text());
    let lists = lists.json();
    let channels: Vec<(&str, u64)> = lists["channels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["name"].as_str().unwrap(), c["type"].as_u64().unwrap()))
        .collect();
    assert_eq!(channels, [("Chat", 4), ("general", 0), ("news", 5), ("Lounge", 2)]);
    assert_eq!(lists["channels"][1]["parent_id"], CATEGORY.to_string());
    let role = |name: &str| {
        lists["roles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("no role {name}"))
    };
    assert_eq!(lists["roles"][0]["name"], "Admin", "highest first");
    assert_eq!(role("Fan")["assignable"], true);
    assert_eq!(role("Fan")["why_not"], Value::Null);
    assert_eq!(
        role("Admin")["why_not"],
        "@Admin is above my highest role. Drag my role above it in Server Settings > Roles, then try again."
    );
    assert_eq!(
        role("Server Booster")["why_not"],
        "@Server Booster is managed by an integration or a bot, so it can't be handed out."
    );
    assert_eq!(
        role("everyone")["why_not"],
        "@everyone can't be handed out: every member already has it."
    );
    assert_eq!(role("everyone")["everyone"], true);
    assert_eq!(lists["bot"]["top_role_position"], BOT_ROLE_POSITION);
    assert_eq!(lists["bot"]["manage_roles"], true);

    // Cached for a minute; a refresh asks again, but not twice within ten seconds.
    let asked = h.mock.channel_lookups.load(Ordering::SeqCst);
    browser.get(&format!("/api/guilds/{guild}/discord")).await;
    assert_eq!(h.mock.channel_lookups.load(Ordering::SeqCst), asked);
    browser.get(&format!("/api/guilds/{guild}/discord?refresh=1")).await;
    browser.get(&format!("/api/guilds/{guild}/discord?refresh=1")).await;
    assert_eq!(h.mock.channel_lookups.load(Ordering::SeqCst), asked + 1);
    // A write naming a role the lists lack asks again no more often than the refresh does.
    let unknown = browser
        .write(
            Method::PUT,
            &format!("/api/guilds/{guild}/level-roles/1"),
            json!({ "level": 2 }),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(h.mock.channel_lookups.load(Ordering::SeqCst), asked + 1);
}

#[tokio::test]
async fn every_setting_saves_with_the_command_rules_and_reaches_the_bot() {
    let Some(h) = Harness::new("settings_patch").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let mut heard = Box::pin(h.state.app.cache.invalidations().await.expect("subscribe"));
    let path = format!("/api/guilds/{guild}/settings");

    let saved = browser
        .write(
            Method::PATCH,
            &path,
            json!({
                "leveling_enabled": false,
                "levelup_mode": "dm",
                "levelup_channel_id": NEWS.to_string(),
                "levelup_message": "GG {user}, level {level}!",
                "level_roles_stack": false,
                "voice_xp_enabled": true,
                "voice_xp_amount": 25,
                "welcome_enabled": true,
                "welcome_channel_id": GENERAL.to_string(),
                "welcome_message": "Hi {user}, you are #{count}",
                "welcome_theme": "forest",
                "welcome_title": "  HELLO ",
                "welcome_accent": "#06D6A0",
                "join_role_id": FAN.to_string(),
                "goodbye_channel_id": GENERAL.to_string(),
                "goodbye_message": "Bye {user}",
                "points_enabled": false,
                "points_name": " shells ",
                "points_emoji": "🐚",
                "points_min": 2,
                "points_max": 6,
                "log_channel_id": NEWS.to_string(),
                "log_members": false,
                "log_names": true,
                "log_bans": false,
                "log_voice": true,
            }),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let view = saved.json();
    assert_eq!(view["welcome_title"], "HELLO", "trimmed like the command trims");
    assert_eq!(view["welcome_accent"], "#06d6a0");
    assert_eq!(view["points_name"], "shells");
    assert_eq!(view["join_role_id"], FAN.to_string());
    let stored = h.state.app.db.guild_settings(guild).await.unwrap();
    assert_eq!(stored.levelup_mode, "dm");
    assert_eq!(stored.voice_xp_amount, 25);
    assert_eq!(stored.welcome_accent, Some(0x06d6a0));
    assert_eq!(stored.goodbye_channel_id, Some(GENERAL as i64));
    assert_eq!((stored.points_min, stored.points_max), (2, 6));
    assert!(!stored.log_members && stored.log_names && !stored.log_bans && stored.log_voice);
    assert!(announced(&mut heard, Invalidation::Guild(guild)).await);

    let rows = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].actor_id, owner as i64);
    let after: Value = serde_json::from_str(rows[0].after.as_deref().unwrap()).unwrap();
    let before: Value = serde_json::from_str(rows[0].before.as_deref().unwrap()).unwrap();
    assert_eq!(after["welcome_theme"], "forest");
    assert_eq!(before["welcome_theme"], Value::Null);
    assert_eq!(before["points_name"], "points");
    assert_eq!(
        after.as_object().unwrap().len(),
        24,
        "every field sent but log_names and log_voice, which were on already: {after}"
    );

    // `null` clears; what is not sent stays.
    let cleared = browser
        .write(
            Method::PATCH,
            &path,
            json!({ "goodbye_channel_id": null, "welcome_accent": null, "join_role_id": null }),
        )
        .await;
    assert_eq!(cleared.status, StatusCode::OK, "{}", cleared.text());
    let stored = h.state.app.db.guild_settings(guild).await.unwrap();
    assert_eq!(
        (stored.goodbye_channel_id, stored.welcome_accent, stored.join_role_id),
        (None, None, None)
    );
    assert_eq!(
        stored.goodbye_message.as_deref(),
        Some("Bye {user}"),
        "the text is kept"
    );
    assert_eq!(stored.welcome_channel_id, Some(GENERAL as i64));
}

#[tokio::test]
async fn settings_answer_with_the_field_and_the_command_sentence() {
    let Some(h) = Harness::new("settings_invalid").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/settings");
    let refused = |r: &Response| {
        let errors = r.json()["error"]["errors"].as_array().cloned().unwrap_or_default();
        errors
            .iter()
            .map(|e| {
                (
                    e["field"].as_str().unwrap().to_owned(),
                    e["message"].as_str().unwrap().to_owned(),
                )
            })
            .collect::<Vec<_>>()
    };

    let bad = browser
        .write(
            Method::PATCH,
            &path,
            json!({
                "welcome_accent": "coral",
                "welcome_title": "A HEADLINE FAR TOO LONG",
                "points_min": 9,
                "points_emoji": "coins",
                "voice_xp_amount": 500,
                "welcome_message": "x".repeat(501),
            }),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", bad.text());
    assert_eq!(bad.code(), "invalid");
    assert_eq!(bad.json()["error"]["field"], "welcome_accent");
    let errors = refused(&bad);
    assert_eq!(
        errors[0].1, "Colours look like `#ff6b4a`.",
        "the /setup welcome card sentence"
    );
    let fields: Vec<&str> = errors.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(
        fields,
        ["welcome_accent"],
        "unreadable fields first, nothing else checked"
    );

    let bad = browser
        .write(
            Method::PATCH,
            &path,
            json!({
                "welcome_title": "A HEADLINE FAR TOO LONG",
                "points_min": 9,
                "points_emoji": "coins",
                "voice_xp_amount": 500,
                "welcome_message": "x".repeat(501),
            }),
        )
        .await;
    let errors = refused(&bad);
    let fields: Vec<&str> = errors.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(
        fields,
        [
            "welcome_message",
            "voice_xp_amount",
            "welcome_title",
            "points_emoji",
            "points_min"
        ]
    );
    assert_eq!(
        errors[4].1, "The least points per message can't be above the most.",
        "the /progression economy points-settings sentence, checked against the stored maximum of 3"
    );

    // Channels must be text or announcement channels that exist; the join role one the bot can give.
    let voice = browser
        .write(
            Method::PATCH,
            &path,
            json!({ "welcome_channel_id": LOUNGE.to_string() }),
        )
        .await;
    assert_eq!(
        refused(&voice),
        [(
            "welcome_channel_id".to_owned(),
            "#Lounge isn't a text or announcement channel.".to_owned()
        )]
    );
    let gone = browser
        .write(Method::PATCH, &path, json!({ "log_channel_id": "123456" }))
        .await;
    assert_eq!(refused(&gone)[0].0, "log_channel_id");
    let above = browser
        .write(Method::PATCH, &path, json!({ "join_role_id": ADMIN.to_string() }))
        .await;
    assert_eq!(
        refused(&above),
        [(
            "join_role_id".to_owned(),
            "@Admin is above my highest role. Drag my role above it in Server Settings > Roles, then try again."
                .to_owned()
        )]
    );
    let not_an_id = browser
        .write(Method::PATCH, &path, json!({ "join_role_id": "Fan" }))
        .await;
    assert_eq!(refused(&not_an_id)[0].0, "join_role_id");

    let untouched = h.state.app.db.guild_settings(guild).await.unwrap();
    assert_eq!(untouched.welcome_channel_id, None);
    assert_eq!(untouched.join_role_id, None);
    assert!(
        h.state.app.db.audit_page(guild, None, 10).await.unwrap().is_empty(),
        "nothing was written, nothing recorded"
    );
}

#[tokio::test]
async fn xp_multipliers_follow_the_command_and_reach_the_bot() {
    let Some(h) = Harness::new("xp_rules").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let bot = h.bot_pod().await;
    listen_for_invalidations(&bot).await;
    let base = format!("/api/guilds/{guild}/xp-rules");

    // The bot caches the server's (empty) rules and pays XP in #2.
    let first = unique();
    on_message(bot.clone(), chat(guild, first)).await.unwrap();
    assert!(server_xp(&bot, guild, first).await > 0);

    let none = browser
        .write(Method::PUT, &format!("{base}/role/{guild}"), json!({ "multiplier": 0 }))
        .await;
    assert_eq!(none.status, StatusCode::OK, "{}", none.text());
    assert_eq!(
        none.json()["items"],
        json!([{ "kind": "role", "target_id": guild.to_string(), "multiplier": 0.0 }]),
        "@everyone set to no XP"
    );
    let mut earned = 1;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) && earned > 0 {
        let user = unique();
        on_message(bot.clone(), chat(guild, user)).await.unwrap();
        earned = server_xp(&bot, guild, user).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(earned, 0, "the bot dropped its cached rules at once");

    let doubled = browser
        .write(
            Method::PUT,
            &format!("{base}/channel/{GENERAL}"),
            json!({ "multiplier": 2.004 }),
        )
        .await;
    assert_eq!(doubled.status, StatusCode::OK, "{}", doubled.text());
    let items = doubled.json()["items"].clone();
    assert_eq!(
        items[0],
        json!({ "kind": "channel", "target_id": GENERAL.to_string(), "multiplier": 2.0 })
    );

    let too_much = browser
        .write(
            Method::PUT,
            &format!("{base}/channel/{GENERAL}"),
            json!({ "multiplier": 6 }),
        )
        .await;
    assert_eq!(too_much.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(too_much.json()["error"]["message"], "A multiplier goes from 0 to 5.");
    let nowhere = browser
        .write(
            Method::PUT,
            &format!("{base}/channel/424242"),
            json!({ "multiplier": 2 }),
        )
        .await;
    assert_eq!(nowhere.json()["error"]["field"], "target_id");
    let wrong_kind = browser
        .write(
            Method::PUT,
            &format!("{base}/user/{GENERAL}"),
            json!({ "multiplier": 2 }),
        )
        .await;
    assert_eq!(wrong_kind.status, StatusCode::NOT_FOUND);

    // 1 drops a rule, like `/progression leveling xp-multiplier value:1`.
    let normal = browser
        .write(
            Method::PUT,
            &format!("{base}/channel/{GENERAL}"),
            json!({ "multiplier": 1 }),
        )
        .await;
    assert_eq!(normal.json()["items"].as_array().unwrap().len(), 1);
    let removed = browser
        .write(Method::DELETE, &format!("{base}/role/{guild}"), json!({}))
        .await;
    assert_eq!(removed.json()["items"], json!([]));
    assert!(h.state.app.db.xp_rules(guild).await.unwrap().is_empty());

    // At the limit, a new rule is refused with the limit code.
    for n in 0..pinchora_db::MAX_XP_RULES as u64 {
        h.state
            .app
            .db
            .set_xp_rule(guild, pinchora_db::XpTarget::Channel, 100_000 + n, 2.0)
            .await
            .unwrap();
    }
    let full = browser
        .write(
            Method::PUT,
            &format!("{base}/channel/{NEWS}"),
            json!({ "multiplier": 2 }),
        )
        .await;
    assert_eq!(
        (full.status, full.code().as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "limit")
    );
    let cleared = browser.write(Method::DELETE, &base, json!({})).await;
    assert_eq!(cleared.json()["items"], json!([]));

    let actions: Vec<String> = h
        .state
        .app
        .db
        .audit_page(guild, None, 20)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.action)
        .collect();
    assert_eq!(
        actions,
        [
            "xp_rules.clear",
            "xp_rules.delete",
            "xp_rules.put",
            "xp_rules.put",
            "xp_rules.put"
        ]
    );
    let clear = h.state.app.db.audit_page(guild, None, 1).await.unwrap().remove(0);
    let before: Value = serde_json::from_str(clear.before.as_deref().unwrap()).unwrap();
    assert_eq!(
        before["rules"].as_array().map(Vec::len),
        Some(pinchora_db::MAX_XP_RULES as usize),
        "a clear keeps the rules it removed"
    );
}

#[tokio::test]
async fn level_roles_rep_roles_and_the_store_refuse_what_the_bot_cannot_give() {
    let Some(h) = Harness::new("reward_lists").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let g = format!("/api/guilds/{guild}");

    let added = browser
        .write(Method::PUT, &format!("{g}/level-roles/{FAN}"), json!({ "level": 5 }))
        .await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.text());
    assert_eq!(
        added.json(),
        json!({ "items": [{ "role_id": FAN.to_string(), "level": 5 }], "max": 25, "stack": true })
    );
    let moved = browser
        .write(Method::PUT, &format!("{g}/level-roles/{FAN}"), json!({ "level": 10 }))
        .await;
    assert_eq!(
        moved.json()["items"][0]["level"],
        10,
        "adding again moves it, like the command"
    );
    for (role, sentence) in [
        (
            ADMIN,
            "@Admin is above my highest role. Drag my role above it in Server Settings > Roles, then try again.",
        ),
        (
            BOOSTER,
            "@Server Booster is managed by an integration or a bot, so it can't be handed out.",
        ),
        (guild, "@everyone can't be handed out: every member already has it."),
    ] {
        let refused = browser
            .write(Method::PUT, &format!("{g}/level-roles/{role}"), json!({ "level": 3 }))
            .await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(refused.json()["error"]["message"], sentence);
        assert_eq!(refused.json()["error"]["field"], "role_id");
    }
    let level_zero = browser
        .write(
            Method::PUT,
            &format!("{g}/level-roles/{REGULAR}"),
            json!({ "level": 0 }),
        )
        .await;
    assert_eq!(level_zero.json()["error"]["field"], "level");
    let gone = browser
        .write(Method::DELETE, &format!("{g}/level-roles/{FAN}"), json!({}))
        .await;
    assert_eq!(gone.json()["items"], json!([]));

    let rep = browser
        .write(
            Method::PUT,
            &format!("{g}/rep-roles/{REGULAR}"),
            json!({ "reputation": 50 }),
        )
        .await;
    assert_eq!(
        rep.json(),
        json!({ "items": [{ "role_id": REGULAR.to_string(), "reputation": 50 }], "max": 25 })
    );
    let rep_above = browser
        .write(
            Method::PUT,
            &format!("{g}/rep-roles/{ADMIN}"),
            json!({ "reputation": 5 }),
        )
        .await;
    assert_eq!(rep_above.status, StatusCode::UNPROCESSABLE_ENTITY);

    let sale = browser
        .write(
            Method::PUT,
            &format!("{g}/store/{REGULAR}"),
            json!({ "price": 250, "stock": 3 }),
        )
        .await;
    assert_eq!(sale.status, StatusCode::OK, "{}", sale.text());
    assert_eq!(sale.json()["items"][0]["price"], 250);
    assert_eq!(sale.json()["items"][0]["stock"], 3);
    let unlimited = browser
        .write(
            Method::PUT,
            &format!("{g}/store/{REGULAR}"),
            json!({ "price": 300, "stock": null }),
        )
        .await;
    assert_eq!(unlimited.json()["items"][0]["stock"], Value::Null);
    let bad_price = browser
        .write(
            Method::PUT,
            &format!("{g}/store/{FAN}"),
            json!({ "price": 0, "stock": 0 }),
        )
        .await;
    assert_eq!(fields_of(&bad_price), ["price", "stock"]);
    let off = browser
        .write(Method::DELETE, &format!("{g}/store/{REGULAR}"), json!({}))
        .await;
    assert_eq!(off.json()["items"], json!([]));

    let actions: Vec<String> = h
        .state
        .app
        .db
        .audit_page(guild, None, 20)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.action)
        .collect();
    assert_eq!(
        actions,
        [
            "store.delete",
            "store.put",
            "store.put",
            "rep_roles.put",
            "level_roles.delete",
            "level_roles.put",
            "level_roles.put"
        ]
    );
}

#[tokio::test]
async fn the_sync_job_gives_members_the_roles_they_earned() {
    let Some(h) = Harness::new("level_role_sync").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let g = format!("/api/guilds/{guild}");
    let none = browser
        .write(Method::POST, &format!("{g}/level-roles/sync"), json!({}))
        .await;
    assert_eq!(none.status, StatusCode::UNPROCESSABLE_ENTITY, "no level roles yet");

    let (veteran, novice) = (unique(), unique());
    h.seed_xp(guild, veteran, 5_000).await;
    h.seed_xp(guild, novice, 10).await;
    browser
        .write(Method::PUT, &format!("{g}/level-roles/{FAN}"), json!({ "level": 2 }))
        .await;
    let started = browser
        .write(Method::POST, &format!("{g}/level-roles/sync"), json!({}))
        .await;
    assert_eq!(started.status, StatusCode::ACCEPTED, "{}", started.text());
    let job = started.json()["job_id"].as_str().unwrap().to_owned();

    let mut state = Value::Null;
    let waited = Instant::now();
    while waited.elapsed() < Duration::from_secs(10) {
        state = browser.get(&format!("{g}/jobs/{job}")).await.json();
        if state["status"] != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(state["status"], "done", "{state}");
    assert_eq!(
        (state["checked"].as_u64(), state["updated"].as_u64()),
        (Some(2), Some(1))
    );
    assert_eq!(
        state["message"], "Checked **2** members and updated the level roles of **1**.",
        "the command's own sentence"
    );
    let calls = h.mock.role_calls.lock().unwrap().clone();
    assert_eq!(calls, [format!("PUT {veteran} {FAN}")]);

    // Another server cannot read this one's job.
    let (other, _, mut stranger) = h.managed_server().await;
    let foreign = stranger.get(&format!("/api/guilds/{other}/jobs/{job}")).await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
    let audit = h.state.app.db.audit_page(guild, None, 1).await.unwrap();
    assert_eq!(audit[0].action, "level_roles.sync");
}

#[tokio::test]
async fn message_previews_fill_in_the_placeholders() {
    let Some(h) = Harness::new("message_preview").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let text = browser
        .write(
            Method::POST,
            &format!("/api/guilds/{guild}/preview/message"),
            json!({ "kind": "welcome", "message": "Hi {user}, member #{count} of {server} {unknown}" }),
        )
        .await;
    assert_eq!(
        text.json()["text"],
        format!("Hi @user{owner} G, member #1234 of Pinchora Reef {{unknown}}")
    );
    let default = browser
        .write(
            Method::POST,
            &format!("/api/guilds/{guild}/preview/message"),
            json!({ "kind": "levelup" }),
        )
        .await;
    assert_eq!(
        default.json()["text"],
        format!("🎉 @user{owner} G reached **level 5**!")
    );
}

/// A PNG of `w`×`h` in one colour.
fn png_of(w: u32, h: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(w, h, image::Rgba([40, 90, 160, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// The new member `user` as the gateway announces them.
fn joined(guild: u64, user: u64) -> MemberAdd {
    serde_json::from_value(json!({
        "guild_id": guild.to_string(),
        "user": user_json(user, &format!("user{user}")),
        "nick": null,
        "roles": [],
        "joined_at": "2024-01-01T00:00:00.000000+00:00",
        "deaf": false,
        "mute": false,
        "flags": 0,
        "pending": false,
    }))
    .expect("member add JSON")
}

#[tokio::test]
async fn the_card_editor_previews_saves_uploads_and_tests_designs() {
    let Some(h) = Harness::new("card_editor").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let base = format!("/api/guilds/{guild}");

    // A server that never opened the editor sends the classic card, with goodbyes as text.
    let cards = browser.get(&format!("{base}/cards")).await;
    assert_eq!(cards.status, StatusCode::OK, "{}", cards.text());
    let cards = cards.json();
    let classic = pinchora_render::templates::classic(CardKind::Welcome, "reef", None, None, Lang::En);
    assert_eq!(cards["welcome"]["design"], json!(classic));
    assert_eq!(
        (&cards["welcome"]["custom"], &cards["welcome"]["delivery"]),
        (&json!(false), &json!("with_message"))
    );
    assert_eq!(cards["goodbye"]["delivery"], "text_only");
    assert_eq!(
        cards["goodbye"]["deliveries"].as_array().unwrap().len(),
        4,
        "no DMs to who left"
    );
    assert_eq!(cards["templates"].as_array().unwrap().len(), 8);
    assert_eq!(cards["uploads"], json!([]));

    // The preview is the bot's own drawing, byte for byte, with the requester as the member.
    let banner = cards["templates"][5]["welcome"].clone();
    assert_eq!(cards["templates"][5]["key"], "starlit-banner");
    let preview = browser
        .write(
            Method::POST,
            &format!("{base}/preview/card"),
            json!({ "design": banner }),
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    assert_eq!(preview.header("content-type"), "image/png");
    assert!(preview.header("cache-control").contains("no-store"));
    let shown = save_png("card_editor", "banner-preview", &preview.body);
    let subject = CardSubject {
        user: Id::new(owner),
        avatar: None,
        name: format!("user{owner} G"),
        username: format!("user{owner}"),
        server: "Pinchora Reef".into(),
        members: Some(1234),
    };
    let design = serde_json::from_value(banner.clone()).unwrap();
    let direct = card_png(&h.state.app, guild, &design, &subject).await.unwrap();
    assert!(
        preview.body.as_ref() == direct.as_slice(),
        "byte for byte: {}",
        shown.display()
    );

    // A broken design is refused with the validator's sentences, and nothing is saved.
    let mut broken = banner.clone();
    broken["dim"] = json!(95);
    broken["layers"][2]["text"] = json!("Hi {nickname}");
    let bad = browser
        .write(
            Method::POST,
            &format!("{base}/preview/card"),
            json!({ "design": broken }),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", bad.text());
    assert_eq!(fields_of(&bad), ["dim", "layers"]);
    assert!(bad.text().contains("{nickname} isn't a placeholder"), "{}", bad.text());
    let put = browser
        .write(
            Method::PUT,
            &format!("{base}/cards/welcome"),
            json!({ "design": broken, "delivery": "with_message" }),
        )
        .await;
    assert_eq!(put.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(h.state.app.db.audit_page(guild, None, 5).await.unwrap().is_empty());

    // Uploads: only pictures, re-encoded and shrunk to fit the largest canvas.
    let upload = |content_type: &'static str, bytes: Vec<u8>| {
        let mut browser = browser.clone();
        let path = format!("{base}/cards/uploads");
        async move {
            let request = Request::builder()
                .method(Method::POST)
                .uri(path)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::ORIGIN, PUBLIC_URL)
                .header("sec-fetch-site", "same-origin")
                .header("x-csrf-token", browser.csrf.clone().unwrap())
                .body(Body::from(bytes))
                .unwrap();
            browser.send(request).await
        }
    };
    let text = upload("text/plain", b"hello".to_vec()).await;
    assert_eq!(text.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(text.text().contains("PNG, JPEG, WebP or GIF"), "{}", text.text());
    let junk = upload("image/png", b"not a picture at all".to_vec()).await;
    assert_eq!(junk.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", junk.text());
    let huge = upload(
        "image/png",
        vec![0; pinchora_core::card_design::UPLOAD_MAX_BYTES + 2048],
    )
    .await;
    assert_eq!(huge.status, StatusCode::PAYLOAD_TOO_LARGE);
    let sent = upload("image/png", png_of(2000, 1500)).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    let sent = sent.json();
    assert_eq!((&sent["width"], &sent["height"]), (&json!(1333), &json!(1000)));
    let id = sent["id"].as_str().unwrap().to_owned();
    let stored = browser.get(sent["url"].as_str().unwrap()).await;
    assert_eq!(stored.status, StatusCode::OK);
    assert_eq!(image::load_from_memory(&stored.body).unwrap().width(), 1333);

    // The design with that upload behind it is saved, audited and announced.
    let mut mine = banner.clone();
    mine["background"] = json!({ "type": "upload", "id": id });
    let mut heard = Box::pin(h.state.app.cache.invalidations().await.expect("subscribe"));
    let saved = browser
        .write(
            Method::PUT,
            &format!("{base}/cards/welcome"),
            json!({ "design": mine, "delivery": "before_message" }),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    assert_eq!(
        (&saved.json()["custom"], &saved.json()["delivery"]),
        (&json!(true), &json!("before_message"))
    );
    assert!(announced(&mut heard, Invalidation::Guild(guild)).await);
    let audit = h.state.app.db.audit_page(guild, None, 5).await.unwrap();
    let actions: Vec<_> = audit.iter().map(|a| a.action.as_str()).collect();
    assert_eq!(actions, ["card.put", "card.upload"]);
    assert_eq!(
        browser.get(&format!("{base}/cards")).await.json()["welcome"]["design"],
        mine
    );

    // Another server cannot borrow it.
    let (other, _, mut stranger) = h.managed_server().await;
    let borrowed = stranger
        .write(
            Method::PUT,
            &format!("/api/guilds/{other}/cards/welcome"),
            json!({ "design": mine, "delivery": "with_message" }),
        )
        .await;
    assert_eq!(borrowed.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(fields_of(&borrowed), ["background"]);
    assert_eq!(
        stranger
            .get(&format!("/api/guilds/{other}/cards/uploads/{id}"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // A test message goes to a text channel, pinging nobody, with the image first as chosen.
    h.mock.posts.lock().unwrap().clear();
    let voice = browser
        .write(
            Method::POST,
            &format!("{base}/cards/welcome/test"),
            json!({ "design": mine, "delivery": "before_message", "channel_id": LOUNGE.to_string() }),
        )
        .await;
    assert_eq!(voice.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(fields_of(&voice), ["channel_id"]);
    let tested = browser
        .write(
            Method::POST,
            &format!("{base}/cards/welcome/test"),
            json!({ "design": mine, "delivery": "before_message", "channel_id": GENERAL.to_string() }),
        )
        .await;
    assert_eq!(tested.status, StatusCode::OK, "{}", tested.text());
    let posts = std::mem::take(&mut *h.mock.posts.lock().unwrap());
    assert_eq!(posts.len(), 2, "the image, then the text");
    assert!(posts.iter().all(|(channel, _)| *channel == GENERAL));
    assert!(posts[0].1.contains("filename=\"welcome.png\""));
    assert!(posts[1].1.contains("Test of the welcome image") && !posts[1].1.contains("filename="));
    assert!(
        posts[1].1.contains(r#""allowed_mentions":{"parse":[]}"#),
        "{}",
        posts[1].1
    );

    // The note under a test message speaks the server's language.
    h.state.app.set_guild_lang(guild, Some(Lang::Tr)).await.unwrap();
    let turkish = browser
        .write(
            Method::POST,
            &format!("{base}/cards/welcome/test"),
            json!({ "design": mine, "delivery": "text_only", "channel_id": GENERAL.to_string() }),
        )
        .await;
    assert_eq!(turkish.status, StatusCode::OK, "{}", turkish.text());
    let posts = std::mem::take(&mut *h.mock.posts.lock().unwrap());
    assert_eq!(posts.len(), 1, "{posts:?}");
    assert!(
        posts[0].1.contains("Karşılama görselinin denemesi") && !posts[0].1.contains("Test of the"),
        "{}",
        posts[0].1
    );
    h.state.app.set_guild_lang(guild, None).await.unwrap();

    // A new member gets the saved design, sent the saved way.
    let settings = browser
        .write(
            Method::PATCH,
            &format!("{base}/settings"),
            json!({ "welcome_enabled": true, "welcome_channel_id": GENERAL.to_string() }),
        )
        .await;
    assert_eq!(settings.status, StatusCode::OK, "{}", settings.text());
    let bot = h.bot_pod().await;
    let member = unique();
    on_member_add(bot.clone(), joined(guild, member)).await.expect("join");
    let posts = std::mem::take(&mut *h.mock.posts.lock().unwrap());
    assert_eq!(posts.len(), 2, "{posts:?}");
    assert!(posts[0].1.contains("filename=\"welcome.png\""));
    assert!(posts[1].1.contains(&format!("<@{member}>")));

    // Goodbyes cannot go by DM; back to the classic card keeps the delivery chosen.
    let dm = browser
        .write(
            Method::PUT,
            &format!("{base}/cards/goodbye"),
            json!({ "design": null, "delivery": "dm" }),
        )
        .await;
    assert_eq!(fields_of(&dm), ["delivery"]);
    let back = browser
        .write(
            Method::PUT,
            &format!("{base}/cards/welcome"),
            json!({ "design": null, "delivery": "image_only" }),
        )
        .await;
    assert_eq!(back.json()["custom"], false);
    assert_eq!(back.json()["design"], json!(classic));
    on_member_add(bot, joined(guild, unique())).await.expect("join");
    let posts = std::mem::take(&mut *h.mock.posts.lock().unwrap());
    assert_eq!(posts.len(), 1, "only the image");
    assert!(posts[0].1.contains("filename=\"welcome.png\""));
}

/// A design with nothing on it, as small as a card goes, which draws in no time.
fn blank_design() -> Value {
    json!({
        "version": 1, "width": 300, "height": 150,
        "background": { "type": "solid", "color": "#102030" }, "dim": 0, "layers": [],
    })
}

#[tokio::test]
async fn previews_are_limited_for_the_whole_server_and_refuse_what_is_too_heavy() {
    let Some(h) = Harness::new("card_preview_limit").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/preview/card");
    for n in 1..=20 {
        let drawn = browser
            .write(Method::POST, &path, json!({ "design": blank_design() }))
            .await;
        assert_eq!(drawn.status, StatusCode::OK, "preview {n}: {}", drawn.text());
    }
    let refused = browser
        .write(Method::POST, &path, json!({ "design": blank_design() }))
        .await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS, "{}", refused.text());
    assert!(refused.header("retry-after").parse::<u64>().is_ok_and(|s| s >= 1));
    // The limit is the server's: another server's managers draw as many as they like.
    let (other_guild, _, mut stranger) = h.managed_server().await;
    let other = format!("/api/guilds/{other_guild}/preview/card");
    let drawn = stranger
        .write(Method::POST, &other, json!({ "design": blank_design() }))
        .await;
    assert_eq!(drawn.status, StatusCode::OK, "{}", drawn.text());

    // Too many layers, too big a layer or too heavy a pile is refused with the validator's sentence.
    let mut heavy = blank_design();
    let avatar = |i: usize| {
        json!({
            "id": format!("a{i}"), "type": "avatar", "x": 0, "y": 0, "w": 150, "h": 150,
            "shape": "circle", "border_color": "#000000", "border_width": 0,
        })
    };
    heavy["layers"] = json!((0..5).map(avatar).collect::<Vec<_>>());
    let bad = stranger.write(Method::POST, &other, json!({ "design": heavy })).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", bad.text());
    assert!(bad.text().contains("at most 4 avatars"), "{}", bad.text());
    let mut bigger = blank_design();
    bigger["layers"] = json!([{
        "id": "s", "type": "shape", "x": 0, "y": 0, "w": 301, "h": 150,
        "shape": "rect", "fill": "#336699", "border_color": "#000000",
    }]);
    let bad = stranger.write(Method::POST, &other, json!({ "design": bigger })).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", bad.text());
    assert!(
        bad.text().contains("bigger than the canvas (300×150 px)"),
        "{}",
        bad.text()
    );
}

#[tokio::test]
async fn previews_wait_in_their_own_lane_and_give_up_rather_than_crowd_out_the_bots_cards() {
    let Some(h) = Harness::new("preview_lane").await else {
        return;
    };
    // Four previews at once, each holding its slot for four seconds (the lane waits three): the slots fill,
    // the rest are told the bot is busy.
    let jobs: Vec<_> = (0..4)
        .map(|_| {
            let app = h.state.app.clone();
            tokio::spawn(async move {
                app.render_preview(|_| {
                    std::thread::sleep(Duration::from_secs(4));
                    Ok(())
                })
                .await
            })
        })
        .collect();
    let (mut drawn, mut busy) = (0, 0);
    for job in jobs {
        match job.await.expect("joined") {
            Ok(()) => drawn += 1,
            Err(PreviewError::Busy) => busy += 1,
            Err(other) => panic!("unexpected: {other}"),
        }
    }
    assert!(drawn >= 1 && busy >= 2, "drawn {drawn}, busy {busy}");
}

#[tokio::test]
async fn a_preview_that_takes_too_long_is_given_up_on() {
    let Some(h) = Harness::new("preview_slow").await else {
        return;
    };
    let slow = h
        .state
        .app
        .render_preview(|_| {
            std::thread::sleep(Duration::from_secs(8));
            Ok(())
        })
        .await;
    assert!(matches!(slow, Err(PreviewError::TooSlow)), "{slow:?}");
    // The job still holds its slot until it ends, so the lane is as busy as it really is.
}

#[tokio::test]
async fn a_noisy_upload_is_shrunk_until_it_is_small_enough_to_keep() {
    let Some(h) = Harness::new("card_upload_noise").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    // Random pixels: they hardly compress, so the stored PNG would be over a megabyte at this size.
    let mut state = 12345u32;
    let noise = image::RgbImage::from_fn(600, 500, |_, _| {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let v = state.to_le_bytes();
        image::Rgb([v[1], v[2], v[3]])
    });
    let mut sent = std::io::Cursor::new(Vec::new());
    noise.write_to(&mut sent, image::ImageFormat::Png).unwrap();
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/guilds/{guild}/cards/uploads"))
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::ORIGIN, PUBLIC_URL)
        .header("sec-fetch-site", "same-origin")
        .header("x-csrf-token", browser.csrf.clone().unwrap())
        .body(Body::from(sent.into_inner()))
        .unwrap();
    let made = browser.send(request).await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    let made = made.json();
    let stored = browser.get(made["url"].as_str().unwrap()).await;
    assert!(
        stored.body.len() <= pinchora_core::card_design::UPLOAD_STORED_MAX_BYTES,
        "{} bytes kept",
        stored.body.len()
    );
    let (w, tall) = (made["width"].as_f64().unwrap(), made["height"].as_f64().unwrap());
    assert!(w < 600.0 && tall < 500.0, "shrunk: {w}x{tall}");
    assert!((w / tall - 1.2).abs() < 0.02, "in proportion: {w}x{tall}");
}

#[tokio::test]
async fn suggestions_and_critter_drops_have_their_own_settings() {
    let Some(h) = Harness::new("community").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let g = format!("/api/guilds/{guild}");
    let start = browser.get(&format!("{g}/community")).await.json();
    assert_eq!(start["suggestions"], json!({ "channel_id": null }));
    assert_eq!(start["critters"], json!({ "enabled": false, "channel_id": null }));
    assert_eq!(start["surge"]["enabled"], false, "timed storms start off");

    let on = browser
        .write(
            Method::PUT,
            &format!("{g}/suggestions/settings"),
            json!({ "channel_id": NEWS.to_string() }),
        )
        .await;
    assert_eq!(on.json(), json!({ "channel_id": NEWS.to_string() }));
    assert_eq!(h.state.app.db.suggestion_channel(guild).await.unwrap(), Some(NEWS));
    let voice = browser
        .write(
            Method::PUT,
            &format!("{g}/suggestions/settings"),
            json!({ "channel_id": LOUNGE.to_string() }),
        )
        .await;
    assert_eq!(voice.status, StatusCode::UNPROCESSABLE_ENTITY);

    let critters = browser
        .write(
            Method::PUT,
            &format!("{g}/critters/settings"),
            json!({ "enabled": true, "channel_id": GENERAL.to_string() }),
        )
        .await;
    assert_eq!(
        critters.json(),
        json!({ "enabled": true, "channel_id": GENERAL.to_string() })
    );
    let anywhere = browser
        .write(
            Method::PUT,
            &format!("{g}/critters/settings"),
            json!({ "channel_id": null }),
        )
        .await;
    assert_eq!(anywhere.json(), json!({ "enabled": true, "channel_id": null }));
    let same = browser
        .write(
            Method::PUT,
            &format!("{g}/critters/settings"),
            json!({ "enabled": true }),
        )
        .await;
    assert_eq!(same.status, StatusCode::OK);

    let actions: Vec<String> = h
        .state
        .app
        .db
        .audit_page(guild, None, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.action)
        .collect();
    assert_eq!(actions, ["critters.set", "critters.set", "suggestions.set"]);
}

#[tokio::test]
async fn the_audit_log_pages_through_every_change() {
    let Some(h) = Harness::new("audit_pages").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    for n in 0..55 {
        h.state
            .app
            .db
            .audit(&pinchora_db::NewAudit {
                guild,
                actor: 7,
                actor_name: "Crab",
                source: pinchora_db::AuditSource::Web,
                action: "settings.patch",
                target: None,
                before: Some("{}"),
                after: Some(&format!(r#"{{"n": {n}}}"#)),
            })
            .await
            .unwrap();
    }
    let first = browser.get(&format!("/api/guilds/{guild}/audit")).await.json();
    assert_eq!(first["items"].as_array().unwrap().len(), 50);
    assert_eq!(first["items"][0]["after"], json!({ "n": 54 }), "newest first");
    let next = first["next"].as_str().expect("a next page").to_owned();
    let second = browser
        .get(&format!("/api/guilds/{guild}/audit?before={next}"))
        .await
        .json();
    assert_eq!(second["items"].as_array().unwrap().len(), 5);
    assert_eq!(second["next"], Value::Null);
    let bad = browser.get(&format!("/api/guilds/{guild}/audit?before=x")).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
}

// /Hardening

#[tokio::test]
async fn a_trusted_proxy_names_the_client_for_the_login_limit() {
    let Some(h) = Harness::with("trusted_proxy", |web| web.trusted_proxies = "10.0.0.0/8".into()).await else {
        return;
    };
    // This browser connects through the explicitly trusted proxy at 10.x.
    let behind = |client: &str| {
        Request::get("/auth/login")
            .header("x-forwarded-for", client)
            .body(Body::empty())
            .unwrap()
    };
    let mut proxy = h.browser();
    proxy.address = SocketAddr::from(([10, 0, 0, 1], 4000));
    let clients: [String; 2] =
        std::array::from_fn(|_| Ipv6Addr::from((0x2001_0db8_u128 << 96) | u128::from(unique())).to_string());
    let mut statuses = Vec::new();
    for _ in 0..21 {
        statuses.push(proxy.send(behind(&clients[0])).await.status);
    }
    assert!(statuses.contains(&StatusCode::TOO_MANY_REQUESTS), "{statuses:?}");
    assert_eq!(
        proxy.send(behind(&clients[1])).await.status,
        StatusCode::SEE_OTHER,
        "another client behind the same proxy has its own bucket"
    );

    // An untrusted peer cannot pick a bucket by sending the header itself.
    let mut outsider = h.browser();
    let mut statuses = Vec::new();
    for n in 0..21 {
        statuses.push(outsider.send(behind(&format!("192.0.2.{n}"))).await.status);
    }
    assert!(statuses.contains(&StatusCode::TOO_MANY_REQUESTS), "{statuses:?}");
}

#[tokio::test]
async fn concurrent_requests_refresh_a_token_once() {
    let Some(h) = Harness::new("refresh_race").await else {
        return;
    };
    h.mock.short_tokens.store(true, Ordering::SeqCst);
    let user = unique();
    let mut browser = h.browser();
    browser.login(&h, user, vec![]).await;
    // From now on Discord rotates refresh tokens, slowly, and the new tokens last a week.
    h.mock.short_tokens.store(false, Ordering::SeqCst);
    h.mock.rotate.store(true, Ordering::SeqCst);
    h.mock.token_delay_ms.store(300, Ordering::SeqCst);
    let before = h.mock.token_forms.lock().unwrap().len();

    let (mut a, mut b, mut c) = (browser.clone(), browser.clone(), browser.clone());
    let (ra, rb, rc) = tokio::join!(
        a.get("/api/me/guilds"),
        b.get("/api/me/guilds"),
        c.get("/api/me/guilds")
    );
    for r in [&ra, &rb, &rc] {
        assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    }
    let refreshes: Vec<String> = h.mock.token_forms.lock().unwrap()[before..]
        .iter()
        .map(|f| f["grant_type"].clone())
        .collect();
    assert_eq!(refreshes, ["refresh_token"], "one refresh for three requests");

    // The session kept the rotated token: the next request works without refreshing again.
    let again = browser.write(Method::POST, "/api/me/guilds/refresh", json!({})).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    assert_eq!(h.mock.token_forms.lock().unwrap().len(), before + 1);
}

#[tokio::test]
async fn hsts_is_sent_only_over_https() {
    let Some(plain) = Harness::new("hsts_plain").await else {
        return;
    };
    let page = plain.browser().get("/").await;
    assert_eq!(page.header("strict-transport-security"), "");

    let Some(secure) = Harness::with("hsts_secure", |web| web.cookie_secure = Some(true)).await else {
        return;
    };
    for path in ["/", "/api/me", "/auth/login"] {
        let response = secure.browser().get(path).await;
        assert_eq!(
            response.header("strict-transport-security"),
            "max-age=31536000; includeSubDomains",
            "{path}"
        );
    }
}

#[tokio::test]
async fn logging_in_again_ends_the_old_session() {
    let Some(h) = Harness::new("relogin").await else {
        return;
    };
    let user = unique();
    let mut browser = h.browser();
    browser.login(&h, user, vec![]).await;
    let old = browser.cookies["pinchora_sid"].clone();
    let mut copy = browser.clone();
    assert_eq!(copy.get("/api/me").await.status, StatusCode::OK);

    browser.login(&h, user, vec![]).await;
    assert_ne!(browser.cookies["pinchora_sid"], old);
    assert_eq!(browser.get("/api/me").await.status, StatusCode::OK);
    assert_eq!(
        copy.get("/api/me").await.status,
        StatusCode::UNAUTHORIZED,
        "the cookie from before the new login no longer works"
    );
}

// /P3: module switches

/// A bot pod's verdict for a module in a channel.
async fn access(
    app: &App,
    guild: u64,
    channel: u64,
    module: pinchora_core::modules::Module,
) -> pinchora_core::modules::Access {
    app.module_access(guild, Some(channel), module)
        .await
        .expect("module access")
}

#[tokio::test]
async fn module_switches_save_with_the_command_rules_and_reach_the_bot() {
    use pinchora_core::modules::{Access, Group, Module};
    let Some(h) = Harness::new("modules").await else {
        return;
    };
    let guild = unique();
    h.add_bot(guild);
    let bot = h.bot_pod().await;
    listen_for_invalidations(&bot).await;
    let mut heard = Box::pin(h.state.app.cache.invalidations().await.expect("subscribe"));
    let manager = unique();
    let mut browser = h.browser();
    browser
        .login(&h, manager, vec![guild_entry(guild, "Reef", false, MANAGE_GUILD)])
        .await;
    let g = format!("/api/guilds/{guild}");

    // Everything starts on, except welcome cards and gambling, which are off until a manager says so.
    let list = browser.get(&format!("{g}/modules")).await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.text());
    let list = list.json();
    assert_eq!(list["max_channels"], 50);
    assert_eq!(list["groups"].as_array().unwrap().len(), Group::ALL.len());
    let items = list["items"].as_array().unwrap();
    assert_eq!(items.len(), Module::ALL.len());
    for item in items {
        let key = item["key"].as_str().unwrap();
        assert_eq!(item["enabled"], !matches!(key, "gambling" | "welcome"), "{key}");
        assert_eq!(item["channel_mode"], "all", "{key}");
        assert_eq!(item["channels"], json!([]), "{key}");
    }
    let overview = browser.get(&g).await.json();
    assert_eq!(overview["modules"].as_array().unwrap().len(), Module::ALL.len());

    // Off for the whole server: the bot hears it at once, and it is recorded.
    assert_eq!(access(&bot, guild, GENERAL, Module::Pets).await, Access::Allowed);
    let off = browser
        .write(Method::PUT, &format!("{g}/modules/pets"), json!({ "enabled": false }))
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    assert_eq!(off.json()["enabled"], false);
    assert!(
        announced(&mut heard, Invalidation::Guild(guild)).await,
        "guild:{guild} was published"
    );
    let started = Instant::now();
    while access(&bot, guild, GENERAL, Module::Pets).await == Access::Allowed
        && started.elapsed() < Duration::from_secs(5)
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(access(&bot, guild, GENERAL, Module::Pets).await, Access::ServerOff);
    let rows = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].action.as_str(), rows[0].target.as_deref()),
        ("module.set", Some("pets"))
    );
    assert_eq!(rows[0].source, "web");
    assert_eq!(rows[0].before.as_deref(), Some(r#"{"enabled": true}"#));
    assert_eq!(rows[0].after.as_deref(), Some(r#"{"enabled": false}"#));
    // The same value again changes nothing and records nothing.
    browser
        .write(Method::PUT, &format!("{g}/modules/pets"), json!({ "enabled": false }))
        .await;
    assert_eq!(h.state.app.db.audit_page(guild, None, 10).await.unwrap().len(), 1);

    // Back on, but only in one channel.
    let only = browser
        .write(
            Method::PUT,
            &format!("{g}/modules/pets"),
            json!({ "enabled": true, "channel_mode": "only", "channels": [GENERAL.to_string()] }),
        )
        .await;
    assert_eq!(only.status, StatusCode::OK, "{}", only.text());
    assert_eq!(only.json()["channel_mode"], "only");
    assert_eq!(only.json()["channels"], json!([GENERAL.to_string()]));
    let started = Instant::now();
    while access(&bot, guild, NEWS, Module::Pets).await == Access::ServerOff
        && started.elapsed() < Duration::from_secs(5)
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(access(&bot, guild, GENERAL, Module::Pets).await, Access::Allowed);
    assert_eq!(access(&bot, guild, NEWS, Module::Pets).await, Access::ChannelOff);
    // The other mode alone means a new list, which it does not have; the same mode alone keeps it.
    let bare = browser
        .write(
            Method::PUT,
            &format!("{g}/modules/pets"),
            json!({ "channel_mode": "except" }),
        )
        .await;
    assert_eq!(
        (bare.status, bare.json()["error"]["field"].clone()),
        (StatusCode::UNPROCESSABLE_ENTITY, json!("channels"))
    );
    let same_mode = browser
        .write(
            Method::PUT,
            &format!("{g}/modules/pets"),
            json!({ "channel_mode": "only" }),
        )
        .await;
    assert_eq!(same_mode.json()["channels"], json!([GENERAL.to_string()]));
    let except = browser
        .write(
            Method::PUT,
            &format!("{g}/modules/pets"),
            json!({ "channel_mode": "except", "channels": [NEWS.to_string()] }),
        )
        .await;
    assert_eq!(except.json()["channels"], json!([NEWS.to_string()]));
    // "All" forgets the list and the row goes away.
    let all = browser
        .write(
            Method::PUT,
            &format!("{g}/modules/pets"),
            json!({ "channel_mode": "all" }),
        )
        .await;
    assert_eq!(all.json()["channels"], json!([]));
    let stored: i64 = sqlx_count(&h, guild).await;
    assert_eq!(stored, 0, "a module that is on everywhere has no row");

    // The switches that already lived in the settings stay there: no second switch for one thing.
    let leveling = browser
        .write(
            Method::PUT,
            &format!("{g}/modules/leveling"),
            json!({ "enabled": false }),
        )
        .await;
    assert_eq!(leveling.json()["enabled"], false);
    assert!(!h.state.app.db.guild_settings(guild).await.unwrap().leveling_enabled);
    let settings = browser.get(&g).await.json();
    assert_eq!(settings["settings"]["leveling_enabled"], false);
    let patched = browser
        .write(
            Method::PATCH,
            &format!("{g}/settings"),
            json!({ "points_enabled": false }),
        )
        .await;
    assert_eq!(patched.status, StatusCode::OK);
    let points = browser.get(&format!("{g}/modules")).await.json();
    let points = points["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["key"] == "points")
        .cloned()
        .unwrap();
    assert_eq!(
        points["enabled"], false,
        "the settings page and the module grid are one switch"
    );
    // Leveling keeps a channel rule next to its column switch.
    let rule = browser
        .write(
            Method::PUT,
            &format!("{g}/modules/leveling"),
            json!({ "enabled": true, "channel_mode": "except", "channels": [NEWS.to_string()] }),
        )
        .await;
    assert_eq!(rule.status, StatusCode::OK, "{}", rule.text());
    assert!(h.state.app.db.guild_settings(guild).await.unwrap().leveling_enabled);
    let gambling = browser
        .write(
            Method::PUT,
            &format!("{g}/modules/gambling"),
            json!({ "enabled": true }),
        )
        .await;
    assert_eq!(gambling.json()["enabled"], true);
    assert!(h.state.app.db.gambling_enabled(guild).await.unwrap());
    let actions: Vec<(String, Option<String>)> = h
        .state
        .app
        .db
        .audit_page(guild, None, 20)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.action, r.target))
        .collect();
    assert!(
        actions.contains(&("module.set".into(), Some("gambling".into()))),
        "{actions:?}"
    );
    assert!(
        actions.contains(&("module.set".into(), Some("leveling".into()))),
        "{actions:?}"
    );
}

/// How many `guild_modules` rows a server has.
async fn sqlx_count(h: &Harness, guild: u64) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT count(*) FROM guild_modules WHERE guild_id = $1")
        .bind(guild as i64)
        .fetch_one(h.state.app.db.pool())
        .await
        .expect("count");
    row.0
}

#[tokio::test]
async fn module_rules_answer_with_the_field_and_the_command_sentence() {
    let Some(h) = Harness::new("module_rules").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let g = format!("/api/guilds/{guild}");
    let put = |module: &'static str, body: Value| {
        let path = format!("{g}/modules/{module}");
        (path, body)
    };
    let mut tries: Vec<(&str, Value, StatusCode, &str)> = vec![
        (
            "pets",
            json!({ "channel_mode": "sideways" }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "channel_mode",
        ),
        (
            "pets",
            json!({ "channel_mode": "only" }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "channels",
        ),
        (
            "pets",
            json!({ "channel_mode": "only", "channels": [] }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "channels",
        ),
        (
            "pets",
            json!({ "channel_mode": "except", "channels": ["0"] }),
            StatusCode::BAD_REQUEST,
            "",
        ),
        (
            "pets",
            json!({ "channel_mode": "only", "channels": ["general"] }),
            StatusCode::BAD_REQUEST,
            "",
        ),
        (
            "pets",
            json!({ "channel_mode": "only", "channels": ["424242"] }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "channels",
        ),
        (
            "welcome",
            json!({ "channel_mode": "only", "channels": [GENERAL.to_string()] }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "channel_mode",
        ),
        (
            "log",
            json!({ "channel_mode": "except", "channels": [GENERAL.to_string()] }),
            StatusCode::UNPROCESSABLE_ENTITY,
            "channel_mode",
        ),
        ("pets", json!({ "colour": "red" }), StatusCode::BAD_REQUEST, ""),
    ];
    let many: Vec<String> = (1..=51u64).map(|n| (7_100 + n).to_string()).collect();
    tries.push((
        "pets",
        json!({ "channel_mode": "only", "channels": many }),
        StatusCode::UNPROCESSABLE_ENTITY,
        "channels",
    ));
    for (module, body, status, field) in tries {
        let (path, body) = put(module, body);
        let response = browser.write(Method::PUT, &path, body.clone()).await;
        assert_eq!(response.status, status, "{module} {body}: {}", response.text());
        if !field.is_empty() {
            assert_eq!(response.json()["error"]["field"], field, "{module} {body}");
        }
    }
    let nothing = browser
        .write(
            Method::PUT,
            &format!("{g}/modules/nonsense"),
            json!({ "enabled": false }),
        )
        .await;
    assert_eq!(nothing.status, StatusCode::NOT_FOUND);
    // The sentence is the command's.
    let none = browser
        .write(
            Method::PUT,
            &format!("{g}/modules/pets"),
            json!({ "channel_mode": "only", "channels": [] }),
        )
        .await;
    assert_eq!(
        none.json()["error"]["message"],
        "Pick at least one channel, or choose all channels."
    );
    let welcome = browser
        .write(
            Method::PUT,
            &format!("{g}/modules/welcome"),
            json!({ "channel_mode": "only", "channels": [GENERAL.to_string()] }),
        )
        .await;
    assert!(
        welcome.json()["error"]["message"]
            .as_str()
            .unwrap()
            .contains("works for the whole server"),
        "{}",
        welcome.text()
    );
    assert_eq!(
        h.state.app.db.audit_page(guild, None, 10).await.unwrap().len(),
        0,
        "refused writes leave no trace"
    );
    assert_eq!(sqlx_count(&h, guild).await, 0);

    // Only managers write.
    let other = unique();
    let mut member = h.browser();
    member
        .login(&h, other, vec![guild_entry(guild, "Reef", false, 0)])
        .await;
    let denied = member
        .write(Method::PUT, &format!("{g}/modules/pets"), json!({ "enabled": false }))
        .await;
    assert_eq!(
        (denied.status, denied.code().as_str()),
        (StatusCode::FORBIDDEN, "not_manager")
    );
    assert_eq!(member.get(&format!("{g}/modules")).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn timed_storms_have_web_settings_with_the_command_rules() {
    let Some(h) = Harness::new("surge_settings").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let g = format!("/api/guilds/{guild}");
    let start = browser.get(&format!("{g}/community")).await.json();
    assert_eq!(start["surge"]["enabled"], false);
    assert_eq!(start["surge"]["every_hours"], 24);
    assert_eq!(start["surge"]["every_choices"], json!([6, 12, 24, 48, 168]));
    assert_eq!(start["surge"]["next_at"], Value::Null);

    for (body, field) in [
        (json!({ "every_hours": 7 }), "every_hours"),
        (json!({ "every_hours": 5_000_000_000_i64 }), "every_hours"),
        (json!({ "channel_id": LOUNGE.to_string() }), "channel_id"),
        (json!({ "channel_id": "424242" }), "channel_id"),
    ] {
        let refused = browser
            .write(Method::PUT, &format!("{g}/surge/settings"), body.clone())
            .await;
        assert_eq!(
            refused.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            refused.text()
        );
        assert_eq!(refused.json()["error"]["field"], field, "{body}");
    }
    assert_eq!(h.state.app.db.audit_page(guild, None, 10).await.unwrap().len(), 0);

    let on = browser
        .write(
            Method::PUT,
            &format!("{g}/surge/settings"),
            json!({ "enabled": true, "channel_id": NEWS.to_string(), "every_hours": 12 }),
        )
        .await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.text());
    let on = on.json();
    assert_eq!(
        (on["enabled"].as_bool(), on["every_hours"].as_i64()),
        (Some(true), Some(12))
    );
    assert_eq!(on["channel_id"], NEWS.to_string());
    assert!(on["next_at"].is_string(), "the next storm is scheduled");
    let stored = h.state.app.db.party_settings(guild).await.unwrap();
    assert_eq!((stored.surge_enabled, stored.surge_every_hours), (true, 12));
    let rows = h.state.app.db.audit_page(guild, None, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].action, "surge.set");

    // The same values again do not push the next storm back, and are not recorded.
    let next = stored.surge_next_at;
    browser
        .write(
            Method::PUT,
            &format!("{g}/surge/settings"),
            json!({ "enabled": true, "channel_id": NEWS.to_string(), "every_hours": 12 }),
        )
        .await;
    assert_eq!(h.state.app.db.party_settings(guild).await.unwrap().surge_next_at, next);
    assert_eq!(h.state.app.db.audit_page(guild, None, 10).await.unwrap().len(), 1);
}

// /Carry-over from the P2 review

#[tokio::test]
async fn a_sync_from_discord_and_one_from_the_dashboard_never_run_at_once_and_rest_between() {
    use pinchora_commands::{SyncOutcome, sync_level_roles};
    let Some(h) = Harness::new("sync_lock").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let g = format!("/api/guilds/{guild}");
    let veteran = unique();
    h.seed_xp(guild, veteran, 5_000).await;
    browser
        .write(Method::PUT, &format!("{g}/level-roles/{FAN}"), json!({ "level": 2 }))
        .await;
    let app = &h.state.app;

    // The slash command's side holds the lock: the dashboard's job starts and reads as failed.
    assert!(
        app.cache
            .try_cooldown(&format!("levelsync:{guild}"), Duration::from_secs(60))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(sync_level_roles(app, guild, |_| {}).await.unwrap(), SyncOutcome::Busy);
    let started = browser
        .write(Method::POST, &format!("{g}/level-roles/sync"), json!({}))
        .await;
    assert_eq!(started.status, StatusCode::ACCEPTED, "{}", started.text());
    let job = started.json()["job_id"].as_str().unwrap().to_owned();
    let mut state = Value::Null;
    let waited = Instant::now();
    while waited.elapsed() < Duration::from_secs(10) {
        state = browser.get(&format!("{g}/jobs/{job}")).await.json();
        if state["status"] != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(state["status"], "failed", "{state}");
    assert!(
        state["message"].as_str().unwrap().contains("already running"),
        "{state}"
    );
    assert!(h.mock.role_calls.lock().unwrap().is_empty(), "nothing ran");
    assert!(
        h.state
            .app
            .db
            .audit_page(guild, None, 10)
            .await
            .unwrap()
            .iter()
            .all(|r| r.action != "level_roles.sync"),
        "a sync that did not run is not recorded"
    );
    app.cache.release_cooldown(&format!("levelsync:{guild}")).await.unwrap();
    app.cache.release_cooldown(&format!("web:sync:{guild}")).await.unwrap();

    // Free: it runs, and then the server rests, from Discord and from the dashboard alike.
    let done = sync_level_roles(app, guild, |_| {}).await.unwrap();
    assert!(
        matches!(done, SyncOutcome::Done(report) if report.updated == 1),
        "{done:?}"
    );
    match sync_level_roles(app, guild, |_| {}).await.unwrap() {
        SyncOutcome::Resting(left) => assert!(
            left > Duration::from_secs(200) && left <= Duration::from_secs(300),
            "{left:?}"
        ),
        other => panic!("expected a rest, got {other:?}"),
    }
    let again = browser
        .write(Method::POST, &format!("{g}/level-roles/sync"), json!({}))
        .await;
    assert_eq!(
        (again.status, again.code().as_str()),
        (StatusCode::TOO_MANY_REQUESTS, "resting")
    );
    assert!(
        again.json()["error"]["message"].as_str().unwrap().contains("min"),
        "{}",
        again.text()
    );
    assert!(again.header("retry-after").parse::<u64>().unwrap() > 200);
}

#[tokio::test]
async fn a_second_sync_is_told_which_job_is_already_running() {
    let Some(h) = Harness::new("sync_conflict").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let g = format!("/api/guilds/{guild}");
    browser
        .write(Method::PUT, &format!("{g}/level-roles/{FAN}"), json!({ "level": 2 }))
        .await;
    // A job of this server is running: its lock and its id are in Redis.
    let cache = &h.state.app.cache;
    assert!(
        cache
            .try_cooldown(&format!("web:sync:{guild}"), Duration::from_secs(60))
            .await
            .unwrap()
            .is_none()
    );
    cache
        .put(&format!("web:sync-job:{guild}"), "job42", Duration::from_secs(60))
        .await
        .unwrap();
    let second = browser
        .write(Method::POST, &format!("{g}/level-roles/sync"), json!({}))
        .await;
    assert_eq!(
        (second.status, second.code().as_str()),
        (StatusCode::CONFLICT, "conflict")
    );
    assert_eq!(
        second.header("x-job-id"),
        "job42",
        "the page follows the running job instead"
    );
}

#[tokio::test]
async fn a_refresh_in_flight_cannot_bring_back_a_session_that_was_logged_out() {
    let Some(h) = Harness::new("refresh_resurrect").await else {
        return;
    };
    h.mock.short_tokens.store(true, Ordering::SeqCst);
    let user = unique();
    let mut browser = h.browser();
    browser.login(&h, user, vec![]).await;
    h.mock.short_tokens.store(false, Ordering::SeqCst);
    h.mock.token_delay_ms.store(500, Ordering::SeqCst);

    // The refresh is waiting on Discord when the user logs out.
    let mut slow = browser.clone();
    let mut leaving = browser.clone();
    let (refreshed, out) = tokio::join!(slow.write(Method::POST, "/api/me/guilds/refresh", json!({})), async {
        tokio::time::sleep(Duration::from_millis(150)).await;
        leaving.write(Method::POST, "/auth/logout", json!({})).await
    });
    assert!(
        out.status.is_success() || out.status == StatusCode::SEE_OTHER,
        "{}",
        out.text()
    );
    assert_eq!(refreshed.status, StatusCode::UNAUTHORIZED, "{}", refreshed.text());
    assert_eq!(refreshed.code(), "session_expired");
    let mut after = browser.clone();
    assert_eq!(
        after.get("/api/me").await.status,
        StatusCode::UNAUTHORIZED,
        "the logged out session stays gone"
    );
}

#[tokio::test]
async fn a_role_is_checked_against_the_bots_position_as_it_is_when_saved() {
    let Some(h) = Harness::new("live_roles").await else {
        return;
    };
    let (guild, _, mut browser) = h.managed_server().await;
    let g = format!("/api/guilds/{guild}");
    // The lists are fetched and cached with the bot above Regular.
    let lists = browser.get(&format!("{g}/discord")).await.json();
    assert_eq!(lists["bot"]["top_role_position"], BOT_ROLE_POSITION);

    // The bot's role is dragged below it a moment later; the cached lists still say it is fine.
    h.mock.bot_position.store(4, Ordering::SeqCst);
    for (method, path, body) in [
        (Method::PUT, format!("{g}/level-roles/{REGULAR}"), json!({ "level": 2 })),
        (
            Method::PUT,
            format!("{g}/rep-roles/{REGULAR}"),
            json!({ "reputation": 2 }),
        ),
        (Method::PUT, format!("{g}/store/{REGULAR}"), json!({ "price": 5 })),
        (
            Method::PATCH,
            format!("{g}/settings"),
            json!({ "join_role_id": REGULAR.to_string() }),
        ),
    ] {
        let refused = browser.write(method, &path, body).await;
        assert_eq!(
            refused.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{path}: {}",
            refused.text()
        );
        assert!(
            refused.json()["error"]["message"]
                .as_str()
                .unwrap()
                .contains("above my highest role"),
            "{path}: {}",
            refused.text()
        );
    }
    // A role below the bot's new position still saves.
    let fine = browser
        .write(Method::PUT, &format!("{g}/level-roles/{FAN}"), json!({ "level": 2 }))
        .await;
    assert_eq!(fine.status, StatusCode::OK, "{}", fine.text());
}

#[tokio::test]
async fn points_that_race_each_other_never_leave_the_least_above_the_most() {
    let Some(h) = Harness::new("points_race").await else {
        return;
    };
    let (guild, _, browser) = h.managed_server().await;
    let path = format!("/api/guilds/{guild}/settings");
    let (mut first, mut second) = (browser.clone(), browser.clone());
    first
        .write(Method::PATCH, &path, json!({ "points_min": 1, "points_max": 500 }))
        .await;
    for round in 0..8 {
        // Each patch is fine against the row the other one started from.
        let (a, b) = tokio::join!(
            first.write(Method::PATCH, &path, json!({ "points_min": 400 })),
            second.write(Method::PATCH, &path, json!({ "points_max": 100 })),
        );
        let settings = h.state.app.db.guild_settings(guild).await.unwrap();
        assert!(
            settings.points_min <= settings.points_max,
            "round {round}: {} > {} after {} and {}",
            settings.points_min,
            settings.points_max,
            a.text(),
            b.text()
        );
        for response in [&a, &b] {
            assert!(
                matches!(response.status, StatusCode::OK | StatusCode::UNPROCESSABLE_ENTITY),
                "{}",
                response.text()
            );
        }
        // Back to the starting rows: min first would be refused while the most is still 100.
        first
            .write(Method::PATCH, &path, json!({ "points_min": 1, "points_max": 500 }))
            .await;
    }
}

/// The moderation pages: the mod-log and the warn ladder, the server log's matrix, the cases with a
/// new reason, and the staff log that reads commands, Discord and the dashboard together.
#[tokio::test]
async fn moderation_logs_cases_and_the_staff_log_follow_the_command_rules() {
    use pinchora_core::moderation::CaseKind;
    let Some(h) = Harness::new("moderation").await else {
        return;
    };
    let (guild, owner, mut browser) = h.managed_server().await;
    let g = format!("/api/guilds/{guild}");

    // Moderation settings: a text channel, the ladder checked like the command checks it.
    let start = browser.get(&format!("{g}/moderation")).await.json();
    assert_eq!(start["dm_members"], true);
    assert_eq!(start["ladder"], json!([]));
    let saved = browser
        .write(
            Method::PUT,
            &format!("{g}/moderation"),
            json!({
                "log_channel_id": GENERAL.to_string(),
                "dm_members": false,
                "ladder": [
                    { "warns": 5, "action": "kick" },
                    { "warns": 3, "action": "timeout", "duration": "1h" },
                ],
            }),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let saved = saved.json();
    assert_eq!(saved["log_channel_id"], GENERAL.to_string());
    assert_eq!(saved["ladder"][0]["label"], "3 warns → 1h timeout");
    assert_eq!(saved["ladder"][1]["label"], "5 warns → kick");
    assert_eq!(h.state.app.db.warn_thresholds(guild).await.unwrap().len(), 2);
    for bad in [
        json!({ "log_channel_id": LOUNGE.to_string() }),
        json!({ "ladder": [{ "warns": 3, "action": "timeout" }] }),
        json!({ "ladder": [{ "warns": 3, "action": "kick" }, { "warns": 3, "action": "ban" }] }),
    ] {
        let refused = browser
            .write(Method::PUT, &format!("{g}/moderation"), bad.clone())
            .await;
        assert_eq!(
            refused.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}: {}",
            refused.text()
        );
    }
    let unknown = browser
        .write(Method::PUT, &format!("{g}/moderation"), json!({ "colour": 1 }))
        .await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);

    // The server log: a channel per category, single events, the ignore lists.
    let logs = browser.get(&format!("{g}/logs")).await.json();
    assert_eq!(logs["categories"].as_array().unwrap().len(), 9);
    let saved = browser
        .write(
            Method::PUT,
            &format!("{g}/logs"),
            json!({
                "channel_id": GENERAL.to_string(),
                "category_channels": { "moderation": NEWS.to_string() },
                "events": { "message_edit": false, "voice_self": true },
                "ignored_channels": [CATEGORY.to_string()],
                "ignored_roles": [FAN.to_string()],
                "ignored_users": [owner.to_string()],
                "ignore_bots": true,
            }),
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.text());
    let config = h.state.app.db.log_config(guild).await.unwrap();
    use pinchora_core::logging::{LogCategory, LogEvent};
    assert_eq!(config.default_channel, Some(GENERAL));
    assert_eq!(config.channels.get(&LogCategory::Moderation), Some(&NEWS));
    assert!(!config.enabled(LogEvent::MessageEdit) && config.enabled(LogEvent::VoiceSelf));
    assert_eq!(
        (config.ignored_channels.clone(), config.ignored_roles.clone()),
        (vec![CATEGORY], vec![FAN])
    );
    assert!(config.ignore_bots);
    assert_eq!(
        h.state.app.db.guild_settings(guild).await.unwrap().log_channel_id,
        Some(GENERAL as i64),
        "the same log channel the settings page shows"
    );
    for bad in [
        json!({ "events": { "nope": true } }),
        json!({ "category_channels": { "moderation": LOUNGE.to_string() } }),
        json!({ "ignored_roles": ["123"] }),
        json!({ "ignored_users": (1..=51).map(|n| n.to_string()).collect::<Vec<_>>() }),
    ] {
        let refused = browser.write(Method::PUT, &format!("{g}/logs"), bad.clone()).await;
        assert_eq!(
            refused.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}: {}",
            refused.text()
        );
    }
    let back = browser
        .write(
            Method::PUT,
            &format!("{g}/logs"),
            json!({ "category_channels": { "moderation": null }, "events": { "message_edit": true } }),
        )
        .await
        .json();
    assert_eq!(back["categories"][5]["key"], "moderation");
    assert_eq!(back["categories"][5]["channel_id"], Value::Null);

    // Cases: found by member and words, a new reason, the member's other cases.
    let db = &h.state.app.db;
    let target = unique();
    for (kind, reason) in [(CaseKind::Warn, "spam"), (CaseKind::Ban, "raid")] {
        db.open_case(&pinchora_db::NewCase {
            guild,
            kind,
            target,
            target_name: "Mia",
            moderator: 9,
            moderator_name: "Mod",
            reason: Some(reason),
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap();
    }
    let list = browser.get(&format!("{g}/cases")).await.json();
    assert_eq!(list["items"].as_array().unwrap().len(), 2);
    assert_eq!(list["items"][0]["label"], "Ban");
    let found = browser
        .get(&format!("{g}/cases?q=spam&kind=warn&member={target}"))
        .await
        .json();
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    let bad_kind = browser.get(&format!("{g}/cases?kind=mute")).await;
    assert_eq!(bad_kind.status, StatusCode::BAD_REQUEST);
    let detail = browser.get(&format!("{g}/cases/2")).await.json();
    assert_eq!(detail["case"]["reason"], "raid");
    assert_eq!(detail["history"].as_array().unwrap().len(), 1, "the warn");
    assert_eq!(
        browser.get(&format!("{g}/cases/99")).await.status,
        StatusCode::NOT_FOUND
    );
    let edited = browser
        .write(
            Method::PATCH,
            &format!("{g}/cases/2"),
            json!({ "reason": "raid with alts" }),
        )
        .await;
    assert_eq!(edited.json()["reason"], "raid with alts");
    let long = browser
        .write(
            Method::PATCH,
            &format!("{g}/cases/2"),
            json!({ "reason": "x".repeat(513) }),
        )
        .await;
    assert_eq!(long.status, StatusCode::UNPROCESSABLE_ENTITY);

    // The staff log: the commands' cases, the new reason from here, and the dashboard's own changes.
    let staff = browser.get(&format!("{g}/staff-log")).await.json();
    let sources: Vec<&str> = staff["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["source"].as_str().unwrap())
        .collect();
    assert!(sources.contains(&"command") && sources.contains(&"web"), "{sources:?}");
    let reasons = browser.get(&format!("{g}/staff-log?q=alts")).await.json();
    assert_eq!(reasons["items"][0]["action"], "reason");
    assert_eq!(reasons["items"][0]["actor_id"], owner.to_string());
    let bad_source = browser.get(&format!("{g}/staff-log?source=mail")).await;
    assert_eq!(bad_source.status, StatusCode::BAD_REQUEST);
    let actions: Vec<String> = db
        .audit_page(guild, None, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.action)
        .collect();
    assert_eq!(actions, ["logs.set", "logs.set", "moderation.set"]);
}

/// A folder shaped like `web/dist`: the app shell, the public pages the build writes out, the text files that
/// name the site's address, and a hashed asset.
fn site_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pinchora-site-{name}-{}", unique()));
    std::fs::create_dir_all(dir.join("commands")).expect("create the site folder");
    std::fs::create_dir_all(dir.join("assets")).expect("create the assets folder");
    let write = |file: &str, text: &str| std::fs::write(dir.join(file), text).expect("write a site file");
    write(
        "index.html",
        "<!doctype html><title>Front page</title><link rel=\"canonical\" href=\"__ORIGIN__/\">",
    );
    write(
        "commands/index.html",
        "<!doctype html><title>Commands</title><link rel=\"canonical\" href=\"__ORIGIN__/commands\">",
    );
    write("robots.txt", "User-agent: *\nSitemap: __ORIGIN__/sitemap.xml\n");
    write(
        "sitemap.xml",
        "<urlset><url><loc>__ORIGIN__/commands</loc></url></urlset>",
    );
    write("assets/app-abc123.js", "console.log('app')");
    write("og.png", "not really a png");
    dir
}

#[tokio::test]
async fn public_pages_are_pages_of_their_own_with_the_sites_address_in_them() {
    let dir = site_dir("pages");
    let folder = dir.clone();
    let Some(h) = Harness::with("public_pages", move |web| web.static_dir = Some(folder)).await else {
        return;
    };
    let mut browser = h.browser();

    let front = browser.get("/").await;
    assert_eq!(front.status, StatusCode::OK);
    assert!(front.text().contains("<title>Front page</title>"));
    assert!(
        front.text().contains(&format!("href=\"{PUBLIC_URL}/\"")) && !front.text().contains("__ORIGIN__"),
        "the placeholder becomes the public URL: {}",
        front.text()
    );
    assert_eq!(front.header("cache-control"), "no-cache");
    assert_eq!(front.header("x-robots-tag"), "", "the front page is for search engines");

    for path in ["/commands", "/commands/"] {
        let page = browser.get(path).await;
        assert_eq!(page.status, StatusCode::OK, "{path}");
        assert!(
            page.text().contains("<title>Commands</title>"),
            "{path}: its own page, not the shell"
        );
        assert!(page.text().contains(&format!("{PUBLIC_URL}/commands")), "{path}");
        assert_eq!(page.header("x-robots-tag"), "", "{path}");
    }

    let shell = browser.get("/servers").await;
    assert!(shell.text().contains("Front page"), "dashboard pages get the shell");
    assert_eq!(
        shell.header("x-robots-tag"),
        "noindex, nofollow",
        "and are not for search engines"
    );
    assert_eq!(
        browser.get("/nothing-here").await.header("x-robots-tag"),
        "noindex, nofollow"
    );

    let robots = browser.get("/robots.txt").await;
    assert_eq!(robots.status, StatusCode::OK);
    assert_eq!(robots.header("content-type"), "text/plain; charset=utf-8");
    assert_eq!(
        robots.text(),
        format!("User-agent: *\nSitemap: {PUBLIC_URL}/sitemap.xml\n")
    );
    let sitemap = browser.get("/sitemap.xml").await;
    assert_eq!(sitemap.header("content-type"), "application/xml; charset=utf-8");
    assert!(sitemap.text().contains(&format!("<loc>{PUBLIC_URL}/commands</loc>")));

    let script = browser.get("/assets/app-abc123.js").await;
    assert_eq!(script.header("cache-control"), "public, max-age=31536000, immutable");
    assert_eq!(
        browser.get("/og.png").await.body.as_ref(),
        b"not really a png",
        "other files are served as they are"
    );
    assert_eq!(browser.get("/commands/missing.png").await.status, StatusCode::NOT_FOUND);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn the_front_page_works_before_login_is_set_up() {
    let dir = site_dir("unconfigured");
    let folder = dir.clone();
    let Some(h) = Harness::with("public_pages_unconfigured", move |web| {
        web.static_dir = Some(folder);
        web.client_secret = None;
    })
    .await
    else {
        return;
    };
    let mut browser = h.browser();
    assert!(browser.get("/").await.text().contains("<title>Front page</title>"));
    assert!(
        browser
            .get("/commands")
            .await
            .text()
            .contains("<title>Commands</title>")
    );
    assert!(
        browser
            .get("/servers")
            .await
            .text()
            .contains("PINCHORA_WEB__CLIENT_SECRET"),
        "the dashboard still says what is missing"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn the_invite_link_sends_visitors_to_discord_with_the_bots_permissions() {
    let Some(h) = Harness::new("invite").await else {
        return;
    };
    let mut browser = h.browser();
    let response = browser.get("/invite").await;
    assert_eq!(response.status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(response.header("cache-control"), "no-cache");
    let location = Url::parse(response.header("location")).expect("an absolute URL");
    assert_eq!(location.path(), "/oauth2/authorize");
    let query: HashMap<String, String> = location.query_pairs().into_owned().collect();
    assert_eq!(query["client_id"], APPLICATION_ID.to_string());
    assert_eq!(query["scope"], "bot applications.commands");
    let permissions: u64 = query["permissions"].parse().expect("permission bits");
    assert!(permissions & MANAGE_ROLES != 0 && permissions & SEND_MESSAGES != 0);
    assert!(
        !query.contains_key("guild_id"),
        "the visitor picks the server on Discord's page"
    );
}

#[tokio::test]
async fn the_landing_data_is_public_cached_and_knows_who_is_looking() {
    let Some(h) = Harness::new("landing_data").await else {
        return;
    };
    let cache = &h.state.app.cache;
    cache.forget("web:landing-counts").await.expect("forget the totals");
    let mut browser = h.browser();

    let first = browser.get("/api/public/landing").await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    assert_eq!(first.header("cache-control"), "no-store");
    let body = first.json();
    assert!(body["viewer"].is_null(), "nobody is logged in");
    for key in ["servers", "players", "houses", "pets"] {
        assert!(body["stats"][key].as_i64().is_some_and(|n| n >= 0), "{key}: {body}");
    }

    // A new player does not show until the totals are counted again.
    let id = (unique() >> 2) as i64;
    sqlx::query("INSERT INTO users (id) VALUES ($1) ON CONFLICT (id) DO NOTHING")
        .bind(id)
        .execute(h.state.app.db.pool())
        .await
        .expect("add a player");
    let second = browser.get("/api/public/landing").await.json();
    assert_eq!(second["stats"], body["stats"], "the totals are cached");
    cache.forget("web:landing-counts").await.expect("forget the totals");
    let third = browser.get("/api/public/landing").await.json();
    assert!(third["stats"]["players"].as_i64() > Some(0), "counted again: {third}");

    browser.login(&h, 7_001, vec![]).await;
    let signed_in = browser.get("/api/public/landing").await.json();
    assert_eq!(signed_in["viewer"]["name"], "user7001 G");
    assert!(
        signed_in["viewer"]["avatar_url"]
            .as_str()
            .is_some_and(|url| url.starts_with("https://cdn.discordapp.com/")),
        "{signed_in}"
    );
    assert!(
        signed_in["viewer"].get("csrf").is_none(),
        "nothing secret is handed out"
    );

    // Past the limit it says to wait: the window may roll over once, so twice the limit is sent.
    let mut flood = h.browser();
    let mut refused = 0;
    for _ in 0..125 {
        if flood.get("/api/public/landing").await.status == StatusCode::TOO_MANY_REQUESTS {
            refused += 1;
        }
    }
    assert!(refused > 0, "an address that asks 125 times a minute is slowed down");
}

#[tokio::test]
async fn case_reason_edits_require_the_author_or_a_higher_moderator() {
    use pinchora_core::moderation::CaseKind;
    let Some(h) = Harness::new("reason_hierarchy").await else {
        return;
    };
    let (guild, junior, senior, target) = (unique(), unique(), unique(), unique());
    h.mock.bot_guilds.lock().unwrap().insert(guild);
    h.mock
        .member_roles
        .lock()
        .unwrap()
        .extend([(junior, vec![FAN]), (senior, vec![REGULAR]), (target, vec![REGULAR])]);
    let mut browser = h.browser();
    browser
        .login(
            &h,
            junior,
            vec![guild_entry(guild, "Reef", false, MANAGE_GUILD | ADMINISTRATOR)],
        )
        .await;
    let db = &h.state.app.db;
    for moderator in [senior, junior, APPLICATION_ID, 0] {
        db.open_case(&pinchora_db::NewCase {
            guild,
            kind: CaseKind::Warn,
            target,
            target_name: "Mia",
            moderator,
            moderator_name: "Mod",
            reason: Some("original"),
            duration: None,
            expires_at: None,
            details: None,
        })
        .await
        .unwrap();
    }
    let path = |number| format!("/api/guilds/{guild}/cases/{number}");
    for number in [1, 3, 4] {
        let denied = browser
            .write(Method::PATCH, &path(number), json!({"reason": "bypass"}))
            .await;
        assert_eq!(denied.status, StatusCode::FORBIDDEN, "{}", denied.json());
        assert_eq!(denied.json()["error"]["code"], "hierarchy");
        assert_eq!(
            db.case(guild, number).await.unwrap().unwrap().reason.as_deref(),
            Some("original")
        );
    }
    let own = browser.write(Method::PATCH, &path(2), json!({"reason": "own"})).await;
    assert_eq!(own.status, StatusCode::OK);
    h.mock.member_roles.lock().unwrap().insert(junior, vec![REGULAR]);
    assert_eq!(
        browser
            .write(Method::PATCH, &path(1), json!({"reason": "peer"}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    h.mock.member_roles.lock().unwrap().insert(junior, vec![ADMIN]);
    for number in [1, 3, 4] {
        let higher = browser
            .write(Method::PATCH, &path(number), json!({"reason": "higher"}))
            .await;
        assert_eq!(higher.status, StatusCode::OK, "{}", higher.json());
    }
}

#[path = "api/command_control.rs"]
mod command_control;

#[tokio::test]
async fn public_bot_status_has_safe_cached_numbers() {
    let Some(h) = Harness::new("public_bot_status").await else {
        return;
    };
    h.state.app.cache.forget("public:status:v1").await.unwrap();
    let mut browser = h.browser();
    let first = browser.get("/api/public/status").await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text());
    let body = first.json();
    for key in [
        "servers",
        "approximate_members",
        "players",
        "rss_mb",
        "postgres_mb",
        "redis_mb",
        "commands_24h",
        "active_games",
        "open_tickets",
    ] {
        assert!(body[key].as_u64().is_some(), "{key}: {body}");
    }
    assert!(body["shards"].is_array());
    assert!(body["status_url"].as_str().unwrap().ends_with("/status"));
    let raw = body.to_string().to_ascii_lowercase();
    assert!(!raw.contains("postgres://") && !raw.contains("redis://") && !raw.contains("token"));
    assert_eq!(browser.get("/api/public/status").await.json(), body);
}
