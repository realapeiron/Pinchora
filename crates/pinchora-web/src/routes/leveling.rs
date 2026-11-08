//! Leveling lists: XP multipliers, level roles (with the member sync job) and reputation roles.
//! Same rules and limits as `/config xp`, `/config level-roles` and `/config rep-roles`.

use std::{collections::HashMap, time::Duration};

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use pinchora_cache::Invalidation;
use pinchora_commands::SyncOutcome;
use pinchora_core::settings;
use pinchora_db::{MAX_LEVEL_ROLES, MAX_REP_ROLES, MAX_XP_RULES, XpTarget};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::watch;

use super::settings::parse_id;
use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info,
    state::WebState,
};

/// A path segment that must be a Discord id.
fn path_id(params: &HashMap<String, String>, name: &str) -> ApiResult<u64> {
    params
        .get(name)
        .and_then(|raw| parse_id(raw))
        .ok_or_else(|| ApiError::malformed("Discord ids are numbers."))
}

fn target_of(params: &HashMap<String, String>) -> ApiResult<XpTarget> {
    match params.get("kind").map(String::as_str) {
        Some("channel") => Ok(XpTarget::Channel),
        Some("role") => Ok(XpTarget::Role),
        _ => Err(ApiError::not_found()),
    }
}

fn kind_key(target: XpTarget) -> &'static str {
    match target {
        XpTarget::Channel => "channel",
        XpTarget::Role => "role",
    }
}

// /XP multipliers

#[derive(Serialize)]
struct RuleView {
    kind: &'static str,
    target_id: String,
    multiplier: f64,
}

async fn rules_view(state: &WebState, guild: u64) -> ApiResult<Value> {
    let rules = state.app.db.xp_rules(guild).await?;
    let mut items: Vec<RuleView> = rules
        .channels
        .iter()
        .map(|(id, m)| (XpTarget::Channel, *id, *m))
        .chain(rules.roles.iter().map(|(id, m)| (XpTarget::Role, *id, *m)))
        .map(|(target, id, multiplier)| RuleView {
            kind: kind_key(target),
            target_id: id.to_string(),
            multiplier,
        })
        .collect();
    items.sort_by(|a, b| a.kind.cmp(b.kind).then_with(|| a.target_id.cmp(&b.target_id)));
    Ok(json!({ "items": items, "max": MAX_XP_RULES }))
}

/// `GET /xp-rules`.
pub async fn xp_rules(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(rules_view(&state, managed.id).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultiplierBody {
    multiplier: f64,
}

/// `PUT /xp-rules/{kind}/{target}`: 1 drops the rule, like the command.
pub async fn put_xp_rule(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<MultiplierBody>,
) -> ApiResult<Json<Value>> {
    let (guild, target, id) = (managed.id, target_of(&params)?, path_id(&params, "target")?);
    let value = settings::check_multiplier(body.multiplier)?;
    let info = guild_info::get_with(&state, guild, |info| match target {
        XpTarget::Channel => info.channel(id).is_some(),
        XpTarget::Role => info.role(id).is_some(),
    })
    .await?;
    match target {
        XpTarget::Channel => info.check_channel("target_id", id).map(|_| ())?,
        XpTarget::Role => info.check_role("target_id", id).map(|_| ())?,
    }
    let db = &state.app.db;
    let rules = db.xp_rules(guild).await?;
    let map = match target {
        XpTarget::Channel => &rules.channels,
        XpTarget::Role => &rules.roles,
    };
    let was = map.get(&id).copied().unwrap_or(1.0);
    if was == value {
        return Ok(Json(rules_view(&state, guild).await?));
    }
    if value == 1.0 {
        db.remove_xp_rule(guild, target, id).await?;
    } else if !db.set_xp_rule(guild, target, id, value).await? {
        return Err(ApiError::limit(format!(
            "A server can have {MAX_XP_RULES} XP rules. Remove one first."
        )));
    }
    let what = format!("{}:{id}", kind_key(target));
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "xp_rules.put",
            target: Some(&what),
            before: json!({ "multiplier": was }),
            after: json!({ "multiplier": value }),
        },
        Some(Invalidation::XpRules(guild)),
    )
    .await;
    Ok(Json(rules_view(&state, guild).await?))
}

/// `DELETE /xp-rules/{kind}/{target}`.
pub async fn delete_xp_rule(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let (guild, target, id) = (managed.id, target_of(&params)?, path_id(&params, "target")?);
    let db = &state.app.db;
    let rules = db.xp_rules(guild).await?;
    let was = match target {
        XpTarget::Channel => rules.channels.get(&id),
        XpTarget::Role => rules.roles.get(&id),
    }
    .copied();
    if let Some(was) = was
        && db.remove_xp_rule(guild, target, id).await?
    {
        let what = format!("{}:{id}", kind_key(target));
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "xp_rules.delete",
                target: Some(&what),
                before: json!({ "multiplier": was }),
                after: json!({ "multiplier": 1.0 }),
            },
            Some(Invalidation::XpRules(guild)),
        )
        .await;
    }
    Ok(Json(rules_view(&state, guild).await?))
}

/// `DELETE /xp-rules`: every rule at once.
pub async fn clear_xp_rules(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    // The audit row keeps the rules themselves, so a clear can be put back by hand.
    let was = rules_view(&state, guild).await?["items"].clone();
    let cleared = state.app.db.clear_xp_rules(guild).await?;
    if cleared > 0 {
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "xp_rules.clear",
                target: None,
                before: json!({ "rules": was }),
                after: json!({ "rules": [] }),
            },
            Some(Invalidation::XpRules(guild)),
        )
        .await;
    }
    Ok(Json(rules_view(&state, guild).await?))
}

// /Level and reputation roles

/// Which reward list a route works on.
#[derive(Clone, Copy)]
enum Rewards {
    Level,
    Rep,
}

impl Rewards {
    /// The audit action for a put, or a delete.
    const fn action(self, put: bool) -> &'static str {
        match (self, put) {
            (Self::Level, true) => "level_roles.put",
            (Self::Level, false) => "level_roles.delete",
            (Self::Rep, true) => "rep_roles.put",
            (Self::Rep, false) => "rep_roles.delete",
        }
    }

    const fn field(self) -> &'static str {
        match self {
            Self::Level => "level",
            Self::Rep => "reputation",
        }
    }

    /// `(role, threshold)` rows, lowest threshold first.
    async fn rows(self, state: &WebState, guild: u64) -> ApiResult<Vec<(u64, i32)>> {
        let db = &state.app.db;
        Ok(match self {
            Self::Level => db
                .level_roles(guild)
                .await?
                .into_iter()
                .map(|r| (r.role_id as u64, r.level))
                .collect(),
            Self::Rep => db
                .rep_roles(guild)
                .await?
                .into_iter()
                .map(|r| (r.role_id as u64, r.reputation))
                .collect(),
        })
    }

    async fn view(self, state: &WebState, guild: u64) -> ApiResult<Value> {
        let field = self.field();
        let items: Vec<Value> = self
            .rows(state, guild)
            .await?
            .into_iter()
            .map(|(role, n)| json!({ "role_id": role.to_string(), field: n }))
            .collect();
        Ok(match self {
            Self::Level => {
                let stack = state.app.db.guild_settings(guild).await?.level_roles_stack;
                json!({ "items": items, "max": MAX_LEVEL_ROLES, "stack": stack })
            }
            Self::Rep => json!({ "items": items, "max": MAX_REP_ROLES }),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelBody {
    level: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepBody {
    reputation: i64,
}

pub async fn level_roles(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(Rewards::Level.view(&state, managed.id).await?))
}

pub async fn rep_roles(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(Rewards::Rep.view(&state, managed.id).await?))
}

pub async fn put_level_role(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<LevelBody>,
) -> ApiResult<Json<Value>> {
    let level = settings::check_level(body.level)?;
    put_reward(state, managed, &params, Rewards::Level, level).await
}

pub async fn put_rep_role(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(body): ApiJson<RepBody>,
) -> ApiResult<Json<Value>> {
    let reputation = settings::check_reputation(body.reputation)?;
    put_reward(state, managed, &params, Rewards::Rep, reputation).await
}

async fn put_reward(
    state: WebState,
    managed: ManagedGuild,
    params: &HashMap<String, String>,
    list: Rewards,
    threshold: i32,
) -> ApiResult<Json<Value>> {
    let (guild, role) = (managed.id, path_id(params, "role")?);
    let info = guild_info::live(&state, guild).await?;
    let grantor = guild_info::grantor(&state, &managed, &info.roles, true).await?;
    info.check_grant("role_id", role, grantor)?;
    let was = list
        .rows(&state, guild)
        .await?
        .into_iter()
        .find(|(id, _)| *id == role)
        .map(|(_, n)| n);
    if was == Some(threshold) {
        return Ok(Json(list.view(&state, guild).await?));
    }
    let db = &state.app.db;
    let added = match list {
        Rewards::Level => db.add_level_role(guild, threshold, role).await?,
        Rewards::Rep => db.add_rep_role(guild, threshold, role).await?,
    };
    if !added {
        let (max, what) = match list {
            Rewards::Level => (MAX_LEVEL_ROLES, "level roles"),
            Rewards::Rep => (MAX_REP_ROLES, "reputation roles"),
        };
        return Err(ApiError::limit(format!(
            "A server can have {max} {what}. Remove one first."
        )));
    }
    let field = list.field();
    let target = role.to_string();
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: list.action(true),
            target: Some(&target),
            before: was.map_or(Value::Null, |n| json!({ field: n })),
            after: json!({ field: threshold }),
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(list.view(&state, guild).await?))
}

pub async fn delete_level_role(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    delete_reward(state, managed, &params, Rewards::Level).await
}

pub async fn delete_rep_role(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    delete_reward(state, managed, &params, Rewards::Rep).await
}

async fn delete_reward(
    state: WebState,
    managed: ManagedGuild,
    params: &HashMap<String, String>,
    list: Rewards,
) -> ApiResult<Json<Value>> {
    let (guild, role) = (managed.id, path_id(params, "role")?);
    let was = list
        .rows(&state, guild)
        .await?
        .into_iter()
        .find(|(id, _)| *id == role)
        .map(|(_, n)| n);
    let db = &state.app.db;
    let removed = match list {
        Rewards::Level => db.remove_level_role(guild, role).await?,
        Rewards::Rep => db.remove_rep_role(guild, role).await?,
    };
    if removed {
        let field = list.field();
        let target = role.to_string();
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: list.action(false),
                target: Some(&target),
                before: was.map_or(Value::Null, |n| json!({ field: n })),
                after: Value::Null,
            },
            Some(Invalidation::Guild(guild)),
        )
        .await;
    }
    Ok(Json(list.view(&state, guild).await?))
}

// /Sync job

/// How long a finished job can still be read.
const JOB_KEEP: Duration = Duration::from_secs(3600);
/// One sync per server at a time; past this a stuck one no longer blocks the next.
const SYNC_LOCK: Duration = Duration::from_secs(15 * 60);

/// A background job's state, as `GET /jobs/{id}` shows it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub guild_id: String,
    pub kind: String,
    /// `running`, `done` or `failed`.
    pub status: String,
    pub checked: usize,
    pub updated: usize,
    pub total: usize,
    /// The command's closing sentence once it is done.
    pub message: Option<String>,
}

fn job_key(id: &str) -> String {
    format!("web:job:{id}")
}

async fn save_job(state: &WebState, job: &Job) {
    let saved = match serde_json::to_string(job) {
        Ok(raw) => state.app.cache.put(&job_key(&job.id), &raw, JOB_KEEP).await,
        Err(error) => Err(error.into()),
    };
    if let Err(error) = saved {
        tracing::warn!(%error, job = %job.id, "job state not saved");
    }
}

/// `POST /level-roles/sync`: gives members who are already past a level the roles they earned, in
/// the background like `/config level-roles sync`. 202 with the job to poll.
pub async fn sync_level_roles(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Response> {
    crate::dashboard_access::check_level_rewards(&state, &managed).await?;
    let guild = managed.id;
    if state.app.db.level_roles(guild).await?.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid",
            "No level roles yet. Add one first.",
        ));
    }
    // A sync that ended a moment ago: no job to start, and no point polling for one.
    if let Some(left) = pinchora_commands::sync_resting(&state.app, guild).await?
        && let Some(message) = SyncOutcome::Resting(left).refusal()
    {
        return Err(ApiError::resting(message, left.as_secs()));
    }
    let lock = format!("web:sync:{guild}");
    if state.app.cache.try_cooldown(&lock, SYNC_LOCK).await?.is_some() {
        // The running job's id rides along, so the page can follow it instead.
        let running = state.app.cache.get(&format!("web:sync-job:{guild}")).await?;
        let mut response = ApiError::new(
            StatusCode::CONFLICT,
            "conflict",
            "A sync is already running for this server.",
        )
        .into_response();
        if let Some(id) = running.and_then(|id| id.parse().ok()) {
            response.headers_mut().insert("x-job-id", id);
        }
        return Ok(response);
    }
    let id = crate::auth::session::random_token(12);
    let mut job = Job {
        id: id.clone(),
        guild_id: guild.to_string(),
        kind: "level_roles.sync".into(),
        status: "running".into(),
        checked: 0,
        updated: 0,
        total: 0,
        message: None,
    };
    save_job(&state, &job).await;
    state
        .app
        .cache
        .put(&format!("web:sync-job:{guild}"), &id, SYNC_LOCK)
        .await?;

    let session = managed.current.session.clone();
    let task_state = state.clone();
    state.app.spawn(async move {
        let state = task_state;
        let (report_tx, mut report_rx) = watch::channel(pinchora_commands::SyncReport::default());
        let run = pinchora_commands::sync_level_roles(&state.app, guild, move |report| {
            report_tx.send_replace(report);
        });
        let mut run = std::pin::pin!(run);
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        let outcome = loop {
            tokio::select! {
                outcome = &mut run => break outcome,
                _ = tick.tick() => {
                    if report_rx.has_changed().unwrap_or(false) {
                        let report = *report_rx.borrow_and_update();
                        (job.checked, job.updated, job.total) = (report.checked, report.updated, report.total);
                        save_job(&state, &job).await;
                    }
                }
            }
        };
        match outcome {
            Ok(SyncOutcome::Done(report)) => {
                (job.checked, job.updated, job.total) = (report.checked, report.updated, report.total);
                job.status = "done".into();
                job.message = Some(report.summary());
                // Recorded before the job reads as finished, so a page that waits for it finds the row.
                audit::finish(
                    &state,
                    &session,
                    Change {
                        guild,
                        action: "level_roles.sync",
                        target: None,
                        before: Value::Null,
                        after: json!({ "checked": job.checked, "updated": job.updated }),
                    },
                    None,
                )
                .await;
            }
            // Started from Discord a moment before, or the level roles were removed meanwhile: nothing ran.
            Ok(other) => {
                job.status = "failed".into();
                job.message = Some(
                    other
                        .refusal()
                        .unwrap_or_else(|| "No level roles yet. Add one first.".to_owned()),
                );
            }
            Err(error) => {
                tracing::warn!(guild, error = ?error, "level role sync from the dashboard failed");
                job.status = "failed".into();
                job.message = Some("The sync stopped on an error. Try again in a moment.".into());
            }
        }
        save_job(&state, &job).await;
        if let Err(error) = state.app.cache.release_cooldown(&lock).await {
            tracing::warn!(%error, guild, "sync lock not released; it runs out by itself");
        }
    });
    Ok((StatusCode::ACCEPTED, Json(json!({ "job_id": id }))).into_response())
}

/// `GET /jobs/{id}`: a job of this server.
pub async fn job(
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
        .filter(|job: &Job| job.guild_id == managed.id.to_string())
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(job))
}
