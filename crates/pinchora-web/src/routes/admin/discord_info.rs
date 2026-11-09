//! `GET /api/guilds/{g}/discord`: the server's channels and roles for the pickers, each role marked
//! with whether the manager asking can set it up to be handed out (the bot must be able to give it, and
//! they may) and, when not, the sentence `/config` would answer with.

use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::extract::ManagedGuild;
use crate::guild_info;
use crate::guild_info::GuildInfo;
use crate::state::WebState;

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
    let info = guild_info::get(&state, managed.id, force).await?;
    // The roles are marked for the manager asking: what they may set up depends on their own roles.
    let grantor = guild_info::grantor(&state, &managed, &info.roles, force).await?;
    Ok(Json(info.for_grantor(grantor)))
}
