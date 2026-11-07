//! Each frontend subscribes before relaying. Only the fenced owner runs the room actor.
use super::{Envelope, Event, Input, Peer, QUEUE, RoomKey, room};
use crate::state::WebState;
use futures_util::StreamExt;
use pinchora_db::ReefFence;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

pub fn lease_key(key: RoomKey) -> String {
    format!("lease:{}", key.name())
}
pub fn controller_key(user: u64) -> String {
    format!("controller:{user}")
}

pub fn location_key(token: &str) -> String {
    format!("location:{token}")
}
pub async fn in_room(state: &WebState, peer: &Peer, key: RoomKey) -> bool {
    matches!(state.app.cache.reef_get(&location_key(&peer.token)).await, Ok(Some(room)) if room == key.name())
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "route")]
enum Wire {
    Join {
        token: String,
        user: u64,
        name: String,
        character: String,
        request: String,
    },
    Command {
        token: String,
        envelope: Envelope,
    },
    Leave {
        token: String,
    },
    Event {
        token: String,
        event: Event,
    },
    Close {
        token: String,
        epoch: String,
        reason: String,
    },
    Layout {
        id: i64,
    },
}
struct Owner {
    fence: ReefFence,
    input: mpsc::Sender<Input>,
    task: JoinHandle<()>,
    proxies: HashMap<String, JoinHandle<()>>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.task.abort();
        for task in self.proxies.values() {
            task.abort();
        }
    }
}
async fn acquire(state: &WebState, key: RoomKey) -> anyhow::Result<Option<Owner>> {
    let cache = &state.app.cache;
    if cache.reef_get(&lease_key(key)).await?.is_some() {
        return Ok(None);
    }
    let Some(fence) = state.app.db.reef_acquire(cache.namespace(), key.owner, key.idx).await? else {
        return Ok(None);
    };
    if !cache
        .reef_claim(&lease_key(key), &fence.token.to_string(), 3000, false)
        .await?
    {
        state.app.db.reef_release(&fence).await?;
        return Ok(None);
    }
    let (input, rx) = mpsc::channel(QUEUE);
    let task = tokio::spawn(room::run(state.clone(), key, fence.clone(), rx));
    Ok(Some(Owner {
        fence,
        input,
        task,
        proxies: HashMap::new(),
    }))
}
pub async fn valid(state: &WebState, key: RoomKey, fence: &ReefFence) -> bool {
    // Never resurrect an expired Redis lease. PostgreSQL refuses renewal of expired fencing rows too.
    matches!(
        state
            .app
            .cache
            .reef_renew(&lease_key(key), &fence.token.to_string(), 3000)
            .await,
        Ok(true)
    ) && matches!(state.app.db.reef_renew(fence).await, Ok(true))
}
pub async fn controller(state: &WebState, peer: &Peer) -> Result<(), &'static str> {
    match state.app.cache.reef_get(&controller_key(peer.user)).await {
        Ok(Some(token)) if token == peer.token => Ok(()),
        Ok(Some(_)) => Err("controller_replaced"),
        _ => Err("routing_unavailable"),
    }
}
fn proxy(
    state: &WebState,
    key: RoomKey,
    epoch: String,
    token: String,
    user: u64,
    name: String,
    character: String,
) -> (Peer, JoinHandle<()>) {
    let (outgoing, mut incoming) = mpsc::channel::<Event>(QUEUE);
    let (close, mut closed) = watch::channel(None);
    let peer = Peer {
        token: token.clone(),
        user,
        name,
        character,
        outgoing,
        close,
    };
    let cache = state.app.cache.clone();
    let stream = crate::auth::session::random_token(16);
    let task = tokio::spawn(async move {
        let mut sequence = 0;
        let mut watching = true;
        loop {
            let wire = tokio::select! {
                biased;
                changed = closed.changed(), if watching => {
                    if changed.is_err() {
                        // Rejected admissions drop the peer after queuing their error. Drain that error first.
                        watching = false;
                        continue;
                    }
                    Wire::Close {token:token.clone(), epoch:epoch.clone(), reason:closed.borrow().unwrap_or("room_unavailable").into()}
                },
                event = incoming.recv() => {
                    let Some(mut event) = event else { break; };
                    sequence += 1;
                    event.delivery = sequence;
                    event.stream = stream.clone();
                    Wire::Event {token:token.clone(),event}
                }
            };
            let done = matches!(wire, Wire::Close { .. });
            let Ok(body) = serde_json::to_string(&wire) else {
                break;
            };
            if !matches!(
                cache
                    .reef_publish_fenced(&lease_key(key), &epoch, &key.name(), &body)
                    .await,
                Ok(true)
            ) || done
            {
                break;
            }
        }
    });
    (peer, task)
}

pub async fn run(state: WebState, key: RoomKey, mut input: mpsc::Receiver<Input>) {
    let mut peers: HashMap<String, Peer> = HashMap::new();
    let mut owner: Option<Owner> = None;
    let result = route(&state, key, &mut input, &mut peers, &mut owner).await;
    let reason = if result.is_err() {
        "routing_unavailable"
    } else {
        "ownership_lost"
    };
    for peer in peers.values() {
        peer.terminate(reason);
    }
    if let Some(owner) = owner {
        let fence = owner.fence.clone();
        drop(owner);
        let _ = state
            .app
            .cache
            .reef_release(&lease_key(key), &fence.token.to_string())
            .await;
        let _ = state.app.db.reef_release(&fence).await;
    }
}
async fn route(
    state: &WebState,
    key: RoomKey,
    input: &mut mpsc::Receiver<Input>,
    peers: &mut HashMap<String, Peer>,
    owner: &mut Option<Owner>,
) -> anyhow::Result<()> {
    let cache = &state.app.cache;
    let mut stream = Box::pin(cache.reef_subscribe(&key.name()).await?);
    let mut shutdown = state.reef.shutdown.subscribe();
    // Snapshot at admission covers earlier committed mutations. Scan IDs, never an unsafe max(id) cursor.
    let mut layouts: HashSet<i64> = state.app.db.reef_layout_events(key.owner).await?.into_iter().collect();
    let mut pending: HashMap<(String, String), Instant> = HashMap::new();
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut idle = Instant::now();
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            incoming = input.recv() => {
                let Some(incoming) = incoming else { break; };
                let wire = match incoming {
                    Input::Join(peer, request) => {
                        peers.insert(peer.token.clone(), peer.clone());
                        let deadline = Instant::now() + Duration::from_secs(4);
                        while cache.reef_get(&lease_key(key)).await?.is_none() {
                            *owner = acquire(state,key).await?;
                            if owner.is_some() { break; }
                            anyhow::ensure!(Instant::now()<deadline,"lease election timeout");
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                        pending.insert((peer.token.clone(),request.clone()),Instant::now());
                        Wire::Join { token:peer.token, user:peer.user, name:peer.name, character:peer.character, request }
                    }
                    Input::Command(token, envelope) => {
                        pending.insert((token.clone(),envelope.request_id.clone()),Instant::now());
                        Wire::Command {token,envelope}
                    }
                    Input::Layout(_) => continue,
                    Input::Leave(token) => {
                        peers.remove(&token);
                        pending.retain(|(t,_),_|t!=&token);
                        Wire::Leave {token}
                    }
                };
                cache.reef_publish(&key.name(), &serde_json::to_string(&wire)?).await?;
            }
            message = stream.next() => {
                let Some(message) = message else { anyhow::bail!("subscription lost"); };
                let wire: Wire = serde_json::from_str(&message)?;
                match wire {
                    Wire::Event {token,event} => {
                        if let Some(peer) = peers.get(&token) {
                            if cache.reef_get(&lease_key(key)).await?.as_deref()!=Some(&event.epoch) { continue; }
                            if let Err(reason)=controller(state,peer).await {peer.terminate(reason);continue;}
                            pending.remove(&(token,event.request.clone()));
                            if event.body["type"] == "chat" {
                                let Some(id) = event.body["message_id"].as_str().and_then(|s|s.parse::<i64>().ok()) else {continue;};
                                if !state.app.db.reef_chat_deliverable(id, peer.user).await? {continue;}
                            }
                            if peer.outgoing.try_send(event).is_err() {peer.terminate("slow_client");}
                        }
                    }
                    Wire::Close {token,epoch,reason} => {
                        if cache.reef_get(&lease_key(key)).await?.as_deref()==Some(&epoch)
                            && let Some(peer) = peers.get(&token) { peer.terminate(close_reason(&reason)); }
                    }
                    Wire::Layout {id} => {
                        if layouts.insert(id) && let Some(actor) = owner {
                            anyhow::ensure!(actor.input.try_send(Input::Layout(id)).is_ok(),"owner queue full");
                        }
                    }
                    wire => if let Some(actor) = owner {
                        let incoming = match wire {
                            Wire::Join {token,user,name,character,request} => {
                                if actor.proxies.contains_key(&token) {
                                    // Duplicate joins ask for a snapshot; never reset position or create a second member.
                                    Input::Command(token, Envelope {protocol_version:1,request_id:request,room_epoch:actor.fence.token.to_string(),sequence:0,command:super::Command::Snapshot})
                                } else {
                                    let (peer, task) = proxy(state,key,actor.fence.token.to_string(),token.clone(),user,name,character);
                                    actor.proxies.insert(token,task);
                                    Input::Join(peer,request)
                                }
                            }
                            Wire::Command {token,envelope} => Input::Command(token,envelope),
                            Wire::Leave {token} => {
                                if let Some(task) = actor.proxies.remove(&token) {task.abort();}
                                Input::Leave(token)
                            }
                            _ => unreachable!(),
                        };
                        anyhow::ensure!(actor.input.try_send(incoming).is_ok(),"owner queue full");
                    }
                }
            }
            _ = tick.tick() => {
                if owner.as_ref().is_some_and(|o|o.task.is_finished()) { break; }
                if let Some(actor) = owner {actor.proxies.retain(|_,task|!task.is_finished());}
                if !peers.is_empty() && cache.reef_get(&lease_key(key)).await?.is_none() {break;}
                for ((token,_),at) in &pending {
                    if at.elapsed()>Duration::from_secs(3) && let Some(peer)=peers.get(token) {peer.terminate("routing_timeout");}
                }
                peers.retain(|_,p|p.close.borrow().is_none());
                pending.retain(|(t,_),_|peers.contains_key(t));
                if peers.is_empty() && owner.is_none() {
                    if idle.elapsed()>Duration::from_secs(2) {break;}
                } else {idle=Instant::now();}
                let events = state.app.db.reef_layout_events(key.owner).await?;
                for id in &events {
                    if !layouts.contains(id) {
                        cache.reef_publish(&key.name(), &serde_json::to_string(&Wire::Layout {id:*id})?).await?;
                    }
                }
                layouts.retain(|id|events.contains(id));
            }
        }
    }
    Ok(())
}
pub(super) fn close_reason(reason: &str) -> &'static str {
    match reason {
        "consent_revoked" => "consent_revoked",
        "logged_out" => "logged_out",
        "controller_replaced" => "controller_replaced",
        "access_revoked" => "access_revoked",
        "sanctioned" => "sanctioned",
        "slow_client" => "slow_client",
        "ownership_lost" => "ownership_lost",
        "routing_unavailable" => "routing_unavailable",
        _ => "room_unavailable",
    }
}
