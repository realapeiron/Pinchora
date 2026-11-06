//! Module switches: which parts of Pinchora a server uses, and where. Same rules and the same write
//! as `/config modules`.

use axum::{
    Json,
    extract::{Path, State},
};
use pinchora_cache::Invalidation;
use pinchora_core::modules::{ChannelMode, Group, MAX_CHANNELS, Module, ModuleRule, Scope, check_channels};
use serde::Deserialize;
use serde_json::{Value, json};

use super::settings::parse_id;
use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info,
    state::WebState,
};

/// One module as the dashboard shows it: what it is, and how this server has it.
pub fn module_json(module: Module, rule: &ModuleRule) -> Value {
    json!({
        "key": module.key(),
        "label": module.label(),
        "blurb": module.blurb(),
        "group": module.group().key(),
        "server_only": module.scope() == Scope::Server,
        "enabled": rule.enabled,
        "channel_mode": rule.mode.key(),
        "channels": rule.channels.iter().map(u64::to_string).collect::<Vec<_>>(),
    })
}

/// Every module of the server, in the order the dashboard lists them.
pub async fn module_list(state: &WebState, guild: u64) -> ApiResult<Vec<Value>> {
    Ok(state
        .app
        .db
        .module_states(guild)
        .await?
        .iter()
        .map(|(module, rule)| module_json(*module, rule))
        .collect())
}

/// `GET /modules`.
pub async fn modules(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let groups: Vec<Value> = Group::ALL
        .iter()
        .map(|g| json!({ "key": g.key(), "label": g.label() }))
        .collect();
    Ok(Json(json!({
        "items": module_list(&state, managed.id).await?,
        "groups": groups,
        "max_channels": MAX_CHANNELS,
    })))
}

/// What `PUT /modules/{module}` may carry: any subset. A mode alone keeps the channels it had when
/// the mode is the same, and wants new ones otherwise.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleBody {
    enabled: Option<bool>,
    channel_mode: Option<String>,
    channels: Option<Vec<String>>,
}

/// `PUT /modules/{module}`: switch it, and say where it works.
pub async fn put_module(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Path(params): Path<std::collections::HashMap<String, String>>,
    ApiJson(body): ApiJson<ModuleBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let module = params
        .get("module")
        .and_then(|key| Module::parse(key))
        .ok_or_else(ApiError::not_found)?;
    let db = &state.app.db;
    let before = db
        .module_states(guild)
        .await?
        .into_iter()
        .find(|(m, _)| *m == module)
        .map(|(_, rule)| rule)
        .unwrap_or_default();

    // Where it works: only when the request says something about it.
    let channels = if body.channel_mode.is_some() || body.channels.is_some() {
        let mode = match body.channel_mode.as_deref() {
            Some(key) => ChannelMode::parse(key).ok_or_else(|| {
                ApiError::invalid(vec![pinchora_core::settings::FieldError::new(
                    "channel_mode",
                    "Pick all channels, only some, or all but some.",
                )])
            })?,
            None => before.mode,
        };
        let list: Vec<u64> = match &body.channels {
            Some(raw) => raw
                .iter()
                .map(|id| parse_id(id).ok_or_else(|| ApiError::malformed("Channel ids are numbers.")))
                .collect::<ApiResult<_>>()?,
            None if mode == before.mode => before.channels.clone(),
            None => Vec::new(),
        };
        let list = check_channels(module, mode, &list)?;
        // Channels already on the list stay even if Discord no longer has them; new ones must exist.
        let added: Vec<u64> = list
            .iter()
            .copied()
            .filter(|id| !before.channels.contains(id))
            .collect();
        if !added.is_empty() {
            let info =
                guild_info::get_with(&state, guild, |info| added.iter().all(|id| info.channel(*id).is_some())).await?;
            for id in &added {
                info.check_channel("channels", *id)?;
            }
        }
        Some((mode, list))
    } else {
        None
    };

    let enabled = body.enabled.filter(|&on| on != before.enabled);
    let channels = channels.filter(|(mode, list)| *mode != before.mode || *list != before.channels);
    if enabled.is_none() && channels.is_none() {
        return Ok(Json(module_json(module, &before)));
    }
    let after = db
        .set_module(
            guild,
            module,
            enabled,
            channels.as_ref().map(|(mode, list)| (*mode, list.as_slice())),
        )
        .await?;
    let (old, new) = pinchora_commands::module_diff(&before, &after);
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "module.set",
            target: Some(module.key()),
            before: old,
            after: new,
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(module_json(module, &after)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_module_is_shown_with_its_words_and_ids_as_strings() {
        let rule = ModuleRule {
            enabled: true,
            mode: ChannelMode::Only,
            channels: vec![12_345_678_901_234_567],
        };
        let shown = module_json(Module::Pets, &rule);
        assert_eq!(shown["key"], "pets");
        assert_eq!(shown["group"], "home");
        assert_eq!(shown["channel_mode"], "only");
        assert_eq!(shown["channels"], json!(["12345678901234567"]));
        assert_eq!(shown["server_only"], false);
        assert_eq!(
            module_json(Module::Welcome, &ModuleRule::default())["server_only"],
            true
        );
    }

    #[test]
    fn an_unknown_field_is_refused() {
        assert!(serde_json::from_value::<ModuleBody>(json!({ "enabled": true, "colour": 1 })).is_err());
        let body: ModuleBody = serde_json::from_value(json!({ "channels": ["1", "2"] })).unwrap();
        assert_eq!(body.channels.unwrap().len(), 2);
    }
}
