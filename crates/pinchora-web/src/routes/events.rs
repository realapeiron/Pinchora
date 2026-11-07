//! Events dashboard routes, scoped to a server the session can manage.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use pinchora_core::{
    events::{Recurrence, RsvpOption},
    settings::FieldError,
};
use pinchora_db::{EventDraft, EventRow};
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::id::Id;

use crate::{
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    state::WebState,
};

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/timezone", post(timezone))
        .route("/{id}", get(one).put(update).delete(cancel))
        .route("/{id}/attendees", get(attendees))
}

#[derive(Deserialize)]
struct EventPath {
    id: i64,
}
#[derive(Deserialize)]
struct TimezoneBody {
    timezone: String,
}
#[derive(Deserialize)]
struct EventBody {
    title: String,
    description: String,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    tz: String,
    location: String,
    channel_id: Option<String>,
    required_role_id: Option<String>,
    options: Vec<RsvpOption>,
    recurrence: Option<Recurrence>,
    sync: bool,
}
fn bad(field: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::invalid(vec![FieldError::new(field, message)])
}
fn sync_error(error: anyhow::Error) -> ApiError {
    if error.to_string().contains("Manage Events") {
        ApiError::blocked("The bot needs Manage Events to sync Discord Scheduled Events.")
    } else {
        ApiError::blocked(format!("Discord Scheduled Event sync failed: {error}"))
    }
}
fn id(raw: &str, field: &'static str) -> ApiResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| bad(field, "Choose a valid Discord ID."))
}
fn draft(body: EventBody) -> ApiResult<EventDraft> {
    if body.channel_id.is_none() {
        return Err(bad("channel_id", "Choose a channel for the RSVP card."));
    }
    let d = EventDraft {
        title: body.title,
        description: body.description,
        starts_at: body.starts_at,
        ends_at: body.ends_at,
        tz: body.tz,
        location: body.location,
        channel_id: body.channel_id.as_deref().map(|s| id(s, "channel_id")).transpose()?,
        host_id: None,
        options: body.options,
        required_role_id: body
            .required_role_id
            .as_deref()
            .map(|s| id(s, "required_role_id"))
            .transpose()?,
        reminders: vec![60, 15],
        recurrence: body.recurrence,
    };
    d.validate().map_err(|e| bad("event", e.to_string()))?;
    Ok(d)
}
fn view(row: &EventRow) -> Value {
    json!({"id":row.id.to_string(),"title":row.title,"description":row.description,
    "starts_at":row.starts_at,"ends_at":row.ends_at,"tz":row.tz,"location":row.location,
    "channel_id":row.channel_id.map(|n|n.to_string()),"message_id":row.message_id.map(|n|n.to_string()),
    "required_role_id":row.required_role_id.map(|n|n.to_string()),"options":row.options(),
    "recurrence":row.recurrence(),"status":row.status,"discord_event_id":row.discord_event_id.map(|n|n.to_string())})
}
async fn list(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let tz = state.app.db.event_timezone(managed.id).await?;
    let events = state.app.db.events(managed.id, true).await?;
    Ok(Json(
        json!({"timezone":tz,"events":events.iter().map(view).collect::<Vec<_>>()}),
    ))
}
async fn timezone(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<TimezoneBody>,
) -> ApiResult<Json<Value>> {
    if body.timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err(bad("timezone", "Use an IANA time zone such as Europe/Istanbul."));
    }
    state.app.db.set_event_timezone(managed.id, &body.timezone).await?;
    Ok(Json(json!({"timezone":body.timezone})))
}
async fn create(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<EventBody>,
) -> ApiResult<Json<Value>> {
    let sync = body.sync;
    let draft = draft(body)?;
    if sync {
        pinchora_commands::check_event_sync_permission(&state.app, managed.id)
            .await
            .map_err(sync_error)?;
    }
    let row = state.app.db.create_event(managed.id, &draft).await?;
    pinchora_commands::publish_event(&state.app, &row).await?;
    if sync {
        pinchora_commands::sync_discord_event(&state.app, &row, true)
            .await
            .map_err(sync_error)?;
    }
    let posted = state
        .app
        .db
        .event(managed.id, row.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(view(&posted)))
}
async fn one(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(path): Path<EventPath>,
) -> ApiResult<Json<Value>> {
    let row = state
        .app
        .db
        .event(managed.id, path.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(view(&row)))
}
async fn update(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(path): Path<EventPath>,
    ApiJson(body): ApiJson<EventBody>,
) -> ApiResult<Json<Value>> {
    let sync = body.sync;
    let draft = draft(body)?;
    let existing = state
        .app
        .db
        .event(managed.id, path.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if sync || existing.discord_event_id.is_some() {
        pinchora_commands::check_event_sync_permission(&state.app, managed.id)
            .await
            .map_err(sync_error)?;
    }
    let row = state
        .app
        .db
        .update_event(managed.id, path.id, &draft)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if row.message_id.is_none() {
        pinchora_commands::publish_event(&state.app, &row).await?;
        if let (Some(channel), Some(message)) = (existing.channel_id, existing.message_id)
            && let Err(error) = state
                .app
                .http
                .delete_message(Id::new(channel as u64), Id::new(message as u64))
                .await
        {
            tracing::warn!(%error, id=row.id, "old event card delete failed");
        }
    } else {
        pinchora_commands::refresh_event_card(&state.app, &row).await;
    }
    pinchora_commands::sync_discord_event(&state.app, &row, sync)
        .await
        .map_err(sync_error)?;
    let posted = state
        .app
        .db
        .event(managed.id, row.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(view(&posted)))
}
async fn cancel(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(path): Path<EventPath>,
) -> ApiResult<Json<Value>> {
    let existing = state
        .app
        .db
        .event(managed.id, path.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if existing.discord_event_id.is_some() {
        pinchora_commands::sync_discord_event(&state.app, &existing, false)
            .await
            .map_err(sync_error)?;
    }
    let row = state
        .app
        .db
        .cancel_event(managed.id, path.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    pinchora_commands::refresh_event_card(&state.app, &row).await;
    Ok(Json(view(&row)))
}
async fn attendees(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(path): Path<EventPath>,
) -> ApiResult<Json<Value>> {
    state
        .app
        .db
        .event(managed.id, path.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let count = state.app.db.event_rsvp_count(path.id).await?;
    let rows = state.app.db.event_rsvps(path.id, 0, 25).await?;
    Ok(Json(
        json!({"count":count,"attendees":rows.iter().map(|r|json!({"user_id":r.user_id.to_string(),"option":r.option,"waitlisted":r.waitlisted,"responded_at":r.responded_at})).collect::<Vec<_>>()}),
    ))
}
