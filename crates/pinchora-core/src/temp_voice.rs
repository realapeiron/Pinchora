//! Temp voice: the limits and the naming rules `/voice creators` and the dashboard share. Pure.

use crate::{i18n::Lang, settings::FieldError, t};

/// Most creator channels one server may set up.
pub const MAX_CREATORS: usize = 10;
/// Most temp rooms a server may have alive at once (Discord stops a server at 500 channels).
pub const MAX_ROOMS: i64 = 60;
/// Most rooms one member may own at a time, per creator: the highest `max_per_user`.
pub const MAX_PER_USER: i32 = 5;
/// The most people a voice channel takes.
pub const MAX_LIMIT: i32 = 99;
/// Discord's longest channel name.
pub const MAX_NAME: usize = 100;
/// The longest name template.
pub const MAX_TEMPLATE: usize = 80;
/// The name a creator gives rooms until its server picks one.
pub const DEFAULT_TEMPLATE: &str = "{user}'s room";
/// A room is renamed at most this often (Discord allows two renames per ten minutes).
pub const RENAME_COOLDOWN_SECS: u64 = 5 * 60;
/// A room nobody has joined or left for this long and that is empty is swept away.
pub const EMPTY_GRACE_SECS: i64 = 20;
/// What a room is called when its template comes out as nothing.
pub const FALLBACK_NAME: &str = "Voice room";

/// The words a template may use.
pub const PLACEHOLDERS: [&str; 2] = ["user", "n"];

/// A template, checked and tidied. It names the room: the owner's name goes where `{user}` is, and the
/// number of the room among the creator's where `{n}` is.
pub fn check_template(raw: &str) -> Result<String, FieldError> {
    let template = tidy(raw);
    if template.is_empty() {
        return Err(FieldError::new(
            "name_template",
            t!(Lang::En, "voice.validation.empty-template"),
        ));
    }
    if template.chars().count() > MAX_TEMPLATE {
        return Err(FieldError::new(
            "name_template",
            t!(Lang::En, "voice.validation.template-length", count = MAX_TEMPLATE),
        ));
    }
    let mut rest = template.as_str();
    while let Some(open) = rest.find('{') {
        if rest[..open].contains('}') {
            return Err(FieldError::new(
                "name_template",
                t!(Lang::En, "voice.validation.extra-close"),
            ));
        }
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return Err(FieldError::new(
                "name_template",
                t!(Lang::En, "voice.validation.unclosed-open"),
            ));
        };
        let word = &after[..close];
        if !PLACEHOLDERS.contains(&word) {
            return Err(FieldError::new(
                "name_template",
                t!(Lang::En, "voice.validation.unknown-word", word = format!("{{{word}}}")),
            ));
        }
        rest = &after[close + 1..];
    }
    if rest.contains('}') {
        return Err(FieldError::new(
            "name_template",
            t!(Lang::En, "voice.validation.extra-close"),
        ));
    }
    Ok(template)
}

/// The most people a room takes, checked: 0 means no limit.
pub fn check_limit(limit: i64) -> Result<i32, FieldError> {
    i32::try_from(limit)
        .ok()
        .filter(|l| (0..=MAX_LIMIT).contains(l))
        .ok_or_else(|| {
            FieldError::new(
                "default_limit",
                t!(Lang::En, "voice.validation.limit", count = MAX_LIMIT),
            )
        })
}

/// How many rooms one member may own at once, checked.
pub fn check_per_user(count: i64) -> Result<i32, FieldError> {
    i32::try_from(count)
        .ok()
        .filter(|c| (1..=MAX_PER_USER).contains(c))
        .ok_or_else(|| {
            FieldError::new(
                "max_per_user",
                t!(Lang::En, "voice.validation.per-user", count = MAX_PER_USER),
            )
        })
}

/// Whitespace collapsed to single spaces, control characters dropped.
fn tidy(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '@' => '＠',
            '<' => '‹',
            '>' => '›',
            c if c.is_whitespace() => ' ',
            c => c,
        })
        .filter(|c| !c.is_control())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A room's name: `template` with the owner's name and the room's number filled in, at most
/// [`MAX_NAME`] characters. The owner's name goes in last, so a name that looks like `{n}` is left as it is.
pub fn room_name(template: &str, owner: &str, number: usize) -> String {
    let name = tidy(&template.replace("{n}", &number.to_string()).replace("{user}", owner));
    clip(&name)
}

/// A name somebody typed for their room, or `None` when nothing is left of it.
pub fn clean_name(raw: &str) -> Option<String> {
    let name = tidy(raw);
    (!name.is_empty()).then(|| clip(&name))
}

/// At most [`MAX_NAME`] characters (not bytes), never empty.
fn clip(name: &str) -> String {
    let name: String = name.chars().take(MAX_NAME).collect();
    let name = name.trim().to_owned();
    if name.is_empty() {
        FALLBACK_NAME.to_owned()
    } else {
        name
    }
}

/// A member's display name made fit for a channel name: no line breaks, at most 40 characters.
pub fn owner_label(name: &str, fallback: &str) -> String {
    let name: String = tidy(name).chars().take(40).collect();
    let name = name.trim().to_owned();
    if name.is_empty() { fallback.to_owned() } else { name }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_name_rooms_after_their_owner() {
        assert_eq!(room_name(DEFAULT_TEMPLATE, "Mira", 1), "Mira's room");
        assert_eq!(room_name("Room {n} · {user}", "Mira", 3), "Room 3 · Mira");
        // The owner's name is not expanded again.
        assert_eq!(room_name("{user}", "{n}", 7), "{n}");
        assert_eq!(room_name("   ", "Mira", 1), FALLBACK_NAME);
        let long = room_name("{user}", &"x".repeat(300), 1);
        assert_eq!(long.chars().count(), MAX_NAME);
        // Characters, not bytes: a name of emoji is cut at a boundary.
        assert_eq!(room_name("{user}", &"🦀".repeat(300), 1).chars().count(), MAX_NAME);
        assert_eq!(room_name("{user}", "line\nbreak\tand  gaps", 1), "line break and gaps");
    }

    #[test]
    fn templates_are_checked() {
        assert_eq!(check_template("  {user}'s   room ").unwrap(), "{user}'s room");
        assert!(check_template("").is_err());
        assert!(check_template("   ").is_err());
        assert!(check_template(&"x".repeat(MAX_TEMPLATE + 1)).is_err());
        assert!(check_template("Hello {nick}").is_err());
        assert!(check_template("Hello {user").is_err());
        assert!(check_template("Hello } {user}").is_err());
        assert!(check_template("Hello {user} }").is_err());
        assert!(check_template("{user} {n}").is_ok());
        // A room that needs no placeholder is fine.
        assert_eq!(check_template("Lounge").unwrap(), "Lounge");
    }

    #[test]
    fn limits_and_counts_are_checked() {
        assert_eq!(check_limit(0).unwrap(), 0);
        assert_eq!(check_limit(99).unwrap(), 99);
        assert!(check_limit(100).is_err());
        assert!(check_limit(-1).is_err());
        assert!(check_limit(i64::MAX).is_err());
        assert_eq!(check_per_user(1).unwrap(), 1);
        assert!(check_per_user(0).is_err());
        assert!(check_per_user(6).is_err());
    }

    #[test]
    fn typed_names_are_cleaned() {
        assert_eq!(clean_name("  Study\nhall  ").as_deref(), Some("Study hall"));
        assert_eq!(clean_name("\n \t").as_deref(), None);
        assert_eq!(clean_name(&"y".repeat(500)).unwrap().chars().count(), MAX_NAME);
        assert_eq!(owner_label("  \n ", "Someone"), "Someone");
        assert_eq!(owner_label("  \n ", "Birisi"), "Birisi");
        assert_eq!(owner_label(&"z".repeat(90), "Someone").chars().count(), 40);
    }
}
