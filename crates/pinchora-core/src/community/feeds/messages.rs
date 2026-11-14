use super::*;

/// Why a message or a filter cannot be saved. [`TextError::key`] is the catalog key of the sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextError {
    MessageTooLong,
    TooManyWords,
    WordTooLong,
}

impl TextError {
    pub const fn key(self) -> &'static str {
        match self {
            Self::MessageTooLong => "feeds.message-too-long",
            Self::TooManyWords => "feeds.filter-too-many",
            Self::WordTooLong => "feeds.filter-too-long",
        }
    }
}

/// A feed's message: trimmed, `None` (the kind's default) when empty, at most [`MESSAGE_MAX`] characters.
pub fn check_message(text: Option<&str>) -> Result<Option<String>, TextError> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > MESSAGE_MAX {
        return Err(TextError::MessageTooLong);
    }
    Ok(Some(text.to_owned()))
}

/// Filter words from a comma-separated list (or several entries): trimmed, lower case, no duplicates.
pub fn filter_words<'a>(entries: impl IntoIterator<Item = &'a str>) -> Result<Vec<String>, TextError> {
    let mut words: Vec<String> = Vec::new();
    for word in entries.into_iter().flat_map(|e| e.split(',')) {
        let word = word.trim().to_lowercase();
        if word.is_empty() || words.contains(&word) {
            continue;
        }
        if word.chars().count() > FILTER_WORD_MAX {
            return Err(TextError::WordTooLong);
        }
        words.push(word);
    }
    if words.len() > FILTER_WORDS_MAX {
        return Err(TextError::TooManyWords);
    }
    Ok(words)
}

/// Whether an item's title passes a feed's filters: one of `include` in it (when there are any), none
/// of `exclude`. Capital letters do not matter.
pub fn passes_filters(title: &str, include: &[String], exclude: &[String]) -> bool {
    let title = title.to_lowercase();
    (include.is_empty() || include.iter().any(|w| title.contains(w.as_str())))
        && !exclude.iter().any(|w| title.contains(w.as_str()))
}

/// What fills a message's placeholders, already made safe to post.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    pub title: String,
    pub url: String,
    pub author: String,
    pub account: String,
    pub published: String,
}

/// `template` with each of [`PLACEHOLDERS`] replaced; anything else stays as written.
pub fn render(template: &str, fields: &Fields) -> String {
    let mut out = String::with_capacity(template.len() + 64);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let here = &rest[start..];
        let value = [
            ("{title}", &fields.title),
            ("{url}", &fields.url),
            ("{author}", &fields.author),
            ("{account}", &fields.account),
            ("{published}", &fields.published),
        ]
        .into_iter()
        .find(|(token, _)| here.starts_with(token));
        match value {
            Some((token, value)) => {
                out.push_str(value);
                rest = &here[token.len()..];
            }
            None => {
                out.push('{');
                rest = &here[1..];
            }
        }
    }
    out.push_str(rest);
    out
}
