//! Cleaning what members type into the party games before anybody sees it.

use pinchora_core::i18n::Lang;
use pinchora_game::party::filter::is_profane_for;

use crate::util::strip_mentions;

/// Why a piece of text was turned away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refused {
    /// Nothing left once mentions, links and formatting were taken out.
    Empty,
    Profane,
}

impl Refused {
    pub(crate) fn say(self) -> &'static str {
        match self {
            Self::Empty => {
                "There is nothing left to show once links, mentions and formatting are taken out. Try again with plain words!"
            }
            Self::Profane => {
                "Let's keep it friendly: that one has a word this table doesn't allow. Try different words!"
            }
        }
    }
}

/// Characters that would turn text into formatting (bold, spoilers, quotes, headers, code) when shown.
fn is_markup(c: char) -> bool {
    matches!(c, '*' | '_' | '~' | '|' | '`' | '>' | '\\' | '#') || c.is_control() && c != '\n'
}

/// Drops `<...>` spans: user and role mentions that survived, channel links, custom emoji, timestamps.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('>') {
            Some(end) if end <= 64 && !after[..end].contains(char::is_whitespace) => rest = &after[end + 1..],
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Whether a word is, or holds, a link or an invite.
fn is_link(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    lower.contains("://")
        || lower.starts_with("www.")
        || lower.contains("discord.gg")
        || lower.contains("discord.com/")
        || lower.contains("discordapp.com/")
        || lower.contains(".gg/")
}

/// Member text made safe for a public message: mentions, tags, links and formatting taken out, spaces
/// tidied, and refused when it holds a word the filter blocks (`strict` adds the milder list). The
/// filter reads the text twice, with its formatting still on (so "f*ck" shows its star) and without.
pub(crate) fn clean(text: &str, strict: bool) -> Result<String, Refused> {
    clean_for(text, strict, Lang::En)
}

/// [`clean`] for text typed in a game in `lang`.
pub(crate) fn clean_for(text: &str, strict: bool, lang: Lang) -> Result<String, Refused> {
    let plain = strip_tags(&strip_mentions(text));
    let kept: Vec<&str> = plain.split_whitespace().filter(|word| !is_link(word)).collect();
    let words: Vec<String> = kept
        .iter()
        .map(|word| word.chars().filter(|c| !is_markup(*c)).collect::<String>())
        .filter(|word| !word.is_empty())
        .collect();
    let cleaned = words.join(" ");
    if cleaned.is_empty() {
        return Err(Refused::Empty);
    }
    if is_profane_for(&cleaned, strict, lang) || is_profane_for(&kept.join(" "), strict, lang) {
        return Err(Refused::Profane);
    }
    Ok(cleaned)
}

/// A name for the table: the member's own, cleaned and at most 24 characters; a rude one is swapped
/// for a plain label rather than shown.
pub(crate) fn name(raw: &str) -> String {
    let cleaned = clean(raw, false).unwrap_or_default();
    let short: String = cleaned.chars().take(24).collect();
    if short.trim().is_empty() {
        "Player".to_owned()
    } else {
        short
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_links_tags_and_formatting_are_taken_out() {
        assert_eq!(
            clean("hello <@123> and @everyone, see https://evil.example/x ok", false).unwrap(),
            "hello and everyone, see ok"
        );
        assert_eq!(clean("join discord.gg/abc now", false).unwrap(), "join now");
        assert_eq!(
            clean("**bold** __under__ ||spoiler|| `code` > quote", false).unwrap(),
            "bold under spoiler code quote"
        );
        assert_eq!(
            clean("a <:custom:12345> b <#99> c <t:1700000000:R> d", false).unwrap(),
            "a b c d"
        );
        assert_eq!(clean("x < y and y > z", false).unwrap(), "x < y and y z");
        assert_eq!(clean("  too   many    spaces  ", false).unwrap(), "too many spaces");
        assert_eq!(clean("line one\nline two", false).unwrap(), "line one line two");
    }

    #[test]
    fn nothing_left_or_rude_text_is_refused() {
        assert_eq!(clean("   ", false), Err(Refused::Empty));
        assert_eq!(clean("<@1> https://x.example", false), Err(Refused::Empty));
        assert_eq!(clean("what the fuck", false), Err(Refused::Profane));
        assert_eq!(clean("oh damn", false).unwrap(), "oh damn");
        assert_eq!(clean("oh damn", true), Err(Refused::Profane));
        // A rude word hidden inside markup, or starred out, is still seen.
        assert_eq!(clean("f**k", false), Err(Refused::Profane));
        assert_eq!(clean("what the f*cking hell", false), Err(Refused::Profane));
        assert_eq!(clean("**shit**", false), Err(Refused::Profane));
        assert_eq!(clean("fu ck", false), Err(Refused::Profane));
        // Formatting round a harmless word is only taken out.
        assert_eq!(clean("**bold** *and* ~~gone~~", false).unwrap(), "bold and gone");
        // A link that is dropped does not count against the text round it.
        assert_eq!(clean("see https://x.example/porn ok", false).unwrap(), "see ok");
    }

    #[test]
    fn names_are_short_and_never_rude() {
        assert_eq!(name("Captain Pinch"), "Captain Pinch");
        assert_eq!(name(&"x".repeat(60)).chars().count(), 24);
        assert_eq!(name("<@5> "), "Player");
        assert_eq!(name("shithead"), "Player");
        assert_eq!(name("Matsushita"), "Matsushita");
        assert_eq!(name("f*ck"), "Player");
        assert_eq!(name("@everyone"), "everyone");
    }
}
