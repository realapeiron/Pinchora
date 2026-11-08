//! Transport-independent room game state. All board moves delegate to the existing rules.
use super::{Game as Kind, PlayContent, checkers, chess};
use pinchora_core::i18n::Lang;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Board {
    Checkers {
        cells: String,
        turn: u8,
        quiet: u32,
        plies: usize,
    },
    Chess {
        moves: String,
    },
    Trivia {
        question: String,
        lang: String,
        order: [usize; 4],
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub board: Board,
    pub phase: String,
    pub winner: Option<i16>,
    pub reason: Option<String>,
}
impl Session {
    pub fn new(kind: &str, lang: Lang, seed: u64) -> Option<Self> {
        let board = match kind {
            "checkers" => Board::Checkers {
                cells: checkers::Game::new().cells_text(),
                turn: 1,
                quiet: 0,
                plies: 0,
            },
            "chess" => Board::Chess { moves: String::new() },
            "trivia" => {
                let content = PlayContent::for_lang(lang);
                let q = content.trivia.get(seed as usize % content.trivia.len())?;
                Board::Trivia {
                    question: q.id.clone(),
                    lang: lang.code().into(),
                    order: super::trivia::permutation(seed),
                }
            }
            _ => return None,
        };
        Some(Self {
            board,
            phase: "waiting".into(),
            winner: None,
            reason: None,
        })
    }
    pub fn kind(&self) -> Kind {
        match self.board {
            Board::Checkers { .. } => Kind::Checkers,
            Board::Chess { .. } => Kind::Chess,
            Board::Trivia { .. } => Kind::Trivia,
        }
    }
    pub fn turn(&self) -> Option<i16> {
        match &self.board {
            Board::Checkers { turn, .. } => Some(i16::from(*turn) - 1),
            Board::Chess { moves } => Some((moves.split_whitespace().count() % 2) as i16),
            _ => None,
        }
    }
    pub fn seconds(&self) -> i64 {
        match self.board {
            Board::Checkers { .. } => 180,
            Board::Chess { .. } => 300,
            Board::Trivia { .. } => 30,
        }
    }
    pub fn plies(&self) -> usize {
        match &self.board {
            Board::Checkers { plies, .. } => *plies,
            Board::Chess { moves } => moves.split_whitespace().count(),
            _ => 0,
        }
    }
    pub fn finish(&mut self, winner: Option<i16>, reason: &str) {
        self.phase = "done".into();
        self.winner = winner;
        self.reason = Some(reason.into());
    }
    pub fn play(&mut self, side: i16, code: &str) -> anyhow::Result<()> {
        anyhow::ensure!(self.phase == "active" && self.turn() == Some(side), "not_your_turn");
        let outcome = match &mut self.board {
            Board::Checkers {
                cells,
                turn,
                quiet,
                plies,
            } => {
                let mut game =
                    checkers::Game::from_parts(cells, *turn, *quiet).ok_or_else(|| anyhow::anyhow!("invalid_state"))?;
                let mv = game
                    .legal_moves()
                    .into_iter()
                    .find(|m| m.code() == code)
                    .ok_or_else(|| anyhow::anyhow!("illegal_move"))?;
                game.play(&mv).ok_or_else(|| anyhow::anyhow!("illegal_move"))?;
                *cells = game.cells_text();
                *turn = game.turn;
                *quiet = game.quiet;
                *plies += 1;
                game.outcome().map(|o| match o {
                    checkers::Outcome::Won(n) => Some(i16::from(n) - 1),
                    checkers::Outcome::Draw => None,
                })
            }
            Board::Chess { moves } => {
                anyhow::ensure!(moves.len() < 20000, "game_too_long");
                let mut game = chess::Game::from_moves(moves).ok_or_else(|| anyhow::anyhow!("invalid_state"))?;
                let mv = chess::Move::from_uci(code).ok_or_else(|| anyhow::anyhow!("illegal_move"))?;
                game.play(mv).ok_or_else(|| anyhow::anyhow!("illegal_move"))?;
                *moves = game.moves_text();
                game.outcome()
                    .map(|o| o.winner().map(|c| if c == chess::Color::White { 0 } else { 1 }))
            }
            _ => anyhow::bail!("invalid_action"),
        };
        if let Some(winner) = outcome {
            self.finish(winner, "completed");
        }
        Ok(())
    }
    pub fn question(&self) -> Option<(&super::Question, [usize; 4])> {
        let Board::Trivia { question, lang, order } = &self.board else {
            return None;
        };
        Some((
            PlayContent::for_lang(Lang::from_code(lang)?).question(question)?,
            *order,
        ))
    }
    /// Only this allowlisted projection crosses a transport. Trivia identity, seed and answer stay private.
    pub fn projection(&self) -> Value {
        let mut out = json!({"phase":self.phase,"winner":self.winner,"reason":self.reason,"turn":self.turn()});
        match &self.board {
            Board::Checkers { cells, turn, quiet, .. } => {
                out["cells"] = json!(cells);
                if self.phase == "active" {
                    out["legal"] = json!(
                        checkers::Game::from_parts(cells, *turn, *quiet)
                            .map(|g| g
                                .legal_moves()
                                .iter()
                                .map(|m| json!({"code":m.code(),"label":m.text(),"from":m.from(),"to":m.to()}))
                                .collect::<Vec<_>>())
                            .unwrap_or_default()
                    );
                }
            }
            Board::Chess { moves } => {
                if let Some(g) = chess::Game::from_moves(moves) {
                    out["fen"] = json!(g.position.to_fen());
                    out["check"] = json!(g.position.in_check());
                    if self.phase == "active" {
                        out["legal"] = json!(
                            g.position
                                .legal_moves()
                                .iter()
                                .map(|m| json!({"code":m.uci(),"label":m.uci(),"from":m.from,"to":m.to}))
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
            Board::Trivia { .. } => {
                if let Some((q, order)) = self.question() {
                    out["question"] = json!(q.question);
                    out["options"] = json!(q.shown(order));
                    if self.phase == "done" {
                        out["correct"] = json!(q.correct_slot(order));
                    }
                }
            }
        }
        out
    }
    /// No multiplayer house can mint rewards, even with two different accounts.
    pub fn reward(&self, plaza: bool, players: usize) -> i64 {
        if !plaza || players < 2 || self.reason.as_deref() == Some("timeout") || self.plies() < self.kind().min_moves()
        {
            return 0;
        }
        self.question()
            .map_or(self.kind().win_reward(), |(q, _)| q.difficulty.reward())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trivia_projection_conceals_solution_and_identity_until_reveal() {
        for lang in [Lang::En, Lang::Tr] {
            let mut s = Session::new("trivia", lang, 42).unwrap();
            let p = s.projection();
            assert!(p.get("correct").is_none());
            assert!(p.get("order").is_none());
            assert!(p.get("lang").is_none());
            assert_eq!(p["question"], s.question().unwrap().0.question);
            s.finish(None, "deadline");
            assert!(s.projection()["correct"].is_number());
        }
    }
    #[test]
    fn private_alts_and_self_play_never_pay() {
        let mut s = Session::new("chess", Lang::En, 0).unwrap();
        s.board = Board::Chess { moves: "a ".repeat(12) };
        assert_eq!(s.reward(false, 2), 0);
        assert_eq!(s.reward(true, 1), 0);
        assert_eq!(s.reward(true, 2), Kind::Chess.win_reward());
        s.finish(Some(1), "timeout");
        assert_eq!(s.reward(true, 2), 0);
    }
    #[test]
    fn chess_full_match_and_stale_turn() {
        let mut s = Session::new("chess", Lang::En, 0).unwrap();
        s.phase = "active".into();
        assert!(s.play(1, "e7e5").is_err());
        for (n, m) in ["f2f3", "e7e5", "g2g4", "d8h4"].iter().enumerate() {
            s.play((n % 2) as i16, m).unwrap();
        }
        assert_eq!(s.winner, Some(1));
        assert_eq!(s.phase, "done");
        assert!(s.play(0, "e2e4").is_err());
    }
    #[test]
    fn checkers_must_use_shared_legal_moves() {
        let mut s = Session::new("checkers", Lang::En, 0).unwrap();
        s.phase = "active".into();
        assert!(s.play(0, "0.31").is_err());
        let mv = s.projection()["legal"][0]["code"].as_str().unwrap().to_owned();
        s.play(0, &mv).unwrap();
        assert_eq!(s.turn(), Some(1));
    }
}

#[cfg(test)]
mod adapter_tests {
    use super::*;
    fn moves(codes: &[&str]) -> Session {
        let mut s = Session::new("chess", Lang::En, 0).unwrap();
        s.phase = "active".into();
        for (i, code) in codes.iter().enumerate() {
            s.play((i % 2) as i16, code).unwrap();
        }
        s
    }
    #[test]
    fn promotion_requires_a_choice_and_allows_underpromotion() {
        let mut s = moves(&["a2a4", "h7h5", "a4a5", "h5h4", "a5a6", "h4h3", "a6b7", "h3g2"]);
        assert!(s.play(0, "b7a8").is_err());
        s.play(0, "b7a8n").unwrap();
        assert!(s.projection()["fen"].as_str().unwrap().starts_with('N'));
    }
    #[test]
    fn adapter_preserves_castling_and_en_passant() {
        let castle = moves(&["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6", "e1g1"]);
        assert!(castle.projection()["fen"].as_str().unwrap().contains("RNBQ1RK1"));
        let ep = moves(&["e2e4", "a7a6", "e4e5", "d7d5", "e5d6"]);
        assert!(ep.projection()["fen"].as_str().unwrap().contains("p2P4"));
    }
    #[test]
    fn complete_checkers_match_uses_only_shared_legal_moves() {
        let mut s = Session::new("checkers", Lang::En, 0).unwrap();
        s.phase = "active".into();
        for _ in 0..400 {
            if s.phase == "done" {
                break;
            }
            let p = s.projection();
            let code = p["legal"][0]["code"].as_str().unwrap();
            s.play(s.turn().unwrap(), code).unwrap();
        }
        assert_eq!(s.phase, "done");
    }
}
