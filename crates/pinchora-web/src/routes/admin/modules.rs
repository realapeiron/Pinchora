//! Module switches: which parts of Pinchora a server uses, and where. Same rules and the same write
//! as `/config modules`.

use axum::{
    Json,
    extract::{Path, State},
};
use pinchora_cache::Invalidation;
use pinchora_core::admin::modules::ChannelMode;
use pinchora_core::admin::modules::Group;
use pinchora_core::admin::modules::MAX_CHANNELS;
use pinchora_core::admin::modules::Module;
use pinchora_core::admin::modules::ModuleRule;
use pinchora_core::admin::modules::Scope;
use pinchora_core::admin::modules::check_channels;
use pinchora_core::t;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::routes::admin::settings::parse_id;
use crate::state::WebState;

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
    expect_enabled: Option<bool>,
    channel_mode: Option<String>,
    channels: Option<Vec<String>>,
}

async fn authorize_module(state: &WebState, managed: &ManagedGuild, module: Module) -> ApiResult<()> {
    let area = match module {
        Module::CommandControl => None,
        Module::Moderation
        | Module::AutoMod
        | Module::Verification
        | Module::AntiRaid
        | Module::AntiNuke
        | Module::Log => Some("safety"),
        Module::RolePanels | Module::AutoRoles | Module::ColorRoles => Some("roles"),
        Module::Tickets
        | Module::TempVoice
        | Module::Starboard
        | Module::Birthdays
        | Module::Events
        | Module::Suggestions
        | Module::Polls
        | Module::Giveaways
        | Module::Achievements
        | Module::Utility
        | Module::Qotd
        | Module::Counting
        | Module::Forms
        | Module::Modmail => Some("community"),
        Module::Feeds | Module::Roblox | Module::GameTrackers => Some("feeds"),
        Module::Stats | Module::Invites => Some("stats"),
        Module::Economy | Module::Shop => Some("economy"),
        Module::Leveling
        | Module::Points
        | Module::Welcome
        | Module::Goodbye
        | Module::Store
        | Module::CustomCommands
        | Module::Profile
        | Module::Quests
        | Module::Social
        | Module::House
        | Module::Plaza
        | Module::Pets
        | Module::Fishing
        | Module::Duels
        | Module::BoardGames
        | Module::Puzzles
        | Module::Party
        | Module::PartyGames
        | Module::Collection
        | Module::Gambling
        | Module::TextCommands
        | Module::Info
        | Module::Branding
        | Module::Workflows => Some("configuration"),
    };
    if let Some(areas) = &managed.areas
        && !area.is_some_and(|area| areas.iter().any(|held| held == area))
    {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "not_manager",
            t!(state.app.guild_lang(managed.id).await, "modules.access"),
        ));
    }
    if module == Module::AntiNuke {
        pinchora_commands::authorize_antinuke_module(&state.app, managed.id, managed.current.session.user_id)
            .await
            .map_err(|e| ApiError::blocked(e.to_string()))?;
    }
    Ok(())
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
    authorize_module(&state, &managed, module).await?;
    let db = &state.app.db;
    let lang = managed.panel_lang;
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
                ApiError::invalid(vec![pinchora_core::admin::settings::FieldError::new(
                    "channel_mode",
                    "Pick all channels, only some, or all but some.",
                )])
            })?,
            None => before.mode,
        };
        let list: Vec<u64> = match &body.channels {
            Some(raw) => raw
                .iter()
                .map(|id| parse_id(id).ok_or_else(|| ApiError::malformed(t!(lang, "common.web.errors.channel-ids"))))
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
    if enabled.is_none() && channels.is_none() && body.expect_enabled.is_none() {
        return Ok(Json(module_json(module, &before)));
    }
    if enabled == Some(true) || before.enabled && channels.is_some() {
        crate::dashboard_access::check_module_roles(&state, &managed, module).await?;
    }
    if module == Module::AntiNuke && enabled == Some(true) && !before.enabled {
        state
            .app
            .cache
            .antinuke_activation(guild, state.app.now().timestamp_millis(), true)
            .await?;
    }
    let after = db
        .set_module_if(
            guild,
            module,
            enabled,
            channels.as_ref().map(|(mode, list)| (*mode, list.as_slice())),
            body.expect_enabled,
        )
        .await
        .map_err(|error| match error.downcast_ref::<pinchora_db::ModuleConflict>() {
            Some(_) => ApiError::conflict(t!(lang, "modules.conflict")),
            None => error.into(),
        })?;
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
    use crate::routes::admin::modules::*;

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
