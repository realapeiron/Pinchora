//! Rock paper scissors, best of one, three or five.

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Rock,
    Paper,
    Scissors,
}

/// Who took a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Round {
    First,
    Second,
    Tie,
}

impl Move {
    pub const ALL: [Move; 3] = [Self::Rock, Self::Paper, Self::Scissors];

    /// One letter, as kept in custom ids and the database.
    pub fn key(self) -> char {
        match self {
            Self::Rock => 'r',
            Self::Paper => 'p',
            Self::Scissors => 's',
        }
    }

    pub fn from_key(key: char) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }

    /// The move's name in `lang` (`rps.rock`, `rps.paper`, `rps.scissors`).
    pub fn label(self, lang: Lang) -> String {
        match self {
            Self::Rock => t!(lang, "rps.rock"),
            Self::Paper => t!(lang, "rps.paper"),
            Self::Scissors => t!(lang, "rps.scissors"),
        }
    }

    pub fn emoji(self) -> &'static str {
        match self {
            Self::Rock => "🪨",
            Self::Paper => "📄",
            Self::Scissors => "✂️",
        }
    }

    fn beats(self, other: Move) -> bool {
        matches!(
            (self, other),
            (Self::Rock, Self::Scissors) | (Self::Paper, Self::Rock) | (Self::Scissors, Self::Paper)
        )
    }
}

pub fn round(first: Move, second: Move) -> Round {
    if first.beats(second) {
        Round::First
    } else if second.beats(first) {
        Round::Second
    } else {
        Round::Tie
    }
}

/// Round wins needed to take a best-of-`rounds` match: 1, 2 or 3.
pub fn wins_needed(rounds: u8) -> u8 {
    rounds / 2 + 1
}

#[cfg(test)]
mod tests {
    use crate::play::rps::*;

    #[test]
    fn every_pairing_has_the_right_winner() {
        use Move::*;
        assert_eq!(round(Rock, Scissors), Round::First);
        assert_eq!(round(Scissors, Rock), Round::Second);
        assert_eq!(round(Paper, Rock), Round::First);
        assert_eq!(round(Rock, Paper), Round::Second);
        assert_eq!(round(Scissors, Paper), Round::First);
        assert_eq!(round(Paper, Scissors), Round::Second);
        for m in Move::ALL {
            assert_eq!(round(m, m), Round::Tie);
            assert_eq!(Move::from_key(m.key()), Some(m));
        }
    }

    #[test]
    fn matches_need_a_majority() {
        assert_eq!([1, 3, 5].map(wins_needed), [1, 2, 3]);
    }

    #[test]
    fn moves_are_named_in_every_language() {
        assert_eq!(Move::ALL.map(|m| m.label(Lang::En)), ["Rock", "Paper", "Scissors"]);
        assert_eq!(Move::ALL.map(|m| m.label(Lang::Tr)), ["Taş", "Kâğıt", "Makas"]);
    }
}
