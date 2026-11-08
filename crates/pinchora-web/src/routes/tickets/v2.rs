//! V2 ticket operations. The guild router maps every path here to W7's community area.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use pinchora_db::{SupportWindow, Ticket, TicketFaq, TicketOperationSettings, TicketPanelOperations};
use serde::Deserialize;
use serde_json::{Value, json};
use twilight_model::{channel::message::AllowedMentions, id::Id};

use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    state::WebState,
};

#[derive(Deserialize)]
struct RouteId {
    #[serde(rename = "g")]
    _guild: String,
    id: i64,
}

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/settings", get(settings).put(put_settings))
        .route("/panel/{id}", get(panel).put(put_panel))
        .route("/snippets", get(snippets).post(save_snippet))
        .route("/leaderboard", get(leaderboard))
        .route("/snippets/{id}", axum::routing::delete(delete_snippet))
        .route("/ticket/{id}/operations", get(operations))
        .route("/ticket/{id}/notes", get(notes).post(add_note))
        .route("/ticket/{id}/claim", post(claim))
        .route("/ticket/{id}/unclaim", post(unclaim))
        .route("/ticket/{id}/reply", post(reply))
}

fn bad(message: impl Into<String>) -> ApiError {
    ApiError::malformed(message.into())
}

async fn owned_ticket(state: &WebState, guild: u64, id: i64) -> ApiResult<Ticket> {
    state
        .app
        .db
        .ticket(id)
        .await?
        .filter(|ticket| ticket.guild_id as u64 == guild)
        .ok_or_else(ApiError::not_found)
}

async fn owned_panel(state: &WebState, guild: u64, id: i64) -> ApiResult<()> {
    state
        .app
        .db
        .ticket_panel(id)
        .await?
        .filter(|panel| panel.guild_id as u64 == guild)
        .map(|_| ())
        .ok_or_else(ApiError::not_found)
}

async fn log_action(
    state: &WebState,
    managed: &ManagedGuild,
    action: &'static str,
    target: &str,
    before: Value,
    after: Value,
) {
    audit::finish(
        state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action,
            target: Some(target),
            before,
            after,
        },
        None,
    )
    .await;
}

fn settings_json(settings: &TicketOperationSettings) -> Value {
    json!({
        "support_timezone": settings.support_timezone,
        "support_hours": settings.support_hours,
        "out_of_hours_message": settings.out_of_hours_message,
        "status_channel_id": settings.status_channel_id.map(|id| id.to_string()),
        "status_message_id": settings.status_message_id.map(|id| id.to_string()),
        "close_timeout_hours": settings.close_timeout_hours,
    })
}

async fn settings(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(settings_json(
        &state.app.db.ticket_operation_settings(managed.id).await?,
    )))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsBody {
    support_timezone: String,
    support_hours: Vec<SupportWindow>,
    out_of_hours_message: String,
    status_channel_id: Option<String>,
    close_timeout_hours: i32,
}

async fn put_settings(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<SettingsBody>,
) -> ApiResult<Json<Value>> {
    if body.support_timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err(bad("Choose a valid timezone."));
    }
    if body.support_hours.len() > 21
        || body
            .support_hours
            .iter()
            .any(|w| !(1..=7).contains(&w.day) || w.start >= w.end || w.end > 1440)
    {
        return Err(bad("Support windows need a weekday and valid start/end minutes."));
    }
    if body.out_of_hours_message.chars().count() > 1000 || !(1..=168).contains(&body.close_timeout_hours) {
        return Err(bad("Message or close timeout is out of range."));
    }
    let channel = body
        .status_channel_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u64>().map_err(|_| bad("Invalid status channel.")))
        .transpose()?;
    if let Some(channel) = channel {
        let info = super::info_for(&state, managed.id, &[channel], &[]).await?;
        info.check_post_channel("status_channel_id", channel)
            .map_err(|e| ApiError::invalid(vec![e]))?;
    }
    let before = state.app.db.ticket_operation_settings(managed.id).await?;
    let now = TicketOperationSettings {
        support_timezone: body.support_timezone,
        support_hours: body.support_hours,
        out_of_hours_message: body.out_of_hours_message,
        status_channel_id: channel.map(|id| id as i64),
        status_message_id: if before.status_channel_id == channel.map(|id| id as i64) {
            before.status_message_id
        } else {
            None
        },
        close_timeout_hours: body.close_timeout_hours,
    };
    if now != before {
        state.app.db.save_ticket_operation_settings(managed.id, &now).await?;
        if before.status_channel_id != now.status_channel_id
            && let (Some(channel), Some(message)) = (before.status_channel_id, before.status_message_id)
            && let Err(error) = state
                .app
                .http
                .delete_message(Id::new(channel as u64), Id::new(message as u64))
                .await
        {
            tracing::warn!(guild = managed.id, %error, "old ticket status board not removed");
        }
        log_action(
            &state,
            &managed,
            "tickets.v2.settings",
            "settings",
            json!(before),
            json!(now),
        )
        .await;
        if now.status_channel_id.is_some()
            && let Err(error) = pinchora_commands::refresh_ticket_status_board(&state.app, managed.id).await
        {
            tracing::warn!(guild = managed.id, %error, "ticket status board not refreshed");
        }
    }
    Ok(Json(settings_json(&now)))
}

fn panel_json(panel: &TicketPanelOperations) -> Value {
    json!({
        "claim_mode": panel.claim_mode,
        "max_open_per_panel": panel.max_open_per_panel,
        "max_open_per_category": panel.max_open_per_category,
        "cooldown_seconds": panel.cooldown_seconds,
        "minimum_account_age_days": panel.minimum_account_age_days,
        "minimum_membership_days": panel.minimum_membership_days,
        "bypass_roles": panel.bypass_roles.iter().map(i64::to_string).collect::<Vec<_>>(),
        "staff_open_limit": panel.staff_open_limit,
        "overflow_message": panel.overflow_message,
        "faq": panel.faq,
    })
}

async fn panel(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(RouteId { id, .. }): Path<RouteId>,
) -> ApiResult<Json<Value>> {
    owned_panel(&state, managed.id, id).await?;
    Ok(Json(panel_json(&state.app.db.ticket_panel_operations(id).await?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PanelBody {
    claim_mode: String,
    max_open_per_panel: i32,
    max_open_per_category: i32,
    cooldown_seconds: i32,
    minimum_account_age_days: i32,
    minimum_membership_days: i32,
    bypass_roles: Vec<String>,
    staff_open_limit: i32,
    overflow_message: String,
    faq: Vec<TicketFaq>,
}

async fn put_panel(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(RouteId { id, .. }): Path<RouteId>,
    ApiJson(body): ApiJson<PanelBody>,
) -> ApiResult<Json<Value>> {
    owned_panel(&state, managed.id, id).await?;
    let bypass_roles: Vec<i64> = body
        .bypass_roles
        .iter()
        .map(|id| id.parse::<i64>().map_err(|_| bad("Invalid bypass role.")))
        .collect::<ApiResult<_>>()?;
    let body = TicketPanelOperations {
        claim_mode: body.claim_mode,
        max_open_per_panel: body.max_open_per_panel,
        max_open_per_category: body.max_open_per_category,
        cooldown_seconds: body.cooldown_seconds,
        minimum_account_age_days: body.minimum_account_age_days,
        minimum_membership_days: body.minimum_membership_days,
        bypass_roles,
        staff_open_limit: body.staff_open_limit,
        overflow_message: body.overflow_message,
        faq: body.faq,
    };
    if !matches!(body.claim_mode.as_str(), "optional" | "required" | "exclusive")
        || !(0..=100).contains(&body.max_open_per_panel)
        || !(0..=100).contains(&body.max_open_per_category)
        || !(0..=86400).contains(&body.cooldown_seconds)
        || !(0..=3650).contains(&body.minimum_account_age_days)
        || !(0..=3650).contains(&body.minimum_membership_days)
        || !(0..=100).contains(&body.staff_open_limit)
        || body.overflow_message.chars().count() > 1000
        || body.bypass_roles.len() > 10
        || body.faq.len() > 10
        || body.faq.iter().any(|f: &TicketFaq| {
            f.question.trim().is_empty()
                || f.question.chars().count() > 150
                || f.answer.trim().is_empty()
                || f.answer.chars().count() > 1000
        })
    {
        return Err(bad("Ticket panel operation settings are out of range."));
    }
    let roles: Vec<u64> = body.bypass_roles.iter().map(|id| *id as u64).collect();
    let info = super::info_for(&state, managed.id, &[], &roles).await?;
    for role in roles {
        let found = info
            .check_role("bypass_roles", role)
            .map_err(|e| ApiError::invalid(vec![e]))?;
        if found.everyone {
            return Err(bad("@everyone cannot bypass ticket limits."));
        }
    }
    let before = state.app.db.ticket_panel_operations(id).await?;
    if before != body {
        state.app.db.save_ticket_panel_operations(id, &body).await?;
        log_action(
            &state,
            &managed,
            "tickets.v2.panel",
            &id.to_string(),
            json!(before),
            json!(body),
        )
        .await;
    }
    Ok(Json(panel_json(&body)))
}

async fn snippets(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({ "items": state.app.db.ticket_snippets(managed.id).await? }),
    ))
}

async fn leaderboard(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let scores = state.app.db.ticket_staff_leaderboard(managed.id).await?;
    let names = super::names(&state, scores.iter().map(|s| s.user_id as u64)).await;
    Ok(Json(json!({ "items": scores.iter().map(|s| json!({
        "user_id": s.user_id.to_string(), "name": names.get(&(s.user_id as u64)),
        "claimed": s.claimed, "closed": s.closed,
    })).collect::<Vec<_>>() })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnippetBody {
    name: String,
    body: String,
}

async fn save_snippet(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<SnippetBody>,
) -> ApiResult<Json<Value>> {
    let name = body.name.trim();
    let text = body.body.trim();
    if name.is_empty() || name.chars().count() > 50 || text.is_empty() || text.chars().count() > 2000 {
        return Err(bad("Saved reply needs a name (up to 50) and text (up to 2000)."));
    }
    let saved = state.app.db.save_ticket_snippet(managed.id, name, text).await?;
    log_action(
        &state,
        &managed,
        "tickets.v2.snippet.save",
        &saved.id.to_string(),
        Value::Null,
        json!({ "name": saved.name }),
    )
    .await;
    Ok(Json(json!(saved)))
}

async fn delete_snippet(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(RouteId { id, .. }): Path<RouteId>,
) -> ApiResult<StatusCode> {
    if !state.app.db.delete_ticket_snippet(managed.id, id).await? {
        return Err(ApiError::not_found());
    }
    log_action(
        &state,
        &managed,
        "tickets.v2.snippet.delete",
        &id.to_string(),
        Value::Null,
        Value::Null,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn operations(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(RouteId { id, .. }): Path<RouteId>,
) -> ApiResult<Json<Value>> {
    owned_ticket(&state, managed.id, id).await?;
    let ops = state.app.db.ticket_operations(id).await?;
    Ok(Json(json!({
        "priority": ops.priority,
        "escalated_at": ops.escalated_at,
        "transferred_to_user": ops.transferred_to_user.map(|id| id.to_string()),
        "transferred_to_role": ops.transferred_to_role.map(|id| id.to_string()),
        "close_requested_by": ops.close_requested_by.map(|id| id.to_string()),
        "close_requested_at": ops.close_requested_at,
        "close_request_expires_at": ops.close_request_expires_at,
        "force_opened_by": ops.force_opened_by.map(|id| id.to_string()),
        "last_reminder_at": ops.last_reminder_at,
    })))
}

async fn notes(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(RouteId { id, .. }): Path<RouteId>,
) -> ApiResult<Json<Value>> {
    owned_ticket(&state, managed.id, id).await?;
    let notes = state.app.db.ticket_staff_notes(id).await?;
    Ok(Json(json!({ "items": notes.into_iter().map(|note| json!({
        "id": note.id.to_string(), "author_id": note.author_id.to_string(),
        "body": note.body, "created_at": note.created_at,
    })).collect::<Vec<_>>() })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextBody {
    text: String,
}

async fn add_note(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(RouteId { id, .. }): Path<RouteId>,
    ApiJson(body): ApiJson<TextBody>,
) -> ApiResult<Json<Value>> {
    owned_ticket(&state, managed.id, id).await?;
    let text = body.text.trim();
    if text.is_empty() || text.chars().count() > 2000 {
        return Err(bad("Note must be 1–2000 characters."));
    }
    let note = state
        .app
        .db
        .add_ticket_staff_note(id, managed.current.session.user_id, text)
        .await?
        .ok_or_else(ApiError::not_found)?;
    log_action(
        &state,
        &managed,
        "tickets.v2.note",
        &id.to_string(),
        Value::Null,
        json!({ "note_id": note.id }),
    )
    .await;
    Ok(Json(
        json!({ "id": note.id.to_string(), "author_id": note.author_id.to_string(),
        "body": note.body, "created_at": note.created_at }),
    ))
}

async fn claim(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(RouteId { id, .. }): Path<RouteId>,
) -> ApiResult<Json<Value>> {
    owned_ticket(&state, managed.id, id).await?;
    let actor = managed.current.session.user_id;
    if !state.app.db.claim_ticket(id, actor).await? {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "ticket_claimed",
            "Ticket is closed, claimed or at the staff limit.",
        ));
    }
    log_action(
        &state,
        &managed,
        "tickets.v2.claim",
        &id.to_string(),
        Value::Null,
        json!({ "claimed_by": actor.to_string() }),
    )
    .await;
    Ok(Json(json!({ "claimed_by": actor.to_string() })))
}

async fn unclaim(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(RouteId { id, .. }): Path<RouteId>,
) -> ApiResult<Json<Value>> {
    owned_ticket(&state, managed.id, id).await?;
    let actor = managed.current.session.user_id;
    if !state.app.db.unclaim_ticket_by(id, actor).await? {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "ticket_unclaim",
            "Ticket is closed, unclaimed or owned by another staff member.",
        ));
    }
    log_action(
        &state,
        &managed,
        "tickets.v2.unclaim",
        &id.to_string(),
        Value::Null,
        Value::Null,
    )
    .await;
    Ok(Json(json!({ "claimed_by": null })))
}

async fn reply(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(RouteId { id, .. }): Path<RouteId>,
    ApiJson(body): ApiJson<TextBody>,
) -> ApiResult<Json<Value>> {
    let ticket = owned_ticket(&state, managed.id, id).await?;
    if !ticket.is_open() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "ticket_closed",
            "Ticket is closed.",
        ));
    }
    let text = body.text.trim();
    if text.is_empty() || text.chars().count() > 2000 {
        return Err(bad("Reply must be 1–2000 characters."));
    }
    let mode = match ticket.panel_id {
        Some(panel) => state.app.db.ticket_panel_operations(panel).await?.claim_mode,
        None => "optional".into(),
    };
    let actor = managed.current.session.user_id as i64;
    if mode != "optional" && ticket.claimed_by.is_none() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "claim_required",
            "Claim the ticket before replying.",
        ));
    }
    if mode == "exclusive" && ticket.claimed_by != Some(actor) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "exclusive_claim",
            "Only the claimant may reply.",
        ));
    }
    let place = ticket.place().ok_or_else(ApiError::not_found)?;
    let mentions = AllowedMentions::default();
    state
        .app
        .http
        .create_message(Id::new(place))
        .content(text)
        .allowed_mentions(Some(&mentions))
        .await
        .map_err(anyhow::Error::from)?;
    state.app.db.touch_ticket(id, state.app.now()).await?;
    log_action(
        &state,
        &managed,
        "tickets.v2.reply",
        &id.to_string(),
        Value::Null,
        json!({ "characters": text.chars().count(), "as_bot": true }),
    )
    .await;
    Ok(Json(json!({ "sent": true })))
}
