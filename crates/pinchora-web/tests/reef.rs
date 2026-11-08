//! Real HTTP upgrades and multiple WebSocket clients against the assigned Postgres/Redis.
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use pinchora_cache::{Cache, Invalidation};
use pinchora_commands::App;
use pinchora_core::config::WebConfig;
use pinchora_db::{Db, Hide, NewRoom};
use pinchora_game::{Content, character::Character};
use pinchora_web::{WebSettings, WebState};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use twilight_http::Client;
use twilight_model::id::Id;
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
struct Site {
    app: Arc<App>,
    state: WebState,
    pool: PgPool,
    url: String,
    server: JoinHandle<()>,
}
impl Drop for Site {
    fn drop(&mut self) {
        self.state.drain_reef();
        self.server.abort();
    }
}
fn unique() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    (SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64)
        * 100
        + NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 100
}
impl Site {
    async fn new() -> Option<Self> {
        Self::with_redis(None).await
    }
    async fn with_redis(redis_override: Option<&str>) -> Option<Self> {
        let (Ok(database), Ok(redis)) = (
            std::env::var("PINCHORA_TEST_DATABASE_URL"),
            std::env::var("PINCHORA_TEST_REDIS_URL"),
        ) else {
            eprintln!("set test database and Redis URLs");
            return None;
        };
        let db = Db::connect(&database, 5).await.expect("test Postgres reachable");
        db.migrate().await.unwrap();
        let cache = Cache::connect(redis_override.unwrap_or(&redis))
            .await
            .expect("test Redis reachable");
        let rest = Arc::new(
            Client::builder()
                .token("Bot reef-test".into())
                .proxy("127.0.0.1:9".into(), true)
                .ratelimiter(None)
                .build(),
        );
        let app = Arc::new(App::new(rest.clone(), rest, Id::new(1), db, cache, "reef-test".into()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}");
        let config = WebConfig {
            public_url: url.clone(),
            operator_ids: "770000000000000099".into(),
            bind: addr,
            ..WebConfig::default()
        };
        let state = WebState::new(app.clone(), WebSettings::resolve(&config, 1).unwrap());
        let server_state = state.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                pinchora_web::router(server_state).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap();
        });
        Some(Self {
            app,
            state,
            pool: PgPool::connect(&database).await.unwrap(),
            url,
            server,
        })
    }
    async fn house(&self, user: u64) {
        let content = Content::get();
        self.app
            .db
            .house(
                user,
                &NewRoom {
                    room_type: "living".into(),
                    wallpaper: content.surfaces.wallpapers[0].id.clone(),
                    floor: content.surfaces.floors[0].id.clone(),
                },
            )
            .await
            .unwrap();
    }
    async fn browser(&self, user: u64) -> Browser {
        self.app
            .db
            .set_character(user, &Character::starter(Content::get()).encode())
            .await
            .unwrap();
        let sid = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
        let now = chrono::Utc::now().timestamp();
        self.app
            .cache
            .put(
                &format!("web:sess:{sid}"),
                &json!({
                    "user_id":user,"username":format!("player{user}"),"global_name":null,"avatar":null,
                    "access_token":"test","refresh_token":null,"token_expires_at":now+3600,
                    "csrf":"reef-test-csrf","created_at":now,"last_seen_at":now,"dev_guilds":[]
                })
                .to_string(),
                Duration::from_secs(600),
            )
            .await
            .unwrap();
        Browser {
            url: self.url.clone(),
            sid,
        }
    }
}
struct Browser {
    url: String,
    sid: String,
}
impl Browser {
    async fn ticket(&self) -> String {
        let response = reqwest::Client::new()
            .post(format!("{}/api/reef/socket-ticket", self.url))
            .header("cookie", format!("pinchora_sid={}", self.sid))
            .header("origin", &self.url)
            .header("x-csrf-token", "reef-test-csrf")
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{}", response.status());
        response.json::<Value>().await.unwrap()["ticket"]
            .as_str()
            .unwrap()
            .into()
    }
    async fn raw(&self, origin: Option<&str>) -> Result<Socket, tokio_tungstenite::tungstenite::Error> {
        let mut request = format!("{}/api/reef/ws", self.url.replace("http:", "ws:"))
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("cookie", format!("pinchora_sid={}", self.sid).parse().unwrap());
        if let Some(origin) = origin {
            request.headers_mut().insert("origin", origin.parse().unwrap());
        }
        connect_async(request).await.map(|(socket, _)| socket)
    }
    async fn connect(&self) -> Player {
        let ticket = self.ticket().await;
        let socket = self.raw(Some(&self.url)).await.unwrap();
        let mut player = Player {
            socket,
            sequence: 0,
            epoch: String::new(),
            revision: String::new(),
        };
        player.send(json!({"type":"authenticate","ticket":ticket})).await;
        player.until("authenticated").await;
        player
    }
}
struct Player {
    socket: Socket,
    sequence: u64,
    epoch: String,
    revision: String,
}
impl Player {
    async fn send(&mut self, mut body: Value) {
        self.sequence += 1;
        body["protocol_version"] = json!(1);
        body["sequence"] = json!(self.sequence);
        body["request_id"] = json!(self.sequence.to_string());
        body["room_epoch"] = json!(self.epoch);
        self.socket.send(Message::Text(body.to_string().into())).await.unwrap();
    }
    async fn until(&mut self, kind: &str) -> Value {
        self.until_any(&[kind]).await
    }
    async fn until_any(&mut self, kinds: &[&str]) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let message = self.socket.next().await.expect("open socket").expect("valid frame");
                if let Message::Text(raw) = message {
                    let mut value: Value = serde_json::from_str(&raw).unwrap();
                    if value["type"] == "correction" {
                        value["players"] = json!(value["updates"].as_array().unwrap().iter().map(|p|
                            json!({"user_id":p[0],"position":[p[1],p[2]],"tile":[p[3],p[4]],"direction":p[5],"state":p[6]})
                        ).collect::<Vec<_>>());
                    }
                    if value["type"] == "ping" {
                        self.send(json!({"type":"ping"})).await;
                    }
                    if kinds.iter().any(|kind| value["type"] == *kind) {
                        if value["type"] == "snapshot" {
                            self.epoch = value["room_epoch"].as_str().unwrap().into();
                            self.revision = value["revision"].as_str().unwrap().into();
                        }
                        return value;
                    }
                } else if let Message::Close(reason) = message {
                    panic!("unexpected close: {reason:?}");
                }
            }
        })
        .await
        .expect("expected protocol message")
    }
    async fn join(&mut self, owner: u64) -> Value {
        self.send(json!({"type":"join","room":format!("house:{owner}:0")}))
            .await;
        self.until("snapshot").await
    }
    async fn close_reason(&mut self) -> String {
        tokio::time::timeout(Duration::from_secs(6), async {
            loop {
                match self.socket.next().await {
                    Some(Ok(Message::Close(Some(frame)))) => return frame.reason.to_string(),
                    Some(Ok(_)) => {}
                    other => panic!("expected reason: {other:?}"),
                }
            }
        })
        .await
        .unwrap()
    }
}
#[tokio::test]
async fn two_players_walk_seat_contention_and_layout_refresh() {
    let Some(site) = Site::new().await else {
        return;
    };
    let owner = unique();
    let guest = unique();
    site.house(owner).await;
    let owner_id = owner.to_string();
    let guest_id = guest.to_string();
    let stool = Content::get()
        .furniture
        .iter()
        .find(|b| b.id == "pebblerest_stool")
        .unwrap();
    let item = format!("{}__{}", stool.id, stool.variants[0]);
    let (seat,): (i64,) = sqlx::query_as(
        "INSERT INTO placements(user_id,room_idx,item_id,x,y,flipped) VALUES($1,0,$2,1,6,false) RETURNING id",
    )
    .bind(owner as i64)
    .bind(item)
    .fetch_one(&site.pool)
    .await
    .unwrap();
    let mut a = site.browser(owner).await.connect().await;
    let mut b = site.browser(guest).await.connect().await;
    assert_eq!(a.join(owner).await["players"].as_array().unwrap().len(), 1);
    assert_eq!(b.join(owner).await["players"].as_array().unwrap().len(), 2);
    loop {
        if a.until("presence").await["player"]["user_id"] == guest_id {
            break;
        }
    }
    a.send(json!({"type":"move_to","tile":[1,7]})).await;
    let path = a.until("path").await;
    assert_eq!(path["path"], json!([[0, 7], [1, 7]]));
    assert_eq!(b.until("path").await["user_id"], owner.to_string());
    loop {
        let update = a.until("correction").await;
        if update["players"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["user_id"] == owner_id && p["state"] == "idle" && p["tile"] == json!([1, 7]))
        {
            break;
        }
    }
    a.send(json!({"type":"use","placement_id":seat.to_string(),"revision":a.revision}))
        .await;
    assert_eq!(a.until("posture").await["player"]["state"], "sit");
    b.send(json!({"type":"move_to","tile":[1,7]})).await;
    b.until("path").await;
    loop {
        if b.until("correction").await["players"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["user_id"] == guest_id && p["state"] == "idle" && p["tile"] == json!([1, 7]))
        {
            break;
        }
    }
    b.send(json!({"type":"use","placement_id":seat.to_string(),"revision":b.revision}))
        .await;
    assert_eq!(b.until("error").await["reason"], "seat_occupied");
    a.send(json!({"type":"stand"})).await;
    a.until("posture").await;
    tokio::time::sleep(Duration::from_millis(510)).await;
    b.send(json!({"type":"use","placement_id":seat.to_string(),"revision":b.revision}))
        .await;
    loop {
        if b.until("posture").await["player"]["user_id"] == guest_id {
            break;
        }
    }
    sqlx::query("DELETE FROM placements WHERE id=$1")
        .bind(seat)
        .execute(&site.pool)
        .await
        .unwrap();
    site.app
        .cache
        .publish_invalidation(Invalidation::House(owner))
        .await
        .unwrap();
    a.until("layout_changed").await;
    let snapshot = a.until("snapshot").await;
    assert!(
        snapshot["players"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["state"] == "idle")
    );
    a.send(json!({"type":"move_to","tile":[99,99]})).await;
    assert_eq!(a.until("error").await["reason"], "unreachable");
    b.send(json!({"type":"leave"})).await;
    assert_eq!(a.until("presence").await["action"], "leave");
}
#[tokio::test]
async fn tickets_are_single_use_session_bound_and_csrf_protected() {
    let Some(site) = Site::new().await else {
        return;
    };
    let a = site.browser(unique()).await;
    let b = site.browser(unique()).await;
    let denied = reqwest::Client::new()
        .post(format!("{}/api/reef/socket-ticket", site.url))
        .header("cookie", format!("pinchora_sid={}", a.sid))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    let profile = reqwest::Client::new()
        .get(format!("{}/api/reef/bootstrap", site.url))
        .header("cookie", format!("pinchora_sid={}", a.sid))
        .send()
        .await
        .unwrap();
    assert_eq!(profile.status(), 200);
    let profile = profile.json::<Value>().await.unwrap();
    assert_eq!(profile["character"], Character::starter(Content::get()).encode());
    assert_eq!(profile["chat_enabled"], false);
    assert_eq!(
        reqwest::get(format!("{}/api/reef/bootstrap", site.url))
            .await
            .unwrap()
            .status(),
        401
    );
    let ticket = a.ticket().await;
    let mut wrong = Player {
        socket: b.raw(Some(&site.url)).await.unwrap(),
        sequence: 0,
        epoch: String::new(),
        revision: String::new(),
    };
    wrong.send(json!({"type":"authenticate","ticket":ticket})).await;
    assert_eq!(wrong.close_reason().await, "authentication_failed");
    let mut replay = Player {
        socket: a.raw(Some(&site.url)).await.unwrap(),
        sequence: 0,
        epoch: String::new(),
        revision: String::new(),
    };
    replay.send(json!({"type":"authenticate","ticket":ticket})).await;
    assert_eq!(replay.close_reason().await, "authentication_failed");
    let ticket = a.ticket().await;
    let mut first = Player {
        socket: a.raw(Some(&site.url)).await.unwrap(),
        sequence: 0,
        epoch: String::new(),
        revision: String::new(),
    };
    first.send(json!({"type":"authenticate","ticket":ticket})).await;
    first.until("authenticated").await;
    let mut replay = Player {
        socket: a.raw(Some(&site.url)).await.unwrap(),
        sequence: 0,
        epoch: String::new(),
        revision: String::new(),
    };
    replay.send(json!({"type":"authenticate","ticket":ticket})).await;
    assert_eq!(replay.close_reason().await, "authentication_failed");
}
#[tokio::test]
async fn upgrade_requires_exact_origin_and_authentication_timeout() {
    let Some(site) = Site::new().await else {
        return;
    };
    let browser = site.browser(unique()).await;
    for origin in [None, Some("null"), Some("https://evil.test")] {
        let error = browser.raw(origin).await.unwrap_err();
        let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
            panic!("expected forbidden");
        };
        assert_eq!(response.status(), 403);
    }
    let mut idle = Player {
        socket: browser.raw(Some(&site.url)).await.unwrap(),
        sequence: 0,
        epoch: String::new(),
        revision: String::new(),
    };
    assert_eq!(idle.close_reason().await, "authentication_failed");
}
#[tokio::test]
async fn capacity_and_controller_replacement() {
    let Some(site) = Site::new().await else {
        return;
    };
    let owner = unique();
    site.house(owner).await;
    let browser = site.browser(owner).await;
    let mut first = browser.connect().await;
    first.join(owner).await;
    let mut players = vec![];
    for _ in 0..15 {
        let mut p = site.browser(unique()).await.connect().await;
        p.join(owner).await;
        players.push(p);
    }
    let mut extra = site.browser(unique()).await.connect().await;
    extra
        .send(json!({"type":"join","room":format!("house:{owner}:0")}))
        .await;
    assert_eq!(extra.until("error").await["reason"], "room_full");
    let mut replacement = browser.connect().await;
    assert_eq!(first.close_reason().await, "controller_replaced");
    assert_eq!(replacement.join(owner).await["players"].as_array().unwrap().len(), 16);
}
#[tokio::test]
async fn replaced_socket_stays_closed_while_new_controller_remains_active() {
    let Some(site) = Site::new().await else {
        return;
    };
    let owner = unique();
    let guest = unique();
    site.house(owner).await;
    let browser = site.browser(guest).await;
    let mut old = browser.connect().await;
    old.join(owner).await;
    let mut replacement = browser.connect().await;
    assert_eq!(old.close_reason().await, "controller_replaced");
    let snapshot = replacement.join(owner).await;
    assert_eq!(snapshot["players"].as_array().unwrap().len(), 1);
    // The previous transport has no way to regain control; a new connection needs a new ticket.
    assert!(old.socket.send(Message::Text("{}".into())).await.is_err());
    replacement.send(json!({"type":"snapshot"})).await;
    assert_eq!(
        replacement.until("snapshot").await["players"].as_array().unwrap().len(),
        1
    );
}
#[tokio::test]
async fn hidden_locked_and_blocked_houses_refuse_visitors_but_allow_owner() {
    let Some(site) = Site::new().await else {
        return;
    };
    let owner = unique();
    let guest = unique();
    site.house(owner).await;
    site.app.db.set_privacy(owner, Hide::House, true).await.unwrap();
    let mut visitor = site.browser(guest).await.connect().await;
    visitor
        .send(json!({"type":"join","room":format!("house:{owner}:0")}))
        .await;
    assert_eq!(visitor.until("error").await["reason"], "access_denied");
    let mut host = site.browser(owner).await.connect().await;
    host.join(owner).await;
    site.app.db.set_privacy(owner, Hide::House, false).await.unwrap();
    site.app.db.set_house_locked(owner, true).await.unwrap();
    visitor
        .send(json!({"type":"join","room":format!("house:{owner}:0")}))
        .await;
    assert_eq!(visitor.until("error").await["reason"], "access_denied");
    site.app.db.set_house_locked(owner, false).await.unwrap();
    site.app.db.block_signer(owner, guest, "guest").await.unwrap();
    visitor
        .send(json!({"type":"join","room":format!("house:{owner}:0")}))
        .await;
    assert_eq!(visitor.until("error").await["reason"], "access_denied");
}
#[tokio::test]
async fn rejects_oversized_messages_and_replayed_sequences() {
    let Some(site) = Site::new().await else {
        return;
    };
    let browser = site.browser(unique()).await;
    let mut player = browser.connect().await;
    player.sequence = 0;
    player.send(json!({"type":"ping"})).await;
    assert_eq!(player.close_reason().await, "invalid_envelope");
    let mut player = browser.connect().await;
    player
        .socket
        .send(Message::Text("x".repeat(4097).into()))
        .await
        .unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match player.socket.next().await {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                _ => {}
            }
        }
    })
    .await;
    assert!(closed.is_ok());
}

/// Stable accounts for the real two-context Playwright journey.
#[tokio::test]
#[ignore]
async fn seed_reef_multiplayer() {
    let site = Site::new().await.expect("test services");
    let owner = 770000000000000091u64;
    let guest = 770000000000000092u64;
    let content = Content::get();
    sqlx::query("DELETE FROM reef_sanctions WHERE user_id IN ($1,$2)")
        .bind(owner as i64)
        .bind(guest as i64)
        .execute(&site.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM guestbook_blocks WHERE host_id IN ($1,$2) OR blocked_id IN ($1,$2)")
        .bind(owner as i64)
        .bind(guest as i64)
        .execute(&site.pool)
        .await
        .unwrap();
    for (index, user) in [owner, guest].into_iter().enumerate() {
        site.house(user).await;
        let mut character = Character::starter(content);
        character.look = content.looks[index].id.clone();
        site.app.db.set_character(user, &character.encode()).await.unwrap();
        site.app.db.set_privacy(user, Hide::House, false).await.unwrap();
        site.app.db.set_house_locked(user, false).await.unwrap();
        sqlx::query("DELETE FROM placements WHERE user_id=$1")
            .bind(user as i64)
            .execute(&site.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE houses SET tier=1 WHERE user_id=$1")
            .bind(user as i64)
            .execute(&site.pool)
            .await
            .unwrap();
    }
    let stool = content.furniture.iter().find(|b| b.id == "pebblerest_stool").unwrap();
    sqlx::query("INSERT INTO placements(user_id,room_idx,item_id,x,y,flipped) VALUES($1,0,$2,1,6,false)")
        .bind(owner as i64)
        .bind(format!("{}__{}", stool.id, stool.variants[0]))
        .execute(&site.pool)
        .await
        .unwrap();
}

impl Browser {
    async fn api(&self, method: reqwest::Method, path: &str, body: Value) -> reqwest::Response {
        reqwest::Client::new()
            .request(method, format!("{}{}", self.url, path))
            .header("cookie", format!("pinchora_sid={}", self.sid))
            .header("origin", &self.url)
            .header("x-csrf-token", "reef-test-csrf")
            .json(&body)
            .send()
            .await
            .unwrap()
    }
    async fn consent(&self) {
        assert!(
            self.api(
                reqwest::Method::PUT,
                "/api/reef/consent",
                json!({"version":1,"age_band":"13_plus","accepted":true})
            )
            .await
            .status()
            .is_success()
        );
    }
}
#[tokio::test]
async fn chat_eligibility_report_queue_action_appeal_and_audit() {
    let Some(site) = Site::new().await else { return };
    let host = unique();
    let guest = unique();
    site.house(host).await;
    let a = site.browser(host).await;
    let b = site.browser(guest).await;
    let mut p = a.connect().await;
    let mut q = b.connect().await;
    p.join(host).await;
    q.join(host).await;
    q.send(json!({"type":"chat","text":"Before consent"})).await;
    assert_eq!(q.until("error").await["reason"], "eligibility_required");
    a.consent().await;
    b.consent().await;
    q.send(json!({"type":"chat","text":"Merhaba dünya"})).await;
    let sent = q.until("chat").await;
    let received = p.until("chat").await;
    assert_eq!(sent["message_id"], received["message_id"]);
    assert_eq!(sent["request_id"], q.sequence.to_string());
    let response = a
        .api(
            reqwest::Method::POST,
            "/api/reef/reports",
            json!({"message_id":received["message_id"],"reason":"harassment"}),
        )
        .await;
    assert_eq!(response.status(), 200);
    let receipt = response.json::<Value>().await.unwrap()["receipt"].clone();
    let stranger = site.browser(unique()).await;
    assert_eq!(
        stranger
            .api(
                reqwest::Method::POST,
                "/api/reef/reports",
                json!({"message_id":received["message_id"],"reason":"other"})
            )
            .await
            .status(),
        403
    );
    let queue = a
        .api(reqwest::Method::GET, "/api/reef/moderation", json!(null))
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert!(
        queue["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == receipt && item["text"] == "Merhaba dünya")
    );
    assert!(
        stranger
            .api(reqwest::Method::GET, "/api/reef/moderation", json!(null))
            .await
            .json::<Value>()
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let action = json!({"target":guest.to_string(),"owner":host.to_string(),"room_idx":null,"kind":"mute","seconds":3600,"report":receipt});
    assert_eq!(
        b.api(reqwest::Method::POST, "/api/reef/moderation", action.clone())
            .await
            .status(),
        403
    );
    assert_eq!(
        a.api(reqwest::Method::POST, "/api/reef/moderation", action)
            .await
            .status(),
        204
    );
    q.send(json!({"type":"chat","text":"Muted message"})).await;
    assert_eq!(q.until("error").await["reason"], "chat_muted");
    assert_eq!(
        b.api(
            reqwest::Method::POST,
            &format!("/api/reef/reports/{}/appeal", receipt.as_str().unwrap()),
            json!({"text":"Please review this decision"})
        )
        .await
        .status(),
        204
    );
    let (reads,actions):(i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE action='evidence_read'),count(*) FILTER(WHERE action='mute') FROM reef_moderator_audit WHERE actor_id=$1").bind(host as i64).fetch_one(&site.pool).await.unwrap();
    assert!(reads > 0);
    assert_eq!(actions, 1);
    assert_eq!(a.api(reqwest::Method::POST,"/api/reef/moderation",json!({"target":guest.to_string(),"owner":host.to_string(),"room_idx":null,"kind":"ban","seconds":3600,"report":receipt})).await.status(),204);
    assert_eq!(q.close_reason().await, "sanctioned");
}
#[tokio::test]
async fn bidirectional_blocks_suppress_chat_in_third_party_house_and_revoke_visits() {
    let Some(site) = Site::new().await else { return };
    let owner = unique();
    let alice = unique();
    let bob = unique();
    site.house(owner).await;
    site.house(alice).await;
    let a = site.browser(alice).await;
    let b = site.browser(bob).await;
    let c = site.browser(owner).await;
    for browser in [&a, &b, &c] {
        browser.consent().await;
    }
    let mut p = a.connect().await;
    let mut q = b.connect().await;
    let mut observer = c.connect().await;
    p.join(owner).await;
    q.join(owner).await;
    observer.join(owner).await;
    assert_eq!(
        a.api(reqwest::Method::PUT, &format!("/api/reef/blocks/{bob}"), json!(null))
            .await
            .status(),
        204
    );
    p.send(json!({"type":"chat","text":"Only the host sees Alice"})).await;
    let first = p.until("chat").await;
    assert_eq!(observer.until("chat").await["sender_id"], alice.to_string());
    q.send(json!({"type":"chat","text":"Only the host sees Bob"})).await;
    let second = q.until("chat").await;
    assert_eq!(observer.until("chat").await["sender_id"], bob.to_string());
    for (socket, other) in [(&mut p, bob), (&mut q, alice)] {
        let other = other.to_string();
        let received = tokio::time::timeout(Duration::from_millis(350), async {
            while let Some(Ok(Message::Text(raw))) = socket.socket.next().await {
                let value: Value = serde_json::from_str(&raw).unwrap();
                assert!(
                    !(value["type"] == "chat" && value["sender_id"] == other.as_str()),
                    "blocked message leaked"
                );
            }
        })
        .await;
        assert!(received.is_err());
    }
    for (message, recipient) in [
        (first["message_id"].as_str().unwrap(), bob),
        (second["message_id"].as_str().unwrap(), alice),
    ] {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM reef_chat_recipients WHERE message_id=$1 AND user_id=$2")
                .bind(message.parse::<i64>().unwrap())
                .bind(recipient as i64)
                .fetch_one(&site.pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
    q.send(json!({"type":"join","room":format!("house:{alice}:0")})).await;
    assert_eq!(q.until("error").await["reason"], "access_denied");
    assert_eq!(
        b.api(reqwest::Method::GET, &format!("/api/house/visit/{alice}"), json!(null))
            .await
            .status(),
        403
    );
    assert_eq!(
        site.app
            .db
            .sign_guestbook(alice, bob, "Bob", "hello", chrono::Utc::now().date_naive())
            .await
            .unwrap(),
        pinchora_db::SignOutcome::Blocked
    );
    assert_eq!(
        site.app
            .db
            .sign_guestbook(bob, alice, "Alice", "hello", chrono::Utc::now().date_naive())
            .await
            .unwrap(),
        pinchora_db::SignOutcome::Blocked
    );
    assert!(matches!(
        site.app
            .db
            .record_visit(alice, bob, chrono::Utc::now().date_naive(), 25, 10)
            .await
            .unwrap(),
        pinchora_db::VisitOutcome::Unavailable
    ));
    a.api(reqwest::Method::DELETE, &format!("/api/reef/blocks/{bob}"), json!(null))
        .await;
    q.join(alice).await;
    a.api(reqwest::Method::PUT, &format!("/api/reef/blocks/{bob}"), json!(null))
        .await;
    assert_eq!(q.close_reason().await, "access_revoked");
}
#[tokio::test]
async fn consent_revocation_logout_and_filter_fail_closed() {
    let Some(site) = Site::new().await else { return };
    let user = unique();
    site.house(user).await;
    let browser = site.browser(user).await;
    browser.consent().await;
    let mut player = browser.connect().await;
    player.join(user).await;
    player.send(json!({"type":"chat","text":"discord.gg/reef"})).await;
    assert_eq!(player.until("error").await["reason"], "chat_filtered");
    browser
        .api(
            reqwest::Method::PUT,
            "/api/reef/consent",
            json!({"version":1,"age_band":"13_plus","accepted":false}),
        )
        .await;
    assert_eq!(player.close_reason().await, "consent_revoked");
    browser.consent().await;
    let mut player = browser.connect().await;
    player.join(user).await;
    assert_eq!(
        browser
            .api(reqwest::Method::POST, "/auth/logout", json!(null))
            .await
            .status(),
        204
    );
    assert_eq!(player.close_reason().await, "logged_out");
}
#[tokio::test]
async fn retention_preserves_reported_evidence_and_deletion_removes_chat_but_not_active_bans() {
    let Some(site) = Site::new().await else { return };
    let user = unique();
    let host = unique();
    site.house(host).await;
    let browser = site.browser(user).await;
    browser.consent().await;
    let mut player = browser.connect().await;
    player.join(host).await;
    player.send(json!({"type":"chat","text":"A retained report"})).await;
    let msg = player.until("chat").await["message_id"]
        .as_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    site.app.db.reef_report(user, msg, "other").await.unwrap();
    sqlx::query("UPDATE reef_chat_evidence SET created_at=now()-interval '8 days' WHERE id=$1")
        .bind(msg)
        .execute(&site.pool)
        .await
        .unwrap();
    let ordinary:i64=sqlx::query_scalar("INSERT INTO reef_chat_evidence(owner_id,room_idx,sender_id,text,created_at) VALUES($1,0,$2,'ordinary',now()-interval '8 days') RETURNING id").bind(host as i64).bind(user as i64).fetch_one(&site.pool).await.unwrap();
    site.app.db.reef_retain(chrono::Utc::now()).await.unwrap();
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM reef_chat_evidence WHERE id IN ($1,$2)")
        .bind(msg)
        .bind(ordinary)
        .fetch_all(&site.pool)
        .await
        .unwrap();
    assert_eq!(ids, vec![msg]);
    sqlx::query("UPDATE reef_chat_evidence SET created_at=now()-interval '91 days' WHERE id=$1")
        .bind(msg)
        .execute(&site.pool)
        .await
        .unwrap();
    site.app.db.reef_retain(chrono::Utc::now()).await.unwrap();
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reef_reports WHERE message_id=$1)")
        .bind(msg)
        .fetch_one(&site.pool)
        .await
        .unwrap();
    assert!(!exists);
    player.send(json!({"type":"chat","text":"Delete this evidence"})).await;
    let fresh = player.until("chat").await["message_id"]
        .as_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    site.app.db.reef_report(user, fresh, "other").await.unwrap();
    site.app
        .db
        .reef_moderate(&pinchora_db::ReefModeration {
            actor: host,
            operator: false,
            target: user,
            owner: Some(host),
            idx: None,
            kind: "ban".into(),
            seconds: 3600,
            report: None,
        })
        .await
        .unwrap();
    // All six Reef tables contain member data; neither exports nor automatic restore points carry it.
    use pinchora_core::config_backup::{EXCLUDED_REEF_TABLES, Names, Policy};
    let guild = unique();
    site.app.db.guild_settings(guild).await.unwrap();
    sqlx::query("DELETE FROM events WHERE guild_id=$1")
        .bind(guild as i64)
        .execute(&site.pool)
        .await
        .unwrap();
    let backup = site.app.db.export_config(guild, Names::default()).await.unwrap();
    for table in EXCLUDED_REEF_TABLES {
        assert!(
            !backup.sections.contains_key(*table),
            "{table} leaked into configuration"
        );
    }
    assert!(!serde_json::to_string(&backup).unwrap().contains("Delete this evidence"));
    let restore = site
        .app
        .db
        .import_config(guild, &backup, Policy::Replace, Names::default())
        .await
        .unwrap();
    assert_eq!(
        site.app.db.config_restore_point(guild, restore).await.unwrap().unwrap(),
        backup
    );
    assert!(site.app.db.reef_eligible(user).await.unwrap());
    assert!(site.app.db.reef_sanctioned(user, host, 0, false).await.unwrap());
    site.app.db.delete_user_data(user, chrono::Utc::now()).await.unwrap();
    assert!(!site.app.db.reef_eligible(user).await.unwrap());
    assert!(site.app.db.reef_sanctioned(user, host, 0, false).await.unwrap());
    for sql in [
        "SELECT count(*) FROM reef_chat_evidence WHERE sender_id=$1",
        "SELECT count(*) FROM reef_chat_recipients WHERE user_id=$1",
        "SELECT count(*) FROM reef_reports WHERE reporter_id=$1",
        "SELECT count(*) FROM reef_consents WHERE user_id=$1",
    ] {
        let count: i64 = sqlx::query_scalar(sql)
            .bind(user as i64)
            .fetch_one(&site.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn socket_rate_limits_and_session_expiry() {
    let Some(site) = Site::new().await else { return };
    let user = unique();
    site.house(user).await;
    let browser = site.browser(user).await;
    browser.consent().await;
    let mut p = browser.connect().await;
    p.join(user).await;
    // Queue a burst without waiting for database-backed acknowledgments: those waits can
    // replenish the one-message-per-second bucket on a busy test database.
    for text in ["first", "second", "third", "fourth"] {
        p.send(json!({"type":"chat","text":text})).await;
    }
    let (mut accepted, mut rejected) = (0, 0);
    for _ in 0..4 {
        let response = p.until_any(&["chat", "error"]).await;
        if response["type"] == "chat" {
            accepted += 1;
        } else {
            assert_eq!(response["reason"], "rate_limited");
            rejected += 1;
        }
    }
    assert_eq!((accepted, rejected), (3, 1));
    site.app
        .cache
        .put(&format!("web:sess:{}", browser.sid), "{}", Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(p.close_reason().await, "session_expired");
}

#[tokio::test]
async fn operator_site_scope_room_scope_and_reversal_are_authorized() {
    let Some(site) = Site::new().await else { return };
    let owner = unique();
    let guest = unique();
    site.house(owner).await;
    let host = site.browser(owner).await;
    let member = site.browser(guest).await;
    let operator = site.browser(770000000000000099).await;
    let mut player = member.connect().await;
    player.join(owner).await;
    let action =
        json!({"target":guest.to_string(),"owner":null,"room_idx":null,"kind":"ban","seconds":3600,"report":null});
    assert_eq!(
        host.api(reqwest::Method::POST, "/api/reef/moderation", action.clone())
            .await
            .status(),
        403
    );
    assert_eq!(
        operator
            .api(reqwest::Method::POST, "/api/reef/moderation", action)
            .await
            .status(),
        204
    );
    assert_eq!(player.close_reason().await, "sanctioned");
    let other = unique();
    site.house(other).await;
    assert!(site.app.db.reef_sanctioned(guest, other, 0, false).await.unwrap());
    assert_eq!(operator.api(reqwest::Method::POST,"/api/reef/moderation",json!({"target":guest.to_string(),"owner":null,"room_idx":null,"kind":"revoke","seconds":1,"report":null})).await.status(),204);
    assert!(!site.app.db.reef_sanctioned(guest, other, 0, false).await.unwrap());
    assert_eq!(host.api(reqwest::Method::POST,"/api/reef/moderation",json!({"target":guest.to_string(),"owner":owner.to_string(),"room_idx":1,"kind":"mute","seconds":3600,"report":null})).await.status(),204);
    assert!(!site.app.db.reef_sanctioned(guest, owner, 0, true).await.unwrap());
    assert!(site.app.db.reef_sanctioned(guest, owner, 1, true).await.unwrap());
    assert!(!site.app.db.reef_sanctioned(guest, owner, 1, false).await.unwrap());
}

#[path = "reef/multipod.rs"]
mod multipod;

impl Browser {
    async fn api_value(&self, method: reqwest::Method, path: &str, body: Value) -> (reqwest::StatusCode, Value) {
        let response = reqwest::Client::new()
            .request(method, format!("{}{}", self.url, path))
            .header("cookie", format!("pinchora_sid={}", self.sid))
            .header("origin", &self.url)
            .header("x-csrf-token", "reef-test-csrf")
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.bytes().await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }
}
#[tokio::test]
async fn wave7_atomic_receipts_revision_races_and_shared_undo() {
    let Some(site) = Site::new().await else { return };
    let owner = unique();
    site.house(owner).await;
    let browser = site.browser(owner).await;
    let item = "pebblerest_stool__sand";
    site.app.db.grant_item(owner, item, 2).await.unwrap();
    let (_, before) = browser
        .api_value(reqwest::Method::GET, "/api/house/rooms/0", Value::Null)
        .await;
    let request = json!({"request_id":"wave7-place","expected_revision":before["revision"],"expected_fingerprint":before["fingerprint"],"op":{"type":"place","item_id":item,"x":1,"y":1,"flipped":false}});
    let replay_after_retention = request.clone();
    let (a, b) = tokio::join!(
        browser.api_value(reqwest::Method::POST, "/api/house/rooms/0/ops", request.clone()),
        browser.api_value(reqwest::Method::POST, "/api/house/rooms/0/ops", request.clone())
    );
    assert_eq!(a.0, 200, "{:?}", a);
    assert_eq!(b.0, 200, "{:?}", b);
    assert_eq!(site.app.db.placements(owner, 0).await.unwrap().len(), 1);
    let mut reused = request.clone();
    reused["op"]["x"] = json!(2);
    assert_eq!(
        browser
            .api_value(reqwest::Method::POST, "/api/house/rooms/0/ops", reused)
            .await
            .0,
        422
    );
    let mut stale = request;
    stale["request_id"] = json!("stale");
    assert_eq!(
        browser
            .api_value(reqwest::Method::POST, "/api/house/rooms/0/ops", stale)
            .await
            .0,
        409
    );
    let rows = site.app.db.placements(owner, 0).await.unwrap();
    let result = pinchora_commands::house_edit::apply(
        &site.app,
        pinchora_commands::Who::own(owner),
        0,
        (8, 8),
        &pinchora_commands::house_edit::fingerprint(&rows),
        pinchora_commands::house_edit::Operation::Undo,
    )
    .await
    .unwrap();
    assert!(matches!(result, pinchora_commands::house_edit::Outcome::Done(_)));
    assert!(site.app.db.placements(owner, 0).await.unwrap().is_empty());
    assert_eq!(
        site.app
            .db
            .inventory(owner)
            .await
            .unwrap()
            .iter()
            .find(|r| r.item_id == item)
            .unwrap()
            .quantity,
        2
    );
    let (_, before) = browser
        .api_value(reqwest::Method::GET, "/api/house/rooms/0", Value::Null)
        .await;
    // A surface-only edit must invalidate a furniture edit even when its fingerprint is unchanged.
    site.app
        .db
        .set_room_surfaces(owner, 0, Some(&Content::get().surfaces.wallpapers[1].id), None)
        .await
        .unwrap();
    let request = json!({"request_id":"after-surface","expected_revision":before["revision"],"expected_fingerprint":before["fingerprint"],"op":{"type":"place","item_id":item,"x":1,"y":1,"flipped":false}});
    assert_eq!(
        browser
            .api_value(reqwest::Method::POST, "/api/house/rooms/0/ops", request)
            .await
            .0,
        409
    );
    let (_, fresh) = browser
        .api_value(reqwest::Method::GET, "/api/house/rooms/0", Value::Null)
        .await;
    let wallpaper = &Content::get().surfaces.wallpapers[0].id;
    site.app
        .db
        .grant_item(owner, &format!("wp:{wallpaper}"), 1)
        .await
        .unwrap();
    let surface = json!({"request_id":"surface-retry","expected_revision":fresh["revision"],"expected_fingerprint":fresh["fingerprint"],"op":{"type":"surface","wallpaper":wallpaper}});
    let (status, saved) = browser
        .api_value(reqwest::Method::POST, "/api/house/rooms/0/ops", surface.clone())
        .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["wallpaper"], *wallpaper);
    let newer = &Content::get().surfaces.wallpapers[2].id;
    site.app
        .db
        .set_room_surfaces(owner, 0, Some(newer), None)
        .await
        .unwrap();
    let revision = site.app.db.reef_edit_revision(owner).await.unwrap();
    let (status, replay) = browser
        .api_value(reqwest::Method::POST, "/api/house/rooms/0/ops", surface)
        .await;
    assert_eq!(status, 200, "{replay}");
    assert_eq!(replay["replayed"], true);
    assert_eq!(
        replay["wallpaper"], *newer,
        "retry reconciles current authoritative surfaces"
    );
    assert_eq!(replay["revision"], revision);
    assert_eq!(site.app.db.reef_edit_revision(owner).await.unwrap(), revision);
    sqlx::query("UPDATE reef_edit_receipts SET created_at=now()-interval '8 days' WHERE actor_id=$1 AND request_id='wave7-place'")
        .bind(owner as i64).execute(&site.pool).await.unwrap();
    site.app.db.reef_prune_outbox().await.unwrap();
    assert_eq!(
        browser
            .api_value(reqwest::Method::POST, "/api/house/rooms/0/ops", replay_after_retention)
            .await
            .0,
        409
    );
    assert!(site.app.db.placements(owner, 0).await.unwrap().is_empty());
}
#[tokio::test]
async fn wave7_doorbell_admission_expiry_blocks_and_presence_consent() {
    let Some(site) = Site::new().await else { return };
    let host = unique();
    let guest = unique();
    site.house(host).await;
    let h = site.browser(host).await;
    let g = site.browser(guest).await;
    site.app.db.befriend(host, guest).await.unwrap();
    assert_eq!(
        h.api_value(
            reqwest::Method::PUT,
            "/api/reef/preferences",
            json!({"presence":false,"visits":"doorbell","doorbell":true})
        )
        .await
        .0,
        200
    );
    let key = format!("house:{host}:0");
    let mut visitor = g.connect().await;
    visitor.send(json!({"type":"join","room":key})).await;
    assert_eq!(visitor.until("error").await["reason"], "access_denied");
    let path = format!("/api/reef/houses/{host}/doorbells");
    let (status, bell) = g.api_value(reqwest::Method::POST, &path, json!({})).await;
    assert_eq!(status, 200, "{bell}");
    assert_eq!(g.api_value(reqwest::Method::POST, &path, json!({})).await.0, 403);
    let decision = format!("/api/reef/doorbells/{}/decision", bell["id"].as_str().unwrap());
    assert_eq!(
        g.api_value(reqwest::Method::POST, &decision, json!({"accept":true}))
            .await
            .0,
        403
    );
    assert_eq!(
        h.api_value(reqwest::Method::POST, &decision, json!({"accept":true}))
            .await
            .0,
        200
    );
    visitor.send(json!({"type":"join","room":key})).await;
    visitor.until("snapshot").await;
    let mut owner = h.connect().await;
    owner.send(json!({"type":"join","room":key})).await;
    owner.until("snapshot").await;
    assert_eq!(
        g.api_value(reqwest::Method::GET, "/api/reef/social", Value::Null)
            .await
            .1["people"][0]["online"],
        false
    );
    h.api_value(
        reqwest::Method::PUT,
        "/api/reef/preferences",
        json!({"presence":true,"visits":"doorbell","doorbell":true}),
    )
    .await;
    assert_eq!(
        g.api_value(reqwest::Method::GET, "/api/reef/social", Value::Null)
            .await
            .1["people"][0]["online"],
        true
    );
    sqlx::query("UPDATE reef_doorbells SET expires_at=now()-interval '1 second' WHERE host_id=$1")
        .bind(host as i64)
        .execute(&site.pool)
        .await
        .unwrap();
    assert!(!site.app.db.reef_house_access(host, guest).await.unwrap());
    site.app.db.block_signer(host, guest, "").await.unwrap();
    assert!(
        g.api_value(reqwest::Method::GET, "/api/reef/social", Value::Null)
            .await
            .1["people"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(g.api_value(reqwest::Method::POST, &path, json!({})).await.0, 403);
    assert_eq!(
        h.api_value(reqwest::Method::POST, &decision, json!({"accept":true}))
            .await
            .0,
        403
    );
}
#[tokio::test]
async fn wave7_guestbook_appreciation_and_evidence_reuse() {
    let Some(site) = Site::new().await else { return };
    let host = unique();
    let guest = unique();
    site.house(host).await;
    let h = site.browser(host).await;
    let g = site.browser(guest).await;
    site.app.db.reef_consent(guest, 1, "13_plus", true).await.unwrap();
    let mut player = g.connect().await;
    player
        .send(json!({"type":"join","room":format!("house:{host}:0")}))
        .await;
    player.until("snapshot").await;
    let path = format!("/api/reef/houses/{host}/guestbook");
    assert_eq!(
        g.api_value(reqwest::Method::POST, &path, json!({"text":"Lovely room"}))
            .await
            .1["signed"],
        true
    );
    assert_eq!(
        g.api_value(reqwest::Method::POST, &path, json!({"text":"Lovely room again"}))
            .await
            .1["signed"],
        false
    );
    assert_eq!(
        g.api_value(reqwest::Method::POST, &path, json!({"text":"https://example.com"}))
            .await
            .0,
        422
    );
    let note = g.api_value(reqwest::Method::GET, &path, Value::Null).await.1["notes"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let report = format!("{path}/{note}/report");
    let a = h
        .api_value(reqwest::Method::POST, &report, json!({"reason":"other"}))
        .await;
    let b = h
        .api_value(reqwest::Method::POST, &report, json!({"reason":"other"}))
        .await;
    assert_eq!(a.0, 200, "{:?}", a);
    assert_eq!(a.1["receipt"], b.1["receipt"]);
    let path = format!("/api/reef/houses/{host}/appreciate");
    assert_eq!(h.api_value(reqwest::Method::POST, &path, json!({})).await.0, 403);
    assert_eq!(
        g.api_value(reqwest::Method::POST, &path, json!({})).await.1["appreciated"],
        true
    );
    assert_eq!(
        g.api_value(reqwest::Method::POST, &path, json!({})).await.1["appreciated"],
        false
    );
    let response = reqwest::Client::new()
        .post(format!("{}{}", h.url, path))
        .header("cookie", format!("pinchora_sid={}", h.sid))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
#[ignore = "seeds isolated Wave 7 browser fixtures for the lead"]
async fn seed_reef_wave7() {
    let site = Site::new().await.expect("test services");
    let host = 770000000000000181u64;
    let guest = 770000000000000182u64;
    let guild = 770000000000000180u64;
    site.app.db.guild_settings(guild).await.unwrap();
    sqlx::query("DELETE FROM events WHERE guild_id=$1")
        .bind(guild as i64)
        .execute(&site.pool)
        .await
        .unwrap();
    for user in [host, guest, guild] {
        site.house(user).await;
        sqlx::query("DELETE FROM placements WHERE user_id=$1")
            .bind(user as i64)
            .execute(&site.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM reef_edit_receipts WHERE actor_id=$1 OR owner_id=$1")
            .bind(user as i64)
            .execute(&site.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM reef_doorbells WHERE host_id=$1 OR guest_id=$1")
            .bind(user as i64)
            .execute(&site.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM guestbook_blocks WHERE host_id=$1 OR blocked_id=$1")
            .bind(user as i64)
            .execute(&site.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM guestbook_notes WHERE host_id=$1 OR author_id=$1")
            .bind(user as i64)
            .execute(&site.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM house_likes WHERE host_id=$1 OR liker_id=$1")
            .bind(user as i64)
            .execute(&site.pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM reef_sanctions WHERE user_id=$1")
            .bind(user as i64)
            .execute(&site.pool)
            .await
            .unwrap();
        site.app.db.set_house_locked(user, false).await.unwrap();
        site.app.db.set_privacy(user, Hide::House, false).await.unwrap();
        site.app
            .db
            .reef_save_space_preferences(
                user,
                &pinchora_db::ReefSpacePreferences {
                    presence: false,
                    visits: "open".into(),
                    doorbell: true,
                },
            )
            .await
            .unwrap();
        site.app.db.grant_item(user, "pebblerest_stool__sand", 5).await.unwrap();
        if user != guild {
            site.app
                .db
                .set_character(user, &Character::starter(Content::get()).encode())
                .await
                .unwrap();
            site.app.db.reef_consent(user, 1, "13_plus", true).await.unwrap();
        }
    }
    site.app.db.befriend(host, guest).await.unwrap();
}

#[tokio::test]
async fn wave7_partner_block_is_rechecked_by_the_shared_transaction() {
    let Some(site) = Site::new().await else { return };
    let host = unique();
    let guest = unique();
    site.house(host).await;
    site.app.db.pair_partners(host, guest).await.unwrap();
    let pair = site.app.db.partner(guest).await.unwrap().unwrap();
    let who = pinchora_commands::Who::partner_of(guest, pair);
    let item = "pebblerest_stool__sand";
    site.app.db.grant_item(guest, item, 1).await.unwrap();
    // Resolve authorization first, then revoke it before the actual mutation (TOCTOU).
    site.app.db.block_signer(guest, host, "").await.unwrap();
    let before = site.app.db.placements(host, 0).await.unwrap();
    let outcome = pinchora_commands::house_edit::apply(
        &site.app,
        who,
        0,
        (8, 8),
        &pinchora_commands::house_edit::fingerprint(&before),
        pinchora_commands::house_edit::Operation::Place {
            item_id: item.into(),
            x: 1,
            y: 1,
            flipped: false,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        outcome,
        pinchora_commands::house_edit::Outcome::Refused { .. }
    ));
    assert!(site.app.db.placements(host, 0).await.unwrap().is_empty());
    assert_eq!(
        site.app
            .db
            .inventory(guest)
            .await
            .unwrap()
            .iter()
            .find(|r| r.item_id == item)
            .unwrap()
            .quantity,
        1
    );
}

#[tokio::test]
async fn wave7_hidden_notes_cannot_be_promoted_to_report_evidence() {
    let Some(site) = Site::new().await else { return };
    let host = unique();
    let author = unique();
    let reader = unique();
    site.house(host).await;
    site.browser(author).await;
    let note: i64 = sqlx::query_scalar("INSERT INTO guestbook_notes(host_id,author_id,author_name,day,body) VALUES($1,$2,'author',CURRENT_DATE,'Lovely room') RETURNING id")
        .bind(host as i64).bind(author as i64).fetch_one(&site.pool).await.unwrap();
    site.app.db.block_signer(author, reader, "").await.unwrap();
    let browser = site.browser(reader).await;
    let path = format!("/api/reef/houses/{host}/guestbook");
    assert_eq!(
        browser.api_value(reqwest::Method::GET, &path, Value::Null).await.1["notes"],
        json!([])
    );
    assert_eq!(
        browser
            .api_value(
                reqwest::Method::POST,
                &format!("{path}/{note}/report"),
                json!({"reason":"other"})
            )
            .await
            .0,
        403
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM reef_guestbook_evidence WHERE note_id=$1")
        .bind(note)
        .fetch_one(&site.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    site.app.db.unblock_signer(author, reader).await.unwrap();
    let evidence = site
        .app
        .db
        .reef_note_evidence(host, reader, note)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        site.app.db.reef_note_evidence(host, reader, note).await.unwrap(),
        Some(evidence)
    );
    assert!(
        site.app
            .db
            .reef_report(reader, evidence, "other")
            .await
            .unwrap()
            .is_some()
    );
    site.app.db.delete_note(host, note).await.unwrap();
    assert!(
        site.app
            .db
            .reef_note_evidence(host, reader, note)
            .await
            .unwrap()
            .is_none()
    );
    let kept: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reef_chat_evidence WHERE id=$1 AND reported)")
        .bind(evidence)
        .fetch_one(&site.pool)
        .await
        .unwrap();
    assert!(kept, "deleting a note must retain reported evidence");
}

#[tokio::test]
async fn wave7_abuse_limits_cover_reports_likes_and_rsvp() {
    let Some(site) = Site::new().await else { return };
    let host = unique();
    let user = unique();
    site.house(host).await;
    let browser = site.browser(user).await;
    for (action, count, path, body) in [
        (
            "report",
            10,
            "/api/reef/reports".to_owned(),
            json!({"message_id":"1","reason":"other"}),
        ),
        (
            "report",
            10,
            format!("/api/reef/houses/{host}/guestbook/1/report"),
            json!({"reason":"other"}),
        ),
        (
            "appreciate",
            20,
            format!("/api/reef/houses/{host}/appreciate"),
            json!({}),
        ),
        (
            "rsvp",
            20,
            format!("/api/reef/guilds/{host}/gatherings/1/rsvp"),
            json!({"option":"going"}),
        ),
        (
            "schedule",
            6,
            format!("/api/reef/guilds/{host}/gatherings"),
            json!({"title":"Tour","starts_at":chrono::Utc::now(),"ends_at":chrono::Utc::now()+chrono::Duration::hours(1)}),
        ),
        ("doorbell", 12, format!("/api/reef/houses/{host}/doorbells"), json!({})),
        (
            "guestbook",
            10,
            format!("/api/reef/houses/{host}/guestbook"),
            json!({"text":"Lovely room"}),
        ),
    ] {
        // Seed both sides of a window boundary so this test cannot flake at xx:xx:59.
        let window = chrono::Utc::now().timestamp() / 60;
        for bucket in [window, window + 1] {
            site.app
                .cache
                .incr_by(
                    &format!("web:rl:reef:{action}:{user}:{bucket}"),
                    count,
                    Duration::from_secs(120),
                )
                .await
                .unwrap();
        }
        assert_eq!(
            browser.api_value(reqwest::Method::POST, &path, body).await.0,
            429,
            "{path}"
        );
    }
}

#[tokio::test]
async fn wave7_socket_binary_and_control_frame_floods_close() {
    let Some(site) = Site::new().await else { return };
    let browser = site.browser(unique()).await;
    let mut player = browser.connect().await;
    player.socket.send(Message::Binary(vec![0; 16].into())).await.unwrap();
    assert_eq!(player.close_reason().await, "invalid_message");
    let mut player = browser.connect().await;
    for _ in 0..70 {
        if player.socket.send(Message::Ping(vec![].into())).await.is_err() {
            break;
        }
    }
    assert_eq!(player.close_reason().await, "rate_limited");
}

#[tokio::test]
async fn wave7_blocks_filter_room_rosters_and_live_movement() {
    let Some(site) = Site::new().await else { return };
    let host = unique();
    let a = unique();
    let b = unique();
    site.house(host).await;
    let first = site.browser(a).await;
    let second = site.browser(b).await;
    site.app.db.block_signer(a, b, "").await.unwrap();
    let mut one = first.connect().await;
    one.join(host).await;
    let mut two = second.connect().await;
    let snapshot = two.join(host).await;
    assert!(
        snapshot["players"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["user_id"].as_str() != Some(a.to_string().as_str()))
    );
    one.send(json!({"type":"snapshot"})).await;
    let snapshot = one.until("snapshot").await;
    assert!(
        snapshot["players"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["user_id"].as_str() != Some(b.to_string().as_str()))
    );
    for _ in 0..3 {
        let correction = one.until("correction").await;
        if correction["roster"] != true {
            continue;
        }
        assert!(
            correction["players"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["user_id"].as_str() != Some(b.to_string().as_str()))
        );
    }
}

#[tokio::test]
async fn wave7_doorbell_retention_preserves_recent_abuse_counters() {
    let Some(site) = Site::new().await else { return };
    let host = unique();
    let guest = unique();
    let old: i64 = sqlx::query_scalar("INSERT INTO reef_doorbells(host_id,guest_id,created_at,expires_at) VALUES($1,$2,now()-interval '1 hour',now()-interval '1 minute') RETURNING id")
        .bind(host as i64).bind(guest as i64).fetch_one(&site.pool).await.unwrap();
    let recent: i64 = sqlx::query_scalar(
        "INSERT INTO reef_doorbells(host_id,guest_id,expires_at) VALUES($1,$2,now()-interval '1 second') RETURNING id",
    )
    .bind(host as i64)
    .bind(guest as i64)
    .fetch_one(&site.pool)
    .await
    .unwrap();
    site.app.db.reef_retain(chrono::Utc::now()).await.unwrap();
    let retained: Vec<i64> = sqlx::query_scalar("SELECT id FROM reef_doorbells WHERE id IN ($1,$2) ORDER BY id")
        .bind(old)
        .bind(recent)
        .fetch_all(&site.pool)
        .await
        .unwrap();
    assert_eq!(retained, vec![recent]);
}
