//! The read-only house API: rooms as data, the catalogue and the sprite sheet, and the bot's own picture
//! of a room, driven like a browser would with dev login.
//!
//! Needs `PINCHORA_TEST_DATABASE_URL` and `PINCHORA_TEST_REDIS_URL`; skipped when either is unset.
//! `seed_demo_house` (ignored) leaves a furnished house for the Playwright screenshots to look at.

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicU64, Ordering},
    },
    time::SystemTime,
};

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::ConnectInfo,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use pinchora_cache::Cache;
use pinchora_commands::{App, RoomData, render_room_png};
use pinchora_core::config::WebConfig;
use pinchora_db::{Db, Hide, NewRoom};
use pinchora_game::{Content, character::Character, house::tier};
use pinchora_render::sprites::{ATLAS_JSON, ATLAS_PNG};
use pinchora_web::{WebSettings, WebState};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use twilight_http::Client;
use twilight_model::id::Id;

const PUBLIC_URL: &str = "http://dash.test";

/// The room the bot's own preview draws, to see the art in place.
const SHOWCASE: [(&str, i16, i16, bool); 21] = [
    ("pebblepane_window__sand", 1, 0, false),
    ("hemhill_painting__cream", 4, 0, false),
    ("pebblehour_clock__navy", 7, 0, false),
    ("dunearc_window__coral", 2, 0, true),
    ("stillframe_door__cream", 6, 1, true),
    ("stillpanel_wardrobe__cream", 0, 0, false),
    ("stillstep_nightstand__cream", 6, 0, false),
    ("pebblecap_lamp__sand", 6, 0, false),
    ("patchpillow_bed__cream", 7, 0, false),
    ("humskein_upright__plum", 0, 3, true),
    ("rivetgrid_rug__sand", 2, 4, false),
    ("crumbcorner_table__moss", 3, 4, false),
    ("pagepair_books__moss", 3, 4, false),
    ("sprigcup_fern__moss", 4, 5, false),
    ("plainwhorl_chair__sand", 5, 4, false),
    ("plainwhorl_chair__sand", 3, 6, true),
    ("crumbrail_bench__sand", 6, 7, false),
    ("stillstem_rubber_tree__cream", 0, 7, false),
    ("rivetarm_floor_lamp__charcoal", 9, 4, false),
    ("blinkslot_cabinet__plum", 8, 7, false),
    ("duneline_runner__coral", 6, 3, false),
];

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

struct Site {
    router: Router,
    app: Arc<App>,
    pool: PgPool,
}

impl Site {
    async fn new(name: &str) -> Option<Self> {
        let (Ok(database), Ok(redis)) = (
            std::env::var("PINCHORA_TEST_DATABASE_URL"),
            std::env::var("PINCHORA_TEST_REDIS_URL"),
        ) else {
            eprintln!("{name}: skipped, set PINCHORA_TEST_DATABASE_URL and PINCHORA_TEST_REDIS_URL");
            return None;
        };
        let db = Db::connect(&database, 5).await.expect("connect to the database");
        db.migrate().await.expect("migrate");
        let cache = Cache::connect(&redis).await.expect("connect to redis");
        // Plaza entry verifies membership freshly even when the OAuth guild list is cached.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener,Router::new().fallback(|uri:axum::http::Uri|async move {
                let user=uri.path().rsplit('/').next().unwrap_or("1");
                Json(json!({"user":{"id":user,"username":"member","discriminator":"0","avatar":null},"roles":[],"joined_at":"2024-01-01T00:00:00.000000+00:00","deaf":false,"mute":false,"flags":0,"pending":false}))
            })).await.unwrap();
        });
        let rest = Arc::new(
            Client::builder()
                .token("Bot house-test".into())
                .proxy(address.to_string(), true)
                .ratelimiter(None)
                .build(),
        );
        let app = Arc::new(App::new(rest.clone(), rest, Id::new(1), db, cache, "house-test".into()));
        let web = WebConfig {
            public_url: PUBLIC_URL.into(),
            client_secret: Some(String::from("test-secret").into()),
            dev_login: true,
            bind: ([127, 0, 0, 1], 8090).into(),
            ..WebConfig::default()
        };
        let settings = WebSettings::resolve(&web, 1).expect("settings");
        let state = WebState::new(app.clone(), settings);
        Some(Self {
            router: pinchora_web::router(state),
            app,
            pool: PgPool::connect(&database).await.expect("a pool"),
        })
    }

    fn browser(&self) -> Browser {
        Browser {
            router: self.router.clone(),
            cookies: HashMap::new(),
            address: SocketAddr::from(([10, 1, (unique() % 250) as u8, (unique() % 250) as u8], 4000)),
        }
    }

    /// A house with `tier` rooms' worth of space, `items` in its first room, a character and some pets.
    async fn move_in(&self, user: u64, tier_number: i16, items: &[(&str, i16, i16, bool)]) {
        let content = Content::get();
        let starter = NewRoom {
            room_type: "living".to_owned(),
            wallpaper: content.surfaces.wallpapers[0].id.clone(),
            floor: content.surfaces.floors[0].id.clone(),
        };
        self.app.db.house(user, &starter).await.expect("a house");
        let id = i64::try_from(user).expect("an id");
        sqlx::query("UPDATE houses SET tier = $2 WHERE user_id = $1")
            .bind(id)
            .bind(tier_number)
            .execute(&self.pool)
            .await
            .expect("tier");
        for idx in 1..tier(tier_number).rooms as i16 {
            sqlx::query(
                "INSERT INTO rooms (user_id, idx, room_type, wallpaper, floor) VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (user_id, idx) DO NOTHING",
            )
            .bind(id)
            .bind(idx)
            .bind(["living", "bedroom", "kitchen", "bathroom", "garden", "studio"][idx as usize])
            .bind(&content.surfaces.wallpapers[idx as usize * 4].id)
            .bind(&content.surfaces.floors[idx as usize * 3].id)
            .execute(&self.pool)
            .await
            .expect("room");
        }
        sqlx::query("DELETE FROM placements WHERE user_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .expect("clear");
        for (item, x, y, flipped) in items {
            sqlx::query(
                "INSERT INTO placements (user_id, room_idx, item_id, x, y, flipped) VALUES ($1, 0, $2, $3, $4, $5)",
            )
            .bind(id)
            .bind(item)
            .bind(x)
            .bind(y)
            .bind(flipped)
            .execute(&self.pool)
            .await
            .expect("placement");
        }
    }

    /// Two pets awake and one that fell asleep, and a character.
    async fn populate(&self, user: u64) -> String {
        let content = Content::get();
        sqlx::query("DELETE FROM pets WHERE owner_id = $1")
            .bind(i64::try_from(user).expect("an id"))
            .execute(&self.pool)
            .await
            .expect("no pets");
        let species = |i: usize| content.pets[i].id.clone();
        self.app.db.adopt(user, &species(0), "Pip").await.expect("a pet");
        self.app.db.adopt(user, &species(14), "Moss").await.expect("a pet");
        let sleeper = self.app.db.adopt(user, &species(23), "Dozy").await.expect("a pet");
        sqlx::query(
            "UPDATE pets SET hunger = 0, fun = 0, hygiene = 0, needs_at = now() - interval '4 days' WHERE id = $1",
        )
        .bind(sleeper.id)
        .execute(&self.pool)
        .await
        .expect("a sleepy pet");
        let code = Character::starter(content).encode();
        self.app.db.set_character(user, &code).await.expect("a character");
        code
    }
}

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

    fn code(&self) -> String {
        self.json()["error"]["code"].as_str().unwrap_or_default().to_owned()
    }
}

#[derive(Clone)]
struct Browser {
    router: Router,
    cookies: HashMap<String, String>,
    address: SocketAddr,
}

impl Browser {
    async fn send(&mut self, mut request: Request<Body>) -> Response {
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

    async fn get_with(&mut self, path: &str, name: &str, value: &str) -> Response {
        self.send(Request::get(path).header(name, value).body(Body::empty()).unwrap())
            .await
    }

    /// Logs in without Discord, as `user`.
    async fn login(&mut self, user: u64) {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/auth/dev-login")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, PUBLIC_URL)
            .header("sec-fetch-site", "same-origin")
            .body(Body::from(
                json!({ "user_id": user.to_string(), "username": format!("player{user}") }).to_string(),
            ))
            .unwrap();
        let response = self.send(request).await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.text());
    }
}

/// Saves a PNG the dashboard served to `target/web-sim/<test>/<step>.png`, for a person to look at.
fn save_png(test: &str, step: &str, bytes: &[u8]) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/web-sim/{test}"));
    std::fs::create_dir_all(&dir).expect("png folder");
    let path = dir.join(format!("{step}.png"));
    std::fs::write(&path, bytes).expect("write png");
    path
}

#[tokio::test]
async fn the_house_pages_need_a_login() {
    let Some(site) = Site::new("the_house_pages_need_a_login").await else {
        return;
    };
    let mut anonymous = site.browser();
    for path in [
        "/api/house",
        "/api/house/rooms/0",
        "/api/house/rooms/0/render.png",
        "/api/house/visit/5",
        "/api/house/character.png?code=x",
    ] {
        let response = anonymous.get(path).await;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{path}");
    }
    // The catalogue and the sprite sheet are the same for everyone.
    assert_eq!(anonymous.get("/api/house/content").await.status, StatusCode::OK);
    assert_eq!(anonymous.get("/assets/atlas.json").await.status, StatusCode::OK);
}

#[tokio::test]
async fn looking_at_a_house_never_moves_anyone_in() {
    let Some(site) = Site::new("looking_at_a_house_never_moves_anyone_in").await else {
        return;
    };
    let user = unique();
    let mut browser = site.browser();
    browser.login(user).await;
    for path in ["/api/house", "/api/house/rooms/0", "/api/house/rooms/0/render.png"] {
        let response = browser.get(path).await;
        assert_eq!(response.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(response.code(), "no_house");
        assert!(response.json()["error"]["message"].as_str().unwrap().contains("/house"));
    }
    assert!(
        site.app.db.house_if_any(user).await.expect("a query").is_none(),
        "the reads created no house"
    );
    assert!(
        site.app.db.inventory(user).await.expect("a query").is_empty(),
        "and handed out no starter kit"
    );
}

#[tokio::test]
async fn a_house_comes_as_data_and_as_the_bots_own_picture() {
    let Some(site) = Site::new("a_house_comes_as_data").await else {
        return;
    };
    let user = unique();
    site.move_in(user, 3, &SHOWCASE).await;
    let code = site.populate(user).await;
    let mut browser = site.browser();
    browser.login(user).await;

    let house = browser.get("/api/house").await;
    assert_eq!(house.status, StatusCode::OK, "{}", house.text());
    assert_eq!(house.header("cache-control"), "no-store");
    let house = house.json();
    assert_eq!(house["scope"], "own");
    assert_eq!(house["owner"]["id"], user.to_string());
    assert_eq!(house["owner"]["you"], true);
    assert_eq!(house["house"]["tier"], 3);
    assert_eq!(house["house"]["tier_name"], "Reef House");
    assert_eq!((house["width"].as_u64(), house["depth"].as_u64()), (Some(10), Some(9)));
    let rooms = house["rooms"].as_array().expect("rooms");
    assert_eq!(rooms.len(), 3);
    assert_eq!(rooms[0]["items"], 21);
    assert_eq!(
        rooms[0]["name"],
        Content::get().room_type("living").expect("a living room").name.as_str()
    );
    assert_eq!(rooms[1]["items"], 0);

    let room = browser.get("/api/house/rooms/0").await;
    assert_eq!(room.status, StatusCode::OK, "{}", room.text());
    let room = room.json();
    assert_eq!((room["width"].as_u64(), room["depth"].as_u64()), (Some(10), Some(9)));
    let placements = room["placements"].as_array().expect("placements");
    assert_eq!(placements.len(), 21);
    assert!(
        placements.iter().all(|p| p["id"].is_string()),
        "every id is a string, as in the rest of the API"
    );
    assert_eq!(placements[0]["item_id"], "pebblepane_window__sand");
    let pets = room["pets"].as_array().expect("pets");
    assert_eq!(pets.len(), 3);
    assert_eq!(
        pets.iter().filter(|p| p["asleep"] == true).count(),
        1,
        "one pet fell asleep"
    );
    assert_eq!(room["characters"], json!([code]));
    let seed = room["seed"].as_u64().expect("a seed");

    // The bot's picture, byte for byte what the Discord path would draw for the same room.
    let png = browser.get(&format!("/api/house/rooms/0/render.png?seed={seed}")).await;
    assert_eq!(png.status, StatusCode::OK);
    assert_eq!(png.header("content-type"), "image/png");
    let (db_house, db_rooms) = site.app.db.house_if_any(user).await.unwrap().unwrap();
    let mut data = RoomData::load(&site.app, user, &db_house, &db_rooms, 0)
        .await
        .unwrap()
        .expect("room 0");
    data.seed = seed;
    let direct = render_room_png(&site.app, data.view(), 2).await.expect("renders");
    assert_eq!(png.body.as_ref(), direct.as_slice());
    save_png("house", "room0", &png.body);
    let native = browser
        .get(&format!("/api/house/rooms/0/render.png?seed={seed}&scale=1"))
        .await;
    let native = image::load_from_memory(&native.body).expect("a PNG");
    let big = image::load_from_memory(&png.body).expect("a PNG");
    assert_eq!((big.width(), big.height()), (native.width() * 2, native.height() * 2));
    for bad in ["scale=0", "scale=9", "scale=two", "seed=-1"] {
        let response = browser.get(&format!("/api/house/rooms/0/render.png?{bad}")).await;
        assert!(response.status == StatusCode::BAD_REQUEST, "{bad}: {}", response.status);
    }

    // The character the room lists, drawn by the server.
    let sprite = browser
        .get(&format!("/api/house/character.png?code={}", urlencode(&code)))
        .await;
    assert_eq!(sprite.status, StatusCode::OK, "{}", sprite.text());
    let sprite = image::load_from_memory(&sprite.body).expect("a PNG");
    assert!(sprite.width() > 8 && sprite.height() > 8);
    assert_eq!(
        browser.get("/api/house/character.png?code=nonsense").await.code(),
        "malformed"
    );
    let long = "a".repeat(500);
    assert_eq!(
        browser
            .get(&format!("/api/house/character.png?code={long}"))
            .await
            .code(),
        "malformed"
    );

    // Rooms that are not there.
    assert_eq!(browser.get("/api/house/rooms/7").await.code(), "no_room");
    assert_eq!(browser.get("/api/house/rooms/-1").await.code(), "malformed");
    assert_eq!(browser.get("/api/house/rooms/x").await.code(), "malformed");
    // Nothing here writes.
    let write = browser
        .send(
            Request::post("/api/house/rooms/0")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::ORIGIN, PUBLIC_URL)
                .header("sec-fetch-site", "same-origin")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
    assert!(write.status.is_client_error(), "{}", write.status);
}

fn urlencode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[tokio::test]
async fn the_catalogue_and_the_sprite_sheet_are_served_as_they_are_and_cached() {
    let Some(site) = Site::new("the_catalogue_and_the_sprite_sheet").await else {
        return;
    };
    let mut browser = site.browser();
    let content = browser.get("/api/house/content").await;
    assert_eq!(content.status, StatusCode::OK);
    assert_eq!(content.header("cache-control"), "public, max-age=3600");
    let tag = content.header("etag").to_owned();
    assert!(!tag.is_empty());
    let catalog = content.json();
    let furniture = catalog["furniture"].as_array().expect("furniture");
    assert_eq!(furniture.len(), Content::get().furniture.len());
    let window = furniture
        .iter()
        .find(|f| f["id"] == "pebblepane_window")
        .expect("a window");
    assert_eq!(window["layer"], "wall");
    assert!(window["variants"].as_array().unwrap().iter().any(|v| v == "sand"));
    assert!(catalog["variant_ramps"]["sand"].is_array());
    assert_eq!(catalog["base_ramp"].as_array().unwrap().len(), 5);
    assert_eq!(catalog["tiers"].as_array().unwrap().len(), 6);
    assert!(
        catalog["assets"]["atlas_png"]
            .as_str()
            .unwrap()
            .starts_with("/assets/atlas.png?v="),
        "the sheet is named with its version"
    );
    let again = browser.get_with("/api/house/content", "if-none-match", &tag).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);

    let png = browser.get("/assets/atlas.png").await;
    assert_eq!(png.status, StatusCode::OK);
    assert_eq!(png.header("content-type"), "image/png");
    assert_eq!(png.header("cache-control"), "public, max-age=86400");
    assert_eq!(png.body.as_ref(), ATLAS_PNG, "the bytes the renderer was built from");
    let png_tag = png.header("etag").to_owned();
    assert_eq!(
        browser
            .get_with("/assets/atlas.png", "if-none-match", &png_tag)
            .await
            .status,
        StatusCode::NOT_MODIFIED
    );
    let index = browser.get("/assets/atlas.json").await;
    assert_eq!(index.body.as_ref(), ATLAS_JSON.as_bytes());
    assert_eq!(index.header("content-type"), "application/json");
}

#[tokio::test]
async fn visitors_see_unlocked_houses_and_locked_doors_stay_shut() {
    let Some(site) = Site::new("visitors_see_unlocked_houses").await else {
        return;
    };
    let (host, guest) = (unique(), unique());
    site.move_in(host, 2, &SHOWCASE[5..9]).await;
    site.populate(host).await;
    site.app.db.remember_name(host, "Captain Crab").await.expect("a name");
    let mut browser = site.browser();
    browser.login(guest).await;

    let house = browser.get(&format!("/api/house/visit/{host}")).await;
    assert_eq!(house.status, StatusCode::OK, "{}", house.text());
    let house = house.json();
    assert_eq!(house["scope"], "visit");
    assert_eq!(house["owner"]["name"], "Captain Crab");
    assert_eq!(house["owner"]["you"], false);
    assert_eq!(house["rooms"].as_array().unwrap().len(), 2);
    assert_eq!(house["rooms"][0]["items"], 4);
    let room = browser.get(&format!("/api/house/visit/{host}/rooms/0")).await;
    assert_eq!(room.status, StatusCode::OK, "{}", room.text());
    assert_eq!(room.json()["placements"].as_array().unwrap().len(), 4);
    let png = browser
        .get(&format!("/api/house/visit/{host}/rooms/0/render.png"))
        .await;
    assert_eq!(png.status, StatusCode::OK);
    save_png("house", "visit", &png.body);
    assert_eq!(
        browser.get(&format!("/api/house/visit/{host}/rooms/5")).await.code(),
        "no_room"
    );

    // Locked: the same sentence `/house visit` answers with, and nothing of the rooms.
    site.app.db.set_house_locked(host, true).await.expect("lock");
    for path in [
        format!("/api/house/visit/{host}"),
        format!("/api/house/visit/{host}/rooms/0"),
        format!("/api/house/visit/{host}/rooms/0/render.png"),
    ] {
        let response = browser.get(&path).await;
        assert_eq!(response.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(response.code(), "locked");
        assert_eq!(
            response.json()["error"]["message"],
            "Captain Crab has locked their house."
        );
    }
    // The owner still walks in through their own door, and through the visit door too.
    let mut owner = site.browser();
    owner.login(host).await;
    assert_eq!(owner.get("/api/house/rooms/0").await.status, StatusCode::OK);
    let own = owner.get(&format!("/api/house/visit/{host}")).await;
    assert_eq!(own.status, StatusCode::OK);
    assert_eq!(own.json()["scope"], "own");

    // Strangers, nobodies and nonsense.
    assert_eq!(
        browser.get(&format!("/api/house/visit/{}", unique())).await.code(),
        "no_house"
    );
    assert_eq!(browser.get("/api/house/visit/0").await.code(), "malformed");
    assert_eq!(browser.get("/api/house/visit/abc").await.code(), "malformed");
    // Visiting from the web pays nobody: the guest has no visit record.
    let (visits,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visits WHERE host_id = $1")
        .bind(i64::try_from(host).unwrap())
        .fetch_one(&site.pool)
        .await
        .expect("a count");
    assert_eq!(visits, 0);
}

#[tokio::test]
async fn hidden_houses_and_server_plazas_are_not_shown_to_visitors() {
    let Some(site) = Site::new("hidden_houses_and_server_plazas").await else {
        return;
    };
    let (host, guest, guild) = (unique(), unique(), unique());
    site.move_in(host, 2, &SHOWCASE[5..9]).await;
    site.app.db.remember_name(host, "Captain Crab").await.expect("a name");
    let mut browser = site.browser();
    browser.login(guest).await;
    let paths = [
        format!("/api/house/visit/{host}"),
        format!("/api/house/visit/{host}/rooms/0"),
        format!("/api/house/visit/{host}/rooms/0/render.png"),
    ];
    for path in &paths {
        assert_eq!(browser.get(path).await.status, StatusCode::OK, "{path}");
    }

    // Hidden: refused before the lock is even looked at, the same sentence the bot answers with.
    site.app.db.set_privacy(host, Hide::House, true).await.expect("hide");
    site.app.db.set_house_locked(host, true).await.expect("lock");
    for path in &paths {
        let response = browser.get(path).await;
        assert_eq!(response.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(response.code(), "private", "{path}");
        assert_eq!(response.json()["error"]["message"], "This house is private.");
    }
    // The owner still sees their own house, hidden and locked, through either door.
    let mut owner = site.browser();
    owner.login(host).await;
    assert_eq!(owner.get("/api/house").await.status, StatusCode::OK);
    assert_eq!(owner.get(&paths[1]).await.status, StatusCode::OK);
    // Hidden comes before missing, as in the bot: a hidden player without a house looks no different.
    let hermit = unique();
    site.app.db.set_privacy(hermit, Hide::House, true).await.expect("hide");
    assert_eq!(
        browser.get(&format!("/api/house/visit/{hermit}")).await.code(),
        "private"
    );
    // Showing it again opens it up to the lock alone.
    site.app.db.set_privacy(host, Hide::House, false).await.expect("show");
    assert_eq!(browser.get(&paths[0]).await.code(), "locked");
    // Hiding the profile or the leaderboard says nothing about the house.
    site.app.db.set_privacy(host, Hide::Profile, true).await.expect("hide");
    site.app.db.set_house_locked(host, false).await.expect("unlock");
    assert_eq!(browser.get(&paths[0]).await.status, StatusCode::OK);

    // A server's plaza is a house owned by the server's id; nobody visits it by that id, not even a
    // member, and it looks like any id with no house.
    site.app.db.guild_settings(guild).await.expect("a server");
    site.move_in(guild, 2, &SHOWCASE[5..9]).await;
    for path in [
        format!("/api/house/visit/{guild}"),
        format!("/api/house/visit/{guild}/rooms/0"),
        format!("/api/house/visit/{guild}/rooms/0/render.png"),
    ] {
        let response = browser.get(&path).await;
        assert_eq!(response.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(response.code(), "no_house", "{path}");
    }
}

/// Leaves a furnished house for the Playwright screenshots: `cargo test -p pinchora-web --test house
/// seed_demo_house -- --ignored`.
#[tokio::test]
#[ignore = "seeds the test database for the browser tests"]
async fn seed_demo_house() {
    let Some(site) = Site::new("seed_demo_house").await else {
        return;
    };
    let (host, locked, empty) = (DEMO_HOST, DEMO_LOCKED, DEMO_EMPTY);
    site.move_in(host, 3, &SHOWCASE).await;
    site.populate(host).await;
    site.app.db.remember_name(host, "Captain Crab").await.expect("a name");
    sqlx::query("UPDATE houses SET likes = 12, name = 'Crab Cove' WHERE user_id = $1")
        .bind(i64::try_from(host).unwrap())
        .execute(&site.pool)
        .await
        .expect("likes");
    // A second room with a few things in it, so the tabs have something to show.
    for (item, x, y) in [
        ("patchpillow_bed__cream", 2, 3),
        ("stillstep_nightstand__cream", 4, 3),
        ("pebblecap_lamp__sand", 4, 3),
        ("rivetgrid_rug__sand", 1, 1),
    ] {
        sqlx::query(
            "INSERT INTO placements (user_id, room_idx, item_id, x, y, flipped) VALUES ($1, 1, $2, $3, $4, false)",
        )
        .bind(i64::try_from(host).unwrap())
        .bind(item)
        .bind(x as i16)
        .bind(y as i16)
        .execute(&site.pool)
        .await
        .expect("placement");
    }
    site.move_in(locked, 1, &[]).await;
    site.app.db.set_house_locked(locked, true).await.expect("lock");
    site.app.db.remember_name(locked, "Shy Shrimp").await.expect("a name");
    site.move_in(empty, 1, &[]).await;
    site.app
        .db
        .grant_item(empty, "plainwhorl_chair__sand", 3)
        .await
        .expect("editor stock");
}

const DEMO_HOST: u64 = 770_000_000_000_000_001;
const DEMO_LOCKED: u64 = 770_000_000_000_000_002;
const DEMO_EMPTY: u64 = 770_000_000_000_000_003;

#[path = "house_edit/mod.rs"]
mod house_edit;

#[tokio::test]
async fn reef_loop_house_snapshot_is_session_scoped_and_respects_house_privacy() {
    let Some(site) = Site::new("reef_loop_house_snapshot").await else {
        return;
    };
    let owner = unique();
    let visitor = unique();
    site.move_in(owner, 1, &[]).await;
    site.app.db.reef_state(owner).await.unwrap();
    sqlx::query("INSERT INTO reef_aquarium(user_id,slot,fish_id,size_cm) VALUES($1,1,'toaststripe_minnow',5)")
        .bind(owner as i64)
        .execute(&site.pool)
        .await
        .unwrap();
    let mut browser = site.browser();
    assert_eq!(browser.get("/api/house").await.status, StatusCode::UNAUTHORIZED);
    browser.login(owner).await;
    let own = browser.get("/api/house").await.json();
    assert_eq!(own["reef_loop"]["fish"].as_array().unwrap().len(), 1);
    assert!(own["reef_loop"]["garden"].is_object());
    browser.login(visitor).await;
    let guest = browser.get(&format!("/api/house/visit/{owner}")).await.json();
    assert!(guest["reef_loop"]["garden"].is_null());
    assert_eq!(guest["reef_loop"]["fish"][0]["id"], "toaststripe_minnow");
    site.app.db.set_privacy(owner, Hide::House, true).await.unwrap();
    assert_eq!(
        browser.get(&format!("/api/house/visit/{owner}")).await.status,
        StatusCode::FORBIDDEN
    );
}
