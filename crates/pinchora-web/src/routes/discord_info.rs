//! `GET /api/guilds/{g}/discord`: the server's channels and roles for the pickers, each role marked
//! with whether the bot can hand it out and, when not, the sentence `/config` would answer with.

use std::time::Duration;

use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;

use crate::{
    error::ApiResult,
    extract::ManagedGuild,
    guild_info::{self, GuildInfo},
    state::WebState,
};

/// How often one server's lists may be fetched again on request.
const REFRESH_COOLDOWN: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
pub struct InfoQuery {
    /// `1` asks Discord again instead of using the minute-old copy.
    refresh: Option<String>,
}

pub async fn discord(
    State(state): State<WebState>,
    managed: ManagedGuild,
    Query(query): Query<InfoQuery>,
) -> ApiResult<Json<GuildInfo>> {
    let mut force = query.refresh.as_deref() == Some("1");
    if force {
        let key = format!("web:guild-info-refresh:{}", managed.id);
        // Too soon after the last refresh: the copy that refresh made is fresh enough.
        force = state.app.cache.try_cooldown(&key, REFRESH_COOLDOWN).await?.is_none();
    }
    Ok(Json(guild_info::get(&state, managed.id, force).await?))
}
