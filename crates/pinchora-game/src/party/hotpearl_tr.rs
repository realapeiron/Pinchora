//! Turkish word endings for Hot Pearl. A Turkish word is a stem with a chain of endings whose vowels follow
//! the stem's ("ev-ler", "göz-ler-i", "kitab-ı", "gel-iyor-um"), so the list holds stems and a typed word
//! counts when stripping a few endings leads back to one. Every ending is a template: `A` is a or e, `I` is
//! ı, i, u or ü, `D` is d or t, `C` is c or ç, and `(y)`, `(n)`, `(s)`, `(r)` are letters that only stand
//! between two vowels, `(I)` a vowel that only stands after a consonant. The vowels follow the previous
//! vowel, the stem's own for the first ending.

const VOWELS: &str = "aeıioöuü";

/// Every ending a word can carry, nouns and verbs together.
const ENDINGS: &[&str] = &[
    // Nouns: plural, cases, possession, with/without, "ki", being.
    "lAr",
    "(y)I",
    "(y)A",
    "DA",
    "DAn",
    "(n)In",
    "(y)lA",
    "lI",
    "sIz",
    "lIk",
    "CI",
    "CIk",
    "CA",
    "(I)m",
    "(I)n",
    "(s)I",
    "(I)mIz",
    "(I)nIz",
    "nDA",
    "nDAn",
    "nA",
    "nI",
    "nIn",
    "ki",
    "DIr",
    "(y)DI",
    "(y)mIş",
    "(y)sA",
    // Verbs: infinitive, tenses, persons, moods, voices.
    "mAk",
    "mA",
    "mAz",
    "mAdAn",
    "mAlI",
    "DI",
    "DIm",
    "DIn",
    "DIk",
    "DInIz",
    "DIlAr",
    "mIş",
    "mIştIr",
    "(I)yor",
    "(I)yorum",
    "(I)yorsun",
    "(I)yoruz",
    "(I)yorsunuz",
    "(I)yorlar",
    "(y)AcAk",
    "(y)AcAğIm",
    "(y)AcAksIn",
    "(y)AcAğIz",
    "(y)AcAklAr",
    "Ar",
    "Ir",
    "(r)",
    "ArIm",
    "IrIm",
    "(r)Im",
    "ArsIn",
    "IrsIn",
    "ArlAr",
    "IrlAr",
    "sA",
    "sIn",
    "(y)In",
    "(y)Ip",
    "(y)An",
    "(y)Iş",
    "(y)ArAk",
    "(y)AlIm",
    "(I)l",
    "(I)ş",
    "(I)t",
    "DIr",
    "(y)IcI",
    "(y)Im",
    "(y)Iz",
];

fn last_vowel(text: &str) -> char {
    text.chars().rev().find(|c| VOWELS.contains(*c)).unwrap_or('a')
}

/// The four-way vowel that follows `v`.
fn high(v: char) -> char {
    match v {
        'a' | 'ı' => 'ı',
        'e' | 'i' => 'i',
        'o' | 'u' => 'u',
        _ => 'ü',
    }
}

/// The ending `template` as it is spelled after `stem`.
fn realize(template: &str, stem: &str) -> String {
    let last = stem.chars().last().unwrap_or(' ');
    let after_vowel = VOWELS.contains(last);
    let voiceless = "fstkçşhp".contains(last);
    let mut v = last_vowel(stem);
    let mut out = String::new();
    let mut chars = template.chars();
    while let Some(c) = chars.next() {
        match c {
            '(' => {
                let buffer = chars.next().unwrap_or(' ');
                chars.next();
                if buffer == 'I' {
                    if !after_vowel {
                        v = high(v);
                        out.push(v);
                    }
                } else if after_vowel {
                    out.push(buffer);
                }
            }
            'A' => {
                v = if "aıou".contains(v) { 'a' } else { 'e' };
                out.push(v);
            }
            'I' => {
                v = high(v);
                out.push(v);
            }
            'D' => out.push(if voiceless { 't' } else { 'd' }),
            'C' => out.push(if voiceless { 'ç' } else { 'c' }),
            c => {
                if VOWELS.contains(c) {
                    v = c;
                }
                out.push(c);
            }
        }
    }
    out
}

/// The stems `word` may have come from by taking one ending off, with the consonant a stem softens in
/// front of a vowel ("kitab-ı") put back.
fn strip_once(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut out = Vec::new();
    for cut in 2..chars.len() {
        let stem: String = chars[..cut].iter().collect();
        let tail: String = chars[cut..].iter().collect();
        if ENDINGS.iter().any(|template| realize(template, &stem) == tail) {
            out.push(stem.clone());
            if let Some(hard) = match chars[cut - 1] {
                'b' => Some('p'),
                'c' => Some('ç'),
                'd' => Some('t'),
                'ğ' => Some('k'),
                _ => None,
            } && VOWELS.contains(tail.chars().next().unwrap_or(' '))
            {
                let mut unsoftened: Vec<char> = chars[..cut].to_vec();
                unsoftened[cut - 1] = hard;
                out.push(unsoftened.into_iter().collect());
            }
        }
    }
    out
}

/// The stems `word` may be built on: up to three endings taken off, in every way that fits.
pub fn stems(word: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut layer = vec![word.to_owned()];
    for _ in 0..3 {
        let mut next: Vec<String> = Vec::new();
        for w in &layer {
            for stem in strip_once(w) {
                if !found.contains(&stem) {
                    found.push(stem.clone());
                    next.push(stem);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        layer = next;
    }
    found
}

#[cfg(test)]
mod tests {
    use crate::party::hotpearl_tr::*;

    #[test]
    fn endings_follow_the_vowels_of_the_stem() {
        assert_eq!(realize("lAr", "ev"), "ler");
        assert_eq!(realize("lAr", "kitap"), "lar");
        assert_eq!(realize("(y)I", "araba"), "yı");
        assert_eq!(realize("(y)I", "ev"), "i");
        assert_eq!(realize("(I)m", "göz"), "üm");
        assert_eq!(realize("(s)I", "kapı"), "sı");
        assert_eq!(realize("DA", "kitap"), "ta");
        assert_eq!(realize("DAn", "ev"), "den");
        assert_eq!(realize("(I)yorum", "gel"), "iyorum");
        assert_eq!(realize("(I)yorum", "oku"), "yorum");
        assert_eq!(realize("(y)AcAk", "oku"), "yacak");
        assert_eq!(realize("(r)", "oku"), "r");
        assert_eq!(realize("(r)", "gel"), "");
    }

    #[test]
    fn stripping_endings_finds_the_stems() {
        assert!(stems("evlerde").contains(&"ev".to_owned()));
        assert!(stems("kitabımızdan").contains(&"kitap".to_owned()));
        assert!(stems("gözlerinden").contains(&"göz".to_owned()));
        assert!(stems("geliyorlar").contains(&"gel".to_owned()));
        assert!(stems("okuyacağım").contains(&"oku".to_owned()));
        assert!(stems("çiçekler").contains(&"çiçek".to_owned()));
        assert!(stems("ağacı").contains(&"ağaç".to_owned()));
        // Harmony is checked: the stem does not take that ending.
        assert!(!stems("kitapler").contains(&"kitap".to_owned()));
        assert!(!stems("evlar").contains(&"ev".to_owned()));
    }
}
