use std::collections::HashMap;

use axum::{
    extract::{FromRequestParts, OriginalUri, Path},
    http::request::Parts,
};

use crate::{
    dashboard_access,
    discord::{self, UserGuild},
    error::ApiError,
    extract::CurrentSession,
    state::WebState,
};

/// A server with the bot present and full Discord management rights or a current role grant for
/// this endpoint. The latter is checked for every read and write; unknown areas fail closed.
#[derive(Clone)]
pub struct ManagedGuild {
    pub id: u64,
    pub guild: UserGuild,
    pub current: CurrentSession,
    /// None means full Discord management rights; Some lists delegated areas.
    pub areas: Option<Vec<String>>,
}

impl FromRequestParts<WebState> for ManagedGuild {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &WebState) -> Result<Self, Self::Rejection> {
        if let Some(managed) = parts.extensions.get::<Self>() {
            return Ok(managed.clone());
        }
        let Path(params) = Path::<HashMap<String, String>>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::malformed("Bad path."))?;
        let id: u64 = params
            .get("g")
            .and_then(|g| g.parse().ok())
            .filter(|&g| g > 0)
            .ok_or_else(|| ApiError::malformed("Server ids are numbers."))?;
        let mut current = CurrentSession::from_request_parts(parts, state).await?;
        let guilds = discord::user_guilds(state, &mut current, true).await?;
        let guild = guilds
            .into_iter()
            .find(|g| g.id() == Some(id))
            .ok_or_else(ApiError::not_manager)?;
        // Even a cached/dev guild list cannot keep a departed member authorised.
        let member = state
            .app
            .http
            .guild_member(
                twilight_model::id::Id::new(id),
                twilight_model::id::Id::new(current.session.user_id),
            )
            .await
            .map_err(|_| ApiError::not_manager())?
            .model()
            .await
            .map_err(|_| ApiError::not_manager())?;
        let areas = if guild.can_manage() {
            None
        } else {
            let areas = dashboard_access::areas_for_roles(state, id, &member.roles).await?;
            let uri = parts
                .extensions
                .get::<OriginalUri>()
                .map(|u| &u.0)
                .unwrap_or(&parts.uri);
            let prefix = format!("/api/guilds/{id}");
            let path = uri.path().strip_prefix(&prefix).ok_or_else(ApiError::not_manager)?;
            if !dashboard_access::permitted(path, parts.method.is_safe(), &areas) {
                return Err(ApiError::not_manager());
            }
            Some(areas)
        };
        if !discord::bot_present(state, id).await {
            let invite = discord::invite_url(&state.settings, state.app.application_id.get(), id);
            return Err(ApiError::bot_missing(invite));
        }
        Ok(Self {
            id,
            guild,
            current,
            areas,
        })
    }
}
