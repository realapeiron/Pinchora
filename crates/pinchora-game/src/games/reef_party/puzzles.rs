use crate::games::reef_party::next;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Maze {
    pub cells: Vec<bool>,
    pub at: usize,
    pub moves: u32,
}
impl Maze {
    pub fn new(mut seed: u64) -> Self {
        let mut cells = vec![false; 49];
        cells[0] = true;
        let mut stack = vec![0];
        while let Some(&at) = stack.last() {
            let x = at % 7;
            let y = at / 7;
            let candidates: Vec<usize> = [(0i32, 2i32), (2, 0), (-2, 0), (0, -2)]
                .into_iter()
                .filter_map(|(dx, dy)| {
                    let (a, b) = (x as i32 + dx, y as i32 + dy);
                    ((0..7).contains(&a) && (0..7).contains(&b)).then_some((b * 7 + a) as usize)
                })
                .filter(|&i| !cells[i])
                .collect();
            if candidates.is_empty() {
                stack.pop();
            } else {
                let to = candidates[next(&mut seed) as usize % candidates.len()];
                cells[(at + to) / 2] = true;
                cells[to] = true;
                stack.push(to);
            }
        }
        Self { cells, at: 0, moves: 0 }
    }
    pub fn step(&mut self, dir: &str) -> bool {
        let (x, y) = (self.at % 7, self.at / 7);
        let to = match dir {
            "up" if y > 0 => self.at - 7,
            "down" if y < 6 => self.at + 7,
            "left" if x > 0 => self.at - 1,
            "right" if x < 6 => self.at + 1,
            _ => return false,
        };
        if !self.cells[to] {
            return false;
        }
        self.at = to;
        self.moves += 1;
        true
    }
    pub fn won(&self) -> bool {
        self.at == 48
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
    pub cells: Vec<u8>,
    pub selected: Option<usize>,
    pub score: u32,
    pub moves: u32,
    pub seed: u64,
}
impl Match {
    pub fn new(seed: u64) -> Self {
        let mut s = Self {
            cells: vec![0; 16],
            selected: None,
            score: 0,
            moves: 0,
            seed,
        };
        s.shuffle();
        s
    }
    fn shuffle(&mut self) {
        for _ in 0..100 {
            for i in 0..16 {
                self.cells[i] = (next(&mut self.seed) % 4 + 1) as u8;
            }
            if self.matches().is_empty() && self.playable() {
                return;
            }
        }
        self.cells = vec![1, 2, 1, 3, 2, 1, 3, 4, 3, 4, 2, 1, 4, 3, 4, 2];
    }
    pub fn matches(&self) -> Vec<usize> {
        let mut found = vec![];
        for i in 0..16 {
            for stride in [1, 4] {
                if (stride == 1 && i % 4 <= 1) || (stride == 4 && i / 4 <= 1) {
                    let v = self.cells[i];
                    if v > 0 && self.cells[i + stride] == v && self.cells[i + 2 * stride] == v {
                        found.extend([i, i + stride, i + 2 * stride]);
                    }
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        found
    }
    fn adjacent(a: usize, b: usize) -> bool {
        a.abs_diff(b) == 4 || a / 4 == b / 4 && a.abs_diff(b) == 1
    }
    pub fn playable(&self) -> bool {
        for a in 0..16 {
            for b in a + 1..16 {
                if Self::adjacent(a, b) {
                    let mut copy = self.clone();
                    copy.cells.swap(a, b);
                    if !copy.matches().is_empty() {
                        return true;
                    }
                }
            }
        }
        false
    }
    pub fn select(&mut self, at: usize) -> bool {
        if at >= 16 {
            return false;
        }
        let Some(from) = self.selected else {
            self.selected = Some(at);
            return true;
        };
        if at == from {
            self.selected = None;
            return true;
        }
        if !Self::adjacent(from, at) {
            self.selected = Some(at);
            return true;
        }
        self.cells.swap(from, at);
        let hit = self.matches();
        if hit.is_empty() {
            self.cells.swap(from, at);
            return false;
        }
        self.selected = None;
        self.moves += 1;
        for _ in 0..32 {
            let hit = self.matches();
            if hit.is_empty() {
                break;
            }
            self.score += hit.len() as u32;
            for i in hit {
                self.cells[i] = 0;
            }
            for col in 0..4 {
                let kept: Vec<u8> = (0..4)
                    .rev()
                    .map(|row| self.cells[row * 4 + col])
                    .filter(|v| *v > 0)
                    .collect();
                for row in 0..4 {
                    self.cells[(3 - row) * 4 + col] = kept
                        .get(row)
                        .copied()
                        .unwrap_or_else(|| (next(&mut self.seed) % 4 + 1) as u8);
                }
            }
        }
        if !self.matches().is_empty() || !self.playable() {
            self.shuffle();
        }
        true
    }
}
#[cfg(test)]
mod tests {
    use crate::games::reef_party::puzzles::*;
    #[test]
    fn mazes_are_connected() {
        for seed in 0..100 {
            let m = Maze::new(seed);
            let mut seen = vec![0];
            let mut n = 0;
            while n < seen.len() {
                let i = seen[n];
                n += 1;
                for j in 0..49 {
                    if m.cells[j] && (i / 7 == j / 7 && i.abs_diff(j) == 1 || i.abs_diff(j) == 7) && !seen.contains(&j)
                    {
                        seen.push(j);
                    }
                }
            }
            assert!(seen.contains(&48));
        }
    }
    #[test]
    fn boards_start_playable_without_matches() {
        for seed in 0..100 {
            let m = Match::new(seed);
            assert!(m.matches().is_empty());
            assert!(m.playable());
        }
    }
    #[test]
    fn bad_swaps_do_not_consume_moves() {
        let mut m = Match::new(1);
        m.select(0);
        m.select(15);
        assert_eq!(m.moves, 0);
        assert!(!m.select(20));
    }
}
