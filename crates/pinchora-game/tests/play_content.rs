//! Checks the shipped game content (trivia, hangman and wordle lists) against the rules the games rely on.

use std::collections::{HashMap, HashSet};

use pinchora_game::play::{Difficulty, HangmanCategory, PlayContent, TriviaCategory, wordle};

/// Words that must never be playable in a family friendly game.
const BANNED: [&str; 24] = [
    "bitch", "whore", "penis", "dildo", "porno", "shits", "fucks", "pussy", "cunts", "twats", "wanks", "boobs",
    "sluts", "dicks", "horny", "nazis", "raped", "rapes", "spics", "homos", "fagot", "nigga", "negro", "pubic",
];

#[test]
fn trivia_questions_are_well_formed() {
    let c = PlayContent::get();
    assert!(c.trivia.len() >= 300, "need 300+ questions, got {}", c.trivia.len());

    let (mut ids, mut questions) = (HashSet::new(), HashSet::new());
    let mut per_category: HashMap<TriviaCategory, usize> = HashMap::new();
    let mut per_difficulty: HashMap<(TriviaCategory, Difficulty), usize> = HashMap::new();
    let mut answer_slots = [0usize; 4];
    for q in &c.trivia {
        assert!(ids.insert(q.id.as_str()), "duplicate id {}", q.id);
        assert!(
            q.id.starts_with(&format!("{}_", q.category.key())),
            "{} must start with its category",
            q.id
        );
        assert!(
            questions.insert(q.question.trim().to_lowercase()),
            "{}: duplicate question",
            q.id
        );
        assert!(
            (10..=110).contains(&q.question.chars().count()),
            "{}: question length",
            q.id
        );
        assert_eq!(q.options.len(), 4, "{}: four options", q.id);
        let distinct: HashSet<_> = q.options.iter().map(|o| o.trim().to_lowercase()).collect();
        assert_eq!(distinct.len(), 4, "{}: options must differ", q.id);
        for option in &q.options {
            assert!(
                !option.trim().is_empty() && option.chars().count() <= 28,
                "{}: option {option:?} must fit a button",
                q.id
            );
        }
        let right: Vec<_> = q.options.iter().enumerate().filter(|(_, o)| **o == q.answer).collect();
        assert_eq!(
            right.len(),
            1,
            "{}: the answer must be exactly one of the options",
            q.id
        );
        answer_slots[right[0].0] += 1;
        assert!(
            !q.question.to_lowercase().contains(&q.answer.to_lowercase()) || q.answer.chars().count() <= 3,
            "{}: the question gives the answer away",
            q.id
        );
        *per_category.entry(q.category).or_default() += 1;
        *per_difficulty.entry((q.category, q.difficulty)).or_default() += 1;
        assert_eq!(c.question(&q.id).map(|f| f.id.as_str()), Some(q.id.as_str()));
    }
    for category in TriviaCategory::ALL {
        assert!(
            per_category.get(&category).copied().unwrap_or(0) >= 40,
            "{}: need 40+ questions",
            category.label()
        );
        for difficulty in Difficulty::ALL {
            assert!(
                per_difficulty.get(&(category, difficulty)).copied().unwrap_or(0) >= 3,
                "{} has too few {} questions",
                category.label(),
                difficulty.label()
            );
        }
    }
    // The file's order of options is shuffled at play time, but a lopsided file hints at a lazy writer.
    let total: usize = answer_slots.iter().sum();
    assert!(
        answer_slots.iter().all(|n| *n * 100 >= total * 15),
        "answers by slot: {answer_slots:?}"
    );
}

#[test]
fn hangman_words_are_well_formed() {
    let c = PlayContent::get();
    assert!(c.hangman.len() >= 400, "need 400+ words, got {}", c.hangman.len());
    let mut seen = HashSet::new();
    let mut per_category: HashMap<HangmanCategory, usize> = HashMap::new();
    for h in &c.hangman {
        assert!(seen.insert(h.word.as_str()), "duplicate word {:?}", h.word);
        let words: Vec<&str> = h.word.split(' ').collect();
        assert!((1..=2).contains(&words.len()), "{:?}: one or two words", h.word);
        for w in &words {
            assert!(
                (2..=16).contains(&w.len()) && w.bytes().all(|b| b.is_ascii_lowercase()),
                "{:?}: lowercase a-z, at most 16 letters a word",
                h.word
            );
        }
        let letters = h.word.len() - words.len() + 1;
        assert!((4..=20).contains(&letters), "{:?}: {letters} letters", h.word);
        assert!(
            !BANNED.contains(&h.word.as_str()),
            "{:?} is not family friendly",
            h.word
        );
        *per_category.entry(h.category).or_default() += 1;
    }
    for category in HangmanCategory::ALL {
        assert!(
            per_category.get(&category).copied().unwrap_or(0) >= 80,
            "{}: need 80+ words",
            category.label()
        );
    }
}

#[test]
fn wordle_lists_are_well_formed() {
    let words = &PlayContent::get().words;
    assert!(
        words.answers.len() >= 1000,
        "need 1000+ answers, got {}",
        words.answers.len()
    );
    assert!(
        words.allowed.len() >= 3000,
        "need 3000+ guesses, got {}",
        words.allowed.len()
    );

    let is_word = |w: &String| w.len() == wordle::WORD_LEN && w.bytes().all(|b| b.is_ascii_lowercase());
    assert!(words.answers.iter().all(is_word), "answers are five lowercase letters");
    assert!(words.allowed.iter().all(is_word), "guesses are five lowercase letters");

    for (name, list) in [("answers", &words.answers), ("allowed", &words.allowed)] {
        assert!(
            list.windows(2).all(|w| w[0] < w[1]),
            "{name} must be sorted with no repeats"
        );
    }
    for answer in &words.answers {
        assert!(words.is_allowed(answer), "answer {answer} must also be a valid guess");
    }
    for banned in BANNED {
        assert!(!words.is_allowed(banned), "{banned} must not be playable");
    }
    assert!(!words.is_allowed("zzzzz") && !words.is_allowed("pear") && words.is_allowed("pearl"));

    // The daily word is always a real answer, on every day of a long stretch.
    for day in 20_700..22_000 {
        let word = wordle::daily_word(&words.answers, day);
        assert!(words.answers.iter().any(|a| a == word));
    }
}
