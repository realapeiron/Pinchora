use axum::http::StatusCode;
use pinchora_commands::PostRefusal;
use pinchora_core::admin::settings::FieldError;
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use pinchora_db::{PanelRoleRow, PanelRow};
use serde_json::{Value, json};

use crate::error::ApiError;

pub(super) fn auto_json(row: &pinchora_db::AutoRoleRow) -> Value {
    json!({
        "role_id": row.role_id.to_string(),
        "kind": row.kind,
        "delay_secs": row.delay_secs,
        "remove_after_secs": row.remove_after_secs,
        "enabled": row.enabled,
    })
}

fn entry_json(entry: &PanelRoleRow) -> Value {
    json!({
        "role_id": entry.role_id.to_string(),
        "label": entry.label,
        "emoji": entry.emoji,
        "description": entry.description,
        "temp_secs": entry.temp_secs,
    })
}

pub(super) fn panel_json(panel: &PanelRow, entries: &[PanelRoleRow]) -> Value {
    json!({
        "id": panel.id.to_string(),
        "title": panel.title,
        "description": panel.description,
        "style": panel.style,
        "exclusive": panel.exclusive,
        "required_role": panel.required_role.map(|r| r.to_string()),
        "min_picks": panel.min_picks,
        "max_picks": panel.max_picks,
        "channel_id": panel.channel_id.map(|c| c.to_string()),
        "message_id": panel.message_id.map(|m| m.to_string()),
        "roles": entries.iter().map(entry_json).collect::<Vec<_>>(),
    })
}

pub(super) fn post_refusal(refusal: PostRefusal, lang: Lang) -> ApiError {
    match refusal {
        PostRefusal::NoRoles => ApiError::invalid(vec![FieldError::new("roles", "Add a role to the panel first.")]),
        PostRefusal::MissingEmoji(label) => ApiError::invalid(vec![FieldError::new(
            "roles",
            format!("A reactions panel needs an emoji on every role, and {label} has none."),
        )]),
        PostRefusal::SameEmoji(label) => ApiError::invalid(vec![FieldError::new(
            "roles",
            format!("Two roles share an emoji: {label} needs one of its own."),
        )]),
        PostRefusal::NoAccess => ApiError::blocked(
            "I can't post there: I need View Channel, Send Messages and Embed Links in that channel (and Add Reactions \
             for a reactions panel).",
        ),
        PostRefusal::UnknownChannel => ApiError::invalid(vec![FieldError::new(
            "channel_id",
            "That channel no longer exists, or I can't see it. Refresh the list.",
        )]),
        PostRefusal::Refused => ApiError::blocked(t!(lang, "roles.web.errors.post-refused")),
        PostRefusal::Unreachable => ApiError::new(
            StatusCode::BAD_GATEWAY,
            "discord_unreachable",
            "Discord could not be reached. Try again in a moment.",
        ),
    }
}
