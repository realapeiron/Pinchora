//! Sticky and scheduled message editor for server managers.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use pinchora_core::admin::afk_sticky_schedule::Schedule;
use pinchora_core::admin::afk_sticky_schedule::clean;
use pinchora_core::admin::afk_sticky_schedule::preview_runs;
use pinchora_core::admin::afk_sticky_schedule::timezone;
use pinchora_core::admin::afk_sticky_schedule::validate;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::community::tags::EmbedSpec;
use pinchora_core::community::tags::clean_embed;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{ScheduledRow, UtilityStickyRow};
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::guild::Permissions;

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

fn invalid(lang: Lang, field: &'static str, key: &str) -> ApiError {
    ApiError::invalid(vec![FieldError::new(field, t!(lang, key))])
}
fn id(lang: Lang, text: &str, field: &'static str) -> ApiResult<u64> {
    text.parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && *n <= i64::MAX as u64)
        .ok_or_else(|| invalid(lang, field, "utility.id-invalid"))
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
    let lang = state.app.guild_lang(guild).await;
    let channel = id(lang, raw, "channel_id")?;
    guild_info::get(state, guild, false)
        .await?
        .check_post_channel("channel_id", channel)?;
    let perms = pinchora_commands::utility_bot_permissions(&state.app, guild, channel)
        .await
        .map_err(|_| invalid(lang, "channel_id", "utility.permissions-unreadable"))?;
    if !perms.contains(Permissions::SEND_MESSAGES) {
        return Err(invalid(lang, "channel_id", "utility.send-needed"));
    }
    if manage && !perms.contains(Permissions::MANAGE_MESSAGES) {
        return Err(invalid(lang, "channel_id", "utility.manage-messages-needed"));
    }
    Ok(channel)
}

fn sticky_json(s: &UtilityStickyRow) -> Value {
    json!({"channel_id":s.channel_id.to_string(),"content":s.content,"embed":s.embed,"min_gap_secs":s.min_gap_secs,"min_messages":s.min_messages,"message_count":s.message_count,"enabled":s.enabled,"pause_reason":s.pause_reason})
}
fn scheduled_json(s: &ScheduledRow) -> Value {
    json!({"id":s.id.to_string(),"channel_id":s.channel_id.to_string(),"content":s.content,"embed":s.embed,"schedule":s.schedule,"tz":s.tz,"next_run_at":s.next_run_at,"last_run_at":s.last_run_at,"runs":s.runs,"enabled":s.enabled,"ping_allowed":s.ping_allowed,"pause_reason":s.pause_reason})
}
async fn record(
    state: &WebState,
    managed: &ManagedGuild,
    action: &str,
    target: Option<String>,
    before: Value,
    after: Value,
) {
    audit::finish(
        state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action,
            target: target.as_deref(),
            before,
            after,
        },
        None,
    )
    .await;
}

async fn list(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let sticky = state.app.db.stickies(managed.id).await?;
    let scheduled = state.app.db.schedules(managed.id).await?;
    let timezone = state.app.db.utility_timezone(managed.id).await?;
    let lang = state.app.guild_lang(managed.id).await;
    let localize = |mut value: Value| {
        if let Some(reason) = value["pause_reason"].as_str() {
            value["pause_reason_text"] = json!(pinchora_core::admin::afk_sticky_schedule::pause_text(lang, reason));
        }
        value
    };
    Ok(Json(
        json!({"language":lang.code(), "strings": pinchora_core::infra::i18n::keys(lang).into_iter().filter_map(|key| key.strip_prefix("utility.ui.").map(|name| (name.to_owned(), pinchora_core::t!(lang, &key)))).collect::<std::collections::BTreeMap<_,_>>(), "sticky":sticky.iter().map(sticky_json).map(localize).collect::<Vec<_>>(),"scheduled":scheduled.iter().map(scheduled_json).map(localize).collect::<Vec<_>>(),"max_scheduled":20,"timezone":timezone,"timezone_notice":pinchora_core::t!(lang,"utility.timezone-notice")}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimezoneBody {
    timezone: String,
}

async fn set_timezone(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<TimezoneBody>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    if timezone(&body.timezone).is_none() {
        return Err(invalid(lang, "timezone", "utility.timezone-invalid"));
    }
    let before = state.app.db.utility_timezone(managed.id).await?;
    state.app.db.set_utility_timezone(managed.id, &body.timezone).await?;
    record(
        &state,
        &managed,
        "utility.timezone",
        None,
        json!({"timezone":before}),
        json!({"timezone":body.timezone}),
    )
    .await;
    Ok(Json(json!({"timezone":body.timezone})))
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
    let lang = state.app.guild_lang(managed.id).await;
    let channel = channel(&state, managed.id, &body.channel_id, true).await?;
    let content = clean(&body.content, 1000);
    let cleaned_embed = embed(body.embed)?;
    if content.is_empty() && cleaned_embed.is_none() {
        return Err(invalid(lang, "content", "utility.content-empty"));
    }
    if body.content.chars().count() > 1000
        || !(5..=86400).contains(&body.min_gap_secs)
        || !(1..=1000).contains(&body.min_messages)
    {
        return Err(invalid(lang, "content", "utility.sticky-limits"));
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
        lease_until: None,
        pause_reason: None,
        updated_by: Some(managed.current.session.user_id as i64),
    };
    let before = state
        .app
        .db
        .stickies(managed.id)
        .await?
        .iter()
        .find(|r| r.channel_id == row.channel_id)
        .map(sticky_json)
        .unwrap_or(Value::Null);
    state.app.db.upsert_sticky(&row).await?;
    record(
        &state,
        &managed,
        "utility.sticky.save",
        Some(channel.to_string()),
        before,
        sticky_json(&row),
    )
    .await;
    Ok(Json(sticky_json(&row)))
}
async fn delete_sticky(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(StickyPath { channel: raw }): Path<StickyPath>,
) -> ApiResult<Json<Value>> {
    let channel = id(state.app.guild_lang(managed.id).await, &raw, "channel_id")?;
    let Some(row) = state.app.db.remove_sticky(managed.id, channel).await? else {
        return Err(ApiError::not_found());
    };
    record(
        &state,
        &managed,
        "utility.sticky.remove",
        Some(channel.to_string()),
        sticky_json(&row),
        Value::Null,
    )
    .await;
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
    let lang = state.app.guild_lang(managed.id).await;
    let channel = channel(state, managed.id, &body.channel_id, false).await?;
    if !validate(&body.schedule) || timezone(&body.tz).is_none() || body.next_run_at <= Utc::now() {
        return Err(invalid(lang, "schedule", "utility.schedule-invalid"));
    }
    if body.content.chars().count() > 2000 {
        return Err(invalid(lang, "content", "utility.content-limit"));
    }
    let content = clean(&body.content, 2000);
    let cleaned_embed = embed(body.embed)?;
    if content.is_empty() && cleaned_embed.is_none() {
        return Err(invalid(lang, "content", "utility.content-empty"));
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
        return Err(invalid(lang, "content", "utility.ping-permissions"));
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
        lease_until: None,
        pause_reason: None,
        created_by: Some(managed.current.session.user_id as i64),
        ping_allowed: broad && actor_may_ping && bot_may_ping,
    })
}

async fn add_schedule(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<ScheduleBody>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let row = checked_schedule(&state, &managed, body).await?;
    let Some(id) = state.app.db.add_schedule(&row).await? else {
        return Err(ApiError::limit(t!(lang, "utility.full")));
    };
    record(
        &state,
        &managed,
        "utility.schedule.add",
        Some(id.to_string()),
        Value::Null,
        scheduled_json(&row),
    )
    .await;
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
    let before = state
        .app
        .db
        .schedule(managed.id, id)
        .await?
        .as_ref()
        .map(scheduled_json)
        .unwrap_or(Value::Null);
    if !state.app.db.update_schedule(&row).await? {
        return Err(ApiError::not_found());
    }
    record(
        &state,
        &managed,
        "utility.schedule.edit",
        Some(id.to_string()),
        before,
        scheduled_json(&row),
    )
    .await;
    Ok(Json(json!({"id":id.to_string()})))
}
async fn delete_schedule(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(ScheduledPath { id }): Path<ScheduledPath>,
) -> ApiResult<Json<Value>> {
    let before = state
        .app
        .db
        .schedule(managed.id, id)
        .await?
        .as_ref()
        .map(scheduled_json)
        .unwrap_or(Value::Null);
    if !state.app.db.remove_schedule(managed.id, id).await? {
        return Err(ApiError::not_found());
    }
    record(
        &state,
        &managed,
        "utility.schedule.remove",
        Some(id.to_string()),
        before,
        Value::Null,
    )
    .await;
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
    let before = state
        .app
        .db
        .schedule(managed.id, id)
        .await?
        .as_ref()
        .map(scheduled_json)
        .unwrap_or(Value::Null);
    if !state.app.db.enable_schedule(managed.id, id, body.enabled).await? {
        return Err(ApiError::not_found());
    }
    record(
        &state,
        &managed,
        "utility.schedule.toggle",
        Some(id.to_string()),
        before,
        json!({"enabled":body.enabled}),
    )
    .await;
    Ok(Json(json!({"enabled":body.enabled})))
}
async fn preview(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(ScheduledPath { id }): Path<ScheduledPath>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let Some(row) = state.app.db.schedule(managed.id, id).await? else {
        return Err(ApiError::not_found());
    };
    let rule: Schedule =
        serde_json::from_value(row.schedule).map_err(|_| invalid(lang, "schedule", "utility.recurrence-invalid"))?;
    let runs = preview_runs(&rule, &row.tz, row.next_run_at, Utc::now());
    Ok(Json(json!({"runs":runs})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewBody {
    schedule: Schedule,
    tz: String,
    next_run_at: DateTime<Utc>,
}

async fn preview_draft(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<PreviewBody>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    if !validate(&body.schedule) || timezone(&body.tz).is_none() {
        return Err(invalid(lang, "schedule", "utility.preview-invalid"));
    }
    let runs = preview_runs(&body.schedule, &body.tz, body.next_run_at, Utc::now());
    Ok(Json(json!({"runs":runs})))
}
