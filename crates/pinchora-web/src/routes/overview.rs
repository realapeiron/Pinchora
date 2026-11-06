//! `GET /api/guilds/{g}`: the server, its settings and its latest changes.

use axum::{Json, extract::State};
use serde_json::{Value, json};

use super::{
    guilds::{AuditView, GuildView, SettingsView},
    modules::module_list,
};
use crate::{error::ApiResult, extract::ManagedGuild, state::WebState};

/// How many recent changes the overview lists.
const RECENT_CHANGES: i64 = 5;

pub async fn overview(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let db = &state.app.db;
    let settings = db.guild_settings(managed.id).await?;
    let audit: Vec<AuditView> = db
        .audit_page(managed.id, None, RECENT_CHANGES)
        .await?
        .into_iter()
        .map(AuditView::from)
        .collect();
    Ok(Json(json!({
        "guild": GuildView::new(managed.id, &managed.guild.name, managed.guild.icon.as_deref()),
        "settings": SettingsView::from(settings),
        "modules": module_list(&state, managed.id).await?,
        "audit": audit,
    })))
}
