//! Command editor, guarded by the same Manage Server extractor as other editors.
use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::state::WebState;
use axum::{Json, Router, extract::State, routing::get};
use pinchora_core::admin::command_control::Config;
use pinchora_core::t;
use serde_json::{Value, json};
use twilight_model::id::Id;
#[path = "command_control_words.rs"]
pub(super) mod words;
pub fn router() -> Router<WebState> {
    Router::new().route("/", get(show).put(save))
}
async fn view(state: &WebState, managed: &ManagedGuild) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let mut commands = pinchora_commands::control_catalog();
    for command in &mut commands {
        let key = format!("cmd.{}", command.path);
        if pinchora_core::infra::i18n::exact(lang, &key).is_some() {
            command.description = t!(lang, &key);
        }
    }
    let permissions = match state
        .app
        .interaction()
        .guild_command_permissions(Id::new(managed.id))
        .await
    {
        Ok(response) => response
            .models()
            .await
            .ok()
            .and_then(|rows| serde_json::to_value(rows).ok()),
        Err(_) => None,
    };
    let mut permission_names = std::collections::BTreeMap::new();
    permission_names.insert(
        state.app.application_id.to_string(),
        t!(lang, "command_control.ui.application"),
    );
    if permissions
        .as_ref()
        .and_then(Value::as_array)
        .is_some_and(|rows| !rows.is_empty())
        && let Ok(response) = state.app.interaction().global_commands().await
        && let Ok(commands) = response.models().await
    {
        for command in commands {
            if let Some(id) = command.id {
                permission_names.insert(id.to_string(), format!("/{}", command.name));
            }
        }
    }
    Ok(Json(
        json!({"config":state.app.db.command_control(managed.id).await?,"commands":commands,"permissions":permissions,"permission_names":permission_names,"strings":words::strings(lang,"command_control")}),
    ))
}
async fn show(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    view(&state, &managed).await
}
async fn save(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(config): ApiJson<Config>,
) -> ApiResult<Json<Value>> {
    let lang = managed.panel_lang;
    let before = state.app.db.command_control(managed.id).await?;
    config
        .validate_update(
            &before,
            &pinchora_commands::control_validation_catalog(),
            &pinchora_commands::command_reserved_aliases(),
            lang,
        )
        .map_err(ApiError::blocked)?;
    let info = guild_info::get(&state, managed.id, false).await?;
    for rule in config
        .rules
        .values()
        .flat_map(|r| std::iter::once(r).chain(r.constraints.values()))
    {
        for role in rule.allowed_roles.iter().chain(&rule.denied_roles) {
            info.check_role(
                "roles",
                role.parse()
                    .map_err(|_| ApiError::blocked(t!(lang, "command_control.invalid")))?,
            )?;
        }
        for channel in rule.allowed_channels.iter().chain(&rule.denied_channels) {
            info.check_channel(
                "channels",
                channel
                    .parse()
                    .map_err(|_| ApiError::blocked(t!(lang, "command_control.invalid")))?,
            )?;
        }
    }
    state.app.db.set_command_control(managed.id, &config).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild: managed.id,
            action: "command_control.set",
            target: None,
            before: json!(before),
            after: json!(config),
        },
        Some(pinchora_cache::Invalidation::Guild(managed.id)),
    )
    .await;
    view(&state, &managed).await
}
