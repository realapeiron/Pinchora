use std::fmt::Write;

use super::{PIXEL, SpriteArt, text};
use crate::{
    Theme,
    svg::{background, compact, data_uri_png, escape, mix, panel, truncate},
};

const TILE: f32 = 112.0;
const GAP: f32 = 12.0;
const BOARD: f32 = 4.0 * TILE + 5.0 * GAP;

/// A tile's colour by exponent (1 is the "2" tile), cool at the bottom and warm at the top.
const TILE_COLORS: [&str; 12] = [
    "#2e3d75", "#35509a", "#3a6dbb", "#3a9bdc", "#2cb7c9", "#06d6a0", "#7fd36a", "#ffd166", "#ff9e4a", "#ff6b4a",
    "#ef476f", "#9b6bc4",
];

/// 560×712 Tide 2048: score and best, a 4×4 board where every tile is a sea creature, a status line.
pub struct Tide2048Card<'a> {
    /// The tiles from the top left, as exponents: 0 empty, 1 the "2" tile, 2 the "4" tile, and so on.
    pub cells: &'a [u8; 16],
    pub score: u32,
    pub best: u32,
    /// A creature for each exponent, starting at 1; the last one is used for any bigger tile.
    pub creatures: &'a [SpriteArt<'a>],
    /// The tile that has just appeared, marked with a small spark.
    pub fresh: Option<usize>,
    pub status: &'a str,
    pub theme: Theme,
}

impl Tide2048Card<'_> {
    pub const WIDTH: u32 = 560;
    pub const HEIGHT: u32 = 712;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH as f32, Self::HEIGHT as f32);
        let mut s = String::with_capacity(64 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);

        for (i, (label, value)) in [("SCORE", self.score), ("BEST", self.best)].into_iter().enumerate() {
            let x = 26.0 + i as f32 * 262.0;
            panel(&mut s, x, 20.0, 246.0, 70.0, 4.0, &t.panel, &t.panel_edge);
            text(&mut s, x + 18.0, 46.0, 12.0, PIXEL, false, &t.muted, "start", label);
            text(
                &mut s,
                x + 18.0,
                78.0,
                28.0,
                PIXEL,
                false,
                &t.gold,
                "start",
                &compact(i64::from(value)),
            );
        }

        let (bx, by) = (26.0, 106.0);
        panel(&mut s, bx, by, BOARD, BOARD, 6.0, "#10204a", &t.panel_edge);
        for (cell, &exp) in self.cells.iter().enumerate() {
            let (x, y) = (
                bx + GAP + (cell % 4) as f32 * (TILE + GAP),
                by + GAP + (cell / 4) as f32 * (TILE + GAP),
            );
            if exp == 0 {
                let _ = write!(
                    s,
                    r##"<rect x="{x}" y="{y}" width="{TILE}" height="{TILE}" fill="#16275a"/>"##
                );
                continue;
            }
            let color = TILE_COLORS[usize::from(exp - 1).min(TILE_COLORS.len() - 1)];
            panel(&mut s, x, y, TILE, TILE, 4.0, &mix(color, "#0b1029", 0.55), color);
            let _ = write!(
                s,
                r##"<rect x="{}" y="{}" width="{}" height="6" fill="#ffffff" fill-opacity="0.2"/>"##,
                x + 4.0,
                y + 4.0,
                TILE - 8.0
            );
            let value = 1u32 << u32::from(exp).min(31);
            let size = if value >= 1024 { 17.0 } else { 20.0 };
            text(
                &mut s,
                x + 12.0,
                y + 30.0,
                size,
                PIXEL,
                false,
                "#fff1e8",
                "start",
                &value.to_string(),
            );
            if let Some(art) = self
                .creatures
                .get(usize::from(exp - 1))
                .or(self.creatures.last())
                .filter(|a| a.w > 0)
            {
                let (aw, ah) = (art.w as f32, art.h as f32);
                let _ = write!(
                    s,
                    r#"<image x="{}" y="{}" width="{aw}" height="{ah}" image-rendering="optimizeSpeed" href="{}"/>"#,
                    x + (TILE - aw) / 2.0,
                    y + 40.0 + (TILE - 48.0 - ah) / 2.0,
                    data_uri_png(art.png)
                );
            }
            if self.fresh == Some(cell) {
                let _ = write!(
                    s,
                    r##"<rect x="{}" y="{}" width="8" height="8" fill="#ffe08a" stroke="#0b1029" stroke-width="2"/>"##,
                    x + TILE - 20.0,
                    y + 12.0
                );
            }
        }

        panel(&mut s, 26.0, 628.0, 508.0, 56.0, 4.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            46.0,
            663.0,
            17.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &escape(&truncate(&self.status.to_uppercase(), 40)),
        );
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;

    fn art() -> Vec<u8> {
        // A 24×12 picture as real PNG bytes, so the renderer can decode it.
        let image = image::RgbaImage::from_pixel(24, 12, image::Rgba([255, 158, 122, 255]));
        crate::encode_png(&image).expect("encodes")
    }

    fn card<'a>(cells: &'a [u8; 16], creatures: &'a [SpriteArt<'a>]) -> Tide2048Card<'a> {
        Tide2048Card {
            cells,
            score: 1_248,
            best: 20_480,
            creatures,
            fresh: Some(0),
            status: "Swipe with the arrows",
            theme: Theme::reef(),
        }
    }

    #[test]
    fn every_tile_shows_its_value_and_a_creature() {
        let png = art();
        let creatures = [SpriteArt {
            png: &png,
            w: 72,
            h: 36,
        }];
        let mut cells = [0u8; 16];
        cells[0] = 1;
        cells[5] = 11;
        cells[15] = 14;
        let svg = card(&cells, &creatures).to_svg();
        assert_eq!(
            svg.matches("<image ").count(),
            3,
            "a creature per tile; bigger tiles reuse the last one"
        );
        assert!(svg.contains(">2<") && svg.contains(">2048<") && svg.contains(">16384<"));
        assert!(
            svg.contains("1,248") || svg.contains("1.2K") || svg.contains(">1248<"),
            "the score is shown"
        );
        assert_eq!(svg.matches("#ffe08a").count(), 1, "the fresh tile's spark");
    }

    #[test]
    fn renders_to_a_png_of_the_documented_size() {
        let png = art();
        let creatures = [SpriteArt {
            png: &png,
            w: 72,
            h: 36,
        }];
        let cells = [1u8; 16];
        let out = Renderer::new()
            .tide2048_card(&card(&cells, &creatures))
            .expect("renders");
        let image = image::load_from_memory(&out).expect("a png");
        assert_eq!(
            (image.width(), image.height()),
            (Tide2048Card::WIDTH, Tide2048Card::HEIGHT)
        );
    }
}
