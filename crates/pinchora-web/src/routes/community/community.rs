//! Suggestions, critter drops and storm surges: where `/suggest` posts, whether chat makes critters
//! wash up, and the timed storms. Same rules as `/config suggestions`, `/config critters` and
//! `/config surge`.

use axum::{Json, extract::State};
use pinchora_cache::Invalidation;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::t;
use pinchora_db::{CollectionSettings, PartySettings};
use pinchora_game::party::surge::EVERY_CHOICES;
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
use crate::routes::admin::settings::present;
use crate::state::WebState;

fn critters_json(c: &CollectionSettings) -> Value {
    json!({ "enabled": c.enabled, "channel_id": c.channel_id.map(|id| id.to_string()) })
}

fn suggestions_json(channel: Option<u64>) -> Value {
    json!({ "channel_id": channel.map(|id| id.to_string()) })
}

fn surge_json(s: &PartySettings) -> Value {
    json!({
        "enabled": s.surge_enabled,
        "channel_id": s.surge_channel_id.map(|id| id.to_string()),
        "every_hours": s.surge_every_hours,
        "every_choices": EVERY_CHOICES,
        "next_at": s.surge_next_at,
    })
}

/// `GET /community`.
pub async fn community(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let db = &state.app.db;
    let suggestions = db.suggestion_channel(managed.id).await?;
    let critters = db.collection_settings(managed.id).await?;
    let surge = db.party_settings(managed.id).await?;
    Ok(Json(json!({
        "suggestions": suggestions_json(suggestions),
        "critters": critters_json(&critters),
        "surge": surge_json(&surge),
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
async fn channel_field(
    state: &WebState,
    guild: u64,
    raw: Option<Option<String>>,
    lang: pinchora_core::infra::i18n::Lang,
) -> ApiResult<Option<Option<u64>>> {
    let Some(raw) = raw else { return Ok(None) };
    let Some(raw) = raw else { return Ok(Some(None)) };
    let id = parse_id(&raw).ok_or_else(|| ApiError::malformed(t!(lang, "common.web.errors.channel-ids")))?;
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
    let channel = channel_field(&state, guild, body.channel_id, managed.panel_lang)
        .await?
        .unwrap_or(was);
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
    let channel = channel_field(&state, guild, body.channel_id, managed.panel_lang)
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurgeBody {
    enabled: Option<bool>,
    channel_id: Option<String>,
    every_hours: Option<i64>,
}

/// `PUT /surge/settings`: timed storms on or off, the channel they come to, and how often. Like
/// `/config surge`, saving schedules the next storm from now, so nothing is written when nothing changed.
pub async fn put_surge(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<SurgeBody>,
) -> ApiResult<Json<Value>> {
    let lang = managed.panel_lang;
    let guild = managed.id;
    let db = &state.app.db;
    let was = db.party_settings(guild).await?;
    let hours = match body.every_hours {
        Some(h) => {
            let h = i32::try_from(h)
                .ok()
                .filter(|h| EVERY_CHOICES.contains(h))
                .ok_or_else(|| {
                    ApiError::invalid(vec![FieldError::new(
                        "every_hours",
                        "Storms come every 6, 12 or 24 hours, every 2 days or every week.",
                    )])
                })?;
            Some(h).filter(|h| *h != was.surge_every_hours)
        }
        None => None,
    };
    let channel = match body.channel_id.as_deref() {
        Some(raw) => {
            let id = parse_id(raw).ok_or_else(|| ApiError::malformed(t!(lang, "common.web.errors.channel-ids")))?;
            let info = guild_info::get_with(&state, guild, |info| info.channel(id).is_some()).await?;
            info.check_post_channel("channel_id", id)?;
            Some(id).filter(|id| Some(*id as i64) != was.surge_channel_id)
        }
        None => None,
    };
    let enabled = body.enabled.filter(|&on| on != was.surge_enabled);
    if enabled.is_none() && channel.is_none() && hours.is_none() {
        return Ok(Json(surge_json(&was)));
    }
    let now = db.set_surge(guild, enabled, channel, hours, state.app.now()).await?;
    let changed = |before: Value, after: Value| -> (Value, Value) {
        let (mut old, mut new) = (serde_json::Map::new(), serde_json::Map::new());
        for key in ["enabled", "channel_id", "every_hours"] {
            if before[key] != after[key] {
                old.insert(key.into(), before[key].clone());
                new.insert(key.into(), after[key].clone());
            }
        }
        (Value::Object(old), Value::Object(new))
    };
    let (before, after) = changed(surge_json(&was), surge_json(&now));
    audit::finish(
        &state,
        &managed.current.session,
        Change {
            guild,
            action: "surge.set",
            target: None,
            before,
            after,
        },
        Some(Invalidation::Guild(guild)),
    )
    .await;
    Ok(Json(surge_json(&now)))
}
