//! Pure rules for quick canvas games. Random orders are supplied by the caller.
use serde::{Deserialize, Serialize};

/// Equal cards count as a correct prediction so neither button is penalized by a tie.
pub fn higher_correct(current: u8, next: u8, higher: bool) -> bool {
    next == current || (next > current) == higher
}

pub fn anagram_correct(expected: &str, submitted: &str) -> bool {
    expected.replace('İ', "i").to_lowercase() == submitted.trim().replace('İ', "i").to_lowercase()
}

pub fn typing_score(expected: &str, submitted: &str, seconds: i64) -> (bool, i64) {
    let wpm = (submitted.chars().count() as i64 * 12 / seconds.max(1)).min(500);
    (expected == submitted, wpm)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Temperature {
    Hot,
    Warm,
    Cold,
}
pub fn temperature(answer: u8, guess: u8) -> Temperature {
    match answer.abs_diff(guess) {
        0..=5 => Temperature::Hot,
        6..=15 => Temperature::Warm,
        _ => Temperature::Cold,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Memory {
    pub pairs: Vec<u8>,
    pub shown: Vec<bool>,
    pub first: Option<usize>,
    #[serde(default)]
    pub last: Option<(usize, usize)>,
    pub misses: u8,
}
impl Memory {
    pub fn new(pairs: Vec<u8>) -> Option<Self> {
        if pairs.len() != 16 || (0..8).any(|n| pairs.iter().filter(|&&p| p == n).count() != 2) {
            return None;
        }
        Some(Self {
            pairs,
            shown: vec![false; 16],
            first: None,
            last: None,
            misses: 0,
        })
    }
    pub fn pick(&mut self, cell: usize) -> Option<bool> {
        if cell >= 16 || self.shown[cell] || self.first == Some(cell) {
            return None;
        }
        self.last = None;
        if let Some(first) = self.first.take() {
            let matched = self.pairs[first] == self.pairs[cell];
            if matched {
                self.shown[first] = true;
                self.shown[cell] = true;
            } else {
                self.misses = self.misses.saturating_add(1);
                self.last = Some((first, cell));
            }
            Some(matched)
        } else {
            self.first = Some(cell);
            Some(false)
        }
    }
    pub fn won(&self) -> bool {
        self.shown.iter().all(|&v| v)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mines {
    pub size: usize,
    pub mines: Vec<bool>,
    pub open: Vec<bool>,
    pub flags: Vec<bool>,
    pub first: bool,
    pub flag_mode: bool,
    pub lost: bool,
}
impl Mines {
    pub fn new(size: usize) -> Option<Self> {
        if size != 8 && size != 10 {
            return None;
        }
        let n = size * size;
        Some(Self {
            size,
            mines: vec![false; n],
            open: vec![false; n],
            flags: vec![false; n],
            first: true,
            flag_mode: false,
            lost: false,
        })
    }
    pub fn neighbors(&self, cell: usize) -> Vec<usize> {
        let row = cell / self.size;
        let col = cell % self.size;
        let mut out = Vec::new();
        for dr in [-1isize, 0, 1] {
            for dc in [-1isize, 0, 1] {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let (r, c) = (row as isize + dr, col as isize + dc);
                if r >= 0 && c >= 0 && r < self.size as isize && c < self.size as isize {
                    out.push(r as usize * self.size + c as usize);
                }
            }
        }
        out
    }
    pub fn count(&self, cell: usize) -> usize {
        self.neighbors(cell).iter().filter(|&&n| self.mines[n]).count()
    }
    /// The first click and its neighbors are safe. The caller supplies a server-side shuffle.
    pub fn reveal(&mut self, cell: usize, draws: &[usize]) -> Option<()> {
        if cell >= self.open.len() || self.open[cell] || self.flags[cell] || self.lost {
            return None;
        }
        if self.first {
            let mut safe = self.neighbors(cell);
            safe.push(cell);
            let count = if self.size == 8 { 10 } else { 15 };
            let mut placed = 0;
            for &n in draws {
                if n < self.mines.len() && !safe.contains(&n) && !self.mines[n] {
                    self.mines[n] = true;
                    placed += 1;
                    if placed == count {
                        break;
                    }
                }
            }
            if placed != count {
                return None;
            }
            self.first = false;
        }
        if self.mines[cell] {
            self.open[cell] = true;
            self.lost = true;
            return Some(());
        }
        let mut stack = vec![cell];
        while let Some(n) = stack.pop() {
            if self.open[n] || self.flags[n] {
                continue;
            }
            self.open[n] = true;
            if self.count(n) == 0 {
                stack.extend(self.neighbors(n).into_iter().filter(|&v| !self.open[v]));
            }
        }
        Some(())
    }
    pub fn flag(&mut self, cell: usize) -> bool {
        if cell >= self.open.len() || self.open[cell] || self.lost {
            return false;
        }
        self.flags[cell] = !self.flags[cell];
        true
    }
    pub fn won(&self) -> bool {
        !self.first && self.open.iter().enumerate().all(|(i, &v)| self.mines[i] || v)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Othello {
    /// 0 empty, 1 pearl, 2 coral.
    pub cells: Vec<u8>,
    pub turn: u8,
    pub passes: u8,
}
impl Othello {
    pub fn new() -> Self {
        let mut cells = vec![0; 64];
        cells[27] = 2;
        cells[28] = 1;
        cells[35] = 1;
        cells[36] = 2;
        Self {
            cells,
            turn: 1,
            passes: 0,
        }
    }
    pub fn flips(&self, at: usize, side: u8) -> Vec<usize> {
        if at >= 64 || self.cells[at] != 0 || !(1..=2).contains(&side) {
            return vec![];
        }
        let mut all = vec![];
        let (r, c) = ((at / 8) as i32, (at % 8) as i32);
        for dr in -1..=1 {
            for dc in -1..=1 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let (mut y, mut x) = (r + dr, c + dc);
                let mut line = vec![];
                while (0..8).contains(&y) && (0..8).contains(&x) {
                    let n = (y * 8 + x) as usize;
                    if self.cells[n] == 3 - side {
                        line.push(n);
                    } else {
                        if self.cells[n] == side {
                            all.extend(line);
                        }
                        break;
                    }
                    y += dr;
                    x += dc;
                }
            }
        }
        all
    }
    pub fn legal(&self, side: u8) -> Vec<usize> {
        (0..64).filter(|&i| !self.flips(i, side).is_empty()).collect()
    }
    pub fn play(&mut self, at: usize) -> bool {
        let flips = self.flips(at, self.turn);
        if flips.is_empty() {
            return false;
        }
        self.cells[at] = self.turn;
        for n in flips {
            self.cells[n] = self.turn;
        }
        self.turn = 3 - self.turn;
        if self.legal(self.turn).is_empty() {
            self.turn = 3 - self.turn;
            self.passes = if self.legal(self.turn).is_empty() { 2 } else { 1 };
        } else {
            self.passes = 0;
        }
        true
    }
    pub fn over(&self) -> bool {
        self.passes >= 2 || self.cells.iter().all(|&v| v != 0)
    }
    pub fn score(&self, side: u8) -> usize {
        self.cells.iter().filter(|&&v| v == side).count()
    }
}
impl Default for Othello {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::play::quick::*;
    #[test]
    fn memory_pairs_and_repeated_clicks() {
        let mut m = Memory::new((0..8).flat_map(|v| [v, v]).collect()).unwrap();
        assert_eq!(m.pick(0), Some(false));
        assert_eq!(m.pick(0), None);
        assert_eq!(m.pick(1), Some(true));
        assert!(m.shown[0] && m.shown[1]);
    }
    #[test]
    fn mines_first_click_safe_and_flood() {
        let mut m = Mines::new(8).unwrap();
        assert!(m.reveal(27, &(0..64).collect::<Vec<_>>()).is_some());
        assert!(!m.mines[27]);
        assert!(m.neighbors(27).iter().all(|&n| !m.mines[n]));
        assert_eq!(m.mines.iter().filter(|&&v| v).count(), 10);
    }
    #[test]
    fn othello_opening_and_flip() {
        let mut g = Othello::new();
        assert_eq!(g.legal(1), [19, 26, 37, 44]);
        assert!(g.play(19));
        assert_eq!(g.score(1), 4);
        assert_eq!(g.turn, 2);
        assert!(!g.play(19));
    }

    #[test]
    fn higher_lower_ties_and_wrong_guesses() {
        assert!(higher_correct(7, 7, true));
        assert!(higher_correct(7, 7, false));
        assert!(higher_correct(7, 8, true));
        assert!(!higher_correct(7, 8, false));
        assert!(higher_correct(7, 6, false));
    }
    #[test]
    fn anagrams_preserve_turkish_letters_and_typing_is_exact() {
        assert!(anagram_correct("İNCİ", "inci"));
        assert!(!anagram_correct("inci", "incı"));
        assert_eq!(typing_score("Sea shell", "Sea shell", 12), (true, 9));
        assert!(!typing_score("Sea shell", "sea shell", 12).0);
    }
    #[test]
    fn hot_cold_boundaries() {
        assert_eq!(temperature(50, 45), Temperature::Hot);
        assert_eq!(temperature(50, 35), Temperature::Warm);
        assert_eq!(temperature(50, 34), Temperature::Cold);
    }
}
