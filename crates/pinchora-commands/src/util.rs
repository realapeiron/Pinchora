use chrono::{DateTime, Utc};
use twilight_interactions::command::ResolvedUser;
use twilight_model::user::User;

/// Pinchora coral.
pub const BRAND: u32 = 0xff6b4a;
pub const SUCCESS: u32 = 0x06d6a0;
pub const WARN: u32 = 0xffd166;

/// Discord's relative timestamp markup, e.g. "in 3 hours".
pub fn relative(at: DateTime<Utc>) -> String {
    format!("<t:{}:R>", at.timestamp())
}

pub fn mention(id: impl std::fmt::Display) -> String {
    format!("<@{id}>")
}

pub fn display_name(user: &User, nick: Option<&str>) -> String {
    nick.map(str::to_owned)
        .or_else(|| user.global_name.clone())
        .unwrap_or_else(|| user.name.clone())
}

pub fn resolved_display_name(resolved: &ResolvedUser) -> String {
    display_name(
        &resolved.resolved,
        resolved.member.as_ref().and_then(|m| m.nick.as_deref()),
    )
}

pub use pinchora_render::thousands;
