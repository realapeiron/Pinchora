//! English draughts: forced captures, multiple jumps, kings.
//!
//! The board has 32 playing squares, numbered 0 to 31 from the top left, four to a row. Player 2 starts
//! on the top three rows and moves down; player 1 starts on the bottom three and moves up. Player 1
//! moves first.

/// Playing squares.
pub const SQUARES: usize = 32;

/// Plies without a capture or a man moving before the game is a draw (40 moves each).
pub const QUIET_LIMIT: u32 = 80;

/// The row of a playing square, 0 at the top.
pub fn row(sq: u8) -> u8 {
    sq / 4
}

/// The column of a playing square, 0 at the left; playing squares are the dark ones, where row and
/// column add up to an odd number.
pub fn col(sq: u8) -> u8 {
    (sq % 4) * 2 + u8::from(row(sq).is_multiple_of(2))
}

fn square(row: i8, col: i8) -> Option<u8> {
    ((0..8).contains(&row) && (0..8).contains(&col) && (row + col) % 2 == 1).then(|| (row * 4 + col / 2) as u8)
}

/// `b2`: the file letter and the rank, 1 at the bottom, as on a chess board.
pub fn name(sq: u8) -> String {
    format!("{}{}", char::from(b'a' + col(sq)), 8 - row(sq))
}

/// One legal turn: the squares a piece visits (two for a plain move, more for a chain of jumps) and the
/// squares whose pieces it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub path: Vec<u8>,
    pub captured: Vec<u8>,
}

impl Move {
    pub fn from(&self) -> u8 {
        self.path[0]
    }

    pub fn to(&self) -> u8 {
        *self.path.last().expect("a move has squares")
    }

    /// `b2-c3`, or `c3xe5xg7` for a chain of jumps.
    pub fn text(&self) -> String {
        let sep = if self.captured.is_empty() { "-" } else { "x" };
        self.path.iter().map(|&s| name(s)).collect::<Vec<_>>().join(sep)
    }

    /// The path as `9.14.19`, for a button's custom id.
    pub fn code(&self) -> String {
        self.path.iter().map(u8::to_string).collect::<Vec<_>>().join(".")
    }
}

/// What a draughts game came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// This player (1 or 2) won: the other has no piece or no move left.
    Won(u8),
    Draw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    cells: [u8; SQUARES],
    /// Whose turn it is, 1 or 2.
    pub turn: u8,
    /// Plies since a capture or a man's move.
    pub quiet: u32,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        let mut cells = [0; SQUARES];
        for (sq, cell) in cells.iter_mut().enumerate() {
            *cell = match sq {
                0..=11 => 2,
                20..=31 => 1,
                _ => 0,
            };
        }
        Self {
            cells,
            turn: 1,
            quiet: 0,
        }
    }

    /// Builds a position from 32 characters (`.` empty, `a` / `A` a man / king of player 1, `b` / `B`
    /// of player 2), the player to move and the quiet count.
    pub fn from_parts(cells: &str, turn: u8, quiet: u32) -> Option<Self> {
        if cells.len() != SQUARES || !(1..=2).contains(&turn) {
            return None;
        }
        let mut out = [0; SQUARES];
        for (cell, c) in out.iter_mut().zip(cells.chars()) {
            *cell = match c {
                '.' => 0,
                'a' => 1,
                'b' => 2,
                'A' => 3,
                'B' => 4,
                _ => return None,
            };
        }
        Some(Self {
            cells: out,
            turn,
            quiet,
        })
    }

    /// The 32 characters [`Game::from_parts`] reads.
    pub fn cells_text(&self) -> String {
        self.cells
            .iter()
            .map(|c| match c {
                1 => 'a',
                2 => 'b',
                3 => 'A',
                4 => 'B',
                _ => '.',
            })
            .collect()
    }

    pub fn cell(&self, sq: u8) -> u8 {
        self.cells[usize::from(sq)]
    }

    /// The owner of the piece on `sq` (1 or 2), if any.
    pub fn owner(&self, sq: u8) -> Option<u8> {
        match self.cell(sq) {
            0 => None,
            1 | 3 => Some(1),
            _ => Some(2),
        }
    }

    pub fn is_king(&self, sq: u8) -> bool {
        self.cell(sq) >= 3
    }

    /// How many pieces `player` has.
    pub fn count(&self, player: u8) -> usize {
        (0..SQUARES as u8).filter(|&sq| self.owner(sq) == Some(player)).count()
    }

    /// The diagonal steps a piece may take: kings go every way, a man only forward.
    fn directions(&self, sq: u8) -> Vec<(i8, i8)> {
        let forward: i8 = if self.owner(sq) == Some(1) { -1 } else { 1 };
        if self.is_king(sq) {
            vec![(-1, -1), (-1, 1), (1, -1), (1, 1)]
        } else {
            vec![(forward, -1), (forward, 1)]
        }
    }

    /// The row a man of `player` is crowned on.
    fn crown_row(player: u8) -> u8 {
        if player == 1 { 0 } else { 7 }
    }

    fn jumps(&self, sq: u8, start: u8, path: &mut Vec<u8>, taken: &mut Vec<u8>, out: &mut Vec<Move>) {
        let me = self.turn;
        let mut extended = false;
        for (dr, dc) in self.directions(start) {
            let (r, c) = (i8::try_from(row(sq)).unwrap_or(0), i8::try_from(col(sq)).unwrap_or(0));
            let (Some(over), Some(land)) = (square(r + dr, c + dc), square(r + 2 * dr, c + 2 * dc)) else {
                continue;
            };
            let enemy = self.owner(over).is_some_and(|o| o != me) && !taken.contains(&over);
            // The jumping piece has left its first square, so that square is free to land on.
            let free = self.cell(land) == 0 || land == start;
            if !enemy || !free {
                continue;
            }
            extended = true;
            path.push(land);
            taken.push(over);
            // A man that reaches the far row is crowned and its turn ends there.
            let crowned = !self.is_king(start) && row(land) == Self::crown_row(me);
            if crowned {
                out.push(Move {
                    path: path.clone(),
                    captured: taken.clone(),
                });
            } else {
                self.jumps(land, start, path, taken, out);
            }
            taken.pop();
            path.pop();
        }
        if !extended && path.len() > 1 {
            out.push(Move {
                path: path.clone(),
                captured: taken.clone(),
            });
        }
    }

    /// Every legal turn for the player to move. When any piece can jump, jumping is compulsory.
    pub fn legal_moves(&self) -> Vec<Move> {
        let me = self.turn;
        let mut captures = Vec::new();
        for sq in (0..SQUARES as u8).filter(|&sq| self.owner(sq) == Some(me)) {
            self.jumps(sq, sq, &mut vec![sq], &mut Vec::new(), &mut captures);
        }
        if !captures.is_empty() {
            return captures;
        }
        let mut moves = Vec::new();
        for sq in (0..SQUARES as u8).filter(|&sq| self.owner(sq) == Some(me)) {
            let (r, c) = (i8::try_from(row(sq)).unwrap_or(0), i8::try_from(col(sq)).unwrap_or(0));
            for (dr, dc) in self.directions(sq) {
                if let Some(to) = square(r + dr, c + dc).filter(|&to| self.cell(to) == 0) {
                    moves.push(Move {
                        path: vec![sq, to],
                        captured: Vec::new(),
                    });
                }
            }
        }
        moves
    }

    /// Plays a turn from [`Game::legal_moves`]; returns `None` (and changes nothing) for anything else.
    pub fn play(&mut self, mv: &Move) -> Option<()> {
        if !self.legal_moves().contains(mv) {
            return None;
        }
        let from = mv.from();
        let was_man = !self.is_king(from);
        let piece = self.cells[usize::from(from)];
        self.cells[usize::from(from)] = 0;
        for &sq in &mv.captured {
            self.cells[usize::from(sq)] = 0;
        }
        let crowned = was_man && row(mv.to()) == Self::crown_row(self.turn);
        // A man is 1 or 2, its king 3 or 4.
        self.cells[usize::from(mv.to())] = if crowned { self.turn + 2 } else { piece };
        self.quiet = if !mv.captured.is_empty() || was_man {
            0
        } else {
            self.quiet + 1
        };
        self.turn = 3 - self.turn;
        Some(())
    }

    /// Whether the game is over, and how.
    pub fn outcome(&self) -> Option<Outcome> {
        if self.legal_moves().is_empty() {
            return Some(Outcome::Won(3 - self.turn));
        }
        (self.quiet >= QUIET_LIMIT).then_some(Outcome::Draw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A position from a list of (square name, piece), player 1 to move.
    fn setup(pieces: &[(&str, char)], turn: u8) -> Game {
        let mut cells = ['.'; SQUARES];
        for (at, piece) in pieces {
            let sq = (0..SQUARES as u8).find(|&s| name(s) == *at).expect("a playing square");
            cells[usize::from(sq)] = *piece;
        }
        Game::from_parts(&cells.iter().collect::<String>(), turn, 0).expect("a position")
    }

    fn texts(game: &Game) -> Vec<String> {
        let mut moves: Vec<String> = game.legal_moves().iter().map(Move::text).collect();
        moves.sort();
        moves
    }

    fn play(game: &mut Game, text: &str) {
        let mv = game
            .legal_moves()
            .into_iter()
            .find(|m| m.text() == text)
            .unwrap_or_else(|| panic!("{text} is not legal; the moves are {:?}", texts(game)));
        game.play(&mv).expect("legal");
    }

    #[test]
    fn squares_are_named_like_a_chess_board_and_dark_squares_alternate() {
        assert_eq!(name(0), "b8");
        assert_eq!(name(31), "g1");
        assert_eq!(name(28), "a1");
        for sq in 0..SQUARES as u8 {
            assert_eq!((row(sq) + col(sq)) % 2, 1, "{}", name(sq));
            assert_eq!(
                square(i8::try_from(row(sq)).unwrap(), i8::try_from(col(sq)).unwrap()),
                Some(sq)
            );
        }
        assert_eq!(square(0, 0), None, "a light square");
        assert_eq!(square(8, 1), None);
    }

    #[test]
    fn the_start_position_has_seven_moves_and_twelve_pieces_each() {
        let game = Game::new();
        assert_eq!((game.count(1), game.count(2)), (12, 12));
        assert_eq!(
            texts(&game),
            ["a3-b4", "c3-b4", "c3-d4", "e3-d4", "e3-f4", "g3-f4", "g3-h4"]
        );
        assert_eq!(game.outcome(), None);
    }

    #[test]
    fn men_move_forward_only_and_kings_both_ways() {
        let man = setup(&[("d4", 'a'), ("h8", 'b')], 1);
        assert_eq!(texts(&man), ["d4-c5", "d4-e5"]);
        let king = setup(&[("d4", 'A'), ("h8", 'b')], 1);
        assert_eq!(texts(&king), ["d4-c3", "d4-c5", "d4-e3", "d4-e5"]);
        let other = setup(&[("c5", 'b'), ("a1", 'a')], 2);
        assert_eq!(texts(&other), ["c5-b4", "c5-d4"], "player 2 moves down the board");
    }

    #[test]
    fn a_jump_is_compulsory_and_takes_the_piece() {
        let mut game = setup(&[("d4", 'a'), ("e5", 'b'), ("b2", 'a'), ("h8", 'b')], 1);
        assert_eq!(
            texts(&game),
            ["d4xf6"],
            "the plain moves of the other man are not allowed"
        );
        play(&mut game, "d4xf6");
        assert_eq!(game.count(2), 1);
        assert_eq!(game.turn, 2);
    }

    #[test]
    fn jumps_chain_and_must_be_finished() {
        let mut game = setup(&[("a1", 'a'), ("b2", 'b'), ("d4", 'b'), ("h8", 'b'), ("f8", 'b')], 1);
        assert_eq!(texts(&game), ["a1xc3xe5"], "the chain over b2 and d4 is one turn");
        play(&mut game, "a1xc3xe5");
        assert_eq!(game.count(2), 2);
    }

    #[test]
    fn a_choice_of_captures_is_offered_whole() {
        let game = setup(&[("d4", 'a'), ("c5", 'b'), ("e5", 'b'), ("h8", 'b')], 1);
        assert_eq!(texts(&game), ["d4xb6", "d4xf6"]);
    }

    #[test]
    fn a_man_that_reaches_the_far_row_is_crowned_and_stops() {
        // After taking c7 the man stands on the last row; the jump over e7 would be a king's.
        let mut game = setup(&[("b6", 'a'), ("c7", 'b'), ("e7", 'b'), ("h2", 'b')], 1);
        assert_eq!(texts(&game), ["b6xd8"]);
        play(&mut game, "b6xd8");
        let crowned = (0..SQUARES as u8)
            .find(|&s| game.cell(s) == 3)
            .expect("a king of player 1");
        assert_eq!(name(crowned), "d8");
        // A plain step onto the last row crowns too.
        let mut step = setup(&[("a7", 'a'), ("h2", 'b')], 1);
        play(&mut step, "a7-b8");
        assert_eq!(step.cells_text().matches('A').count(), 1);
    }

    #[test]
    fn kings_jump_backwards_and_can_circle_back_to_where_they_started() {
        let game = setup(&[("d4", 'A'), ("c5", 'b'), ("e3", 'b'), ("h8", 'b')], 1);
        let moves = texts(&game);
        assert!(moves.contains(&"d4xb6".to_owned()), "{moves:?}");
        assert!(moves.contains(&"d4xf2".to_owned()), "{moves:?}");
        let ring = setup(
            &[
                ("c3", 'A'),
                ("d4", 'b'),
                ("d6", 'b'),
                ("b6", 'b'),
                ("b4", 'b'),
                ("h8", 'b'),
            ],
            1,
        );
        let longest = ring.legal_moves().iter().map(|m| m.captured.len()).max().unwrap_or(0);
        assert_eq!(
            longest, 4,
            "the king takes all four men and lands on its own first square"
        );
        assert!(
            ring.legal_moves().iter().all(|m| m.captured.len() == 4),
            "a chain must be finished"
        );
    }

    #[test]
    fn a_player_without_moves_or_pieces_loses() {
        assert_eq!(setup(&[("a1", 'a'), ("b2", 'b')], 2).outcome(), None);
        // The man on h8 cannot move: g7 is taken, and the jump would land on f6, which is taken too.
        let blocked = setup(&[("h8", 'b'), ("g7", 'a'), ("f6", 'a')], 2);
        assert_eq!(blocked.outcome(), Some(Outcome::Won(1)));
        assert_eq!(
            setup(&[("d4", 'a')], 2).outcome(),
            Some(Outcome::Won(1)),
            "no pieces left"
        );
    }

    #[test]
    fn forty_quiet_moves_each_is_a_draw() {
        let mut game = setup(&[("a1", 'A'), ("h8", 'B')], 1);
        game.quiet = QUIET_LIMIT - 1;
        assert_eq!(game.outcome(), None);
        play(&mut game, "a1-b2");
        assert_eq!(game.outcome(), Some(Outcome::Draw));
        // A man's move or a capture starts the count again.
        let mut fresh = setup(&[("a1", 'A'), ("h8", 'B'), ("c3", 'a')], 1);
        fresh.quiet = 50;
        play(&mut fresh, "c3-b4");
        assert_eq!(fresh.quiet, 0);
    }

    #[test]
    fn positions_round_trip_and_illegal_input_is_refused() {
        let game = Game::new();
        let again = Game::from_parts(&game.cells_text(), 1, 0).expect("round trip");
        assert_eq!(again, game);
        assert!(Game::from_parts("short", 1, 0).is_none());
        assert!(Game::from_parts(&"x".repeat(32), 1, 0).is_none());
        assert!(Game::from_parts(&".".repeat(32), 3, 0).is_none());
        let mut game = game;
        let bogus = Move {
            path: vec![20, 4],
            captured: Vec::new(),
        };
        assert!(game.play(&bogus).is_none());
        assert_eq!(game, Game::new(), "nothing changed");
    }

    #[test]
    fn a_whole_game_always_ends() {
        let mut game = Game::new();
        let mut plies = 0;
        while game.outcome().is_none() && plies < 1_000 {
            let moves = game.legal_moves();
            let mv = &moves[(plies * 7 + plies / 3) % moves.len()];
            game.play(mv).expect("legal");
            plies += 1;
        }
        assert!(game.outcome().is_some(), "ended after {plies} plies");
        assert!(game.count(1) + game.count(2) <= 24);
    }
}
