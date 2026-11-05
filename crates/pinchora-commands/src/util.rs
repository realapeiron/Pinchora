use chrono::{DateTime, Utc};
use twilight_interactions::command::ResolvedUser;
use twilight_model::user::User;

/// Pinchora coral.
pub const BRAND: u32 = 0xff6b4a;
pub const SUCCESS: u32 = 0x06d6a0;
pub const WARN: u32 = 0xffd166;
pub const DANGER: u32 = 0xef476f;

/// Discord's relative timestamp markup, e.g. "in 3 hours".
pub fn relative(at: DateTime<Utc>) -> String {
    format!("<t:{}:R>", at.timestamp())
}

pub fn mention(id: impl std::fmt::Display) -> String {
    format!("<@{id}>")
}

/// Fills the `{key}` placeholders of a message template. Unknown placeholders stay as written, and
/// a value that itself looks like a placeholder is not expanded (a name can be anything).
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find(['{', '}']) {
            Some(end) if after.as_bytes()[end] == b'}' => {
                let key = &after[..end];
                match values.iter().find(|(k, _)| *k == key) {
                    Some((_, value)) => out.push_str(value),
                    None => {
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The user's avatar on Discord's CDN, or their default one.
pub fn avatar_url(user: &User) -> String {
    match user.avatar {
        Some(hash) => format!("https://cdn.discordapp.com/avatars/{}/{hash}.png?size=128", user.id),
        None => format!(
            "https://cdn.discordapp.com/embed/avatars/{}.png",
            (user.id.get() >> 22) % 6
        ),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_every_placeholder() {
        let text = fill(
            "Bye {user}, {server} has {members} left. {user}!",
            &[("user", "Sim"), ("server", "Reef"), ("members", "41")],
        );
        assert_eq!(text, "Bye Sim, Reef has 41 left. Sim!");
    }

    #[test]
    fn unknown_and_broken_placeholders_stay() {
        let values = [("user", "Sim")];
        assert_eq!(fill("{user} {level} {", &values), "Sim {level} {");
        assert_eq!(fill("{{user}}", &values), "{Sim}");
        assert_eq!(fill("no braces", &values), "no braces");
        assert_eq!(fill("", &values), "");
    }

    #[test]
    fn values_are_not_filled_again() {
        let text = fill("{user} left {server}", &[("user", "{server}"), ("server", "Reef")]);
        assert_eq!(text, "{server} left Reef");
    }

    #[test]
    fn keeps_unicode_around_placeholders() {
        assert_eq!(fill("👋 {user} — bye", &[("user", "Zoë")]), "👋 Zoë — bye");
    }
}
