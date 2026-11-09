//! Connect four on a 7x6 board.

pub const COLS: usize = 7;
pub const ROWS: usize = 6;
const CELLS: usize = COLS * ROWS;

/// The board as a flat grid, row 0 on top. Each cell is 0 (empty) or the player 1 or 2 who owns it.
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

    /// Reads a board written by [`Board::encode`]: 42 characters of `.`, `1` and `2`.
    pub fn decode(text: &str) -> Option<Self> {
        if text.len() != CELLS {
            return None;
        }
        let mut cells = [0; CELLS];
        for (cell, c) in cells.iter_mut().zip(text.chars()) {
            *cell = match c {
                '.' => 0,
                '1' => 1,
                '2' => 2,
                _ => return None,
            };
        }
        Some(Self { cells })
    }

    pub fn encode(&self) -> String {
        self.cells
            .iter()
            .map(|c| match c {
                1 => '1',
                2 => '2',
                _ => '.',
            })
            .collect()
    }

    pub fn cells(&self) -> &[u8; CELLS] {
        &self.cells
    }

    pub fn get(&self, row: usize, col: usize) -> u8 {
        self.cells[row * COLS + col]
    }

    pub fn column_full(&self, col: usize) -> bool {
        col >= COLS || self.cells[col] != 0
    }

    pub fn is_full(&self) -> bool {
        self.cells[..COLS].iter().all(|&c| c != 0)
    }

    /// Drops a disc for `player` (1 or 2) into `col`; returns the cell index it landed in, or
    /// `None` when the column is full or does not exist.
    pub fn drop_disc(&mut self, col: usize, player: u8) -> Option<usize> {
        if self.column_full(col) || !(1..=2).contains(&player) {
            return None;
        }
        let row = (0..ROWS).rev().find(|&r| self.cells[r * COLS + col] == 0)?;
        let at = row * COLS + col;
        self.cells[at] = player;
        Some(at)
    }

    /// The player with four in a row and the four cells that show it, if anyone.
    pub fn winner(&self) -> Option<(u8, [usize; 4])> {
        const DIRECTIONS: [(isize, isize); 4] = [(0, 1), (1, 0), (1, 1), (1, -1)];
        for row in 0..ROWS as isize {
            for col in 0..COLS as isize {
                let owner = self.cells[row as usize * COLS + col as usize];
                if owner == 0 {
                    continue;
                }
                for (dr, dc) in DIRECTIONS {
                    let (end_r, end_c) = (row + 3 * dr, col + 3 * dc);
                    if !(0..ROWS as isize).contains(&end_r) || !(0..COLS as isize).contains(&end_c) {
                        continue;
                    }
                    let line = [0, 1, 2, 3].map(|k| ((row + k * dr) as usize) * COLS + (col + k * dc) as usize);
                    if line.iter().all(|&i| self.cells[i] == owner) {
                        return Some((owner, line));
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::play::connect4::*;

    fn play(moves: &[(usize, u8)]) -> Board {
        let mut board = Board::new();
        for &(col, player) in moves {
            board.drop_disc(col, player).expect("legal move");
        }
        board
    }

    #[test]
    fn discs_fall_to_the_bottom_and_stack() {
        let mut board = Board::new();
        assert_eq!(board.drop_disc(3, 1), Some(5 * COLS + 3));
        assert_eq!(board.drop_disc(3, 2), Some(4 * COLS + 3));
        assert_eq!(board.get(5, 3), 1);
        assert_eq!(board.get(4, 3), 2);
        assert_eq!(board.drop_disc(7, 1), None, "no eighth column");
        assert_eq!(board.drop_disc(0, 3), None, "only players 1 and 2");
    }

    #[test]
    fn a_full_column_refuses_discs() {
        let mut board = Board::new();
        for i in 0..ROWS {
            assert!(board.drop_disc(2, (i % 2) as u8 + 1).is_some());
        }
        assert!(board.column_full(2));
        assert_eq!(board.drop_disc(2, 1), None);
        assert!(!board.column_full(1));
    }

    #[test]
    fn finds_horizontal_vertical_and_both_diagonal_wins() {
        let horizontal = play(&[(0, 1), (1, 1), (2, 1), (3, 1)]);
        assert_eq!(horizontal.winner().map(|w| w.0), Some(1));

        let vertical = play(&[(4, 2), (4, 2), (4, 2), (4, 2)]);
        let (who, line) = vertical.winner().expect("four stacked");
        assert_eq!(who, 2);
        assert!(line.iter().all(|i| i % COLS == 4));

        // Rising diagonal: player 1 on (5,0) (4,1) (3,2) (2,3), with filler beneath.
        let rising = play(&[
            (0, 1),
            (1, 2),
            (1, 1),
            (2, 2),
            (2, 2),
            (2, 1),
            (3, 2),
            (3, 2),
            (3, 2),
            (3, 1),
        ]);
        let (who, line) = rising.winner().expect("rising diagonal");
        assert_eq!(who, 1);
        assert_eq!(line, [2 * COLS + 3, 3 * COLS + 2, 4 * COLS + 1, 5 * COLS]);

        // Falling diagonal: the mirror image.
        let falling = play(&[
            (6, 2),
            (5, 1),
            (5, 2),
            (4, 1),
            (4, 1),
            (4, 2),
            (3, 1),
            (3, 1),
            (3, 1),
            (3, 2),
        ]);
        assert_eq!(falling.winner().map(|w| w.0), Some(2));
    }

    #[test]
    fn three_in_a_row_and_broken_lines_do_not_win() {
        assert!(play(&[(0, 1), (1, 1), (2, 1)]).winner().is_none());
        assert!(play(&[(0, 1), (1, 1), (2, 2), (3, 1)]).winner().is_none());
        assert!(Board::new().winner().is_none());
    }

    #[test]
    fn a_line_does_not_wrap_around_the_edge() {
        // Player 1 holds the last two cells of row 4 and the first two of row 5: four consecutive
        // indexes, but not a line.
        let board = play(&[(5, 2), (5, 1), (6, 2), (6, 1), (0, 1), (1, 1)]);
        assert_eq!(board.get(4, 5), 1);
        assert_eq!(board.get(5, 1), 1);
        assert!(board.winner().is_none());
    }

    #[test]
    fn a_full_board_without_a_line_is_a_draw() {
        // Columns filled in blocks that never line up four.
        let pattern = [
            [1, 1, 2, 2, 1, 1, 2],
            [2, 2, 1, 1, 2, 2, 1],
            [1, 1, 2, 2, 1, 1, 2],
            [2, 2, 1, 1, 2, 2, 1],
            [1, 1, 2, 2, 1, 1, 2],
            [2, 2, 1, 1, 2, 2, 1],
        ];
        let mut board = Board::new();
        for row in (0..ROWS).rev() {
            for (col, owner) in pattern[row].iter().enumerate() {
                board.drop_disc(col, *owner).expect("room");
            }
        }
        assert!(board.is_full());
        assert!(board.winner().is_none(), "{:?}", board.winner());
    }

    #[test]
    fn boards_survive_encoding() {
        let board = play(&[(0, 1), (0, 2), (6, 1)]);
        let text = board.encode();
        assert_eq!(text.len(), 42);
        assert_eq!(Board::decode(&text), Some(board));
        assert_eq!(Board::decode("short"), None);
        assert_eq!(Board::decode(&"x".repeat(42)), None);
    }
}
