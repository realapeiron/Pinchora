use super::{Command, Envelope, Event, Input, MAX_MESSAGE, Peer, QUEUE, RoomKey};
use crate::{
    auth::session,
    error::{ApiError, ApiResult},
    extract::CurrentSession,
    state::WebState,
};
use axum::{
    Json,
    extract::{
        State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::{HeaderMap, header::ORIGIN},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

pub fn exact_origin(headers: &HeaderMap, expected: &str) -> bool {
    let mut values = headers.get_all(ORIGIN).iter();
    matches!((values.next(),values.next()),(Some(value),None) if value.to_str().ok()==Some(expected))
}
pub async fn ticket(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    let token = session::random_token(32);
    state
        .app
        .cache
        .put(
            &state.app.cache.reef_key(&format!("ticket:{token}")),
            &current.sid,
            Duration::from_secs(30),
        )
        .await?;
    Ok(Json(json!({"ticket":token,"expires_in":30,"protocol_version":1})))
}
pub async fn consume(cache: &pinchora_cache::Cache, ticket: &str, sid: &str) -> anyhow::Result<bool> {
    if ticket.len() != 43
        || !ticket
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Ok(false);
    }
    Ok(cache
        .take(&cache.reef_key(&format!("ticket:{ticket}")))
        .await?
        .as_deref()
        == Some(sid))
}
pub async fn upgrade(
    State(state): State<WebState>,
    current: CurrentSession,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    if !exact_origin(&headers, &state.settings.origin) {
        return Err(ApiError::csrf());
    }
    Ok(ws
        .max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .max_write_buffer_size(64 * 1024)
        .write_buffer_size(0)
        .on_upgrade(move |socket| connection(state, current, socket)))
}
async fn connection(state: WebState, current: CurrentSession, mut socket: WebSocket) {
    let authenticated = tokio::time::timeout(Duration::from_secs(5), async {
        let Some(Ok(Message::Text(text))) = socket.recv().await else {
            return None;
        };
        let envelope = Envelope::parse(&text)?;
        if envelope.protocol_version != 1
            || envelope.sequence != 1
            || !envelope.room_epoch.is_empty()
            || envelope.request_id.is_empty()
            || envelope.request_id.len() > 64
        {
            return None;
        }
        let Command::Authenticate { ticket } = envelope.command else {
            return None;
        };
        if !consume(&state.app.cache, &ticket, &current.sid).await.ok()? {
            return None;
        }
        let saved = session::load(&state.app.cache, &current.sid, chrono::Utc::now().timestamp())
            .await
            .ok()??;
        if saved.user_id != current.session.user_id {
            return None;
        }
        let character = state.app.db.character(saved.user_id).await.ok()??;
        Some((saved, character, envelope.request_id))
    })
    .await;
    let Ok(Some((saved, character, request))) = authenticated else {
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: 1008,
                reason: "authentication_failed".into(),
            })))
            .await;
        return;
    };
    let (outgoing, mut incoming) = mpsc::channel::<Event>(QUEUE);
    let (close, mut closed) = watch::channel(None);
    let peer = Peer {
        token: session::random_token(16),
        user: saved.user_id,
        name: pinchora_game::chat::display_name(saved.display_name()),
        character,
        outgoing,
        close,
    };
    let Ok(control) = state.app.cache.reef_subscribe(&format!("control:{}", peer.user)).await else {
        return;
    };
    let mut control = Box::pin(control);
    if state.reef.claim(&state, &peer).await.is_err() {
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: 4001,
                reason: "routing_unavailable".into(),
            })))
            .await;
        return;
    }
    peer.send(
        "",
        &request,
        json!({"type":"authenticated","user_id":peer.user.to_string()}),
    );
    let (mut writer, mut reader) = socket.split();
    let writer_peer = peer.clone();
    let writer_task = tokio::spawn(async move {
        let mut sequence = 0u64;
        let mut delivery = 0u64;
        let mut delivery_stream = String::new();
        loop {
            tokio::select! {
                biased;
                changed=closed.changed() => {
                    if changed.is_ok() {
                        let reason=(*closed.borrow()).unwrap_or("closed");
                        let _=tokio::time::timeout(Duration::from_secs(2),writer.send(Message::Close(Some(CloseFrame {code:4001,reason:reason.into()})))).await;
                    }
                    break;
                }
                event=incoming.recv() => {
                    let Some(event)=event else { break; };
                    if event.delivery > 0 {
                        if delivery_stream != event.stream { delivery=0; delivery_stream=event.stream.clone(); }
                        if event.delivery <= delivery {continue;}
                        sequence += event.delivery - delivery;
                        delivery = event.delivery;
                    } else { sequence+=1; }
                    let mut body=event.body;
                    body["protocol_version"]=json!(1); body["request_id"]=json!(event.request);
                    body["room_epoch"]=json!(event.epoch); body["sequence"]=json!(sequence);
                    if !matches!(tokio::time::timeout(Duration::from_secs(2),writer.send(Message::Text(body.to_string().into()))).await,Ok(Ok(()))) {
                        writer_peer.terminate("slow_client");
                        let _=tokio::time::timeout(Duration::from_secs(2),writer.send(Message::Close(Some(CloseFrame {code:4001,reason:"slow_client".into()})))).await;
                        break;
                    }
                }
            }
        }
    });
    let mut current_room: Option<mpsc::Sender<Input>> = None;
    let mut current_key: Option<RoomKey> = None;
    let mut sequence = 1;
    let mut seen = Instant::now();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    heartbeat.tick().await;
    let mut validity = tokio::time::interval(Duration::from_secs(1));
    let mut shutdown = peer.close.subscribe();
    let mut draining = state.reef.shutdown.subscribe();
    loop {
        tokio::select! {
            _=shutdown.changed() => break,
            _=draining.changed() => {peer.terminate("pod_draining");break;},
            message=control.next() => {
                match message {
                    Some(message) => {
                        if let Ok(body) = serde_json::from_str::<Value>(&message) {
                            if let Some(reason)=body["close"].as_str() {peer.terminate(super::routing::close_reason(reason));break;}
                            peer.send("","",json!({"type":"policy_changed"}));
                        }
                    }
                    None => {peer.terminate("routing_unavailable");break;}
                }
            },
            _=tokio::time::sleep_until(tokio::time::Instant::from_std(seen + Duration::from_secs(45))) => {
                peer.terminate("presence_expired"); break;
            }
            _=validity.tick() => {
                if let Some(key)=current_key
                    && !matches!(state.app.cache.reef_renew(&super::routing::location_key(&peer.token), &key.name(), 5000).await,Ok(true)) {
                    peer.terminate("routing_unavailable");break;
                }
                match state.app.cache.reef_renew(&super::routing::controller_key(peer.user), &peer.token, 5000).await {
                    Ok(true) => {},
                    Ok(false) => {peer.terminate(super::routing::controller(&state,&peer).await.err().unwrap_or("routing_unavailable"));break;},
                    Err(_) => {peer.terminate("routing_unavailable");break;}
                }
                match session::load(&state.app.cache,&current.sid,chrono::Utc::now().timestamp()).await {
                    Ok(Some(s)) if s.user_id==peer.user => {},
                    _ => { peer.terminate("session_expired"); break; }
                }
            }
            _=heartbeat.tick() => {
                if writer_task.is_finished() {
                    peer.terminate("presence_expired"); break;
                }
                match session::load(&state.app.cache,&current.sid,chrono::Utc::now().timestamp()).await {
                    Ok(Some(s)) if s.user_id==peer.user => {},
                    _ => { peer.terminate("session_expired"); break; }
                }
                peer.send("","",json!({"type":"ping"}));
            }
            message=reader.next() => {
                let Some(Ok(message))=message else { break; };
                if peer.close.borrow().is_some() { break; }
                let Message::Text(text)=message else {
                    if matches!(message,Message::Close(_)) { break; }
                    continue;
                };
                let Some(envelope)=Envelope::parse(&text) else { peer.terminate("invalid_message"); break; };
                if envelope.protocol_version!=1 || envelope.sequence!=sequence+1 || envelope.request_id.is_empty() || envelope.request_id.len()>64 || envelope.room_epoch.len()>64 {
                    peer.terminate("invalid_envelope"); break;
                }
                sequence=envelope.sequence;
                seen=Instant::now();
                if !state.reef.allow(peer.user,&envelope.command).await { peer.error(&envelope.room_epoch,&envelope.request_id,"rate_limited"); continue; }
                match &envelope.command {
                    Command::Authenticate{..} => { peer.error("",&envelope.request_id,"already_authenticated"); }
                    Command::Ping => {},
                    Command::Join {room} => {
                        if !matches!(session::load(&state.app.cache, &current.sid, chrono::Utc::now().timestamp()).await,
                            Ok(Some(s)) if s.user_id == peer.user) {
                            peer.terminate("session_expired"); break;
                        }
                        let Some(key)=RoomKey::parse(room) else {peer.error("",&envelope.request_id,"invalid_room"); continue;};
                        // Check before leaving the old room. Actor repeats this check at admission.
                        if room_access(&state,key,peer.user).await.is_err() {peer.error("",&envelope.request_id,"access_denied");continue;}
                        if let Some(tx)=current_room.take() { let _=tx.send(Input::Leave(peer.token.clone())).await; }
                        if state.app.cache.reef_claim(&super::routing::location_key(&peer.token), &key.name(), 5000, true).await.is_err() {
                            peer.terminate("routing_unavailable");break;
                        }
                        current_key=Some(key);
                        let tx=state.reef.room(&state,key).await;
                        if tx.try_send(Input::Join(peer.clone(),envelope.request_id)).is_err() {peer.terminate("room_busy");break;}
                        state.reef.locate(peer.user,key).await;
                        current_room=Some(tx);
                    }
                    Command::Leave => {
                        if let Some(key)=current_key.take() {
                            let _=state.app.cache.reef_release(&super::routing::location_key(&peer.token), &key.name()).await;
                        }
                        state.reef.unlocate(peer.user).await;
                        if let Some(tx)=current_room.take() {let _=tx.send(Input::Leave(peer.token.clone())).await;}
                        peer.send("",&envelope.request_id,json!({"type":"left"}));
                    }
                    _ => {
                        if let Some(tx)=&current_room {
                            if tx.try_send(Input::Command(peer.token.clone(),envelope)).is_err() {peer.terminate("room_busy");break;}
                        } else {peer.error("",&envelope.request_id,"not_joined");}
                    }
                }
            }
        }
    }
    if let Some(tx) = current_room {
        let _ = tx.send(Input::Leave(peer.token.clone())).await;
    }
    if let Some(key) = current_key {
        let _ = state
            .app
            .cache
            .reef_release(&super::routing::location_key(&peer.token), &key.name())
            .await;
    }
    state.reef.release(&state, &peer).await;
    if peer.close.borrow().is_none() {
        peer.terminate("closed");
    }
    let _ = writer_task.await;
}
pub(super) async fn room_access(state: &WebState, key: RoomKey, user: u64) -> anyhow::Result<()> {
    anyhow::ensure!(state.app.db.character(user).await?.is_some(), "access denied");
    anyhow::ensure!(
        !state.app.db.reef_sanctioned(user, key.owner, key.idx, false).await?,
        "sanctioned"
    );
    anyhow::ensure!(!state.app.db.is_guild(key.owner).await?, "access denied");
    if key.owner == user {
        return Ok(());
    }
    anyhow::ensure!(!state.app.db.privacy(key.owner).await?.hide_house, "access denied");
    let (house, rooms) = state
        .app
        .db
        .house_if_any(key.owner)
        .await?
        .ok_or_else(|| anyhow::anyhow!("access denied"))?;
    anyhow::ensure!(!house.locked && rooms.iter().any(|r| r.idx == key.idx), "access denied");
    anyhow::ensure!(!state.app.db.socially_blocked(key.owner, user).await?, "access denied");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn redis_ticket_is_consumed_once_and_bound_to_the_session() {
        let Ok(url) = std::env::var("PINCHORA_TEST_REDIS_URL") else {
            return;
        };
        let cache = pinchora_cache::Cache::connect(&url).await.expect("test Redis");
        let ticket = session::random_token(32);
        cache
            .put(
                &cache.reef_key(&format!("ticket:{ticket}")),
                "session-a",
                Duration::from_secs(30),
            )
            .await
            .unwrap();
        let (a, b) = tokio::join!(
            consume(&cache, &ticket, "session-a"),
            consume(&cache, &ticket, "session-a")
        );
        assert_ne!(a.unwrap(), b.unwrap());
        let ticket = session::random_token(32);
        cache
            .put(
                &cache.reef_key(&format!("ticket:{ticket}")),
                "session-a",
                Duration::from_secs(30),
            )
            .await
            .unwrap();
        assert!(!consume(&cache, &ticket, "session-b").await.unwrap());
        assert!(!consume(&cache, &ticket, "session-a").await.unwrap());
    }
    #[test]
    fn origin_is_exact_and_required() {
        let mut h = HeaderMap::new();
        assert!(!exact_origin(&h, "https://reef.test"));
        for raw in [
            "null",
            "https://reef.test.evil",
            "http://reef.test",
            "https://reef.test/",
            "https://reef.test:443",
        ] {
            h.insert(ORIGIN, raw.parse().unwrap());
            assert!(!exact_origin(&h, "https://reef.test"));
        }
        h.insert(ORIGIN, "https://reef.test".parse().unwrap());
        assert!(exact_origin(&h, "https://reef.test"));
        h.append(ORIGIN, "https://reef.test".parse().unwrap());
        assert!(!exact_origin(&h, "https://reef.test"));
    }
}
