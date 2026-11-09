use std::fmt::Write;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::text;
use crate::{
    Theme,
    svg::{SHELL, background, escape, mix, notched_path, panel, sprite, truncate},
};

/// The two disc colours: coral for player 1, sky blue for player 2.
const DISC_COLORS: [(&str, &str); 2] = [("#ff6b4a", "#ffb9a8"), ("#5bc0eb", "#bfe9fa")];

/// An 11×11 pixel disc.
const DISC: &[&str] = &[
    "...#####...",
    "..o######..",
    ".oo#######.",
    "#o#########",
    "###########",
    "###########",
    "###########",
    "###########",
    ".#########.",
    "..#######..",
    "...#####...",
];

/// 580×670 connect-four board: two player panels, column numbers over the 7×6 grid, a status line.
pub struct Connect4Card<'a> {
    /// The language of the labels; the status line comes translated, and in capitals.
    pub lang: Lang,
    /// The 42 cells, row 0 on top: 0 empty, 1 or 2 for a player's disc.
    pub cells: &'a [u8],
    pub names: [&'a str; 2],
    /// Whose turn it is (1 or 2); `None` when the game is over.
    pub turn: Option<u8>,
    pub winner: Option<u8>,
    /// Cell indexes of the winning four, drawn on a gold tile.
    pub win_cells: &'a [usize],
    /// The disc dropped last, marked with a small light square.
    pub last: Option<usize>,
    pub status: &'a str,
    /// Shells on the table; 0 hides the pot.
    pub pot: i64,
    pub theme: Theme,
}

impl Connect4Card<'_> {
    pub const COLS: usize = 7;
    pub const ROWS: usize = 6;
    const CELL: f32 = 68.0;
    const SCALE: f32 = 5.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (580.0, 670.0);
        let mut s = String::with_capacity(48 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);

        // One drawing per kind of cell, placed with <use>: the board has 42 of them.
        s.push_str("<defs>");
        for (id, (color, light)) in [
            ("d0", ("#0b1029", "#0b1029")),
            ("d1", DISC_COLORS[0]),
            ("d2", DISC_COLORS[1]),
        ] {
            let _ = write!(s, r#"<g id="{id}">"#);
            sprite(&mut s, DISC, 0.0, 0.0, Self::SCALE, color, light);
            s.push_str("</g>");
        }
        s.push_str("</defs>");

        // Player panels.
        for (player, name) in self.names.iter().enumerate() {
            let x = 24.0 + player as f32 * 270.0;
            let number = player as u8 + 1;
            let active = self.turn == Some(number);
            let won = self.winner == Some(number);
            let edge = if active || won { &t.gold } else { &t.panel_edge };
            let fill = if active || won {
                mix(&t.panel, &t.gold, 0.12)
            } else {
                t.panel.clone()
            };
            panel(&mut s, x, 24.0, 262.0, 76.0, 4.0, &fill, edge);
            let (color, light) = DISC_COLORS[player];
            sprite(&mut s, DISC, x + 16.0, 38.0, 4.0, color, light);
            text(
                &mut s,
                x + 72.0,
                56.0,
                20.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate(name, 13)),
            );
            let (note, note_color) = if won {
                (t!(self.lang, "board.card.winner"), &t.gold)
            } else if active {
                (t!(self.lang, "board.card.your-move"), &t.mint)
            } else {
                (
                    t!(
                        self.lang,
                        if player == 0 {
                            "connect4.card.coral"
                        } else {
                            "connect4.card.sky"
                        }
                    ),
                    &t.muted,
                )
            };
            text(&mut s, x + 72.0, 84.0, 14.0, PIXEL, false, note_color, "start", &note);
        }

        // Column numbers match the buttons under the message.
        let (bx, by) = (36.0, 134.0);
        for col in 0..Self::COLS {
            let cx = bx + 16.0 + col as f32 * Self::CELL + Self::CELL / 2.0;
            text(
                &mut s,
                cx,
                126.0,
                16.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &(col + 1).to_string(),
            );
        }

        // The board.
        let board_w = Self::COLS as f32 * Self::CELL + 32.0;
        let board_h = Self::ROWS as f32 * Self::CELL + 32.0;
        panel(&mut s, bx, by, board_w, board_h, 6.0, "#1a2a66", &t.panel_edge);
        let disc_px = 11.0 * Self::SCALE;
        let pad = (Self::CELL - disc_px) / 2.0;
        for row in 0..Self::ROWS {
            for col in 0..Self::COLS {
                let i = row * Self::COLS + col;
                let (x, y) = (bx + 16.0 + col as f32 * Self::CELL, by + 16.0 + row as f32 * Self::CELL);
                if self.win_cells.contains(&i) {
                    let _ = write!(
                        s,
                        r#"<path d="{}" fill="{}"/>"#,
                        notched_path(x + 2.0, y + 2.0, Self::CELL - 4.0, Self::CELL - 4.0, 4.0),
                        t.gold
                    );
                }
                let owner = self.cells.get(i).copied().unwrap_or(0).min(2);
                let _ = write!(s, r##"<use href="#d{owner}" x="{}" y="{}"/>"##, x + pad, y + pad);
                if self.last == Some(i) && owner != 0 {
                    let _ = write!(
                        s,
                        r##"<rect x="{}" y="{}" width="10" height="10" fill="#fff7d6" stroke="#0b1029" stroke-width="2"/>"##,
                        x + Self::CELL / 2.0 - 5.0,
                        y + Self::CELL / 2.0 - 5.0
                    );
                }
            }
        }

        // Status line, with the pot when shells are on the table.
        panel(&mut s, 24.0, 590.0, 532.0, 56.0, 4.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            44.0,
            625.0,
            17.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &escape(&truncate(
                &self.lang.upper(self.status),
                if self.pot > 0 { 26 } else { 40 },
            )),
        );
        if self.pot > 0 {
            sprite(&mut s, SHELL, 462.0, 606.0, 3.0, &t.gold, "#ffffff");
            text(
                &mut s,
                536.0,
                625.0,
                17.0,
                PIXEL,
                false,
                &t.gold,
                "end",
                &crate::svg::compact_in(self.lang, self.pot),
            );
        }
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use crate::Renderer;
    use crate::cards::connect4::*;

    fn card<'a>(cells: &'a [u8], win: &'a [usize], pot: i64) -> Connect4Card<'a> {
        Connect4Card {
            lang: Lang::En,
            cells,
            names: ["A", "B"],
            turn: Some(2),
            winner: None,
            win_cells: win,
            last: Some(41),
            status: "B to move",
            pot,
            theme: Theme::reef(),
        }
    }

    #[test]
    fn draws_every_cell_once_and_marks_the_winning_four() {
        let mut cells = [0u8; 42];
        cells[41] = 1;
        let svg = card(&cells, &[], 0).to_svg();
        assert_eq!(svg.matches("<use ").count(), 42, "one drawing per cell");
        assert!(svg.contains("YOUR MOVE") && !svg.contains("WINNER!"));
        let won = Connect4Card {
            turn: None,
            winner: Some(1),
            ..card(&cells, &[41, 34, 27, 20], 0)
        }
        .to_svg();
        assert!(won.contains("WINNER!") && !won.contains("YOUR MOVE"));
        assert!(
            won.matches(r##"fill="#ffd166""##).count() > 4,
            "four gold tiles and the winner's frame"
        );
    }

    #[test]
    fn the_pot_shows_only_when_shells_are_on_the_table() {
        let cells = [0u8; 42];
        assert!(!card(&cells, &[], 0).to_svg().contains(">400<"));
        assert!(card(&cells, &[], 400).to_svg().contains(">400<"));
    }

    #[test]
    fn renders_to_a_png_of_the_documented_size() {
        let cells = [1u8; 42];
        let png = Renderer::new().connect4_card(&card(&cells, &[], 50)).expect("renders");
        let image = image::load_from_memory(&png).expect("a png");
        assert_eq!((image.width(), image.height()), (580, 670));
    }
}
