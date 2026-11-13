//! Tickets: the server's ticket settings and panels (the same rules as `/tickets`), posting a panel, the
//! tickets themselves with their transcripts, and a week's and a month's numbers.

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use chrono::{Duration, Utc};
use futures_util::future::join_all;
use pinchora_commands::{TicketPanelPost, close_ticket_from_web, post_ticket_panel};
use pinchora_core::admin::settings::FieldError;
use pinchora_core::admin::settings::parse_hex_color;
use pinchora_core::community::tickets;
use pinchora_core::community::tickets::ANSWER_MAX;
use pinchora_core::community::tickets::AUTO_CLOSE_MAX;
use pinchora_core::community::tickets::DESCRIPTION_MAX;
use pinchora_core::community::tickets::LABEL_MAX;
use pinchora_core::community::tickets::MAX_BUTTONS;
use pinchora_core::community::tickets::MAX_OPTIONS;
use pinchora_core::community::tickets::MAX_PANELS;
use pinchora_core::community::tickets::MAX_QUESTIONS;
use pinchora_core::community::tickets::MAX_SUPPORT_ROLES;
use pinchora_core::community::tickets::Mode;
use pinchora_core::community::tickets::NAMING_MAX;
use pinchora_core::community::tickets::OPEN_LIMIT_MAX;
use pinchora_core::community::tickets::OPTION_DESCRIPTION_MAX;
use pinchora_core::community::tickets::PANEL_NAME_MAX;
use pinchora_core::community::tickets::PLACEHOLDER_MAX;
use pinchora_core::community::tickets::PanelDraft;
use pinchora_core::community::tickets::PanelStyle;
use pinchora_core::community::tickets::QUESTION_MAX;
use pinchora_core::community::tickets::Question;
use pinchora_core::community::tickets::TITLE_MAX;
use pinchora_core::community::tickets::Topic;
use pinchora_core::community::tickets::TranscriptHead;
use pinchora_core::community::tickets::WELCOME_MAX;
use pinchora_core::infra::i18n::Lang;
use pinchora_db::{
    PanelWrite, StoredTranscript, Ticket, TicketFilter, TicketPanel, TicketPanelSaved, TicketSettings, TicketSummary,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::guild_info::GuildInfo;
use crate::routes::admin::settings::parse_id;
use crate::state::WebState;

mod operations;

/// Tickets one page of the list holds.
const PAGE: i64 = 50;
/// Category channels' type number.
const CATEGORY: u8 = 4;

pub fn router() -> Router<WebState> {
    Router::new()
        .nest("/operations", crate::routes::community::tickets::operations::router())
        .route("/", get(overview))
        .route("/settings", put(put_settings))
        .route("/panels", post(create_panel))
        .route("/panels/{id}", put(replace_panel).delete(delete_panel))
        .route("/panels/{id}/post", post(post_panel))
        .route("/list", get(list))
        .route("/ticket/{id}", get(ticket))
        .route("/ticket/{id}/close", post(close_ticket))
        .route("/ticket/{id}/transcript.html", get(transcript_page))
}

fn id_string(id: Option<i64>) -> Value {
    id.map_or(Value::Null, |id| Value::String(id.to_string()))
}

fn settings_json(s: &TicketSettings) -> Value {
    json!({
        "open_category_id": id_string(s.open_category_id),
        "closed_category_id": id_string(s.closed_category_id),
        "log_channel_id": id_string(s.log_channel_id),
        "auto_close_hours": s.auto_close_hours,
        "dm_transcript": s.dm_transcript,
        "rating_enabled": s.rating_enabled,
        "naming": s.naming,
        "mode": s.mode().key(),
        "max_open_per_user": s.max_open_per_user,
    })
}

fn panel_json(p: &TicketPanel) -> Value {
    json!({
        "id": p.id.to_string(),
        "name": p.name,
        "channel_id": p.channel_id.to_string(),
        "message_id": id_string(p.message_id),
        "title": p.title,
        "description": p.description,
        "color": format!("#{:06x}", p.color as u32 & 0xff_ffff),
        "style": p.style.key(),
        "topics": p.topics,
        "questions": p.questions,
        "support_roles": p.support_roles.iter().map(i64::to_string).collect::<Vec<_>>(),
        "welcome_text": p.welcome_text,
        "open_category_id": id_string(p.open_category_id),
        "log_channel_id": id_string(p.log_channel_id),
        "enabled": p.enabled,
        "updated_at": p.updated_at,
    })
}

fn summary_json(s: &TicketSummary) -> Value {
    json!({
        "opened": s.opened,
        "closed": s.closed,
        "open_now": s.open_now,
        "median_first_reply_secs": s.median_first_reply_secs,
        "ratings": s.ratings,
        "rating_avg": s.rating_avg,
    })
}

fn limits() -> Value {
    json!({
        "panels": MAX_PANELS,
        "buttons": MAX_BUTTONS,
        "options": MAX_OPTIONS,
        "questions": MAX_QUESTIONS,
        "support_roles": MAX_SUPPORT_ROLES,
        "name": PANEL_NAME_MAX,
        "title": TITLE_MAX,
        "description": DESCRIPTION_MAX,
        "label": LABEL_MAX,
        "option_description": OPTION_DESCRIPTION_MAX,
        "welcome": WELCOME_MAX,
        "question": QUESTION_MAX,
        "placeholder": PLACEHOLDER_MAX,
        "answer": ANSWER_MAX,
        "naming": NAMING_MAX,
        "auto_close_hours": AUTO_CLOSE_MAX,
        "open_per_user": OPEN_LIMIT_MAX,
    })
}

/// `GET /tickets`: settings, panels, limits, and the last week's and month's numbers.
pub async fn overview(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let db = &state.app.db;
    let guild = managed.id;
    let now = Utc::now();
    let (settings, panels, week, month) = (
        db.ticket_settings(guild).await?,
        db.ticket_panels(guild).await?,
        db.ticket_summary(guild, now - Duration::days(7)).await?,
        db.ticket_summary(guild, now - Duration::days(30)).await?,
    );
    Ok(Json(json!({
        "settings": settings_json(&settings),
        "panels": panels.iter().map(panel_json).collect::<Vec<_>>(),
        "limits": limits(),
        "message_content": state.app.message_content,
        "summary": { "week": summary_json(&week), "month": summary_json(&month) },
    })))
}

fn invalid(field: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::invalid(vec![FieldError::new(field, message)])
}

/// An id field of a body: absent or `null` is none, anything else must be a number.
fn optional_id(field: &'static str, raw: &Option<String>) -> ApiResult<Option<u64>> {
    match raw.as_deref() {
        None | Some("") => Ok(None),
        Some(raw) => parse_id(raw)
            .map(Some)
            .ok_or_else(|| ApiError::malformed(format!("`{field}` is not an id."))),
    }
}

fn check_category(info: &GuildInfo, field: &'static str, id: Option<u64>) -> Result<(), FieldError> {
    let Some(id) = id else { return Ok(()) };
    match info.channel(id) {
        Some(c) if c.kind == CATEGORY => Ok(()),
        Some(c) => Err(FieldError::new(field, format!("{} isn't a category.", c.name))),
        None => Err(FieldError::new(
            field,
            "That category no longer exists. Refresh the list.",
        )),
    }
}

/// The server's lists, fresh enough to know the channels and roles a body names.
async fn info_for(state: &WebState, guild: u64, channels: &[u64], roles: &[u64]) -> ApiResult<GuildInfo> {
    guild_info::get_with(state, guild, |info| {
        channels.iter().all(|c| info.channel(*c).is_some()) && roles.iter().all(|r| info.role(*r).is_some())
    })
    .await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsBody {
    open_category_id: Option<String>,
    closed_category_id: Option<String>,
    log_channel_id: Option<String>,
    auto_close_hours: i64,
    dm_transcript: bool,
    rating_enabled: bool,
    naming: String,
    mode: String,
    max_open_per_user: i64,
}

/// `PUT /tickets/settings`: every setting at once, as `/tickets settings` checks them.
pub async fn put_settings(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<SettingsBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let lang = Lang::En;
    let open = optional_id("open_category_id", &body.open_category_id)?;
    let closed = optional_id("closed_category_id", &body.closed_category_id)?;
    let log = optional_id("log_channel_id", &body.log_channel_id)?;
    let named: Vec<u64> = [open, closed, log].into_iter().flatten().collect();
    let info = info_for(&state, guild, &named, &[]).await?;
    let mut errors = Vec::new();
    errors.extend(check_category(&info, "open_category_id", open).err());
    errors.extend(check_category(&info, "closed_category_id", closed).err());
    if let Some(log) = log {
        errors.extend(info.check_post_channel("log_channel_id", log).err());
    }
    let hours = tickets::check_auto_close(body.auto_close_hours, lang)
        .map_err(|e| errors.push(e))
        .ok();
    let naming = tickets::check_naming(&body.naming, lang)
        .map_err(|e| errors.push(e))
        .ok();
    let max = tickets::check_open_limit(body.max_open_per_user, lang)
        .map_err(|e| errors.push(e))
        .ok();
    let mode = Mode::parse(&body.mode);
    if mode.is_none() {
        errors.push(FieldError::new(
            "mode",
            "Tickets are channels, private threads or automatic.",
        ));
    }
    if !errors.is_empty() {
        return Err(ApiError::invalid(errors));
    }
    let db = &state.app.db;
    let was = db.ticket_settings(guild).await?;
    let now = TicketSettings {
        open_category_id: open.map(|id| id as i64),
        closed_category_id: closed.map(|id| id as i64),
        log_channel_id: log.map(|id| id as i64),
        auto_close_hours: hours.unwrap_or_default(),
        dm_transcript: body.dm_transcript,
        rating_enabled: body.rating_enabled,
        naming: naming.unwrap_or_default(),
        mode: mode.unwrap_or_default().key().to_owned(),
        max_open_per_user: max.unwrap_or(1),
    };
    if now != was {
        db.save_ticket_settings(guild, &now).await?;
        let (before, after) = changed(&settings_json(&was), &settings_json(&now));
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "tickets.settings",
                target: None,
                before,
                after,
            },
            None,
        )
        .await;
    }
    Ok(Json(settings_json(&now)))
}

/// Only the keys whose values differ, before and after.
fn changed(before: &Value, after: &Value) -> (Value, Value) {
    let (mut old, mut new) = (serde_json::Map::new(), serde_json::Map::new());
    if let (Some(b), Some(a)) = (before.as_object(), after.as_object()) {
        for (key, value) in a {
            if b.get(key) != Some(value) {
                old.insert(key.clone(), b.get(key).cloned().unwrap_or(Value::Null));
                new.insert(key.clone(), value.clone());
            }
        }
    }
    (Value::Object(old), Value::Object(new))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelBody {
    name: String,
    channel_id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    style: Option<String>,
    topics: Vec<Topic>,
    #[serde(default)]
    questions: Vec<Question>,
    #[serde(default)]
    support_roles: Vec<String>,
    #[serde(default)]
    welcome_text: String,
    #[serde(default)]
    open_category_id: Option<String>,
    #[serde(default)]
    log_channel_id: Option<String>,
    #[serde(default = "yes")]
    enabled: bool,
}

fn yes() -> bool {
    true
}

/// A panel body checked like `/tickets panel` checks one, against the server's channels and roles.
struct CheckedPanel {
    channel: u64,
    color: u32,
    style: PanelStyle,
    roles: Vec<u64>,
    open_category: Option<u64>,
    log: Option<u64>,
}

async fn check_panel_body(state: &WebState, guild: u64, body: &PanelBody) -> ApiResult<CheckedPanel> {
    let channel = parse_id(&body.channel_id).ok_or_else(|| invalid("channel_id", "Pick a channel."))?;
    let roles: Vec<u64> = body
        .support_roles
        .iter()
        .map(|raw| parse_id(raw).ok_or_else(|| ApiError::malformed("Role ids are numbers.")))
        .collect::<ApiResult<_>>()?;
    let open_category = optional_id("open_category_id", &body.open_category_id)?;
    let log = optional_id("log_channel_id", &body.log_channel_id)?;
    let mut named = vec![channel];
    named.extend(open_category);
    named.extend(log);
    let info = info_for(state, guild, &named, &roles).await?;
    let style = match body.style.as_deref() {
        None => PanelStyle::Buttons,
        Some(key) => PanelStyle::parse(key).ok_or_else(|| invalid("style", "A panel shows buttons or a menu."))?,
    };
    let mut errors = tickets::check_panel(
        &PanelDraft {
            name: &body.name,
            title: &body.title,
            description: &body.description,
            style,
            topics: &body.topics,
            questions: &body.questions,
            welcome: &body.welcome_text,
            support_roles: roles.len(),
        },
        Lang::En,
    );
    errors.extend(info.check_post_channel("channel_id", channel).err());
    errors.extend(check_category(&info, "open_category_id", open_category).err());
    if let Some(log) = log {
        errors.extend(info.check_post_channel("log_channel_id", log).err());
    }
    for role in &roles {
        match info.check_role("support_roles", *role) {
            Ok(r) if r.everyone => errors.push(FieldError::new(
                "support_roles",
                "@everyone can't be a support role: everybody would see every ticket.",
            )),
            Ok(_) => {}
            Err(problem) => errors.push(problem),
        }
    }
    let color = match body.color.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        None => tickets::DEFAULT_COLOR,
        Some(hex) => match parse_hex_color(hex) {
            Some(color) => color,
            None => {
                errors.push(FieldError::new("color", "Colours look like `#ff6b4a`."));
                0
            }
        },
    };
    if !errors.is_empty() {
        return Err(ApiError::invalid(errors));
    }
    Ok(CheckedPanel {
        channel,
        color,
        style,
        roles,
        open_category,
        log,
    })
}

async fn save_panel(
    state: &WebState,
    managed: &ManagedGuild,
    id: Option<i64>,
    body: PanelBody,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let guild = managed.id;
    let checked = check_panel_body(state, guild, &body).await?;
    let db = &state.app.db;
    let was = match id {
        Some(id) => Some(
            db.ticket_panel(id)
                .await?
                .filter(|p| p.guild_id as u64 == guild)
                .ok_or_else(ApiError::not_found)?,
        ),
        None => None,
    };
    let write = PanelWrite {
        guild_id: guild,
        name: &body.name,
        channel_id: checked.channel,
        title: &body.title,
        description: &body.description,
        color: checked.color,
        style: checked.style,
        topics: &body.topics,
        questions: &body.questions,
        support_roles: &checked.roles,
        welcome_text: &body.welcome_text,
        open_category_id: checked.open_category,
        log_channel_id: checked.log,
        enabled: body.enabled,
    };
    let panel = match db.save_ticket_panel(id, &write).await? {
        TicketPanelSaved::Saved(panel) => panel,
        TicketPanelSaved::Full => {
            return Err(ApiError::limit(format!(
                "A server has at most {MAX_PANELS} ticket panels."
            )));
        }
        TicketPanelSaved::NameTaken => {
            return Err(invalid(
                "name",
                format!("Another panel is already called “{}”.", body.name.trim()),
            ));
        }
        TicketPanelSaved::Gone => return Err(ApiError::not_found()),
    };
    let after = panel_json(&panel);
    let target = panel.id.to_string();
    let (before, after) = match &was {
        Some(was) => changed(&panel_json(was), &after),
        None => (Value::Null, after.clone()),
    };
    audit::finish(
        state,
        &managed.current.session,
        Change {
            guild,
            action: if was.is_some() {
                "tickets.panel.update"
            } else {
                "tickets.panel.create"
            },
            target: Some(&target),
            before,
            after,
        },
        None,
    )
    .await;
    let status = if was.is_some() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(panel_json(&panel))))
}

/// `POST /tickets/panels`.
pub async fn create_panel(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<PanelBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    save_panel(&state, &managed, None, body).await
}

/// `PUT /tickets/panels/{id}`: the whole panel.
pub async fn replace_panel(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<PanelBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let id = path_id(&params)?;
    save_panel(&state, &managed, Some(id), body).await
}

fn path_id(params: &HashMap<String, String>) -> ApiResult<i64> {
    params
        .get("id")
        .and_then(|id| id.parse::<i64>().ok())
        .filter(|id| *id > 0)
        .ok_or_else(ApiError::not_found)
}

/// `DELETE /tickets/panels/{id}`: the panel goes, its tickets stay.
pub async fn delete_panel(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<StatusCode> {
    let id = path_id(&params)?;
    let guild = managed.id;
    let Some(panel) = state.app.db.delete_ticket_panel(guild, id).await? else {
        return Err(ApiError::not_found());
    };
    let target = panel.id.to_string();
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "tickets.panel.delete",
            target: Some(&target),
            before: panel_json(&panel),
            after: Value::Null,
        },
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /tickets/panels/{id}/post`: the bot posts the panel in its channel, or edits it there.
pub async fn post_panel(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let id = path_id(&params)?;
    let guild = managed.id;
    let owned = state
        .app
        .db
        .ticket_panel(id)
        .await?
        .is_some_and(|p| p.guild_id as u64 == guild);
    if !owned {
        return Err(ApiError::not_found());
    }
    match post_ticket_panel(&state.app, id, Lang::En).await? {
        TicketPanelPost::Posted {
            channel,
            message,
            updated,
        } => {
            let target = id.to_string();
            audit::finish(
                &state,
                &managed.current.session,
                Change {
                    guild,
                    action: "tickets.panel.post",
                    target: Some(&target),
                    before: Value::Null,
                    after: json!({ "channel_id": channel.to_string(), "updated": updated }),
                },
                None,
            )
            .await;
            Ok(Json(json!({
                "channel_id": channel.to_string(),
                "message_id": message.to_string(),
                "updated": updated,
            })))
        }
        TicketPanelPost::Refused(why) => Err(ApiError::blocked(why)),
        TicketPanelPost::Gone => Err(ApiError::not_found()),
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    state: Option<String>,
    staff: Option<String>,
    user: Option<String>,
    q: Option<String>,
    before: Option<String>,
}

/// Names for the people a page of tickets mentions, as Discord has them; unknown ones are left out.
pub(in crate::routes) async fn names(state: &WebState, ids: impl IntoIterator<Item = u64>) -> HashMap<u64, String> {
    let mut wanted: Vec<u64> = ids.into_iter().collect();
    wanted.sort_unstable();
    wanted.dedup();
    wanted.truncate(60);
    let found = join_all(wanted.iter().map(|id| state.app.user(*id))).await;
    wanted
        .into_iter()
        .zip(found)
        .filter_map(|(id, user)| user.map(|u| (id, u.global_name.clone().unwrap_or_else(|| u.name.clone()))))
        .collect()
}

fn ticket_json(t: &Ticket, names: &HashMap<u64, String>, panels: &HashMap<i64, String>) -> Value {
    let person = |id: Option<i64>| -> Value {
        id.map_or(
            Value::Null,
            |id| json!({ "id": id.to_string(), "name": names.get(&(id as u64)) }),
        )
    };
    json!({
        "id": t.id.to_string(),
        "number": t.number,
        "state": t.state().key(),
        "topic": t.topic,
        "panel": t.panel_id.and_then(|p| panels.get(&p)),
        "opener": person(Some(t.opener_id)),
        "claimed_by": person(t.claimed_by),
        "closed_by": person(t.closed_by),
        "close_reason": t.close_reason,
        "channel_id": id_string(t.channel_id),
        "thread_id": id_string(t.thread_id),
        "locked": t.locked,
        "opened_at": t.opened_at,
        "closed_at": t.closed_at,
        "first_reply_at": t.first_reply_at,
        "rating": t.rating,
        "answers": t.answers(),
    })
}

/// `GET /tickets/list`: newest first, 50 at a time, filtered by state, staff, member or words.
pub async fn list(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let parse = |raw: &Option<String>| -> ApiResult<Option<u64>> {
        match raw.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(raw) => parse_id(raw)
                .map(Some)
                .ok_or_else(|| ApiError::malformed("Member ids are numbers.")),
        }
    };
    let filter = TicketFilter {
        state: query.state.clone().filter(|s| !s.is_empty()),
        staff: parse(&query.staff)?,
        opener: parse(&query.user)?,
        text: query.q.clone().filter(|q| !q.trim().is_empty()),
        before: query.before.as_deref().and_then(|b| b.parse().ok()),
    };
    let db = &state.app.db;
    let items = db.ticket_list(guild, &filter, PAGE + 1).await?;
    let more = items.len() as i64 > PAGE;
    let items = &items[..items.len().min(PAGE as usize)];
    let mut ids: Vec<u64> = Vec::new();
    for t in items {
        ids.extend(
            [Some(t.opener_id), t.claimed_by, t.closed_by]
                .into_iter()
                .flatten()
                .map(|id| id as u64),
        );
    }
    let people = names(&state, ids).await;
    let panels: HashMap<i64, String> = db
        .ticket_panels(guild)
        .await?
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect();
    Ok(Json(json!({
        "items": items.iter().map(|t| ticket_json(t, &people, &panels)).collect::<Vec<_>>(),
        "next": if more { items.last().map(|t| Value::String(t.id.to_string())) } else { None },
    })))
}

/// The ticket of this server numbered `id` in the path, or 404.
async fn owned_ticket(state: &WebState, guild: u64, params: &HashMap<String, String>) -> ApiResult<Ticket> {
    let id = path_id(params)?;
    state
        .app
        .db
        .ticket(id)
        .await?
        .filter(|t| t.guild_id as u64 == guild)
        .ok_or_else(ApiError::not_found)
}

/// `POST /tickets/ticket/{id}/close`: a server manager closes it from the dashboard.
pub async fn close_ticket(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let ticket = owned_ticket(&state, guild, &params).await?;
    if !ticket.is_open() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "ticket_closed",
            pinchora_core::t!(Lang::En, "tickets.web.already-closed"),
        ));
    }
    let closed = close_ticket_from_web(
        &state.app,
        ticket,
        managed.current.session.user_id,
        managed.current.session.display_name().to_owned(),
    )
    .await?
    .ok_or_else(|| {
        ApiError::new(
            StatusCode::CONFLICT,
            "ticket_closed",
            pinchora_core::t!(Lang::En, "tickets.web.already-closed"),
        )
    })?;
    let target = closed.id.to_string();
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "tickets.close",
            target: Some(&target),
            before: json!({ "state": "open" }),
            after: json!({ "state": "closed" }),
        },
        None,
    )
    .await;
    Ok(Json(json!({ "id": closed.id.to_string(), "state": "closed" })))
}

/// Only retained author rows can give a mention its name. Privacy deletion removes that row.
fn mention_names(stored: &StoredTranscript) -> HashMap<u64, String> {
    stored
        .author_ids
        .iter()
        .filter_map(|(idx, id)| {
            stored
                .authors
                .iter()
                .find(|author| author.idx == *idx)?
                .name
                .as_ref()
                .map(|name| (*id, name.clone()))
        })
        .collect()
}

/// `GET /tickets/ticket/{id}`: the ticket and, once closed, its transcript.
pub async fn ticket(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let ticket = owned_ticket(&state, guild, &params).await?;
    let stored = state.app.db.transcript(ticket.id).await?;
    let people = names(
        &state,
        [Some(ticket.opener_id), ticket.claimed_by, ticket.closed_by]
            .into_iter()
            .flatten()
            .map(|id| id as u64),
    )
    .await;
    let panels: HashMap<i64, String> = state
        .app
        .db
        .ticket_panels(guild)
        .await?
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect();
    let transcript = stored.map(|s| {
        let mentions = mention_names(&s);
        let lines: Vec<_> = s.lines.into_iter().map(|mut line| {
            line.c = line.c.map(|content| tickets::render_mentions(&content, &mentions, Lang::En));
            line
        }).collect();
        json!({
            "with_content": s.with_content,
            "messages": s.messages,
            "authors": s.authors.iter().map(|a| json!({ "idx": a.idx, "name": a.name, "bot": a.bot })).collect::<Vec<_>>(),
            "lines": lines,
        })
    });
    Ok(Json(json!({
        "ticket": ticket_json(&ticket, &people, &panels),
        "transcript": transcript,
        "message_content": state.app.message_content,
    })))
}

/// `GET /tickets/ticket/{id}/transcript.html`: the transcript page, as a download.
pub async fn transcript_page(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Response> {
    let guild = managed.id;
    let ticket = owned_ticket(&state, guild, &params).await?;
    let Some(stored) = state.app.db.transcript(ticket.id).await? else {
        return Err(ApiError::not_found());
    };
    let people = names(
        &state,
        [Some(ticket.opener_id), ticket.closed_by]
            .into_iter()
            .flatten()
            .map(|id| id as u64),
    )
    .await;
    let server = managed.guild.name.clone();
    let head = TranscriptHead {
        server: &server,
        number: ticket.number,
        topic: &ticket.topic,
        opener: people.get(&(ticket.opener_id as u64)).map(String::as_str),
        opened_at: ticket.opened_at,
        closed_at: ticket.closed_at,
        closed_by: ticket
            .closed_by
            .and_then(|id| people.get(&(id as u64)))
            .map(String::as_str),
        reason: ticket.close_reason.as_deref(),
        with_content: stored.with_content,
    };
    let html =
        tickets::render_html_with_mentions(&head, &stored.lines, &stored.authors, &mention_names(&stored), Lang::En);
    let mut response = (StatusCode::OK, html).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    let disposition = format!("attachment; filename=\"ticket-{:04}.html\"", ticket.number);
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}
