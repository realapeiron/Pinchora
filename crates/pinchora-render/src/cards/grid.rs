use std::fmt::Write;

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{SHELL, background, data_uri_png, escape, panel, sprite, truncate},
};

/// One cell of an item grid.
pub struct GridSlot {
    /// Sprite already scaled up with nearest-neighbour, at most 120×110.
    pub sprite_png: Vec<u8>,
    pub sprite_w: u32,
    pub sprite_h: u32,
    pub name: String,
    /// Main line, e.g. a price.
    pub line1: String,
    /// Second line, e.g. stock left or quantity owned.
    pub line2: String,
    /// `0..=1` fill of a small bar under line 2 (stock left), if any.
    pub bar: Option<f32>,
    /// Frame colour, usually the rarity colour.
    pub frame: u32,
    pub badge: String,
    /// Shows the shell icon before line 1.
    pub shell_price: bool,
}

/// 1000×600 card with up to 8 items in a 4×2 grid: hourly market, catalog, inventory.
pub struct ItemGridCard {
    pub title: String,
    pub subtitle: String,
    pub slots: Vec<GridSlot>,
    pub footer: String,
    pub theme: Theme,
}

impl ItemGridCard {
    pub const SPRITE_BOX: (u32, u32) = (120, 110);

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (1000.0, 600.0);
        let mut s = String::with_capacity(96 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(
            &mut s,
            36.0,
            62.0,
            30.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &escape(&self.title.to_uppercase()),
        );
        text(
            &mut s,
            964.0,
            60.0,
            18.0,
            PIXEL,
            false,
            &t.gold,
            "end",
            &escape(&self.subtitle),
        );

        for (i, slot) in self.slots.iter().take(8).enumerate() {
            let (col, row) = ((i % 4) as f32, (i / 4) as f32);
            let (x, y) = (36.0 + col * 236.0, 88.0 + row * 248.0);
            let frame = format!("#{:06x}", slot.frame);
            panel(&mut s, x, y, 220.0, 232.0, 4.0, &t.panel, &frame);
            // Rarity strip and slot badge.
            let _ = write!(
                s,
                r#"<rect x="{}" y="{}" width="212" height="6" fill="{frame}"/>"#,
                x + 4.0,
                y + 4.0
            );
            text(
                &mut s,
                x + 16.0,
                y + 32.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &escape(&slot.badge),
            );

            let (bw, bh) = Self::SPRITE_BOX;
            let sx = x + 110.0 - slot.sprite_w as f32 / 2.0;
            let sy = y + 40.0 + (bh as f32 - slot.sprite_h as f32);
            let _ = write!(
                s,
                r##"<ellipse cx="{}" cy="{}" rx="{}" ry="7" fill="#000000" fill-opacity="0.25"/>"##,
                x + 110.0,
                y + 40.0 + bh as f32 + 2.0,
                (slot.sprite_w as f32 / 2.0).min(bw as f32 / 2.0)
            );
            let _ = write!(
                s,
                r#"<image x="{sx}" y="{sy}" width="{}" height="{}" href="{}"/>"#,
                slot.sprite_w,
                slot.sprite_h,
                data_uri_png(&slot.sprite_png)
            );

            text(
                &mut s,
                x + 110.0,
                y + 178.0,
                17.0,
                SANS,
                true,
                &t.ink,
                "middle",
                &escape(&truncate(&slot.name, 22)),
            );
            let price_x = if slot.shell_price { x + 120.0 } else { x + 110.0 };
            if slot.shell_price {
                let label_w = slot.line1.chars().count() as f32 * 9.5;
                sprite(
                    &mut s,
                    SHELL,
                    x + 110.0 - label_w / 2.0 - 18.0,
                    y + 189.0,
                    2.6,
                    &t.gold,
                    "#fff7d6",
                );
            }
            text(
                &mut s,
                price_x,
                y + 204.0,
                16.0,
                PIXEL,
                false,
                &t.gold,
                "middle",
                &escape(&slot.line1),
            );
            text(
                &mut s,
                x + 110.0,
                y + 222.0,
                12.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &escape(&slot.line2),
            );
            if let Some(fill) = slot.bar {
                let bw = 160.0;
                let _ = write!(
                    s,
                    r##"<rect x="{}" y="{}" width="{bw}" height="4" fill="#0b1029"/><rect x="{}" y="{}" width="{}" height="4" fill="{}"/>"##,
                    x + 30.0,
                    y + 226.0,
                    x + 30.0,
                    y + 226.0,
                    bw * fill.clamp(0.0, 1.0),
                    t.mint
                );
            }
        }
        if self.slots.is_empty() {
            text(
                &mut s,
                500.0,
                330.0,
                22.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                "NOTHING HERE YET",
            );
        }
        text(
            &mut s,
            964.0,
            588.0,
            13.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &escape(&self.footer),
        );
        s.push_str("</svg>");
        s
    }
}
