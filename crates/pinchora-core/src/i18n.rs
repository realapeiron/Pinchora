//! Languages and translated text.
//!
//! Catalogs are TOML files under `crates/pinchora-core/locales/<code>/<area>.toml`, embedded at build
//! time. Tables nest into dotted keys (`[profile]` + `level = "LEVEL"` is `profile.level`), `{name}`
//! is an argument, and a table of `one`/`other` is a plural picked by the `count` argument. A key a
//! language lacks falls back to English. Look text up with [`t!`](crate::t):
//!
//! ```
//! use pinchora_core::{i18n::Lang, t};
//! assert_eq!(t!(Lang::Tr, "card.level"), "SEVİYE");
//! assert_eq!(t!(Lang::En, "daily.streak-days", count = 1), "1 day");
//! ```

use std::{
    collections::{BTreeSet, HashMap},
    fmt::Display,
    sync::LazyLock,
};

/// A language the bot speaks. `En` is the default and the fallback for missing keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lang {
    #[default]
    En,
    Tr,
}
impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Tr];
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Tr => "tr",
        }
    }
    /// Discord's locale string ("tr", "en-US", "en-GB") → our language.
    pub fn from_discord(locale: &str) -> Lang {
        if locale.starts_with("tr") { Lang::Tr } else { Lang::En }
    }
}

impl Lang {
    /// [`Lang::code`] back to a language.
    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code() == code)
    }

    /// The language's name in itself, for menus and replies.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Tr => "Türkçe",
        }
    }

    /// The Discord locales whose command names and descriptions registration fills from this
    /// language's `commands` catalog. English is the commands' own text, so it has none.
    pub fn discord_locales(self) -> &'static [&'static str] {
        match self {
            Lang::En => &[],
            Lang::Tr => &["tr"],
        }
    }
}

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/locales.rs"));
}

/// One catalog entry.
#[derive(Debug)]
enum Entry {
    Text(String),
    /// `one` is used when `count` is 1 (when given), `other` otherwise.
    Plural {
        one: Option<String>,
        other: String,
    },
}

impl Entry {
    fn forms(&self) -> Vec<&str> {
        match self {
            Entry::Text(text) => vec![text],
            Entry::Plural { one, other } => one.iter().map(String::as_str).chain([other.as_str()]).collect(),
        }
    }
}

struct Catalogs {
    entries: HashMap<Lang, HashMap<String, Entry>>,
    /// Files that did not load, keys defined twice, values that are not text.
    problems: Vec<String>,
}

static CATALOGS: LazyLock<Catalogs> = LazyLock::new(|| load(embedded::SOURCES));

fn load(sources: &[(&str, &str, &str)]) -> Catalogs {
    let mut catalogs = Catalogs {
        entries: Lang::ALL.into_iter().map(|l| (l, HashMap::new())).collect(),
        problems: Vec::new(),
    };
    for &(code, area, source) in sources {
        let Some(lang) = Lang::from_code(code) else {
            catalogs
                .problems
                .push(format!("locales/{code}: not a language in `Lang`"));
            continue;
        };
        let table = match source.parse::<toml::Table>() {
            Ok(table) => table,
            Err(error) => {
                catalogs.problems.push(format!("locales/{code}/{area}.toml: {error}"));
                continue;
            }
        };
        let entries = catalogs.entries.get_mut(&lang).expect("every language has a map");
        flatten(&format!("{code}/{area}"), "", table, entries, &mut catalogs.problems);
    }
    catalogs
}

fn flatten(
    file: &str,
    prefix: &str,
    table: toml::Table,
    into: &mut HashMap<String, Entry>,
    problems: &mut Vec<String>,
) {
    for (name, value) in table {
        let key = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}.{name}")
        };
        let entry = match value {
            toml::Value::String(text) => Entry::Text(text),
            toml::Value::Table(table) if is_plural(&table) => {
                let form = |name: &str| table.get(name).and_then(|v| v.as_str()).map(str::to_owned);
                Entry::Plural {
                    one: form("one"),
                    other: form("other").expect("is_plural checked it"),
                }
            }
            toml::Value::Table(table) => {
                flatten(file, &key, table, into, problems);
                continue;
            }
            _ => {
                problems.push(format!("{file}: `{key}` is not text"));
                continue;
            }
        };
        if into.insert(key.clone(), entry).is_some() {
            problems.push(format!("{file}: `{key}` is defined twice"));
        }
    }
}

/// A table of only `one`/`other` strings, `other` required.
fn is_plural(table: &toml::Table) -> bool {
    table.contains_key("other")
        && table
            .iter()
            .all(|(k, v)| matches!(k.as_str(), "one" | "other") && v.is_str())
}

/// The text for `key` in `lang` (else English, else the key itself) with `args` filled in.
/// [`t!`](crate::t) is the short way to call it.
pub fn text(lang: Lang, key: &str, args: &[(&str, &dyn Display)]) -> String {
    let catalogs = &*CATALOGS;
    let entry = catalogs.entries[&lang]
        .get(key)
        .or_else(|| catalogs.entries[&Lang::En].get(key));
    let Some(entry) = entry else {
        tracing::warn!(key, "no text for this key in any language");
        return key.to_owned();
    };
    let template = match entry {
        Entry::Text(text) => text,
        Entry::Plural { one, other } => {
            let count = args
                .iter()
                .find(|(name, _)| *name == "count")
                .and_then(|(_, value)| value.to_string().parse::<i64>().ok());
            match (count, one) {
                (Some(1), Some(one)) => one,
                _ => other,
            }
        }
    };
    fill(template, args)
}

/// The text for `key` in exactly `lang`, without falling back and without arguments filled in;
/// `None` when that language has no such key. For callers that do their own fallback, like
/// command registration.
pub fn exact(lang: Lang, key: &str) -> Option<&'static str> {
    match CATALOGS.entries[&lang].get(key)? {
        Entry::Text(text) => Some(text),
        Entry::Plural { other, .. } => Some(other),
    }
}

/// Every key `lang` defines, sorted.
pub fn keys(lang: Lang) -> Vec<&'static str> {
    let mut keys: Vec<&str> = CATALOGS.entries[&lang].keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys
}

/// The `{argument}` names `key` uses in `lang`, across its plural forms.
pub fn args(lang: Lang, key: &str) -> Option<BTreeSet<String>> {
    let entry = CATALOGS.entries[&lang].get(key)?;
    Some(entry.forms().into_iter().flat_map(placeholders).collect())
}

/// What is wrong with the catalogs: files that did not parse, keys defined twice, a key one
/// language has and another lacks, a key whose arguments differ between languages. Empty when all
/// is well; a unit test keeps it that way. Keys under `cmd.` are checked against the registered
/// commands instead (see pinchora-commands), since English has them in the command definitions.
pub fn problems() -> Vec<String> {
    let mut found = CATALOGS.problems.clone();
    let english: BTreeSet<&str> = keys(Lang::En).into_iter().filter(|k| !k.starts_with("cmd.")).collect();
    for lang in Lang::ALL.into_iter().filter(|l| *l != Lang::En) {
        let theirs: BTreeSet<&str> = keys(lang).into_iter().filter(|k| !k.starts_with("cmd.")).collect();
        for key in english.difference(&theirs) {
            found.push(format!("{}: `{key}` is not translated", lang.code()));
        }
        for key in theirs.difference(&english) {
            found.push(format!(
                "{}: `{key}` is not in English (renamed or removed?)",
                lang.code()
            ));
        }
        for key in english.intersection(&theirs) {
            let (ours, them) = (args(Lang::En, key), args(lang, key));
            if ours != them {
                found.push(format!(
                    "{}: `{key}` takes {:?} but English takes {:?}",
                    lang.code(),
                    them.unwrap_or_default(),
                    ours.unwrap_or_default()
                ));
            }
        }
    }
    found
}

/// `{name}` placeholders in `template`, in order.
fn placeholders(template: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else { break };
        let name = &rest[..end];
        if is_arg_name(name) {
            found.push(name.to_owned());
            rest = &rest[end + 1..];
        }
    }
    found
}

fn is_arg_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `template` with each `{name}` replaced by its argument; an unknown name stays as written.
fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let value = after
            .find('}')
            .map(|end| (&after[..end], end))
            .filter(|(name, _)| is_arg_name(name))
            .and_then(|(name, end)| args.iter().find(|(n, _)| *n == name).map(|(_, v)| (*v, end)));
        match value {
            Some((value, end)) => {
                out.push_str(&value.to_string());
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Translated text: `t!(lang, "key")` or `t!(lang, "key", name = value, count = n)`. Every value
/// only needs [`Display`]; `count` also picks a plural form.
///
/// The arguments are borrowed as `&dyn Display`, which is not `Send`: the `let` drops them before
/// the statement around the macro goes on, so `cx.reply(t!(…)).await` keeps its future `Send`.
#[macro_export]
macro_rules! t {
    ($lang:expr, $key:expr $(,)?) => {
        $crate::i18n::text($lang, $key, &[])
    };
    ($lang:expr, $key:expr, $($name:ident = $value:expr),+ $(,)?) => {{
        let text = $crate::i18n::text($lang, $key, &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+]);
        text
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fails while any key is missing from a language, defined twice, or takes other arguments
    /// there; the message lists every one.
    #[test]
    fn catalogs_match_across_languages() {
        let problems = problems();
        assert!(problems.is_empty(), "catalog problems:\n{}", problems.join("\n"));
    }

    #[test]
    fn missing_keys_fall_back_to_english_then_the_key() {
        let catalogs = load(&[
            ("en", "a", "[x]\nonly = \"English {n}\"\nboth = \"Hello\""),
            ("tr", "a", "[x]\nboth = \"Merhaba\""),
        ]);
        assert!(catalogs.problems.is_empty());
        assert!(matches!(&catalogs.entries[&Lang::Tr]["x.both"], Entry::Text(t) if t == "Merhaba"));
        assert!(!catalogs.entries[&Lang::Tr].contains_key("x.only"));
        // Through the real catalogs: Turkish lacks no key, so fall back on one nobody has.
        assert_eq!(text(Lang::Tr, "no.such.key", &[]), "no.such.key");
        assert_eq!(text(Lang::Tr, "card.level", &[]), "SEVİYE");
        assert_eq!(text(Lang::En, "card.level", &[]), "LEVEL");
    }

    #[test]
    fn arguments_and_plurals_fill_in() {
        assert_eq!(t!(Lang::En, "daily.streak-days", count = 1), "1 day");
        assert_eq!(t!(Lang::En, "daily.streak-days", count = 4), "4 days");
        assert_eq!(t!(Lang::Tr, "daily.streak-days", count = 4), "4 gün");
        let name = String::from("Mira");
        assert_eq!(
            fill("{name} and {name}, {other} {", &[("name", &name)]),
            "Mira and Mira, {other} {"
        );
        assert_eq!(placeholders("a {b} {c_2} {Not} {}"), ["b", "c_2"]);
    }

    #[test]
    fn bad_catalogs_are_reported() {
        let catalogs = load(&[
            ("en", "a", "x = 1"),
            ("en", "b", "[y\n"),
            ("xx", "c", "z = \"\""),
            ("en", "d", "w = \"one\""),
            ("en", "e", "w = \"two\""),
        ]);
        assert_eq!(catalogs.problems.len(), 4, "{:?}", catalogs.problems);
    }

    #[test]
    fn discord_locales_map_to_languages() {
        assert_eq!(Lang::from_discord("tr"), Lang::Tr);
        assert_eq!(Lang::from_discord("en-GB"), Lang::En);
        assert_eq!(Lang::from_discord("de"), Lang::En);
        for lang in Lang::ALL {
            assert_eq!(Lang::from_code(lang.code()), Some(lang));
            for locale in lang.discord_locales() {
                assert_eq!(Lang::from_discord(locale), lang);
            }
        }
    }
}
