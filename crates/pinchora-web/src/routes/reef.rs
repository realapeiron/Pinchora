//! Reef transport endpoints; all persistent writes remain in the existing game services.
use crate::{error::ApiResult, extract::CurrentSession, reef, state::WebState};
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
pub fn router() -> Router<WebState> {
    Router::new()
        .merge(reef::safety::router())
        .route("/bootstrap", get(bootstrap))
        .route("/socket-ticket", post(reef::ticket))
        .route("/ws", get(reef::upgrade))
}

/// Saved appearance does not require owning a house; visiting is authorized when joining.
async fn bootstrap(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "protocol_version": 1,
        "navigation_version": 1,
        "user_id": current.session.user_id.to_string(),
        "character": state.app.db.character(current.session.user_id).await?,
        "chat_enabled": state.app.db.reef_eligible(current.session.user_id).await?,
        "consent_version": pinchora_db::REEF_CONSENT_VERSION,
        "operator": reef::safety::operator(&state,current.session.user_id),
        "display_name": pinchora_game::chat::display_name(current.session.display_name())
    })))
}
