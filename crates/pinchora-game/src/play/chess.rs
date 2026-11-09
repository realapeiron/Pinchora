//! Chess: legal moves (castling, en passant, promotion), check, mate and the draws. The board is a
//! plain array; a game is the list of its moves, so storing it means storing that list.
//!
//! Squares count from a1 = 0 along the rank: b1 = 1, h1 = 7, a2 = 8, h8 = 63.

use std::fmt;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

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

    /// The piece's name in `lang` (`chess.piece-<kind>`).
    pub fn name(self, lang: Lang) -> String {
        match self {
            Self::Pawn => t!(lang, "chess.piece-pawn"),
            Self::Knight => t!(lang, "chess.piece-knight"),
            Self::Bishop => t!(lang, "chess.piece-bishop"),
            Self::Rook => t!(lang, "chess.piece-rook"),
            Self::Queen => t!(lang, "chess.piece-queen"),
            Self::King => t!(lang, "chess.piece-king"),
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
        // Only a legal capture changes the repetition identity. A pinned pawn cannot take.
        let ep = self.ep.filter(|&to| {
            self.legal_moves().iter().any(|mv| {
                mv.to == to && mv.from % 8 != to % 8 && self.piece_at(mv.from).is_some_and(|p| p.kind == Kind::Pawn)
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

    /// Why the game is over, as a phrase in `lang` (`chess.how-<outcome>`).
    pub fn describe(self, lang: Lang) -> String {
        match self {
            Self::Checkmate { .. } => t!(lang, "chess.how-checkmate"),
            Self::Stalemate => t!(lang, "chess.how-stalemate"),
            Self::FiftyMoves => t!(lang, "chess.how-fifty"),
            Self::Repetition => t!(lang, "chess.how-repetition"),
            Self::Insufficient => t!(lang, "chess.how-insufficient"),
        }
    }
}

mod game;
pub use game::*;

#[cfg(test)]
#[path = "chess/tests.rs"]
mod tests;
