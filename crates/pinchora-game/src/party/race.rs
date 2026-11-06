//! Wordle Race: two to four players race to find the same five-letter word, each on a board of their own.
//! The first to solve it wins; the table sees only the colours of each other's guesses, never the letters.

use crate::play::wordle::{self, Tile};

pub use crate::play::wordle::{MAX_TRIES, WORD_LEN};

/// Seconds the whole race lasts before the word is shown and nobody wins.
pub const RACE_SECS: i64 = 420;

/// What a player's board has come to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Board {
    Playing,
    Solved {
        tries: usize,
    },
    /// Six wrong guesses.
    Failed,
}

/// Where a player stands, from their guesses against the word.
pub fn board(guesses: &[String], word: &str) -> Board {
    match guesses.iter().position(|g| g == word) {
        Some(at) => Board::Solved { tries: at + 1 },
        None if guesses.len() >= MAX_TRIES => Board::Failed,
        None => Board::Playing,
    }
}

/// Whether the race is over without a winner: every player has used up their tries.
pub fn everyone_failed(boards: &[Board]) -> bool {
    !boards.is_empty() && boards.iter().all(|b| *b == Board::Failed)
}

/// The colours of a guess, as marks a card can draw (1 absent, 2 present, 3 correct).
pub fn marks(guess: &str, word: &str) -> [u8; WORD_LEN] {
    wordle::score(guess, word).map(|tile| match tile {
        Tile::Absent => 1,
        Tile::Present => 2,
        Tile::Correct => 3,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guesses(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn a_board_is_playing_solved_or_failed() {
        assert_eq!(board(&[], "pearl"), Board::Playing);
        assert_eq!(board(&guesses(&["crane"]), "pearl"), Board::Playing);
        assert_eq!(
            board(&guesses(&["crane", "pearl"]), "pearl"),
            Board::Solved { tries: 2 }
        );
        let six = guesses(&["crane", "fight", "stomp", "bulky", "jazzy", "quick"]);
        assert_eq!(board(&six, "pearl"), Board::Failed);
        // Solving on the last try is still solving.
        let last = guesses(&["crane", "fight", "stomp", "bulky", "jazzy", "pearl"]);
        assert_eq!(board(&last, "pearl"), Board::Solved { tries: 6 });
    }

    #[test]
    fn the_race_is_lost_only_when_everyone_is() {
        assert!(everyone_failed(&[Board::Failed, Board::Failed]));
        assert!(!everyone_failed(&[Board::Failed, Board::Playing]));
        assert!(!everyone_failed(&[]));
    }

    #[test]
    fn marks_follow_wordle_colouring() {
        assert_eq!(marks("crane", "pearl"), [1, 2, 3, 1, 2]);
        assert_eq!(marks("pearl", "pearl"), [3; 5]);
    }
}
