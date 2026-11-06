//! Hot Pearl: a syllable goes round the table, and whoever holds the pearl must type a word that
//! contains it before it pops. Each pop costs a life; the last player with lives wins.
//!
//! The word list is `content/pearl_words.txt`: the lowercase 3 to 12 letter headwords of Webster's
//! Second (public domain, in `/usr/share/dict/web2`) and the wordle lists, plus a few hundred modern
//! words, minus anything rude. It holds headwords only, so [`Dictionary::accepts`] also takes the
//! usual inflections of a listed word (plurals, `ed`, `ing`, `er`, `est`, `ly`).

use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

use pinchora_core::{i18n::Lang, t};

use super::{Dice, filter, hotpearl_tr};
use crate::lang;

/// Lives a table plays with unless the host picks another number, and the range they may pick from.
pub const DEFAULT_LIVES: i32 = 2;
pub const LIVES: std::ops::RangeInclusive<i32> = 1..=3;
/// A word must be at least this long.
pub const MIN_WORD: usize = 3;
pub const MAX_WORD: usize = 24;

/// A game pays its winner only when at least this many words were played in all, and at least
/// [`MIN_WINNER_WORDS`] of them by the winner: a pearl nobody tried to catch is not worth shells.
pub const MIN_WORDS: usize = 4;
pub const MIN_WINNER_WORDS: usize = 2;

/// Whether the winner of a game earns shells, given who played each of its words.
pub fn earns_reward(authors: &[u64], winner: u64) -> bool {
    authors.len() >= MIN_WORDS && authors.iter().filter(|a| **a == winner).count() >= MIN_WINNER_WORDS
}

/// Seconds the pearl stays with a player on `turn` (counted from 0): it burns shorter as the game goes on.
pub fn turn_secs(turn: u32) -> i64 {
    (25 - i64::from(turn) / 4).max(15)
}

/// How hard a syllable is, by how many words in the list contain it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Easy,
    Medium,
    Hard,
}

impl Tier {
    /// Which mix of tiers a turn draws from: the first turns are gentle, the late ones are not.
    pub fn for_turn(turn: u32, roll: usize) -> Self {
        match (turn, roll % 10) {
            (0..=3, _) => Self::Easy,
            (4..=9, 0..=5) => Self::Easy,
            (4..=9, _) => Self::Medium,
            (_, 0..=2) => Self::Easy,
            (_, 3..=6) => Self::Medium,
            _ => Self::Hard,
        }
    }
}

/// Words with a syllable in them, at least: how many a fragment needs to be a syllable of that tier.
const EASY_MIN: u32 = 2_500;
const MEDIUM_MIN: u32 = 1_500;
const MEDIUM_MIN_SHORT: u32 = 800;
const HARD_MIN: u32 = 400;
/// Fragments common enough to be syllables that a family table should not see in big letters.
const NOT_SHOWN: &[&str] = &["ass", "tit", "cum", "sex", "fag", "nig"];

/// The same thresholds for the Turkish list, which is about a twentieth the size.
const EASY_MIN_TR: u32 = 200;
const MEDIUM_MIN_TR: u32 = 60;
const MEDIUM_MIN_SHORT_TR: u32 = 80;
const HARD_MIN_TR: u32 = 30;
/// Turkish fragments never shown in big letters.
const NOT_SHOWN_TR: &[&str] = &["sik", "göt", "bok", "amk", "amc", "piç", "ibn", "pus", "oro"];

pub struct Dictionary {
    lang: Lang,
    words: HashSet<&'static str>,
    /// The stems a verb's infinitive stands for ("gel" for "gelmek"), also words themselves.
    roots: HashSet<String>,
    counts: HashMap<&'static str, u32>,
    easy: Vec<&'static str>,
    medium: Vec<&'static str>,
    hard: Vec<&'static str>,
}

static WORDS: &str = include_str!("../../../../content/pearl_words.txt");
static WORDS_TR: &str = include_str!("../../../../content/pearl_words_tr.txt");

static DICTIONARY: LazyLock<Dictionary> = LazyLock::new(|| Dictionary::build(WORDS, Lang::En));
static DICTIONARY_TR: LazyLock<Dictionary> = LazyLock::new(|| Dictionary::build(WORDS_TR, Lang::Tr));

/// The shipped English word list and the syllables made from it. Built on first use.
pub fn dictionary() -> &'static Dictionary {
    &DICTIONARY
}

/// The shipped word list of `lang` and the syllables made from it.
pub fn dictionary_for(lang: Lang) -> &'static Dictionary {
    match lang {
        Lang::En => &DICTIONARY,
        Lang::Tr => &DICTIONARY_TR,
    }
}

impl Dictionary {
    fn build(text: &'static str, lang: Lang) -> Self {
        let words: HashSet<&'static str> = text.lines().filter(|w| !w.is_empty()).collect();
        let mut counts: HashMap<&'static str, u32> = HashMap::new();
        for word in &words {
            // A fragment counts once per word, however often the word repeats it.
            let at: Vec<usize> = word.char_indices().map(|(i, _)| i).chain([word.len()]).collect();
            let mut seen: Vec<&str> = Vec::with_capacity(24);
            for n in [2usize, 3] {
                for i in 0..at.len().saturating_sub(n) {
                    let part = &word[at[i]..at[i + n]];
                    if !seen.contains(&part) {
                        seen.push(part);
                        *counts.entry(part).or_default() += 1;
                    }
                }
            }
        }
        let (easy_min, medium_min, medium_short, hard_min) = match lang {
            Lang::En => (EASY_MIN, MEDIUM_MIN, MEDIUM_MIN_SHORT, HARD_MIN),
            Lang::Tr => (EASY_MIN_TR, MEDIUM_MIN_TR, MEDIUM_MIN_SHORT_TR, HARD_MIN_TR),
        };
        let hidden = match lang {
            Lang::En => NOT_SHOWN,
            Lang::Tr => NOT_SHOWN_TR,
        };
        let mut easy = Vec::new();
        let mut medium = Vec::new();
        let mut hard = Vec::new();
        for (&part, &n) in counts.iter().filter(|(part, _)| !hidden.contains(*part)) {
            match (part.chars().count(), n) {
                (2, n) if n >= easy_min => easy.push(part),
                (2, n) if n >= medium_short => medium.push(part),
                (3, n) if n >= medium_min => medium.push(part),
                (3, n) if n >= hard_min => hard.push(part),
                _ => {}
            }
        }
        // A fixed order, so the same dice always pick the same syllable.
        for tier in [&mut easy, &mut medium, &mut hard] {
            tier.sort_unstable();
        }
        let roots = if lang == Lang::Tr {
            words
                .iter()
                .filter_map(|w| w.strip_suffix("mak").or_else(|| w.strip_suffix("mek")))
                .filter(|root| root.chars().count() >= 2)
                .map(str::to_owned)
                .collect()
        } else {
            HashSet::new()
        };
        Self {
            lang,
            words,
            roots,
            counts,
            easy,
            medium,
            hard,
        }
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// How many listed words hold `syllable`.
    pub fn count(&self, syllable: &str) -> u32 {
        self.counts.get(syllable).copied().unwrap_or(0)
    }

    pub fn tier(&self, tier: Tier) -> &[&'static str] {
        match tier {
            Tier::Easy => &self.easy,
            Tier::Medium => &self.medium,
            Tier::Hard => &self.hard,
        }
    }

    /// The syllable for turn number `turn`, drawn by `dice`; never `avoid`, so it changes every turn.
    pub fn syllable(&self, turn: u32, dice: &mut Dice, avoid: Option<&str>) -> &'static str {
        let tier = Tier::for_turn(turn, dice.below(10));
        let pool = self.tier(tier);
        for _ in 0..8 {
            let pick = pool[dice.below(pool.len())];
            if Some(pick) != avoid {
                return pick;
            }
        }
        pool[0]
    }

    /// Whether `word` (lowercase letters) is a word: listed, or an inflection of a listed one, and not rude.
    pub fn accepts(&self, word: &str) -> bool {
        if filter::is_profane_for(word, false, self.lang) {
            return false;
        }
        match self.lang {
            Lang::En => self.words.contains(word) || stems(word).any(|stem| self.words.contains(stem.as_str())),
            Lang::Tr => self.knows(word) || hotpearl_tr::stems(word).iter().any(|stem| self.knows(stem)),
        }
    }

    /// Whether `word` is listed, or the stem of a listed verb.
    fn knows(&self, word: &str) -> bool {
        self.words.contains(word) || self.roots.contains(word)
    }
}

/// The words `word` may be an inflection of: one step of `s`, `es`, `ed`, `ing`, `er`, `est` or `ly`.
fn stems(word: &str) -> impl Iterator<Item = String> {
    let mut out: Vec<String> = Vec::new();
    let mut cut = |suffix: &str, add: &str| {
        if word.len() > suffix.len() + 2
            && let Some(root) = word.strip_suffix(suffix)
        {
            out.push(format!("{root}{add}"));
            // Stopped, running, bigger: the last letter of the root was doubled.
            let mut chars = root.chars().rev();
            if let (Some(a), Some(b)) = (chars.next(), chars.next())
                && a == b
                && add.is_empty()
            {
                out.push(root[..root.len() - 1].to_owned());
            }
        }
    };
    cut("ies", "y");
    cut("es", "");
    if !word.ends_with("ss") {
        cut("s", "");
    }
    cut("ied", "y");
    cut("ed", "");
    cut("d", "");
    cut("ing", "");
    cut("ing", "e");
    cut("ier", "y");
    cut("er", "");
    cut("r", "");
    cut("iest", "y");
    cut("est", "");
    cut("st", "");
    cut("ily", "y");
    cut("ly", "");
    out.into_iter()
}

/// Why a word did not count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordError {
    /// Not just letters a to z.
    Letters,
    TooShort,
    TooLong,
    MissingSyllable,
    NotAWord,
    AlreadyUsed,
}

impl WordError {
    /// What to tell the player, with the syllable in capitals.
    pub fn say(self, syllable: &str) -> String {
        self.say_in(Lang::En, syllable)
    }

    /// The same in `lang`.
    pub fn say_in(self, lang: Lang, syllable: &str) -> String {
        let syllable = lang.upper(syllable);
        match self {
            Self::Letters => t!(lang, "hotpearl.err.letters"),
            Self::TooShort => t!(lang, "hotpearl.err.too-short", min = MIN_WORD),
            Self::TooLong => t!(lang, "hotpearl.err.too-long"),
            Self::MissingSyllable => t!(lang, "hotpearl.err.missing-syllable", syllable = syllable),
            Self::NotAWord => t!(lang, "hotpearl.err.not-a-word"),
            Self::AlreadyUsed => t!(lang, "hotpearl.err.already-used"),
        }
    }
}

/// Checks a typed word against the syllable, the list and the words played so far; the lowercase
/// word when it counts.
pub fn check_word(dictionary: &Dictionary, syllable: &str, typed: &str, used: &[String]) -> Result<String, WordError> {
    let lang = dictionary.lang;
    let word = lang::lower(lang, typed.trim());
    if word.is_empty() || !word.chars().all(|c| lang::is_letter(lang, c)) {
        return Err(WordError::Letters);
    }
    let length = word.chars().count();
    if length < MIN_WORD {
        return Err(WordError::TooShort);
    }
    if length > MAX_WORD {
        return Err(WordError::TooLong);
    }
    if !word.contains(syllable) {
        return Err(WordError::MissingSyllable);
    }
    if used.contains(&word) {
        return Err(WordError::AlreadyUsed);
    }
    if !dictionary.accepts(&word) {
        return Err(WordError::NotAWord);
    }
    Ok(word)
}

/// The player after `after` in seat order who still has lives, going round; `None` when nobody does.
/// `order` lists the seats with whether each is still in.
pub fn next_player(order: &[(u64, bool)], after: u64) -> Option<u64> {
    let at = order.iter().position(|(user, _)| *user == after)?;
    (1..=order.len())
        .map(|step| order[(at + step) % order.len()])
        .find(|(_, alive)| *alive)
        .map(|(user, _)| user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_win_needs_real_words_from_the_table_and_the_winner() {
        // Nobody typed: the last player standing won by waiting.
        assert!(!earns_reward(&[], 1));
        // A few words, but not enough in all, or not the winner's.
        assert!(!earns_reward(&[1, 1, 2], 1));
        assert!(!earns_reward(&[2, 2, 2, 1], 1));
        assert!(!earns_reward(&[2, 2, 2, 2, 2], 1));
        // Enough from the table, and enough from the winner.
        assert!(earns_reward(&[1, 2, 1, 2], 1));
        assert!(earns_reward(&[1, 1, 1, 1], 1));
        assert!(earns_reward(&[2, 1, 2, 3, 1], 1));
    }

    #[test]
    fn the_pearl_burns_faster_and_stops_at_fifteen_seconds() {
        assert_eq!(turn_secs(0), 25);
        assert_eq!(turn_secs(4), 24);
        assert!(turn_secs(10) < turn_secs(0));
        assert_eq!(turn_secs(1000), 15);
    }

    #[test]
    fn the_list_is_big_clean_and_has_the_words_players_try() {
        let d = dictionary();
        assert!(d.words.len() > 150_000, "{}", d.words.len());
        for word in [
            "pearl", "ocean", "running", "cats", "jumped", "happiest", "quickly", "okay", "pizza", "smoothie",
        ] {
            assert!(d.accepts(word), "{word}");
        }
        for word in [
            "qzxv", "fuck", "shitty", "negro", "jumpedly", "xyzzy", "wetback", "squaw", "chinks", "erotic", "orgies",
        ] {
            assert!(!d.accepts(word), "{word}");
        }
    }

    #[test]
    fn every_tier_has_syllables_and_each_has_plenty_of_words() {
        let d = dictionary();
        for tier in [Tier::Easy, Tier::Medium, Tier::Hard] {
            let pool = d.tier(tier);
            assert!(pool.len() >= 100, "{tier:?}: {}", pool.len());
            for syllable in pool {
                assert!(
                    (2..=3).contains(&syllable.len()) && syllable.bytes().all(|b| b.is_ascii_lowercase()),
                    "{syllable}"
                );
                assert!(d.count(syllable) >= HARD_MIN, "{syllable}");
            }
        }
        assert!(
            d.tier(Tier::Easy)
                .iter()
                .all(|s| s.len() == 2 && d.count(s) >= EASY_MIN)
        );
        assert!(d.tier(Tier::Hard).iter().all(|s| s.len() == 3));
        assert!(d.count("er") > d.count("zz"));
        assert!(d.count("ass") >= HARD_MIN, "common enough, and still never shown");
        for tier in [Tier::Easy, Tier::Medium, Tier::Hard] {
            assert!(d.tier(tier).iter().all(|s| !NOT_SHOWN.contains(s)));
        }
    }

    #[test]
    fn the_syllable_changes_every_turn_and_follows_the_dice() {
        let d = dictionary();
        let mut dice = Dice::new(42);
        let mut last: Option<&str> = None;
        for turn in 0..60 {
            let syllable = d.syllable(turn, &mut dice, last);
            assert_ne!(Some(syllable), last);
            assert!(d.count(syllable) >= HARD_MIN);
            last = Some(syllable);
        }
        let again: Vec<_> = {
            let mut a = Dice::new(7);
            (0..10).map(|t| d.syllable(t, &mut a, None)).collect()
        };
        let same: Vec<_> = {
            let mut b = Dice::new(7);
            (0..10).map(|t| d.syllable(t, &mut b, None)).collect()
        };
        assert_eq!(again, same);
        // The first turns are gentle.
        let mut dice = Dice::new(1);
        assert!((0..4).all(|t| d.syllable(t, &mut dice, None).len() == 2));
    }

    #[test]
    fn a_word_has_to_hold_the_syllable_be_real_and_be_new() {
        let d = dictionary();
        let used = vec!["learn".to_owned()];
        assert_eq!(check_word(d, "ear", " Pearl ", &used), Ok("pearl".to_owned()));
        assert_eq!(check_word(d, "ear", "learn", &used), Err(WordError::AlreadyUsed));
        assert_eq!(check_word(d, "ear", "ocean", &used), Err(WordError::MissingSyllable));
        assert_eq!(check_word(d, "ear", "xearx", &used), Err(WordError::NotAWord));
        assert_eq!(check_word(d, "ea", "ea", &used), Err(WordError::TooShort));
        assert_eq!(check_word(d, "ear", "pe arl", &used), Err(WordError::Letters));
        assert_eq!(check_word(d, "ear", "", &used), Err(WordError::Letters));
        assert_eq!(check_word(d, "ear", &"e".repeat(30), &used), Err(WordError::TooLong));
        assert!(WordError::MissingSyllable.say("ear").contains("EAR"));
    }

    #[test]
    fn the_pearl_goes_round_and_skips_players_who_are_out() {
        let order = [(1, true), (2, false), (3, true), (4, true)];
        assert_eq!(next_player(&order, 1), Some(3));
        assert_eq!(next_player(&order, 3), Some(4));
        assert_eq!(next_player(&order, 4), Some(1));
        // The holder who just lost their last life is not "after" themselves.
        assert_eq!(next_player(&[(1, false), (2, true)], 1), Some(2));
        assert_eq!(next_player(&[(1, true)], 1), Some(1), "alone, the pearl comes back");
        assert_eq!(next_player(&[(1, false), (2, false)], 1), None);
        assert_eq!(next_player(&order, 99), None);
    }

    #[test]
    fn stems_find_the_usual_inflections() {
        let found = |w: &str| stems(w).collect::<Vec<_>>();
        assert!(found("stopped").contains(&"stop".to_owned()));
        assert!(found("running").contains(&"run".to_owned()));
        assert!(found("liking").contains(&"like".to_owned()));
        assert!(found("cities").contains(&"city".to_owned()));
        assert!(found("biggest").contains(&"big".to_owned()));
        assert!(found("boxes").contains(&"box".to_owned()));
        assert!(found("happily").contains(&"happy".to_owned()));
    }

    #[test]
    fn the_turkish_list_is_big_clean_and_takes_the_usual_endings() {
        let d = dictionary_for(Lang::Tr);
        assert_eq!(d.lang(), Lang::Tr);
        assert!(d.words.len() > 7_000, "{}", d.words.len());
        for word in [
            "kitap",
            "çiçek",
            "deniz",
            "gelmek",
            "evler",
            "kitaplarımız",
            "gözlerinden",
            "geliyorum",
            "okuyacağız",
            "ağacı",
            "kitabı",
            "yapıyor",
            "güzellik",
            "ışık",
        ] {
            assert!(d.accepts(word), "{word}");
        }
        for word in [
            "qzxv",
            "kitapler",
            "evlar",
            "xyzzy",
            "siktir",
            "orospu",
            "gelmekxx",
            "ççççç",
        ] {
            assert!(!d.accepts(word), "{word}");
        }
        assert!(d.words.iter().all(|w| w.chars().all(|c| lang::is_letter(Lang::Tr, c))));
    }

    #[test]
    fn turkish_syllables_come_in_three_tiers_of_real_fragments() {
        let d = dictionary_for(Lang::Tr);
        for tier in [Tier::Easy, Tier::Medium, Tier::Hard] {
            let pool = d.tier(tier);
            assert!(pool.len() >= 40, "{tier:?}: {}", pool.len());
            for syllable in pool {
                let n = syllable.chars().count();
                assert!(
                    (2..=3).contains(&n) && syllable.chars().all(|c| lang::is_letter(Lang::Tr, c)),
                    "{syllable}"
                );
                assert!(d.count(syllable) >= HARD_MIN_TR, "{syllable}");
                assert!(!NOT_SHOWN_TR.contains(syllable));
            }
        }
        assert!(d.tier(Tier::Easy).iter().all(|s| s.chars().count() == 2));
        let mut dice = Dice::new(5);
        let mut last: Option<&str> = None;
        for turn in 0..80 {
            let syllable = d.syllable(turn, &mut dice, last);
            assert_ne!(Some(syllable), last);
            last = Some(syllable);
        }
    }

    #[test]
    fn a_turkish_word_has_to_hold_the_syllable_with_turkish_case() {
        let d = dictionary_for(Lang::Tr);
        let used = vec!["kitap".to_owned()];
        assert_eq!(check_word(d, "iş", " İŞÇİ ", &used), Ok("işçi".to_owned()));
        assert_eq!(check_word(d, "ış", "IŞIK", &used), Ok("ışık".to_owned()));
        assert_eq!(check_word(d, "ış", "işçi", &used), Err(WordError::MissingSyllable));
        assert_eq!(check_word(d, "ki", "kitap", &used), Err(WordError::AlreadyUsed));
        assert_eq!(check_word(d, "ka", "kaw", &used), Err(WordError::Letters));
        assert_eq!(check_word(d, "ka", "kxx", &used), Err(WordError::Letters));
        assert!(WordError::MissingSyllable.say_in(Lang::Tr, "ış").contains("IŞ"));
        assert!(WordError::MissingSyllable.say_in(Lang::Tr, "iş").contains("İŞ"));
    }
}
