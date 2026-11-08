//! HTTP and WebSocket share this adapter; no transport accepts a score or reward amount.
use super::{RoomKey, socket::room_access, spaces::limit};
use crate::{
    error::{ApiError, ApiJson, ApiResult},
    extract::CurrentSession,
    state::WebState,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    routing::{get, post},
};
use pinchora_core::{i18n::Lang, modules::Module};
use pinchora_db::{NewReefGame, ReefFence, ReefGame, ReefGamePlayer, ReefGameStep};
use pinchora_game::play::room::Session;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/rooms/{room}/games", get(lobby).post(create))
        .route("/games/{id}", get(read))
        .route("/games/{id}/actions", post(act))
        .merge(super::picross::router())
        .layer(DefaultBodyLimit::max(4096))
}
fn error(code: &'static str, status: StatusCode) -> ApiError {
    ApiError::new(status, code, code)
}
fn invalid() -> ApiError {
    error("invalid_game_action", StatusCode::UNPROCESSABLE_ENTITY)
}
fn map_error(e: anyhow::Error) -> ApiError {
    match e.to_string().as_str() {
        "stale_action" | "request_conflict" => error("stale_action", StatusCode::CONFLICT),
        "not_your_turn" | "spectator" | "access_denied" => error("access_denied", StatusCode::FORBIDDEN),
        "invalid_room" | "illegal_move" | "already_answered" | "invalid_action" | "game_finished" | "room_busy"
        | "stale_layout" => invalid(),
        _ => e.into(),
    }
}
fn number(s: &str) -> ApiResult<i64> {
    s.parse::<i64>()
        .ok()
        .filter(|n| *n > 0 && n.to_string() == s)
        .ok_or_else(invalid)
}
fn request(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub(super) async fn access(s: &WebState, key: RoomKey, user: u64, kind: &str) -> ApiResult<()> {
    room_access(s, key, user)
        .await
        .map_err(|_| error("access_denied", StatusCode::FORBIDDEN))?;
    if key.instance.is_some() {
        let module = if matches!(kind, "trivia" | "picross") {
            Module::Puzzles
        } else {
            Module::BoardGames
        };
        if !s.app.db.guild_modules(key.owner).await?.rule(module).enabled {
            return Err(error("access_denied", StatusCode::FORBIDDEN));
        }
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    kind: String,
    request_id: String,
    placement_id: Option<String>,
    layout_revision: Option<String>,
    lang: Option<String>,
}
async fn create(
    State(s): State<WebState>,
    c: CurrentSession,
    Path(room): Path<String>,
    ApiJson(body): ApiJson<Create>,
) -> ApiResult<Json<Value>> {
    limit(&s, c.session.user_id, "game-create", 12).await?;
    let key = RoomKey::parse(&room).ok_or_else(invalid)?;
    access(&s, key, c.session.user_id, &body.kind).await?;
    if !request(&body.request_id) {
        return Err(invalid());
    }
    let lang = body
        .lang
        .as_deref()
        .map(Lang::from_code)
        .unwrap_or(Some(Lang::En))
        .ok_or_else(invalid)?;
    let session = Session::new(&body.kind, lang, rand::random()).ok_or_else(invalid)?;
    let placement = body.placement_id.as_deref().map(number).transpose()?;
    if let Some(id) = placement {
        let rows = s.app.db.placements(key.owner, key.idx).await?;
        let p = rows.iter().find(|p| p.id == id).ok_or_else(invalid)?;
        let item = pinchora_game::Content::get().item(&p.item_id).ok_or_else(invalid)?;
        if !item.base.has_tag("game") && !item.base.has_tag("table") {
            return Err(invalid());
        }
    }
    let revision = body
        .layout_revision
        .as_deref()
        .map(|r| r.parse::<i64>().map_err(|_| invalid()))
        .transpose()?;
    let game = s
        .app
        .db
        .reef_create_game(NewReefGame {
            room: &room,
            owner: key.owner,
            host: c.session.user_id,
            kind: &body.kind,
            request: &body.request_id,
            placement,
            layout_revision: revision,
            state: serde_json::to_value(session).map_err(anyhow::Error::from)?,
        })
        .await
        .map_err(map_error)?;
    crate::audit::finish(
        &s,
        &c.session,
        crate::audit::Change {
            guild: if key.instance.is_some() { key.owner } else { 0 },
            action: "reef.game.create",
            target: Some(&game.id.to_string()),
            before: Value::Null,
            after: json!({"kind":body.kind}),
        },
        None,
    )
    .await;
    Ok(Json(project(&s, &game, c.session.user_id).await?))
}
async fn lobby(State(s): State<WebState>, c: CurrentSession, Path(room): Path<String>) -> ApiResult<Json<Value>> {
    limit(&s, c.session.user_id, "game-read", 240).await?;
    let key = RoomKey::parse(&room).ok_or_else(invalid)?;
    room_access(&s, key, c.session.user_id)
        .await
        .map_err(|_| error("access_denied", StatusCode::FORBIDDEN))?;
    let mut games = vec![];
    for g in s.app.db.reef_room_games(&room).await? {
        if access(&s, key, c.session.user_id, &g.kind).await.is_ok() && visible(&s, g.id, c.session.user_id).await? {
            let g = expire(&s, g, c.session.user_id).await?;
            games.push(project(&s, &g, c.session.user_id).await?);
        }
    }
    Ok(Json(
        json!({"games":games,"layout_revision":s.app.db.reef_edit_revision(key.owner).await?}),
    ))
}
async fn read(State(s): State<WebState>, c: CurrentSession, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    limit(&s, c.session.user_id, "game-read", 240).await?;
    let g = load(&s, number(&id)?, c.session.user_id).await?;
    let g = expire(&s, g, c.session.user_id).await?;
    Ok(Json(project(&s, &g, c.session.user_id).await?))
}
async fn visible(s: &WebState, id: i64, user: u64) -> ApiResult<bool> {
    for p in s.app.db.reef_game_players(id).await? {
        if p.user_id != user as i64 && s.app.db.socially_blocked(user, p.user_id as u64).await? {
            return Ok(false);
        }
    }
    Ok(true)
}
async fn load(s: &WebState, id: i64, user: u64) -> ApiResult<ReefGame> {
    let g = s
        .app
        .db
        .reef_game(id)
        .await?
        .ok_or_else(|| error("game_missing", StatusCode::NOT_FOUND))?;
    let key = RoomKey::parse(&g.room_key).ok_or_else(invalid)?;
    access(s, key, user, &g.kind).await?;
    if !visible(s, id, user).await? {
        return Err(error("access_denied", StatusCode::FORBIDDEN));
    }
    Ok(g)
}
#[derive(Debug, Clone, Serialize)]
pub struct Action {
    pub request_id: String,
    pub revision: String,
    #[serde(flatten)]
    pub action: Intent,
}
impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut value = Value::deserialize(deserializer)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| serde::de::Error::custom("expected action object"))?;
        let request_id = object
            .remove("request_id")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(|| serde::de::Error::custom("request_id required"))?;
        let revision = object
            .remove("revision")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(|| serde::de::Error::custom("revision required"))?;
        let action = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            request_id,
            revision,
            action,
        })
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Intent {
    Join,
    Leave,
    Start,
    Move { code: String },
    Answer { choice: u8 },
    Refresh,
}
async fn act(
    State(s): State<WebState>,
    c: CurrentSession,
    Path(id): Path<String>,
    ApiJson(a): ApiJson<Action>,
) -> ApiResult<Json<Value>> {
    let value = action(&s, c.session.user_id, number(&id)?, a, None, None).await?;
    crate::audit::finish(
        &s,
        &c.session,
        crate::audit::Change {
            guild: 0,
            action: "reef.game.action",
            target: Some(&id),
            before: Value::Null,
            after: json!({"accepted":true}),
        },
        None,
    )
    .await;
    Ok(Json(value))
}
pub async fn action(
    s: &WebState,
    user: u64,
    id: i64,
    a: Action,
    room: Option<RoomKey>,
    fence: Option<&ReefFence>,
) -> ApiResult<Value> {
    limit(s, user, "game-action", 120).await?;
    if !request(&a.request_id) || a.revision.len() > 20 || matches!(&a.action,Intent::Move{code} if code.len()>64) {
        return Err(invalid());
    }
    let revision = a.revision.parse::<i64>().map_err(|_| invalid())?;
    let g = load(s, id, user).await?;
    if room.is_some_and(|key| key.name() != g.room_key) {
        return Err(error("access_denied", StatusCode::FORBIDDEN));
    }
    let payload = serde_json::to_value(&a).map_err(anyhow::Error::from)?;
    let result = s
        .app
        .db
        .reef_game_step(id, user, &a.request_id, &payload, fence, |g, p, now| {
            transition(g, p, user, revision, &a.action, now)
        })
        .await
        .map_err(map_error)?;
    project(s, &result, user).await
}
fn transition(
    g: &ReefGame,
    players: &[ReefGamePlayer],
    user: u64,
    revision: i64,
    intent: &Intent,
    now: i64,
) -> anyhow::Result<ReefGameStep> {
    let mut state: Session = serde_json::from_value(g.state.clone()).map_err(anyhow::Error::from)?;
    let me = players.iter().find(|p| p.user_id == user as i64);
    let mut join = false;
    let mut answer = None;
    let mut seconds = 0;
    let mut winners = vec![];
    anyhow::ensure!(!g.finished, "game_finished");
    if now >= g.deadline.timestamp() {
        if state.phase == "active" {
            if let Some(turn) = state.turn() {
                state.finish(Some(1 - turn), "timeout");
            } else {
                state.finish(None, "reveal");
            }
        } else {
            state.finish(None, "expired");
        }
    } else {
        // Answers are independently single-use: another user's answer does not stale yours.
        if !matches!(intent, Intent::Answer { .. }) {
            anyhow::ensure!(revision == g.revision, "stale_action");
        }
        match intent {
            Intent::Join => {
                anyhow::ensure!(state.phase == "waiting" && me.is_none(), "invalid_action");
                let max = if g.kind == "trivia" { 32 } else { 2 };
                anyhow::ensure!(players.len() < max, "invalid_action");
                join = true;
                if max == 2 {
                    state.phase = "active".into();
                    seconds = state.seconds();
                }
            }
            Intent::Start => {
                anyhow::ensure!(
                    g.kind == "trivia" && state.phase == "waiting" && g.host_id == user as i64 && players.len() >= 2,
                    "invalid_action"
                );
                state.phase = "active".into();
                seconds = state.seconds();
            }
            Intent::Leave => {
                let me = me.ok_or_else(|| anyhow::anyhow!("spectator"))?;
                state.finish(
                    if state.phase == "active" && g.kind != "trivia" {
                        Some(1 - me.side)
                    } else {
                        None
                    },
                    "resigned",
                );
            }
            Intent::Move { code } => {
                let me = me.ok_or_else(|| anyhow::anyhow!("spectator"))?;
                state.play(me.side, code)?;
                seconds = state.seconds();
            }
            Intent::Answer { choice } => {
                let me = me.ok_or_else(|| anyhow::anyhow!("spectator"))?;
                anyhow::ensure!(
                    g.kind == "trivia" && state.phase == "active" && *choice < 4 && me.answer.is_none(),
                    "invalid_action"
                );
                answer = Some(i16::from(*choice));
                if players.iter().all(|p| p.user_id == user as i64 || p.answer.is_some()) {
                    state.finish(None, "reveal");
                }
            }
            Intent::Refresh => anyhow::bail!("invalid_action"),
        }
    }
    if state.phase == "done" {
        if let Some(w) = state.winner {
            winners.push(w);
        }
        if g.kind == "trivia" && state.reason.as_deref() == Some("reveal") {
            let correct = state.question().and_then(|(q, o)| q.correct_slot(o)).map(|n| n as i16);
            for p in players {
                let a = if p.user_id == user as i64 {
                    answer.or(p.answer)
                } else {
                    p.answer
                };
                if a.is_some() && a == correct {
                    winners.push(p.side);
                }
            }
        }
    }
    let reward = state.reward(g.room_key.starts_with("plaza:"), players.len());
    Ok(ReefGameStep {
        finished: state.phase == "done",
        reward,
        cap: state.kind().daily_cap(),
        state: serde_json::to_value(state)?,
        seconds,
        join,
        answer,
        winners,
    })
}
async fn expire(s: &WebState, g: ReefGame, user: u64) -> ApiResult<ReefGame> {
    if g.finished || g.deadline > chrono::Utc::now() {
        return Ok(g);
    }
    let req = format!("expire-{}", g.revision);
    match s
        .app
        .db
        .reef_game_step(g.id, user, &req, &json!({"action":"expire"}), None, |g, p, now| {
            transition(g, p, user, g.revision, &Intent::Refresh, now)
        })
        .await
    {
        Ok(g) => Ok(g),
        Err(e) if e.to_string() == "game_finished" => s.app.db.reef_game(g.id).await?.ok_or_else(invalid),
        Err(e) => Err(map_error(e)),
    }
}
async fn project(s: &WebState, g: &ReefGame, user: u64) -> ApiResult<Value> {
    let session: Session = serde_json::from_value(g.state.clone()).map_err(anyhow::Error::from)?;
    let players = s.app.db.reef_game_players(g.id).await?;
    let me = players.iter().find(|p| p.user_id == user as i64);
    Ok(
        json!({"id":g.id.to_string(),"room":g.room_key,"kind":g.kind,"revision":g.revision.to_string(),
        "placement_id":g.placement_id.map(|id|id.to_string()),"host_id":g.host_id.to_string(),"deadline":g.deadline.to_rfc3339(),
        "practice":!g.room_key.starts_with("plaza:"),"side":me.map(|p|p.side),"answered":me.is_some_and(|p|p.answer.is_some()),
        "players":players.iter().map(|p|json!({"id":p.user_id.to_string(),"side":p.side,"paid":p.paid,"answered":p.answer.is_some()})).collect::<Vec<_>>(),"board":session.projection()}),
    )
}

/// Persisted deadlines settle even when every browser has gone away. Multiple pods race safely.
pub async fn sweep(app: std::sync::Arc<pinchora_commands::App>) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        tick.tick().await;
        let Ok(games) = app.db.reef_late_games().await else {
            continue;
        };
        for g in games {
            let request = format!("expire-{}", g.revision);
            let result = app
                .db
                .reef_game_step(
                    g.id,
                    g.host_id as u64,
                    &request,
                    &json!({"action":"expire"}),
                    None,
                    |row, players, now| transition(row, players, g.host_id as u64, row.revision, &Intent::Refresh, now),
                )
                .await;
            if let Err(e) = result
                && e.to_string() != "game_finished"
                && e.to_string() != "game_missing"
            {
                tracing::warn!("Reef game expiry will retry");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_actions_reject_unknown_scores_and_unbounded_answers() {
        let a = json!({"request_id":"one","revision":"0","action":"move","code":"e2e4"});
        assert!(serde_json::from_value::<Action>(a.clone()).is_ok());
        let mut forged = a;
        forged["reward"] = json!(900);
        assert!(serde_json::from_value::<Action>(forged).is_err());
        assert!(
            serde_json::from_value::<Action>(json!({"request_id":"x","revision":"0","action":"answer","choice":256}))
                .is_err()
        );
    }
    #[test]
    fn server_deadline_precedes_stale_or_late_move() {
        let mut session = Session::new("chess", Lang::En, 0).unwrap();
        session.phase = "active".into();
        let g = ReefGame {
            id: 1,
            room_key: "house:1:0".into(),
            owner_id: 1,
            host_id: 1,
            kind: "chess".into(),
            placement_id: None,
            layout_revision: None,
            state: serde_json::to_value(session).unwrap(),
            revision: 3,
            finished: false,
            deadline: chrono::DateTime::from_timestamp(100, 0).unwrap(),
        };
        let p = vec![
            ReefGamePlayer {
                user_id: 1,
                side: 0,
                answer: None,
                paid: 0,
            },
            ReefGamePlayer {
                user_id: 2,
                side: 1,
                answer: None,
                paid: 0,
            },
        ];
        let next = transition(&g, &p, 1, 3, &Intent::Move { code: "e2e4".into() }, 100).unwrap();
        assert!(next.finished);
        assert_eq!(next.winners, vec![1]);
        assert_eq!(next.reward, 0);
        assert!(transition(&g, &p, 1, 2, &Intent::Move { code: "e2e4".into() }, 99).is_err());
    }
}
