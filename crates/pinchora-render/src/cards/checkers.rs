use std::fmt::Write;

use pinchora_core::{i18n::Lang, t};

use super::boards::{self, Marks, OUTLINE, SQUARE};
use crate::{Theme, svg::background};

/// The two sides' pearls (body, light, shade): a white pearl and a black one.
const PEARLS: [(&str, &str, &str); 2] = [("#fff1e8", "#ffffff", "#d3bccf"), ("#6a4a9a", "#c3b0ee", "#3a2350")];

const PEARL_SIZE: usize = 14;

/// A round pearl as `x` outline, `#` body, `o` shine and `s` shade, worked out from a circle so it is
/// the same on every side.
fn pearl() -> Vec<String> {
    let centre = (PEARL_SIZE as f32 - 1.0) / 2.0;
    let radius = PEARL_SIZE as f32 / 2.0;
    let inside = |r: isize, c: isize| {
        let size = PEARL_SIZE as isize;
        if r < 0 || c < 0 || r >= size || c >= size {
            return false;
        }
        let (dy, dx) = (r as f32 - centre, c as f32 - centre);
        dx * dx + dy * dy <= radius * radius - 0.5
    };
    (0..PEARL_SIZE as isize)
        .map(|r| {
            (0..PEARL_SIZE as isize)
                .map(|c| {
                    if !inside(r, c) {
                        return '.';
                    }
                    let edge = !(inside(r - 1, c) && inside(r + 1, c) && inside(r, c - 1) && inside(r, c + 1));
                    if edge {
                        return 'x';
                    }
                    let (dy, dx) = (r as f32 - centre, c as f32 - centre);
                    if dx + dy < -5.0 && dx < -1.0 && dy < -1.0 {
                        'o'
                    } else if dx + dy > 4.5 {
                        's'
                    } else {
                        '#'
                    }
                })
                .collect()
        })
        .collect()
}

/// A king's crown: `y` gold, `x` its shadow.
const CROWN: [&str; 3] = ["y..y..y", "yyyyyyy", "yyyyyyy"];

fn pearl_color(c: char, (body, light, shade): (&str, &str, &str)) -> Option<String> {
    match c {
        'x' => Some(OUTLINE.to_owned()),
        '#' => Some(body.to_owned()),
        'o' => Some(light.to_owned()),
        's' => Some(shade.to_owned()),
        _ => None,
    }
}

/// 580×730 draughts board on the 32 dark squares, with pearls for pieces.
pub struct CheckersCard<'a> {
    /// The language of the labels; the status line comes translated, and in capitals.
    pub lang: Lang,
    /// The 32 playing squares from the top left: 0 empty, 1 and 2 a man of the first or second
    /// player, 3 and 4 their kings.
    pub cells: &'a [u8; 32],
    pub names: [&'a str; 2],
    /// Whose turn it is (0 or 1); `None` when the game is over.
    pub turn: Option<u8>,
    pub winner: Option<u8>,
    /// The squares the last move visited.
    pub last: &'a [u8],
    /// The square of the piece the player picked up, and the squares it can end on.
    pub selected: Option<u8>,
    pub targets: &'a [u8],
    pub status: &'a str,
    /// Shells on the table; 0 hides the pot.
    pub pot: i64,
    pub theme: Theme,
}

/// The playing square at `row` and `col`, if that square is a dark one.
fn square_at(row: usize, col: usize) -> Option<u8> {
    ((row + col) % 2 == 1).then(|| (row * 4 + col / 2) as u8)
}

impl CheckersCard<'_> {
    pub const WIDTH: u32 = 580;
    pub const HEIGHT: u32 = 730;

    fn marks(&self, row: usize, col: usize) -> Marks {
        let Some(sq) = square_at(row, col) else {
            return Marks::default();
        };
        Marks {
            last: self.last.contains(&sq),
            selected: self.selected == Some(sq),
            target: self.targets.contains(&sq).then_some(false),
            danger: false,
        }
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH as f32, Self::HEIGHT as f32);
        let mut s = String::with_capacity(48 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);

        let rows = pearl();
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        s.push_str("<defs>");
        for (side, colors) in PEARLS.iter().enumerate() {
            let _ = write!(s, r#"<g id="m{}">"#, side + 1);
            boards::bitmap(&mut s, &rows, 0.0, 0.0, 4.0, &|c| pearl_color(c, *colors));
            s.push_str("</g>");
        }
        // The crown, with a shadow, centred on the pearl it sits on.
        s.push_str(r#"<g id="crown">"#);
        boards::bitmap(&mut s, &CROWN, 2.0, 2.0, 4.0, &|c| {
            (c == 'y').then(|| OUTLINE.to_owned())
        });
        boards::bitmap(&mut s, &CROWN, 0.0, 0.0, 4.0, &|c| {
            (c == 'y').then(|| "#ffd166".to_owned())
        });
        s.push_str("</g></defs>");

        let counts = [1, 2].map(|p| self.cells.iter().filter(|&&c| c == p || c == p + 2).count());
        for (side, count) in counts.iter().enumerate() {
            let x = 24.0 + side as f32 * 270.0;
            let active = self.turn == Some(side as u8);
            let won = self.winner == Some(side as u8);
            let (note, color) = if won {
                (t!(self.lang, "board.card.winner"), t.gold.as_str())
            } else if active {
                (t!(self.lang, "board.card.your-move"), t.mint.as_str())
            } else {
                (t!(self.lang, "checkers.card.left", count = count), t.muted.as_str())
            };
            boards::player_panel(
                &mut s,
                t,
                (x, 20.0),
                (active, won),
                self.names[side],
                (&note, color),
                |out, ix, iy| {
                    let _ = write!(
                        out,
                        r##"<g transform="translate({ix} {iy}) scale(0.75)"><use href="#m{}"/></g>"##,
                        side + 1
                    );
                },
            );
        }

        let marks = |row: usize, col: usize| self.marks(row, col);
        let (ox, oy) = boards::board(&mut s, t, (30.0, 112.0), &marks, true);
        let pad = (SQUARE - PEARL_SIZE as f32 * 4.0) / 2.0;
        for row in 0..8 {
            for col in 0..8 {
                let Some(sq) = square_at(row, col) else { continue };
                let piece = self.cells[usize::from(sq)];
                if piece == 0 {
                    continue;
                }
                let (x, y) = (ox + col as f32 * SQUARE + pad, oy + row as f32 * SQUARE + pad);
                let _ = write!(s, r##"<use href="#m{}" x="{x}" y="{y}"/>"##, (piece - 1) % 2 + 1);
                if piece >= 3 {
                    // 7×3 cells of crown at 4 px: 28×12 px, centred on the pearl.
                    let _ = write!(s, r##"<use href="#crown" x="{}" y="{}"/>"##, x + 14.0, y + 22.0);
                }
            }
        }
        boards::targets(&mut s, t, (ox, oy), &marks);
        boards::status_panel(&mut s, t, 650.0, self.status, self.pot);
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;

    fn start() -> [u8; 32] {
        std::array::from_fn(|sq| match sq {
            0..=11 => 2,
            20..=31 => 1,
            _ => 0,
        })
    }

    fn card<'a>(cells: &'a [u8; 32], last: &'a [u8], targets: &'a [u8]) -> CheckersCard<'a> {
        CheckersCard {
            lang: Lang::En,
            cells,
            names: ["Marlowe", "Cap"],
            turn: Some(0),
            winner: None,
            last,
            selected: Some(21),
            targets,
            status: "Pearl to move",
            pot: 0,
            theme: Theme::reef(),
        }
    }

    #[test]
    fn the_pearl_is_a_round_disc_with_an_outline() {
        let rows = pearl();
        assert_eq!(rows.len(), PEARL_SIZE);
        assert!(rows.iter().all(|r| r.chars().count() == PEARL_SIZE));
        assert!(rows[0].chars().any(|c| c != '.'));
        for needle in ['x', '#', 'o', 's'] {
            assert!(rows.iter().any(|r| r.contains(needle)), "no {needle}");
        }
        // Mirror image left to right, apart from the shine and the shade.
        for row in &rows {
            let flat: String = row.chars().map(|c| if c == '.' { '.' } else { 'x' }).collect();
            assert_eq!(flat.chars().rev().collect::<String>(), flat);
        }
    }

    #[test]
    fn draws_every_pearl_once_and_crowns_kings() {
        let mut cells = start();
        cells[20] = 3;
        let svg = card(&cells, &[], &[]).to_svg();
        assert_eq!(
            svg.matches("<use href=\"#m").count(),
            24 + 2,
            "24 pearls and the two in the panels"
        );
        assert_eq!(svg.matches("<use href=\"#crown\"").count(), 1);
        assert!(svg.contains("YOUR MOVE") && svg.contains("12 LEFT"));
    }

    #[test]
    fn renders_to_a_png_of_the_documented_size() {
        let cells = start();
        let png = Renderer::new()
            .checkers_card(&card(&cells, &[21, 17], &[17]))
            .expect("renders");
        let image = image::load_from_memory(&png).expect("a png");
        assert_eq!(
            (image.width(), image.height()),
            (CheckersCard::WIDTH, CheckersCard::HEIGHT)
        );
    }
}
