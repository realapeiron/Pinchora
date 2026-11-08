//! Personal Picross uses the exact same rows and atomic completion as Discord.
use super::{RoomKey, games::access, spaces::limit};
use crate::{
    error::{ApiError, ApiJson, ApiResult},
    extract::CurrentSession,
    state::WebState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
};
use pinchora_db::PicrossGame;
use pinchora_game::play::picross::{self, Grid, Puzzle};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn router() -> Router<WebState> {
    Router::new()
        .route("/rooms/{room}/picross", post(open))
        .route("/rooms/{room}/picross/marks", post(mark))
}
fn invalid() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_game_action",
        "invalid_game_action",
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Open {
    mode: String,
    next_revision: Option<String>,
}
async fn check(s: &WebState, c: &CurrentSession, room: &str, mode: &str) -> ApiResult<i32> {
    let key = RoomKey::parse(room).ok_or_else(invalid)?;
    access(s, key, c.session.user_id, "picross").await?;
    match mode {
        "daily" => Ok(chrono::Utc::now().timestamp().div_euclid(86400) as i32),
        "practice" => Ok(0),
        _ => Err(invalid()),
    }
}
fn projection(g: &PicrossGame) -> Value {
    let p = Puzzle::new(g.picture as usize, g.variant as u8);
    let clues = p.clues();
    json!({"mode":g.mode,"day":g.day,"revision":g.revision.to_string(),"grid":g.grid,"checks":g.checks,"status":g.status,"paid":g.paid,"rows":clues.rows,"cols":clues.cols})
}
async fn open(
    State(s): State<WebState>,
    c: CurrentSession,
    Path(room): Path<String>,
    ApiJson(body): ApiJson<Open>,
) -> ApiResult<Json<Value>> {
    limit(&s, c.session.user_id, "picross-open", 60).await?;
    let day = check(&s, &c, &room, &body.mode).await?;
    let today = chrono::Utc::now().timestamp().div_euclid(86400);
    let (picture, variant) = if body.mode == "daily" {
        picross::daily(today)
    } else {
        let allowed = picross::practice_pictures(today);
        (
            allowed[rand::random_range(0..allowed.len())],
            rand::random_range(0..picross::VARIANTS),
        )
    };
    if let Some(revision) = body.next_revision {
        if body.mode != "practice" {
            return Err(invalid());
        }
        let g = s
            .app
            .db
            .next_picross(
                c.session.user_id,
                revision.parse().map_err(|_| invalid())?,
                picture as u8,
                variant,
            )
            .await?
            .ok_or_else(invalid)?;
        audit(&s, &c, "reef.picross.next", &g).await;
        return Ok(Json(projection(&g)));
    }
    let existing = s.app.db.picross_game(c.session.user_id, &body.mode, day).await?;
    let replace = body.mode == "practice"
        && existing
            .as_ref()
            .is_some_and(|g| !g.solved() && !picross::practice_pictures(today).contains(&(g.picture as usize)));
    let g = s
        .app
        .db
        .start_picross(c.session.user_id, &body.mode, day, (picture as u8, variant), replace)
        .await?;
    if existing.is_none() || replace {
        audit(&s, &c, "reef.picross.open", &g).await;
    }
    Ok(Json(projection(&g)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Marks {
    mode: String,
    day: i32,
    revision: String,
    grid: String,
    cell: usize,
    filled: bool,
}
async fn mark(
    State(s): State<WebState>,
    c: CurrentSession,
    Path(room): Path<String>,
    ApiJson(b): ApiJson<Marks>,
) -> ApiResult<Json<Value>> {
    limit(&s, c.session.user_id, "picross-mark", 180).await?;
    let day = check(&s, &c, &room, &b.mode).await?;
    if day != b.day || b.cell >= 100 || b.grid.len() != 100 {
        return Err(invalid());
    }
    let revision = b.revision.parse::<i64>().map_err(|_| invalid())?;
    let old = s
        .app
        .db
        .picross_game(c.session.user_id, &b.mode, day)
        .await?
        .ok_or_else(invalid)?;
    if b.mode == "practice"
        && !picross::practice_pictures(chrono::Utc::now().timestamp().div_euclid(86400))
            .contains(&(old.picture as usize))
    {
        return Err(invalid());
    }
    let mut grid = Grid::decode(&b.grid).ok_or_else(invalid)?;
    grid.set(b.cell, b.filled);
    let solved = grid.solves(&Puzzle::new(old.picture as usize, old.variant as u8).clues());
    let g = s
        .app
        .db
        .picross_marks_at(
            c.session.user_id,
            &b.mode,
            day,
            &b.grid,
            &grid.encode(),
            solved,
            Some(revision),
        )
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "stale_action", "stale_action"))?;
    audit(&s, &c, "reef.picross.mark", &g).await;
    Ok(Json(projection(&g)))
}

async fn audit(s: &WebState, c: &CurrentSession, action: &'static str, g: &PicrossGame) {
    crate::audit::finish(
        s,
        &c.session,
        crate::audit::Change {
            guild: 0,
            action,
            target: None,
            before: Value::Null,
            after: json!({"mode":g.mode,"day":g.day,"revision":g.revision,"status":g.status}),
        },
        None,
    )
    .await;
}
