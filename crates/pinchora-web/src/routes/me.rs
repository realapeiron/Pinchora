//! The logged-in user and the servers they can manage.

use std::time::Duration;

use axum::{Json, extract::State};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    auth::session::Session,
    discord::{self, UserGuild},
    error::{ApiError, ApiResult},
    extract::CurrentSession,
    state::WebState,
};

/// How often one user may make the dashboard ask Discord for their server list again.
const REFRESH_COOLDOWN: Duration = Duration::from_secs(10);

/// `GET /api/me`: who is logged in, and the token writes must carry.
pub async fn me(State(state): State<WebState>, current: CurrentSession) -> Json<Value> {
    let s = &current.session;
    Json(json!({
        "user": {
            "id": s.user_id.to_string(),
            "username": s.username,
            "global_name": s.global_name,
            "avatar_url": avatar_url(s),
        },
        "csrf": s.csrf,
        "capabilities": { "message_content": state.settings.message_content },
    }))
}

/// The user's avatar on Discord's CDN, or the default one their id picks.
pub fn avatar_url(s: &Session) -> String {
    match &s.avatar {
        Some(hash) => format!("https://cdn.discordapp.com/avatars/{}/{hash}.png?size=64", s.user_id),
        None => format!("https://cdn.discordapp.com/embed/avatars/{}.png", (s.user_id >> 22) % 6),
    }
}

/// A server on the picker.
#[derive(Serialize)]
struct GuildEntry {
    id: String,
    name: String,
    icon_url: Option<String>,
    owner: bool,
    bot_present: bool,
    /// Adds the bot, when it is not there yet.
    invite_url: Option<String>,
}

/// `GET /api/me/guilds`: the servers the user manages, the ones with the bot first.
pub async fn guilds(State(state): State<WebState>, mut current: CurrentSession) -> ApiResult<Json<Value>> {
    let list = discord::user_guilds(&state, &mut current, false).await?;
    Ok(Json(json!({ "items": entries(&state, list).await })))
}

/// `POST /api/me/guilds/refresh`: asks Discord again now, at most once per ten seconds.
pub async fn refresh_guilds(State(state): State<WebState>, mut current: CurrentSession) -> ApiResult<Json<Value>> {
    let key = format!("web:guilds-refresh:{}", current.session.user_id);
    if let Some(left) = state.app.cache.try_cooldown(&key, REFRESH_COOLDOWN).await? {
        return Err(ApiError::too_many(left.as_secs() + 1));
    }
    let list = discord::user_guilds(&state, &mut current, true).await?;
    Ok(Json(json!({ "items": entries(&state, list).await })))
}

async fn entries(state: &WebState, list: Vec<UserGuild>) -> Vec<GuildEntry> {
    let managed: Vec<(u64, UserGuild)> = list
        .into_iter()
        .filter(UserGuild::can_manage)
        .filter_map(|g| Some((g.id()?, g)))
        .collect();
    let ids: Vec<u64> = managed.iter().map(|(id, _)| *id).collect();
    let present = discord::bots_present(state, &ids).await;
    let mut entries: Vec<GuildEntry> = managed
        .into_iter()
        .zip(present)
        .map(|((id, g), bot_present)| GuildEntry {
            icon_url: icon_url(id, g.icon.as_deref()),
            invite_url: (!bot_present)
                .then(|| discord::invite_url(&state.settings, state.app.application_id.get(), id)),
            id: g.id,
            name: g.name,
            owner: g.owner,
            bot_present,
        })
        .collect();
    entries.sort_by(|a, b| {
        b.bot_present
            .cmp(&a.bot_present)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

/// A server's icon on Discord's CDN.
pub fn icon_url(guild: u64, icon: Option<&str>) -> Option<String> {
    icon.filter(|hash| hash.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        .map(|hash| format!("https://cdn.discordapp.com/icons/{guild}/{hash}.png?size=128"))
}
