//! Fenced, multi-pod authoritative rooms. No economy writes or persisted presence.
pub(crate) mod games;
mod navigation;
mod picross;
mod room;
mod routing;
pub(crate) mod safety;
mod socket;
pub(crate) mod spaces;
pub(crate) use socket::{ticket, upgrade};

use crate::state::WebState;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, mpsc, watch};

pub const QUEUE: usize = 64;
pub const MAX_MESSAGE: usize = 4096;

#[derive(Serialize, Deserialize, Debug)]
pub struct Envelope {
    pub protocol_version: u8,
    pub request_id: String,
    pub room_epoch: String,
    pub sequence: u64,
    #[serde(flatten)]
    pub command: Command,
}
impl Envelope {
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > MAX_MESSAGE {
            return None;
        }
        let envelope: Self = serde_json::from_str(text).ok()?;
        // serde flatten cannot enforce deny_unknown_fields. Keep the wire surface explicit.
        let raw: Value = serde_json::from_str(text).ok()?;
        let fields: &[&str] = match &envelope.command {
            Command::Authenticate { .. } => &["ticket"],
            Command::Join { .. } => &["room"],
            Command::Chat { .. } => &["text"],
            Command::MoveTo { .. } => &["tile"],
            Command::Use { .. } => &["placement_id", "revision"],
            Command::GameAction { .. } => &["game_id", "payload"],
            _ => &[],
        };
        raw.as_object()?
            .keys()
            .all(|key| {
                ["protocol_version", "request_id", "room_epoch", "sequence", "type"].contains(&key.as_str())
                    || fields.contains(&key.as_str())
            })
            .then_some(envelope)
    }
}
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Authenticate { ticket: String },
    Join { room: String },
    Leave,
    Snapshot,
    MoveTo { tile: navigation::Tile },
    Stop,
    Use { placement_id: String, revision: String },
    GameAction { game_id: String, payload: games::Action },
    Stand,
    Ping,
    Chat { text: String },
}
#[derive(Serialize, Deserialize)]
pub struct Event {
    #[serde(default)]
    pub delivery: u64,
    #[serde(default)]
    pub stream: String,
    pub epoch: String,
    pub request: String,
    pub body: Value,
}
#[derive(Clone)]
pub struct Peer {
    pub token: String,
    pub user: u64,
    pub name: String,
    pub character: String,
    pub outgoing: mpsc::Sender<Event>,
    pub close: watch::Sender<Option<&'static str>>,
}
impl Peer {
    pub fn terminate(&self, reason: &'static str) {
        self.close.send_if_modified(|current| {
            if current.is_some() {
                false
            } else {
                *current = Some(reason);
                true
            }
        });
    }
    pub fn send(&self, epoch: &str, request: &str, body: Value) {
        if self
            .outgoing
            .try_send(Event {
                delivery: 0,
                stream: String::new(),
                epoch: epoch.into(),
                request: request.into(),
                body,
            })
            .is_err()
        {
            self.terminate("slow_client");
        }
    }
    pub fn error(&self, epoch: &str, request: &str, reason: &str) {
        self.send(epoch, request, json!({"type":"error","reason":reason}));
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RoomKey {
    pub owner: u64,
    pub idx: i16,
    pub instance: Option<u16>,
}
impl RoomKey {
    pub fn parse(raw: &str) -> Option<Self> {
        let parts: Vec<_> = raw.split(':').collect();
        if !((parts.len() == 3 && parts[0] == "house") || (parts.len() == 4 && parts[0] == "plaza")) {
            return None;
        }
        let key = Self {
            owner: parts[1].parse().ok()?,
            idx: parts[2].parse().ok()?,
            instance: if parts[0] == "plaza" {
                Some(parts[3].parse::<u16>().ok().filter(|n| *n < 1024)?)
            } else {
                None
            },
        };
        (key.owner > 0 && key.owner <= i64::MAX as u64 && (0..6).contains(&key.idx) && key.name() == raw).then_some(key)
    }
    pub fn name(self) -> String {
        match self.instance {
            Some(instance) => format!("plaza:{}:{}:{}", self.owner, self.idx, instance),
            None => format!("house:{}:{}", self.owner, self.idx),
        }
    }
}
pub enum Input {
    Join(Peer, String),
    Command(String, Envelope),
    Leave(String),
    Layout(i64),
}
#[derive(Default)]
pub struct Runtime {
    pub(crate) safety: Mutex<()>,
    pub shutdown: watch::Sender<bool>,
    pub tasks: std::sync::Mutex<Vec<tokio::task::AbortHandle>>,
    locations: Mutex<HashMap<u64, RoomKey>>,
    rooms: Mutex<HashMap<RoomKey, mpsc::Sender<Input>>>,
    controllers: Mutex<HashMap<u64, Peer>>,
    limits: Mutex<HashMap<u64, Limits>>,
}
impl Runtime {
    pub async fn terminate(&self, state: &WebState, user: u64, reason: &'static str) {
        let _ = state
            .app
            .cache
            .reef_publish(&format!("control:{user}"), &json!({"close":reason}).to_string())
            .await;
        if let Some(peer) = self.controllers.lock().await.get(&user) {
            peer.terminate(reason);
        }
    }
    pub async fn policy_changed(&self, state: &WebState, user: u64) {
        let _ = state.app.cache.reef_publish(&format!("control:{user}"), "{}").await;
        if let Some(peer) = self.controllers.lock().await.get(&user) {
            peer.send("", "", json!({"type":"policy_changed"}));
        }
    }
    pub async fn locate(&self, user: u64, key: RoomKey) {
        // This index belongs to local sockets. Remote actor proxies have no local release
        // callback, so recording them here would retain their identities indefinitely.
        let controllers = self.controllers.lock().await;
        if controllers.contains_key(&user) {
            self.locations.lock().await.insert(user, key);
        }
    }
    pub async fn recheck(&self, state: &WebState) {
        let locations = self.locations.lock().await.clone();
        for (user, key) in locations {
            let Some(peer) = self.controllers.lock().await.get(&user).cloned() else {
                continue;
            };
            let reason = if matches!(
                state.app.db.reef_sanctioned(user, key.owner, key.idx, false).await,
                Ok(true)
            ) {
                Some("sanctioned")
            } else if socket::room_access(state, key, user).await.is_err() {
                Some("access_revoked")
            } else {
                None
            };
            if let Some(reason) = reason {
                // A room transition or controller replacement may have completed during the DB read.
                let controllers = self.controllers.lock().await;
                let locations = self.locations.lock().await;
                if controllers.get(&user).is_some_and(|p| p.token == peer.token) && locations.get(&user) == Some(&key) {
                    peer.terminate(reason);
                }
            }
        }
    }

    pub async fn unlocate(&self, user: u64) {
        self.locations.lock().await.remove(&user);
    }
    pub async fn claim(&self, state: &WebState, peer: &Peer) -> anyhow::Result<()> {
        state
            .app
            .cache
            .reef_claim(&routing::controller_key(peer.user), &peer.token, 5000, true)
            .await?;
        self.locations.lock().await.remove(&peer.user);
        if let Some(old) = self.controllers.lock().await.insert(peer.user, peer.clone()) {
            old.terminate("controller_replaced");
        }
        Ok(())
    }
    pub async fn release(&self, state: &WebState, peer: &Peer) {
        let _ = state
            .app
            .cache
            .reef_release(&routing::controller_key(peer.user), &peer.token)
            .await;
        let mut controllers = self.controllers.lock().await;
        if controllers.get(&peer.user).is_some_and(|p| p.token == peer.token) {
            controllers.remove(&peer.user);
            self.locations.lock().await.remove(&peer.user);
        }
    }
    pub async fn allow(&self, user: u64, command: &Command) -> bool {
        let now = Instant::now();
        let mut limits = self.limits.lock().await;
        // Retain account budgets across reconnects without retaining account ids indefinitely.
        limits.retain(|_, l| now.duration_since(l.seen) < Duration::from_secs(120));
        limits.entry(user).or_insert_with(Limits::new).allow(command, now)
    }
    pub async fn room(&self, state: &WebState, key: RoomKey) -> mpsc::Sender<Input> {
        let mut rooms = self.rooms.lock().await;
        rooms.retain(|_, tx| !tx.is_closed());
        rooms
            .entry(key)
            .or_insert_with(|| {
                let (tx, rx) = mpsc::channel(QUEUE);
                let task = tokio::spawn(routing::run(state.clone(), key, rx));
                let mut tasks = self.tasks.lock().expect("room task registry");
                tasks.retain(|task| !task.is_finished());
                tasks.push(task.abort_handle());
                tx
            })
            .clone()
    }
}
pub struct Bucket {
    tokens: f64,
    burst: f64,
    rate: f64,
    at: Instant,
}
impl Bucket {
    fn new(rate: f64, burst: f64) -> Self {
        Self {
            tokens: burst,
            burst,
            rate,
            at: Instant::now(),
        }
    }
    fn take(&mut self, now: Instant) -> bool {
        self.tokens = (self.tokens + now.duration_since(self.at).as_secs_f64() * self.rate).min(self.burst);
        self.at = now;
        if self.tokens < 1.0 {
            false
        } else {
            self.tokens -= 1.0;
            true
        }
    }
}
struct Limits {
    chat: Bucket,
    chats: Vec<Instant>,
    duplicate: Vec<(String, Instant)>,
    movement: Bucket,
    furniture: Bucket,
    joins: Vec<Instant>,
    frames: Bucket,
    seen: Instant,
    used: Option<Instant>,
}
impl Limits {
    fn new() -> Self {
        Self {
            chat: Bucket::new(1.0, 3.0),
            chats: Vec::new(),
            duplicate: Vec::new(),
            movement: Bucket::new(5.0, 10.0),
            furniture: Bucket::new(2.0, 2.0),
            joins: Vec::new(),
            frames: Bucket::new(30.0, 60.0),
            seen: Instant::now(),
            used: None,
        }
    }
    fn allow(&mut self, command: &Command, now: Instant) -> bool {
        self.seen = now;
        if !self.frames.take(now) {
            return false;
        }
        match command {
            Command::Chat { text } => {
                self.chats
                    .retain(|at| now.duration_since(*at) < Duration::from_secs(60));
                self.duplicate
                    .retain(|(_, at)| now.duration_since(*at) < Duration::from_secs(60));
                let fingerprint = pinchora_game::chat::fingerprint(text);
                if !self.chat.take(now)
                    || self.chats.len() >= 20
                    || self.duplicate.iter().any(|(old, _)| old == &fingerprint)
                {
                    return false;
                }
                self.chats.push(now);
                self.duplicate.push((fingerprint, now));
                true
            }
            Command::Join { .. } => {
                self.joins
                    .retain(|at| now.duration_since(*at) < Duration::from_secs(60));
                if self.joins.len() >= 6 {
                    false
                } else {
                    self.joins.push(now);
                    true
                }
            }
            Command::MoveTo { .. } | Command::Stop => self.movement.take(now),
            Command::Use { .. } => {
                if !self.furniture.take(now)
                    || self
                        .used
                        .is_some_and(|at| now.duration_since(at) < Duration::from_millis(500))
                {
                    false
                } else {
                    self.used = Some(now);
                    true
                }
            }
            Command::Stand | Command::GameAction { .. } => self.furniture.take(now),
            _ => true,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn remote_peers_do_not_accumulate_in_the_local_location_index() {
        let runtime = Runtime::default();
        for user in 1..1000 {
            runtime
                .locate(
                    user,
                    RoomKey {
                        owner: 1,
                        idx: 0,
                        instance: None,
                    },
                )
                .await;
        }
        assert!(runtime.locations.lock().await.is_empty());
    }
    #[tokio::test]
    async fn slow_client_eviction_preserves_the_reason() {
        let (outgoing, mut incoming) = mpsc::channel(1);
        let (close, _) = watch::channel(None);
        let peer = Peer {
            token: "slow".into(),
            user: 1,
            name: "slow".into(),
            character: "".into(),
            outgoing,
            close,
        };
        peer.send("1", "1", json!({"type":"snapshot"}));
        peer.send("1", "2", json!({"type":"correction"}));
        assert_eq!(*peer.close.borrow(), Some("slow_client"));
        peer.terminate("closed");
        assert_eq!(*peer.close.borrow(), Some("slow_client"));
        assert_eq!(incoming.recv().await.unwrap().request, "1");
    }
    #[test]
    fn protocol_intentions_only() {
        let envelope = serde_json::json!({"protocol_version":1,"request_id":"1","room_epoch":"","sequence":1,"type":"authenticate","ticket":"test"});
        assert!(Envelope::parse(&envelope.to_string()).is_some());
        let mut oversized = envelope.clone();
        oversized["ticket"] = json!("a".repeat(MAX_MESSAGE));
        assert!(Envelope::parse(&oversized.to_string()).is_none());
        let forged = serde_json::json!({"protocol_version":1,"request_id":"2","room_epoch":"e","sequence":2,"type":"move_to","tile":[1,1],"position":[8,8],"speed":99});
        assert!(Envelope::parse(&forged.to_string()).is_none());
        for command in ["chat", "game_action", "teleport"] {
            let envelope =
                serde_json::json!({"protocol_version":1,"request_id":"2","room_epoch":"e","sequence":2,"type":command});
            assert!(Envelope::parse(&envelope.to_string()).is_none());
        }
    }
    #[test]
    fn account_rate_limits() {
        let mut limits = Limits::new();
        let now = Instant::now();
        for _ in 0..10 {
            assert!(limits.allow(&Command::MoveTo { tile: [0, 0] }, now));
        }
        assert!(!limits.allow(&Command::Stop, now));
        assert!(limits.allow(&Command::Stop, now + Duration::from_millis(200)));
        let mut limits = Limits::new();
        for _ in 0..6 {
            assert!(limits.allow(&Command::Join { room: String::new() }, now));
        }
        assert!(!limits.allow(&Command::Join { room: String::new() }, now));
        assert!(!limits.allow(&Command::Join { room: String::new() }, now + Duration::from_secs(59)));
        assert!(limits.allow(&Command::Join { room: String::new() }, now + Duration::from_secs(60)));
        let now = now + Duration::from_secs(60);
        assert!(limits.allow(&Command::Stand, now));
        assert!(limits.allow(&Command::Stand, now));
        assert!(!limits.allow(&Command::Stand, now));
    }
    #[test]
    fn chat_burst_minute_and_duplicates() {
        let mut limits = Limits::new();
        let now = Instant::now();
        for text in ["one", "two", "three"] {
            assert!(limits.allow(&Command::Chat { text: text.into() }, now));
        }
        assert!(!limits.allow(&Command::Chat { text: "four".into() }, now));
        assert!(limits.allow(&Command::Chat { text: "four".into() }, now + Duration::from_secs(1)));
        assert!(!limits.allow(&Command::Chat { text: "FOUR".into() }, now + Duration::from_secs(2)));
        let mut limits = Limits::new();
        let now = Instant::now();
        for i in 0..20 {
            assert!(limits.allow(
                &Command::Chat {
                    text: format!("message {}", (b'a' + i) as char)
                },
                now + Duration::from_secs(i as u64)
            ));
        }
        assert!(!limits.allow(&Command::Chat { text: "another".into() }, now + Duration::from_secs(20)));
        assert!(limits.allow(&Command::Chat { text: "another".into() }, now + Duration::from_secs(60)));
    }
    #[test]
    fn canonical_house_and_guild_room_keys() {
        assert_eq!(
            RoomKey::parse("house:123:0"),
            Some(RoomKey {
                owner: 123,
                idx: 0,
                instance: None
            })
        );
        assert_eq!(RoomKey::parse("plaza:123:0:17").unwrap().instance, Some(17));
        for raw in [
            "plaza:123:0:01",
            "plaza:123:0:1024",
            "plaza:123:6:0",
            "plaza:0:0:0",
            "plaza:123:0",
            "house:123:6",
            "house:0:0",
            "house:01:0",
            "house:1:0:1",
            "house:18446744073709551615:0",
        ] {
            assert!(RoomKey::parse(raw).is_none());
        }
    }
}
