//! `GET /api/guilds/{g}`: the server, its settings and its latest changes.

use axum::{Json, extract::State};
use pinchora_core::t;
use serde_json::{Value, json};

use super::{
    guilds::{AuditView, GuildView, SettingsView},
    modules::module_list,
};
use crate::{error::ApiResult, extract::ManagedGuild, state::WebState};

/// How many recent changes the overview lists.
const RECENT_CHANGES: i64 = 5;

pub async fn overview(State(state): State<WebState>, managed: ManagedGuild) -> ApiResult<Json<Value>> {
    let lang = state.app.guild_lang(managed.id).await;
    let db = &state.app.db;
    let settings = db.guild_settings(managed.id).await?;
    let has_area = |area: &str| {
        managed
            .areas
            .as_ref()
            .is_none_or(|areas| areas.iter().any(|a| a == area))
    };
    let audit: Vec<AuditView> = if has_area("audit") {
        db.audit_page(managed.id, None, RECENT_CHANGES)
            .await?
            .into_iter()
            .map(AuditView::from)
            .collect()
    } else {
        Vec::new()
    };
    Ok(Json(json!({
        "guild": GuildView::new(managed.id, &managed.guild.name, managed.guild.icon.as_deref()),
        "settings": has_area("configuration").then(|| SettingsView::from(settings)),
        "access": { "full": managed.areas.is_none(), "areas": managed.areas },
        "language": lang.code(),
        "modules": if has_area("configuration") { module_list(&state, managed.id).await? } else { Vec::new() },
        "audit": audit,
        "navigation": {
            "utility": t!(lang, "module.utility"),
            "events": t!(lang, "module.events"),
            "birthdays": t!(lang, "module.birthdays"),
            "anti-nuke": t!(lang, "module.anti_nuke"),
            "achievements": t!(lang, "module.achievements"),
            "command-control": t!(lang, "command_control.ui.title"),
            "backup": t!(lang, "config_backup.ui.title"),
            "invites": t!(lang, "module.invites"),
            "safety": t!(lang, "verification.ui.title"),
        },
    })))
}
