//! Staff queues require ManagedGuild; banned members can appeal through their own CSRF-checked session.
use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::CurrentSession;
use crate::extract::ManagedGuild;
use crate::state::WebState;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use pinchora_core::moderation::policy::Config;
use pinchora_core::t;
use serde::Deserialize;
use serde_json::{Value, json};
pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(show).put(save))
        .route("/preview", post(preview))
        .route("/{queue}/{id}", post(review))
}
pub fn member_router() -> Router<WebState> {
    Router::new()
        .route("/", get(member_cases))
        .route("/export", get(export))
        .route("/{guild}/{case}", get(member).post(submit))
}
async fn show(State(s): State<WebState>, m: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"config":s.app.db.moderation_policy_config(m.id).await?,"queues":s.app.db.moderation_policy_queues(m.id).await?}),
    ))
}
async fn save(State(s): State<WebState>, m: ManagedGuild, ApiJson(c): ApiJson<Config>) -> ApiResult<Json<Value>> {
    let lang = s.app.guild_lang(m.id).await;
    pinchora_commands::check_moderation_policy_config(&s.app, m.id, m.current.session.user_id, &c)
        .await
        .map_err(|_| ApiError::blocked(t!(lang, "parity.invalid")))?;
    let info = crate::guild_info::get(&s, m.id, false).await?;
    if let Some(channel) = c
        .report_channel
        .as_deref()
        .and_then(pinchora_core::moderation::policy::id)
    {
        info.check_post_channel("report_channel", channel)?;
    }
    for r in &c.autopurge {
        info.check_post_channel("autopurge", pinchora_core::moderation::policy::id(&r.channel).unwrap())?;
    }
    let before = s.app.db.moderation_policy_config(m.id).await?;
    s.app.db.save_moderation_policy_config(m.id, &c).await?;
    audit::finish(
        &s,
        &m.current.session,
        Change {
            guild: m.id,
            action: "moderation.policy",
            target: None,
            before: json!(before),
            after: json!(c),
        },
        None,
    )
    .await;
    show(State(s), m).await
}
async fn preview(State(s): State<WebState>, m: ManagedGuild, ApiJson(c): ApiJson<Config>) -> ApiResult<Json<Value>> {
    let lang = s.app.guild_lang(m.id).await;
    pinchora_commands::check_moderation_policy_config(&s.app, m.id, m.current.session.user_id, &c)
        .await
        .map_err(|_| ApiError::blocked(t!(lang, "parity.invalid")))?;
    Ok(Json(json!({"valid":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    accept: bool,
    reason: String,
}
async fn review(
    State(s): State<WebState>,
    m: ManagedGuild,
    Path((_, queue, id)): Path<(String, String, i64)>,
    ApiJson(d): ApiJson<Decision>,
) -> ApiResult<Json<Value>> {
    let lang = s.app.guild_lang(m.id).await;
    pinchora_commands::review_moderation_policy(
        &s.app,
        m.id,
        (m.current.session.user_id, &m.current.session.username),
        &queue,
        id,
        d.accept,
        &d.reason,
    )
    .await
    .map_err(|_| ApiError::blocked(t!(lang, "parity.refused")))?;
    audit::finish(
        &s,
        &m.current.session,
        Change {
            guild: m.id,
            action: "moderation.review",
            target: Some(&id.to_string()),
            before: json!(null),
            after: json!({"queue":queue,"accepted":d.accept,"reason":d.reason}),
        },
        None,
    )
    .await;
    show(State(s), m).await
}
async fn member(
    State(s): State<WebState>,
    current: CurrentSession,
    Path((g, n)): Path<(u64, i32)>,
) -> ApiResult<Json<Value>> {
    let c = s.app.db.moderation_policy_config(g).await?;
    let case = s
        .app
        .db
        .case(g, n)
        .await?
        .filter(|c| c.target_id as u64 == current.session.user_id && matches!(c.kind.as_str(), "ban" | "warn"))
        .ok_or_else(ApiError::not_found)?;
    if !c.appeals {
        return Err(ApiError::not_found());
    }
    let appeal = s.app.db.member_mod_appeal(g, n, current.session.user_id).await?;
    Ok(Json(
        json!({"case":n,"kind":case.kind,"reason":case.reason,"active":case.active,"appeal":appeal}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Appeal {
    body: String,
}
async fn submit(
    State(s): State<WebState>,
    current: CurrentSession,
    Path((g, n)): Path<(u64, i32)>,
    ApiJson(d): ApiJson<Appeal>,
) -> ApiResult<Json<Value>> {
    let lang = s.app.guild_lang(g).await;
    if !s.app.db.moderation_policy_config(g).await?.appeals || !(10..=2000).contains(&d.body.trim().chars().count()) {
        return Err(ApiError::blocked(t!(lang, "parity.invalid")));
    }
    if !s
        .app
        .db
        .submit_mod_appeal(g, current.session.user_id, n, d.body.trim())
        .await?
    {
        return Err(ApiError::blocked(t!(lang, "parity.refused")));
    }
    Ok(Json(json!({"submitted":true})))
}

async fn member_cases(State(s): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"cases":s.app.db.member_appealable_cases(current.session.user_id).await?}),
    ))
}

async fn export(State(s): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    Ok(Json(
        s.app.db.export_moderation_policy_data(current.session.user_id).await?,
    ))
}
