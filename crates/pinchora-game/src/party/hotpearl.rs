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

use super::{Dice, filter};

/// Lives a table plays with unless the host picks another number, and the range they may pick from.
pub const DEFAULT_LIVES: i32 = 2;
pub const LIVES: std::ops::RangeInclusive<i32> = 1..=3;
/// A word must be at least this long.
pub const MIN_WORD: usize = 3;
pub const MAX_WORD: usize = 24;

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

pub struct Dictionary {
    words: HashSet<&'static str>,
    counts: HashMap<&'static str, u32>,
    easy: Vec<&'static str>,
    medium: Vec<&'static str>,
    hard: Vec<&'static str>,
}

static WORDS: &str = include_str!("../../../../content/pearl_words.txt");

static DICTIONARY: LazyLock<Dictionary> = LazyLock::new(Dictionary::build);

/// The shipped word list and the syllables made from it. Built on first use.
pub fn dictionary() -> &'static Dictionary {
    &DICTIONARY
}

impl Dictionary {
    fn build() -> Self {
        let words: HashSet<&'static str> = WORDS.lines().filter(|w| !w.is_empty()).collect();
        let mut counts: HashMap<&'static str, u32> = HashMap::new();
        for word in &words {
            // A fragment counts once per word, however often the word repeats it.
            let mut seen: Vec<&str> = Vec::with_capacity(24);
            for n in [2usize, 3] {
                for i in 0..=word.len().saturating_sub(n) {
                    let Some(part) = word.get(i..i + n) else { continue };
                    if !seen.contains(&part) {
                        seen.push(part);
                        *counts.entry(part).or_default() += 1;
                    }
                }
            }
        }
        let mut easy = Vec::new();
        let mut medium = Vec::new();
        let mut hard = Vec::new();
        for (&part, &n) in counts.iter().filter(|(part, _)| !NOT_SHOWN.contains(*part)) {
            match (part.len(), n) {
                (2, n) if n >= EASY_MIN => easy.push(part),
                (2, n) if n >= MEDIUM_MIN_SHORT => medium.push(part),
                (3, n) if n >= MEDIUM_MIN => medium.push(part),
                (3, n) if n >= HARD_MIN => hard.push(part),
                _ => {}
            }
        }
        // A fixed order, so the same dice always pick the same syllable.
        for tier in [&mut easy, &mut medium, &mut hard] {
            tier.sort_unstable();
        }
        Self {
            words,
            counts,
            easy,
            medium,
            hard,
        }
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
        if filter::is_profane(word, false) {
            return false;
        }
        self.words.contains(word) || stems(word).any(|stem| self.words.contains(stem.as_str()))
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
        let syllable = syllable.to_uppercase();
        match self {
            Self::Letters => "Just letters, please: a to z.".to_owned(),
            Self::TooShort => format!("A word needs at least {MIN_WORD} letters."),
            Self::TooLong => "That one is too long to be a word.".to_owned(),
            Self::MissingSyllable => format!("That word doesn't contain **{syllable}**."),
            Self::NotAWord => "That isn't in my word list.".to_owned(),
            Self::AlreadyUsed => "That word was already played this game.".to_owned(),
        }
    }
}

/// Checks a typed word against the syllable, the list and the words played so far; the lowercase
/// word when it counts.
pub fn check_word(dictionary: &Dictionary, syllable: &str, typed: &str, used: &[String]) -> Result<String, WordError> {
    let word = typed.trim().to_ascii_lowercase();
    if word.is_empty() || !word.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(WordError::Letters);
    }
    if word.len() < MIN_WORD {
        return Err(WordError::TooShort);
    }
    if word.len() > MAX_WORD {
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
}
