//! `PATCH /api/guilds/{g}/settings`: change server settings. Writes, records the change, tells the bot.

use axum::{Json, extract::State};
use pinchora_cache::Invalidation;
use serde::Deserialize;
use serde_json::{Map, Value};

use super::guilds::SettingsView;
use crate::{
    audit::{self, Change},
    error::{ApiJson, ApiResult},
    extract::ManagedGuild,
    invalidate,
    state::WebState,
};

/// The settings the dashboard can change so far; absent fields stay as they are.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsPatch {
    pub leveling_enabled: Option<bool>,
}

pub async fn patch(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(patch): ApiJson<SettingsPatch>,
) -> ApiResult<Json<SettingsView>> {
    let db = &state.app.db;
    let guild = managed.id;
    let current = db.guild_settings(guild).await?;
    let (mut before, mut after) = (Map::new(), Map::new());
    if let Some(enabled) = patch.leveling_enabled.filter(|&e| e != current.leveling_enabled) {
        before.insert("leveling_enabled".into(), current.leveling_enabled.into());
        after.insert("leveling_enabled".into(), enabled.into());
    }
    if after.is_empty() {
        return Ok(Json(current.into()));
    }
    let updated = db.update_leveling(guild, patch.leveling_enabled, None, None).await?;
    audit::record(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "settings.patch",
            target: None,
            before: Value::Object(before),
            after: Value::Object(after),
        },
    )
    .await?;
    invalidate::publish(&state, Invalidation::Guild(guild)).await;
    tracing::info!(
        guild,
        user_id = managed.current.session.user_id,
        "settings changed on the web"
    );
    Ok(Json(updated.into()))
}
