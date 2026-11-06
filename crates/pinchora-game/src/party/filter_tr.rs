//! The Turkish part of the word filter. Turkish text is read on its own terms: the English filter folds
//! `ü` to `u`, which would make "gün" (day) read as "gun" for a strict server, and chops words at `ı`, `ğ`
//! and `ş`. Words are lowercased with Turkish rules, and the usual disguises are undone: the letters with
//! marks written plain (`ş` as `s`, `ğ` as `g`, `ç` as `c`, `ö` as `o`, `ü` as `u`), numbers for letters,
//! stretched letters, letters spaced or starred out. The dotless `ı` and dotted `i` are never mixed up,
//! because "sıkış" (a squeeze) is not "sikiş".

/// Rude words that carry marks. Matched whole, in the spelling given: their plain-letter spellings are
/// ordinary words ("got", "pic").
const EXACT: &[&str] = &[
    "göt",
    "götü",
    "götüne",
    "götüm",
    "götveren",
    "götlek",
    "piç",
    "piçler",
    "piçin",
    "oç",
    "amına",
    "amcık",
    "taşak",
    "taşşak",
    "sürtük",
    "yavşak",
    "şerefsiz",
    "fahişe",
    "puşt",
    "sıç",
    "sıçtı",
    "sıçmak",
    "sıçar",
    "sıçarım",
    "sıçayım",
    "sıçtığım",
    "kıro",
    "ibneler",
    "orospuçocuğu",
    "ananı",
    "amınakoyim",
    "amınakoyayım",
    "amınakodum",
];

/// Rude words in plain letters (marks folded, `i` and `ı` kept apart), matched whole.
const BASE: &[&str] = &[
    "amk",
    "aq",
    "amq",
    "sik",
    "sikim",
    "sikik",
    "sikis",
    "siktir",
    "siktirgit",
    "sikeyim",
    "sikerim",
    "siker",
    "sikti",
    "sikmek",
    "sikmeyim",
    "orospu",
    "orspu",
    "oruspu",
    "yarak",
    "yarrak",
    "amcik",
    "amcigi",
    "amina",
    "aminakoyim",
    "aminakoyayim",
    "gavat",
    "kahpe",
    "pezevenk",
    "ibne",
    "pust",
    "tasak",
    "tassak",
    "kaltak",
    "surtuk",
    "yavsak",
    "serefsiz",
    "fahise",
    "anani",
    "ananisikeyim",
    "dalyarak",
];

/// Strong stems caught wherever they sit in a longer word.
const ANYWHERE: &[&str] = &[
    "orospu", "pezevenk", "yarrak", "kahpe", "gavat", "amcik", "siktir", "sikeyim", "sikerim",
];

/// Stems caught at the start of a longer word (a plain `sikt`, `sikey` is never an innocent word start).
const STARTS: &[&str] = &["sikt", "sikey", "siker", "sikik", "sikim", "sikis"];

/// What strict mode adds: milder words and name-calling, in the spelling given.
const STRICT: &[&str] = &[
    "salak",
    "aptal",
    "gerizekalı",
    "dangalak",
    "ahmak",
    "budala",
    "enayi",
    "embesil",
    "beyinsiz",
    "ezik",
    "mal",
    "bok",
    "boktan",
    "boku",
    "gavur",
    "zenci",
    "öl",
    "öldür",
    "öldürmek",
    "geber",
    "gebertmek",
    "intihar",
    "silah",
    "tabanca",
    "uyuşturucu",
    "esrar",
    "bira",
    "rakı",
    "şarap",
    "votka",
    "viski",
    "sigara",
    "seks",
    "seksi",
    "çıplak",
    "kıç",
    "kaka",
    "osuruk",
    "çiş",
    "nefret",
];

/// The letter a disguise stands for: a number or sign inside a word.
fn disguise(c: char) -> Option<char> {
    Some(match c {
        '0' => 'o',
        '1' | '!' | '|' => 'i',
        '3' => 'e',
        '4' | '@' => 'a',
        '5' | '$' => 's',
        '7' => 't',
        _ => return None,
    })
}

/// Turkish lowercase, with the invisible characters dropped.
fn lower(c: char) -> Option<char> {
    Some(match c {
        'I' => 'ı',
        'İ' => 'i',
        '\u{AD}'
        | '\u{300}'..='\u{36F}'
        | '\u{180E}'
        | '\u{200B}'..='\u{200F}'
        | '\u{2060}'..='\u{2064}'
        | '\u{FEFF}' => {
            return None;
        }
        c => c.to_lowercase().next().unwrap_or(c),
    })
}

/// A marked letter written plain; `ı` and `i` stay as they are.
fn fold(c: char) -> char {
    match c {
        'ç' => 'c',
        'ğ' => 'g',
        'ö' => 'o',
        'ş' => 's',
        'ü' => 'u',
        c => c,
    }
}

/// Any letter repeated three or more times cut to one; doubles stay.
fn squeeze(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    let mut out = String::with_capacity(word.len());
    let mut i = 0;
    while i < chars.len() {
        let mut j = i;
        while j < chars.len() && chars[j] == chars[i] {
            j += 1;
        }
        out.push(chars[i]);
        if j - i == 2 {
            out.push(chars[i]);
        }
        i = j;
    }
    out
}

/// The words of `text`: lowercase, disguises undone, runs of single letters joined ("s i k t i r").
fn words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().filter_map(lower).collect();
    let mut raw: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let before = i.checked_sub(1).is_some_and(|p| chars[p].is_alphabetic());
        let after = chars.get(i + 1).is_some_and(|n| n.is_alphabetic());
        let letter = if c.is_alphabetic() {
            Some(c)
        } else if matches!(c, '!' | '|') {
            // "s!ktir", not "wow!": only in the middle of a word.
            (before && after).then(|| disguise(c)).flatten()
        } else if before || after {
            disguise(c)
        } else {
            None
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
    let mut joined: Vec<String> = Vec::new();
    let mut run = String::new();
    for word in raw {
        if word.chars().count() == 1 {
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

fn blocked(token: &str, strict: bool) -> bool {
    let key: String = token.chars().map(fold).collect();
    EXACT.contains(&token)
        || BASE.contains(&key.as_str())
        || (strict && STRICT.contains(&token))
        || ANYWHERE.iter().any(|stem| key.contains(stem))
        || STARTS.iter().any(|stem| key.starts_with(stem))
}

/// A chunk as a pattern of letters and `*` (the stars standing for hidden letters), when it holds one.
fn mask(chunk: &str) -> Option<Vec<char>> {
    let chars: Vec<char> = chunk.chars().filter_map(lower).map(fold).collect();
    let keep = |c: &char| c.is_alphanumeric() || *c == '*';
    let start = chars.iter().position(keep)?;
    let end = chars.iter().rposition(keep)?;
    let pattern: Option<Vec<char>> = chars[start..=end]
        .iter()
        .map(|&c| match c {
            '*' => Some('*'),
            c if c.is_alphabetic() => Some(c),
            other => disguise(other),
        })
        .collect();
    pattern.filter(|p| p.contains(&'*'))
}

/// Whether `window` is `word` with letters starred out and shows at least two of them.
fn starred(window: &[char], word: &str) -> bool {
    let literal = window.iter().filter(|c| **c != '*').count();
    literal >= 2
        && literal < window.len()
        && window.len() == word.chars().count()
        && window.iter().zip(word.chars()).all(|(p, w)| *p == '*' || *p == w)
}

fn masked(text: &str) -> bool {
    text.split_whitespace().filter_map(mask).any(|pattern| {
        BASE.iter().any(|word| starred(&pattern, word))
            || ANYWHERE
                .iter()
                .any(|stem| pattern.windows(stem.chars().count()).any(|w| starred(w, stem)))
    })
}

/// Whether `text` holds a Turkish word the filter blocks; `strict` adds the milder list.
pub fn is_profane(text: &str, strict: bool) -> bool {
    words(text).iter().any(|token| blocked(token, strict)) || masked(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everyday_turkish_passes_even_for_strict_servers() {
        for text in [
            "Her gün çay içerim",
            "sıkıntı yok, sıkış tıkış otobüs",
            "bugün hava çok güzel",
            "Sıkı çalış, sıkıcı bir film",
            "sıçan ve sıçrama",
            "kızılcık sıkımı",
            "Amasya'da elma, amcam geldi",
            "tasarım ve taşıt",
            "piyano, pilav, pişti",
            "göz, gül, götürmek",
            "bokeh ve boğaz",
            "malzeme ve mandalina",
            "Işık İstanbul'da şakıyor",
            "ananas ve anahtar",
            "hamsi, simit, ayran",
            "silgi ve kalem",
        ] {
            assert!(!is_profane(text, false), "{text}");
            assert!(!is_profane(text, true), "{text} stays fine when strict");
        }
    }

    #[test]
    fn strong_words_and_their_disguises_are_caught() {
        for text in [
            "siktir git",
            "S1KT1R",
            "s i k t i r",
            "s.i.k.t.i.r",
            "siiiiktir",
            "orospu",
            "OROSPU çocuğu",
            "oruspu",
            "amk",
            "AMK!",
            "aq",
            "sikeyim",
            "sikerim seni",
            "yarrak",
            "pezevenk",
            "kahpe",
            "gavat",
            "göt",
            "Götüne",
            "piç",
            "oç",
            "amına koyayım",
            "amcık",
            "s*ktir",
            "or*spu",
            "a*k",
            "pezev*nk",
            "amınakoyim",
            "yavşak",
            "yavsak",
            "serefsiz",
            "şerefsiz",
            "sürtük",
            "surtuk",
            "taşşak",
            "ibne",
        ] {
            assert!(is_profane(text, false), "{text}");
        }
    }

    #[test]
    fn marks_keep_ordinary_homographs_safe() {
        // The plain spellings of göt and piç are the English "got" and "pic".
        for text in [
            "I got a pic",
            "got it",
            "pic of the day",
            "sıkış",
            "sıkık bir gün",
            "sikke",
        ] {
            assert!(!is_profane(text, false), "{text}");
        }
    }

    #[test]
    fn strict_mode_adds_the_mild_list() {
        for text in [
            "salak",
            "aptal herif",
            "gerizekalı",
            "dangalak",
            "boktan bir gün",
            "uyuşturucu",
            "bira",
            "Seksi",
        ] {
            assert!(!is_profane(text, false), "{text} is mild");
            assert!(is_profane(text, true), "{text} is blocked when strict");
        }
    }
}
