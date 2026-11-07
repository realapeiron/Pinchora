//! Versioned exports and strictly read-only previews; every endpoint requires Manage Server.
use super::command_control::words;
use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    state::WebState,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, header},
    routing::{get, post},
};
use pinchora_core::{
    config_backup::{Import, MAX_BYTES},
    t,
};
use serde_json::{Value, json};
pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(show))
        .route("/export", get(export))
        .route("/preview", post(preview))
        .route("/import", post(apply))
        .route("/restore/{point}", get(point))
        .layer(DefaultBodyLimit::max(MAX_BYTES + 1024))
}
async fn show(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    Ok(Json(
        json!({"points":state.app.db.config_restore_points(managed.id).await?,"strings":words::strings(lang,"config_backup")}),
    ))
}
async fn export(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<(HeaderMap, Json<Value>)> {
    let lang = state.app.guild_lang(managed.id).await;
    pinchora_commands::backup_rate_limit(&state.app, managed.id, "export", 10, lang)
        .await
        .map_err(|e| ApiError::blocked(e.to_string()))?;
    let names = pinchora_commands::backup_names(&state.app, managed.id).await?;
    let backup = state.app.db.export_config(managed.id, names).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"pinchora-{}-v1.json\"", managed.id))
            .map_err(|e| ApiError::internal(e.into()))?,
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok((headers, Json(json!(backup))))
}
async fn preview(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<Import>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let (_, preview) =
        pinchora_commands::backup_preview(&state.app, managed.id, &body, lang, managed.current.session.user_id)
            .await
            .map_err(|e| ApiError::blocked(e.to_string()))?;
    Ok(Json(json!(preview)))
}
async fn apply(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<Import>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let point = pinchora_commands::backup_apply(&state.app, managed.id, &body, lang, managed.current.session.user_id)
        .await
        .map_err(|e| ApiError::blocked(e.to_string()))?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "backup.import",
            target: None,
            before: Value::Null,
            after: json!({"restore_point":point.to_string(),"policy":body.policy}),
        },
        Some(pinchora_cache::Invalidation::Guild(managed.id)),
    )
    .await;
    Ok(Json(
        json!({"message":t!(lang,"config_backup.saved"),"restore_point":point.to_string()}),
    ))
}
async fn point(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<std::collections::HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let id = params
        .get("point")
        .and_then(|p| p.parse().ok())
        .ok_or_else(ApiError::not_found)?;
    let point = state
        .app
        .db
        .config_restore_point(managed.id, id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(json!(point)))
}
