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
    pool: PgPool,
    url: String,
    server: JoinHandle<()>,
}
impl Drop for Site {
    fn drop(&mut self) {
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
        let (Ok(database), Ok(redis)) = (
            std::env::var("PINCHORA_TEST_DATABASE_URL"),
            std::env::var("PINCHORA_TEST_REDIS_URL"),
        ) else {
            eprintln!("set test database and Redis URLs");
            return None;
        };
        let db = Db::connect(&database, 5).await.expect("test Postgres reachable");
        db.migrate().await.unwrap();
        let cache = Cache::connect(&redis).await.expect("test Redis reachable");
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
            bind: addr,
            ..WebConfig::default()
        };
        let state = WebState::new(app.clone(), WebSettings::resolve(&config, 1).unwrap());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                pinchora_web::router(state).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap();
        });
        Some(Self {
            app,
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
                    if value["type"] == kind {
                        if kind == "snapshot" {
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
