//! Temp voice through the real handlers: a member joins a creator channel and gets a room, runs it with
//! `/voice` and the panel, and the last one out deletes it.
//!
//! The common mock Discord only knows messages, roles and member lookups; temp voice also makes, changes
//! and deletes channels and moves members, so this file stands up a mock of its own for those routes and
//! keeps a small world behind it (the channels, who sits where). Handlers see it through the same twilight
//! client as ever.

use super::*;

const CONNECT: u64 = 1 << 20;
const VIEW_CHANNEL: u64 = 1 << 10;
const MANAGE_CHANNELS: u64 = 1 << 4;
const MOVE_MEMBERS: u64 = 1 << 24;

// These mocks use separate Discord worlds but share the same real Redis pending-cleanup set.
// Keep their sweeps from deleting another test's mock-only channel markers.
static TEMP_VOICE_TESTS: std::sync::OnceLock<Arc<tokio::sync::Mutex<()>>> = std::sync::OnceLock::new();

// /World

/// What the mock Discord holds for one test.
#[derive(Default)]
struct World {
    /// Every channel it knows, as Discord's JSON: creators, categories, rooms and threads.
    channels: HashMap<u64, Value>,
    /// Who sits in which voice channel, as Discord would say.
    voice: HashMap<u64, u64>,
    /// Channel and member calls in the order they came: method, path, body.
    log: Vec<(String, String, Value)>,
    /// Messages posted to or edited in channels.
    posts: Vec<Post>,
    /// Webhook calls an interaction made.
    webhooks: Vec<Call>,
    /// Members the bot moved, as (user, channel): a real Discord sends each one a voice event.
    moved: Vec<(u64, u64)>,
    /// A call that answers an error instead: the call's name, then Discord's error code.
    refuse: HashMap<&'static str, u64>,
    /// Permission bits the bot holds as a role.
    bot_permissions: u64,
    /// Threads and their members, as (thread, user).
    thread_members: Vec<(u64, u64)>,
    /// Users who are bots, for a picker transfer's user lookup.
    bot_users: Vec<u64>,
    member_roles: HashMap<u64, Vec<u64>>,
    extra_roles: Vec<Value>,
    voice_delay: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    review_gate: Option<Arc<tokio::sync::Barrier>>,
}

type Shared = Arc<Mutex<World>>;

fn error_json(code: u64, message: &str) -> String {
    json!({ "code": code, "message": message }).to_string()
}

fn refused(world: &World, call: &'static str) -> Option<(StatusCode, String)> {
    let code = *world.refuse.get(call)?;
    let status = if code == 50035 {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::FORBIDDEN
    };
    Some((status, error_json(code, "refused by the mock")))
}

async fn voice_discord(
    State(world): State<Shared>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> (StatusCode, String) {
    let path = uri.path().to_owned();
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let number = |part: &str| part.parse::<u64>().unwrap_or(0);
    let (json_body, files) = request_body(&headers, &body);
    let voice_delay = if path.contains("/voice-states/") {
        world.lock().unwrap().voice_delay.clone()
    } else {
        None
    };
    if let Some((started, release)) = voice_delay {
        started.notify_one();
        release.notified().await;
    }
    let gate = { world.lock().unwrap().review_gate.clone() };
    if let Some(gate) = gate {
        let rename = method == Method::PATCH && json_body.get("name").is_some();
        let overwrite = method == Method::PUT && path.contains("/permissions/") && json_body["type"] == json!(0);
        if rename || overwrite {
            let _ = tokio::time::timeout(std::time::Duration::from_millis(100), gate.wait()).await;
        }
    }
    let mut w = world.lock().unwrap();
    if path.contains("/voice-states/")
        && let Some(answer) = refused(&w, "voice")
    {
        return answer;
    }
    let note = |w: &mut World| w.log.push((method.to_string(), path.clone(), json_body.clone()));
    let gone = || (StatusCode::NOT_FOUND, error_json(10003, "Unknown Channel"));
    match (&method, parts.as_slice()) {
        (&Method::POST, [.., "guilds", guild, "channels"]) => {
            note(&mut w);
            if let Some(answer) = refused(&w, "create") {
                return answer;
            }
            let id = unique();
            let mut channel = json_body.clone();
            channel["id"] = json!(id.to_string());
            channel["guild_id"] = json!(guild);
            channel["position"] = json!(1);
            w.channels.insert(id, channel.clone());
            (StatusCode::OK, channel.to_string())
        }
        (&Method::GET, [.., "channels", id]) => {
            if let Some(answer) = refused(&w, "read") {
                return answer;
            }
            match w.channels.get(&number(id)) {
                Some(channel) => (StatusCode::OK, channel.to_string()),
                None => gone(),
            }
        }
        (&Method::PATCH, [.., "channels", id]) => {
            note(&mut w);
            if let Some(answer) = refused(&w, "edit") {
                return answer;
            }
            let Some(channel) = w.channels.get_mut(&number(id)) else {
                return gone();
            };
            for key in ["name", "user_limit"] {
                if let Some(value) = json_body.get(key) {
                    channel[key] = value.clone();
                }
            }
            (StatusCode::OK, channel.to_string())
        }
        (&Method::DELETE, [.., "channels", id]) => {
            note(&mut w);
            if let Some(answer) = refused(&w, "delete") {
                return answer;
            }
            if w.channels.get(&number(id)).is_some_and(|c| c["type"] == json!(12))
                && let Some(answer) = refused(&w, "thread_delete")
            {
                return answer;
            }
            match w.channels.remove(&number(id)) {
                Some(channel) => (StatusCode::OK, channel.to_string()),
                None => gone(),
            }
        }
        (&Method::PUT | &Method::DELETE, [.., "channels", id, "permissions", target]) => {
            note(&mut w);
            if let Some(answer) = refused(&w, "overwrite") {
                return answer;
            }
            let (id, target) = (number(id), number(target));
            let Some(channel) = w.channels.get_mut(&id) else {
                return gone();
            };
            let mut list: Vec<Value> = channel["permission_overwrites"].as_array().cloned().unwrap_or_default();
            list.retain(|o| o["id"] != json!(target.to_string()));
            if method == Method::PUT {
                list.push(json!({
                    "id": target.to_string(),
                    "type": json_body["type"],
                    "allow": json_body["allow"].as_str().unwrap_or("0"),
                    "deny": json_body["deny"].as_str().unwrap_or("0"),
                }));
            }
            channel["permission_overwrites"] = Value::Array(list);
            (StatusCode::NO_CONTENT, String::new())
        }
        (&Method::PATCH, [.., "guilds", _, "members", user]) => {
            note(&mut w);
            if let Some(answer) = refused(&w, "move") {
                return answer;
            }
            let user = number(user);
            if !w.voice.contains_key(&user) {
                return (
                    StatusCode::BAD_REQUEST,
                    error_json(40032, "Target user is not connected to voice."),
                );
            }
            match json_body["channel_id"].as_str().map(number) {
                Some(channel) => {
                    w.voice.insert(user, channel);
                    w.moved.push((user, channel));
                }
                None => {
                    w.voice.remove(&user);
                    w.moved.push((user, 0));
                }
            }
            (StatusCode::OK, member_json(user, &[]).to_string())
        }
        (&Method::GET, [.., "guilds", guild, "voice-states", user]) => match w.voice.get(&number(user)) {
            Some(channel) => (
                StatusCode::OK,
                json!({
                    "guild_id": guild, "channel_id": channel.to_string(), "user_id": user,
                    "session_id": "sim", "deaf": false, "mute": false, "self_deaf": false,
                    "self_mute": false, "self_video": false, "suppress": false,
                    "request_to_speak_timestamp": null,
                })
                .to_string(),
            ),
            None => (StatusCode::NOT_FOUND, error_json(10065, "Unknown Voice State")),
        },
        (&Method::GET, [.., "guilds", _, "members", user]) => {
            let roles = if number(user) == APPLICATION_ID {
                vec![BOT_ROLE_ID]
            } else {
                w.member_roles.get(&number(user)).cloned().unwrap_or_default()
            };
            (StatusCode::OK, member_json(number(user), &roles).to_string())
        }
        (&Method::GET, [.., "guilds", guild, "roles"]) => {
            let mut bot = role_json(BOT_ROLE_ID, "Pinchora", BOT_ROLE_POSITION, true);
            bot["permissions"] = json!(w.bot_permissions.to_string());
            let everyone = role_json(number(guild), "@everyone", 0, false);
            let mut roles = vec![bot, everyone];
            roles.extend(w.extra_roles.iter().cloned());
            (StatusCode::OK, json!(roles).to_string())
        }
        (&Method::GET, [.., "guilds", guild]) => (StatusCode::OK, guild_json(number(guild)).to_string()),
        (&Method::GET, [.., "users", user]) => {
            let id = number(user);
            (
                StatusCode::OK,
                person_json(id, "member", None, w.bot_users.contains(&id)).to_string(),
            )
        }
        (&Method::POST, [.., "channels", channel, "messages"]) => {
            let (channel, id) = (number(channel), unique());
            if let Some(answer) = refused(&w, "post") {
                return answer;
            }
            w.posts.push(Post {
                method: method.clone(),
                channel,
                message: id,
                body: json_body.clone(),
                files,
            });
            (StatusCode::OK, message_json(channel, id).to_string())
        }
        (&Method::PATCH, [.., "channels", channel, "messages", id]) => {
            let (channel, id) = (number(channel), number(id));
            w.posts.push(Post {
                method: method.clone(),
                channel,
                message: id,
                body: json_body.clone(),
                files,
            });
            (StatusCode::OK, message_json(channel, id).to_string())
        }
        (&Method::DELETE, [.., "channels", _, "messages", _]) => (StatusCode::NO_CONTENT, String::new()),
        (&Method::POST, [.., "channels", parent, "threads"]) => {
            note(&mut w);
            if let Some(answer) = refused(&w, "thread") {
                return answer;
            }
            let id = unique();
            let thread = json!({
                "id": id.to_string(), "type": 12, "guild_id": "1", "name": json_body["name"],
                "parent_id": parent,
            });
            w.channels.insert(id, thread.clone());
            (StatusCode::OK, thread.to_string())
        }
        (&Method::PUT, [.., "channels", thread, "thread-members", user]) => {
            w.thread_members.push((number(thread), number(user)));
            (StatusCode::NO_CONTENT, String::new())
        }
        (&Method::DELETE, [.., "channels", thread, "thread-members", user]) => {
            let (thread, user) = (number(thread), number(user));
            w.thread_members.retain(|m| *m != (thread, user));
            (StatusCode::NO_CONTENT, String::new())
        }
        _ if method != Method::GET && path.contains("/webhooks/") => {
            w.webhooks.push(Call {
                method: method.clone(),
                path: path.clone(),
                body: json_body,
                files,
            });
            (StatusCode::OK, "{}".to_owned())
        }
        _ => (StatusCode::NOT_FOUND, "{}".to_owned()),
    }
}

// /Harness

/// The real [`App`] on the test databases, with Discord replaced by [`voice_discord`].
struct Voice {
    _test_guard: tokio::sync::OwnedMutexGuard<()>,
    app: Arc<App>,
    world: Shared,
    guild: u64,
    name: &'static str,
    step: usize,
    /// Whether the invoker has Manage Server.
    manage: bool,
    extra_permissions: u64,
    /// The channel interactions come from.
    here: u64,
    /// The invoker's Discord client language.
    locale: &'static str,
}

impl Voice {
    async fn new(name: &'static str) -> Option<Self> {
        let (Ok(database), Ok(redis)) = (
            std::env::var("PINCHORA_TEST_DATABASE_URL"),
            std::env::var("PINCHORA_TEST_REDIS_URL"),
        ) else {
            eprintln!("{name}: skipped, set PINCHORA_TEST_DATABASE_URL and PINCHORA_TEST_REDIS_URL");
            return None;
        };
        let test_guard = TEMP_VOICE_TESTS
            .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
            .lock_owned()
            .await;
        init_logging();
        let db = Db::connect_waiting(&database, 5, std::time::Duration::from_secs(60))
            .await
            .expect("connect to the database");
        db.migrate().await.expect("migrate");
        let cache = Cache::connect(&redis).await.expect("connect to redis");
        let world = Shared::new(Mutex::new(World {
            bot_permissions: MANAGE_CHANNELS | MOVE_MEMBERS | (1 << 28),
            ..Default::default()
        }));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind the mock");
        let mock = listener.local_addr().expect("mock address");
        let router = Router::new().fallback(voice_discord).with_state(world.clone());
        tokio::spawn(axum::serve(listener, router).into_future());
        let rest = Arc::new(
            Client::builder()
                .token("Bot sim".into())
                .proxy(mock.to_string(), true)
                .ratelimiter(None)
                .build(),
        );
        let mut app = App::new(rest.clone(), rest, Id::new(APPLICATION_ID), db, cache, "sim".into());
        app.message_content = true;
        app.web = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{mock}")).expect("proxy"))
            .build()
            .expect("http client");
        let guild = unique();
        // A fresh server for the test: the module cache of an earlier run cannot be seen.
        pinchora_commands::forget_temp_voice(guild).await;
        Some(Self {
            _test_guard: test_guard,
            app: Arc::new(app),
            world,
            guild,
            name,
            step: 0,
            manage: false,
            extra_permissions: 0,
            here: CHANNEL_ID,
            locale: "en-US",
        })
    }

    /// Adds a channel to the mock (type 2 voice, 4 category, 0 text) and returns its id.
    fn channel(&self, kind: u8, name: &str, parent: Option<u64>) -> u64 {
        let id = unique();
        self.world.lock().unwrap().channels.insert(
            id,
            json!({
                "id": id.to_string(), "type": kind, "guild_id": self.guild.to_string(), "name": name,
                "parent_id": parent.map(|p| p.to_string()), "permission_overwrites": [], "position": 0,
            }),
        );
        id
    }

    /// `data.resolved` for options that name these channels as (id, type).
    fn resolved(&self, channels: &[(u64, u8)]) -> Value {
        let world = self.world.lock().unwrap();
        json!({
            "channels": channels
                .iter()
                .map(|(id, kind)| {
                    let known = world.channels.get(id);
                    (
                        id.to_string(),
                        json!({
                            "id": id.to_string(), "type": kind, "permissions": "0",
                            "name": known.map_or("sim-channel", |c| c["name"].as_str().unwrap_or("sim-channel")),
                            "parent_id": known.map_or(Value::Null, |c| c["parent_id"].clone()),
                        }),
                    )
                })
                .collect::<serde_json::Map<_, _>>(),
        })
    }

    fn users(&self, users: &[u64]) -> Value {
        json!({ "users": users.iter().map(|u| (u.to_string(), user_json(*u))).collect::<serde_json::Map<_, _>>() })
    }

    // /Voice events

    async fn deliver(&self, user: u64, name: &str, channel: Option<u64>) {
        let event: VoiceStateUpdate = serde_json::from_value(json!({
            "guild_id": self.guild.to_string(),
            "channel_id": channel.map(|c| c.to_string()),
            "user_id": user.to_string(),
            "member": {
                "user": person_json(user, name, Some(name), false),
                "roles": [], "joined_at": "2024-01-01T00:00:00.000000+00:00",
                "deaf": false, "mute": false, "flags": 0, "pending": false,
            },
            "session_id": "sim", "deaf": false, "mute": false, "self_deaf": false, "self_mute": false,
            "self_stream": false, "self_video": false, "suppress": false, "request_to_speak_timestamp": null,
        }))
        .expect("voice state JSON");
        println!("\n== {}: {name} voice {channel:?}", self.name);
        ERRORS.with_borrow_mut(Vec::clear);
        on_voice_state_update(self.app.clone(), event)
            .await
            .expect("the voice handler failed");
        let errors = ERRORS.take();
        assert!(errors.is_empty(), "the voice handler failed:\n{}", errors.join("\n"));
    }

    /// `user` joins a voice channel (or moves to it).
    async fn join(&self, user: u64, name: &str, channel: u64) {
        self.world.lock().unwrap().voice.insert(user, channel);
        self.deliver(user, name, Some(channel)).await;
    }

    /// `user` leaves voice.
    async fn leave(&self, user: u64, name: &str) {
        self.world.lock().unwrap().voice.remove(&user);
        self.deliver(user, name, None).await;
    }

    /// Delivers the voice events Discord would send for every member the bot moved or disconnected.
    async fn echo(&self, names: &HashMap<u64, &str>) {
        let moved = std::mem::take(&mut self.world.lock().unwrap().moved);
        for (user, channel) in moved {
            let name = names.get(&user).copied().unwrap_or("someone");
            self.deliver(user, name, (channel != 0).then_some(channel)).await;
        }
    }

    // /What the mock saw

    /// The bodies of the channels created since the last call.
    fn created(&self) -> Vec<Value> {
        let mut world = self.world.lock().unwrap();
        let (made, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut world.log)
            .into_iter()
            .partition(|(method, path, _)| method == "POST" && path.ends_with("/channels"));
        world.log = rest;
        made.into_iter().map(|(_, _, body)| body).collect()
    }

    /// The channels deleted so far, as ids.
    fn deleted(&self) -> Vec<u64> {
        self.world
            .lock()
            .unwrap()
            .log
            .iter()
            .filter(|(m, _, _)| m == "DELETE")
            .filter_map(|(_, p, _)| {
                let parts: Vec<&str> = p.trim_matches('/').split('/').collect();
                match parts.as_slice() {
                    [.., "channels", id] => id.parse().ok(),
                    _ => None,
                }
            })
            .collect()
    }

    /// The messages posted or edited since the last call.
    fn posts(&self) -> Vec<Post> {
        std::mem::take(&mut self.world.lock().unwrap().posts)
    }

    /// A channel's JSON as the mock holds it now.
    fn channel_json(&self, id: u64) -> Option<Value> {
        self.world.lock().unwrap().channels.get(&id).cloned()
    }

    /// `(allow, deny)` bits of a channel's overwrite for `target`.
    fn overwrite(&self, channel: u64, target: u64) -> Option<(u64, u64)> {
        let json = self.channel_json(channel)?;
        json["permission_overwrites"].as_array()?.iter().find_map(|o| {
            (o["id"] == json!(target.to_string())).then(|| {
                let bits = |key: &str| o[key].as_str().and_then(|b| b.parse().ok()).unwrap_or(0u64);
                (bits("allow"), bits("deny"))
            })
        })
    }

    fn refuse(&self, call: &'static str, code: u64) {
        self.world.lock().unwrap().refuse.insert(call, code);
    }

    fn allow(&self, call: &'static str) {
        self.world.lock().unwrap().refuse.remove(call);
    }

    /// The room `user` owns in this server, by its channel.
    async fn room_of(&self, user: u64) -> u64 {
        let rooms = self.app.db.rooms_owned_by(self.guild, user).await.expect("rooms");
        rooms.first().expect("the member owns no room").channel_id as u64
    }

    /// Lets `user` ask for a room again right away (a member is held back for a few seconds between two).
    async fn cool_down(&self, user: u64) {
        let key = format!("tvoice:make:{}:{user}", self.guild);
        self.app
            .cache
            .release_cooldown(&key)
            .await
            .expect("release the cooldown");
    }

    // /Interactions

    fn interaction(&self, user: u64, kind: u8, data: Value, on: Option<&Reply>) -> Interaction {
        let id = unique();
        let mut interaction = json!({
            "id": id.to_string(),
            "application_id": APPLICATION_ID.to_string(),
            "type": kind,
            "token": format!("sim-{id}"),
            "version": 1,
            "guild_id": self.guild.to_string(),
            "channel": { "id": self.here.to_string(), "type": 2, "guild_id": self.guild.to_string() },
            "locale": self.locale,
            "guild_locale": "en-US",
            "context": 0,
            "attachment_size_limit": 26_214_400,
            "authorizing_integration_owners": {},
            "entitlements": [],
            "member": {
                "user": person_json(user, &format!("sim{user}"), Some("Sim"), false),
                "roles": [], "joined_at": "2024-01-01T00:00:00.000000+00:00",
                "deaf": false, "mute": false, "flags": 0,
                "permissions": (self.extra_permissions | if self.manage { MANAGE_GUILD } else { 0 }).to_string(),
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
        let token = interaction.token.clone();
        self.world.lock().unwrap().webhooks.clear();
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
        let calls = std::mem::take(&mut self.world.lock().unwrap().webhooks);
        let reply = Reply::build(first, on, calls, &token);
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
                "step {step} ({label}): opened a form, which a defer would break"
            );
        }
        println!("\n== {} step {step:02}: {label}\n{reply}", self.name);
        reply
    }

    async fn command(&mut self, user: u64, options: Value, resolved: Value) -> Reply {
        let data = json!({
            "id": unique().to_string(), "name": "voice", "type": 1, "options": options, "resolved": resolved,
        });
        self.run(format!("/voice {options}"), user, 2, data, None).await
    }

    /// `/voice <sub>` with no options.
    async fn say(&mut self, user: u64, sub_name: &str) -> Reply {
        self.command(user, json!([sub(sub_name, vec![])]), json!({})).await
    }

    async fn click(&mut self, user: u64, on: &Reply, label: &str) -> Reply {
        let button = on.button(label);
        assert!(!button.disabled, "button {label:?} is disabled");
        let id = button.custom_id.clone().expect("button without a custom id");
        let data = json!({ "custom_id": id, "component_type": 2, "values": [] });
        self.run(format!("click [{label}]"), user, 3, data, Some(on)).await
    }

    /// Picks `value` in the user select whose custom id starts with `menu`.
    async fn pick_user(&mut self, user: u64, on: &Reply, menu: &str, value: u64) -> Reply {
        let id = on.menu(menu).custom_id.clone();
        let data = json!({ "custom_id": id, "component_type": 5, "values": [value.to_string()] });
        self.run(format!("pick user {value} in {id}"), user, 3, data, Some(on))
            .await
    }

    async fn submit(&mut self, user: u64, form: &Reply, fields: &[(&str, &str)]) -> Reply {
        let (custom_id, _) = form.modal.clone().expect("the step opened no form");
        let components: Vec<Value> = fields
            .iter()
            .enumerate()
            .map(|(n, (id, value))| {
                json!({ "type": 18, "id": n * 2 + 1,
                        "component": { "type": 4, "id": n * 2 + 2, "custom_id": id, "value": value } })
            })
            .collect();
        let data = json!({ "custom_id": custom_id, "components": components });
        self.run(format!("submit {custom_id} {fields:?}"), user, 5, data, None)
            .await
    }

    /// Makes `channel` a creator, as a manager, through the command. Rooms go under `category`.
    async fn add_creator(&mut self, admin: u64, channel: u64, extra: Vec<Value>) -> Reply {
        let was = self.manage;
        self.manage = true;
        let mut options = vec![channel_option("channel", channel)];
        options.extend(extra);
        let resolved = self.resolved(&[(channel, 2)]);
        let reply = self
            .command(admin, json!([group("creators", sub("add", options))]), resolved)
            .await;
        self.manage = was;
        reply
    }
}

/// The names a test gives its members, for the voice events the bot's moves cause.
fn names(list: &[(u64, &'static str)]) -> HashMap<u64, &'static str> {
    list.iter().copied().collect()
}

fn bits_of(post: &Post) -> Vec<String> {
    post.body["components"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| row["components"].as_array().cloned().unwrap_or_default())
        .filter_map(|c| c["custom_id"].as_str().map(str::to_owned))
        .collect()
}

// /Tests

#[tokio::test]
async fn joining_a_creator_makes_a_room_and_the_last_one_out_deletes_it() {
    let Some(mut v) = Voice::new("tempvoice_room_lifecycle").await else {
        return;
    };
    let (admin, alice, bob) = (unique(), unique(), unique());
    let category = v.channel(4, "Rooms", None);
    let creator = v.channel(2, "Join to create", Some(category));
    let added = v
        .add_creator(
            admin,
            creator,
            vec![text_option("template", "{user}'s room"), int_option("limit", 4)],
        )
        .await;
    assert!(added.content().contains("is a creator now"), "{}", added.content());
    let row = v.app.db.voice_creator(v.guild, creator).await.unwrap().expect("saved");
    assert_eq!((row.category_id, row.default_limit), (Some(category as i64), 4));

    let who = names(&[(alice, "Alice"), (bob, "Bob")]);
    v.join(alice, "Alice", creator).await;
    let made = v.created();
    assert_eq!(made.len(), 1, "one room: {made:?}");
    assert_eq!(made[0]["name"], "Alice's room");
    assert_eq!(
        (made[0]["type"].clone(), made[0]["user_limit"].clone()),
        (json!(2), json!(4))
    );
    assert_eq!(made[0]["parent_id"], json!(category.to_string()));
    let room = v.room_of(alice).await;
    assert_eq!(
        v.world.lock().unwrap().voice[&alice],
        room,
        "Alice was moved into her room"
    );
    // She is let in by name; nobody else is shut out (the room starts open).
    let bits = v.overwrite(room, alice).expect("her overwrite");
    assert_eq!(bits.0 & (VIEW_CHANNEL | CONNECT), VIEW_CHANNEL | CONNECT);
    assert!(v.overwrite(room, v.guild).is_none());
    // The panel sits in the room's own chat: two rows of eight buttons.
    let posts = v.posts();
    let panel = posts.iter().find(|p| p.channel == room).expect("a panel in the room");
    assert_eq!(
        bits_of(panel),
        ["vc:lk", "vc:ul", "vc:lm", "vc:rn", "vc:kk", "vc:tr", "vc:hd", "vc:cl"]
    );
    assert!(
        panel.description().contains(&format!("<@{alice}>")),
        "{}",
        panel.description()
    );
    assert!(
        v.app
            .db
            .temp_room(room)
            .await
            .unwrap()
            .unwrap()
            .panel_message_id
            .is_some(),
        "the panel is remembered so it can be refreshed"
    );

    // Discord tells us about the move: nothing more to do. Bob walks in, and Alice walks out.
    v.echo(&who).await;
    assert!(v.created().is_empty(), "the echo of the move made no second room");
    v.join(bob, "Bob", room).await;
    v.leave(alice, "Alice").await;
    assert!(v.deleted().is_empty(), "Bob is still inside");
    assert!(v.app.db.temp_room(room).await.unwrap().is_some());
    v.leave(bob, "Bob").await;
    sqlx::query("UPDATE temp_voice_channels SET created_at = now() - interval '5 minutes' WHERE channel_id = $1")
        .bind(room as i64)
        .execute(v.app.db.pool())
        .await
        .unwrap();
    pinchora_commands::tempvoice_tick(&v.app).await.unwrap();
    assert_eq!(v.deleted(), [room], "the empty room is removed by the fenced sweep");
    assert!(v.app.db.temp_room(room).await.unwrap().is_none());
    assert!(pinchora_commands::temp_voice_members(&v.app, room).await == 0);
}

#[tokio::test]
async fn a_member_who_already_has_a_room_is_taken_back_to_it() {
    let Some(mut v) = Voice::new("tempvoice_one_room_each").await else {
        return;
    };
    let (admin, alice, bob) = (unique(), unique(), unique());
    let creator = v.channel(2, "Join to create", None);
    v.add_creator(admin, creator, vec![]).await;
    let who = names(&[(alice, "Alice"), (bob, "Bob")]);
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.echo(&who).await;
    v.join(bob, "Bob", room).await;
    v.created();
    // She goes to the lobby again (a click on the creator) while Bob keeps the room alive: she is sent
    // back to it, and no second room is made.
    v.cool_down(alice).await;
    v.join(alice, "Alice", creator).await;
    assert!(v.created().is_empty(), "no second room");
    assert_eq!(v.world.lock().unwrap().moved.last(), Some(&(alice, room)));
    assert_eq!(v.app.db.rooms_owned_by(v.guild, alice).await.unwrap().len(), 1);
    // Held back from asking again and again: a second click inside the cooldown does nothing at all.
    v.join(alice, "Alice", creator).await;
    assert!(v.created().is_empty());
}

#[tokio::test]
async fn an_owner_locks_hides_limits_and_renames_and_the_panel_follows() {
    let Some(mut v) = Voice::new("tempvoice_owner_controls").await else {
        return;
    };
    let (admin, alice, bob) = (unique(), unique(), unique());
    let category = v.channel(4, "Rooms", None);
    let creator = v.channel(2, "Join to create", Some(category));
    // The category keeps one thing private: @everyone may not speak there.
    v.world.lock().unwrap().channels.get_mut(&category).unwrap()["permission_overwrites"] =
        json!([{ "id": v.guild.to_string(), "type": 0, "allow": "0", "deny": (1u64 << 21).to_string() }]);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.posts();
    let everyone = v.guild;
    // The room started as its category is: the speak ban came along.
    assert_eq!(v.overwrite(room, everyone).map(|b| b.1), Some(1 << 21));

    let locked = v.say(alice, "lock").await;
    assert!(locked.content().contains("locked"), "{}", locked.content());
    let (allow, deny) = v.overwrite(room, everyone).expect("@everyone overwrite");
    assert_eq!(
        deny,
        CONNECT | (1 << 21),
        "connect is shut; the category's speak ban stays"
    );
    assert_eq!(allow, 0);
    let bot = v.overwrite(room, APPLICATION_ID).expect("the bot keeps its seat");
    assert_eq!(bot.0 & (VIEW_CHANNEL | CONNECT), VIEW_CHANNEL | CONNECT);
    assert!(v.app.db.temp_room(room).await.unwrap().unwrap().locked);
    let refreshed = v.posts();
    let panel = refreshed
        .iter()
        .find(|p| p.method == Method::PATCH)
        .expect("the panel is edited");
    assert!(panel.description().contains("Locked"), "{}", panel.description());

    // A failed category read must not clear its private permissions.
    v.refuse("read", 50013);
    let refused = v.say(alice, "unlock").await;
    assert!(refused.content().contains("View Channel"), "{}", refused.content());
    assert_eq!(v.overwrite(room, everyone).unwrap().1 & CONNECT, CONNECT);
    v.allow("read");
    let unlocked = v.say(alice, "unlock").await;
    assert!(unlocked.content().contains("open again"), "{}", unlocked.content());
    assert_eq!(
        v.overwrite(room, everyone).map(|b| b.1),
        Some(1 << 21),
        "unlocking goes back to what the category says, not to nothing"
    );

    let hidden = v.say(alice, "hide").await;
    assert!(hidden.content().contains("hidden"), "{}", hidden.content());
    assert_eq!(v.overwrite(room, everyone).unwrap().1 & VIEW_CHANNEL, VIEW_CHANNEL);
    v.say(alice, "show").await;
    assert_eq!(v.overwrite(room, everyone).unwrap().1 & VIEW_CHANNEL, 0);

    let limit = v
        .command(alice, json!([sub("limit", vec![int_option("people", 3)])]), json!({}))
        .await;
    assert!(limit.content().contains("3 people"), "{}", limit.content());
    assert_eq!(v.channel_json(room).unwrap()["user_limit"], 3);
    let free = v
        .command(alice, json!([sub("limit", vec![int_option("people", 0)])]), json!({}))
        .await;
    assert!(free.content().contains("no limit"), "{}", free.content());

    let named = v
        .command(
            alice,
            json!([sub("rename", vec![text_option("name", "  Study\nhall ")])]),
            json!({}),
        )
        .await;
    assert!(named.content().contains("**Study hall**"), "{}", named.content());
    assert_eq!(v.channel_json(room).unwrap()["name"], "Study hall");
    // The database keeps the cooldown even if Redis loses its short-lived key.
    v.app.cache.forget(&format!("vc:rn:{room}")).await.unwrap();
    // Discord allows two renames in ten minutes: the second waits.
    let again = v
        .command(
            alice,
            json!([sub("rename", vec![text_option("name", "Again")])]),
            json!({}),
        )
        .await;
    assert!(again.content().contains("Try again <t:"), "{}", again.content());
    assert_eq!(v.channel_json(room).unwrap()["name"], "Study hall");

    // Somebody else cannot run Alice's room, and a stranger to any room is told so.
    v.join(bob, "Bob", room).await;
    let refused = v.say(bob, "lock").await;
    assert!(
        refused.content().contains(&format!("<@{alice}>")),
        "{}",
        refused.content()
    );
    let outsider = unique();
    let nothing = v.say(outsider, "lock").await;
    assert!(nothing.content().contains("don't own one"), "{}", nothing.content());
}

#[tokio::test]
async fn the_first_one_in_claims_a_room_whose_owner_left_and_owners_hand_it_over() {
    let Some(mut v) = Voice::new("tempvoice_claim_and_transfer").await else {
        return;
    };
    let (admin, alice, bob, carol, dave, robot) = (unique(), unique(), unique(), unique(), unique(), unique());
    let creator = v.channel(2, "Join to create", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.join(bob, "Bob", room).await;
    v.join(carol, "Carol", room).await;

    // The owner is in the room: nothing to claim.
    let early = v.say(bob, "claim").await;
    assert!(early.content().contains("still in the room"), "{}", early.content());
    // Alice walks out; Bob is first to claim, and Carol finds the room taken.
    v.leave(alice, "Alice").await;
    let claimed = v.say(bob, "claim").await;
    assert!(claimed.content().contains("yours now"), "{}", claimed.content());
    let late = v.say(carol, "claim").await;
    assert!(late.content().contains("still in the room"), "{}", late.content());
    assert_eq!(
        v.app.db.temp_room(room).await.unwrap().unwrap().owner_id,
        Some(bob as i64)
    );
    // Somebody who is in no room has nothing to claim.
    let outsider = unique();
    let far = v.say(outsider, "claim").await;
    assert!(far.content().contains("don't own one"), "{}", far.content());

    // Bob hands the room over: to someone who is in it, never to a bot or to somebody elsewhere.
    let to_bot = v
        .command(
            bob,
            json!([sub("transfer", vec![user_option("user", robot)])]),
            json!({ "users": { robot.to_string(): person_json(robot, "robot", None, true) } }),
        )
        .await;
    assert!(to_bot.content().contains("bot can't own"), "{}", to_bot.content());
    let to_dave = v
        .command(
            bob,
            json!([sub("transfer", vec![user_option("user", dave)])]),
            v.users(&[dave]),
        )
        .await;
    assert!(
        to_dave.content().contains("isn't in your room"),
        "{}",
        to_dave.content()
    );
    let gave = v
        .command(
            bob,
            json!([sub("transfer", vec![user_option("user", carol)])]),
            v.users(&[carol]),
        )
        .await;
    assert!(gave.content().contains(&format!("<@{carol}>")), "{}", gave.content());
    assert_eq!(
        v.app.db.temp_room(room).await.unwrap().unwrap().owner_id,
        Some(carol as i64)
    );
    let not_any_more = v.say(bob, "lock").await;
    assert!(
        not_any_more.content().contains(&format!("<@{carol}>")),
        "{}",
        not_any_more.content()
    );
    // Alice comes back: the room is Carol's, and she runs nothing.
    v.join(alice, "Alice", room).await;
    let back = v.say(alice, "lock").await;
    assert!(back.content().contains(&format!("<@{carol}>")), "{}", back.content());
}

#[tokio::test]
async fn kicked_members_are_sent_out_and_stay_out_until_invited() {
    let Some(mut v) = Voice::new("tempvoice_kick_and_invite").await else {
        return;
    };
    let (admin, alice, pest) = (unique(), unique(), unique());
    let creator = v.channel(2, "Join to create", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.join(pest, "Pest", room).await;

    let kicked = v
        .command(
            alice,
            json!([sub("kick", vec![user_option("user", pest)])]),
            v.users(&[pest]),
        )
        .await;
    assert!(kicked.content().contains(&format!("<@{pest}>")), "{}", kicked.content());
    assert!(v.app.db.is_banned_from_room(room, pest).await.unwrap());
    assert_eq!(v.overwrite(room, pest).map(|b| b.1 & CONNECT), Some(CONNECT));
    assert!(
        !v.world.lock().unwrap().voice.contains_key(&pest),
        "the kicked member was disconnected"
    );
    let who = names(&[(pest, "Pest")]);
    v.echo(&who).await;

    // They come back anyway (Discord let them in: say the bot lacked Manage Roles): the bot sends them out.
    v.join(pest, "Pest", room).await;
    assert!(!v.world.lock().unwrap().voice.contains_key(&pest), "sent out again");
    v.echo(&who).await;
    // The owner cannot kick themselves, and a kick is always about someone else.
    let myself = v
        .command(
            alice,
            json!([sub("kick", vec![user_option("user", alice)])]),
            v.users(&[alice]),
        )
        .await;
    assert!(myself.content().contains("can't kick yourself"), "{}", myself.content());

    // An invite lifts the kick and lets them in even if the room is locked.
    v.say(alice, "lock").await;
    let invited = v
        .command(
            alice,
            json!([sub("invite", vec![user_option("user", pest)])]),
            v.users(&[pest]),
        )
        .await;
    assert!(
        invited.content().contains(&format!("<@{pest}>")),
        "{}",
        invited.content()
    );
    assert!(!v.app.db.is_banned_from_room(room, pest).await.unwrap());
    let bits = v.overwrite(room, pest).expect("their overwrite");
    assert_eq!((bits.0 & CONNECT, bits.1 & CONNECT), (CONNECT, 0));
    v.join(pest, "Pest", room).await;
    assert_eq!(v.world.lock().unwrap().voice.get(&pest), Some(&room), "they stay");
}

#[tokio::test]
async fn the_panel_opens_forms_and_pickers_for_the_owner_only() {
    let Some(mut v) = Voice::new("tempvoice_panel_clicks").await else {
        return;
    };
    let (admin, alice, bob) = (unique(), unique(), unique());
    let creator = v.channel(2, "Join to create", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.join(bob, "Bob", room).await;
    let panel = posted_reply(v.posts().iter().find(|p| p.channel == room).expect("a panel"));
    // The panel lives in the room's chat, so a click there means that room.
    v.here = room;

    // The owner's buttons.
    let locked = v.click(alice, &panel, "Lock").await;
    assert!(locked.content().contains("locked"), "{}", locked.content());
    let shown = v.click(alice, &panel, "Hide / show").await;
    assert!(shown.content().contains("hidden"), "{}", shown.content());
    let shown = v.click(alice, &panel, "Hide / show").await;
    assert!(
        shown.content().contains("visible again"),
        "the same button toggles: {}",
        shown.content()
    );

    // A form for the limit, then for the name.
    let form = v.click(alice, &panel, "Limit").await;
    assert_eq!(form.modal.as_ref().map(|m| m.0.clone()), Some(format!("vc:lm:{room}")));
    let done = v.submit(alice, &form, &[("limit", "6")]).await;
    assert!(done.content().contains("6 people"), "{}", done.content());
    let bad = v.submit(alice, &form, &[("limit", "lots")]).await;
    assert!(bad.content().contains("number from 0 to 99"), "{}", bad.content());
    let form = v.click(alice, &panel, "Rename").await;
    assert_eq!(form.modal.as_ref().map(|m| m.0.clone()), Some(format!("vc:rn:{room}")));
    let done = v.submit(alice, &form, &[("name", "Late night")]).await;
    assert!(done.content().contains("**Late night**"), "{}", done.content());
    // Asking again within five minutes opens no form: it says when.
    let waits = v.click(alice, &panel, "Rename").await;
    assert!(
        waits.modal.is_none() && waits.content().contains("Try again"),
        "{}",
        waits.content()
    );

    // A person is picked from a menu.
    let picker = v.click(alice, &panel, "Kick").await;
    assert!(picker.ephemeral);
    let out = v.pick_user(alice, &picker, "vo:k", bob).await;
    assert!(out.content().contains(&format!("<@{bob}>")), "{}", out.content());
    assert!(out.components.is_empty(), "the picker goes away once used");
    assert!(v.app.db.is_banned_from_room(room, bob).await.unwrap());

    // A bot chosen from the user picker cannot own a room.
    let robot = unique();
    v.world.lock().unwrap().bot_users.push(robot);
    let picker = v.click(alice, &panel, "Transfer").await;
    let id = picker.menu("vo:t").custom_id.clone();
    let data = json!({
        "custom_id": id,
        "component_type": 5,
        "values": [robot.to_string()],
        "resolved": { "users": { robot.to_string(): person_json(robot, "robot", None, true) } },
    });
    let no_bot = v.run("pick bot owner".into(), alice, 3, data, Some(&picker)).await;
    assert!(no_bot.content().contains("bot can't own"), "{}", no_bot.content());
    assert_eq!(
        v.app.db.temp_room(room).await.unwrap().unwrap().owner_id,
        Some(alice as i64)
    );

    // Bob presses buttons too, and the room is not his.
    let no = v.click(bob, &panel, "Lock").await;
    assert!(no.content().contains(&format!("<@{alice}>")), "{}", no.content());
    let no_form = v.click(bob, &panel, "Limit").await;
    assert!(
        no_form.modal.is_none() && no_form.content().contains(&format!("<@{alice}>")),
        "{}",
        no_form.content()
    );
    // A forged picker for a room of another server counts for nothing.
    let forged = json!({ "custom_id": format!("vo:k:{}", unique()), "component_type": 5, "values": [bob.to_string()] });
    let gone = v.run("forged picker".into(), bob, 3, forged, None).await;
    assert!(gone.content().contains("gone"), "{}", gone.content());
}

#[tokio::test]
async fn a_server_that_lacks_a_permission_hears_which_one() {
    let Some(mut v) = Voice::new("tempvoice_missing_permissions").await else {
        return;
    };
    let (admin, alice) = (unique(), unique());
    let creator = v.channel(2, "Join to create", None);
    // The bot has no permissions at all as a role: the setup warns.
    v.world.lock().unwrap().bot_permissions = 0;
    let added = v.add_creator(admin, creator, vec![]).await;
    for needed in ["Manage Channels", "Move Members", "Manage Roles"] {
        assert!(
            added.content().contains(needed),
            "the warning names {needed}: {}",
            added.content()
        );
    }
    v.world.lock().unwrap().bot_permissions = MANAGE_CHANNELS | MOVE_MEMBERS | (1 << 28);

    // Discord refuses to make the channel: the creator's chat says which permission is missing, to Alice.
    v.refuse("create", 50013);
    v.join(alice, "Alice", creator).await;
    let posts = v.posts();
    let notice = posts
        .iter()
        .find(|p| p.channel == creator)
        .expect("a notice in the creator's chat");
    assert!(
        notice.content().contains(&format!("<@{alice}>")),
        "{}",
        notice.content()
    );
    assert!(notice.content().contains("**Manage Channels**"), "{}", notice.content());
    assert!(v.app.db.rooms_owned_by(v.guild, alice).await.unwrap().is_empty());
    v.allow("create");

    // The room is made but the member cannot be moved: no empty room is left behind.
    v.refuse("move", 50013);
    v.leave(alice, "Alice").await;
    v.cool_down(alice).await;
    v.join(alice, "Alice", creator).await;
    let posts = v.posts();
    let notice = posts.iter().find(|p| p.channel == creator).expect("a notice");
    assert!(notice.content().contains("**Move Members**"), "{}", notice.content());
    assert!(v.app.db.rooms_owned_by(v.guild, alice).await.unwrap().is_empty());
    assert_eq!(v.deleted().len(), 1, "the room that could not be used was deleted");
    v.allow("move");

    // A full category: Discord's invalid form on the parent.
    let category = v.channel(4, "Full", None);
    let other = v.channel(2, "Join too", Some(category));
    v.add_creator(admin, other, vec![]).await;
    v.refuse("create", 50035);
    v.leave(alice, "Alice").await;
    v.cool_down(alice).await;
    v.join(alice, "Alice", other).await;
    let posts = v.posts();
    let notice = posts.iter().find(|p| p.channel == other).expect("a notice");
    assert!(notice.content().contains("category is full"), "{}", notice.content());
}

#[tokio::test]
async fn only_managers_set_creators_up_and_the_module_switch_stops_rooms() {
    let Some(mut v) = Voice::new("tempvoice_admin_and_module").await else {
        return;
    };
    let (admin, member, alice) = (unique(), unique(), unique());
    let creator = v.channel(2, "Join to create", None);
    let resolved = v.resolved(&[(creator, 2)]);
    let options = json!([group("creators", sub("add", vec![channel_option("channel", creator)]))]);
    let denied = v.command(member, options.clone(), resolved.clone()).await;
    assert!(denied.content().contains("Manage Server"), "{}", denied.content());
    assert!(v.app.db.voice_creators(v.guild).await.unwrap().is_empty());
    let listed = v
        .command(member, json!([group("creators", sub("list", vec![]))]), json!({}))
        .await;
    assert!(listed.content().contains("Manage Server"), "{}", listed.content());
    let removed = v
        .command(
            member,
            json!([group(
                "creators",
                sub("remove", vec![channel_option("channel", creator)])
            )]),
            resolved.clone(),
        )
        .await;
    assert!(removed.content().contains("Manage Server"), "{}", removed.content());

    // A bad template says what is wrong with it.
    v.manage = true;
    let bad = v
        .command(
            admin,
            json!([group(
                "creators",
                sub(
                    "add",
                    vec![channel_option("channel", creator), text_option("template", "{nick}")]
                )
            )]),
            resolved.clone(),
        )
        .await;
    assert!(bad.content().contains("{nick}"), "{}", bad.content());
    v.add_creator(admin, creator, vec![text_option("template", "Room {n} · {user}")])
        .await;
    let listed = v
        .command(admin, json!([group("creators", sub("list", vec![]))]), json!({}))
        .await;
    assert!(
        listed.content().contains(&format!("<#{creator}>")) && listed.content().contains("Room {n} · {user}"),
        "{}",
        listed.content()
    );
    v.manage = false;

    // With the module off, a creator makes nothing and /voice is refused.
    v.app
        .db
        .set_module(v.guild, pinchora_core::modules::Module::TempVoice, Some(false), None)
        .await
        .expect("switch the module off");
    v.app.announce_guild_change(v.guild).await;
    v.join(alice, "Alice", creator).await;
    assert!(v.created().is_empty(), "no room while the module is off");
    let off = v.say(alice, "lock").await;
    assert!(off.content().contains("turned off"), "{}", off.content());
    assert!(off.ephemeral);
    // On again: rooms are made, and the template with its number applies.
    v.app
        .db
        .set_module(v.guild, pinchora_core::modules::Module::TempVoice, Some(true), None)
        .await
        .expect("switch the module on");
    v.app.announce_guild_change(v.guild).await;
    v.leave(alice, "Alice").await;
    v.cool_down(alice).await;
    v.join(alice, "Alice", creator).await;
    let made = v.created();
    assert_eq!(made.len(), 1);
    assert_eq!(made[0]["name"], "Room 1 · Alice");
    // Removing the creator leaves the room until it empties, and makes no more.
    v.manage = true;
    let gone = v
        .command(
            admin,
            json!([group(
                "creators",
                sub("remove", vec![channel_option("channel", creator)])
            )]),
            resolved,
        )
        .await;
    assert!(
        gone.content().contains("isn't a creator any more"),
        "{}",
        gone.content()
    );
    v.manage = false;
    assert!(v.app.db.temp_room(v.room_of(alice).await).await.unwrap().is_some());
}

#[tokio::test]
async fn a_restart_puts_the_rooms_right_and_the_sweep_catches_a_missed_leave() {
    let Some(mut v) = Voice::new("tempvoice_reconcile_and_sweep").await else {
        return;
    };
    let (admin, alice, bob, carol) = (unique(), unique(), unique(), unique());
    let creator = v.channel(2, "Join to create", None);
    v.add_creator(admin, creator, vec![]).await;
    // Three rooms: Alice's (she is still inside), Bob's (he left while the bot was away) and Carol's, whose
    // channel somebody deleted.
    let mut rooms = Vec::new();
    for owner in [alice, bob, carol] {
        v.join(owner, "Member", creator).await;
        rooms.push(v.room_of(owner).await);
        v.echo(&names(&[(owner, "Member")])).await;
    }
    // Bob and Carol are gone from voice by the time the bot is back; Carol's channel is gone too.
    {
        let mut world = v.world.lock().unwrap();
        world.voice.remove(&bob);
        world.voice.remove(&carol);
        world.channels.remove(&rooms[2]);
    }
    let [alices, bobs, carols] = [rooms[0], rooms[1], rooms[2]];
    // Everything above happened more than the grace period ago.
    sqlx::query("UPDATE temp_voice_channels SET created_at = now() - interval '5 minutes' WHERE guild_id = $1")
        .bind(v.guild as i64)
        .execute(v.app.db.pool())
        .await
        .unwrap();
    // The bot "restarts": its memory of who sits where is gone.
    for room in &rooms {
        for user in [alice, bob, carol] {
            v.app
                .cache
                .set_remove(&format!("tvoice:in:{room}"), &user.to_string())
                .await
                .unwrap();
        }
    }
    // Discord's GUILD_CREATE: the channels there are now, and who is in voice. Alice is in hers; Bob and
    // Carol are in none; Carol's channel is gone.
    let mut guild = guild_json(v.guild);
    guild["channels"] = json!([creator, alices, bobs]
        .iter()
        .map(|id| json!({ "id": id.to_string(), "type": 2, "guild_id": v.guild.to_string(), "name": "x", "parent_id": null }))
        .collect::<Vec<_>>());
    guild["voice_states"] = json!([{
        "channel_id": alices.to_string(), "user_id": alice.to_string(), "session_id": "sim", "deaf": false,
        "mute": false, "self_deaf": false, "self_mute": false, "self_video": false, "suppress": false,
        "request_to_speak_timestamp": null,
    }]);
    guild["members"] = json!([{
        "user": person_json(alice, "Alice", Some("Alice"), false), "roles": [],
        "joined_at": "2024-01-01T00:00:00.000000+00:00", "deaf": false, "mute": false, "flags": 0, "pending": false,
    }]);
    let event: GuildCreate = serde_json::from_value(guild).expect("guild create JSON");
    on_guild_create(v.app.clone(), event)
        .await
        .expect("the guild create handler failed");

    assert!(
        v.app.db.temp_room(alices).await.unwrap().is_some(),
        "Alice's room stays"
    );
    assert_eq!(
        pinchora_commands::temp_voice_members(&v.app, alices).await,
        1,
        "and knows she is in it"
    );
    assert!(
        v.app.db.temp_room(bobs).await.unwrap().is_none(),
        "Bob's empty room is gone"
    );
    assert!(v.deleted().contains(&bobs), "from Discord too");
    assert!(
        v.app.db.temp_room(carols).await.unwrap().is_none(),
        "Carol's room had no channel left"
    );

    // A snapshot taken before Alice joined can finish after her voice event. Its empty voice list
    // must not erase the newer Redis presence or delete her room.
    let mut old = guild_json(v.guild);
    old["channels"] = json!([creator, alices]
        .iter()
        .map(|id| json!({ "id": id.to_string(), "type": 2, "guild_id": v.guild.to_string(), "name": "x", "parent_id": null }))
        .collect::<Vec<_>>());
    old["voice_states"] = json!([]);
    let old: GuildCreate = serde_json::from_value(old).unwrap();
    on_guild_create(v.app.clone(), old).await.unwrap();
    assert!(v.app.db.temp_room(alices).await.unwrap().is_some());
    assert_eq!(pinchora_commands::temp_voice_members(&v.app, alices).await, 1);

    // Later a leave is lost: Alice is gone from voice but the bot never heard, so Redis still says
    // she is inside. The sweep must check Discord and then require two empty rounds.
    v.world.lock().unwrap().voice.remove(&alice);
    assert_eq!(pinchora_commands::temp_voice_members(&v.app, alices).await, 1);
    v.app
        .cache
        .release_cooldown(&format!("tvoice:verify:{alices}"))
        .await
        .unwrap();
    // One round per pod at a time: this test is every pod in turn.
    assert!(
        v.app.db.temp_room(alices).await.unwrap().is_some(),
        "still there before the sweep"
    );
    v.app.cache.release_cooldown("tvoice:sweep").await.unwrap();
    let first = pinchora_commands::tempvoice_tick(&v.app).await.unwrap();
    println!("first round deleted {first}; deleted so far {:?}", v.deleted());
    assert!(
        v.app.db.temp_room(alices).await.unwrap().is_some(),
        "seen empty once: kept"
    );
    // If Discord refuses deletion, keep tracking the room and retry on the next sweep.
    v.app.cache.release_cooldown("tvoice:sweep").await.unwrap();
    v.refuse("delete", 50013);
    assert_eq!(pinchora_commands::tempvoice_tick(&v.app).await.unwrap(), 0);
    assert!(v.app.db.temp_room(alices).await.unwrap().is_some());
    v.allow("delete");
    v.app.cache.release_cooldown("tvoice:sweep").await.unwrap();
    assert!(pinchora_commands::tempvoice_tick(&v.app).await.unwrap() >= 1);
    assert!(
        v.app.db.temp_room(alices).await.unwrap().is_none(),
        "seen empty twice: deleted"
    );
    assert!(v.deleted().contains(&alices));
}

#[tokio::test]
async fn voice_xp_is_still_paid_in_a_temp_room() {
    let Some(mut v) = Voice::new("tempvoice_voice_xp").await else {
        return;
    };
    let (admin, alice, bob) = (unique(), unique(), unique());
    let creator = v.channel(2, "Join to create", None);
    v.add_creator(admin, creator, vec![]).await;
    v.manage = true;
    let on = v
        .run(
            "/config xp voice".into(),
            admin,
            2,
            json!({ "id": unique().to_string(), "name": "config", "type": 1,
                    "options": [group("xp", sub("voice", vec![bool_option("enabled", true), int_option("amount", 20)]))] }),
            None,
        )
        .await;
    assert!(on.description().contains("20 XP a minute"), "{}", on.description());
    v.manage = false;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.echo(&names(&[(alice, "Alice")])).await;
    v.join(bob, "Bob", room).await;
    // A minute of its own: an earlier run's lock on the same minute would pay nobody.
    v.app
        .shift_clock(chrono::Duration::minutes(1 + (unique() % 600) as i64));
    let paid = pinchora_commands::voice_tick(&v.app).await.expect("tick");
    assert!(paid >= 2, "the tick paid {paid}");
    for member in [alice, bob] {
        let xp = v.app.db.member_rank(v.guild, member).await.unwrap().map_or(0, |r| r.xp);
        assert_eq!(xp, 20, "{member} earned for the minute in the temp room");
    }
}

#[tokio::test]
async fn a_room_can_have_a_private_thread_for_its_chat() {
    let Some(mut v) = Voice::new("tempvoice_thread").await else {
        return;
    };
    let (alice, bob) = (unique(), unique());
    let creator = v.channel(2, "Join to create", None);
    let chat = v.channel(0, "voice-chat", None);
    v.app
        .db
        .save_voice_creator(&pinchora_db::VoiceCreator {
            guild_id: v.guild as i64,
            channel_id: creator as i64,
            category_id: None,
            name_template: "{user}'s room".to_owned(),
            default_limit: 0,
            default_locked: true,
            max_per_user: 2,
            panel: true,
            panel_channel_id: Some(chat as i64),
            enabled: true,
        })
        .await
        .unwrap();
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    // A creator that starts rooms locked: @everyone cannot connect, and the bot keeps its own seat.
    assert_eq!(v.overwrite(room, v.guild).map(|b| b.1 & CONNECT), Some(CONNECT));
    assert!(v.overwrite(room, APPLICATION_ID).is_some());
    let row = v.app.db.temp_room(room).await.unwrap().unwrap();
    let thread = row.text_channel_id.expect("a thread") as u64;
    assert_eq!(row.panel_channel_id, Some(thread as i64), "the panel is in the thread");
    assert!(
        v.world.lock().unwrap().thread_members.contains(&(thread, alice)),
        "the owner is in the thread"
    );
    let posts = v.posts();
    assert!(
        posts.iter().any(|p| p.channel == thread && !bits_of(p).is_empty()),
        "a panel in the thread"
    );
    assert!(posts.iter().all(|p| p.channel != room), "none in the room itself");
    // A click in the thread means the room.
    let panel = posted_reply(posts.iter().find(|p| p.channel == thread).unwrap());
    v.here = thread;
    let unlocked = v.click(alice, &panel, "Unlock").await;
    assert!(unlocked.content().contains("open again"), "{}", unlocked.content());
    // Bob comes and goes: added to the thread, then taken out.
    v.join(bob, "Bob", room).await;
    assert!(v.world.lock().unwrap().thread_members.contains(&(thread, bob)));
    v.leave(bob, "Bob").await;
    assert!(!v.world.lock().unwrap().thread_members.contains(&(thread, bob)));
    // The room goes with its thread.
    v.leave(alice, "Alice").await;
    sqlx::query("UPDATE temp_voice_channels SET created_at = now() - interval '5 minutes' WHERE channel_id = $1")
        .bind(room as i64)
        .execute(v.app.db.pool())
        .await
        .unwrap();
    pinchora_commands::tempvoice_tick(&v.app).await.unwrap();
    let gone = v.deleted();
    assert!(gone.contains(&room) && gone.contains(&thread), "{gone:?}");
}

#[tokio::test]
async fn turkish_members_and_servers_get_turkish_text() {
    let Some(mut v) = Voice::new("tempvoice_turkish").await else {
        return;
    };
    let (admin, alice) = (unique(), unique());
    let creator = v.channel(2, "Join to create", None);
    v.app
        .set_guild_lang(v.guild, Some(pinchora_core::i18n::Lang::Tr))
        .await
        .unwrap();
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Ayşe", creator).await;
    // The panel is written in the server's language.
    let posts = v.posts();
    let panel = posts.iter().find(|p| !bits_of(p).is_empty()).expect("a panel");
    assert_eq!(panel.title(), "Oda kontrolleri");
    assert!(panel.description().contains("Sahibi"), "{}", panel.description());
    // Answers follow the member's own language.
    v.locale = "tr";
    let locked = v.say(alice, "lock").await;
    assert!(locked.content().contains("Odan kilitlendi"), "{}", locked.content());
    assert!(!locked.content().contains("Your room"), "{}", locked.content());
    v.locale = "en-US";
    let english = v.say(alice, "unlock").await;
    assert!(english.content().contains("open again"), "{}", english.content());
    // The room is named after the member, Turkish letters and all.
    assert!(v.created().iter().any(|c| c["name"] == "Ayşe odası"));
}

#[tokio::test]
async fn a_server_holds_a_limited_number_of_rooms() {
    let Some(mut v) = Voice::new("tempvoice_room_cap").await else {
        return;
    };
    let (admin, alice) = (unique(), unique());
    let creator = v.channel(2, "Join to create", None);
    v.add_creator(admin, creator, vec![]).await;
    // The server is full of rooms already (made some other way: rows are enough).
    for _ in 0..pinchora_core::temp_voice::MAX_ROOMS {
        v.app
            .db
            .create_temp_room(pinchora_db::NewTempRoom {
                channel_id: unique(),
                guild_id: v.guild,
                source_id: creator,
                owner_id: unique(),
                locked: false,
                user_limit: 0,
            })
            .await
            .unwrap();
    }
    pinchora_commands::forget_temp_voice(v.guild).await;
    v.join(alice, "Alice", creator).await;
    assert!(v.created().is_empty(), "no room past the cap");
    let posts = v.posts();
    let notice = posts.iter().find(|p| p.channel == creator).expect("a notice");
    assert!(notice.content().contains("temp rooms already"), "{}", notice.content());
}

// Review-only negative tests: assertions describe required safe behavior.
#[tokio::test]
async fn review_probe_kick_cannot_disconnect_a_bot_in_another_room() {
    let mut v = Voice::new("review_bot_elsewhere").await.unwrap();
    let (admin, alice, robot) = (unique(), unique(), unique());
    let creator = v.channel(2, "Creator", None);
    let other = v.channel(2, "Unrelated music room", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    v.world.lock().unwrap().voice.insert(robot, other);
    v.command(
        alice,
        json!([sub("kick", vec![user_option("user", robot)])]),
        json!({"users": {robot.to_string(): person_json(robot, "Music", None, true)}}),
    )
    .await;
    assert_eq!(
        v.world.lock().unwrap().voice.get(&robot).copied(),
        Some(other),
        "room owner disconnected a bot from an unrelated room"
    );
}

#[tokio::test]
async fn review_probe_stale_presence_cannot_disconnect_a_human_elsewhere() {
    let mut v = Voice::new("review_human_elsewhere").await.unwrap();
    let (admin, alice, bob) = (unique(), unique(), unique());
    let creator = v.channel(2, "Creator", None);
    let other = v.channel(2, "Private meeting", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.join(bob, "Bob", room).await;
    // Bob moves, but this gateway event is dropped/delayed.
    v.world.lock().unwrap().voice.insert(bob, other);
    v.command(
        alice,
        json!([sub("kick", vec![user_option("user", bob)])]),
        v.users(&[bob]),
    )
    .await;
    assert_eq!(
        v.world.lock().unwrap().voice.get(&bob).copied(),
        Some(other),
        "stale Redis membership lets an owner disconnect a human in another room"
    );
}

#[tokio::test]
async fn tempvoice_kick_respects_higher_role() {
    let Some(mut v) = Voice::new("tempvoice_kick_hierarchy").await else {
        return;
    };
    let (admin, alice, moderator, role) = (unique(), unique(), unique(), unique());
    let creator = v.channel(2, "Creator", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.join(moderator, "Moderator", room).await;
    {
        let mut world = v.world.lock().unwrap();
        world.member_roles.insert(moderator, vec![role]);
        world.extra_roles.push(role_json(role, "Moderator", 5, false));
    }
    v.command(
        alice,
        json!([sub("kick", vec![user_option("user", moderator)])]),
        v.users(&[moderator]),
    )
    .await;
    assert_eq!(v.world.lock().unwrap().voice.get(&moderator).copied(), Some(room));
    assert!(!v.app.db.is_banned_from_room(room, moderator).await.unwrap());
}

#[tokio::test]
async fn review_probe_sweep_preserves_an_occupied_room_after_presence_loss() {
    let mut v = Voice::new("review_sweep_live").await.unwrap();
    let (admin, alice) = (unique(), unique());
    let creator = v.channel(2, "Creator", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    sqlx::query("UPDATE temp_voice_channels SET created_at = now() - interval '5 minutes' WHERE channel_id = $1")
        .bind(room as i64)
        .execute(v.app.db.pool())
        .await
        .unwrap();
    v.app
        .cache
        .set_remove(&format!("tvoice:in:{room}"), &alice.to_string())
        .await
        .unwrap();
    for _ in 0..2 {
        v.app.cache.release_cooldown("tvoice:sweep").await.unwrap();
        pinchora_commands::tempvoice_tick(&v.app).await.unwrap();
    }
    assert!(
        v.channel_json(room).is_some(),
        "sweep deleted a room still occupied on Discord"
    );
}

#[tokio::test]
async fn review_probe_sweep_removes_stale_member_without_a_place_key() {
    let mut v = Voice::new("review_sweep_leak").await.unwrap();
    let (admin, alice) = (unique(), unique());
    let creator = v.channel(2, "Creator", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    sqlx::query("UPDATE temp_voice_channels SET created_at = now() - interval '5 minutes' WHERE channel_id = $1")
        .bind(room as i64)
        .execute(v.app.db.pool())
        .await
        .unwrap();
    // The 48-hour place key expires; the non-expiring member set survives a missed leave.
    v.app
        .cache
        .forget(&format!("tvoice:at:{}:{alice}", v.guild))
        .await
        .unwrap();
    v.world.lock().unwrap().voice.remove(&alice);
    for round in 0..8 {
        if round % 4 == 0 {
            // Simulate the 120-second verification cooldown expiring every fourth sweep.
            v.app
                .cache
                .release_cooldown(&format!("tvoice:verify:{room}"))
                .await
                .unwrap();
        }
        v.app.cache.release_cooldown("tvoice:sweep").await.unwrap();
        pinchora_commands::tempvoice_tick(&v.app).await.unwrap();
    }
    assert!(
        v.app.db.temp_room(room).await.unwrap().is_none(),
        "empty room never passes two empty rounds because stale membership survives reconciliation"
    );
}

#[tokio::test]
async fn tempvoice_sweep_fence_survives_slow_discord_reads() {
    let Some(mut v) = Voice::new("tempvoice_sweep_fence").await else {
        return;
    };
    let (admin, alice) = (unique(), unique());
    let creator = v.channel(2, "Creator", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    sqlx::query("UPDATE temp_voice_channels SET created_at = now() - interval '5 minutes' WHERE channel_id = $1")
        .bind(room as i64)
        .execute(v.app.db.pool())
        .await
        .unwrap();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    v.world.lock().unwrap().voice_delay = Some((started.clone(), release.clone()));
    let notified = started.notified();
    let app = v.app.clone();
    let first = tokio::spawn(async move { pinchora_commands::tempvoice_tick(&app).await.unwrap() });
    tokio::time::timeout(std::time::Duration::from_secs(10), notified)
        .await
        .unwrap();
    let second_app = v.app.clone();
    let mut second = tokio::spawn(async move { pinchora_commands::tempvoice_tick(&second_app).await.unwrap() });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), &mut second)
            .await
            .is_err(),
        "the second sweep must wait for the first sweep's Discord read"
    );
    // The sweep is global: unrelated old fixtures may be deleted. Its total is not a
    // per-room assertion. Release the deterministic gate, then check this room survives.
    v.world.lock().unwrap().voice_delay = None;
    release.notify_one();
    first.await.unwrap();
    second.await.unwrap();
    assert!(v.app.db.temp_room(room).await.unwrap().is_some());
}

#[tokio::test]
async fn review_probe_transfer_respects_per_member_room_limit() {
    let mut v = Voice::new("review_transfer_cap").await.unwrap();
    let (admin, alice, bob, guest) = (unique(), unique(), unique(), unique());
    let creator = v.channel(2, "Creator", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let alice_room = v.room_of(alice).await;
    v.join(guest, "Guest", alice_room).await;
    v.join(bob, "Bob", creator).await;
    let bob_room = v.room_of(bob).await;
    v.join(alice, "Alice", bob_room).await;
    v.command(
        bob,
        json!([sub("transfer", vec![user_option("user", alice)])]),
        v.users(&[alice]),
    )
    .await;
    assert_eq!(
        v.app.db.rooms_owned_by(v.guild, alice).await.unwrap().len(),
        1,
        "transfer bypasses max_per_user=1"
    );
}

#[tokio::test]
async fn review_probe_rename_filters_mentions_and_profanity() {
    let mut v = Voice::new("review_name_filter").await.unwrap();
    let (admin, alice) = (unique(), unique());
    let creator = v.channel(2, "Creator", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    let raw = "@everyone fuck";
    v.command(alice, json!([sub("rename", vec![text_option("name", raw)])]), json!({}))
        .await;
    assert_ne!(
        v.channel_json(room).unwrap()["name"],
        json!(raw),
        "unfiltered name sent to Discord unchanged"
    );
}

#[tokio::test]
async fn review_probe_claim_fails_closed_when_owner_presence_is_unknown() {
    let mut v = Voice::new("review_claim_unknown").await.unwrap();
    let (admin, alice, bob) = (unique(), unique(), unique());
    let creator = v.channel(2, "Creator", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.join(bob, "Bob", room).await;
    // Alice remains in Discord, but her Redis entry was lost and REST cannot confirm presence.
    v.app
        .cache
        .set_remove(&format!("tvoice:in:{room}"), &alice.to_string())
        .await
        .unwrap();
    v.refuse("voice", 50013);
    v.here = room;
    v.say(bob, "claim").await;
    assert_eq!(
        v.app.db.temp_room(room).await.unwrap().unwrap().owner_id,
        Some(alice as i64),
        "claim treats Discord permission errors as evidence that the owner left"
    );
}

#[tokio::test]
async fn review_probe_failed_thread_deletion_remains_tracked() {
    let mut v = Voice::new("review_thread_leak").await.unwrap();
    let (admin, alice) = (unique(), unique());
    let creator = v.channel(2, "Creator", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    let parent = v.channel(0, "Private chats", None);
    let thread = v.channel(12, "Room chat", Some(parent));
    v.app.db.set_room_panel(room, Some(thread), None, None).await.unwrap();
    v.refuse("thread_delete", 50013);
    v.leave(alice, "Alice").await;
    assert!(
        v.channel_json(thread).is_none() || v.app.db.temp_room(room).await.unwrap().is_some(),
        "thread survives but its only database reference is removed, so the sweep cannot retry"
    );
}

#[tokio::test]
async fn tempvoice_orphan_channels_and_threads_retry_cleanup() {
    let Some(v) = Voice::new("tempvoice_orphan_cleanup").await else {
        return;
    };
    let channel = v.channel(2, "Untracked room", None);
    let thread = v.channel(12, "Untracked chat", None);
    for (kind, id) in [("channel", channel), ("thread", thread)] {
        let entry = format!("{kind}:{id}");
        v.app
            .cache
            .put(
                &format!("tvoice:pending-at:{entry}"),
                "0",
                std::time::Duration::from_secs(300),
            )
            .await
            .unwrap();
        v.app.cache.set_add("tvoice:pending", &entry).await.unwrap();
    }
    v.refuse("delete", 50013);
    pinchora_commands::tempvoice_tick(&v.app).await.unwrap();
    assert!(v.channel_json(channel).is_some() && v.channel_json(thread).is_some());
    v.allow("delete");
    pinchora_commands::tempvoice_tick(&v.app).await.unwrap();
    assert!(v.channel_json(channel).is_none() && v.channel_json(thread).is_none());
}

async fn review_parallel_commands(v: &Voice, user: u64, first: Value, second: Value) {
    let data = |options: Value| {
        json!({
            "id": unique().to_string(), "name": "voice", "type": 1, "options": options, "resolved": {}
        })
    };
    let a = v.interaction(user, 2, data(first), None);
    let b = v.interaction(user, 2, data(second), None);
    let (ta, _ra) = oneshot::channel();
    let (tb, _rb) = oneshot::channel();
    let ra = Arc::new(Responder::http(v.app.clone(), &a, ta));
    let rb = Arc::new(Responder::http(v.app.clone(), &b, tb));
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(
            handle_interaction(v.app.clone(), a, ra),
            handle_interaction(v.app.clone(), b, rb)
        );
    })
    .await
    .expect("both commands should reach Discord");
}

#[tokio::test]
async fn review_probe_parallel_renames_share_one_cooldown() {
    let mut v = Voice::new("review_rename_race").await.unwrap();
    let (admin, alice) = (unique(), unique());
    let creator = v.channel(2, "Creator", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    v.world.lock().unwrap().review_gate = Some(Arc::new(tokio::sync::Barrier::new(2)));
    review_parallel_commands(
        &v,
        alice,
        json!([sub("rename", vec![text_option("name", "First")])]),
        json!([sub("rename", vec![text_option("name", "Second")])]),
    )
    .await;
    let edits = v
        .world
        .lock()
        .unwrap()
        .log
        .iter()
        .filter(|(method, _, body)| method == "PATCH" && body.get("name").is_some())
        .count();
    assert_eq!(edits, 1, "parallel requests both spend the same rename cooldown");
}

#[tokio::test]
async fn tempvoice_failed_rename_releases_only_its_reservation() {
    let Some(mut v) = Voice::new("tempvoice_rename_retry").await else {
        return;
    };
    let (admin, alice) = (unique(), unique());
    let creator = v.channel(2, "Creator", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.refuse("edit", 50013);
    v.command(
        alice,
        json!([sub("rename", vec![text_option("name", "First")])]),
        json!({}),
    )
    .await;
    v.allow("edit");
    v.command(
        alice,
        json!([sub("rename", vec![text_option("name", "Second")])]),
        json!({}),
    )
    .await;
    assert_eq!(v.channel_json(room).unwrap()["name"], "Second");
}

#[tokio::test]
async fn review_probe_parallel_lock_and_hide_preserve_both_denies() {
    let mut v = Voice::new("review_overwrite_race").await.unwrap();
    let (admin, alice) = (unique(), unique());
    let creator = v.channel(2, "Creator", None);
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.world.lock().unwrap().review_gate = Some(Arc::new(tokio::sync::Barrier::new(2)));
    review_parallel_commands(&v, alice, json!([sub("lock", vec![])]), json!([sub("hide", vec![])])).await;
    let saved = v.app.db.temp_room(room).await.unwrap().unwrap();
    assert!(saved.locked && saved.hidden);
    assert_eq!(
        v.overwrite(room, v.guild).unwrap().1 & (CONNECT | VIEW_CHANNEL),
        CONNECT | VIEW_CHANNEL,
        "read-modify-write overwrites lose one of the protection bits while DB claims both applied"
    );
}

#[tokio::test]
async fn review_probe_creation_waits_for_delete_my_data_user_lock() {
    let v = Voice::new("review_privacy_lock").await.unwrap();
    let alice = unique();
    let room = unique();
    let mut deletion = v.app.db.pool().begin().await.unwrap();
    pinchora_db::Db::delete_user_data_in(&mut deletion, alice, v.app.now())
        .await
        .unwrap();
    let spare = v.app.db.pool().acquire().await.unwrap();
    drop(spare);
    let raced = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        v.app.db.create_temp_room(pinchora_db::NewTempRoom {
            channel_id: room,
            guild_id: v.guild,
            source_id: unique(),
            owner_id: alice,
            locked: false,
            user_limit: 0,
        }),
    )
    .await;
    deletion.commit().await.unwrap();
    if let Ok(result) = &raced {
        assert!(
            matches!(result, Ok(pinchora_db::RoomMake::Made(_))),
            "unexpected creation result: {result:?}"
        );
        assert_eq!(
            v.app.db.temp_room(room).await.unwrap().unwrap().owner_id,
            Some(alice as i64)
        );
        println!("owner_id persisted while delete-my-data held the user lock, and survived its commit");
    }
    assert!(
        raced.is_err(),
        "create_temp_room bypasses the active privacy user lock and commits owner_id after privacy cleared it"
    );
}

#[tokio::test]
async fn tempvoice_creation_started_before_erasure_does_not_restore_owner() {
    let Some(v) = Voice::new("tempvoice_privacy_revalidation").await else {
        return;
    };
    let (alice, room) = (unique(), unique());
    v.app.db.user(alice).await.unwrap();
    let mut deletion = v.app.db.pool().begin().await.unwrap();
    pinchora_db::Db::delete_user_data_in(&mut deletion, alice, v.app.now())
        .await
        .unwrap();
    let app = v.app.clone();
    let guild = v.guild;
    let creating = tokio::spawn(async move {
        app.db
            .create_temp_room(pinchora_db::NewTempRoom {
                channel_id: room,
                guild_id: guild,
                source_id: unique(),
                owner_id: alice,
                locked: false,
                user_limit: 0,
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    deletion.commit().await.unwrap();
    assert!(creating.await.unwrap().is_err());
    assert!(v.app.db.temp_room(room).await.unwrap().is_none());
}

#[tokio::test]
async fn review_probe_lock_and_hide_override_inherited_member_role_allows() {
    let mut v = Voice::new("review_role_allow").await.unwrap();
    let (admin, alice, members_role) = (unique(), unique(), unique());
    let category = v.channel(4, "Community", None);
    v.world.lock().unwrap().channels.get_mut(&category).unwrap()["permission_overwrites"] = json!([
        {"id": members_role.to_string(), "type": 0, "allow": (CONNECT | VIEW_CHANNEL).to_string(), "deny": "0"}
    ]);
    let creator = v.channel(2, "Creator", Some(category));
    v.add_creator(admin, creator, vec![]).await;
    v.join(alice, "Alice", creator).await;
    let room = v.room_of(alice).await;
    v.say(alice, "lock").await;
    v.say(alice, "hide").await;
    // Discord applies @everyone, then role overwrites. A non-owner carrying Members can still enter.
    let (everyone_allow, everyone_deny) = v.overwrite(room, v.guild).unwrap();
    let (role_allow, role_deny) = v.overwrite(room, members_role).unwrap();
    let effective = (((CONNECT | VIEW_CHANNEL) & !everyone_deny) | everyone_allow) & !role_deny | role_allow;
    assert_eq!(
        effective & (CONNECT | VIEW_CHANNEL),
        0,
        "non-owner role still has CONNECT and VIEW_CHANNEL after successful lock and hide"
    );
}

#[tokio::test]
async fn tempvoice_creator_admin_paths_require_manage_guild_with_other_permissions() {
    let Some(mut v) = Voice::new("tempvoice_admin_matrix").await else {
        return;
    };
    let member = unique();
    let creator = v.channel(2, "Creator", None);
    v.extra_permissions = twilight_model::guild::Permissions::all().bits()
        & !(MANAGE_GUILD | twilight_model::guild::Permissions::ADMINISTRATOR.bits());
    let resolved = v.resolved(&[(creator, 2)]);
    for options in [
        json!([group("creators", sub("add", vec![channel_option("channel", creator)]))]),
        json!([group(
            "creators",
            sub("remove", vec![channel_option("channel", creator)])
        )]),
        json!([group("creators", sub("list", vec![]))]),
    ] {
        let reply = v.command(member, options, resolved.clone()).await;
        assert!(
            reply.ephemeral && reply.content().contains("Manage Server"),
            "{}",
            reply.content()
        );
    }
    assert!(v.app.db.voice_creators(v.guild).await.unwrap().is_empty());
}
