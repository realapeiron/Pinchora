//! Tracker settings for the Feeds dashboard.
use std::collections::BTreeMap;

use axum::{Json, Router, extract::State, routing::get};
use pinchora_cache::Invalidation;
use pinchora_core::community::trackers;
use pinchora_core::t;
use pinchora_db::TrackerSettings;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::audit;
use crate::audit::Change;
use crate::error::ApiError;
use crate::error::ApiJson;
use crate::error::ApiResult;
use crate::extract::CurrentSession;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::state::WebState;

pub fn router() -> Router<WebState> {
    Router::new().route("/", get(show).put(save))
}

/// Authenticated personal export, independent of guild management permissions.
pub async fn export(State(state): State<WebState>, current: CurrentSession) -> ApiResult<Json<Value>> {
    Ok(Json(state.app.db.tracker_user_data(current.session.user_id).await?))
}

fn view(settings: &TrackerSettings, lang: pinchora_core::infra::i18n::Lang) -> Value {
    json!({
        "patch_channels": settings.patch_channels,
        "mc_watch": settings.mc_watch,
        "mc_news_channel_id": settings.mc_news_channel_id.map(|id|id.to_string()),
        "language":lang.code(),
        "strings": pinchora_core::infra::i18n::keys(lang).into_iter().filter_map(|key|key.strip_prefix("trackers.ui.").map(|name|(name.to_owned(),t!(lang,&key)))).collect::<BTreeMap<_,_>>()
    })
}
async fn show(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let settings = state.app.db.tracker_settings(managed.id).await?;
    Ok(Json(view(&settings, state.app.guild_lang(managed.id).await)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Watch {
    host: String,
    channel_id: String,
    #[serde(default, rename = "message_id")]
    _message_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    patch_channels: BTreeMap<String, String>,
    mc_watch: Vec<Watch>,
    mc_news_channel_id: Option<String>,
}
fn channel(raw: &str, lang: pinchora_core::infra::i18n::Lang) -> ApiResult<u64> {
    raw.parse::<u64>()
        .ok()
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        .ok_or_else(|| ApiError::malformed(t!(lang, "trackers.invalid-channel")))
}
async fn save(
    State(state): State<WebState>,
    managed: ManagedGuild,
    ApiJson(body): ApiJson<Save>,
) -> ApiResult<Json<Value>> {
    let guild = managed.id;
    let lang = state.app.guild_lang(guild).await;
    if body.patch_channels.len() > 3
        || body.mc_watch.len() > 20
        || body
            .patch_channels
            .keys()
            .any(|k| !matches!(k.as_str(), "lol" | "valorant" | "mc"))
    {
        return Err(ApiError::malformed(t!(lang, "trackers.invalid-settings")));
    }
    let info = guild_info::get_with(&state, guild, |info| {
        body.patch_channels
            .values()
            .chain(body.mc_watch.iter().map(|w| &w.channel_id))
            .chain(body.mc_news_channel_id.iter())
            .all(|id| id.parse::<u64>().ok().is_some_and(|id| info.channel(id).is_some()))
    })
    .await?;
    for id in body
        .patch_channels
        .values()
        .chain(body.mc_watch.iter().map(|w| &w.channel_id))
        .chain(body.mc_news_channel_id.iter())
    {
        info.check_post_channel("channel_id", channel(id, lang)?)?;
    }
    let before = state.app.db.tracker_settings(guild).await?;
    let mut watches = Vec::new();
    for watch in body.mc_watch {
        let (host, port) =
            trackers::parse_host(&watch.host).map_err(|_| ApiError::malformed(t!(lang, "trackers.invalid-host")))?;
        let host = format!("{host}:{port}");
        if watches.iter().any(|v: &Value| v["host"] == host) {
            return Err(ApiError::malformed(t!(lang, "trackers.invalid-settings")));
        }
        let previous = before
            .mc_watch
            .as_array()
            .into_iter()
            .flatten()
            .find(|old| old["host"] == host && old["channel_id"] == watch.channel_id)
            .and_then(|old| old["message_id"].as_str());
        watches.push(json!({"host":host,"channel_id":watch.channel_id,"message_id":previous}));
    }
    let settings = TrackerSettings {
        guild_id: guild as i64,
        patch_channels: json!(body.patch_channels),
        mc_watch: json!(watches),
        mc_news_channel_id: body
            .mc_news_channel_id
            .as_deref()
            .map(|s| channel(s, lang).map(|id| id as i64))
            .transpose()?,
    };
    state.app.db.save_tracker_settings(guild, &settings).await?;
    // Return the authoritative message IDs, including a poller update racing this save.
    let settings = state.app.db.tracker_settings(guild).await?;
    audit::finish(&state,&managed.current.session,Change {guild,action:"trackers.set",target:None,
        before:json!({"patch_channels":before.patch_channels,"mc_watch":before.mc_watch,"mc_news_channel_id":before.mc_news_channel_id}),
        after:json!({"patch_channels":settings.patch_channels,"mc_watch":settings.mc_watch,"mc_news_channel_id":settings.mc_news_channel_id})},
        Some(Invalidation::Guild(guild))).await;
    Ok(Json(view(&settings, lang)))
}
