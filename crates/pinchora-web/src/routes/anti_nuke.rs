//! Anti-nuke settings and recovery; ManagedGuild plus fresh owner/trust checks on every write.
use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info,
    state::WebState,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use pinchora_core::{
    anti_nuke::{self as rules, Config},
    t,
};
use serde::Deserialize;
use serde_json::{Value, json};
#[path = "anti_nuke_words.rs"]
mod words;
pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(show).put(save))
        .route("/preview", post(preview))
        .route("/events/{event}/restore", post(restore))
}
async fn view(state: &WebState, managed: &ManagedGuild) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let lang = state.app.guild_lang(guild).await;
    let c = state.app.db.antinuke_config(guild).await?;
    let owner = state
        .app
        .http
        .guild(twilight_model::id::Id::new(guild))
        .await
        .map_err(|e| ApiError::blocked(e.to_string()))?
        .model()
        .await
        .map_err(|e| ApiError::blocked(e.to_string()))?
        .owner_id
        .get();
    let actor = managed.current.session.user_id;
    let events=state.app.db.antinuke_events(guild).await?.into_iter().map(|e|json!({"id":e.id.to_string(),"actor_id":e.actor_id.to_string(),"kind":e.kind,"count":e.count,"action":if e.action_taken=="observed"{t!(lang,"anti_nuke.observed")}else{e.action_taken},"created_at":e.created_at,"reverted":e.reverted,"recovery_state":e.recovery_state,"restorable":matches!(e.kind.as_str(),"channel_delete"|"role_delete"|"ban"|"vip_ban")&&e.recovery_state=="ready"&&!e.reverted})).collect::<Vec<_>>();
    Ok(Json(
        json!({"config":c,"events":events,"strings":words::strings(lang),"kinds":rules::KINDS,"responses":rules::RESPONSES,"max_list":rules::MAX_LIST,"can_edit":rules::may_configure(&c,owner,actor),"can_trust":owner==actor,"can_restore":owner==actor||c.trusted_users.contains(&actor.to_string()),"viewer":actor.to_string()}),
    ))
}
async fn show(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    view(&state, &managed).await
}
async fn save(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(c): ApiJson<Config>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let lang = state.app.guild_lang(guild).await;
    rules::validate(&c, lang).map_err(ApiError::blocked)?;
    if let Some(id) = &c.log_channel_id {
        guild_info::get_with(&state, guild, |_| true)
            .await?
            .check_post_channel(
                "log_channel_id",
                id.parse()
                    .map_err(|_| ApiError::blocked(t!(lang, "anti_nuke.invalid")))?,
            )?;
    }
    let before = state.app.db.antinuke_config(guild).await?;
    pinchora_commands::save_antinuke_config(&state.app, guild, managed.current.session.user_id, &c)
        .await
        .map_err(|e| ApiError::blocked(e.to_string()))?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "antinuke.set",
            target: None,
            before: json!(before),
            after: json!(c),
        },
        None,
    )
    .await;
    view(&state, &managed).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preview {
    config: Config,
    kind: String,
    count: u32,
}
async fn preview(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<Preview>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    rules::validate(&body.config, lang).map_err(ApiError::blocked)?;
    let limit = body
        .config
        .limits
        .get(&body.kind)
        .ok_or_else(|| ApiError::blocked(t!(lang, "anti_nuke.invalid")))?;
    let result = if !body.config.enabled {
        t!(lang, "anti_nuke.off")
    } else if body.count > limit.count || body.kind.starts_with("vip_") {
        t!(lang, &format!("anti_nuke.response.{}", body.config.response))
    } else {
        t!(lang, "anti_nuke.observed")
    };
    Ok(Json(
        json!({"message":t!(lang,"anti_nuke.ui.preview_result",count=body.count,result=result)}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Restore {
    confirm: bool,
}
async fn restore(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<std::collections::HashMap<String, String>>,
    ApiJson(body): ApiJson<Restore>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let id = params
        .get("event")
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| ApiError::blocked(t!(lang, "anti_nuke.invalid")))?;
    if !body.confirm {
        return Err(ApiError::blocked(t!(lang, "anti_nuke.ui.confirm_hint")));
    }
    let message =
        pinchora_commands::restore_antinuke(&state.app, managed.id, id, managed.current.session.user_id, false)
            .await
            .map_err(|e| ApiError::blocked(e.to_string()))?;
    Ok(Json(json!({"message":message})))
}
