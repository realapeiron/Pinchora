//! Tide 2048: slide the sea creatures, merge two of a kind into the next one up.
//!
//! A tile is stored as its exponent: 1 is the "2" tile, 2 the "4", 11 the "2048". The board is a flat
//! grid, row 0 on top.

pub const SIZE: usize = 4;
pub const CELLS: usize = SIZE * SIZE;

/// The exponent of the tile that wins the game (2048).
pub const GOAL: u8 = 11;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Self::Up, Self::Left, Self::Right, Self::Down];

    pub fn key(self) -> char {
        match self {
            Self::Up => 'u',
            Self::Down => 'd',
            Self::Left => 'l',
            Self::Right => 'r',
        }
    }

    pub fn from_key(key: char) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.key() == key)
    }
}

/// The tile for an exponent: 1 → 2, 2 → 4, and so on.
pub fn value(exponent: u8) -> u32 {
    1u32 << exponent.min(31)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    cells: [u8; CELLS],
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    pub fn new() -> Self {
        Self { cells: [0; CELLS] }
    }

    /// Sixteen base-36 digits, one per cell: the exponent of the tile there, 0 for none.
    pub fn encode(&self) -> String {
        self.cells
            .iter()
            .map(|&e| char::from_digit(u32::from(e), 36).unwrap_or('0'))
            .collect()
    }

    pub fn decode(text: &str) -> Option<Self> {
        if text.len() != CELLS {
            return None;
        }
        let mut cells = [0; CELLS];
        for (cell, c) in cells.iter_mut().zip(text.chars()) {
            *cell = u8::try_from(c.to_digit(36)?).ok().filter(|&e| e <= 30)?;
        }
        Some(Self { cells })
    }

    pub fn cells(&self) -> &[u8; CELLS] {
        &self.cells
    }

    pub fn set(&mut self, cell: usize, exponent: u8) {
        self.cells[cell] = exponent;
    }

    /// The exponent of the biggest tile.
    pub fn best(&self) -> u8 {
        self.cells.iter().copied().max().unwrap_or(0)
    }

    pub fn empties(&self) -> usize {
        self.cells.iter().filter(|&&c| c == 0).count()
    }

    /// Puts a new tile on the `pick`-th empty cell (counting from the top left, wrapping): a 4 when
    /// `four`, else a 2. Returns where it went, or `None` when the board is full.
    pub fn spawn(&mut self, pick: usize, four: bool) -> Option<usize> {
        let empties = self.empties();
        if empties == 0 {
            return None;
        }
        let cell = (0..CELLS).filter(|&c| self.cells[c] == 0).nth(pick % empties)?;
        self.cells[cell] = if four { 2 } else { 1 };
        Some(cell)
    }

    /// The cells of each line in the order a slide in `dir` walks them, front first.
    fn lines(dir: Dir) -> [[usize; SIZE]; SIZE] {
        std::array::from_fn(|n| {
            std::array::from_fn(|k| match dir {
                Dir::Left => n * SIZE + k,
                Dir::Right => n * SIZE + SIZE - 1 - k,
                Dir::Up => k * SIZE + n,
                Dir::Down => (SIZE - 1 - k) * SIZE + n,
            })
        })
    }

    /// Slides every tile as far as it goes in `dir` and merges equal neighbours once each. Returns the
    /// new board and the points the merges scored, or `None` when nothing would move.
    pub fn slide(&self, dir: Dir) -> Option<(Board, u32)> {
        let mut next = Board::new();
        let mut gained = 0;
        for line in Self::lines(dir) {
            let tiles: Vec<u8> = line.iter().map(|&c| self.cells[c]).filter(|&e| e != 0).collect();
            let mut merged = Vec::with_capacity(SIZE);
            let mut i = 0;
            while i < tiles.len() {
                if i + 1 < tiles.len() && tiles[i] == tiles[i + 1] {
                    merged.push(tiles[i] + 1);
                    gained += value(tiles[i] + 1);
                    i += 2;
                } else {
                    merged.push(tiles[i]);
                    i += 1;
                }
            }
            for (k, &cell) in line.iter().enumerate() {
                next.cells[cell] = merged.get(k).copied().unwrap_or(0);
            }
        }
        (next != *self).then_some((next, gained))
    }

    /// Whether any slide changes the board.
    pub fn can_move(&self) -> bool {
        Dir::ALL.iter().any(|&d| self.slide(d).is_some())
    }

    pub fn moves(&self) -> Vec<Dir> {
        Dir::ALL.into_iter().filter(|&d| self.slide(d).is_some()).collect()
    }
}

/// Shells for a finished game, by the biggest tile reached.
pub fn reward(best: u8) -> i64 {
    match best {
        0..=7 => 0,
        8 => 5,
        9 => 10,
        10 => 20,
        11 => 40,
        _ => 60,
    }
}

#[cfg(test)]
mod tests {
    use crate::play::tide2048::*;

    /// A board from rows of tile values (2, 4, 8 ... or 0).
    fn board(rows: [[u32; 4]; 4]) -> Board {
        let mut b = Board::new();
        for (y, row) in rows.iter().enumerate() {
            for (x, &v) in row.iter().enumerate() {
                b.set(y * SIZE + x, if v == 0 { 0 } else { v.trailing_zeros() as u8 });
            }
        }
        b
    }

    fn rows(b: &Board) -> [[u32; 4]; 4] {
        std::array::from_fn(|y| {
            std::array::from_fn(|x| {
                if b.cells[y * SIZE + x] == 0 {
                    0
                } else {
                    value(b.cells[y * SIZE + x])
                }
            })
        })
    }

    #[test]
    fn tiles_slide_and_equal_neighbours_merge_once() {
        let b = board([[2, 2, 2, 2], [2, 2, 4, 0], [4, 2, 2, 0], [0, 0, 0, 2]]);
        let (left, gained) = b.slide(Dir::Left).expect("moves");
        assert_eq!(rows(&left), [[4, 4, 0, 0], [4, 4, 0, 0], [4, 4, 0, 0], [2, 0, 0, 0]]);
        assert_eq!(gained, 16, "points are the values of the new tiles: 4 + 4, 4 and 4");
    }

    #[test]
    fn points_are_the_values_of_the_new_tiles() {
        let b = board([[2, 2, 2, 2], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]);
        let (left, gained) = b.slide(Dir::Left).expect("moves");
        assert_eq!(rows(&left)[0], [4, 4, 0, 0]);
        assert_eq!(gained, 8);
        let b = board([[4, 4, 8, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]);
        let (right, gained) = b.slide(Dir::Right).expect("moves");
        assert_eq!(
            rows(&right)[0],
            [0, 0, 8, 8],
            "a merged tile does not merge again in the same move"
        );
        assert_eq!(gained, 8);
    }

    #[test]
    fn every_direction_walks_its_own_way() {
        let b = board([[0, 0, 0, 2], [0, 0, 0, 0], [0, 0, 0, 0], [4, 0, 0, 0]]);
        assert_eq!(
            rows(&b.slide(Dir::Left).unwrap().0),
            [[2, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [4, 0, 0, 0]]
        );
        assert_eq!(
            rows(&b.slide(Dir::Right).unwrap().0),
            [[0, 0, 0, 2], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 4]]
        );
        assert_eq!(
            rows(&b.slide(Dir::Up).unwrap().0),
            [[4, 0, 0, 2], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]
        );
        assert_eq!(
            rows(&b.slide(Dir::Down).unwrap().0),
            [[0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [4, 0, 0, 2]]
        );
    }

    #[test]
    fn a_slide_that_changes_nothing_is_refused() {
        let b = board([[2, 4, 2, 4], [4, 2, 4, 2], [2, 4, 2, 4], [4, 2, 4, 2]]);
        assert!(Dir::ALL.iter().all(|&d| b.slide(d).is_none()));
        assert!(!b.can_move());
        let packed = board([[2, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]);
        assert!(packed.slide(Dir::Left).is_none() && packed.slide(Dir::Up).is_none());
        assert_eq!(packed.moves(), [Dir::Right, Dir::Down]);
    }

    #[test]
    fn a_full_board_with_a_pair_can_still_move() {
        let b = board([[2, 2, 4, 8], [4, 8, 2, 4], [2, 4, 8, 2], [4, 2, 4, 8]]);
        assert!(b.can_move());
        assert_eq!(b.moves(), [Dir::Left, Dir::Right]);
    }

    #[test]
    fn new_tiles_land_on_empty_cells_only() {
        let mut b = board([[2, 4, 2, 4], [4, 2, 4, 2], [2, 4, 2, 4], [4, 2, 0, 0]]);
        assert_eq!(b.empties(), 2);
        assert_eq!(b.spawn(0, false), Some(14));
        assert_eq!(b.cells[14], 1);
        assert_eq!(b.spawn(5, true), Some(15), "picks wrap around the empty cells");
        assert_eq!(b.cells[15], 2);
        assert_eq!(b.spawn(0, false), None, "a full board takes nothing");
    }

    #[test]
    fn the_board_round_trips_as_text() {
        let b = board([
            [2, 4, 8, 16],
            [32, 64, 128, 256],
            [512, 1024, 2048, 4096],
            [8192, 0, 0, 2],
        ]);
        let text = b.encode();
        assert_eq!(text, "123456789abcd001");
        assert_eq!(Board::decode(&text), Some(b.clone()));
        assert_eq!(b.best(), 13);
        assert!(Board::decode("short").is_none());
        assert!(Board::decode(&"!".repeat(CELLS)).is_none());
        assert!(
            Board::decode(&"z".repeat(CELLS)).is_none(),
            "an exponent no game reaches"
        );
    }

    #[test]
    fn rewards_grow_with_the_biggest_tile() {
        assert_eq!(reward(7), 0);
        assert_eq!([8, 9, 10, 11, 12].map(reward), [5, 10, 20, 40, 60]);
        assert_eq!(value(1), 2);
        assert_eq!(value(GOAL), 2048);
        assert_eq!(Dir::from_key('l'), Some(Dir::Left));
        assert_eq!(Dir::from_key('x'), None);
    }
}
