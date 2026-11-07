//! Letters and letter case for the languages the word games speak. Turkish has two letters I: dotless
//! `ı`/`I` and dotted `i`/`İ`, so the plain `to_uppercase` of `i` (which gives `I`) is wrong for it.

use pinchora_core::i18n::Lang;

pub const EN_ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz";
pub const TR_ALPHABET: &str = "abcçdefgğhıijklmnoöprsştuüvyz";

/// The lowercase letters words are made of, in alphabetical order.
pub fn alphabet(lang: Lang) -> &'static str {
    match lang {
        Lang::En => EN_ALPHABET,
        Lang::Tr => TR_ALPHABET,
    }
}

/// Whether `c` is a lowercase letter of the language.
pub fn is_letter(lang: Lang, c: char) -> bool {
    alphabet(lang).contains(c)
}

/// Lowercase with the language's rules: Turkish `I` is `ı` and `İ` is `i`.
pub fn lower(lang: Lang, text: &str) -> String {
    match lang {
        Lang::En => text.to_ascii_lowercase(),
        Lang::Tr => text
            .chars()
            .flat_map(|c| match c {
                'I' => 'ı'.to_lowercase(),
                'İ' => 'i'.to_lowercase(),
                c => c.to_lowercase(),
            })
            .collect(),
    }
}

/// The capital of one letter: Turkish `i` is `İ` and `ı` is `I`.
pub fn upper_char(lang: Lang, c: char) -> char {
    lang.upper(&c.to_string()).chars().next().unwrap_or(c)
}

/// The keyboard a wordle card draws, three rows of lowercase letters: QWERTY, or the Turkish Q layout
/// without the keys Turkish words never use (q, w, x).
pub fn keyboard_rows(lang: Lang) -> [&'static str; 3] {
    match lang {
        Lang::En => ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
        Lang::Tr => ["ertyuıopğü", "asdfghjklşi", "zcvbnmöç"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turkish_has_twenty_nine_letters_and_the_keyboard_holds_each_once() {
        assert_eq!(TR_ALPHABET.chars().count(), 29);
        assert_eq!(EN_ALPHABET.chars().count(), 26);
        for lang in Lang::ALL {
            let mut keys: Vec<char> = keyboard_rows(lang).iter().flat_map(|r| r.chars()).collect();
            keys.sort_unstable();
            let mut letters: Vec<char> = alphabet(lang).chars().collect();
            letters.sort_unstable();
            assert_eq!(keys, letters, "{lang:?}");
        }
    }

    #[test]
    fn the_two_turkish_i_letters_keep_their_dots() {
        assert_eq!(Lang::Tr.upper("ışık iyi çiçek"), "IŞIK İYİ ÇİÇEK");
        assert_eq!(lower(Lang::Tr, "IŞIK İYİ ÇİÇEK"), "ışık iyi çiçek");
        assert_eq!(upper_char(Lang::Tr, 'i'), 'İ');
        assert_eq!(upper_char(Lang::Tr, 'ı'), 'I');
        assert_eq!(upper_char(Lang::Tr, 'ğ'), 'Ğ');
        assert_eq!(lower(Lang::En, "PeArL"), "pearl");
        assert_eq!(upper_char(Lang::En, 'i'), 'I');
        assert_eq!(Lang::En.upper("pearl"), "PEARL");
    }
}
