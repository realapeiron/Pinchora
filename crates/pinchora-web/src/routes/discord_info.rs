//! `GET /api/guilds/{g}/discord`: the server's channels and roles for the pickers, each role marked
//! with whether the bot can hand it out and, when not, the sentence `/config` would answer with.

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
    // Too soon after the last refresh: the copy that refresh made is fresh enough.
    let force = query.refresh.as_deref() == Some("1") && guild_info::may_refresh(&state, managed.id).await?;
    Ok(Json(guild_info::get(&state, managed.id, force).await?))
}
