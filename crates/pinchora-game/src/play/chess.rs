//! Chess: legal moves (castling, en passant, promotion), check, mate and the draws. The board is a
//! plain array; a game is the list of its moves, so storing it means storing that list.
//!
//! Squares count from a1 = 0 along the rank: b1 = 1, h1 = 7, a2 = 8, h8 = 63.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

impl Color {
    pub fn other(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::White => 0,
            Self::Black => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl Kind {
    /// The letter in move text; pawns have none.
    pub fn letter(self) -> char {
        match self {
            Self::Pawn => 'P',
            Self::Knight => 'N',
            Self::Bishop => 'B',
            Self::Rook => 'R',
            Self::Queen => 'Q',
            Self::King => 'K',
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Pawn => "Pawn",
            Self::Knight => "Knight",
            Self::Bishop => "Bishop",
            Self::Rook => "Rook",
            Self::Queen => "Queen",
            Self::King => "King",
        }
    }

    fn from_letter(c: char) -> Option<Self> {
        match c.to_ascii_uppercase() {
            'P' => Some(Self::Pawn),
            'N' => Some(Self::Knight),
            'B' => Some(Self::Bishop),
            'R' => Some(Self::Rook),
            'Q' => Some(Self::Queen),
            'K' => Some(Self::King),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub kind: Kind,
}

/// A move: where from, where to and, for a pawn reaching the last rank, what it becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub from: u8,
    pub to: u8,
    pub promotion: Option<Kind>,
}

impl Move {
    /// `e2e4`, or `e7e8q` for a promotion.
    pub fn uci(self) -> String {
        let mut text = format!("{}{}", square_name(self.from), square_name(self.to));
        if let Some(kind) = self.promotion {
            text.push(kind.letter().to_ascii_lowercase());
        }
        text
    }

    pub fn from_uci(text: &str) -> Option<Self> {
        if !(4..=5).contains(&text.len()) || !text.is_ascii() {
            return None;
        }
        let promotion = match text.get(4..5) {
            None => None,
            Some(c) => Some(
                Kind::from_letter(c.chars().next()?)
                    .filter(|k| matches!(k, Kind::Knight | Kind::Bishop | Kind::Rook | Kind::Queen))?,
            ),
        };
        Some(Self {
            from: parse_square(&text[0..2])?,
            to: parse_square(&text[2..4])?,
            promotion,
        })
    }
}

/// `e4` for square 28.
pub fn square_name(sq: u8) -> String {
    format!("{}{}", char::from(b'a' + sq % 8), sq / 8 + 1)
}

pub fn parse_square(text: &str) -> Option<u8> {
    let bytes = text.as_bytes();
    if bytes.len() != 2 || !(b'a'..=b'h').contains(&bytes[0]) || !(b'1'..=b'8').contains(&bytes[1]) {
        return None;
    }
    Some((bytes[1] - b'1') * 8 + (bytes[0] - b'a'))
}

const KNIGHT_STEPS: [(i8, i8); 8] = [(1, 2), (2, 1), (2, -1), (1, -2), (-1, -2), (-2, -1), (-2, 1), (-1, 2)];
const KING_STEPS: [(i8, i8); 8] = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
const DIAGONALS: [(i8, i8); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];
const STRAIGHTS: [(i8, i8); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
const PROMOTIONS: [Kind; 4] = [Kind::Queen, Kind::Rook, Kind::Bishop, Kind::Knight];

const WHITE_KINGSIDE: u8 = 1;
const WHITE_QUEENSIDE: u8 = 2;
const BLACK_KINGSIDE: u8 = 4;
const BLACK_QUEENSIDE: u8 = 8;

fn step(sq: u8, df: i8, dr: i8) -> Option<u8> {
    let (file, rank) = ((sq % 8) as i8 + df, (sq / 8) as i8 + dr);
    ((0..8).contains(&file) && (0..8).contains(&rank)).then(|| (rank * 8 + file) as u8)
}

/// One position: the pieces, whose turn it is and what the rules remember (castling, en passant, the
/// fifty-move clock).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Position {
    board: [Option<Piece>; 64],
    pub turn: Color,
    castling: u8,
    /// The square a pawn that just moved two ranks passed over.
    ep: Option<u8>,
    /// Plies since a pawn move or a capture.
    pub halfmove: u32,
    pub fullmove: u32,
}

impl fmt::Debug for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_fen())
    }
}

pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

impl Position {
    pub fn start() -> Self {
        Self::from_fen(START_FEN).expect("the start position parses")
    }

    pub fn piece_at(&self, sq: u8) -> Option<Piece> {
        self.board[usize::from(sq)]
    }

    /// The en passant target, if the last move opened one.
    pub fn en_passant(&self) -> Option<u8> {
        self.ep
    }

    pub fn from_fen(fen: &str) -> Option<Self> {
        let mut fields = fen.split_whitespace();
        let placement = fields.next()?;
        let mut board = [None; 64];
        let ranks: Vec<&str> = placement.split('/').collect();
        if ranks.len() != 8 {
            return None;
        }
        for (i, rank) in ranks.iter().enumerate() {
            let mut file = 0usize;
            for c in rank.chars() {
                if let Some(skip) = c.to_digit(10) {
                    file += skip as usize;
                } else {
                    let kind = Kind::from_letter(c)?;
                    let color = if c.is_ascii_uppercase() {
                        Color::White
                    } else {
                        Color::Black
                    };
                    if file >= 8 {
                        return None;
                    }
                    board[(7 - i) * 8 + file] = Some(Piece { color, kind });
                    file += 1;
                }
            }
            if file != 8 {
                return None;
            }
        }
        let turn = match fields.next()? {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return None,
        };
        let mut castling = 0;
        for c in fields.next()?.chars() {
            castling |= match c {
                'K' => WHITE_KINGSIDE,
                'Q' => WHITE_QUEENSIDE,
                'k' => BLACK_KINGSIDE,
                'q' => BLACK_QUEENSIDE,
                '-' => 0,
                _ => return None,
            };
        }
        let ep = match fields.next()? {
            "-" => None,
            square => Some(parse_square(square)?),
        };
        let halfmove = fields.next().map_or(Some(0), |v| v.parse().ok())?;
        let fullmove = fields.next().map_or(Some(1), |v| v.parse().ok())?;
        Some(Self {
            board,
            turn,
            castling,
            ep,
            halfmove,
            fullmove,
        })
    }

    pub fn to_fen(&self) -> String {
        let mut text = String::new();
        for rank in (0..8).rev() {
            let mut empty = 0;
            for file in 0..8 {
                match self.board[rank * 8 + file] {
                    None => empty += 1,
                    Some(piece) => {
                        if empty > 0 {
                            text.push_str(&empty.to_string());
                            empty = 0;
                        }
                        let letter = piece.kind.letter();
                        text.push(if piece.color == Color::White {
                            letter
                        } else {
                            letter.to_ascii_lowercase()
                        });
                    }
                }
            }
            if empty > 0 {
                text.push_str(&empty.to_string());
            }
            if rank > 0 {
                text.push('/');
            }
        }
        let castling: String = [
            (WHITE_KINGSIDE, 'K'),
            (WHITE_QUEENSIDE, 'Q'),
            (BLACK_KINGSIDE, 'k'),
            (BLACK_QUEENSIDE, 'q'),
        ]
        .iter()
        .filter(|(bit, _)| self.castling & bit != 0)
        .map(|(_, c)| *c)
        .collect();
        format!(
            "{text} {} {} {} {} {}",
            if self.turn == Color::White { 'w' } else { 'b' },
            if castling.is_empty() { "-" } else { &castling },
            self.ep.map_or_else(|| "-".to_owned(), square_name),
            self.halfmove,
            self.fullmove
        )
    }

    /// Where `color`'s king stands.
    pub fn king_square(&self, color: Color) -> Option<u8> {
        (0..64u8).find(|&sq| {
            self.board[usize::from(sq)]
                == Some(Piece {
                    color,
                    kind: Kind::King,
                })
        })
    }

    /// Whether a piece of `by` could capture on `sq`.
    pub fn attacked(&self, sq: u8, by: Color) -> bool {
        let is = |at: Option<u8>, kind: Kind| {
            at.is_some_and(|s| self.board[usize::from(s)] == Some(Piece { color: by, kind }))
        };
        // A pawn attacks one rank ahead of itself, so it stands one rank behind the square.
        let behind = if by == Color::White { -1 } else { 1 };
        if is(step(sq, -1, behind), Kind::Pawn) || is(step(sq, 1, behind), Kind::Pawn) {
            return true;
        }
        if KNIGHT_STEPS.iter().any(|&(df, dr)| is(step(sq, df, dr), Kind::Knight)) {
            return true;
        }
        if KING_STEPS.iter().any(|&(df, dr)| is(step(sq, df, dr), Kind::King)) {
            return true;
        }
        for (dirs, a, b) in [
            (DIAGONALS, Kind::Bishop, Kind::Queen),
            (STRAIGHTS, Kind::Rook, Kind::Queen),
        ] {
            for (df, dr) in dirs {
                let mut at = step(sq, df, dr);
                while let Some(s) = at {
                    if let Some(piece) = self.board[usize::from(s)] {
                        if piece.color == by && (piece.kind == a || piece.kind == b) {
                            return true;
                        }
                        break;
                    }
                    at = step(s, df, dr);
                }
            }
        }
        false
    }

    pub fn in_check(&self) -> bool {
        self.king_square(self.turn)
            .is_some_and(|king| self.attacked(king, self.turn.other()))
    }

    /// Moves that obey the piece rules but may leave the own king in check.
    fn pseudo_moves(&self, out: &mut Vec<Move>) {
        let me = self.turn;
        let push = |out: &mut Vec<Move>, from: u8, to: u8| {
            out.push(Move {
                from,
                to,
                promotion: None,
            })
        };
        for from in 0..64u8 {
            let Some(piece) = self.board[usize::from(from)].filter(|p| p.color == me) else {
                continue;
            };
            match piece.kind {
                Kind::Pawn => {
                    let (dir, start, last) = if me == Color::White { (1, 1, 7) } else { (-1, 6, 0) };
                    let add = |out: &mut Vec<Move>, to: u8| {
                        if to / 8 == last {
                            for kind in PROMOTIONS {
                                out.push(Move {
                                    from,
                                    to,
                                    promotion: Some(kind),
                                });
                            }
                        } else {
                            push(out, from, to);
                        }
                    };
                    if let Some(one) = step(from, 0, dir).filter(|&s| self.board[usize::from(s)].is_none()) {
                        add(out, one);
                        if from / 8 == start
                            && let Some(two) = step(one, 0, dir).filter(|&s| self.board[usize::from(s)].is_none())
                        {
                            push(out, from, two);
                        }
                    }
                    for df in [-1, 1] {
                        let Some(to) = step(from, df, dir) else { continue };
                        let enemy = self.board[usize::from(to)].is_some_and(|p| p.color != me);
                        if enemy || self.ep == Some(to) {
                            add(out, to);
                        }
                    }
                }
                Kind::Knight => {
                    for (df, dr) in KNIGHT_STEPS {
                        if let Some(to) = step(from, df, dr)
                            && self.board[usize::from(to)].is_none_or(|p| p.color != me)
                        {
                            push(out, from, to);
                        }
                    }
                }
                Kind::King => {
                    for (df, dr) in KING_STEPS {
                        if let Some(to) = step(from, df, dr)
                            && self.board[usize::from(to)].is_none_or(|p| p.color != me)
                        {
                            push(out, from, to);
                        }
                    }
                    self.castling_moves(from, out);
                }
                Kind::Bishop | Kind::Rook | Kind::Queen => {
                    let diagonal = matches!(piece.kind, Kind::Bishop | Kind::Queen);
                    let straight = matches!(piece.kind, Kind::Rook | Kind::Queen);
                    let dirs = DIAGONALS
                        .iter()
                        .filter(|_| diagonal)
                        .chain(STRAIGHTS.iter().filter(|_| straight));
                    for &(df, dr) in dirs {
                        let mut at = step(from, df, dr);
                        while let Some(to) = at {
                            match self.board[usize::from(to)] {
                                None => push(out, from, to),
                                Some(other) => {
                                    if other.color != me {
                                        push(out, from, to);
                                    }
                                    break;
                                }
                            }
                            at = step(to, df, dr);
                        }
                    }
                }
            }
        }
    }

    fn castling_moves(&self, king: u8, out: &mut Vec<Move>) {
        let me = self.turn;
        let (home, kingside, queenside) = match me {
            Color::White => (4u8, WHITE_KINGSIDE, WHITE_QUEENSIDE),
            Color::Black => (60u8, BLACK_KINGSIDE, BLACK_QUEENSIDE),
        };
        if king != home || self.castling & (kingside | queenside) == 0 {
            return;
        }
        let enemy = me.other();
        if self.attacked(home, enemy) {
            return;
        }
        let rook = Some(Piece {
            color: me,
            kind: Kind::Rook,
        });
        let empty = |sq: u8| self.board[usize::from(sq)].is_none();
        if self.castling & kingside != 0
            && self.board[usize::from(home + 3)] == rook
            && empty(home + 1)
            && empty(home + 2)
            && !self.attacked(home + 1, enemy)
            && !self.attacked(home + 2, enemy)
        {
            out.push(Move {
                from: home,
                to: home + 2,
                promotion: None,
            });
        }
        if self.castling & queenside != 0
            && self.board[usize::from(home - 4)] == rook
            && empty(home - 1)
            && empty(home - 2)
            && empty(home - 3)
            && !self.attacked(home - 1, enemy)
            && !self.attacked(home - 2, enemy)
        {
            out.push(Move {
                from: home,
                to: home - 2,
                promotion: None,
            });
        }
    }

    /// The position after `mv`, which must be a legal (or at least pseudo-legal) move.
    pub fn make(&self, mv: Move) -> Self {
        let mut next = *self;
        let piece = next.board[usize::from(mv.from)].expect("a piece moves");
        let captured = next.board[usize::from(mv.to)];
        next.board[usize::from(mv.from)] = None;
        next.board[usize::from(mv.to)] = Some(match mv.promotion {
            Some(kind) => Piece { kind, ..piece },
            None => piece,
        });
        next.ep = None;
        if piece.kind == Kind::Pawn {
            // En passant: the pawn goes diagonally onto an empty square and takes the one beside it.
            if mv.from % 8 != mv.to % 8 && captured.is_none() {
                let taken = if piece.color == Color::White {
                    mv.to - 8
                } else {
                    mv.to + 8
                };
                next.board[usize::from(taken)] = None;
            }
            if mv.to.abs_diff(mv.from) == 16 {
                next.ep = Some((mv.from + mv.to) / 2);
            }
        }
        if piece.kind == Kind::King && mv.to.abs_diff(mv.from) == 2 {
            let (rook_from, rook_to) = if mv.to > mv.from {
                (mv.from + 3, mv.from + 1)
            } else {
                (mv.from - 4, mv.from - 1)
            };
            next.board[usize::from(rook_to)] = next.board[usize::from(rook_from)].take();
        }
        for sq in [mv.from, mv.to] {
            next.castling &= match sq {
                0 => !WHITE_QUEENSIDE,
                7 => !WHITE_KINGSIDE,
                4 => !(WHITE_KINGSIDE | WHITE_QUEENSIDE),
                56 => !BLACK_QUEENSIDE,
                63 => !BLACK_KINGSIDE,
                60 => !(BLACK_KINGSIDE | BLACK_QUEENSIDE),
                _ => 0xff,
            };
        }
        next.halfmove = if piece.kind == Kind::Pawn || captured.is_some() {
            0
        } else {
            next.halfmove + 1
        };
        if self.turn == Color::Black {
            next.fullmove += 1;
        }
        next.turn = self.turn.other();
        next
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        let mut moves = Vec::with_capacity(48);
        self.pseudo_moves(&mut moves);
        moves.retain(|&mv| {
            let after = self.make(mv);
            after
                .king_square(self.turn)
                .is_none_or(|king| !after.attacked(king, self.turn.other()))
        });
        moves
    }

    /// Counts the leaves of the move tree `depth` plies down: the standard check of a move generator.
    pub fn perft(&self, depth: u32) -> u64 {
        let moves = self.legal_moves();
        if depth <= 1 {
            return moves.len() as u64;
        }
        moves.iter().map(|&mv| self.make(mv).perft(depth - 1)).sum()
    }

    /// A number that is the same for positions with the same pieces, side to move, castling rights and
    /// en passant chance: the repetition rule's idea of "the same position".
    fn key(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |byte: u8| h = (h ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        for cell in &self.board {
            feed(match cell {
                None => 0,
                Some(p) => 1 + p.kind as u8 + 6 * p.color.index() as u8,
            });
        }
        feed(self.turn.index() as u8 + 20);
        feed(self.castling + 30);
        // The en passant square only counts when a pawn could really take.
        let ep = self.ep.filter(|&to| {
            let me = self.turn;
            let behind = if me == Color::White { -1 } else { 1 };
            [-1, 1].iter().any(|&df| {
                step(to, df, behind).is_some_and(|s| {
                    self.board[usize::from(s)]
                        == Some(Piece {
                            color: me,
                            kind: Kind::Pawn,
                        })
                })
            })
        });
        feed(ep.map_or(0, |s| s + 40));
        h
    }

    /// The move in standard notation (`Nf3`, `exd5`, `O-O`, `e8=Q+`), as played from this position.
    pub fn san(&self, mv: Move) -> String {
        let Some(piece) = self.piece_at(mv.from) else {
            return mv.uci();
        };
        let capture = self.piece_at(mv.to).is_some() || (piece.kind == Kind::Pawn && mv.from % 8 != mv.to % 8);
        let mut text = String::new();
        if piece.kind == Kind::King && mv.to.abs_diff(mv.from) == 2 {
            text.push_str(if mv.to > mv.from { "O-O" } else { "O-O-O" });
        } else if piece.kind == Kind::Pawn {
            if capture {
                text.push(char::from(b'a' + mv.from % 8));
                text.push('x');
            }
            text.push_str(&square_name(mv.to));
            if let Some(kind) = mv.promotion {
                text.push('=');
                text.push(kind.letter());
            }
        } else {
            text.push(piece.kind.letter());
            let rivals: Vec<Move> = self
                .legal_moves()
                .into_iter()
                .filter(|m| m.to == mv.to && m.from != mv.from && self.piece_at(m.from) == Some(piece))
                .collect();
            if !rivals.is_empty() {
                let same_file = rivals.iter().any(|m| m.from % 8 == mv.from % 8);
                let same_rank = rivals.iter().any(|m| m.from / 8 == mv.from / 8);
                if !same_file {
                    text.push(char::from(b'a' + mv.from % 8));
                } else if !same_rank {
                    text.push(char::from(b'1' + mv.from / 8));
                } else {
                    text.push_str(&square_name(mv.from));
                }
            }
            if capture {
                text.push('x');
            }
            text.push_str(&square_name(mv.to));
        }
        let after = self.make(mv);
        if after.in_check() {
            text.push(if after.legal_moves().is_empty() { '#' } else { '+' });
        }
        text
    }

    /// Whether neither side could ever deliver mate: bare kings, one minor piece, or bishops all on
    /// one colour of square.
    pub fn insufficient_material(&self) -> bool {
        let mut minors = 0;
        let mut bishop_squares = Vec::new();
        let mut other_minor = false;
        for sq in 0..64u8 {
            let Some(piece) = self.piece_at(sq) else { continue };
            match piece.kind {
                Kind::King => {}
                Kind::Pawn | Kind::Rook | Kind::Queen => return false,
                Kind::Knight => {
                    minors += 1;
                    other_minor = true;
                }
                Kind::Bishop => {
                    minors += 1;
                    bishop_squares.push((sq % 8 + sq / 8) % 2);
                }
            }
        }
        minors <= 1 || (!other_minor && bishop_squares.windows(2).all(|w| w[0] == w[1]))
    }

    /// How many of each piece (pawns, knights, bishops, rooks, queens) `color` has lost, counted
    /// against the starting set.
    pub fn missing(&self, color: Color) -> [u8; 5] {
        let mut have = [0u8; 5];
        for piece in self.board.iter().flatten().filter(|p| p.color == color) {
            if piece.kind != Kind::King {
                have[piece.kind as usize] += 1;
            }
        }
        let full: [u8; 5] = [8, 2, 2, 2, 1];
        std::array::from_fn(|i| full[i].saturating_sub(have[i]))
    }
}

/// How a finished game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Checkmate {
        winner: Color,
    },
    Stalemate,
    /// Fifty moves each without a pawn move or a capture.
    FiftyMoves,
    /// The same position for the third time.
    Repetition,
    Insufficient,
}

impl Outcome {
    pub fn winner(self) -> Option<Color> {
        match self {
            Self::Checkmate { winner } => Some(winner),
            _ => None,
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Checkmate { .. } => "checkmate",
            Self::Stalemate => "stalemate",
            Self::FiftyMoves => "the fifty-move rule",
            Self::Repetition => "threefold repetition",
            Self::Insufficient => "not enough pieces left to mate",
        }
    }
}

/// A game from the start position: its moves, its position and how often each position came up.
#[derive(Debug, Clone)]
pub struct Game {
    pub position: Position,
    moves: Vec<Move>,
    keys: Vec<u64>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn perft(fen: &str, depth: u32) -> u64 {
        Position::from_fen(fen).expect("a good FEN").perft(depth)
    }

    #[test]
    fn the_start_position_matches_the_known_perft_counts() {
        for (depth, nodes) in [(1, 20), (2, 400), (3, 8_902), (4, 197_281)] {
            assert_eq!(Position::start().perft(depth), nodes, "depth {depth}");
        }
    }

    #[test]
    fn kiwipete_matches_the_known_perft_counts() {
        let kiwipete = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
        for (depth, nodes) in [(1, 48), (2, 2_039), (3, 97_862)] {
            assert_eq!(perft(kiwipete, depth), nodes, "depth {depth}");
        }
    }

    #[test]
    fn the_other_standard_test_positions_match() {
        // En passant and pins, castling out of and through check, and promotions.
        let endgame = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
        for (depth, nodes) in [(1, 14), (2, 191), (3, 2_812), (4, 43_238)] {
            assert_eq!(perft(endgame, depth), nodes, "endgame depth {depth}");
        }
        let mirrored = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
        for (depth, nodes) in [(1, 6), (2, 264), (3, 9_467)] {
            assert_eq!(perft(mirrored, depth), nodes, "promotions depth {depth}");
        }
        let promotion = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
        for (depth, nodes) in [(1, 44), (2, 1_486), (3, 62_379)] {
            assert_eq!(perft(promotion, depth), nodes, "position 5 depth {depth}");
        }
    }

    #[test]
    fn fen_round_trips() {
        for fen in [
            START_FEN,
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq e6 0 2",
            "8/8/8/8/8/8/8/K6k b - - 12 40",
        ] {
            assert_eq!(Position::from_fen(fen).expect("parses").to_fen(), fen);
        }
        for bad in [
            "",
            "8/8/8 w - - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1",
            "9/8/8/8/8/8/8/8 w - - 0 1",
        ] {
            assert!(Position::from_fen(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn moves_read_and_write_as_text() {
        let mv = Move::from_uci("e7e8q").expect("a promotion");
        assert_eq!((mv.from, mv.to, mv.promotion), (52, 60, Some(Kind::Queen)));
        assert_eq!(mv.uci(), "e7e8q");
        assert_eq!(Move::from_uci("a1h8").map(Move::uci), Some("a1h8".to_owned()));
        for bad in ["", "e2", "e2e9", "i2e4", "e7e8k", "e7e8p", "e2e4x1", "e2é4"] {
            assert!(Move::from_uci(bad).is_none(), "{bad:?}");
        }
        assert_eq!(
            (parse_square("a1"), parse_square("h8"), parse_square("e4")),
            (Some(0), Some(63), Some(28))
        );
        assert_eq!(square_name(28), "e4");
    }

    fn play(game: &mut Game, moves: &str) {
        for word in moves.split_whitespace() {
            let mv = Move::from_uci(word).expect("a move");
            assert!(game.play(mv).is_some(), "{word} should be legal in {:?}", game.position);
        }
    }

    #[test]
    fn fools_mate_is_checkmate() {
        let mut game = Game::new();
        play(&mut game, "f2f3 e7e5 g2g4 d8h4");
        assert_eq!(game.outcome(), Some(Outcome::Checkmate { winner: Color::Black }));
        assert!(game.position.in_check());
        assert_eq!(game.recent_san(8), "1. f3 e5 2. g4 Qh4#");
    }

    #[test]
    fn stalemate_is_a_draw() {
        // Black to move with no legal move and not in check.
        let position = Position::from_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1").expect("fen");
        assert!(position.legal_moves().is_empty() && !position.in_check());
        let game = Game {
            position,
            moves: Vec::new(),
            keys: vec![position.key()],
        };
        assert_eq!(game.outcome(), Some(Outcome::Stalemate));
    }

    #[test]
    fn castling_needs_rights_empty_squares_and_safe_squares() {
        let moves = |fen: &str| -> Vec<String> {
            Position::from_fen(fen)
                .expect("fen")
                .legal_moves()
                .into_iter()
                .filter(|m| m.from == 4 || m.from == 60)
                .filter(|m| m.to.abs_diff(m.from) == 2)
                .map(Move::uci)
                .collect()
        };
        assert_eq!(moves("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1"), ["e1g1", "e1c1"]);
        assert_eq!(moves("r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1"), ["e8g8", "e8c8"]);
        // Rights gone, or a piece in the way, or the king attacked, or a square on the way attacked.
        assert!(moves("r3k2r/8/8/8/8/8/8/R3K2R w kq - 0 1").is_empty());
        assert!(moves("4k3/8/8/8/8/8/8/R2QK1NR w KQ - 0 1").is_empty());
        assert!(moves("4k3/8/8/8/8/8/4r3/R3K2R w KQ - 0 1").is_empty(), "in check");
        assert_eq!(
            moves("4k3/8/8/8/8/8/5r2/R3K2R w KQ - 0 1"),
            ["e1c1"],
            "f1 is attacked, d1 and c1 are not"
        );
        assert_eq!(
            moves("1r2k3/8/8/8/8/8/8/R3K2R w KQ - 0 1"),
            ["e1g1", "e1c1"],
            "b1 may be attacked"
        );
    }

    #[test]
    fn castling_moves_the_rook_and_loses_the_rights() {
        let mut game = Game::new();
        play(&mut game, "e2e4 e7e5 g1f3 b8c6 f1c4 g8f6 e1g1");
        assert_eq!(game.position.piece_at(6).map(|p| p.kind), Some(Kind::King));
        assert_eq!(game.position.piece_at(5).map(|p| p.kind), Some(Kind::Rook));
        assert!(game.position.piece_at(7).is_none());
        assert!(game.recent_san(2).ends_with("4. O-O"), "{}", game.recent_san(2));
        assert!(game.position.to_fen().contains(" kq "), "{:?}", game.position);
    }

    #[test]
    fn en_passant_must_be_taken_at_once_or_not_at_all() {
        let mut game = Game::new();
        play(&mut game, "e2e4 a7a6 e4e5 d7d5");
        assert_eq!(game.position.en_passant().map(square_name).as_deref(), Some("d6"));
        let mut after = game.clone();
        play(&mut after, "e5d6");
        assert!(
            after.position.piece_at(parse_square("d5").unwrap()).is_none(),
            "the pawn that passed is taken"
        );
        assert!(after.recent_san(2).ends_with("3. exd6"), "{}", after.recent_san(2));
        // One move later the chance is gone.
        play(&mut game, "h2h3 h7h6");
        assert!(game.play(Move::from_uci("e5d6").unwrap()).is_none());
    }

    #[test]
    fn promotion_offers_all_four_pieces_and_makes_the_choice() {
        let position = Position::from_fen("8/P7/8/8/8/8/8/k1K5 w - - 0 1").expect("fen");
        let moves: Vec<String> = position
            .legal_moves()
            .into_iter()
            .filter(|m| m.from == 48)
            .map(Move::uci)
            .collect();
        assert_eq!(moves, ["a7a8q", "a7a8r", "a7a8b", "a7a8n"]);
        let after = position.make(Move::from_uci("a7a8n").unwrap());
        assert_eq!(
            after.piece_at(56),
            Some(Piece {
                color: Color::White,
                kind: Kind::Knight
            })
        );
        assert_eq!(position.san(Move::from_uci("a7a8q").unwrap()), "a8=Q#");
    }

    #[test]
    fn a_pinned_piece_cannot_leave_the_line() {
        let position = Position::from_fen("4r1k1/8/8/8/8/8/4N3/4K3 w - - 0 1").expect("fen");
        assert!(
            position.legal_moves().iter().all(|m| m.from != 12),
            "the knight is pinned to the king"
        );
        assert!(!position.in_check());
    }

    #[test]
    fn san_disambiguates_and_marks_checks() {
        // The king is in the way, so only one rook can reach each square.
        let position = Position::from_fen("4k3/8/8/8/8/8/8/R3K2R w - - 0 1").expect("fen");
        assert_eq!(position.san(Move::from_uci("a1d1").unwrap()), "Rd1");
        assert_eq!(position.san(Move::from_uci("h1f1").unwrap()), "Rf1");
        let both = Position::from_fen("4k3/8/8/8/8/8/4K3/R6R w - - 0 1").expect("fen");
        assert_eq!(both.san(Move::from_uci("a1d1").unwrap()), "Rad1");
        assert_eq!(both.san(Move::from_uci("h1d1").unwrap()), "Rhd1");
        let check = Position::from_fen("4k3/8/8/8/8/8/8/R3K3 w - - 0 1").expect("fen");
        assert_eq!(check.san(Move::from_uci("a1a8").unwrap()), "Ra8+");
        let knights = Position::from_fen("4k3/8/8/8/8/2N3N1/8/4K3 w - - 0 1").expect("fen");
        assert_eq!(knights.san(Move::from_uci("c3e4").unwrap()), "Nce4");
        assert_eq!(knights.san(Move::from_uci("g3e4").unwrap()), "Nge4");
        assert_eq!(Position::start().san(Move::from_uci("g1f3").unwrap()), "Nf3");
    }

    #[test]
    fn threefold_repetition_ends_the_game() {
        let mut game = Game::new();
        play(&mut game, "g1f3 g8f6 f3g1 f6g8");
        assert_eq!(game.outcome(), None, "the start position came up twice");
        play(&mut game, "g1f3 g8f6 f3g1 f6g8");
        assert_eq!(game.outcome(), Some(Outcome::Repetition));
    }

    #[test]
    fn the_fifty_move_rule_counts_plies_without_pawns_or_captures() {
        let mut position = Position::from_fen("4k3/8/8/8/8/8/R7/4K3 w - - 99 80").expect("fen");
        let game = |p: Position| Game {
            position: p,
            moves: Vec::new(),
            keys: vec![p.key()],
        };
        assert_eq!(game(position).outcome(), None);
        position = position.make(Move::from_uci("a2a3").unwrap());
        assert_eq!(position.halfmove, 100);
        assert_eq!(game(position).outcome(), Some(Outcome::FiftyMoves));
        // A pawn move or a capture starts the count again.
        let reset = Position::from_fen("4k3/8/8/8/8/8/P7/4K3 w - - 99 80")
            .expect("fen")
            .make(Move::from_uci("a2a3").unwrap());
        assert_eq!(reset.halfmove, 0);
    }

    #[test]
    fn mate_wins_over_the_fifty_move_rule() {
        let position = Position::from_fen("7k/6pp/8/8/8/8/8/R3K3 w - - 99 80")
            .expect("fen")
            .make(Move::from_uci("a1a8").unwrap());
        let game = Game {
            position,
            moves: Vec::new(),
            keys: vec![position.key()],
        };
        assert_eq!(game.outcome(), Some(Outcome::Checkmate { winner: Color::White }));
    }

    #[test]
    fn insufficient_material_is_spotted() {
        let draw = |fen: &str| Position::from_fen(fen).expect("fen").insufficient_material();
        assert!(draw("4k3/8/8/8/8/8/8/4K3 w - - 0 1"));
        assert!(draw("4k3/8/8/8/8/8/8/3NK3 w - - 0 1"));
        assert!(draw("4k3/8/8/8/8/8/8/3BK3 w - - 0 1"));
        assert!(draw("2b1k3/8/8/8/8/8/8/3BK3 w - - 0 1"), "bishops on the same colour");
        assert!(
            !draw("3bk3/8/8/8/8/8/8/3BK3 w - - 0 1"),
            "bishops on different colours can mate"
        );
        assert!(!draw("4k3/8/8/8/8/8/8/2NNK3 w - - 0 1"));
        assert!(!draw("4k3/8/8/8/8/8/P7/4K3 w - - 0 1"));
        assert!(!draw("4k3/8/8/8/8/8/8/3RK3 w - - 0 1"));
        let game = Game {
            position: Position::from_fen("4k3/8/8/8/8/8/8/3NK3 w - - 0 1").unwrap(),
            moves: Vec::new(),
            keys: vec![0],
        };
        assert_eq!(game.outcome(), Some(Outcome::Insufficient));
    }

    #[test]
    fn the_shortest_known_stalemate_is_recognised() {
        // Sam Loyd's ten-move stalemate: black has a queen, two rooks and a bishop and no move.
        let moves = "e2e3 a7a5 d1h5 a8a6 h5a5 h7h5 h2h4 a6h6 a5c7 f7f6 c7d7 e8f7 d7b7 d8d3 b7b8 d3h7 b8c8 f7g6 c8e6";
        let game = Game::from_moves(moves).expect("every move is legal");
        assert_eq!(game.plies(), 19);
        assert!(!game.position.in_check());
        assert_eq!(game.outcome(), Some(Outcome::Stalemate));
        let before = Game::from_moves(&moves[..moves.len() - 5]).expect("one move less");
        assert_eq!(before.outcome(), None);
    }

    #[test]
    fn games_are_stored_as_their_moves() {
        let mut game = Game::new();
        play(&mut game, "e2e4 e7e5 g1f3 b8c6");
        let text = game.moves_text();
        assert_eq!(text, "e2e4 e7e5 g1f3 b8c6");
        let again = Game::from_moves(&text).expect("replays");
        assert_eq!(again.position, game.position);
        assert_eq!(again.plies(), 4);
        assert!(Game::from_moves("e2e5").is_none(), "an illegal move is refused");
        assert!(Game::from_moves("e2e4 e2e4").is_none());
        assert_eq!(Game::from_moves("").map(|g| g.plies()), Some(0));
    }

    #[test]
    fn lost_pieces_are_counted() {
        let position = Position::from_fen("rnbqkbnr/ppp2ppp/8/8/8/8/PPPP1PPP/RNB1KBNR w KQkq - 0 1").expect("fen");
        assert_eq!(position.missing(Color::White), [1, 0, 0, 0, 1]);
        assert_eq!(position.missing(Color::Black), [2, 0, 0, 0, 0]);
    }
}
