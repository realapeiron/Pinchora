use std::fmt::Write;

use pinchora_core::{i18n::Lang, t};

use super::boards::{self, COLORS, Marks, OUTLINE, SQUARE};
use crate::{Theme, svg::background};

/// 14×14 pieces: `x` outline, `#` body, `o` highlight, `w` white, `y` gold. A pawn is a pearl on a
/// base, a knight a horse with a fin for a mane, a bishop a mitre, a rook a striped lighthouse, a
/// queen a crowned octopus and a king a crab wearing a cross.
const PIECES: [[&str; 14]; 6] = [
    [
        "..............",
        "..............",
        ".....xxxx.....",
        "....x####x....",
        "...x#oo###x...",
        "...x#o####x...",
        "...x######x...",
        "....x####x....",
        ".....x##x.....",
        "....x####x....",
        "...x######x...",
        "..x########x..",
        "..xxxxxxxxxx..",
        "..............",
    ],
    [
        "......xx......",
        ".....x##xx....",
        "....x###x##x..",
        "...x#o###x##x.",
        "..x##w####x##x",
        ".x###x#####x#x",
        "x####x#####x#.",
        "xx###xxx####x.",
        ".xx#x..x####x.",
        "...x....x###x.",
        "....x..x####x.",
        "...x#####x##x.",
        "..x########x..",
        "..xxxxxxxxxx..",
    ],
    [
        "......yy......",
        "......yy......",
        ".....x##x.....",
        "....x#o##x....",
        "...x#o####x...",
        "...x##x####x..",
        "...x###x###x..",
        "...x####x##x..",
        "....x#####x...",
        ".....x###x....",
        "....x#####x...",
        "...x#######x..",
        "...xxxxxxxxx..",
        "..............",
    ],
    [
        "......xx......",
        ".....x##x.....",
        "....xx##xx....",
        "...x#yyyy#x...",
        "...xxxxxxxx...",
        "....x####x....",
        "....x#oo#x....",
        "...xxxxxxxx...",
        "...x######x...",
        "...x#oo###x...",
        "..xxxxxxxxxx..",
        "..x########x..",
        "..x########x..",
        "..xxxxxxxxxx..",
    ],
    [
        "....y.yy.y....",
        "....yyyyyy....",
        "...xxxxxxxx...",
        "..x########x..",
        "..x#oo#####x..",
        "..x#w##w###x..",
        "..x########x..",
        "...x#xxxx#x...",
        "..x##x..x##x..",
        ".x##x....x##x.",
        ".x#x.x##x.x#x.",
        "..xx.x##x.xx..",
        "......xx......",
        "..............",
    ],
    [
        "......yy......",
        "....yyyyyy....",
        "......yy......",
        "....yyyyyy....",
        "xx.xxxxxxxx.xx",
        "x#xx#o####xx#x",
        "x##x#w##w#x##x",
        ".x#x######x#x.",
        "..x########x..",
        "..x#xxxxxx#x..",
        ".x#x.x##x.x#x.",
        ".xx.x#xx#x.xx.",
        "....xx..xx....",
        "..............",
    ],
];

/// 580×730 chess board: both players, the 8×8 board with sea creatures for pieces, a status line.
///
/// Pieces are numbered 1 to 6 (pawn, knight, bishop, rook, queen, king) for the first player, coral,
/// and 7 to 12 for the second, sky blue; 0 is an empty square.
pub struct ChessCard<'a> {
    /// The language of the labels; the status line comes translated, and in capitals.
    pub lang: Lang,
    /// The 64 squares from a1 (index 0) along the rank to h8 (index 63).
    pub pieces: &'a [u8; 64],
    pub names: [&'a str; 2],
    /// Whose turn it is (0 or 1); `None` when the game is over.
    pub turn: Option<u8>,
    pub winner: Option<u8>,
    /// The squares the last move went from and to.
    pub last: Option<(u8, u8)>,
    /// The square of the piece the player picked up, and where it can go.
    pub selected: Option<u8>,
    pub targets: &'a [u8],
    /// The square of a king in check.
    pub check: Option<u8>,
    pub status: &'a str,
    /// Shells on the table; 0 hides the pot.
    pub pot: i64,
    pub theme: Theme,
}

impl ChessCard<'_> {
    pub const WIDTH: u32 = 580;
    pub const HEIGHT: u32 = 730;

    /// The marks for the square at `row` (from the top) and `col`.
    fn marks(&self, row: usize, col: usize) -> Marks {
        let sq = ((7 - row) * 8 + col) as u8;
        Marks {
            last: self.last.is_some_and(|(a, b)| a == sq || b == sq),
            selected: self.selected == Some(sq),
            target: self.targets.contains(&sq).then(|| self.pieces[usize::from(sq)] != 0),
            danger: self.check == Some(sq),
        }
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH as f32, Self::HEIGHT as f32);
        let mut s = String::with_capacity(64 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);

        // One drawing per kind of piece and colour, placed with <use>: a board has up to 32.
        s.push_str("<defs>");
        for (side, (body, light)) in COLORS.iter().enumerate() {
            for (kind, rows) in PIECES.iter().enumerate() {
                let _ = write!(s, r#"<g id="p{}">"#, kind + 1 + 6 * side);
                boards::bitmap(&mut s, rows, 0.0, 0.0, 4.0, &|c| piece_color(c, body, light));
                s.push_str("</g>");
            }
        }
        s.push_str("</defs>");

        for (side, name) in self.names.iter().enumerate() {
            let x = 24.0 + side as f32 * 270.0;
            let active = self.turn == Some(side as u8);
            let won = self.winner == Some(side as u8);
            let (note, color) = if won {
                (t!(self.lang, "board.card.winner"), &t.gold)
            } else if active {
                (t!(self.lang, "board.card.your-move"), &t.mint)
            } else if side == 0 {
                (t!(self.lang, "chess.card.coral-first"), &t.muted)
            } else {
                (t!(self.lang, "chess.card.sky"), &t.muted)
            };
            boards::player_panel(
                &mut s,
                t,
                (x, 20.0),
                (active, won),
                name,
                (&note, color),
                |out, ix, iy| {
                    let (body, light) = COLORS[side];
                    let _ = write!(out, r#"<g transform="translate({ix} {iy})">"#);
                    boards::bitmap(out, &PIECES[1], 0.0, 0.0, 3.0, &|c| piece_color(c, body, light));
                    out.push_str("</g>");
                },
            );
        }

        let marks = |row: usize, col: usize| self.marks(row, col);
        let (ox, oy) = boards::board(&mut s, t, (30.0, 112.0), &marks, false);
        let pad = (SQUARE - 14.0 * 4.0) / 2.0;
        for row in 0..8 {
            for col in 0..8 {
                let piece = self.pieces[(7 - row) * 8 + col];
                if piece != 0 {
                    let _ = write!(
                        s,
                        r##"<use href="#p{piece}" x="{}" y="{}"/>"##,
                        ox + col as f32 * SQUARE + pad,
                        oy + row as f32 * SQUARE + pad
                    );
                }
            }
        }
        boards::targets(&mut s, t, (ox, oy), &marks);
        boards::status_panel(&mut s, t, 650.0, self.status, self.pot);
        s.push_str("</svg>");
        s
    }
}

fn piece_color(c: char, body: &str, light: &str) -> Option<String> {
    match c {
        'x' => Some(OUTLINE.to_owned()),
        '#' => Some(body.to_owned()),
        'o' => Some(light.to_owned()),
        'w' => Some("#ffffff".to_owned()),
        'y' => Some("#ffd166".to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;

    /// The start position as the card wants it.
    fn start() -> [u8; 64] {
        let back = [4u8, 2, 3, 5, 6, 3, 2, 4];
        let mut board = [0u8; 64];
        for file in 0..8 {
            board[file] = back[file];
            board[8 + file] = 1;
            board[48 + file] = 7;
            board[56 + file] = back[file] + 6;
        }
        board
    }

    fn card<'a>(pieces: &'a [u8; 64], targets: &'a [u8]) -> ChessCard<'a> {
        ChessCard {
            lang: Lang::En,
            pieces,
            names: ["Marlowe", "Cap"],
            turn: Some(0),
            winner: None,
            last: Some((12, 28)),
            selected: Some(1),
            targets,
            check: None,
            status: "Coral to move",
            pot: 0,
            theme: Theme::reef(),
        }
    }

    #[test]
    fn draws_every_piece_once_and_the_marks() {
        let board = start();
        let svg = card(&board, &[16, 18]).to_svg();
        assert_eq!(svg.matches("<use ").count(), 32, "one drawing per piece");
        assert!(svg.contains("YOUR MOVE") && !svg.contains("WINNER!"));
        let won = ChessCard {
            turn: None,
            winner: Some(1),
            ..card(&board, &[])
        }
        .to_svg();
        assert!(won.contains("WINNER!") && !won.contains("YOUR MOVE"));
    }

    #[test]
    fn renders_to_a_png_of_the_documented_size() {
        let board = start();
        let png = Renderer::new()
            .chess_card(&card(&board, &[16, 18, 28]))
            .expect("renders");
        let image = image::load_from_memory(&png).expect("a png");
        assert_eq!((image.width(), image.height()), (ChessCard::WIDTH, ChessCard::HEIGHT));
    }
}
