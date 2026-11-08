use super::{
    Command, Envelope, Input, Peer, RoomKey,
    navigation::{self, Grid, Target, Tile},
    socket::room_access,
};
use crate::state::WebState;
use pinchora_commands::RoomData;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

struct Layout {
    grid: Grid,
    targets: Vec<Target>,
    revision: String,
    json: Value,
}
impl Layout {
    async fn load(state: &WebState, key: RoomKey) -> anyhow::Result<Self> {
        let (house, rooms) = state
            .app
            .db
            .house_if_any(key.owner)
            .await?
            .ok_or_else(|| anyhow::anyhow!("missing house"))?;
        let room = rooms
            .into_iter()
            .find(|r| r.idx == key.idx)
            .ok_or_else(|| anyhow::anyhow!("missing room"))?;
        let tier = pinchora_game::house::tier(house.tier);
        let data = RoomData {
            width: tier.width,
            depth: tier.depth,
            room,
            placements: state.app.db.placements(key.owner, key.idx).await?,
            pets: vec![],
            characters: vec![],
            seed: 0,
        };
        let (grid, targets) = navigation::navigation(&data);
        let json = data.json();
        let revision = hex::encode(Sha256::digest(json.to_string()));
        Ok(Self {
            grid,
            targets,
            revision,
            json,
        })
    }
}
struct Member {
    peer: Peer,
    tile: Tile,
    path: Vec<Tile>,
    progress: f64,
    seat: Option<Target>,
    direction: String,
    used: Option<Instant>,
    advanced: Instant,
    using: bool,
    last_sequence: u64,
}
impl Member {
    fn position(&self) -> [f64; 2] {
        if let Some(seat) = &self.seat {
            return seat.anchor.map(f64::from);
        }
        let mut p = self.tile.map(f64::from);
        if let Some(next) = self.path.get(1) {
            for i in 0..2 {
                p[i] += f64::from(next[i] - self.tile[i]) * self.progress;
            }
        }
        p
    }
    fn posture(&self) -> &'static str {
        if self.seat.is_some() {
            "sit"
        } else if self.path.len() > 1 {
            "walk"
        } else if self.using && self.used.is_some_and(|at| at.elapsed() < Duration::from_secs(1)) {
            "use"
        } else {
            "idle"
        }
    }
    fn json(&self) -> Value {
        json!({"user_id":self.peer.user.to_string(),"name":self.peer.name,"character":self.peer.character,
            "tile":self.tile,"position":self.position(),"direction":self.direction,
            "state":self.posture(),
            "seat":self.seat})
    }
    fn correction(&self) -> Value {
        // Compact tuple: id, x, y, tile x/y, facing, posture. Appearance is sent on join/change only.
        let p = self.position().map(|n| (n * 1000.0).round() / 1000.0);
        json!([
            self.peer.user.to_string(),
            p[0],
            p[1],
            self.tile[0],
            self.tile[1],
            self.direction,
            self.posture()
        ])
    }
    fn stop(&mut self) {
        // Finish the current edge: stopping/replanning never rounds across a blocked corner.
        if self.path.len() > 1 {
            self.path.truncate(2);
        } else {
            self.path.clear();
            self.progress = 0.0;
        }
    }
    fn advance(&mut self, seconds: f64) {
        if self.path.len() < 2 {
            return;
        }
        self.progress += seconds * 3.0;
        while self.progress >= 1.0 && self.path.len() > 1 {
            self.progress -= 1.0;
            self.path.remove(0);
            self.tile = self.path[0];
        }
        if let Some(next) = self.path.get(1) {
            let dx = next[0] - self.tile[0];
            let dy = next[1] - self.tile[1];
            self.direction = if dx > 0 {
                "SE"
            } else if dx < 0 {
                "NW"
            } else if dy > 0 {
                "SW"
            } else {
                "NE"
            }
            .into();
        } else {
            self.path.clear();
            self.progress = 0.0;
        }
    }
}
struct Room {
    key: RoomKey,
    epoch: String,
    layout: Layout,
    fence: pinchora_db::ReefFence,
    layout_events: std::collections::HashSet<i64>,
    members: BTreeMap<String, Member>,
}
impl Room {
    fn broadcast(&self, request: &str, body: Value) {
        for member in self.members.values() {
            member.peer.send(&self.epoch, request, body.clone());
        }
    }
    fn roster(&self) -> Vec<Value> {
        self.members.values().map(Member::json).collect()
    }
    fn snapshot(&self, peer: &Peer, request: &str) {
        peer.send(
            &self.epoch,
            request,
            json!({"type":"snapshot","room":self.key.name(),
            "revision":self.layout.revision,"players":self.roster(),"layout":self.layout.json}),
        );
    }
    fn leave(&mut self, token: &str) {
        if let Some(member) = self.members.remove(token) {
            self.broadcast(
                "",
                json!({"type":"presence","action":"leave","user_id":member.peer.user.to_string()}),
            );
        }
    }
    async fn refresh(&mut self, state: &WebState) -> anyhow::Result<()> {
        let events = state.app.db.reef_layout_events(self.key.owner).await?;
        let fresh: Vec<_> = events
            .iter()
            .filter(|id| !self.layout_events.contains(id))
            .map(|id| id.to_string())
            .collect();
        let layout = Layout::load(state, self.key).await?;
        self.layout_events = events.into_iter().collect();
        if layout.revision == self.layout.revision && fresh.is_empty() {
            return Ok(());
        }
        let entry = layout.grid.spawn().ok_or_else(|| anyhow::anyhow!("no free entry"))?;
        for member in self.members.values_mut() {
            member.advanced = Instant::now();
            member.path.clear();
            member.progress = 0.0;
            member.seat = None;
            member.using = false;
            if !layout.grid.free(member.tile) {
                member.tile = entry;
            }
        }
        self.layout = layout;
        // Publish each committed transaction once per live room epoch, even when several commits
        // coalesce into the same loaded geometry. Reconnecting clients obtain the current snapshot.
        for event in &fresh {
            self.broadcast(
                "",
                json!({"type":"layout_changed","revision":self.layout.revision,
                "layout":self.layout.json,"event_id":event,"event_ids":[event]}),
            );
        }
        if fresh.is_empty() {
            self.broadcast(
                "",
                json!({"type":"layout_changed","revision":self.layout.revision,"layout":self.layout.json}),
            );
        }
        for member in self.members.values() {
            self.snapshot(&member.peer, "");
        }
        Ok(())
    }
    async fn join(&mut self, state: &WebState, peer: Peer, request: String) {
        // Old connections can have queued work when a replacement claims the account.
        if peer.close.borrow().is_some() {
            return;
        }
        if let Err(reason) = super::routing::controller(state, &peer).await {
            peer.terminate(reason);
            return;
        }
        if !super::routing::in_room(state, &peer, self.key).await {
            return;
        }
        if room_access(state, self.key, peer.user).await.is_err() {
            peer.error(&self.epoch, &request, "access_denied");
            return;
        }
        if self.refresh(state).await.is_err() {
            peer.error(&self.epoch, &request, "room_unavailable");
            return;
        }
        let old: Vec<_> = self
            .members
            .iter()
            .filter(|(_, m)| m.peer.user == peer.user)
            .map(|(k, _)| k.clone())
            .collect();
        for token in old {
            self.leave(&token);
        }
        if self.members.len() >= if self.key.instance.is_some() { 32 } else { 16 } {
            peer.error(&self.epoch, &request, "room_full");
            return;
        }
        let Some(tile) = self.layout.grid.spawn() else {
            peer.error(&self.epoch, &request, "no_free_entry");
            return;
        };
        let member = Member {
            peer: peer.clone(),
            tile,
            path: vec![],
            progress: 0.0,
            seat: None,
            direction: "SW".into(),
            used: None,
            advanced: Instant::now(),
            using: false,
            last_sequence: 0,
        };
        let presence = member.json();
        self.members.insert(peer.token.clone(), member);
        if self.key.instance.is_none() && self.key.owner != peer.user {
            let _ = state
                .app
                .db
                .record_visit(self.key.owner, peer.user, chrono::Utc::now().date_naive(), 0, 0)
                .await;
        }
        self.snapshot(&peer, &request);
        self.broadcast("", json!({"type":"presence","action":"join","player":presence}));
    }
    async fn command(&mut self, state: &WebState, token: &str, envelope: Envelope) -> anyhow::Result<()> {
        let Some(member) = self.members.get_mut(token) else {
            return Ok(());
        };
        if member.peer.close.borrow().is_some() {
            return Ok(());
        }
        if let Err(reason) = super::routing::controller(state, &member.peer).await {
            member.peer.terminate(reason);
            return Ok(());
        }
        if !super::routing::in_room(state, &member.peer, self.key).await {
            self.leave(token);
            return Ok(());
        }
        // Recovery and duplicate-command snapshots expose the same private room as movement.
        // A remote revocation must apply before any early snapshot return.
        if let Err(error) = room_access(state, self.key, member.peer.user).await {
            member.peer.terminate(super::socket::access_close_reason(&error));
            return Ok(());
        }
        if envelope.sequence > 0 && envelope.sequence <= member.last_sequence {
            let peer = member.peer.clone();
            self.snapshot(&peer, &envelope.request_id);
            return Ok(());
        }
        member.last_sequence = member.last_sequence.max(envelope.sequence);
        let member = &self.members[token];
        let request = &envelope.request_id;
        if envelope.room_epoch != self.epoch {
            member.peer.error(&self.epoch, request, "stale_epoch");
            self.snapshot(&member.peer, request);
            return Ok(());
        }
        if matches!(envelope.command, Command::Snapshot) {
            self.snapshot(&member.peer, request);
            return Ok(());
        }
        if let Command::GameAction { game_id, payload } = &envelope.command {
            let peer = member.peer.clone();
            let id = game_id.parse::<i64>().unwrap_or(0);
            match super::games::action(state, peer.user, id, payload.clone(), Some(self.key), Some(&self.fence)).await {
                Ok(game) => {
                    peer.send(&self.epoch, request, json!({"type":"game_changed","game":game}));
                    for m in self.members.values().filter(|m| m.peer.user != peer.user) {
                        m.peer
                            .send(&self.epoch, "", json!({"type":"game_changed","game_id":game_id}));
                    }
                }
                Err(e) => peer.error(&self.epoch, request, e.code),
            }
            return Ok(());
        }
        if let Command::Chat { text } = &envelope.command {
            let peer = member.peer.clone();
            if let Err(reason) = pinchora_game::chat::check(text) {
                peer.error(&self.epoch, request, reason);
                return Ok(());
            }
            let _guard = state.reef.safety.lock().await;
            if peer.close.borrow().is_some() {
                return Ok(());
            }
            let peers: Vec<_> = self
                .members
                .values()
                .filter(|m| m.peer.close.borrow().is_none())
                .map(|m| m.peer.user)
                .collect();
            match state
                .app
                .db
                .reef_chat_fenced(self.key.owner, self.key.idx, peer.user, text, &peers, Some(&self.fence))
                .await?
            {
                Err(reason) => peer.error(&self.epoch, request, reason),
                Ok((evidence, recipients)) => {
                    let mut event = evidence.json();
                    event["type"] = json!("chat");
                    event["name"] = json!(peer.name);
                    event["room"] = json!(self.key.name());
                    for member in self.members.values() {
                        if recipients.contains(&member.peer.user) {
                            member.peer.send(
                                &self.epoch,
                                if member.peer.user == peer.user { request } else { "" },
                                event.clone(),
                            );
                        }
                    }
                }
            }
            return Ok(());
        }
        // Refresh before accepting an intention, including after missed pub/sub notifications.
        self.refresh(state).await?;
        let occupied: Vec<_> = self
            .members
            .iter()
            .filter(|(k, _)| k.as_str() != token)
            .filter_map(|(_, m)| m.seat.as_ref().map(|s| s.id.clone()))
            .collect();
        let member = self.members.get_mut(token).expect("member remains during refresh");
        let now = Instant::now();
        member.advance(now.duration_since(member.advanced).as_secs_f64());
        member.advanced = now;
        let mut event = None;
        match envelope.command {
            Command::MoveTo { tile } => {
                let start = member.path.get(1).copied().unwrap_or(member.tile);
                if let Some(mut path) = self.layout.grid.path(start, tile) {
                    if start != member.tile {
                        path.insert(0, member.tile);
                    } else {
                        member.progress = 0.0;
                    }
                    member.seat = None;
                    member.using = false;
                    member.path = path;
                    event = Some(
                        json!({"type":"path","user_id":member.peer.user.to_string(),"path":member.path,
                        "position":member.position(),"speed":3,"player":member.json()}),
                    );
                } else {
                    member.peer.error(&self.epoch, request, "unreachable");
                }
            }
            Command::Stop => {
                member.stop();
                event = Some(
                    json!({"type":"path","user_id":member.peer.user.to_string(),"path":member.path,"speed":3,"player":member.json()}),
                );
            }
            Command::Stand => {
                member.seat = None;
                member.using = false;
                event = Some(json!({"type":"posture","player":member.json()}));
            }
            Command::Use { placement_id, revision } => {
                let target = self.layout.targets.iter().find(|t| t.id == placement_id);
                let reason = if revision != self.layout.revision {
                    Some("stale_layout")
                } else if member.used.is_some_and(|t| t.elapsed() < Duration::from_millis(500)) {
                    Some("cooldown")
                } else if target.is_none() {
                    Some("not_usable")
                } else if member.path.len() > 1 || !target.is_some_and(|t| t.approaches.contains(&member.tile)) {
                    Some("too_far")
                } else if occupied.contains(&placement_id) {
                    Some("seat_occupied")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    member.peer.error(&self.epoch, request, reason);
                } else {
                    let target = target.unwrap().clone();
                    member.used = Some(Instant::now());
                    member.direction = target.direction.clone();
                    member.using = target.kind == "usable";
                    if target.kind == "seat" {
                        member.seat = Some(target);
                    }
                    event = Some(json!({"type":"posture","player":member.json()}));
                }
            }
            _ => member.peer.error(&self.epoch, request, "invalid_command"),
        }
        if let Some(event) = event {
            for (recipient, member) in &self.members {
                member.peer.send(
                    &self.epoch,
                    if recipient == token { request } else { "" },
                    event.clone(),
                );
            }
        }
        Ok(())
    }
}
pub async fn run(state: WebState, key: RoomKey, fence: pinchora_db::ReefFence, input: mpsc::Receiver<Input>) {
    // Room work can wait on player locks or layout reads. Renewal must keep running during
    // those waits; loss cancels the in-flight work instead of draining stale commands.
    tokio::select! {
        biased;
        _ = async {
            let mut tick = tokio::time::interval(Duration::from_millis(500));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                if !matches!(tokio::time::timeout(Duration::from_secs(1), super::routing::valid(&state, key, &fence)).await, Ok(true)) {
                    break;
                }
            }
        } => {},
        _ = run_owned(&state, key, &fence, input) => {},
    }
}
async fn run_owned(state: &WebState, key: RoomKey, fence: &pinchora_db::ReefFence, mut input: mpsc::Receiver<Input>) {
    let epoch = fence.token.to_string();
    let Ok(events) = state.app.db.reef_layout_events(key.owner).await else {
        return;
    };
    let layout = match Layout::load(state, key).await {
        Ok(layout) => layout,
        Err(_) => {
            if let Some(Input::Join(peer, request)) = input.recv().await {
                peer.error(&epoch, &request, "room_unavailable");
            }
            return;
        }
    };
    let mut room = Room {
        key,
        epoch,
        layout,
        fence: fence.clone(),
        layout_events: events.into_iter().collect(),
        members: BTreeMap::new(),
    };
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut refreshed = Instant::now();
    let mut policy = Instant::now();
    let mut empty_since = Instant::now();
    loop {
        tokio::select! {
            command=input.recv()=>{
                if !super::routing::valid(state,key,fence).await {break;}
                match command {
                    Some(Input::Join(peer,request))=>room.join(state,peer,request).await,
                    Some(Input::Leave(token))=>room.leave(&token),
                    Some(Input::Layout(id))=>{
                        if !room.layout_events.contains(&id) && room.refresh(state).await.is_err() {break;}
                    }
                    Some(Input::Command(token,envelope))=>{
                        if room.command(state,&token,envelope).await.is_err() {break;}
                    }
                    None=>break,
                }
            }
            _=tick.tick()=>{
                if !super::routing::valid(state,key,fence).await {break;}
                let now=Instant::now();
                let gone:Vec<_>=room.members.iter().filter(|(_,m)|m.peer.close.borrow().is_some() || m.peer.outgoing.is_closed()).map(|(k,_)|k.clone()).collect();
                for token in gone {room.leave(&token);}
                if room.members.is_empty() {
                    if empty_since.elapsed()>Duration::from_secs(2) {break;}
                } else {empty_since=now;}
                if refreshed.elapsed()>=Duration::from_secs(1) {
                    if key.instance.is_some() {
                        let _=state.app.cache.reef_claim(&format!("occupancy:{}",key.name()),&room.members.len().to_string(),3000,true).await;
                    }
                    if state.app.cache.ping().await.is_err() || room.refresh(state).await.is_err() {break;}
                    refreshed=Instant::now();
                }
                if policy.elapsed()>=Duration::from_secs(1) {
                    let mut relocated=Vec::new();
                    for (token,member) in &room.members {
                        if !super::routing::in_room(state,&member.peer,key).await {relocated.push(token.clone());continue;}
                        if let Err(reason)=super::routing::controller(state,&member.peer).await {member.peer.terminate(reason);}
                        else if let Err(error)=room_access(state,key,member.peer.user).await {member.peer.terminate(super::socket::access_close_reason(&error));}
                    }
                    for token in relocated {room.leave(&token);}
                    policy=Instant::now();
                }
                for member in room.members.values_mut() {
                    member.advance(now.saturating_duration_since(member.advanced).as_secs_f64());
                    member.advanced = now;
                }
                if !room.members.is_empty() {room.broadcast("",json!({"type":"correction","roster":true,"updates":room.members.values().map(Member::correction).collect::<Vec<_>>()}));}
            }
        }
    }
    for member in room.members.values() {
        member.peer.terminate("ownership_lost");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn movement_clock_and_corner_safe_stop() {
        let (outgoing, _) = mpsc::channel(64);
        let (close, _) = tokio::sync::watch::channel(None);
        let peer = Peer {
            token: "t".into(),
            user: 1,
            name: "n".into(),
            character: "c".into(),
            outgoing,
            close,
        };
        let mut m = Member {
            peer,
            tile: [0, 0],
            path: vec![[0, 0], [1, 0], [1, 1]],
            progress: 0.0,
            seat: None,
            direction: "SW".into(),
            used: None,
            advanced: Instant::now(),
            using: false,
            last_sequence: 0,
        };
        m.advance(0.1);
        assert!((m.position()[0] - 0.3).abs() < 0.0001);
        m.stop();
        m.advance(0.9);
        assert_eq!(m.position(), [1.0, 0.0]);
        assert!(m.path.is_empty());
    }
}
