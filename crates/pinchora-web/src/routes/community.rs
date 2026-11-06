//! Suggestions and critter drops: where `/suggest` posts, and whether chat makes critters wash up.
//! Same rules as `/config suggestions` and `/config critters`.

use axum::{Json, extract::State};
use pinchora_cache::Invalidation;
use pinchora_db::CollectionSettings;
use serde::Deserialize;
use serde_json::{Value, json};

use super::settings::{parse_id, present};
use crate::{
    audit::{self, Change},
    error::{ApiError, ApiJson, ApiResult},
    extract::ManagedGuild,
    guild_info,
    state::WebState,
};

fn critters_json(c: &CollectionSettings) -> Value {
    json!({ "enabled": c.enabled, "channel_id": c.channel_id.map(|id| id.to_string()) })
}

fn suggestions_json(channel: Option<u64>) -> Value {
    json!({ "channel_id": channel.map(|id| id.to_string()) })
}

/// `GET /community`.
pub async fn community(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let db = &state.app.db;
    let suggestions = db.suggestion_channel(managed.id).await?;
    let critters = db.collection_settings(managed.id).await?;
    Ok(Json(json!({
        "suggestions": suggestions_json(suggestions),
        "critters": critters_json(&critters),
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelBody {
    /// `null` turns it off (suggestions) or lets drops land anywhere (critters).
    #[serde(default, deserialize_with = "present")]
    channel_id: Option<Option<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrittersBody {
    enabled: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    channel_id: Option<Option<String>>,
}

/// A channel field: `None` absent, `Some(None)` cleared, or an id the bot can post in.
async fn channel_field(state: &WebState, guild: u64, raw: Option<Option<String>>) -> ApiResult<Option<Option<u64>>> {
    let Some(raw) = raw else { return Ok(None) };
    let Some(raw) = raw else { return Ok(Some(None)) };
    let id = parse_id(&raw).ok_or_else(|| ApiError::malformed("Channel ids are numbers."))?;
    let info = guild_info::get_with(state, guild, |info| info.channel(id).is_some()).await?;
    info.check_post_channel("channel_id", id)?;
    Ok(Some(Some(id)))
}

/// `PUT /suggestions/settings`: the suggestion channel, or `null` for off.
pub async fn put_suggestions(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<ChannelBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let db = &state.app.db;
    let was = db.suggestion_channel(guild).await?;
    let channel = channel_field(&state, guild, body.channel_id).await?.unwrap_or(was);
    if channel != was {
        db.set_suggestion_channel(guild, channel).await?;
        audit::finish(
            &state,
            &managed.current.session,
            Change {
                guild,
                action: "suggestions.set",
                target: None,
                before: suggestions_json(was),
                after: suggestions_json(channel),
            },
            Some(Invalidation::Guild(guild)),
        )
        .await;
    }
    Ok(Json(suggestions_json(channel)))
}

/// `PUT /critters/settings`: drops on or off, and the channel they land in (`null`: anywhere busy).
pub async fn put_critters(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<CrittersBody>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let db = &state.app.db;
    let was = db.collection_settings(guild).await?;
    let channel = channel_field(&state, guild, body.channel_id)
        .await?
        .filter(|c| c.map(|id| id as i64) != was.channel_id);
    let enabled = body.enabled.filter(|&e| e != was.enabled);
    if enabled.is_none() && channel.is_none() {
        return Ok(Json(critters_json(&was)));
    }
    let now = db.set_collection_settings(guild, enabled, channel).await?;
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "critters.set",
            target: None,
            before: critters_json(&was),
            after: critters_json(&now),
        },
        // The bot keeps these a minute per pod; the guild announcement drops them too.
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(critters_json(&now)))
}
