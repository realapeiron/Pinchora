//! Sticky and scheduled message editor for server managers.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use pinchora_core::{
    settings::FieldError,
    tags::{EmbedSpec, clean_embed},
    utility::{Schedule, clean, next_run, offset, validate},
};
use pinchora_db::{ScheduledRow, UtilityStickyRow};
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::guild::Permissions;

use crate::{
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info,
    state::WebState,
};

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(list))
        .route("/sticky", post(save_sticky))
        .route("/sticky/{channel}", axum::routing::delete(delete_sticky))
        .route("/scheduled", post(add_schedule))
        .route("/scheduled/preview", post(preview_draft))
        .route(
            "/scheduled/{id}",
            axum::routing::delete(delete_schedule)
                .patch(patch_schedule)
                .put(put_schedule),
        )
        .route("/scheduled/{id}/preview", get(preview))
        .route("/timezone", axum::routing::put(set_timezone))
}

fn invalid(field: &'static str, message: &str) -> ApiError {
    ApiError::invalid(vec![FieldError::new(field, message)])
}
fn id(text: &str, field: &'static str) -> ApiResult<u64> {
    text.parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= i64::MAX as u64)
        .ok_or_else(|| invalid(field, "Choose a valid Discord ID."))
}
fn embed(value: Option<EmbedSpec>) -> ApiResult<Option<Value>> {
    Ok(clean_embed(value)
        .map_err(ApiError::invalid)?
        .map(|e| serde_json::to_value(e).expect("embed serializes")))
}
#[derive(Deserialize)]
struct StickyPath {
    channel: String,
}
#[derive(Deserialize)]
struct ScheduledPath {
    id: i64,
}
async fn channel(state: &WebState, guild: u64, raw: &str, manage: bool) -> ApiResult<u64> {
    let channel = id(raw, "channel_id")?;
    guild_info::get(state, guild, false)
        .await?
        .check_post_channel("channel_id", channel)?;
    let perms = pinchora_commands::utility_bot_permissions(&state.app, guild, channel)
        .await
        .map_err(|_| invalid("channel_id", "Pinchora cannot read this channel's permissions."))?;
    if !perms.contains(Permissions::SEND_MESSAGES) {
        return Err(invalid("channel_id", "Pinchora needs Send Messages in this channel."));
    }
    if manage && !perms.contains(Permissions::MANAGE_MESSAGES) {
        return Err(invalid("channel_id", "Pinchora needs Manage Messages in this channel."));
    }
    Ok(channel)
}

fn sticky_json(s: &UtilityStickyRow) -> Value {
    json!({"channel_id":s.channel_id.to_string(),"content":s.content,"embed":s.embed,"min_gap_secs":s.min_gap_secs,"min_messages":s.min_messages,"message_count":s.message_count,"enabled":s.enabled})
}
fn scheduled_json(s: &ScheduledRow) -> Value {
    json!({"id":s.id.to_string(),"channel_id":s.channel_id.to_string(),"content":s.content,"embed":s.embed,"schedule":s.schedule,"tz":s.tz,"next_run_at":s.next_run_at,"last_run_at":s.last_run_at,"runs":s.runs,"enabled":s.enabled,"ping_allowed":s.ping_allowed})
}
async fn list(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let sticky = state.app.db.stickies(managed.id).await?;
    let scheduled = state.app.db.schedules(managed.id).await?;
    let utc_offset = state.app.db.utility_offset(managed.id).await?;
    Ok(Json(
        json!({"sticky":sticky.iter().map(sticky_json).collect::<Vec<_>>(),"scheduled":scheduled.iter().map(scheduled_json).collect::<Vec<_>>(),"max_scheduled":20,"utc_offset":utc_offset}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimezoneBody {
    utc_offset: String,
}

async fn set_timezone(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<TimezoneBody>,
) -> ApiResult<Json<Value>> {
    if offset(&body.utc_offset).is_none() {
        return Err(invalid("utc_offset", "Use a UTC offset such as +03:00."));
    }
    state.app.db.set_utility_offset(managed.id, &body.utc_offset).await?;
    Ok(Json(json!({"utc_offset":body.utc_offset})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StickyBody {
    channel_id: String,
    content: String,
    embed: Option<EmbedSpec>,
    min_gap_secs: i32,
    min_messages: i32,
    enabled: bool,
}
async fn save_sticky(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<StickyBody>,
) -> ApiResult<Json<Value>> {
    let channel = channel(&state, managed.id, &body.channel_id, true).await?;
    let content = clean(&body.content, 1000);
    let cleaned_embed = embed(body.embed)?;
    if content.is_empty() && cleaned_embed.is_none() {
        return Err(invalid("content", "Write a message or add an embed."));
    }
    if body.content.chars().count() > 1000
        || !(5..=86400).contains(&body.min_gap_secs)
        || !(1..=1000).contains(&body.min_messages)
    {
        return Err(invalid(
            "content",
            "Use up to 1000 characters, 1–1000 messages and a gap of 5–86400 seconds.",
        ));
    }
    let row = UtilityStickyRow {
        channel_id: channel as i64,
        guild_id: managed.id as i64,
        content,
        embed: cleaned_embed,
        last_message_id: None,
        min_gap_secs: body.min_gap_secs,
        min_messages: body.min_messages,
        message_count: 0,
        enabled: body.enabled,
        updated_by: Some(managed.current.session.user_id as i64),
    };
    state.app.db.upsert_sticky(&row).await?;
    Ok(Json(sticky_json(&row)))
}
async fn delete_sticky(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(StickyPath { channel: raw }): Path<StickyPath>,
) -> ApiResult<Json<Value>> {
    let channel = id(&raw, "channel_id")?;
    let Some(row) = state.app.db.remove_sticky(managed.id, channel).await? else {
        return Err(ApiError::not_found());
    };
    if let Some(copy) = row.last_message_id {
        state.app.db.queue_sticky_cleanup(channel, copy as u64).await?;
    }
    Ok(Json(json!({"removed":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleBody {
    channel_id: String,
    content: String,
    embed: Option<EmbedSpec>,
    schedule: Schedule,
    tz: String,
    next_run_at: DateTime<Utc>,
}
async fn checked_schedule(state: &WebState, managed: &ManagedGuild, body: ScheduleBody) -> ApiResult<ScheduledRow> {
    let channel = channel(state, managed.id, &body.channel_id, false).await?;
    if !validate(&body.schedule) || offset(&body.tz).is_none() || body.next_run_at <= Utc::now() {
        return Err(invalid(
            "schedule",
            "Choose a future time, valid recurrence and UTC offset.",
        ));
    }
    if body.content.chars().count() > 2000 {
        return Err(invalid("content", "Use up to 2000 characters."));
    }
    let content = clean(&body.content, 2000);
    let cleaned_embed = embed(body.embed)?;
    if content.is_empty() && cleaned_embed.is_none() {
        return Err(invalid("content", "Write a message or add an embed."));
    }
    let broad = content.contains("@everyone") || content.contains("@here") || content.contains("<@&");
    let bot_may_ping = !broad
        || pinchora_commands::utility_bot_permissions(&state.app, managed.id, channel)
            .await
            .is_ok_and(|perms| perms.contains(Permissions::MENTION_EVERYONE));
    let actor_may_ping = !broad
        || pinchora_commands::utility_creator_may_ping(
            &state.app,
            managed.id,
            channel,
            managed.current.session.user_id,
        )
        .await;
    if broad && (!actor_may_ping || !bot_may_ping) {
        return Err(invalid(
            "content",
            "You and Pinchora need Mention Everyone to save broad pings.",
        ));
    }
    Ok(ScheduledRow {
        id: 0,
        guild_id: managed.id as i64,
        channel_id: channel as i64,
        content,
        embed: cleaned_embed,
        schedule: json!(body.schedule),
        tz: body.tz,
        next_run_at: body.next_run_at,
        last_run_at: None,
        runs: 0,
        enabled: true,
        created_by: Some(managed.current.session.user_id as i64),
        ping_allowed: broad && actor_may_ping && bot_may_ping,
    })
}

async fn add_schedule(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<ScheduleBody>,
) -> ApiResult<Json<Value>> {
    let row = checked_schedule(&state, &managed, body).await?;
    let Some(id) = state.app.db.add_schedule(&row).await? else {
        return Err(ApiError::limit("This server already has 20/20 scheduled messages."));
    };
    Ok(Json(json!({"id":id.to_string()})))
}

async fn put_schedule(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(ScheduledPath { id }): Path<ScheduledPath>,
    ApiJson(body): ApiJson<ScheduleBody>,
) -> ApiResult<Json<Value>> {
    let mut row = checked_schedule(&state, &managed, body).await?;
    row.id = id;
    if !state.app.db.update_schedule(&row).await? {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"id":id.to_string()})))
}
async fn delete_schedule(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(ScheduledPath { id }): Path<ScheduledPath>,
) -> ApiResult<Json<Value>> {
    if !state.app.db.remove_schedule(managed.id, id).await? {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"removed":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SchedulePatch {
    enabled: bool,
}
async fn patch_schedule(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(ScheduledPath { id }): Path<ScheduledPath>,
    ApiJson(body): ApiJson<SchedulePatch>,
) -> ApiResult<Json<Value>> {
    if !state.app.db.enable_schedule(managed.id, id, body.enabled).await? {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"enabled":body.enabled})))
}
async fn preview(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(ScheduledPath { id }): Path<ScheduledPath>,
) -> ApiResult<Json<Value>> {
    let Some(row) = state.app.db.schedule(managed.id, id).await? else {
        return Err(ApiError::not_found());
    };
    let rule: Schedule =
        serde_json::from_value(row.schedule).map_err(|_| invalid("schedule", "Invalid recurrence."))?;
    let mut at = row.next_run_at;
    let mut runs = vec![at];
    for _ in 0..4 {
        let Some(next) = next_run(&rule, &row.tz, at) else {
            break;
        };
        at = next;
        runs.push(at);
    }
    Ok(Json(json!({"runs":runs})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewBody {
    schedule: Schedule,
    tz: String,
    next_run_at: DateTime<Utc>,
}

async fn preview_draft(_managed: ManagedGuild, ApiJson(body): ApiJson<PreviewBody>) -> ApiResult<Json<Value>> {
    if !validate(&body.schedule) || offset(&body.tz).is_none() {
        return Err(invalid("schedule", "Choose a valid recurrence and UTC offset."));
    }
    let mut at = body.next_run_at;
    let mut runs = vec![at];
    for _ in 0..4 {
        let Some(next) = next_run(&body.schedule, &body.tz, at) else {
            break;
        };
        at = next;
        runs.push(at);
    }
    Ok(Json(json!({"runs":runs})))
}
