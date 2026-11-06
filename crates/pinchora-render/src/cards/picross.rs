use std::fmt::Write;

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{background, escape, mix, panel, truncate},
};

const CELL: f32 = 34.0;
const SIDE: f32 = CELL * 10.0;
/// The room the row clues take on the left and the column clues take on top.
const CLUE_W: f32 = 112.0;
const CLUE_H: f32 = 112.0;

/// A finished picture shown instead of the marks: its colour and, per cell, 0 empty, 1 lit, 2 main or 3 shaded.
pub struct Reveal<'a> {
    pub name: &'a str,
    /// `#rrggbb`.
    pub color: &'a str,
    pub shades: &'a [u8; 100],
}

/// 540×610 pixel picross: the clues on two sides of a 10×10 grid, the player's marks in it, or, once the
/// puzzle is solved, the picture it was hiding.
pub struct PicrossCard<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    /// The player's marks.
    pub grid: &'a [bool; 100],
    pub rows: &'a [Vec<u8>],
    pub cols: &'a [Vec<u8>],
    /// Lines whose marks already match their clue; their clues dim.
    pub row_done: [bool; 10],
    pub col_done: [bool; 10],
    /// Marked cells that the picture leaves empty, crossed out in red.
    pub mistakes: &'a [usize],
    pub footer: &'a str,
    pub reveal: Option<Reveal<'a>>,
    pub theme: Theme,
}

impl PicrossCard<'_> {
    pub const WIDTH: u32 = 540;
    pub const HEIGHT: u32 = 610;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH as f32, Self::HEIGHT as f32);
        let mut s = String::with_capacity(48 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        panel(&mut s, 24.0, 20.0, w - 48.0, 52.0, 4.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            42.0,
            54.0,
            20.0,
            PIXEL,
            false,
            &t.gold,
            "start",
            &escape(&truncate(&self.title.to_uppercase(), 26)),
        );
        text(
            &mut s,
            w - 42.0,
            52.0,
            12.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &escape(&truncate(&self.subtitle.to_uppercase(), 16)),
        );

        match &self.reveal {
            Some(reveal) => self.picture(&mut s, reveal),
            None => self.puzzle(&mut s),
        }
        text(
            &mut s,
            w / 2.0,
            h - 22.0,
            13.0,
            PIXEL,
            false,
            &t.muted,
            "middle",
            &escape(&truncate(&self.footer.to_uppercase(), 48)),
        );
        s.push_str("</svg>");
        s
    }

    fn puzzle(&self, s: &mut String) {
        let t = &self.theme;
        let (gx, gy) = (24.0 + CLUE_W, 76.0 + CLUE_H);
        // Clue bands.
        panel(s, 24.0, gy, CLUE_W - 6.0, SIDE, 4.0, &t.panel, &t.panel_edge);
        panel(s, gx, 76.0, SIDE, CLUE_H - 6.0, 4.0, &t.panel, &t.panel_edge);
        let dim = mix(&t.muted, &t.panel, 0.45);
        for (y, clue) in self.rows.iter().enumerate().take(10) {
            let color = if self.row_done[y] { &dim } else { &t.ink };
            for (k, n) in clue.iter().rev().enumerate() {
                text(
                    s,
                    24.0 + CLUE_W - 18.0 - k as f32 * 19.0,
                    gy + y as f32 * CELL + CELL / 2.0 + 5.0,
                    14.0,
                    PIXEL,
                    false,
                    color,
                    "middle",
                    &n.to_string(),
                );
            }
        }
        for (x, clue) in self.cols.iter().enumerate().take(10) {
            let color = if self.col_done[x] { &dim } else { &t.ink };
            for (k, n) in clue.iter().rev().enumerate() {
                text(
                    s,
                    gx + x as f32 * CELL + CELL / 2.0,
                    76.0 + CLUE_H - 16.0 - k as f32 * 19.0,
                    14.0,
                    PIXEL,
                    false,
                    color,
                    "middle",
                    &n.to_string(),
                );
            }
        }
        // The grid: cells in blocks of five tinted alternately.
        panel(
            s,
            gx - 4.0,
            gy - 4.0,
            SIDE + 8.0,
            SIDE + 8.0,
            4.0,
            "#0b1029",
            &t.panel_edge,
        );
        for y in 0..10usize {
            for x in 0..10usize {
                let cell = y * 10 + x;
                let (cx, cy) = (gx + x as f32 * CELL, gy + y as f32 * CELL);
                let tint = if (x / 5 + y / 5) % 2 == 0 { "#1b2757" } else { "#222f66" };
                let _ = write!(
                    s,
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{tint}"/>"#,
                    cx + 1.0,
                    cy + 1.0,
                    CELL - 2.0,
                    CELL - 2.0
                );
                if self.grid[cell] {
                    let mistake = self.mistakes.contains(&cell);
                    let body = if mistake { "#ef476f" } else { t.mint.as_str() };
                    let _ = write!(
                        s,
                        r##"<rect x="{}" y="{}" width="{}" height="{}" fill="{body}"/><rect x="{}" y="{}" width="{}" height="4" fill="#ffffff" fill-opacity="0.35"/><rect x="{}" y="{}" width="{}" height="4" fill="#000000" fill-opacity="0.22"/>"##,
                        cx + 3.0,
                        cy + 3.0,
                        CELL - 6.0,
                        CELL - 6.0,
                        cx + 3.0,
                        cy + 3.0,
                        CELL - 6.0,
                        cx + 3.0,
                        cy + CELL - 7.0,
                        CELL - 6.0
                    );
                    if mistake {
                        let _ = write!(
                            s,
                            r##"<path d="M{} {} L{} {} M{} {} L{} {}" stroke="#fff1e8" stroke-width="3"/>"##,
                            cx + 9.0,
                            cy + 9.0,
                            cx + CELL - 9.0,
                            cy + CELL - 9.0,
                            cx + CELL - 9.0,
                            cy + 9.0,
                            cx + 9.0,
                            cy + CELL - 9.0
                        );
                    }
                }
            }
        }
        // Row numbers beside the grid and column letters under it, the way a cell is typed.
        for i in 0..10 {
            text(
                s,
                gx + SIDE + 18.0,
                gy + i as f32 * CELL + CELL / 2.0 + 4.0,
                11.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &(i + 1).to_string(),
            );
            text(
                s,
                gx + i as f32 * CELL + CELL / 2.0,
                gy + SIDE + 22.0,
                11.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &char::from(b'A' + i as u8).to_string(),
            );
        }
    }

    fn picture(&self, s: &mut String, reveal: &Reveal<'_>) {
        let t = &self.theme;
        let (cell, x0, y0) = (42.0, 60.0, 112.0);
        panel(
            s,
            x0 - 12.0,
            y0 - 12.0,
            cell * 10.0 + 24.0,
            cell * 10.0 + 24.0,
            6.0,
            "#10204a",
            &t.panel_edge,
        );
        let shades = [
            mix(reveal.color, "#ffffff", 0.35),
            reveal.color.to_owned(),
            mix(reveal.color, "#0b1029", 0.45),
        ];
        for y in 0..10usize {
            for x in 0..10usize {
                let shade = reveal.shades[y * 10 + x];
                if shade == 0 {
                    continue;
                }
                let _ = write!(
                    s,
                    r#"<rect x="{}" y="{}" width="{cell}" height="{cell}" fill="{}"/>"#,
                    x0 + x as f32 * cell,
                    y0 + y as f32 * cell,
                    shades[usize::from(shade - 1).min(2)]
                );
            }
        }
        text(
            s,
            270.0,
            566.0,
            26.0,
            SANS,
            true,
            &t.ink,
            "middle",
            &escape(&truncate(reveal.name, 22)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;

    fn clues(rows: &[&[u8]]) -> Vec<Vec<u8>> {
        rows.iter().map(|r| r.to_vec()).collect()
    }

    fn card<'a>(
        grid: &'a [bool; 100],
        rows: &'a [Vec<u8>],
        cols: &'a [Vec<u8>],
        mistakes: &'a [usize],
        reveal: Option<Reveal<'a>>,
    ) -> PicrossCard<'a> {
        PicrossCard {
            title: "Daily picross #12",
            subtitle: "42 filled",
            grid,
            rows,
            cols,
            row_done: [true, false, false, false, false, false, false, false, false, false],
            col_done: [false; 10],
            mistakes,
            footer: "Pick a row, then its filled cells",
            reveal,
            theme: Theme::reef(),
        }
    }

    #[test]
    fn marks_mistakes_and_finished_lines_are_drawn() {
        let mut grid = [false; 100];
        grid[0] = true;
        grid[11] = true;
        let rows = clues(&[&[1], &[1, 2], &[0], &[0], &[0], &[0], &[0], &[0], &[0], &[0]]);
        let cols = rows.clone();
        let svg = card(&grid, &rows, &cols, &[11], None).to_svg();
        assert_eq!(svg.matches(r##"fill="#ef476f""##).count(), 1, "one mistake");
        assert!(svg.contains("<path d=\"M"), "the mistake is crossed out");
        assert!(svg.contains("DAILY PICROSS #12"));
        assert_eq!(svg.matches("#06d6a0").count(), 1, "one correct mark");
    }

    #[test]
    fn renders_the_puzzle_and_the_picture_to_pngs_of_the_documented_size() {
        let grid = [false; 100];
        let rows = clues(&[
            &[5, 1, 1, 1, 1],
            &[2],
            &[0],
            &[0],
            &[0],
            &[0],
            &[0],
            &[0],
            &[0],
            &[1, 1, 1, 1, 1],
        ]);
        let cols = rows.clone();
        let png = Renderer::new()
            .picross_card(&card(&grid, &rows, &cols, &[], None))
            .expect("renders");
        let image = image::load_from_memory(&png).expect("a png");
        assert_eq!(
            (image.width(), image.height()),
            (PicrossCard::WIDTH, PicrossCard::HEIGHT)
        );
        let shades: [u8; 100] = std::array::from_fn(|i| (i % 4) as u8);
        let png = Renderer::new()
            .picross_card(&card(
                &grid,
                &rows,
                &cols,
                &[],
                Some(Reveal {
                    name: "Anchor",
                    color: "#8fa0d8",
                    shades: &shades,
                }),
            ))
            .expect("renders");
        let image = image::load_from_memory(&png).expect("a png");
        assert_eq!(
            (image.width(), image.height()),
            (PicrossCard::WIDTH, PicrossCard::HEIGHT)
        );
    }
}
