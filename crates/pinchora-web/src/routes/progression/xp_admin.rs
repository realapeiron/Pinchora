//! Dashboard XP editor and MEE6 import job.

use std::{collections::HashMap, time::Duration};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
};
use pinchora_commands::{ImportMode, ImportProgress, ImportRequest, import_mee6, mee6_resting};
use pinchora_core::t;
use pinchora_db::{AuditSource, XpEdit};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::watch;
use twilight_model::id::{
    Id,
    marker::{GuildMarker, UserMarker},
};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::state::WebState;

const JOB_TTL: Duration = Duration::from_secs(24 * 3600);
const LOCK_TTL: Duration = Duration::from_secs(20 * 60);

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/members", get(members))
        .route("/members/{user}", put(edit_member))
        .route("/import/mee6", post(start_import))
        .route("/jobs/{id}", get(job))
}

#[derive(Deserialize)]
struct Search {
    search: Option<String>,
}

/// Search by Discord member name or id, then read their Pinchora server XP.
async fn members(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(query): Query<Search>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let search = query.search.unwrap_or_default();
    if search.trim().is_empty() {
        return Ok(Json(json!({ "items": [] })));
    }
    if search.len() > 80 {
        return Err(ApiError::malformed(t!(lang, "xp.web.search-long")));
    }
    let guild = managed.id;
    let people: Vec<(u64, String)> = if let Ok(id) = search.parse::<u64>() {
        if id == 0 || id > i64::MAX as u64 {
            return Err(ApiError::malformed(t!(lang, "xp.web.invalid-member")));
        }
        let user = state
            .app
            .http
            .guild_member(Id::<GuildMarker>::new(guild), Id::<UserMarker>::new(id))
            .await
            .map_err(|_| ApiError::not_found())?
            .model()
            .await
            .map_err(|_| ApiError::not_found())?;
        vec![(id, user.user.name)]
    } else {
        let members = state
            .app
            .http
            .search_guild_members(Id::<GuildMarker>::new(guild), &search)
            .limit(10)
            .await
            .map_err(|_| ApiError::blocked(t!(lang, "xp.web.search-unavailable")))?
            .model()
            .await
            .map_err(|_| ApiError::blocked(t!(lang, "xp.web.search-unavailable")))?;
        members.into_iter().map(|m| (m.user.id.get(), m.user.name)).collect()
    };
    let mut items = Vec::new();
    for (id, name) in people {
        let xp = state.app.db.member_rank(guild, id).await?.map_or(0, |r| r.xp);
        items.push(json!({ "id": id.to_string(), "name": name, "xp": xp }));
    }
    Ok(Json(json!({ "items": items })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditBody {
    operation: String,
    amount: i64,
}

/// `PUT /members/{user}`: the same lock-safe edit as `/xp`.
async fn edit_member(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<EditBody>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let user = params
        .get("user")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        .ok_or_else(|| ApiError::malformed(t!(lang, "xp.web.invalid-member")))?;
    if !(0..=1_000_000_000).contains(&body.amount) {
        return Err(ApiError::malformed(t!(lang, "xp.invalid-amount")));
    }
    let edit = match body.operation.as_str() {
        "add" => XpEdit::Add,
        "remove" => XpEdit::Remove,
        "set" => XpEdit::Set,
        _ => return Err(ApiError::malformed(t!(lang, "xp.web.operation"))),
    };
    state
        .app
        .http
        .guild_member(Id::new(managed.id), Id::new(user))
        .await
        .map_err(|_| ApiError::not_found())?;
    crate::dashboard_access::check_level_rewards(&state, &managed).await?;
    let (old, new) = state.app.db.edit_member_xp(managed.id, user, edit, body.amount).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "xp.member.edit",
            target: Some(&user.to_string()),
            before: json!({"xp": old}),
            after: json!({"xp": new}),
        },
        None,
    )
    .await;
    pinchora_commands::sync_member_roles(&state.app, managed.id, user).await?;
    Ok(Json(json!({ "id": user.to_string(), "xp": new })))
}

#[derive(Clone, Serialize, Deserialize)]
struct Job {
    id: String,
    guild_id: String,
    status: String,
    checked: usize,
    updated: usize,
    total: usize,
    message: Option<String>,
}
fn job_key(id: &str) -> String {
    format!("web:xp-import-job:{id}")
}
async fn save(state: &WebState, job: &Job) {
    if let Err(error) = state
        .app
        .cache
        .put(
            &job_key(&job.id),
            &serde_json::to_string(job).unwrap_or_default(),
            JOB_TTL,
        )
        .await
    {
        tracing::warn!(%error, "XP import job state not saved");
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportBody {
    mode: String,
}

/// Starts a bounded background import and returns a pollable job id.
async fn start_import(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<ImportBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let lang = state.app.guild_lang(managed.id).await;
    let mode = ImportMode::parse(&body.mode).ok_or_else(|| ApiError::malformed(t!(lang, "xp.web.mode")))?;
    crate::dashboard_access::check_level_rewards(&state, &managed).await?;
    let guild = managed.id;
    if let Some(left) = mee6_resting(&state.app, guild).await? {
        return Err(ApiError::resting(t!(lang, "xp.web.resting"), left.as_secs()));
    }
    let lock = format!("web:xp-import:{guild}");
    if state.app.cache.try_cooldown(&lock, LOCK_TTL).await?.is_some() {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "conflict",
            t!(lang, "xp.web.running"),
        ));
    }
    let id = crate::auth::session::random_token(12);
    let mut job = Job {
        id: id.clone(),
        guild_id: guild.to_string(),
        status: "running".into(),
        checked: 0,
        updated: 0,
        total: 0,
        message: None,
    };
    save(&state, &job).await;
    let actor = managed.current.session.user_id;
    let actor_name = managed.current.session.display_name().to_owned();
    let task_state = state.clone();
    state.app.spawn(async move {
        let state = task_state;
        let (report_tx, mut report_rx) = watch::channel(ImportProgress::default());
        let run = import_mee6(
            &state.app,
            ImportRequest {
                guild,
                actor,
                actor_name: &actor_name,
                source: AuditSource::Web,
                mode,
            },
            move |r| {
                report_tx.send_replace(r);
            },
        );
        let mut run = std::pin::pin!(run);
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        let outcome = loop {
            tokio::select! {
                result = &mut run => break result,
                _ = tick.tick() => if report_rx.has_changed().unwrap_or(false) {
                    let report = *report_rx.borrow_and_update();
                    (job.checked, job.updated, job.total) = (report.checked, report.updated, report.total);
                    save(&state, &job).await;
                }
            }
        };
        match outcome {
            Ok(report) => {
                (job.checked, job.updated, job.total) = (report.checked, report.updated, report.total);
                job.status = "done".into();
                job.message = Some(t!(lang, "xp.web.done", count = report.updated));
            }
            Err(error) => {
                tracing::warn!(guild, %error, "MEE6 import failed");
                job.status = "failed".into();
                job.message = Some(t!(lang, "xp.import-error"));
            }
        }
        save(&state, &job).await;
        if let Err(error) = state.app.cache.release_cooldown(&lock).await {
            tracing::warn!(%error, "XP import lock release failed");
        }
    });
    Ok((StatusCode::ACCEPTED, Json(json!({ "job_id": id }))))
}

async fn job(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Job>> {
    let id = params.get("id").ok_or_else(ApiError::not_found)?;
    if id.len() > 64 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(ApiError::not_found());
    }
    let job: Job = state
        .app
        .cache
        .get(&job_key(id))
        .await?
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .filter(|j: &Job| j.guild_id == managed.id.to_string())
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(job))
}
