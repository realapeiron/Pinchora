//! Bounded, local alpha-beta. No threads, processes, downloads or network in the engine.
use crate::play::chess::{Kind, Move, Position};
use std::time::{Duration, Instant};
pub struct Search {
    pub mv: Option<Move>,
    pub nodes: u32,
}
struct Budget {
    left: u32,
    nodes: u32,
    until: Instant,
}
fn value(p: &Position) -> i32 {
    (0..64)
        .filter_map(|sq| {
            p.piece_at(sq).map(|x| {
                let n = match x.kind {
                    Kind::Pawn => 100,
                    Kind::Knight => 320,
                    Kind::Bishop => 330,
                    Kind::Rook => 500,
                    Kind::Queen => 900,
                    Kind::King => 0,
                };
                if x.color == p.turn { n } else { -n }
            })
        })
        .sum()
}
fn search(p: Position, depth: u8, mut alpha: i32, beta: i32, b: &mut Budget) -> i32 {
    if b.left == 0 || Instant::now() >= b.until {
        return value(&p);
    }
    b.left -= 1;
    b.nodes += 1;
    let moves = p.legal_moves();
    if moves.is_empty() {
        return if p.in_check() { -30000 - i32::from(depth) } else { 0 };
    }
    if p.insufficient_material() || p.halfmove >= 100 {
        return 0;
    }
    if depth == 0 {
        return value(&p);
    }
    for mv in moves {
        let score = -search(p.make(mv), depth - 1, -beta, -alpha, b);
        alpha = alpha.max(score);
        if alpha >= beta || b.left == 0 || Instant::now() >= b.until {
            break;
        }
    }
    alpha
}
pub fn choose(p: &Position, level: u8) -> Search {
    let (depth, nodes, ms) = match level {
        1 => (1, 120, 20),
        3 => (3, 6000, 150),
        _ => (2, 1200, 75),
    };
    let mut budget = Budget {
        left: nodes,
        nodes: 0,
        until: Instant::now() + Duration::from_millis(ms),
    };
    let mut moves = p.legal_moves();
    moves.sort_by_key(|m| {
        std::cmp::Reverse(p.piece_at(m.to).map_or(0, |x| match x.kind {
            Kind::Queen => 9,
            Kind::Rook => 5,
            Kind::Bishop | Kind::Knight => 3,
            _ => 1,
        }))
    });
    let mut best = moves.first().copied();
    let mut score = i32::MIN;
    for mv in moves {
        if budget.left == 0 || Instant::now() >= budget.until {
            break;
        }
        let n = -search(p.make(mv), depth - 1, -32000, 32000, &mut budget);
        if n > score {
            score = n;
            best = Some(mv);
        }
    }
    Search {
        mv: best,
        nodes: budget.nodes,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn levels_are_legal_and_bounded() {
        for fen in [
            crate::play::chess::START_FEN,
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        ] {
            for (level, cap) in [(1, 120), (2, 1200), (3, 6000)] {
                let p = Position::from_fen(fen).unwrap();
                let s = choose(&p, level);
                assert!(p.legal_moves().contains(&s.mv.unwrap()));
                assert!(s.nodes <= cap);
            }
        }
    }
    #[test]
    fn drawn_leaves_score_zero_even_with_a_material_advantage() {
        for fen in ["4k3/8/8/8/8/8/R7/4K3 w - - 100 80", "4k3/8/8/8/8/8/B7/4K3 w - - 0 1"] {
            let p = Position::from_fen(fen).unwrap();
            assert!(value(&p) > 0);
            for depth in [0, 2] {
                let mut budget = Budget {
                    left: 100,
                    nodes: 0,
                    until: Instant::now() + Duration::from_secs(1),
                };
                assert_eq!(search(p, depth, -32000, 32000, &mut budget), 0);
            }
        }
    }
    #[test]
    fn takes_free_queen() {
        let p = Position::from_fen("4k3/8/8/8/8/8/q7/R3K3 w Q - 0 1").unwrap();
        assert_eq!(choose(&p, 2).mv.unwrap().uci(), "a1a2");
    }
    #[test]
    fn terminal_returns_none() {
        let p = Position::from_fen("7k/6Q1/5K2/8/8/8/8/8 b - - 0 1").unwrap();
        assert_eq!(p.turn, crate::play::chess::Color::Black);
        assert!(choose(&p, 3).mv.is_none());
    }
}
