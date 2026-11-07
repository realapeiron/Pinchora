//! House-only chat safety HTTP surface. CSRF/session middleware protects every write.
use crate::{
    error::{ApiError, ApiResult},
    extract::CurrentSession,
    state::WebState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
};
use pinchora_db::{REEF_CONSENT_VERSION, ReefModeration};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/consent", put(consent))
        .route("/blocks", get(blocks))
        .route("/blocks/{user}", put(block).delete(unblock))
        .route("/reports", post(report).get(receipts))
        .route("/reports/{id}/appeal", post(appeal))
        .route("/moderation", get(queue).post(moderate))
}
fn denied() -> ApiError {
    ApiError::new(StatusCode::FORBIDDEN, "access_denied", "access_denied")
}
fn invalid() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_safety_request",
        "invalid_safety_request",
    )
}
fn id(raw: &str) -> ApiResult<u64> {
    raw.parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= i64::MAX as u64 && n.to_string() == raw)
        .ok_or_else(invalid)
}
pub fn operator(state: &WebState, user: u64) -> bool {
    state.settings.operator_ids.contains(&user)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Consent {
    version: i32,
    age_band: String,
    accepted: bool,
}
async fn consent(
    State(state): State<WebState>,
    current: CurrentSession,
    Json(body): Json<Consent>,
) -> ApiResult<Json<Value>> {
    if body.version != REEF_CONSENT_VERSION || !["under_13", "13_plus"].contains(&body.age_band.as_str()) {
        return Err(invalid());
    }
    let _guard = state.reef.safety.lock().await;
    state
        .app
        .db
        .reef_consent(current.session.user_id, body.version, &body.age_band, body.accepted)
        .await?;
    let eligible = body.accepted && body.age_band == "13_plus";
    if !eligible {
        state.reef.terminate(current.session.user_id, "consent_revoked").await;
    }
    Ok(Json(json!({"eligible":eligible,"version":REEF_CONSENT_VERSION})))
}
async fn blocks(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    let ids = state
        .app
        .db
        .guestbook_blocks(current.session.user_id)
        .await?
        .into_iter()
        .map(|(id, _)| id.to_string())
        .collect::<Vec<_>>();
    Ok(Json(json!({"users":ids})))
}
async fn block(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(raw): Path<String>,
) -> ApiResult<StatusCode> {
    let user = id(&raw)?;
    if user == current.session.user_id {
        return Err(invalid());
    }
    let _guard = state.reef.safety.lock().await;
    state.app.db.block_signer(current.session.user_id, user, "").await?;
    // Generic policy notification clears local transcripts without disclosing the blocker.
    state.reef.policy_changed(current.session.user_id).await;
    state.reef.policy_changed(user).await;
    state.reef.recheck(&state).await;
    Ok(StatusCode::NO_CONTENT)
}
async fn unblock(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(raw): Path<String>,
) -> ApiResult<StatusCode> {
    let _guard = state.reef.safety.lock().await;
    state.app.db.unblock_signer(current.session.user_id, id(&raw)?).await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    message_id: String,
    reason: String,
}
async fn report(
    State(state): State<WebState>,
    current: CurrentSession,
    Json(body): Json<Report>,
) -> ApiResult<Json<Value>> {
    let receipt = state
        .app
        .db
        .reef_report(current.session.user_id, id(&body.message_id)? as i64, &body.reason)
        .await?
        .ok_or_else(denied)?;
    Ok(Json(json!({"receipt":receipt})))
}
async fn receipts(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"items":state.app.db.reef_receipts(current.session.user_id).await?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Appeal {
    text: String,
}
async fn appeal(
    State(state): State<WebState>,
    current: CurrentSession,
    Path(raw): Path<String>,
    Json(body): Json<Appeal>,
) -> ApiResult<StatusCode> {
    pinchora_game::chat::check(&body.text).map_err(|_| invalid())?;
    if !state
        .app
        .db
        .reef_appeal(current.session.user_id, id(&raw)? as i64, &body.text)
        .await?
    {
        return Err(denied());
    }
    Ok(StatusCode::NO_CONTENT)
}
async fn queue(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"items":state.app.db.reef_queue(current.session.user_id,operator(&state,current.session.user_id)).await?}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Moderate {
    target: String,
    owner: Option<String>,
    room_idx: Option<i16>,
    kind: String,
    seconds: i32,
    report: Option<String>,
}
async fn moderate(
    State(state): State<WebState>,
    current: CurrentSession,
    Json(body): Json<Moderate>,
) -> ApiResult<StatusCode> {
    let action = ReefModeration {
        actor: current.session.user_id,
        operator: operator(&state, current.session.user_id),
        target: id(&body.target)?,
        owner: body.owner.as_deref().map(id).transpose()?,
        idx: body.room_idx,
        kind: body.kind,
        seconds: body.seconds,
        report: body.report.as_deref().map(id).transpose()?.map(|n| n as i64),
    };
    let _guard = state.reef.safety.lock().await;
    if !state.app.db.reef_moderate(&action).await? {
        return Err(denied());
    }
    if action.owner.is_none() && ["ban", "kick"].contains(&action.kind.as_str()) {
        state.reef.terminate(action.target, "sanctioned").await;
    }
    state.reef.recheck(&state).await;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn retention(app: std::sync::Arc<pinchora_commands::App>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
    loop {
        interval.tick().await;
        loop {
            match app.db.reef_retain(chrono::Utc::now()).await {
                Ok(500) => tokio::task::yield_now().await,
                Ok(_) => break,
                Err(_) => {
                    tracing::warn!("Reef retention job failed");
                    break;
                }
            }
        }
    }
}
