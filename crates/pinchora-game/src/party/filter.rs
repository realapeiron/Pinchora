//! The small profanity filter the party games run member text through. A base list (strong words and
//! slurs) always applies; a server's strict mode adds mild words and insults. It reads whole words,
//! so "class" and "this hit" pass, and it undoes the usual disguises: numbers for letters, stretched
//! letters, words spelled out with dots or spaces, accented and look-alike letters, invisible characters,
//! a few misspellings and letters starred out ("f*ck"). Short stems are only caught where a swear word
//! really carries them (the start of "shitty", the end of "bullshit"), never in the middle of a name.

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
    "fuk",
    "fukk",
    "fuker",
    "fukker",
    "fuking",
    "fukking",
    "fukin",
    "fukkin",
    "fuked",
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

/// Strong words and their misspellings, caught wherever they sit in a longer word ("fuuuckkk",
/// "bignigga"): no name or word holds them.
const ANYWHERE: &[&str] = &["fuck", "phuck", "fvck", "bitch", "nigger", "nigga", "niqqer", "niqqa"];

/// Stems caught at the start of a longer word ("shitty", "shithead", "faggots", "niggaz") and not in
/// the middle of one: "Matsushita" and "Nishitani" carry the letters and nothing rude.
const STARTS: &[&str] = &["shit", "niggu", "fagg"];

/// Stems caught at the end of a longer word ("bullshit", "dipshits", "bullshitting").
const ENDS: &[&str] = &["shit", "shits", "shitter", "shitters", "shitting", "shitted"];

/// Real words that a stem rule would catch: to snigger is to snicker, a niggard is a miser, and a mishit
/// is a bad stroke.
const EXEMPT: &[&str] = &[
    "snigger",
    "sniggers",
    "sniggered",
    "sniggering",
    "niggard",
    "niggards",
    "niggardly",
    "niggardliness",
    "mishit",
    "mishits",
    "mishitting",
    "shitake",
    "shitakes",
];

/// The letter a character stands for when it is used as a disguise, if it is one. A one, an exclamation
/// mark and a bar can be an `i` or an `l` ("sh!t", "s1ut"): `one` says which this reading takes.
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
        'ѕ' => 's',
        'ј' => 'j',
        _ => c,
    })
}

/// The words of `text` as the filter reads them: lowercase, look-alikes and disguises undone (`one` is
/// what a one or a bar stands for), runs of single letters joined ("f u c k"), and any letter repeated
/// three or more times cut to one.
fn words(text: &str, one: char) -> Vec<String> {
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

/// Whether `token` is a listed word, or carries a stem where that stem is a swear word.
fn blocked(token: &str, strict: bool) -> bool {
    if BASE.contains(&token) || (strict && STRICT.contains(&token)) {
        return true;
    }
    !EXEMPT.contains(&token)
        && (ANYWHERE.iter().any(|stem| token.contains(stem))
            || STARTS.iter().any(|stem| token.starts_with(stem))
            || ENDS.iter().any(|stem| token.ends_with(stem)))
}

/// The lowercase letters of a whitespace-free chunk, when it is nothing else: a few letters of a word
/// that was cut up with spaces ("fu ck"). Anything with a stop, an apostrophe or a number in it is not
/// a piece of one, so "it's hit" and "Sh, it" stay apart.
fn piece(chunk: &str) -> Option<String> {
    let letters: String = chunk
        .chars()
        .filter_map(|c| plain(c.to_lowercase().next().unwrap_or(c)))
        .collect();
    let short = (1..=3).contains(&letters.chars().count());
    (short && letters.chars().all(|c| c.is_ascii_alphabetic())).then_some(letters)
}

/// Whether short pieces of a word, set apart by spaces only, spell a blocked one together ("fu ck",
/// "sh it", "f uck"). Only the whole words and the stems that hold anywhere count here: joined words
/// would otherwise spell "shit" out of "was hit".
fn spaced(text: &str, strict: bool) -> bool {
    let chunks: Vec<Option<String>> = text.split_whitespace().map(piece).collect();
    chunks.split(Option::is_none).any(|run| {
        let run: Vec<&str> = run.iter().flatten().map(String::as_str).collect();
        (2..=3).any(|size| {
            run.windows(size).any(|pieces| {
                let joined = squeeze(&pieces.concat());
                BASE.contains(&joined.as_str())
                    || (strict && STRICT.contains(&joined.as_str()))
                    || ANYWHERE.iter().any(|stem| joined.contains(stem))
            })
        })
    })
}

/// A chunk of text as a pattern of letters and `*`, when it holds a star and nothing but letters
/// (disguised ones included) round it; punctuation at either end is left off.
fn mask(chunk: &str) -> Option<Vec<char>> {
    let chars: Vec<char> = chunk
        .chars()
        .filter_map(|c| plain(c.to_lowercase().next().unwrap_or(c)))
        .collect();
    let keep = |c: &char| c.is_alphanumeric() || *c == '*';
    let start = chars.iter().position(keep)?;
    let end = chars.iter().rposition(keep)?;
    let pattern: Option<Vec<char>> = chars[start..=end]
        .iter()
        .map(|&c| match c {
            '*' => Some('*'),
            c if c.is_ascii_alphabetic() => Some(c),
            other => disguise(other, 'i'),
        })
        .collect();
    pattern.filter(|p| p.contains(&'*'))
}

/// Whether `window` is `word` with some letters starred out, and shows enough of it to say so: two
/// letters, or the first of three or more stars ("f***").
fn starred(window: &[char], word: &str) -> bool {
    let literal = window.iter().filter(|c| **c != '*').count();
    let stars = window.len() - literal;
    let shows = literal >= 2 || (literal == 1 && window[0] != '*' && stars >= 3);
    shows
        && stars >= 1
        && window.len() == word.len()
        && window.iter().zip(word.chars()).all(|(p, w)| *p == '*' || *p == w)
}

/// Whether a swear word has letters starred out in `text`: "f*ck", "sh*t", "b*tch", "f***".
fn masked(text: &str, strict: bool) -> bool {
    text.split_whitespace().filter_map(mask).any(|pattern| {
        let whole = BASE
            .iter()
            .chain(strict.then_some(STRICT).into_iter().flatten())
            .any(|word| starred(&pattern, word));
        let anywhere = ANYWHERE
            .iter()
            .any(|stem| pattern.windows(stem.len()).any(|window| starred(window, stem)));
        let starts = STARTS
            .iter()
            .any(|stem| pattern.len() >= stem.len() && starred(&pattern[..stem.len()], stem));
        let ends = ENDS
            .iter()
            .any(|stem| pattern.len() >= stem.len() && starred(&pattern[pattern.len() - stem.len()..], stem));
        whole || anywhere || starts || ends
    })
}

/// Whether `text` holds a word the filter blocks; `strict` adds the milder list.
pub fn is_profane(text: &str, strict: bool) -> bool {
    // A one, an exclamation mark or a bar is an `i` or an `l` ("sh!t", "s1ut"): both readings are tried.
    let readings: &[char] = if text.contains(['1', '!', '|']) {
        &['i', 'l']
    } else {
        &['i']
    };
    let read = readings
        .iter()
        .any(|&one| words(text, one).iter().any(|token| blocked(token, strict)));
    // Two words that only make a blocked one together are not looked for: "this hit" stays fine.
    read || spaced(text, strict) || masked(text, strict) || (strict && text.to_lowercase().contains("shut up"))
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
    fn real_words_and_names_that_carry_a_swear_stem_pass() {
        for text in [
            "sniggering",
            "She sniggered at the snigger",
            "niggle",
            "niggling doubts",
            "a niggardly offer",
            "Matsushita",
            "Takeshita and Nishitani",
            "Yoshitaka, Ashitaka",
            "a mishit and a shitake",
            "Fukushima",
            "Is there a FAQ?",
            "He was hit",
            "it's hit the fan",
            "Sh, it's late",
            "who're you",
            "cumulus clouds",
            "a bit chilly",
            "the stuff of us fun day",
        ] {
            assert!(!is_profane(text, false), "{text}");
            assert!(!is_profane(text, true), "{text} stays fine when strict");
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
    fn misspellings_spaced_and_starred_spellings_are_caught() {
        for text in [
            // Misspelled.
            "phuck",
            "what the phuuuck",
            "fvck you",
            "fuk",
            "fukk this",
            "so fuking good",
            "niqqer",
            "n1qqer",
            // A one for an l.
            "s1ut",
            "$lut",
            "you s1ut",
            // Cut up with spaces.
            "fu ck",
            "f uck",
            "sh it",
            "nig ger",
            "what the fu ck",
            // Starred out.
            "f*ck",
            "f**k",
            "f***",
            "what the f*cking",
            "sh*t",
            "sh*thead",
            "b*tch",
            "n*****",
            "(f*ck)",
            // Stems where a swear word carries them.
            "bullshit",
            "dipshit",
            "horseshits",
            "shitty",
            "shithead",
            "niggaz",
            "faggots",
            "bignigga",
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
    fn starred_text_needs_enough_of_a_word_to_be_one() {
        // Stars alone, formatting round a harmless word, sums and a star between two numbers say nothing.
        for text in [
            "***",
            "**bold** and *italic*",
            "2*3 is 6",
            "a*b",
            "wow ****",
            "5 stars **",
        ] {
            assert!(!is_profane(text, false), "{text}");
        }
        // The mild list is starred out too, but only for a server that wants it.
        assert!(!is_profane("d*mn", false));
        assert!(is_profane("d*mn", true));
    }

    #[test]
    fn a_lone_exclamation_is_not_a_letter() {
        assert!(!is_profane("wow! amazing! ! !", false));
        assert!(!is_profane("1 2 3 go", false));
    }
}
