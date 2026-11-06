//! Hangman: six misses and the sea takes the sandcastle.

use pinchora_core::i18n::Lang;

use crate::lang;

pub const MAX_MISSES: usize = 6;

/// What a guessed letter did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guess {
    /// In the word this many times.
    Hit(usize),
    Miss,
    /// Already guessed.
    Repeat,
    /// Not a letter of the language's alphabet.
    Invalid,
}

/// How a letter of the word is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Hidden(char),
    Guessed(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hangman {
    lang: Lang,
    word: String,
    /// Guesses in the order they were made.
    guessed: Vec<char>,
}

impl Hangman {
    pub fn new(word: &str, lang: Lang) -> Self {
        Self {
            lang,
            word: word.to_owned(),
            guessed: Vec::new(),
        }
    }

    /// Reads a game written by [`Hangman::encode`]: `word|guessed`, and `|tr` after them for a Turkish one.
    pub fn decode(state: &str) -> Option<Self> {
        let (word, rest) = state.split_once('|')?;
        let (guessed, lang) = match rest.split_once('|') {
            Some((guessed, "tr")) => (guessed, Lang::Tr),
            Some(_) => return None,
            None => (rest, Lang::En),
        };
        let valid = |c: char| lang::is_letter(lang, c);
        if word.is_empty() || !word.chars().all(|c| valid(c) || c == ' ') || !guessed.chars().all(valid) {
            return None;
        }
        Some(Self {
            lang,
            word: word.to_owned(),
            guessed: guessed.chars().collect(),
        })
    }

    pub fn encode(&self) -> String {
        let tag = if self.lang == Lang::En { "" } else { "|tr" };
        format!("{}|{}{tag}", self.word, self.guessed.iter().collect::<String>())
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    pub fn word(&self) -> &str {
        &self.word
    }

    pub fn has_guessed(&self, letter: char) -> bool {
        self.guessed.contains(&letter)
    }

    pub fn guess(&mut self, letter: char) -> Guess {
        let letter = lang::lower(self.lang, &letter.to_string())
            .chars()
            .next()
            .unwrap_or(' ');
        if !lang::is_letter(self.lang, letter) {
            return Guess::Invalid;
        }
        if self.has_guessed(letter) {
            return Guess::Repeat;
        }
        self.guessed.push(letter);
        match self.word.chars().filter(|c| *c == letter).count() {
            0 => Guess::Miss,
            n => Guess::Hit(n),
        }
    }

    /// Wrong guesses, in order.
    pub fn wrong(&self) -> Vec<char> {
        self.guessed
            .iter()
            .copied()
            .filter(|c| !self.word.contains(*c))
            .collect()
    }

    pub fn misses(&self) -> usize {
        self.wrong().len()
    }

    pub fn is_won(&self) -> bool {
        self.word.chars().filter(|c| *c != ' ').all(|c| self.has_guessed(c))
    }

    pub fn is_lost(&self) -> bool {
        self.misses() >= MAX_MISSES
    }

    pub fn is_over(&self) -> bool {
        self.is_won() || self.is_lost()
    }

    /// The word one line per word, each letter hidden or guessed.
    pub fn lines(&self) -> Vec<Vec<Slot>> {
        self.word
            .split(' ')
            .map(|w| {
                w.chars()
                    .map(|c| {
                        if self.has_guessed(c) {
                            Slot::Guessed(c)
                        } else {
                            Slot::Hidden(c)
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// The word as text with `_` for letters not found yet, e.g. `s_a h_rs_`.
    pub fn masked(&self) -> String {
        self.word
            .chars()
            .map(|c| if c == ' ' || self.has_guessed(c) { c } else { '_' })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_until_letters_are_found() {
        let mut game = Hangman::new("sea horse", Lang::En);
        assert_eq!(game.masked(), "___ _____");
        assert_eq!(game.guess('s'), Guess::Hit(2));
        assert_eq!(game.masked(), "s__ ___s_");
        assert_eq!(game.guess('E'), Guess::Hit(2), "case does not matter");
        assert_eq!(game.masked(), "se_ ___se");
        assert_eq!(game.guess('z'), Guess::Miss);
        assert_eq!(game.guess('z'), Guess::Repeat);
        assert_eq!(game.guess('1'), Guess::Invalid);
        assert_eq!(game.guess(' '), Guess::Invalid);
        assert_eq!(game.misses(), 1);
        assert_eq!(game.wrong(), vec!['z']);
    }

    #[test]
    fn six_misses_lose_and_every_letter_wins() {
        let mut lost = Hangman::new("pearl", Lang::En);
        for c in "xyzqwj".chars() {
            assert!(!lost.is_over() || c == 'j');
            lost.guess(c);
        }
        assert!(lost.is_lost() && !lost.is_won() && lost.is_over());

        let mut won = Hangman::new("sea horse", Lang::En);
        for c in "seahor".chars() {
            assert!(!won.is_won());
            won.guess(c);
        }
        assert!(won.is_won() && !won.is_lost());
        assert_eq!(won.misses(), 0, "spaces are never guessed");
    }

    #[test]
    fn lines_split_at_spaces_and_show_found_letters() {
        let mut game = Hangman::new("tea pot", Lang::En);
        game.guess('t');
        let lines = game.lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], vec![Slot::Guessed('t'), Slot::Hidden('e'), Slot::Hidden('a')]);
        assert_eq!(lines[1][2], Slot::Guessed('t'));
    }

    #[test]
    fn games_survive_encoding_and_bad_states_are_refused() {
        let mut game = Hangman::new("sea horse", Lang::En);
        game.guess('s');
        game.guess('q');
        assert_eq!(Hangman::decode(&game.encode()), Some(game));
        for bad in ["", "|", "no bar", "UPPER|a", "pearl|A", "pearl|1", "|abc"] {
            assert_eq!(Hangman::decode(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn turkish_games_use_the_turkish_alphabet_and_survive_encoding() {
        let mut game = Hangman::new("çiçek bahçesi", Lang::Tr);
        assert_eq!(game.guess('İ'), Guess::Hit(2), "the dotted i is a letter of its own");
        assert_eq!(game.guess('ı'), Guess::Miss);
        assert_eq!(game.guess('Ç'), Guess::Hit(3));
        assert_eq!(game.guess('q'), Guess::Invalid);
        assert_eq!(game.guess('w'), Guess::Invalid);
        assert_eq!(game.masked(), "çiç__ ___ç__i");
        assert_eq!(Hangman::decode(&game.encode()), Some(game.clone()));
        assert!(game.encode().ends_with("|tr"));
        // An English game stays as it was and refuses Turkish letters.
        let mut english = Hangman::new("pearl", Lang::En);
        assert_eq!(english.guess('ç'), Guess::Invalid);
        assert_eq!(english.encode(), "pearl|");
        assert_eq!(Hangman::decode("pearl|ç"), None);
        assert_eq!(Hangman::decode("kedi|k|de"), None);
    }
}
