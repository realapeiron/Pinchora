use super::*;

/// A game from the start position: its moves, its position and how often each position came up.
#[derive(Debug, Clone)]
pub struct Game {
    pub position: Position,
    pub(super) moves: Vec<Move>,
    pub(super) keys: Vec<u64>,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        let position = Position::start();
        Self {
            position,
            moves: Vec::new(),
            keys: vec![position.key()],
        }
    }

    /// Replays a list of `e2e4` style moves; `None` when any of them is illegal.
    pub fn from_moves(text: &str) -> Option<Self> {
        let mut game = Self::new();
        for word in text.split_whitespace() {
            game.play(Move::from_uci(word)?)?;
        }
        Some(game)
    }

    /// The moves so far in the form [`Game::from_moves`] reads.
    pub fn moves_text(&self) -> String {
        self.moves.iter().map(|m| m.uci()).collect::<Vec<_>>().join(" ")
    }

    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// Plies played.
    pub fn plies(&self) -> usize {
        self.moves.len()
    }

    /// Plays `mv` if it is legal; returns `None` (and changes nothing) when it is not.
    pub fn play(&mut self, mv: Move) -> Option<()> {
        if !self.position.legal_moves().contains(&mv) {
            return None;
        }
        self.position = self.position.make(mv);
        self.moves.push(mv);
        self.keys.push(self.position.key());
        Some(())
    }

    /// The last `count` moves in standard notation, oldest first, numbered (`12. Nf3 Nc6`).
    pub fn recent_san(&self, count: usize) -> String {
        let mut replay = Position::start();
        let mut words = Vec::new();
        for (i, &mv) in self.moves.iter().enumerate() {
            let san = replay.san(mv);
            words.push(if i % 2 == 0 {
                format!("{}. {san}", i / 2 + 1)
            } else {
                san
            });
            replay = replay.make(mv);
        }
        let from = words.len().saturating_sub(count);
        // Start on a white move so every black reply has its number.
        let from = if from % 2 == 1 { from - 1 } else { from };
        words[from..].join(" ")
    }

    pub fn outcome(&self) -> Option<Outcome> {
        let position = &self.position;
        if position.legal_moves().is_empty() {
            return Some(if position.in_check() {
                Outcome::Checkmate {
                    winner: position.turn.other(),
                }
            } else {
                Outcome::Stalemate
            });
        }
        if position.insufficient_material() {
            return Some(Outcome::Insufficient);
        }
        let now = self.keys.last().copied().unwrap_or_default();
        if self.keys.iter().filter(|&&k| k == now).count() >= 3 {
            return Some(Outcome::Repetition);
        }
        if position.halfmove >= 100 {
            return Some(Outcome::FiftyMoves);
        }
        None
    }
}
