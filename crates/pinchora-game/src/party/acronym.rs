//! Acronym Tide: a few letters come up, everyone writes a phrase whose words start with them in
//! order, and the table votes for the best one without knowing who wrote what.

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::lang;
use crate::party::Dice;

/// Rounds a game plays unless the host picks another number, and the range they may pick from.
pub const DEFAULT_ROUNDS: i32 = 3;
pub const ROUNDS: std::ops::RangeInclusive<i32> = 1..=5;
/// Seconds to write a phrase, and to vote on them.
pub const WRITE_SECS: i64 = 75;
pub const VOTE_SECS: i64 = 40;
/// How long the results of a round stay up before the next one.
pub const REVEAL_SECS: i64 = 12;
/// Longest phrase and longest single word, in characters.
pub const MAX_PHRASE: usize = 100;
pub const MAX_WORD: usize = 20;

/// Letters that make a good acronym, the easy ones more often. No Q, X or Z.
const POOL: &str = "AAABBCCCDDDEEFFGGHHIIJKLLLMMMNNOOOPPPRRRSSSTTTUVWWY";

/// The same for Turkish: no Ğ (no word starts with it), no dotless I, J, Q, W or X, and the letters with
/// marks (Ç, Ö, Ş, Ü) once each. The dotted İ is the capital of `i`.
const POOL_TR: &str = "AAAABBBCCÇDDDEEEFGGHHİİİKKKKLLLMMMNNOÖPPRRRSSSŞTTTUÜVYZ";

fn pool(lang: Lang) -> Vec<char> {
    match lang {
        Lang::En => POOL.chars().collect(),
        Lang::Tr => POOL_TR.chars().collect(),
    }
}

/// How many letters round `round` (counted from 0) shows: 3, 4, 5, then round again.
pub fn letter_count(round: i32) -> usize {
    3 + round.rem_euclid(3) as usize
}

/// Distinct letters for a round, in the order they must be used.
pub fn draw_letters(lang: Lang, dice: &mut Dice, count: usize) -> Vec<char> {
    let pool = pool(lang);
    let mut letters: Vec<char> = Vec::with_capacity(count);
    while letters.len() < count {
        let pick = pool[dice.below(pool.len())];
        if !letters.contains(&pick) {
            letters.push(pick);
        }
    }
    letters
}

/// Why a phrase did not fit the letters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhraseError {
    Empty,
    /// The phrase has `got` words and the letters ask for `want`.
    Count {
        want: usize,
        got: usize,
    },
    /// The word at `index` (from 0) does not start with the letter `want`.
    Letter {
        index: usize,
        want: char,
    },
    TooLong,
}

impl PhraseError {
    /// What to tell the player, in English.
    pub fn say(&self, letters: &[char]) -> String {
        self.say_in(Lang::En, letters)
    }

    /// The same in `lang` (`acronym.err.*` in the catalogs).
    pub fn say_in(&self, lang: Lang, letters: &[char]) -> String {
        let shown: String = letters.iter().map(|c| format!("{c} ")).collect();
        match self {
            Self::Empty => t!(lang, "acronym.err.empty"),
            Self::Count { want, got } => t!(
                lang,
                "acronym.err.count",
                count = got,
                letters = shown.trim_end(),
                want = want
            ),
            Self::Letter { index, want } => t!(lang, "acronym.err.letter", n = index + 1, want = want),
            Self::TooLong => t!(
                lang,
                "acronym.err.too-long",
                max_phrase = MAX_PHRASE,
                max_word = MAX_WORD
            ),
        }
    }
}

/// Checks a phrase against the letters: one word per letter, each starting with it, case ignored.
/// Returns the phrase with single spaces between the words.
pub fn check_phrase(lang: Lang, text: &str, letters: &[char]) -> Result<String, PhraseError> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Err(PhraseError::Empty);
    }
    if words.len() != letters.len() {
        return Err(PhraseError::Count {
            want: letters.len(),
            got: words.len(),
        });
    }
    for (index, (word, want)) in words.iter().zip(letters).enumerate() {
        // Quotes and brackets before the first letter do not count.
        let first = word.chars().find(|c| c.is_alphanumeric());
        if first.map(|c| lang::upper_char(lang, c)) != Some(lang::upper_char(lang, *want)) {
            return Err(PhraseError::Letter { index, want: *want });
        }
        if word.chars().count() > MAX_WORD {
            return Err(PhraseError::TooLong);
        }
    }
    let phrase = words.join(" ");
    if phrase.chars().count() > MAX_PHRASE {
        return Err(PhraseError::TooLong);
    }
    Ok(phrase)
}

#[cfg(test)]
mod tests {
    use crate::party::acronym::*;

    #[test]
    fn rounds_get_longer_and_start_over() {
        assert_eq!((0..6).map(letter_count).collect::<Vec<_>>(), vec![3, 4, 5, 3, 4, 5]);
    }

    #[test]
    fn letters_are_distinct_easy_and_follow_the_dice() {
        for seed in 0..200 {
            let mut dice = Dice::new(seed);
            for count in 3..=5 {
                let letters = draw_letters(Lang::En, &mut dice, count);
                assert_eq!(letters.len(), count);
                let mut sorted = letters.clone();
                sorted.sort_unstable();
                sorted.dedup();
                assert_eq!(sorted.len(), count, "{letters:?}");
                assert!(letters.iter().all(|c| c.is_ascii_uppercase() && !"QXZ".contains(*c)));
            }
        }
        assert_eq!(
            draw_letters(Lang::En, &mut Dice::new(9), 4),
            draw_letters(Lang::En, &mut Dice::new(9), 4)
        );
    }

    #[test]
    fn a_phrase_needs_one_word_per_letter_in_order() {
        let letters = ['T', 'W', 'R'];
        assert_eq!(
            check_phrase(Lang::En, "  Tiny   whales  RIDE ", &letters),
            Ok("Tiny whales RIDE".to_owned())
        );
        assert!(check_phrase(Lang::En, "\"Tiny\" (whales) 'ride'", &letters).is_ok());
        assert_eq!(check_phrase(Lang::En, "", &letters), Err(PhraseError::Empty));
        assert_eq!(
            check_phrase(Lang::En, "Tiny whales", &letters),
            Err(PhraseError::Count { want: 3, got: 2 })
        );
        assert_eq!(
            check_phrase(Lang::En, "Tiny whales swim", &letters),
            Err(PhraseError::Letter { index: 2, want: 'R' })
        );
        assert_eq!(
            check_phrase(Lang::En, "Whales tiny ride", &letters),
            Err(PhraseError::Letter { index: 0, want: 'T' })
        );
        assert_eq!(
            check_phrase(Lang::En, &format!("T{} w r", "t".repeat(30)), &letters),
            Err(PhraseError::TooLong)
        );
        assert_eq!(
            check_phrase(Lang::En, &format!("{0} {0} {0}", "t".repeat(19)), &['T', 'T', 'T']).map(|_| ()),
            Ok(())
        );
    }

    #[test]
    fn a_phrase_has_a_total_limit_too() {
        let long = vec!["a".repeat(19); 6].join(" ");
        assert!(long.chars().count() > MAX_PHRASE);
        assert_eq!(check_phrase(Lang::En, &long, &['A'; 6]), Err(PhraseError::TooLong));
    }

    #[test]
    fn the_messages_name_the_problem() {
        let letters = ['T', 'W', 'R'];
        assert!(PhraseError::Count { want: 3, got: 1 }.say(&letters).contains("1 word,"));
        assert!(
            PhraseError::Letter { index: 1, want: 'W' }
                .say(&letters)
                .contains("Word 2")
        );
    }

    #[test]
    fn the_messages_come_in_turkish_too() {
        let letters = ['İ', 'K', 'Ş'];
        let count = PhraseError::Count { want: 3, got: 1 }.say_in(Lang::Tr, &letters);
        assert!(
            count.contains("1 kelime yazdın") && count.contains("**İ K Ş**"),
            "{count}"
        );
        assert!(
            PhraseError::Letter { index: 1, want: 'K' }
                .say_in(Lang::Tr, &letters)
                .contains("2. kelime **K** ile")
        );
        assert!(PhraseError::TooLong.say_in(Lang::Tr, &letters).contains("100 karakter"));
        assert_eq!(
            PhraseError::Empty.say_in(Lang::En, &letters),
            PhraseError::Empty.say(&letters)
        );
    }

    #[test]
    fn turkish_letters_are_easy_ones_and_the_dotted_i_is_its_own_letter() {
        for seed in 0..200 {
            let mut dice = Dice::new(seed);
            for count in 3..=5 {
                let letters = draw_letters(Lang::Tr, &mut dice, count);
                let mut sorted = letters.clone();
                sorted.sort_unstable();
                sorted.dedup();
                assert_eq!(sorted.len(), count, "{letters:?}");
                assert!(
                    letters.iter().all(|c| "ABCÇDEFGHİKLMNOÖPRSŞTUÜVYZ".contains(*c)),
                    "no Ğ, I, J, Q, W or X: {letters:?}"
                );
            }
        }
        let letters = ['İ', 'K', 'Ş'];
        assert!(check_phrase(Lang::Tr, "iyi kediler şimdi", &letters).is_ok());
        assert!(check_phrase(Lang::Tr, "\"İnce\" kuşlar (şarkı)", &letters).is_ok());
        assert_eq!(
            check_phrase(Lang::Tr, "ışık kuşlar şarkı", &letters),
            Err(PhraseError::Letter { index: 0, want: 'İ' }),
            "ışık starts with the dotless I"
        );
        assert!(check_phrase(Lang::Tr, "Işık Kuşlar Şarkı", &['I', 'K', 'Ş']).is_ok());
        assert!(check_phrase(Lang::Tr, "çiçek ödül", &['Ç', 'Ö']).is_ok());
    }
}
