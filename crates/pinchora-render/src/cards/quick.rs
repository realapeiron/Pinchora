//! Shared pixel canvas for the compact puzzle and party games.
use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{background, escape, panel, wrap_laid_out},
};
use pinchora_core::i18n::Lang;
use std::fmt::Write;

pub struct QuickCard<'a> {
    pub lang: Lang,
    pub title: &'a str,
    pub subtitle: &'a str,
    pub cells: &'a [String],
    pub columns: usize,
    pub prompt: &'a str,
    pub status: &'a str,
    pub theme: Theme,
    /// Text challenge: move each glyph slightly so an image remains readable but cannot be selected.
    pub distort: bool,
}
impl QuickCard<'_> {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let w = 900.0f32;
        let h = if self.cells.is_empty() { 350.0 } else { 680.0 };
        let mut s = String::with_capacity(16_000);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        panel(&mut s, 22.0, 20.0, w - 44.0, h - 40.0, 5.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            50.0,
            68.0,
            27.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &escape(&self.lang.upper(self.title)),
        );
        text(
            &mut s,
            50.0,
            100.0,
            16.0,
            SANS,
            false,
            &t.gold,
            "start",
            &escape(self.subtitle),
        );
        if !self.cells.is_empty() && self.columns > 0 {
            let rows = self.cells.len().div_ceil(self.columns);
            let side = ((w - 120.0) / self.columns as f32).min(470.0 / rows as f32);
            let x0 = (w - side * self.columns as f32) / 2.0;
            let y0 = 130.0;
            for (i, cell) in self.cells.iter().enumerate() {
                let (x, y) = (
                    x0 + (i % self.columns) as f32 * side,
                    y0 + (i / self.columns) as f32 * side,
                );
                let fill = if cell == "*" || cell == "O" {
                    &t.gold
                } else if cell == "X" || cell == "F" {
                    &t.accent
                } else {
                    &t.ink
                };
                panel(
                    &mut s,
                    x + 2.0,
                    y + 2.0,
                    side - 4.0,
                    side - 4.0,
                    3.0,
                    &t.panel,
                    &t.muted,
                );
                text(
                    &mut s,
                    x + side / 2.0,
                    y + side * 0.64,
                    (side * 0.43).min(32.0),
                    PIXEL,
                    false,
                    fill,
                    "middle",
                    &escape(cell),
                );
            }
            if self.columns >= 8 {
                for col in 0..self.columns {
                    let x = x0 + (col as f32 + 0.5) * side;
                    text(
                        &mut s,
                        x,
                        122.0,
                        13.0,
                        PIXEL,
                        false,
                        &t.muted,
                        "middle",
                        &((b'A' + col as u8) as char).to_string(),
                    );
                }
                for row in 0..rows {
                    let y = y0 + (row as f32 + 0.6) * side;
                    text(
                        &mut s,
                        x0 - 14.0,
                        y,
                        13.0,
                        PIXEL,
                        false,
                        &t.muted,
                        "end",
                        &(row + 1).to_string(),
                    );
                }
            }
        } else if self.distort {
            let chars: Vec<_> = self.prompt.chars().collect();
            let size = if chars.len() > 42 { 18.0 } else { 22.0 };
            let width = (w - 120.0) / (chars.len().max(1) as f32);
            for (i, c) in chars.iter().enumerate() {
                let x = 60.0 + i as f32 * width;
                let y = 190.0 + (i % 5) as f32 * 1.6;
                let angle = (i as i32 % 5 - 2) as f32 * 1.8;
                let _ = write!(s, r#"<g transform="rotate({angle} {x} {y})">"#);
                text(&mut s, x, y, size, SANS, true, &t.ink, "start", &escape(&c.to_string()));
                s.push_str("</g>");
            }
        } else {
            let lines = wrap_laid_out(self.prompt, SANS, true, 30.0, 780.0, 4);
            for (i, line) in lines.iter().enumerate() {
                text(
                    &mut s,
                    450.0,
                    160.0 + i as f32 * 45.0,
                    30.0,
                    SANS,
                    true,
                    &t.ink,
                    "middle",
                    &escape(line),
                );
            }
        }
        text(
            &mut s,
            450.0,
            h - 56.0,
            17.0,
            PIXEL,
            false,
            &t.mint,
            "middle",
            &escape(&self.lang.upper(self.status)),
        );
        s.push_str("</svg>");
        s
    }
}
