//! The admin area of one server, `/api/guilds/{g}/…`, and the JSON shapes it shares.

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, patch, post, put},
};
use chrono::{DateTime, Utc};
use pinchora_core::admin::card_design::UPLOAD_MAX_BYTES;
use pinchora_db::{AuditRow, GuildSettings};
use serde::Serialize;
use serde_json::Value;

use crate::routes::admin::cards;
use crate::routes::admin::commands;
use crate::routes::admin::discord_info;
use crate::routes::admin::modules;
use crate::routes::admin::overview;
use crate::routes::admin::preview;
use crate::routes::admin::responders;
use crate::routes::admin::settings;
use crate::routes::community::community;
use crate::routes::economy::store;
use crate::routes::infra::me::icon_url;
use crate::routes::moderation::audit_log;
use crate::routes::moderation::moderation;
use crate::routes::moderation::moderation::{moderation as get_moderation, put_moderation};
use crate::routes::progression::leveling;
use crate::state::WebState;

pub fn router() -> Router<WebState> {
    Router::new()
        .nest("/access", crate::routes::admin::access::router())
        .nest("/audit-tools", crate::routes::moderation::audit_tools::router())
        .nest("/mod-parity", crate::routes::moderation::policy::router())
        .route("/", get(overview::overview))
        .route("/discord", get(discord_info::discord))
        .nest("/notifications", crate::routes::admin::notifications::router())
        .route("/settings", patch(settings::patch))
        .route("/audit", get(audit_log::audit))
        .route("/xp-rules", get(leveling::xp_rules).delete(leveling::clear_xp_rules))
        .nest(
            "/leveling-parity",
            crate::routes::progression::leveling_policy::router(),
        )
        .route(
            "/xp-rules/{kind}/{target}",
            put(leveling::put_xp_rule).delete(leveling::delete_xp_rule),
        )
        .route("/level-roles", get(leveling::level_roles))
        .route("/level-roles/sync", post(leveling::sync_level_roles))
        .route(
            "/level-roles/{role}",
            put(leveling::put_level_role).delete(leveling::delete_level_role),
        )
        .route("/rep-roles", get(leveling::rep_roles))
        .route(
            "/rep-roles/{role}",
            put(leveling::put_rep_role).delete(leveling::delete_rep_role),
        )
        .route("/jobs/{id}", get(leveling::job))
        .nest("/xp-admin", crate::routes::progression::xp_admin::router())
        .nest("/economy", crate::routes::economy::player::router())
        .route("/store", get(store::store))
        .route("/store/{role}", put(store::put_item).delete(store::delete_item))
        .route("/modules", get(modules::modules))
        .route("/modules/{module}", put(modules::put_module))
        .route("/commands", get(commands::list).post(commands::create))
        .route("/commands/export", get(commands::export))
        .route("/commands/import", post(commands::import))
        .route("/commands/preview", post(commands::preview))
        .route(
            "/commands/{id}",
            get(commands::get)
                .put(commands::replace)
                .patch(commands::set_enabled)
                .delete(commands::delete),
        )
        .route("/commands/{id}/test", post(commands::test))
        .route("/responders", get(responders::list).post(responders::create))
        .route("/responders/export", get(responders::export))
        .route("/responders/import", post(responders::import))
        .route("/responders/preview", post(responders::preview))
        .route(
            "/responders/{id}",
            get(responders::get)
                .put(responders::replace)
                .patch(responders::set_enabled)
                .delete(responders::delete),
        )
        .route("/responders/{id}/test", post(responders::test))
        .route("/community", get(community::community))
        .nest("/community-pack", crate::routes::community::engagement::router())
        .nest("/community-v2", crate::routes::community::tools::router())
        .route("/suggestions/settings", put(community::put_suggestions))
        .route("/critters/settings", put(community::put_critters))
        .route("/surge/settings", put(community::put_surge))
        .route("/moderation", get(get_moderation).put(put_moderation))
        .route("/logs", get(moderation::logs).put(moderation::put_logs))
        .route("/cases", get(moderation::cases))
        .route("/cases/{case}", get(moderation::case).patch(moderation::patch_case))
        .route("/cases/{case}/revoke", post(moderation::revoke_case))
        .route("/staff-log", get(moderation::staff_log))
        .nest("/command-control", crate::routes::admin::command_control::router())
        .nest("/backup", crate::routes::admin::config_backup::router())
        .nest("/branding", crate::routes::admin::branding::router())
        .nest("/automod", crate::routes::moderation::automod::router())
        .nest("/anti_nuke", crate::routes::moderation::anti_nuke::router())
        .nest("/temp-voice", crate::routes::admin::temp_voice::router())
        .nest("/safety", crate::routes::moderation::verification::router())
        .route("/preview/card", post(cards::preview))
        .route("/preview/message", post(preview::message))
        .route("/messages/{kind}/test", post(preview::test_message))
        .route("/cards", get(cards::cards))
        .route(
            "/cards/uploads",
            post(cards::upload).layer(DefaultBodyLimit::max(UPLOAD_MAX_BYTES + 1024)),
        )
        .route("/cards/uploads/{id}", get(cards::upload_png))
        .route("/cards/{kind}", put(cards::put_card))
        .route("/cards/{kind}/history", get(cards::history))
        .route("/cards/{kind}/history/{revision}/rollback", post(cards::rollback))
        .route("/cards/{kind}/test", post(cards::test))
        .nest("/message-studio", crate::routes::admin::message_studio::router())
        .nest("/roles", crate::routes::roles::roles::router())
        .nest("/stats", crate::routes::progression::stats::router())
        .nest("/tickets", crate::routes::community::tickets::router())
        .nest("/birthdays", crate::routes::community::birthdays::router())
        .nest("/events", crate::routes::community::events::router())
        .nest("/feeds", crate::routes::integrations::feeds::router())
        .nest("/trackers", crate::routes::community::trackers::router())
        .nest("/starboard", crate::routes::community::starboard::router())
        .nest("/invites", crate::routes::community::invites::router())
        .nest("/roblox", crate::routes::integrations::roblox::router())
        .nest("/achievements", crate::routes::progression::achievements::router())
        .nest("/workflows", crate::routes::admin::workflows::router())
        .nest("/utility", crate::routes::admin::afk_sticky_schedule::router())
}

#[derive(Serialize)]
pub struct GuildView {
    pub id: String,
    pub name: String,
    pub icon_url: Option<String>,
}

impl GuildView {
    pub fn new(id: u64, name: &str, icon: Option<&str>) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_owned(),
            icon_url: icon_url(id, icon),
        }
    }
}

/// A server's settings row, ids as strings and the card accent as `#rrggbb`.
#[derive(Serialize)]
pub struct SettingsView {
    pub leveling_enabled: bool,
    pub levelup_channel_id: Option<String>,
    pub levelup_message: Option<String>,
    pub levelup_mode: String,
    pub level_roles_stack: bool,
    pub voice_xp_enabled: bool,
    pub voice_xp_amount: i32,
    pub welcome_enabled: bool,
    pub welcome_channel_id: Option<String>,
    pub welcome_message: Option<String>,
    pub welcome_theme: Option<String>,
    pub welcome_title: Option<String>,
    pub welcome_accent: Option<String>,
    pub join_role_id: Option<String>,
    pub goodbye_channel_id: Option<String>,
    pub goodbye_message: Option<String>,
    pub points_enabled: bool,
    pub points_name: String,
    pub points_emoji: String,
    pub points_min: i32,
    pub points_max: i32,
    pub log_channel_id: Option<String>,
    pub log_members: bool,
    pub log_names: bool,
    pub log_bans: bool,
    pub log_voice: bool,
    /// The prefix text commands start with, the default when the server picked none.
    pub prefix: String,
}

fn id(value: Option<i64>) -> Option<String> {
    value.map(|v| v.to_string())
}

impl From<GuildSettings> for SettingsView {
    fn from(s: GuildSettings) -> Self {
        let prefix = s.prefix().to_owned();
        Self {
            leveling_enabled: s.leveling_enabled,
            levelup_channel_id: id(s.levelup_channel_id),
            levelup_message: s.levelup_message,
            levelup_mode: s.levelup_mode,
            level_roles_stack: s.level_roles_stack,
            voice_xp_enabled: s.voice_xp_enabled,
            voice_xp_amount: s.voice_xp_amount,
            welcome_enabled: s.welcome_enabled,
            welcome_channel_id: id(s.welcome_channel_id),
            welcome_message: s.welcome_message,
            welcome_theme: s.welcome_theme,
            welcome_title: s.welcome_title,
            welcome_accent: s.welcome_accent.map(|c| format!("#{:06x}", c & 0x00ff_ffff)),
            join_role_id: id(s.join_role_id),
            goodbye_channel_id: id(s.goodbye_channel_id),
            goodbye_message: s.goodbye_message,
            points_enabled: s.points_enabled,
            points_name: s.points_name,
            points_emoji: s.points_emoji,
            points_min: s.points_min,
            points_max: s.points_max,
            log_channel_id: id(s.log_channel_id),
            log_members: s.log_members,
            log_names: s.log_names,
            log_bans: s.log_bans,
            log_voice: s.log_voice,
            prefix,
        }
    }
}

/// One audit log row.
#[derive(Serialize)]
pub struct AuditView {
    pub id: String,
    pub actor_id: String,
    pub actor_name: String,
    pub source: String,
    pub action: String,
    pub target: Option<String>,
    pub before: Value,
    pub after: Value,
    pub created_at: DateTime<Utc>,
}

impl From<AuditRow> for AuditView {
    fn from(mut row: AuditRow) -> Self {
        row.redact();
        let parse = |raw: Option<String>| raw.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or(Value::Null);
        Self {
            id: row.id.to_string(),
            actor_id: row.actor_id.to_string(),
            actor_name: row.actor_name,
            source: row.source,
            action: row.action,
            target: row.target,
            before: parse(row.before),
            after: parse(row.after),
            created_at: row.created_at,
        }
    }
}
