//! Drives the dashboard like a browser would, with Discord replaced by a local mock.
//!
//! The mock answers the OAuth token endpoint, the user-token routes (`/users/@me`, its guilds) and,
//! through twilight's proxy setting, the bot-token routes. A `Browser` keeps cookies and the CSRF
//! token and sends requests straight into the router.
//!
//! Needs `PINCHORA_TEST_DATABASE_URL` and `PINCHORA_TEST_REDIS_URL`; skipped when either is unset.

use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
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
use pinchora_commands::{App, listen_for_invalidations, on_message};
use pinchora_core::config::WebConfig;
use pinchora_db::Db;
use pinchora_web::{WebSettings, WebState};
use reqwest::Url;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tower::ServiceExt;
use twilight_http::Client;
use twilight_model::{gateway::payload::incoming::MessageCreate, id::Id};

const APPLICATION_ID: u64 = 1;
const PUBLIC_URL: &str = "http://dash.test";
const MANAGE_GUILD: u64 = 1 << 5;
const ADMINISTRATOR: u64 = 1 << 3;
const SEND_MESSAGES: u64 = 1 << 11;

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
    /// Forms posted to the token endpoint, oldest first.
    token_forms: Arc<Mutex<Vec<HashMap<String, String>>>>,
    /// Tokens revoked at logout.
    revoked: Arc<Mutex<Vec<String>>>,
    /// Hand out tokens that are about to expire, so the next use refreshes them.
    short_tokens: Arc<AtomicBool>,
    /// Refuse refresh tokens, as Discord does once the user removed the app.
    refuse_refresh: Arc<AtomicBool>,
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
async fn discord(State(mock): State<Mock>, method: Method, uri: Uri, headers: HeaderMap) -> (StatusCode, String) {
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
        (&Method::GET, [.., "guilds", guild]) => {
            let id = guild.parse::<u64>().unwrap_or(0);
            if mock.bot_guilds.lock().unwrap().contains(&id) {
                (StatusCode::OK, guild_json(id).to_string())
            } else {
                let gone = json!({ "message": "Unknown Guild", "code": 10004 });
                (StatusCode::NOT_FOUND, gone.to_string())
            }
        }
        (&Method::POST, [.., "channels", channel, "messages"]) => {
            let channel = channel.parse::<u64>().unwrap_or(1);
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
            address: SocketAddr::from(([10, 0, (unique() % 250) as u8, (unique() % 250) as u8], 4000)),
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

    /// Another pod of the bot on the same databases, as the gateway would run it.
    async fn bot_pod(&self) -> Arc<App> {
        app(&self.database, &self.redis, self.mock_addr).await
    }
}

/// The real [`App`] on the test databases, its REST client pointed at the mock.
async fn app(database: &str, redis: &str, mock: SocketAddr) -> Arc<App> {
    let db = Db::connect(database, 5).await.expect("connect to the database");
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

/// A chat message from `user` in `guild`, as the gateway delivers it.
fn chat(guild: u64, user: u64) -> MessageCreate {
    serde_json::from_value(json!({
        "id": unique().to_string(),
        "channel_id": "2",
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
    let page = browser.get("/").await;
    assert_eq!(page.status, StatusCode::OK);
    let html = page.text();
    assert!(html.contains("PINCHORA_WEB__CLIENT_SECRET"), "{html}");
    assert!(html.contains(&redirect_uri()), "the redirect to register is shown");
    assert_eq!(browser.get("/servers").await.text(), html, "every app page shows it");
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
