use super::*;

/// The duels that keep their state in the database, with a challenge to accept first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Connect4,
    Rps,
    ShareGrab,
    SeaFleet,
    Checkers,
    Chess,
    Othello,
}

impl Kind {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Connect4 => "connect4",
            Self::Rps => "rps",
            Self::ShareGrab => "sharegrab",
            Self::SeaFleet => "seafleet",
            Self::Checkers => "checkers",
            Self::Chess => "chess",
            Self::Othello => "othello",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        match key {
            "connect4" => Some(Self::Connect4),
            "rps" => Some(Self::Rps),
            "sharegrab" => Some(Self::ShareGrab),
            "seafleet" => Some(Self::SeaFleet),
            "checkers" => Some(Self::Checkers),
            "chess" => Some(Self::Chess),
            "othello" => Some(Self::Othello),
            _ => None,
        }
    }

    /// The game it keeps the records of, which also names it.
    pub(super) fn game(self) -> Play {
        match self {
            Self::Connect4 => Play::Connect4,
            Self::Rps => Play::Rps,
            Self::ShareGrab => Play::ShareGrab,
            Self::SeaFleet => Play::SeaFleet,
            Self::Checkers => Play::Checkers,
            Self::Chess => Play::Chess,
            Self::Othello => Play::Othello,
        }
    }

    /// The custom id prefix of its buttons.
    pub(super) fn prefix(self) -> &'static str {
        match self {
            Self::Connect4 => "c4",
            Self::Rps => "rp",
            Self::ShareGrab => "sh",
            Self::SeaFleet => "bs",
            Self::Checkers => "ck",
            Self::Chess => "cc",
            Self::Othello => "of",
        }
    }

    /// Whether the player who runs out of time loses a wagered game (the other takes the pot), so letting the
    /// clock run cannot cancel a bet. Share or Grab settles its own no-shows.
    pub(super) fn forfeits(self) -> bool {
        !matches!(self, Self::ShareGrab)
    }

    /// How long a player has for the first turn or pick once the challenge is accepted.
    pub(super) fn first_secs(self) -> i64 {
        match self {
            Self::Connect4 | Self::Rps => TURN_SECS,
            Self::ShareGrab => pinchora_game::party::share::PICK_SECS,
            Self::SeaFleet => board::FLEET_PLACE_SECS,
            Self::Checkers | Self::Othello => board::CHECKERS_SECS,
            Self::Chess => board::CHESS_SECS,
        }
    }
}
