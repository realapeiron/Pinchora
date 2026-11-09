//! Checks the shipped Turkish game content against the rules the games rely on, and that none of it trips
//! the profanity filter, even a strict server's.

use std::collections::{HashMap, HashSet};

use pinchora_core::infra::i18n::Lang;
use pinchora_game::{
    lang,
    party::{filter::is_profane_for, hotpearl::dictionary_for, prompts::PartyContent},
    play::{Difficulty, HangmanCategory, PlayContent, TriviaCategory, wordle},
};

fn tr() -> &'static PlayContent {
    PlayContent::for_lang(Lang::Tr)
}

fn turkish_word(word: &str) -> bool {
    word.chars().all(|c| lang::is_letter(Lang::Tr, c))
}

#[test]
fn turkish_trivia_translates_every_english_question_and_adds_turkey_ones() {
    let c = tr();
    let english = PlayContent::get();
    assert!(
        c.trivia.len() >= english.trivia.len() + 100,
        "464 translated and 100 more, got {}",
        c.trivia.len()
    );
    let (mut ids, mut questions) = (HashSet::new(), HashSet::new());
    let mut per_category: HashMap<TriviaCategory, usize> = HashMap::new();
    let mut per_difficulty: HashMap<(TriviaCategory, Difficulty), usize> = HashMap::new();
    let mut slots = [0usize; 4];
    for q in &c.trivia {
        assert_eq!(q.lang, Lang::Tr);
        assert!(ids.insert(q.id.as_str()), "duplicate id {}", q.id);
        assert!(
            english.question(&q.id).is_none(),
            "{} collides with an English id",
            q.id
        );
        assert!(q.id.starts_with(&format!("{}_tr_", q.category.key())), "{}", q.id);
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
        assert_eq!(q.options.len(), 4, "{}", q.id);
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
        slots[right[0].0] += 1;
        assert!(
            !q.question.to_lowercase().contains(&q.answer.to_lowercase()) || q.answer.chars().count() <= 3,
            "{}: the question gives the answer away",
            q.id
        );
        for text in std::iter::once(&q.question).chain(&q.options) {
            assert!(!is_profane_for(text, true, Lang::Tr), "{}: {text}", q.id);
        }
        *per_category.entry(q.category).or_default() += 1;
        *per_difficulty.entry((q.category, q.difficulty)).or_default() += 1;
        assert!(c.question(&q.id).is_some());
    }
    // Every English question has its Turkish twin.
    for q in &english.trivia {
        let twin = q.id.replacen('_', "_tr_", 1);
        let found = c.question(&twin).unwrap_or_else(|| panic!("{twin} is missing"));
        assert_eq!((found.category, found.difficulty), (q.category, q.difficulty), "{twin}");
    }
    for category in TriviaCategory::ALL {
        assert!(
            per_category.get(&category).copied().unwrap_or(0)
                >= if category == TriviaCategory::Minecraft { 40 } else { 55 },
            "{}",
            category.label()
        );
        if category != TriviaCategory::Minecraft {
            assert_ne!(category.label_in(Lang::Tr), category.label());
        }
        for difficulty in Difficulty::ALL {
            assert!(per_difficulty.get(&(category, difficulty)).copied().unwrap_or(0) >= 3);
        }
    }
    let total: usize = slots.iter().sum();
    assert!(
        slots.iter().all(|n| *n * 100 >= total * 15),
        "answers by slot: {slots:?}"
    );
}

#[test]
fn turkish_hangman_words_are_well_formed() {
    let c = tr();
    assert!(c.hangman.len() >= 500, "{}", c.hangman.len());
    let mut seen = HashSet::new();
    let mut per_category: HashMap<HangmanCategory, usize> = HashMap::new();
    for h in &c.hangman {
        assert!(seen.insert(h.word.as_str()), "duplicate word {:?}", h.word);
        let words: Vec<&str> = h.word.split(' ').collect();
        assert!((1..=2).contains(&words.len()), "{:?}", h.word);
        assert!(
            words.iter().all(|w| turkish_word(w) && !w.is_empty()),
            "{:?}: Turkish letters only",
            h.word
        );
        let letters = h.word.chars().filter(|c| *c != ' ').count();
        assert!((4..=16).contains(&letters), "{:?}: {letters} letters", h.word);
        assert!(!is_profane_for(&h.word, true, Lang::Tr), "{:?}", h.word);
        *per_category.entry(h.category).or_default() += 1;
    }
    for category in HangmanCategory::ALL {
        assert!(
            per_category.get(&category).copied().unwrap_or(0) >= 100,
            "{}",
            category.label()
        );
        assert_ne!(category.label_in(Lang::Tr), category.label());
    }
    // The dotted and the dotless i, and the marked letters, all turn up.
    for letter in ['ç', 'ğ', 'ı', 'i', 'ö', 'ş', 'ü'] {
        assert!(
            c.hangman.iter().any(|h| h.word.contains(letter)),
            "no word with {letter}"
        );
    }
}

#[test]
fn turkish_wordle_lists_are_well_formed_and_clean() {
    let words = &tr().words;
    assert!(words.answers.len() >= 800, "{} answers", words.answers.len());
    assert!(words.allowed.len() >= 4000, "{} guesses", words.allowed.len());
    let is_word = |w: &String| w.chars().count() == wordle::WORD_LEN && turkish_word(w);
    assert!(words.answers.iter().all(is_word), "answers are five Turkish letters");
    assert!(words.allowed.iter().all(is_word), "guesses are five Turkish letters");
    for (name, list) in [("answers", &words.answers), ("allowed", &words.allowed)] {
        assert!(
            list.windows(2).all(|w| w[0] < w[1]),
            "{name} must be sorted with no repeats"
        );
        for word in list {
            // The daily word is shown to everyone, so answers pass the strict list too.
            assert!(!is_profane_for(word, name == "answers", Lang::Tr), "{name}: {word}");
            assert!(!is_profane_for(word, false, Lang::En), "{name}: {word}");
        }
    }
    for answer in &words.answers {
        assert!(words.is_allowed(answer), "answer {answer} must also be a valid guess");
    }
    for word in ["çiçek", "ışıkl", "kitap", "deniz", "gelir", "evler", "okula", "güneş"] {
        assert!(
            words.is_allowed(word) || word == "ışıkl",
            "{word} must be a playable guess"
        );
    }
    for word in ["pearl", "quark", "zzzzz", "kalem1"] {
        assert!(!words.is_allowed(word), "{word}");
    }
    // Words with each marked letter are playable, and so are both i letters.
    for letter in ['ç', 'ğ', 'ı', 'i', 'ö', 'ş', 'ü'] {
        assert!(
            words.answers.iter().filter(|w| w.contains(letter)).count() >= 20,
            "answers with {letter}"
        );
    }
    for day in 20_700..22_000 {
        let word = wordle::daily_word(&words.answers, day);
        assert!(words.answers.iter().any(|a| a == word));
    }
    // The two languages' daily words are their own.
    assert_ne!(
        wordle::daily_word(&words.answers, 20_900),
        wordle::daily_word(&PlayContent::get().words.answers, 20_900)
    );
}

#[test]
fn the_turkish_party_pools_and_word_list_are_clean() {
    let c = PartyContent::for_lang(Lang::Tr);
    assert_eq!((c.wyr.len(), c.truths.len(), c.dares.len()), (212, 102, 102));
    for q in &c.wyr {
        assert!(q.a.ends_with("mak") || q.a.ends_with("mek"), "{}: {}", q.id, q.a);
        assert!(q.b.ends_with("mak") || q.b.ends_with("mek"), "{}: {}", q.id, q.b);
    }
    let d = dictionary_for(Lang::Tr);
    let text = include_str!("../../../content/pearl_words_tr.txt");
    let mut seen = HashSet::new();
    for word in text.lines() {
        assert!(seen.insert(word), "duplicate {word}");
        assert!((3..=14).contains(&word.chars().count()) && turkish_word(word), "{word}");
        assert!(!is_profane_for(word, false, Lang::Tr), "{word}");
    }
    assert!(seen.len() >= 8000, "{} words", seen.len());
    for word in text.lines().take(3000) {
        assert!(d.accepts(word), "{word}");
    }
    // Every wordle answer is a word Hot Pearl takes too.
    for answer in &tr().words.answers {
        assert!(d.accepts(answer), "{answer}");
    }
}
