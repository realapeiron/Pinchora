//! Moderation on the dashboard: the mod-log and the warn ladder (`/moderation`), the server log's
//! channels, events and ignore lists (`/logs`), the cases (`/cases`) and the staff action log
//! (`/staff-log`). The rules are the bot's own (`pinchora_core::moderation::moderation` and `::logging`).

mod log_settings;
use log_settings::{LogChanges, check_log_destinations, parse_log_changes};

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{DateTime, Utc};
use pinchora_cache::Invalidation;
use pinchora_core::admin::logging::LogCategory;
use pinchora_core::admin::logging::LogConfig;
use pinchora_core::admin::logging::LogEvent;
use pinchora_core::admin::logging::MAX_IGNORED;
use pinchora_core::admin::logging::check_ignored;
use pinchora_core::admin::server_events::parse_duration;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::moderation::moderation::CaseKind;
use pinchora_core::moderation::moderation::MAX_THRESHOLDS;
use pinchora_core::moderation::moderation::Rank;
use pinchora_core::moderation::moderation::Threshold;
use pinchora_core::moderation::moderation::check_reason;
use pinchora_core::moderation::moderation::check_threshold;
use pinchora_core::moderation::moderation::format_duration;
use pinchora_core::moderation::moderation::outranks;
use pinchora_core::t;
use pinchora_db::{CaseFilter, CaseRow, ModSettings, StaffActionRow, StaffFilter};
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
use crate::routes::admin::settings::parse_id;
use crate::routes::admin::settings::present;
use crate::state::WebState;

/// Rows per page of the cases and the staff log.
const PAGE: i64 = 50;

fn ids(list: &[u64]) -> Vec<String> {
    list.iter().map(u64::to_string).collect()
}

fn rung_json(rung: &Threshold) -> Value {
    json!({
        "warns": rung.warns,
        "action": rung.action.key(),
        "duration": rung.duration.map(format_duration),
        "duration_secs": rung.duration.map(|d| d.num_seconds()),
        "label": rung.describe(),
    })
}

fn moderation_json(settings: &ModSettings, ladder: &[Threshold]) -> Value {
    json!({
        "log_channel_id": settings.log_channel_id.map(|id| id.to_string()),
        "dm_members": settings.dm_members,
        "ladder": ladder.iter().map(rung_json).collect::<Vec<_>>(),
        "max_rungs": MAX_THRESHOLDS,
    })
}

/// `GET /moderation`.
pub async fn moderation(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let db = &state.app.db;
    let settings = db.mod_settings(managed.id).await?;
    let ladder = db.warn_thresholds(managed.id).await?;
    Ok(Json(moderation_json(&settings, &ladder)))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RungBody {
    warns: i64,
    action: String,
    #[serde(default)]
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModerationBody {
    #[serde(default, deserialize_with = "present")]
    log_channel_id: Option<Option<String>>,
    dm_members: Option<bool>,
    ladder: Option<Vec<RungBody>>,
}

/// The ladder as sent, checked with the command's rules: each rung, no warn count twice, not too many.
fn check_ladder(rungs: &[RungBody]) -> Result<Vec<Threshold>, FieldError> {
    if rungs.len() > MAX_THRESHOLDS {
        return Err(FieldError::new(
            "ladder",
            format!("The warn ladder holds at most {MAX_THRESHOLDS} rungs."),
        ));
    }
    let mut ladder: Vec<Threshold> = Vec::with_capacity(rungs.len());
    for rung in rungs {
        let duration = match rung.duration.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
            Some(text) => Some(
                parse_duration(text)
                    .ok_or_else(|| FieldError::new("ladder", "Write a duration like `1h`, `12h` or `7d`."))?,
            ),
            None => None,
        };
        let checked = check_threshold(rung.warns, &rung.action, duration)
            .map_err(|e| FieldError::new("ladder", format!("{} warns: {}", rung.warns, e.message)))?;
        if ladder.iter().any(|t| t.warns == checked.warns) {
            return Err(FieldError::new(
                "ladder",
                format!("{} warns is on the ladder twice.", checked.warns),
            ));
        }
        ladder.push(checked);
    }
    ladder.sort_by_key(|t| t.warns);
    Ok(ladder)
}

/// `PUT /moderation`: the mod-log channel (`null`: the server log's moderation channel), direct
/// messages, and the whole warn ladder.
pub async fn put_moderation(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<ModerationBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let db = &state.app.db;
    let ladder = body.ladder.as_deref().map(check_ladder).transpose()?;
    let channel = match body.log_channel_id {
        Some(Some(raw)) => {
            let id = parse_id(&raw).ok_or_else(|| ApiError::malformed("Channel ids are numbers."))?;
            let info = guild_info::get_with(&state, guild, |info| info.channel(id).is_some()).await?;
            info.check_post_channel("log_channel_id", id)?;
            Some(Some(id))
        }
        Some(None) => Some(None),
        None => None,
    };
    let before = moderation_json(&db.mod_settings(guild).await?, &db.warn_thresholds(guild).await?);
    let settings = if channel.is_some() || body.dm_members.is_some() {
        db.set_mod_settings(guild, channel, body.dm_members).await?
    } else {
        db.mod_settings(guild).await?
    };
    let ladder = match ladder {
        Some(ladder) => db.replace_warn_thresholds(guild, &ladder).await?,
        None => db.warn_thresholds(guild).await?,
    };
    let after = moderation_json(&settings, &ladder);
    if before != after {
        let (old, new) = changed(&before, &after, &["log_channel_id", "dm_members", "ladder"]);
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "moderation.set",
                target: None,
                before: old,
                after: new,
            },
            None,
        )
        .await;
    }
    Ok(Json(after))
}

/// The fields of `keys` that differ, as `(before, after)` objects.
fn changed(before: &Value, after: &Value, keys: &[&str]) -> (Value, Value) {
    let (mut old, mut new) = (serde_json::Map::new(), serde_json::Map::new());
    for key in keys {
        if before[key] != after[key] {
            old.insert((*key).into(), before[key].clone());
            new.insert((*key).into(), after[key].clone());
        }
    }
    (Value::Object(old), Value::Object(new))
}

/// The server log's settings as the dashboard shows them: each category with its channel and events.
pub fn logs_json(config: &LogConfig) -> Value {
    let categories: Vec<Value> = LogCategory::ALL
        .iter()
        .map(|category| {
            let events: Vec<Value> = category
                .events()
                .map(|event| {
                    json!({
                        "key": event.key(),
                        "label": event.label(),
                        "enabled": config.enabled(event),
                        "audit_log": event.needs_audit_log(),
                    })
                })
                .collect();
            json!({
                "key": category.key(),
                "label": category.label(),
                "channel_id": config.channels.get(category).map(u64::to_string),
                "events": events,
            })
        })
        .collect();
    json!({
        "channel_id": config.default_channel.map(|id| id.to_string()),
        "categories": categories,
        "ignored_channels": ids(&config.ignored_channels),
        "ignored_roles": ids(&config.ignored_roles),
        "ignored_users": ids(&config.ignored_users),
        "ignore_bots": config.ignore_bots,
        "max_ignored": MAX_IGNORED,
    })
}

/// `GET /logs`.
pub async fn logs(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(logs_json(&state.app.db.log_config(managed.id).await?)))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogsBody {
    #[serde(default, deserialize_with = "present")]
    channel_id: Option<Option<String>>,
    /// Category key to its own channel, or `null` for the log channel.
    category_channels: Option<HashMap<String, Option<String>>>,
    /// Event key to on or off.
    events: Option<HashMap<String, bool>>,
    ignored_channels: Option<Vec<String>>,
    ignored_roles: Option<Vec<String>>,
    ignored_users: Option<Vec<String>>,
    ignore_bots: Option<bool>,
}

fn id_list(field: &'static str, raw: &[String]) -> Result<Vec<u64>, FieldError> {
    let parsed: Vec<u64> = raw
        .iter()
        .map(|id| parse_id(id).ok_or_else(|| FieldError::new(field, "That isn't a Discord id.")))
        .collect::<Result<_, _>>()?;
    check_ignored(field, &parsed)
}

/// `PUT /logs`: any of the log channel, the category channels, single events and the ignore lists.
pub async fn put_logs(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<LogsBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let db = &state.app.db;
    let now = db.log_config(guild).await?;
    let LogChanges {
        events,
        categories,
        default_channel,
        lists,
    } = parse_log_changes(&body)?;
    check_log_destinations(&state, guild, &now, default_channel, &categories, &lists).await?;

    let [channels_list, roles_list, users_list] = lists;
    let after = db
        .update_log_config(guild, |config| {
            if let Some(channel) = default_channel {
                config.default_channel = channel;
            }
            for (category, channel) in &categories {
                match channel {
                    Some(id) => {
                        config.channels.insert(*category, *id);
                    }
                    None => {
                        config.channels.remove(category);
                    }
                }
            }
            for (event, on) in &events {
                config.toggle(*event, *on);
            }
            if let Some(list) = channels_list {
                config.ignored_channels = list;
            }
            if let Some(list) = roles_list {
                config.ignored_roles = list;
            }
            if let Some(list) = users_list {
                config.ignored_users = list;
            }
            if let Some(bots) = body.ignore_bots {
                config.ignore_bots = bots;
            }
        })
        .await?;
    let (before_json, after_json) = (logs_json(&now), logs_json(&after));
    if before_json != after_json {
        let (old, new) = changed(
            &before_json,
            &after_json,
            &[
                "channel_id",
                "categories",
                "ignored_channels",
                "ignored_roles",
                "ignored_users",
                "ignore_bots",
            ],
        );
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "logs.set",
                target: None,
                before: old,
                after: new,
            },
            Some(Invalidation::Guild(guild)),
        )
        .await;
    }
    Ok(Json(after_json))
}

/// One case as the dashboard shows it.
pub fn case_json(case: &CaseRow) -> Value {
    let kind = case.kind();
    json!({
        "number": case.number,
        "kind": case.kind,
        "label": kind.map_or_else(|| case.kind.clone(), |k| k.label().to_owned()),
        "on_channel": kind.is_some_and(CaseKind::on_channel),
        "target_id": case.target_id.to_string(),
        "target_name": case.target_name,
        "moderator_id": case.moderator_id.to_string(),
        "moderator_name": case.moderator_name,
        "reason": case.reason,
        "duration": case.duration().map(format_duration),
        "expires_at": case.expires_at,
        "details": case.details,
        "active": case.active,
        "dm_sent": case.dm_sent,
        "created_at": case.created_at,
        "updated_at": case.updated_at,
    })
}

#[derive(Debug, Deserialize)]
pub struct CasesQuery {
    q: Option<String>,
    kind: Option<String>,
    member: Option<String>,
    moderator: Option<String>,
    active: Option<bool>,
    before: Option<i32>,
}

fn optional_id(raw: Option<&str>) -> ApiResult<Option<u64>> {
    match raw.map(str::trim).filter(|r| !r.is_empty()) {
        Some(raw) => parse_id(raw)
            .map(Some)
            .ok_or_else(|| ApiError::malformed("Ids are numbers.")),
        None => Ok(None),
    }
}

/// `GET /cases?q=&kind=&member=&moderator=&active=&before=`: newest first, a page at a time.
pub async fn cases(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(query): Query<CasesQuery>,
) -> ApiResult<Json<Value>> {
    let kind = match query.kind.as_deref().filter(|k| !k.is_empty()) {
        Some(key) => Some(CaseKind::parse(key).ok_or_else(|| ApiError::malformed("Unknown kind of case."))?),
        None => None,
    };
    let filter = CaseFilter {
        target: optional_id(query.member.as_deref())?,
        moderator: optional_id(query.moderator.as_deref())?,
        kind,
        active_only: query.active == Some(true),
        query: query.q.map(|q| q.chars().take(100).collect()),
        before: query.before,
        limit: PAGE + 1,
    };
    let rows = state.app.db.cases(managed.id, &filter).await?;
    let counts = state.app.db.case_kind_counts(managed.id, &filter).await?;
    let more = rows.len() as i64 > PAGE;
    let items: Vec<Value> = rows.iter().take(PAGE as usize).map(case_json).collect();
    let next = more.then(|| rows.get(PAGE as usize - 1).map(|c| c.number)).flatten();
    Ok(Json(json!({
        "items": items,
        "next": next,
        "kinds": CaseKind::ALL.iter().map(|k| json!({ "key": k.key(), "label": k.label(), "count": counts.iter().find(|(kind, _)| kind == k.key()).map(|(_, count)| *count).unwrap_or(0) })).collect::<Vec<_>>(),
    })))
}

fn case_number(params: &HashMap<String, String>) -> ApiResult<i32> {
    params
        .get("case")
        .and_then(|raw| raw.parse::<i32>().ok())
        .filter(|n| *n > 0)
        .ok_or_else(ApiError::not_found)
}

/// `GET /cases/{case}`: the case, and the member's other cases.
pub async fn case(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let number = case_number(&params)?;
    let db = &state.app.db;
    let case = db.case(managed.id, number).await?.ok_or_else(ApiError::not_found)?;
    let history = db
        .cases(
            managed.id,
            &CaseFilter {
                target: Some(case.target_id as u64),
                limit: 25,
                ..CaseFilter::default()
            },
        )
        .await?;
    Ok(Json(json!({
        "case": case_json(&case),
        "history": history.iter().filter(|c| c.number != number).map(case_json).collect::<Vec<_>>(),
    })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasonBody {
    reason: Option<String>,
}

/// Read the hierarchy live for a reason edit; failed lookups never grant permission.
async fn check_reason_editor(state: &WebState, managed: &ManagedGuild, case: &CaseRow) -> ApiResult<()> {
    let actor = managed.current.session.user_id;
    let opened_by = case.moderator_id as u64;
    if actor == opened_by || managed.guild.owner {
        return Ok(());
    }
    let lang = state.app.guild_lang(managed.id).await;
    let unavailable = || ApiError::blocked(t!(lang, "mod.no-roles"));
    let by_bot = opened_by == state.app.application_id.get() || opened_by == 0;
    let other = if by_bot {
        if case.kind().is_some_and(CaseKind::on_channel) {
            return Ok(());
        }
        case.target_id as u64
    } else {
        opened_by
    };
    let guild = Id::new(managed.id);
    let facts = state
        .app
        .http
        .guild(guild)
        .await
        .map_err(|_| unavailable())?
        .model()
        .await
        .map_err(|_| unavailable())?;
    let roles = state
        .app
        .http
        .roles(guild)
        .await
        .map_err(|_| unavailable())?
        .models()
        .await
        .map_err(|_| unavailable())?;
    let rank = async |user| -> ApiResult<Rank> {
        match state.app.http.guild_member(guild, Id::new(user)).await {
            Ok(response) => {
                let member = response.model().await.map_err(|_| unavailable())?;
                let top = roles
                    .iter()
                    .filter(|r| member.roles.contains(&r.id))
                    .map(|r| r.position)
                    .max()
                    .unwrap_or(0);
                Ok(Rank { id: user, top })
            }
            Err(error) if matches!(error.kind(), twilight_http::error::ErrorType::Response { error: twilight_http::api_error::ApiError::General(error), .. } if error.code == 10007) => {
                Ok(Rank { id: user, top: -1 })
            }
            Err(_) => Err(unavailable()),
        }
    };
    let actor_rank = rank(actor).await?;
    let other_rank = rank(other).await?;
    if !outranks(facts.owner_id.get(), actor_rank, other_rank) {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "hierarchy",
            t!(
                lang,
                if by_bot {
                    "mod.reason-target"
                } else {
                    "mod.reason-moderator"
                },
                number = case.number,
                moderator = format!("<@{opened_by}>")
            ),
        ));
    }
    Ok(())
}

/// `PATCH /cases/{case}`: a new reason. The mod-log entry is edited to match, and the staff log notes it.
pub async fn patch_case(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<ReasonBody>,
) -> ApiResult<Json<Value>> {
    let number = case_number(&params)?;
    let reason = check_reason(body.reason.as_deref())?;
    let session = &managed.current.session;
    let original = state
        .app
        .db
        .case(managed.id, number)
        .await?
        .ok_or_else(ApiError::not_found)?;
    check_reason_editor(&state, &managed, &original).await?;
    let case = state
        .app
        .db
        .set_case_reason(
            managed.id,
            number,
            reason.as_deref(),
            session.user_id,
            session.display_name(),
            "web",
        )
        .await?
        .ok_or_else(ApiError::not_found)?;
    pinchora_commands::refresh_case_post(&state.app, &case).await;
    tracing::info!(
        guild = managed.id,
        case = number,
        user_id = session.user_id,
        "case reason changed on the web"
    );
    Ok(Json(case_json(&case)))
}

fn staff_json(row: &StaffActionRow) -> Value {
    json!({
        "key": row.key,
        "actor_id": row.actor_id.to_string(),
        "actor_name": row.actor_name,
        "source": row.source,
        "action": row.action,
        "target_id": row.target_id.map(|id| id.to_string()),
        "target_name": row.target_name,
        "case_number": row.case_number,
        "reason": row.reason,
        "details": row.details,
        "created_at": row.created_at,
    })
}

#[derive(Debug, Deserialize)]
pub struct StaffQuery {
    q: Option<String>,
    source: Option<String>,
    actor: Option<String>,
    before: Option<DateTime<Utc>>,
}

/// `GET /staff-log?q=&source=&actor=&before=`: what staff did through Pinchora (commands and the
/// dashboard) and by hand (Discord's audit log), newest first.
pub async fn staff_log(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(query): Query<StaffQuery>,
) -> ApiResult<Json<Value>> {
    let source = match query.source.as_deref().filter(|s| !s.is_empty()) {
        Some(s @ ("command" | "discord" | "web")) => Some(s.to_owned()),
        Some(_) => return Err(ApiError::malformed("The source is command, discord or web.")),
        None => None,
    };
    let filter = StaffFilter {
        actor: optional_id(query.actor.as_deref())?,
        source,
        query: query.q.map(|q| q.chars().take(100).collect()),
        before: query.before,
        limit: PAGE + 1,
    };
    let rows = state.app.db.staff_actions(managed.id, &filter).await?;
    let more = rows.len() as i64 > PAGE;
    let items: Vec<Value> = rows.iter().take(PAGE as usize).map(staff_json).collect();
    let next = more
        .then(|| rows.get(PAGE as usize - 1).map(|r| r.created_at))
        .flatten();
    Ok(Json(json!({ "items": items, "next": next })))
}

#[cfg(test)]
mod tests {
    use crate::routes::moderation::moderation::*;

    fn rung(warns: i64, action: &str, duration: Option<&str>) -> RungBody {
        RungBody {
            warns,
            action: action.to_owned(),
            duration: duration.map(str::to_owned),
        }
    }

    #[test]
    fn the_ladder_follows_the_command_rules() {
        let ladder = check_ladder(&[rung(5, "kick", None), rung(3, "timeout", Some("1h"))]).unwrap();
        assert_eq!(ladder.iter().map(|t| t.warns).collect::<Vec<_>>(), [3, 5], "sorted");
        let twice = check_ladder(&[rung(3, "kick", None), rung(3, "ban", None)]).unwrap_err();
        assert!(twice.message.contains("twice"), "{}", twice.message);
        let no_length = check_ladder(&[rung(3, "timeout", None)]).unwrap_err();
        assert!(
            no_length.message.starts_with("3 warns: Say how long"),
            "{}",
            no_length.message
        );
        assert!(check_ladder(&[rung(3, "timeout", Some("soon"))]).is_err());
        let many: Vec<RungBody> = (1..=11).map(|n| rung(n, "kick", None)).collect();
        assert!(check_ladder(&many).is_err());
    }

    #[test]
    fn the_log_settings_list_every_category_and_event() {
        let shown = logs_json(&LogConfig::default());
        let categories = shown["categories"].as_array().unwrap();
        assert_eq!(categories.len(), LogCategory::ALL.len());
        let events: usize = categories.iter().map(|c| c["events"].as_array().unwrap().len()).sum();
        assert_eq!(events, LogEvent::ALL.len());
        let voice_self = categories
            .iter()
            .flat_map(|c| c["events"].as_array().unwrap())
            .find(|e| e["key"] == "voice_self")
            .unwrap();
        assert_eq!(voice_self["enabled"], false);
    }

    #[test]
    fn unknown_fields_are_refused() {
        assert!(serde_json::from_value::<LogsBody>(json!({ "events": {}, "colour": 1 })).is_err());
        let body: LogsBody = serde_json::from_value(json!({ "channel_id": null })).unwrap();
        assert_eq!(body.channel_id, Some(None));
    }
}
