//! Discord-independent EN/TR policy; preserve accepted Unicode text exactly.
use pinchora_core::infra::chat_text;
use pinchora_core::infra::i18n::Lang;
pub fn check(text: &str) -> Result<(), &'static str> {
    chat_text::check(text)?;
    if crate::party::filter::is_profane(text, false) || crate::party::filter::is_profane_for(text, true, Lang::Tr) {
        return Err("chat_filtered");
    }
    Ok(())
}
pub fn display_name(text: &str) -> String {
    if chat_text::graphemes(text) > 32 || check(text).is_err() {
        "🐚".into()
    } else {
        text.trim().into()
    }
}
pub fn fingerprint(text: &str) -> String {
    let words = pinchora_core::moderation::automod::words(text, 'i').join(" ");
    if words.is_empty() {
        text.trim().to_lowercase()
    } else {
        words
    }
}
#[cfg(test)]
mod tests {
    use crate::chat::*;
    #[test]
    fn english_turkish_filter_table() {
        for text in [
            "Merhaba dünya",
            "Hello reef",
            "İyi akşamlar",
            "class shell",
            "güzel gün",
            "👩🏽‍🚀",
        ] {
            assert!(check(text).is_ok(), "{text}");
        }
        for text in [
            "f.u.c.k",
            "siktir",
            "aptal",
            "https://example.com",
            "`discord.gg/reef`",
            "a@example.com",
            "+90 (555) 123 4567",
            "@everyone",
            "ｈｔｔｐｓ：／／example.com",
            "fu\u{200b}ck",
        ] {
            assert!(check(text).is_err(), "{text}");
        }
    }
    #[test]
    fn length_and_names() {
        assert!(check(&"e\u{301}".repeat(200)).is_ok());
        assert_eq!(check(&"x".repeat(201)), Err("chat_length"));
        assert_eq!(display_name("discord.gg/reef"), "🐚");
        assert_eq!(display_name("Deniz"), "Deniz");
    }
}
