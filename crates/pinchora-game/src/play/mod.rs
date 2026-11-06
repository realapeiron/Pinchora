//! The calm games: pure rules and content for connect four, rock-paper-scissors, hangman, wordle,
//! trivia and the board and puzzle games (Sea Fleet, Pixel Picross, Tide 2048, Pearl Checkers and Reef
//! Chess); the party games live in [`crate::party`]. Nothing here touches the database or Discord.

pub mod checkers;
pub mod chess;
pub mod connect4;
mod content;
pub mod fleet;
pub mod hangman;
pub mod picross;
pub mod rps;
pub mod tide2048;
pub mod trivia;
pub mod wordle;

pub use content::*;

use pinchora_core::{i18n::Lang, t};

use crate::party;

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
    Derby,
    ShareGrab,
    ShellGame,
    Heist,
    Surge,
    SeaFleet,
    Picross,
    Tide2048,
    Checkers,
    Chess,
    HotPearl,
    Acronym,
    Postcard,
    WordRace,
    WouldYouRather,
    TruthOrDare,
}

impl Game {
    pub const ALL: [Game; 26] = [
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
        Self::Derby,
        Self::ShareGrab,
        Self::ShellGame,
        Self::Heist,
        Self::Surge,
        Self::SeaFleet,
        Self::Picross,
        Self::Tide2048,
        Self::Checkers,
        Self::Chess,
        Self::HotPearl,
        Self::Acronym,
        Self::Postcard,
        Self::WordRace,
        Self::WouldYouRather,
        Self::TruthOrDare,
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
            Self::Derby => "derby",
            Self::ShareGrab => "sharegrab",
            Self::ShellGame => "shellgame",
            Self::Heist => "heist",
            Self::Surge => "surge",
            Self::SeaFleet => "seafleet",
            Self::Picross => "picross",
            Self::Tide2048 => "tide2048",
            Self::Checkers => "checkers",
            Self::Chess => "chess",
            Self::HotPearl => "hotpearl",
            Self::Acronym => "acronym",
            Self::Postcard => "postcard",
            Self::WordRace => "wordrace",
            Self::WouldYouRather => "wouldyourrather",
            Self::TruthOrDare => "truthordare",
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
            Self::Derby => "Crab Derby",
            Self::ShareGrab => "Share or Grab",
            Self::ShellGame => "Shell Game",
            Self::Heist => "Treasure Heist",
            Self::Surge => "Storm Surge",
            Self::SeaFleet => "Sea Fleet",
            Self::Picross => "Pixel Picross",
            Self::Tide2048 => "Tide 2048",
            Self::Checkers => "Pearl Checkers",
            Self::Chess => "Reef Chess",
            Self::HotPearl => "Hot Pearl",
            Self::Acronym => "Acronym Tide",
            Self::Postcard => "Caption the Postcard",
            Self::WordRace => "Wordle Race",
            Self::WouldYouRather => "Would You Rather",
            Self::TruthOrDare => "Truth or Dare",
        }
    }

    /// The game's name in `lang` (`game.<key>` in the catalogs); in English it is [`Game::label`].
    pub fn name(self, lang: Lang) -> String {
        t!(lang, &format!("game.{}", self.key()))
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
            Self::Derby => "🦀",
            Self::ShareGrab => "🤝",
            Self::ShellGame => "🐚",
            Self::Heist => "💰",
            Self::Surge => "⛈️",
            Self::SeaFleet => "🚢",
            Self::Picross => "🧩",
            Self::Tide2048 => "🌊",
            Self::Checkers => "⚪",
            Self::Chess => "♟️",
            Self::HotPearl => "🦪",
            Self::Acronym => "🌊",
            Self::Postcard => "💌",
            Self::WordRace => "🏁",
            Self::WouldYouRather => "⚖️",
            Self::TruthOrDare => "🎲",
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
            Self::Derby => 20,
            // The three below pay by the shell through their own daily limits, see `party`.
            Self::ShareGrab => party::share::POOL / 2,
            Self::ShellGame => party::shells::payout(party::shells::MIN_STAKE),
            Self::Heist => party::heist::Tier::Haul.each(),
            Self::Surge => party::surge::WIN_BASE,
            Self::SeaFleet => 25,
            Self::Picross => 10,
            Self::Tide2048 => 10,
            Self::Checkers => 20,
            Self::Chess => 30,
            // The party games pay by how the game went; these are about what a win is worth.
            Self::HotPearl => 32,
            Self::Acronym => 25,
            Self::Postcard => 25,
            Self::WordRace => 25,
            Self::WouldYouRather => 1,
            Self::TruthOrDare => 4,
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
            Self::Derby => 5,
            Self::ShareGrab => 6,
            Self::ShellGame => i64::from(party::shells::DAILY_PLAYS),
            Self::Heist => 6,
            Self::Surge => 3,
            Self::SeaFleet | Self::Picross | Self::Tide2048 | Self::Checkers | Self::Chess => 5,
            // Paid games a day; later ones are still counted in /play stats.
            Self::HotPearl => 6,
            Self::Acronym => 6,
            Self::Postcard => 6,
            Self::WordRace => 6,
            Self::WouldYouRather => 10,
            Self::TruthOrDare => 5,
        }
    }

    /// How many moves a two-player board game must have had before a win pays the system's reward: plies
    /// of chess and of checkers, shots of both captains in Sea Fleet. A game cut shorter (one player
    /// resigning at once) is still a win in the stats, but two friends cannot farm shells with it.
    pub fn min_moves(self) -> usize {
        match self {
            Self::Chess => 10,
            Self::Checkers => 6,
            Self::SeaFleet => 10,
            _ => 0,
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
        assert_eq!(Game::from_key("poker"), None);
    }

    #[test]
    fn every_game_has_a_name_in_every_language() {
        for game in Game::ALL {
            assert_eq!(game.name(Lang::En), game.label());
            assert_ne!(game.name(Lang::Tr), format!("game.{}", game.key()));
        }
    }

    #[test]
    fn only_the_two_player_board_games_ask_for_a_minimum_of_moves() {
        let asking: Vec<_> = Game::ALL.into_iter().filter(|g| g.min_moves() > 0).collect();
        assert_eq!(asking, [Game::SeaFleet, Game::Checkers, Game::Chess]);
        assert_eq!(
            (
                Game::Chess.min_moves(),
                Game::Checkers.min_moves(),
                Game::SeaFleet.min_moves()
            ),
            (10, 6, 10)
        );
    }
}
