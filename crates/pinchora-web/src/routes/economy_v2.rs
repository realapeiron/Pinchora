//! D8 API: global player actions via Session, server controls via ManagedGuild.
use crate::{
    error::{ApiError, ApiJson, ApiResult},
    extract::{CurrentSession, ManagedGuild},
    state::WebState,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Query, State},
    routing::{get, post},
};
use pinchora_core::{economy_v2::EconomySettings, i18n::Lang, t};
use pinchora_db::{
    AuditSource,
    economy_v2::{Action, EconomyAudit, Rejected},
};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn player_router() -> Router<WebState> {
    Router::new()
        .route("/", get(snapshot))
        .route("/action", post(action))
        .route("/pool", get(pool))
        .layer(DefaultBodyLimit::max(16 * 1024))
}
pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(settings).put(save))
        .route("/points", get(preview).post(scale))
}
fn error(e: anyhow::Error, l: Lang) -> ApiError {
    if let Some(r) = e.downcast_ref::<Rejected>() {
        ApiError::blocked(t!(l, &format!("eco.error_{}", r.0)))
    } else {
        ApiError::internal(e)
    }
}
#[derive(Default, Deserialize)]
struct Locale {
    lang: Option<Lang>,
}
async fn snapshot(
    State(s): State<WebState>,
    c: CurrentSession,
    Query(_locale): Query<Locale>,
) -> ApiResult<Json<Value>> {
    let mut v = s.app.db.economy_snapshot(c.session.user_id, s.app.now()).await?;
    let l = _locale.lang.unwrap_or(super::web_player::lang(&s, &c.session).await);
    let inventory = v["inventory"].as_array().cloned().unwrap_or_default();
    v["inventory"] = json!(
        inventory
            .iter()
            .filter_map(|i| {
                let id = i[0].as_str()?;
                let item = pinchora_game::Content::get().item(id)?;
                (!item.bound).then(|| json!({"id":id,"name":item.name_in(l),"quantity":i[1]}))
            })
            .collect::<Vec<_>>()
    );
    Ok(Json(v))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionBody {
    request_id: String,
    command: Action,
}
async fn action(
    State(s): State<WebState>,
    mut c: CurrentSession,
    ApiJson(body): ApiJson<ActionBody>,
) -> ApiResult<Json<Value>> {
    let l = super::web_player::lang(&s, &c.session).await;
    if let Action::Donate { guild, .. } = &body.command {
        let guild = guild
            .parse::<u64>()
            .ok()
            .filter(|g| *g > 0 && *g <= i64::MAX as u64)
            .ok_or_else(|| ApiError::blocked(t!(l, "eco.error_invalid")))?;
        let list = crate::discord::user_guilds(&s, &mut c, false).await?;
        if !list.iter().any(|g| g.id() == Some(guild)) {
            return Err(ApiError::not_manager());
        }
        s.app
            .http
            .guild_member(
                twilight_model::id::Id::new(guild),
                twilight_model::id::Id::new(c.session.user_id),
            )
            .await
            .map_err(|_| ApiError::not_manager())?;
        let access = s
            .app
            .module_access(guild, None, pinchora_core::modules::Module::Economy)
            .await?;
        if !matches!(access, pinchora_core::modules::Access::Allowed) {
            return Err(ApiError::blocked(t!(l, "eco.error_forbidden")));
        }
    }
    let result = s
        .app
        .db
        .economy_action(c.session.user_id, &body.request_id, &body.command, s.app.now())
        .await
        .map_err(|e| error(e, l))?;
    super::web_player::finish(&s, &c.session, "economy.action", Value::Null, json!({"ok":true})).await;
    Ok(Json(result))
}
async fn settings(State(s): State<WebState>, m: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(s.app.db.economy_settings(m.id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveBody {
    settings: EconomySettings,
    revision: i64,
}
async fn save(State(s): State<WebState>, m: ManagedGuild, ApiJson(body): ApiJson<SaveBody>) -> ApiResult<Json<Value>> {
    let l = super::web_player::lang(&s, &m.current.session).await;
    s.app
        .db
        .save_economy_settings_audited(
            m.id,
            &body.settings,
            body.revision,
            Some(&EconomyAudit {
                actor: m.current.session.user_id,
                name: m.current.session.display_name(),
                source: AuditSource::Web,
            }),
        )
        .await
        .map_err(|e| error(e, l))?;
    Ok(Json(s.app.db.economy_settings(m.id).await?))
}
async fn preview(State(s): State<WebState>, m: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(s.app.db.economy_points_preview(m.id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScaleBody {
    percent: i32,
    version: String,
}
async fn scale(
    State(s): State<WebState>,
    m: ManagedGuild,
    ApiJson(body): ApiJson<ScaleBody>,
) -> ApiResult<Json<Value>> {
    let l = super::web_player::lang(&s, &m.current.session).await;
    s.app
        .db
        .economy_scale_points_audited(
            m.id,
            body.percent,
            &body.version,
            Some(&EconomyAudit {
                actor: m.current.session.user_id,
                name: m.current.session.display_name(),
                source: AuditSource::Web,
            }),
        )
        .await
        .map_err(|e| error(e, l))?;
    Ok(Json(s.app.db.economy_points_preview(m.id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PoolQuery {
    guild: u64,
}
async fn pool(State(s): State<WebState>, mut c: CurrentSession, Query(q): Query<PoolQuery>) -> ApiResult<Json<Value>> {
    if q.guild == 0 || q.guild > i64::MAX as u64 {
        return Err(ApiError::not_manager());
    }
    let guilds = crate::discord::user_guilds(&s, &mut c, false).await?;
    if !guilds.iter().any(|g| g.id() == Some(q.guild)) {
        return Err(ApiError::not_manager());
    }
    s.app
        .http
        .guild_member(
            twilight_model::id::Id::new(q.guild),
            twilight_model::id::Id::new(c.session.user_id),
        )
        .await
        .map_err(|_| ApiError::not_manager())?;
    let v = s.app.db.economy_settings(q.guild).await?;
    Ok(Json(json!({"pool_total":v["pool_total"],"pool_goal":v["pool_goal"]})))
}
