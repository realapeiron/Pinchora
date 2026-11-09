//! Events dashboard routes, scoped to a server the session can manage.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use pinchora_core::admin::modules::Access;
use pinchora_core::admin::modules::Module;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::community::events::Recurrence;
use pinchora_core::community::events::RsvpOption;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{EventDraft, EventRow};
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::id::Id;

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::state::WebState;

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
fn id(raw: &str, field: &'static str, lang: Lang) -> ApiResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| bad(field, t!(lang, "events.web.invalid-id")))
}
fn draft(body: EventBody, lang: Lang) -> ApiResult<EventDraft> {
    if body.channel_id.is_none() {
        return Err(bad("channel_id", t!(lang, "events.web.channel")));
    }
    let d = EventDraft {
        title: body.title,
        description: body.description,
        starts_at: body.starts_at,
        ends_at: body.ends_at,
        tz: body.tz,
        location: body.location,
        channel_id: body
            .channel_id
            .as_deref()
            .map(|s| id(s, "channel_id", lang))
            .transpose()?,
        host_id: None,
        options: body.options,
        required_role_id: body
            .required_role_id
            .as_deref()
            .map(|s| id(s, "required_role_id", lang))
            .transpose()?,
        reminders: vec![60, 15],
        recurrence: body.recurrence,
    };
    d.validate().map_err(|_| bad("event", t!(lang, "events.invalid")))?;
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
    let lang = state.app.guild_lang(managed.id).await;
    let tz = state.app.db.event_timezone(managed.id).await?;
    let events = state.app.db.events(managed.id, true).await?;
    Ok(Json(
        json!({"language":lang.code(), "strings": pinchora_core::infra::i18n::keys(lang).into_iter().filter_map(|key| key.strip_prefix("events.ui.").map(|name| (name.to_owned(), pinchora_core::t!(lang, &key)))).collect::<std::collections::BTreeMap<_,_>>(), "timezone":tz,"events":events.iter().map(view).collect::<Vec<_>>()}),
    ))
}
async fn timezone(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<TimezoneBody>,
) -> ApiResult<Json<Value>> {
    if body.timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err(bad(
            "timezone",
            t!(state.app.guild_lang(managed.id).await, "events.web.timezone"),
        ));
    }
    enabled(&state, managed.id, None).await?;
    let before = state.app.db.event_timezone(managed.id).await?;
    state.app.db.set_event_timezone(managed.id, &body.timezone).await?;
    record(&state, &managed, "event.timezone", json!(before), json!(body.timezone)).await;
    Ok(Json(json!({"timezone":body.timezone})))
}
async fn create(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<EventBody>,
) -> ApiResult<Json<Value>> {
    let sync = body.sync;
    let mut draft = draft(body, state.app.guild_lang(managed.id).await)?;
    validate_scope(&state, &managed, &draft).await?;
    draft.host_id = Some(managed.current.session.user_id as i64);
    let row = state.app.db.create_event(managed.id, &draft).await?;
    record(&state, &managed, "event.create", Value::Null, view(&row)).await;
    let mut warning = publish_warning(&state, &row).await;
    if sync {
        warning = sync_warning(&state, &row, true).await.or(warning);
    }
    let posted = state
        .app
        .db
        .event(managed.id, row.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(with_warning(&posted, warning)))
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
    let draft = draft(body, state.app.guild_lang(managed.id).await)?;
    let existing = state
        .app
        .db
        .event(managed.id, path.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    validate_scope(&state, &managed, &draft).await?;
    let row = state
        .app
        .db
        .update_event(managed.id, path.id, &draft)
        .await?
        .ok_or_else(ApiError::not_found)?;
    record(&state, &managed, "event.update", view(&existing), view(&row)).await;
    let mut warning = None;
    if row.message_id.is_none() {
        warning = publish_warning(&state, &row).await;
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
    warning = sync_warning(&state, &row, sync).await.or(warning);
    let posted = state
        .app
        .db
        .event(managed.id, row.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(with_warning(&posted, warning)))
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
    let row = state
        .app
        .db
        .cancel_event(managed.id, path.id)
        .await?
        .ok_or_else(ApiError::not_found)?;
    record(&state, &managed, "event.cancel", view(&existing), view(&row)).await;
    let warning = sync_warning(&state, &row, false).await;
    pinchora_commands::refresh_event_card(&state.app, &row).await;
    Ok(Json(with_warning(&row, warning)))
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

async fn enabled(state: &WebState, guild: u64, channel: Option<u64>) -> ApiResult<()> {
    if state.app.module_access(guild, channel, Module::Events).await? != Access::Allowed {
        return Err(ApiError::blocked(t!(
            state.app.guild_lang(guild).await,
            "events.web.disabled"
        )));
    }
    Ok(())
}
async fn validate_scope(state: &WebState, managed: &ManagedGuild, draft: &EventDraft) -> ApiResult<()> {
    let channel = draft.channel_id.unwrap_or_default() as u64;
    enabled(state, managed.id, Some(channel)).await?;
    let info = guild_info::get(state, managed.id, false).await?;
    let channel = info
        .check_channel("channel_id", channel)
        .map_err(|e| ApiError::invalid(vec![e]))?;
    if !channel.takes_posts() {
        return Err(bad(
            "channel_id",
            t!(state.app.guild_lang(managed.id).await, "events.web.channel"),
        ));
    }
    if let Some(role) = draft.required_role_id {
        info.check_role("required_role_id", role as u64)
            .map_err(|e| ApiError::invalid(vec![e]))?;
    }
    Ok(())
}
async fn record(state: &WebState, managed: &ManagedGuild, action: &str, before: Value, after: Value) {
    audit::finish(
        state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action,
            target: None,
            before,
            after,
        },
        None,
    )
    .await;
}
fn with_warning(row: &EventRow, warning: Option<String>) -> Value {
    let mut value = view(row);
    value["warning"] = json!(warning);
    value
}
async fn sync_warning(state: &WebState, row: &EventRow, sync: bool) -> Option<String> {
    if let Err(error) = pinchora_commands::sync_discord_event(&state.app, row, sync).await {
        tracing::warn!(%error, id=row.id, "event saved but Discord sync failed");
        Some(t!(
            state.app.guild_lang(row.guild_id as u64).await,
            "events.web.sync-warning"
        ))
    } else {
        None
    }
}
async fn publish_warning(state: &WebState, row: &EventRow) -> Option<String> {
    if let Err(error) = pinchora_commands::publish_event(&state.app, row).await {
        tracing::warn!(%error, id=row.id, "event saved but card publication failed");
        Some(t!(
            state.app.guild_lang(row.guild_id as u64).await,
            "events.web.post-warning"
        ))
    } else {
        None
    }
}
