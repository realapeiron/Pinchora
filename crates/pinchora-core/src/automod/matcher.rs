//! Word de-disguise follows the party filter (including word boundaries).
use super::Rule;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

static PATTERNS: LazyLock<Mutex<HashMap<String, regex::Regex>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn compiled(pattern: &str) -> Option<regex::Regex> {
    if let Some(found) = PATTERNS.lock().unwrap().get(pattern) {
        return Some(found.clone());
    }
    let regex = super::compile(pattern).ok()?;
    let mut cache = PATTERNS.lock().unwrap();
    if cache.len() >= 256 {
        cache.clear();
    }
    cache.insert(pattern.to_owned(), regex.clone());
    Some(regex)
}

fn disguise(c: char, one: char) -> Option<char> {
    Some(match c {
        '0' => 'o',
        '1' | '!' | '|' => one,
        '3' => 'e',
        '4' | '@' => 'a',
        '5' | '$' => 's',
        '7' => 't',
        '8' => 'b',
        _ => return None,
    })
}

/// The plain letter a look-alike stands for, so "fück", "fuсk" (a Cyrillic с) and "ｆｕｃｋ" read as
/// "fuck"; `None` for an invisible character (zero-width space, soft hyphen, combining mark), which is dropped.
fn plain(c: char) -> Option<char> {
    Some(match c {
        '\u{AD}'
        | '\u{300}'..='\u{36F}'
        | '\u{180E}'
        | '\u{200B}'..='\u{200F}'
        | '\u{2060}'..='\u{2064}'
        | '\u{FEFF}' => {
            return None;
        }
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        'à'..='å' | 'а' | 'α' => 'a',
        'ç' | 'с' => 'c',
        'è'..='ë' | 'е' | 'ё' | 'ε' => 'e',
        'ì'..='ï' | 'і' | 'ї' | 'ι' => 'i',
        'ò'..='ö' | 'ø' | 'о' | 'ο' => 'o',
        'ù'..='ü' | 'υ' => 'u',
        'ñ' => 'n',
        'ý' | 'ÿ' | 'у' => 'y',
        'к' | 'κ' => 'k',
        'р' | 'ρ' => 'p',
        'х' | 'χ' => 'x',
        'ѕ' | 'ş' => 's',
        'ğ' => 'g',
        'ı' => 'i',
        'ј' => 'j',
        _ => c,
    })
}

/// The words of `text` as the filter reads them: lowercase, look-alikes and disguises undone (`one` is
/// what a one or a bar stands for), runs of single letters joined ("f u c k"), and any letter repeated
/// three or more times cut to one.
pub fn words(text: &str, one: char) -> Vec<String> {
    let chars: Vec<char> = text
        .chars()
        .filter_map(|c| plain(c.to_lowercase().next().unwrap_or(c)))
        .collect();
    let mut raw: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let letter = match c {
            c if c.is_ascii_alphabetic() => Some(c),
            // "!" and "|" are only a disguise in the middle of a word: "sh!t", not "wow!".
            '!' | '|' if !current.is_empty() && chars.get(i + 1).is_some_and(char::is_ascii_alphabetic) => {
                disguise(c, one)
            }
            '!' | '|' => None,
            other => disguise(other, one),
        };
        match letter {
            Some(l) => current.push(l),
            None => {
                if !current.is_empty() {
                    raw.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if !current.is_empty() {
        raw.push(current);
    }
    // "f u c k", "s.h.i.t": single letters in a row make one word.
    let mut joined: Vec<String> = Vec::new();
    let mut run = String::new();
    for word in raw {
        if word.len() == 1 {
            run.push_str(&word);
            continue;
        }
        if !run.is_empty() {
            joined.push(std::mem::take(&mut run));
        }
        joined.push(word);
    }
    if !run.is_empty() {
        joined.push(run);
    }
    joined.into_iter().map(|w| squeeze(&w)).collect()
}

/// Cuts any letter repeated three or more times down to one ("fuuuck" to "fuck"); doubles stay.
fn squeeze(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let chars: Vec<char> = word.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let mut j = i;
        while j < chars.len() && chars[j] == chars[i] {
            j += 1;
        }
        let run = j - i;
        out.push(chars[i]);
        if run == 2 {
            out.push(chars[i]);
        }
        i = j;
    }
    out
}

#[derive(Debug, Default)]
pub struct Input<'a> {
    pub content: &'a str,
    pub mentions: usize,
    pub attachments: usize,
    pub media: bool,
    pub spam: u32,
    pub duplicates: u32,
}

fn domain_in(host: &str, domains: &[String]) -> bool {
    domains
        .iter()
        .any(|d| host == d.to_lowercase() || host.ends_with(&format!(".{}", d.to_lowercase())))
}
/// Hosts without paths, credentials, ports or punctuation around markdown links.
pub fn hosts(text: &str) -> Vec<String> {
    let visible = outside_code(text);
    visible
        .split(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '(' | ')' | '[' | ']' | '"' | '\''))
        .filter_map(|chunk| {
            let chunk = chunk.to_lowercase();
            let chunk = chunk.trim_end_matches([',', '.', '!', '?', ';', ':']);
            let explicit = chunk.starts_with("https://") || chunk.starts_with("http://");
            let chunk = chunk
                .strip_prefix("https://")
                .or_else(|| chunk.strip_prefix("http://"))
                .unwrap_or(chunk);
            if !explicit && chunk.contains('@') {
                return None;
            }
            let host = chunk
                .split(['/', '?', '#'])
                .next()?
                .rsplit('@')
                .next()?
                .split(':')
                .next()?
                .to_lowercase();
            super::domain_valid(&host).then_some(host)
        })
        .collect()
}

fn outside_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut fence = false;
    let mut inline = false;
    while let Some(c) = chars.next() {
        if c == '`' {
            let mut n = 1;
            while chars.peek() == Some(&'`') {
                chars.next();
                n += 1;
            }
            if n >= 3 {
                fence = !fence;
            } else if !fence {
                inline = !inline;
            }
            out.push(' ');
        } else if fence || inline {
            out.push(if c == '\n' { '\n' } else { ' ' });
        } else {
            out.push(c);
        }
    }
    out
}
fn longest_run<T: Eq>(items: impl IntoIterator<Item = T>) -> usize {
    let (mut previous, mut run, mut best) = (None, 0, 0);
    for item in items {
        run = if previous.as_ref() == Some(&item) { run + 1 } else { 1 };
        best = best.max(run);
        previous = Some(item);
    }
    best
}
fn emoji_count(text: &str) -> usize {
    let unicode = text
        .chars()
        .filter(|c| matches!(*c as u32, 0x1f000..=0x1faff | 0x2600..=0x27bf))
        .count();
    let custom = text
        .split('<')
        .skip(1)
        .filter(|part| {
            let Some(p) = part.strip_prefix(':').or_else(|| part.strip_prefix("a:")) else {
                return false;
            };
            let Some((name, rest)) = p.split_once(':') else {
                return false;
            };
            let Some((id, _)) = rest.split_once('>') else {
                return false;
            };
            !name.is_empty() && !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())
        })
        .count();
    unicode + custom
}
fn combining(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f)
}

pub fn matches(rule: &Rule, input: &Input<'_>) -> bool {
    let text = input.content;
    let n = rule.params.threshold as usize;
    match rule.kind.as_str() {
        "spam" => input.spam >= rule.params.threshold,
        "duplicate" => input.duplicates >= rule.params.threshold,
        "repeat" => longest_run(text.chars()) >= n || longest_run(text.split_whitespace().map(str::to_lowercase)) >= n,
        "bad_words" => {
            ['i', 'l'].into_iter().any(|one| {
                let tokens = words(text, one);
                rule.params.words.iter().any(|w| {
                    let needle = words(w, one);
                    !needle.is_empty() && tokens.windows(needle.len()).any(|part| part == needle)
                })
            }) || rule
                .params
                .regex
                .iter()
                .any(|p| compiled(p).is_some_and(|r| r.is_match(text)))
        }
        "invites" => {
            hosts(text).iter().any(|h| domain_in(h, &["discord.gg".into()]))
                || compiled(r"(?i)(?:^|[\s/(<])(?:www\.)?discord(?:app)?\.com/invite/[a-z0-9-]+")
                    .is_some_and(|r| r.is_match(text))
        }
        "links" => hosts(text).iter().any(|h| {
            !domain_in(h, &rule.params.allow_domains)
                && (rule.params.block_domains.is_empty() || domain_in(h, &rule.params.block_domains))
        }),
        "caps" => {
            let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
            letters.len() >= 8 && letters.iter().filter(|c| c.is_uppercase()).count() * 100 >= letters.len() * n
        }
        "emoji" => emoji_count(text) >= n,
        "mentions" => input.mentions >= n,
        "attachments" => input.attachments >= n,
        "newlines" => text.chars().filter(|c| *c == '\n').count() >= n,
        "zalgo" => text.chars().filter(|c| combining(*c)).count() >= n,
        "media_only" => !input.media,
        "youtube_only" => {
            let links = hosts(text);
            links.is_empty()
                || links
                    .iter()
                    .any(|h| !domain_in(h, &["youtube.com".into(), "youtu.be".into()]))
        }
        "honeypot" => true,
        _ => false, // Discord's proprietary preset detector only runs natively.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matcher_table() {
        let mut rule = crate::automod::profile("standard", chrono::Utc::now())
            .unwrap()
            .rules
            .remove(0);
        for (kind, text, n, hit) in [
            ("caps", "HELLO WORLD", 80, true),
            ("caps", "HI!", 50, false),
            ("caps", "Hello world", 50, false),
            ("links", "(https://x.com)", 1, true),
            ("links", "HTTPS://X.COM/path", 1, true),
            ("links", "plain text", 1, false),
            ("emoji", "😀<a:dance:123>🙂", 3, true),
            ("emoji", "<:broken:no>", 1, false),
            ("zalgo", "h\u{301}\u{302}\u{303}", 3, true),
            ("zalgo", "café", 1, false),
            ("repeat", "no no NO", 3, true),
            ("repeat", "hellooooo", 5, true),
            ("invites", "https://discord.gg/example", 1, true),
            ("invites", "https://notdiscord.com/invite/example", 1, false),
            ("invites", "https://discord.com/invite/example", 1, true),
            ("youtube_only", "https://youtube.com.evil.test/x", 1, true),
            ("youtube_only", "https://youtu.be/abc", 1, false),
            ("newlines", "a\nb\nc", 2, true),
        ] {
            rule.kind = kind.into();
            rule.params.threshold = n;
            assert_eq!(
                matches(
                    &rule,
                    &Input {
                        content: text,
                        ..Input::default()
                    }
                ),
                hit,
                "{kind}: {text}"
            );
        }
        rule.kind = "links".into();
        rule.params.allow_domains = vec!["x.com".into()];
        assert!(!matches(
            &rule,
            &Input {
                content: "https://x.com)",
                ..Input::default()
            }
        ));
        assert!(matches(
            &rule,
            &Input {
                content: "https://x.com.bad.test",
                ..Input::default()
            }
        ));
        rule.kind = "duplicate".into();
        rule.params.threshold = 3;
        for (duplicates, expected) in [(2, false), (3, true), (4, true)] {
            assert_eq!(
                matches(
                    &rule,
                    &Input {
                        duplicates,
                        ..Input::default()
                    }
                ),
                expected
            );
        }
    }
    #[test]
    fn links_require_real_hosts_outside_code() {
        let mut rule = crate::automod::profile("strict", chrono::Utc::now())
            .unwrap()
            .rules
            .into_iter()
            .find(|r| r.kind == "links")
            .unwrap();
        rule.params.block_domains.clear();
        for text in [
            "Pi is 3.14",
            "`https://x.com`",
            "```rust\nlet version = 1.23;\nhttps://x.com\n```",
        ] {
            assert!(
                !matches(
                    &rule,
                    &Input {
                        content: text,
                        ..Input::default()
                    }
                ),
                "{text}"
            );
        }
        assert!(matches(
            &rule,
            &Input {
                content: "https://bücher.de/a",
                ..Input::default()
            }
        ));
        rule.params.block_domains = vec!["x.com".into()];
        for text in [
            "https://safe.example/docs/x.com",
            "https://safe.example/?next=x.com",
            "https://user:x.com@safe.example/docs",
        ] {
            assert!(
                !matches(
                    &rule,
                    &Input {
                        content: text,
                        ..Input::default()
                    }
                ),
                "{text}"
            );
        }
        for text in ["[site](https://x.com/path)", "https://user:pass@x.com/path"] {
            assert!(
                matches(
                    &rule,
                    &Input {
                        content: text,
                        ..Input::default()
                    }
                ),
                "{text}"
            );
        }
    }
    #[test]
    fn words_are_dedisguised_without_substring_false_positives() {
        let mut rule = crate::automod::profile("standard", chrono::Utc::now())
            .unwrap()
            .rules
            .remove(3);
        rule.params.words = vec!["bad".into()];
        for text in ["b4d", "ｂａｄ", "b.a.d", "baaaddd", "b\u{200b}ad"] {
            assert!(
                matches(
                    &rule,
                    &Input {
                        content: text,
                        ..Input::default()
                    }
                ),
                "{text}"
            );
        }
        assert!(!matches(
            &rule,
            &Input {
                content: "badge",
                ..Input::default()
            }
        ));
    }
}
