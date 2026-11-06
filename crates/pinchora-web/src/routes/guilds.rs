//! The admin area of one server, `/api/guilds/{g}/…`, and the JSON shapes it shares.

use axum::{
    Router,
    routing::{get, patch},
};
use chrono::{DateTime, Utc};
use pinchora_db::{AuditRow, GuildSettings};
use serde::Serialize;
use serde_json::Value;

use super::{me::icon_url, overview, settings};
use crate::state::WebState;

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/", get(overview::overview))
        .route("/settings", patch(settings::patch))
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
}

fn id(value: Option<i64>) -> Option<String> {
    value.map(|v| v.to_string())
}

impl From<GuildSettings> for SettingsView {
    fn from(s: GuildSettings) -> Self {
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
    fn from(row: AuditRow) -> Self {
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
