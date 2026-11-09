//! Managed-guild Roblox settings and bind editor.
use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::routes::admin::settings::parse_id;
use crate::state::WebState;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, put},
};
use pinchora_commands::integrations::roblox::RobloxApi;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::integrations::roblox;
use pinchora_core::t;
use pinchora_db::RobloxSettings;
use serde::Deserialize;
use serde_json::{Value, json};

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(show))
        .route("/settings", put(save))
        .route("/binds", axum::routing::post(add))
        .route("/binds/{id}", axum::routing::delete(remove))
        .route("/groups/{id}/ranks", get(ranks))
}
fn settings_json(s: &RobloxSettings) -> Value {
    json!({"enabled":s.enabled,"verified_role_id":s.verified_role_id.map(|v|v.to_string()),"unverified_role_id":s.unverified_role_id.map(|v|v.to_string()),"nickname_template":s.nickname_template,"verify_channel_id":s.verify_channel_id.map(|v|v.to_string()),"require_verified":s.require_verified})
}
async fn view(state: &WebState, guild: u64) -> ApiResult<Value> {
    let s = state.app.db.roblox_settings(guild).await?;
    let binds = state.app.db.roblox_binds(guild).await?;
    let lang = state.app.guild_lang(guild).await;
    Ok(
        json!({"settings":settings_json(&s),"binds":binds.iter().map(|b|json!({"id":b.id.to_string(),"kind":b.kind,"group_id":b.group_id.map(|v|v.to_string()),"rank_min":b.rank_min,"rank_max":b.rank_max,"role_id":b.role_id.to_string(),"remove_if_unmet":b.remove_if_unmet})).collect::<Vec<_>>(),"strings":pinchora_core::infra::i18n::keys(lang).into_iter().filter_map(|key|key.strip_prefix("roblox.ui.").map(|short|(short.to_owned(),t!(lang,&key)))).collect::<std::collections::BTreeMap<_,_>>() }),
    )
}
async fn show(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    Ok(Json(view(&state, managed.id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsBody {
    enabled: bool,
    verified_role_id: Option<String>,
    unverified_role_id: Option<String>,
    nickname_template: Option<String>,
    verify_channel_id: Option<String>,
    require_verified: bool,
}
fn field(name: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::invalid(vec![FieldError::new(name, message)])
}
async fn save(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Json(body): Json<SettingsBody>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let id = |key: &'static str, raw: &Option<String>| -> ApiResult<Option<i64>> {
        raw.as_ref()
            .map(|v| {
                parse_id(v)
                    .and_then(|n| i64::try_from(n).ok())
                    .ok_or_else(|| field(key, t!(lang, "roblox.web.valid-id")))
            })
            .transpose()
    };
    let verified = id("verified_role_id", &body.verified_role_id)?;
    let unverified = id("unverified_role_id", &body.unverified_role_id)?;
    let channel = id("verify_channel_id", &body.verify_channel_id)?;
    if verified.is_some() && verified == unverified {
        return Err(field("unverified_role_id", t!(lang, "roblox.web.different-roles")));
    }
    if let Some(template) = &body.nickname_template
        && !roblox::valid_template(template)
    {
        return Err(field("nickname_template", t!(lang, "roblox.web.template")));
    }
    let info = guild_info::get(&state, managed.id, true).await?;
    let grantor = guild_info::grantor(&state, &managed, &info.roles, true).await?;
    for (key, role) in [("verified_role_id", verified), ("unverified_role_id", unverified)] {
        if let Some(role) = role
            && info.check_action_role(key, role as u64, grantor).is_err()
        {
            return Err(field(key, t!(lang, "roblox.web.safe-role")));
        }
    }
    if let Some(channel) = channel
        && !info
            .channels
            .iter()
            .any(|c| c.id == channel.to_string() && c.takes_posts())
    {
        return Err(field("verify_channel_id", t!(lang, "roblox.web.channel")));
    }
    let before = settings_json(&state.app.db.roblox_settings(managed.id).await?);
    let saved = RobloxSettings {
        enabled: body.enabled,
        verified_role_id: verified,
        unverified_role_id: unverified,
        nickname_template: body.nickname_template,
        verify_channel_id: channel,
        require_verified: body.require_verified,
    };
    state.app.db.roblox_save_settings(managed.id, &saved).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "roblox.settings.put",
            target: None,
            before,
            after: settings_json(&saved),
        },
        None,
    )
    .await;
    Ok(Json(view(&state, managed.id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindBody {
    kind: String,
    group_id: Option<String>,
    rank_min: Option<i16>,
    rank_max: Option<i16>,
    role_id: String,
    remove_if_unmet: bool,
}
async fn add(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Json(body): Json<BindBody>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let role = parse_id(&body.role_id)
        .filter(|n| i64::try_from(*n).is_ok())
        .ok_or_else(|| field("role_id", t!(lang, "roblox.web.valid-role")))?;
    let group = body
        .group_id
        .as_ref()
        .map(|s| {
            parse_id(s)
                .filter(|n| i64::try_from(*n).is_ok())
                .ok_or_else(|| field("group_id", t!(lang, "roblox.web.valid-group")))
        })
        .transpose()?;
    let valid = match body.kind.as_str() {
        "verified" => group.is_none() && body.rank_min.is_none() && body.rank_max.is_none(),
        "group_member" => group.is_some() && body.rank_min.is_none() && body.rank_max.is_none(),
        "group_rank" => {
            group.is_some()
                && body.rank_min.is_some_and(|v| (1..=255).contains(&v))
                && body
                    .rank_max
                    .is_some_and(|v| v >= body.rank_min.unwrap_or(256) && v <= 255)
        }
        _ => false,
    };
    if !valid {
        return Err(field("kind", t!(lang, "roblox.web.rank-range")));
    }
    let info = guild_info::get(&state, managed.id, true).await?;
    let grantor = guild_info::grantor(&state, &managed, &info.roles, true).await?;
    if info.check_action_role("role_id", role, grantor).is_err() {
        return Err(field("role_id", t!(lang, "roblox.web.safe-role")));
    }
    let id = state
        .app
        .db
        .roblox_add_bind(
            managed.id,
            &body.kind,
            group,
            body.rank_min,
            body.rank_max,
            role,
            body.remove_if_unmet,
        )
        .await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "roblox.bind.add",
            target: Some(&body.role_id),
            before: json!({}),
            after: json!({"id":id,"kind":body.kind,"group_id":group,"role_id":body.role_id}),
        },
        None,
    )
    .await;
    Ok(Json(view(&state, managed.id).await?))
}
async fn remove(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path((_, id)): Path<(String, i64)>,
) -> ApiResult<Json<Value>> {
    let bind = state
        .app
        .db
        .roblox_binds(managed.id)
        .await?
        .into_iter()
        .find(|b| b.id == id)
        .ok_or_else(ApiError::not_found)?;
    state.app.db.roblox_remove_bind(managed.id, id).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "roblox.bind.remove",
            target: Some(&id.to_string()),
            before: json!({"kind":bind.kind,"group_id":bind.group_id,"role_id":bind.role_id}),
            after: json!({}),
        },
        None,
    )
    .await;
    Ok(Json(view(&state, managed.id).await?))
}
async fn ranks(
    State(state): State<WebState>,
    _managed: ManagedGuild,
    Path((_, id)): Path<(String, u64)>,
) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(_managed.id).await;
    if id == 0 {
        return Err(field("group_id", t!(lang, "roblox.web.valid-group")));
    }
    let key = format!("ranks:{id}");
    let ranks = if let Some(value) = state.app.db.roblox_cache(&key).await? {
        value
    } else {
        let value = RobloxApi::with_cache(&state.app.cache)
            .ranks(id)
            .await
            .map_err(|_| ApiError::blocked(t!(lang, "roblox.web.group-failed")))?;
        state.app.db.roblox_cache_put(&key, &value).await?;
        value
    };
    Ok(Json(ranks))
}
