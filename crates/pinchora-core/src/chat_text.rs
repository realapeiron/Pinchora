//! Transport-independent text checks shared by web social surfaces.
use unicode_segmentation::UnicodeSegmentation;

pub fn graphemes(text: &str) -> usize {
    text.graphemes(true).count()
}

pub fn check(text: &str) -> Result<(), &'static str> {
    if text.trim().is_empty() || text.len() > 1600 || graphemes(text) > 200 {
        return Err("chat_length");
    }
    if text
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
    {
        return Err("chat_filtered");
    }
    // Chat is plain text; backticks never exempt a link from AutoMod detection.
    let plain: String = text
        .chars()
        .filter_map(|c| match c {
            '\u{200b}'..='\u{200f}' | '\u{feff}' => None,
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0),
            '`' => Some(' '),
            _ => Some(c),
        })
        .collect();
    let compact: String = plain.chars().filter(|c| !c.is_whitespace()).collect();
    if !crate::automod::hosts(&plain).is_empty()
        || !crate::automod::hosts(&compact).is_empty()
        || plain.contains('@')
        || compact.contains("://")
        || compact.chars().filter(char::is_ascii_digit).count() >= 7
    {
        return Err("chat_filtered");
    }
    Ok(())
}
