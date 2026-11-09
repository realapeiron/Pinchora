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

pub use pinchora_render::thousands_in;

/// The length of the user or role mention (`<@1>`, `<@!1>`, `<@&1>`) at the start of `text`, if any.
fn mention_len(text: &str) -> Option<usize> {
    let inner = text.strip_prefix("<@")?;
    let digits = inner.strip_prefix(['!', '&']).unwrap_or(inner);
    let end = digits.find('>')?;
    (end > 0 && digits[..end].bytes().all(|b| b.is_ascii_digit())).then(|| text.len() - digits.len() + end + 1)
}

/// Member text made safe to show: user and role mentions are dropped and `@everyone` and `@here` lose
/// their `@`, so nothing in it can ping. Spaces are tidied per line and the ends trimmed; line
/// breaks stay.
pub fn strip_mentions(text: &str) -> String {
    // Dropping one mention can join what was around it into another (`<<@1>@2>`, `@@everyone`), so
    // strip until nothing changes.
    let mut out = strip_once(text);
    loop {
        let next = strip_once(&out);
        if next == out {
            return out;
        }
        out = next;
    }
}

fn strip_once(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(['<', '@']) {
        out.push_str(&rest[..at]);
        let here = &rest[at..];
        if let Some(len) = mention_len(here) {
            rest = &here[len..];
            continue;
        }
        let lower = here.to_ascii_lowercase();
        if here.starts_with('@') && (lower.starts_with("@everyone") || lower.starts_with("@here")) {
            rest = &here[1..];
            continue;
        }
        out.push_str(&here[..1]);
        rest = &here[1..];
    }
    out.push_str(rest);
    out.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// Text on one line: every run of whitespace, line breaks included, becomes one space.
pub fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use crate::infra::util::*;

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
    fn mentions_are_dropped_and_broadcasts_defanged() {
        assert_eq!(
            strip_mentions("hi <@123> and <@!456> and <@&789>, @everyone @here @Everyone!"),
            "hi and and , everyone here Everyone!"
        );
        assert_eq!(strip_mentions("<@1><@2>"), "");
        assert_eq!(strip_mentions("a  b \n\n  c  "), "a b\n\nc");
        for nested in ["<<@1>@2>", "<@<@1>&2>", "@@everyone", "@every<@1>one", "@<@1>here"] {
            let out = strip_mentions(nested);
            assert!(
                !out.contains("<@") && !out.contains("@everyone") && !out.contains("@here"),
                "{nested:?} → {out:?}"
            );
        }
    }

    #[test]
    fn things_that_only_look_like_mentions_stay() {
        for text in [
            "<@>",
            "<@abc>",
            "<@!>",
            "<@&>",
            "<@!&1>",
            "<@12",
            "a < b @ c",
            "<#123> is a channel",
            "a <@1x> b",
        ] {
            assert_eq!(strip_mentions(text), text, "{text:?}");
        }
        assert_eq!(strip_mentions("Zoë <@1> 🎉"), "Zoë 🎉");
    }

    #[test]
    fn single_line_flattens_whitespace() {
        assert_eq!(single_line("  one\n two\t\tthree  "), "one two three");
        assert_eq!(single_line(""), "");
    }

    #[test]
    fn keeps_unicode_around_placeholders() {
        assert_eq!(fill("👋 {user} — bye", &[("user", "Zoë")]), "👋 Zoë — bye");
    }
}
