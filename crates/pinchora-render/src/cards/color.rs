use std::fmt::Write;

use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::text;
use crate::{
    Theme,
    svg::{CRAB, STAR, background, escape, panel, sprite, truncate_to},
};

/// One colour on the card: its name and `0xrrggbb`.
pub struct ColorCell {
    pub name: String,
    pub rgb: u32,
}

/// 900-wide colour card: a tile per colour with its name, the member's own marked with a star and a
/// gold edge, and a strip under them that shows a name in the picked colour on Discord's dark chat.
/// All the words are given already in the member's language.
pub struct ColorCard {
    pub title: String,
    pub hint: String,
    pub cells: Vec<ColorCell>,
    /// The index of the member's colour in `cells`.
    pub selected: Option<usize>,
    /// The name the strip shows, in the picked colour.
    pub sample: String,
    /// What the strip says when no colour is picked.
    pub none: String,
    pub theme: Theme,
}

const COLS: usize = 6;
const TILE_W: f32 = 128.0;
const TILE_H: f32 = 84.0;
const GAP: f32 = 12.0;
const LEFT: f32 = 36.0;
const STRIP_H: f32 = 76.0;
/// Discord's dark theme chat background.
const CHAT: &str = "#313338";

fn hex(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0xff_ffff)
}

impl ColorCard {
    pub const WIDTH: f32 = 900.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let rows = self.cells.len().div_ceil(COLS).max(1) as f32;
        let grid_top = 108.0;
        let strip_y = grid_top + rows * (TILE_H + GAP) + 8.0;
        let (w, h) = (Self::WIDTH, strip_y + STRIP_H + 56.0);
        let mut s = String::with_capacity(16 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(
            &mut s,
            LEFT,
            60.0,
            32.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&truncate_to(&self.title, 32.0, 700.0)),
        );
        text(
            &mut s,
            LEFT,
            90.0,
            17.0,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&truncate_to(&self.hint, 17.0, 828.0)),
        );

        for (i, cell) in self.cells.iter().enumerate() {
            let x = LEFT + (i % COLS) as f32 * (TILE_W + GAP);
            let y = grid_top + (i / COLS) as f32 * (TILE_H + GAP);
            let picked = self.selected == Some(i);
            let edge = if picked { &t.gold } else { &t.panel_edge };
            panel(&mut s, x, y, TILE_W, TILE_H, 4.0, &t.panel, edge);
            let _ = write!(
                s,
                r#"<rect x="{}" y="{}" width="{}" height="38" fill="{}"/>"#,
                x + 12.0,
                y + 12.0,
                TILE_W - 24.0,
                hex(cell.rgb)
            );
            text(
                &mut s,
                x + TILE_W / 2.0,
                y + TILE_H - 14.0,
                15.0,
                SANS,
                true,
                &t.ink,
                "middle",
                &escape(&truncate_to(&cell.name, 15.0, TILE_W - 20.0)),
            );
            if picked {
                sprite(&mut s, STAR, x + TILE_W - 26.0, y + 14.0, 2.0, &t.gold, "#fff1e8");
            }
        }

        panel(&mut s, LEFT, strip_y, 828.0, STRIP_H, 4.0, CHAT, &t.panel_edge);
        let _ = write!(
            s,
            r#"<rect x="{}" y="{}" width="44" height="44" fill="{}"/>"#,
            LEFT + 20.0,
            strip_y + 16.0,
            t.accent
        );
        match self.selected.and_then(|i| self.cells.get(i)) {
            Some(cell) => text(
                &mut s,
                LEFT + 78.0,
                strip_y + 46.0,
                24.0,
                SANS,
                true,
                &hex(cell.rgb),
                "start",
                &escape(&truncate_to(&self.sample, 24.0, 700.0)),
            ),
            None => text(
                &mut s,
                LEFT + 78.0,
                strip_y + 45.0,
                20.0,
                SANS,
                false,
                &t.muted,
                "start",
                &escape(&truncate_to(&self.none, 20.0, 700.0)),
            ),
        }

        let footer = h - 20.0;
        sprite(&mut s, CRAB, 752.0, footer - 12.0, 2.0, &t.accent, "#fff1e8");
        text(&mut s, 864.0, footer, 14.0, PIXEL, false, &t.muted, "end", "PINCHORA");
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use crate::Renderer;
    use crate::cards::color::*;

    fn card(n: usize, selected: Option<usize>) -> ColorCard {
        ColorCard {
            title: "COLOURS".to_owned(),
            hint: "Pick one below to colour your name.".to_owned(),
            cells: (0..n)
                .map(|i| ColorCell {
                    name: format!("Colour {i}"),
                    rgb: 0x10_2030 + (i as u32) * 0x0a_0b0c,
                })
                .collect(),
            selected,
            sample: "Sim".to_owned(),
            none: "No colour yet".to_owned(),
            theme: Theme::reef(),
        }
    }

    #[test]
    fn renders_a_grid_that_grows_with_its_colours() {
        let r = Renderer::new();
        let image = |png: &[u8]| image::load_from_memory(png).expect("a png");
        let few = r.color_card(&card(5, None)).expect("renders");
        let many = r.color_card(&card(24, Some(3))).expect("renders");
        assert!(few.starts_with(b"\x89PNG"));
        assert_eq!(image(&few).width(), 900);
        assert!(image(&many).height() > image(&few).height());
    }

    #[test]
    fn the_picked_colour_is_marked_and_named_in_the_strip() {
        let picked = card(8, Some(2)).to_svg();
        let none = card(8, None).to_svg();
        assert!(picked.contains("fill=\"#ffd166\""), "the star and edge are gold");
        assert!(picked.contains(&hex(0x10_2030 + 2 * 0x0a_0b0c)));
        assert!(none.contains("No colour yet") && !picked.contains("No colour yet"));
    }

    #[test]
    fn awkward_names_are_escaped_and_cut() {
        let mut c = card(3, Some(0));
        c.cells[0].name = format!("<b>{}</b>", "w".repeat(80));
        c.sample = "x".repeat(200);
        let svg = c.to_svg();
        assert!(!svg.contains("<b>") && svg.contains("&lt;b&gt;"));
        assert!(Renderer::new().color_card(&c).is_ok());
    }
}
