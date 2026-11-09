//! Manage Server-only grants; a delegate can never delegate further.
use crate::{
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info,
    state::WebState,
};
use axum::{Json, Router, extract::State, routing::get};
use pinchora_core::t;
use pinchora_db::{DASHBOARD_AREAS, DashboardManager, MAX_DASHBOARD_MANAGERS};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

pub fn router() -> Router<WebState> {
    Router::new().route("/", get(read).put(save))
}

fn view(items: &[DashboardManager]) -> Value {
    json!({"items": items.iter().map(|g| json!({"role_id": g.role_id.to_string(), "areas": g.areas})).collect::<Vec<_>>(),
        "areas": DASHBOARD_AREAS, "max": MAX_DASHBOARD_MANAGERS})
}

async fn read(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    if !managed.guild.can_manage() {
        return Err(ApiError::not_manager());
    }
    Ok(Json(view(&state.app.db.dashboard_managers(managed.id).await?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    role_id: String,
    areas: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    items: Vec<Grant>,
}

async fn save(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<Body>,
) -> ApiResult<Json<Value>> {
    if !managed.guild.can_manage() {
        return Err(ApiError::not_manager());
    }
    let lang = state
        .app
        .user_lang_choice(managed.current.session.user_id)
        .await
        .unwrap_or(state.app.guild_lang(managed.id).await);
    let invalid = || ApiError::malformed(t!(lang, "server_tools.access-invalid"));
    if body.items.len() > MAX_DASHBOARD_MANAGERS {
        return Err(invalid());
    }
    let info = guild_info::live(&state, managed.id).await?;
    let mut seen = HashSet::new();
    let mut grants = Vec::new();
    for item in body.items {
        let role: i64 = item.role_id.parse().ok().filter(|r| *r > 0).ok_or_else(invalid)?;
        let valid_role = info
            .roles
            .iter()
            .any(|r| r.id == role.to_string() && !r.everyone && !r.managed);
        if !valid_role
            || !seen.insert(role)
            || item.areas.is_empty()
            || item.areas.len() > DASHBOARD_AREAS.len()
            || item.areas.iter().any(|a| !DASHBOARD_AREAS.contains(&a.as_str()))
        {
            return Err(invalid());
        }
        grants.push(DashboardManager {
            role_id: role,
            areas: item.areas,
        });
    }
    state
        .app
        .db
        .replace_dashboard_managers(
            managed.id,
            managed.current.session.user_id,
            managed.current.session.display_name(),
            &grants,
        )
        .await?;
    Ok(Json(view(&state.app.db.dashboard_managers(managed.id).await?)))
}
