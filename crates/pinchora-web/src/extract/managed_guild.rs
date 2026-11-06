use std::collections::HashMap;

use axum::{
    extract::{FromRequestParts, Path},
    http::request::Parts,
};

use crate::{
    discord::{self, UserGuild},
    error::ApiError,
    extract::CurrentSession,
    state::WebState,
};

/// A server from the `{g}` path segment that the user may manage: on their list with Manage Server
/// (or owner/Administrator), and with the bot in it. 403 `not_manager` or `bot_missing` otherwise.
pub struct ManagedGuild {
    pub id: u64,
    pub guild: UserGuild,
    pub current: CurrentSession,
}

impl FromRequestParts<WebState> for ManagedGuild {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &WebState) -> Result<Self, Self::Rejection> {
        let Path(params) = Path::<HashMap<String, String>>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::malformed("Bad path."))?;
        let id: u64 = params
            .get("g")
            .and_then(|g| g.parse().ok())
            .filter(|&g| g > 0)
            .ok_or_else(|| ApiError::malformed("Server ids are numbers."))?;
        let mut current = CurrentSession::from_request_parts(parts, state).await?;
        let guilds = discord::user_guilds(state, &mut current, false).await?;
        let guild = guilds
            .into_iter()
            .find(|g| g.id() == Some(id) && g.can_manage())
            .ok_or_else(ApiError::not_manager)?;
        if !discord::bot_present(state, id).await {
            let invite = discord::invite_url(&state.settings, state.app.application_id.get(), id);
            return Err(ApiError::bot_missing(invite));
        }
        Ok(Self { id, guild, current })
    }
}
