//! The calm games: pure rules and content for connect four, rock-paper-scissors, hangman, wordle
//! and trivia. Nothing here touches the database or Discord.

pub mod connect4;
mod content;
pub mod hangman;
pub mod rps;
pub mod trivia;
pub mod wordle;

pub use content::*;

/// Every game that keeps win counts in `/play stats`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    TicTacToe,
    Connect4,
    Rps,
    Hangman,
    Wordle,
    Trivia,
    CritterDrop,
    Bottle,
    Beachcomb,
    CrabPot,
}

impl Game {
    pub const ALL: [Game; 10] = [
        Self::TicTacToe,
        Self::Connect4,
        Self::Rps,
        Self::Hangman,
        Self::Wordle,
        Self::Trivia,
        Self::CritterDrop,
        Self::Bottle,
        Self::Beachcomb,
        Self::CrabPot,
    ];

    /// What the database and the reward-cap counters call it.
    pub fn key(self) -> &'static str {
        match self {
            Self::TicTacToe => "tictactoe",
            Self::Connect4 => "connect4",
            Self::Rps => "rps",
            Self::Hangman => "hangman",
            Self::Wordle => "wordle",
            Self::Trivia => "trivia",
            Self::CritterDrop => "critterdrop",
            Self::Bottle => "bottle",
            Self::Beachcomb => "beachcomb",
            Self::CrabPot => "crabpot",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::TicTacToe => "Tic-tac-toe",
            Self::Connect4 => "Connect four",
            Self::Rps => "Rock paper scissors",
            Self::Hangman => "Hangman",
            Self::Wordle => "Wordle",
            Self::Trivia => "Trivia",
            Self::CritterDrop => "Critter drop",
            Self::Bottle => "Message in a bottle",
            Self::Beachcomb => "Beachcomb",
            Self::CrabPot => "Crab pot",
        }
    }

    pub fn emoji(self) -> &'static str {
        match self {
            Self::TicTacToe => "❌",
            Self::Connect4 => "🔴",
            Self::Rps => "✂️",
            Self::Hangman => "🏰",
            Self::Wordle => "🟩",
            Self::Trivia => "🧠",
            Self::CritterDrop => "🦀",
            Self::Bottle => "🍾",
            Self::Beachcomb => "🐚",
            Self::CrabPot => "🦞",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|g| g.key() == key)
    }

    /// Shells the system pays for one win (trivia and wordle pay by difficulty and tries; this is
    /// their base).
    pub fn win_reward(self) -> i64 {
        match self {
            Self::TicTacToe => 30,
            Self::Connect4 => 25,
            Self::Rps => 15,
            Self::Hangman => 15,
            Self::Wordle => 10,
            Self::Trivia => 8,
            // The collection games pay by what they turn up; these are about an average one.
            Self::CritterDrop => 17,
            Self::Bottle => 34,
            Self::Beachcomb => 17,
            Self::CrabPot => 170,
        }
    }

    /// How many wins per day the system pays for, per player.
    pub fn daily_cap(self) -> i64 {
        match self {
            Self::TicTacToe => 5,
            Self::Connect4 => 5,
            Self::Rps => 5,
            Self::Hangman => 6,
            Self::Wordle => 6,
            Self::Trivia => 12,
            // Plays a day that pay shells; later ones still fill the dex.
            Self::CritterDrop => 20,
            Self::Bottle => 10,
            Self::Beachcomb => 24,
            Self::CrabPot => 3,
        }
    }
}

/// Smallest and largest friendly wager on a duel, in shells.
pub const MIN_BET: i64 = 10;
pub const MAX_BET: i64 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_keys_round_trip() {
        for game in Game::ALL {
            assert_eq!(Game::from_key(game.key()), Some(game));
            assert!(game.win_reward() > 0 && game.daily_cap() > 0);
        }
        assert_eq!(Game::from_key("chess"), None);
    }
}
