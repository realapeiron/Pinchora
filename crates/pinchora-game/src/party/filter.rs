//! The small profanity filter the party games run member text through. A base list (strong words and
//! slurs) always applies; a server's strict mode adds mild words and insults. It reads whole words,
//! so "class" and "this hit" pass, and it undoes the usual disguises: numbers for letters, stretched
//! letters, words spelled out with dots or spaces, accented and look-alike letters, invisible characters.

/// Words nobody types in a family friendly game: strong profanity and slurs.
const BASE: &[&str] = &[
    "fuck",
    "fucker",
    "fucking",
    "fucked",
    "shit",
    "shitty",
    "bullshit",
    "bitch",
    "cunt",
    "asshole",
    "whore",
    "slut",
    "twat",
    "wanker",
    "dickhead",
    "motherfucker",
    "nigger",
    "nigga",
    "faggot",
    "fag",
    "retard",
    "retarded",
    "tranny",
    "chink",
    "spic",
    "kike",
    "porn",
    "rape",
    "rapist",
    "nazi",
    "cum",
];

/// What strict mode adds: milder words and name-calling.
const STRICT: &[&str] = &[
    "damn", "hell", "crap", "piss", "pissed", "dick", "cock", "bastard", "ass", "arse", "sucks", "stupid", "idiot",
    "dumb", "moron", "loser", "ugly", "hate", "kill", "die", "dead", "sex", "sexy", "boob", "boobs", "butt", "poop",
    "fart", "drunk", "beer", "wine", "vodka", "weed", "drug", "drugs", "gun",
];

/// Strong words that are caught even inside a longer one ("fuuuckkk", "shithead").
const INSIDE: &[&str] = &["fuck", "shit", "nigg", "fagg", "bitch"];

/// The letter a character stands for when it is used as a disguise, if it is one.
fn disguise(c: char) -> Option<char> {
    Some(match c {
        '0' => 'o',
        '1' | '!' | '|' => 'i',
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
        'ѕ' => 's',
        'ј' => 'j',
        _ => c,
    })
}

/// The words of `text` as the filter reads them: lowercase, look-alikes and disguises undone, runs of
/// single letters joined ("f u c k"), and any letter repeated three or more times cut to one.
fn words(text: &str) -> Vec<String> {
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
            '!' | '|' if !current.is_empty() && chars.get(i + 1).is_some_and(char::is_ascii_alphabetic) => disguise(c),
            '!' | '|' => None,
            other => disguise(other),
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

/// Whether `text` holds a word the filter blocks; `strict` adds the milder list.
pub fn is_profane(text: &str, strict: bool) -> bool {
    let tokens = words(text);
    let listed = |token: &str| BASE.contains(&token) || (strict && STRICT.contains(&token));
    if tokens
        .iter()
        .any(|token| listed(token) || INSIDE.iter().any(|bad| token.contains(bad)))
    {
        return true;
    }
    // Two words that only make a blocked one together are not looked for: "this hit" stays fine.
    strict && text.to_lowercase().contains("shut up")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_passes() {
        for text in [
            "A pearl in every pocket",
            "Class assignments are boring",
            "this hit the spot",
            "Scunthorpe never did anything wrong",
            "Moby Dick was a whale",
            "I will pass it on",
            "Shiitake mushrooms",
            "grape",
            "assist the hello world",
            "classic cocktail",
        ] {
            assert!(!is_profane(text, false), "{text}");
        }
    }

    #[test]
    fn strong_words_and_their_disguises_are_caught() {
        for text in [
            "what the fuck",
            "SHIT",
            "you b1tch",
            "f u c k",
            "f.u.c.k",
            "fuuuuck",
            "sh!t happens",
            "$h1t",
            "holy bullshit",
            "motherfucker",
            "a n1gga",
        ] {
            assert!(is_profane(text, false), "{text}");
        }
    }

    #[test]
    fn strict_mode_adds_the_mild_list() {
        for text in ["oh damn", "what the hell", "you loser", "Shut up!", "that sucks"] {
            assert!(!is_profane(text, false), "{text} is mild");
            assert!(is_profane(text, true), "{text} is blocked when strict");
        }
        // Hell inside a word is a place, not a curse.
        assert!(!is_profane("hello shell helloween", true));
        assert!(!is_profane("assistant classes", true));
    }

    #[test]
    fn look_alike_and_invisible_letters_are_read_as_plain_ones() {
        for text in [
            "fück",
            "fu\u{441}k",
            "ｆｕｃｋ",
            "fu\u{200B}ck",
            "f\u{AD}uck",
            "fu\u{308}ck",
            "sh\u{456}t",
            "n\u{456}gger",
        ] {
            assert!(is_profane(text, false), "{text:?}");
        }
        assert!(!is_profane("Café crème brûlée, Ελλάδα, Привет", false));
    }

    #[test]
    fn a_lone_exclamation_is_not_a_letter() {
        assert!(!is_profane("wow! amazing! ! !", false));
        assert!(!is_profane("1 2 3 go", false));
    }
}
