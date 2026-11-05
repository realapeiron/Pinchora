use std::fmt::Write;

use super::{PIXEL, SANS, avatar_frame, text};
use crate::{
    Theme,
    svg::{HEART, SHELL, STAR, background, compact, escape, panel, sprite, truncate},
};

/// 900×300 wallet card.
pub struct WalletCard<'a> {
    pub display_name: &'a str,
    /// PNG, 136×136.
    pub avatar_png: &'a [u8],
    pub shells: i64,
    pub pearls: i64,
    pub streak: i32,
    pub next_daily: &'a str,
    pub pets: i64,
    pub fish_found: i64,
    pub fish_total: i64,
    pub theme: Theme,
}

impl WalletCard<'_> {
    pub const AVATAR_SIZE: u32 = 136;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (900.0, 300.0);
        let mut s = String::with_capacity(48 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        avatar_frame(&mut s, self.avatar_png, 32.0, 32.0, 152.0, 8.0, &t.gold, "avatar");
        text(
            &mut s,
            108.0,
            222.0,
            20.0,
            SANS,
            true,
            &t.ink,
            "middle",
            &escape(&truncate(self.display_name, 14)),
        );
        text(&mut s, 108.0, 250.0, 13.0, PIXEL, false, &t.muted, "middle", "WALLET");

        let big = [
            ("SHELLS", compact(self.shells), Some((SHELL, t.gold.as_str()))),
            ("PEARLS", compact(self.pearls), Some((STAR, "#8fd3ff"))),
        ];
        for (i, (label, value, icon)) in big.iter().enumerate() {
            let x = 216.0 + i as f32 * 336.0;
            panel(&mut s, x, 32.0, 320.0, 112.0, 4.0, &t.panel, &t.panel_edge);
            text(&mut s, x + 20.0, 66.0, 15.0, PIXEL, false, &t.muted, "start", label);
            text(
                &mut s,
                x + 20.0,
                120.0,
                40.0,
                PIXEL,
                false,
                &t.ink,
                "start",
                &escape(value),
            );
            if let Some((rows, color)) = icon {
                sprite(&mut s, rows, x + 254.0, 54.0, 6.0, color, "#ffffff");
            }
        }
        let small = [
            (
                "STREAK",
                format!("{} DAY{}", self.streak.max(0), if self.streak == 1 { "" } else { "S" }),
            ),
            ("NEXT DAILY", self.next_daily.to_uppercase()),
            ("PETS", self.pets.to_string()),
            ("FISH BOOK", format!("{}/{}", self.fish_found, self.fish_total)),
        ];
        for (i, (label, value)) in small.iter().enumerate() {
            let x = 216.0 + i as f32 * 168.0;
            panel(&mut s, x, 160.0, 152.0, 104.0, 4.0, &t.panel, &t.panel_edge);
            text(&mut s, x + 16.0, 190.0, 13.0, PIXEL, false, &t.muted, "start", label);
            text(
                &mut s,
                x + 16.0,
                236.0,
                20.0,
                PIXEL,
                false,
                &t.ink,
                "start",
                &escape(&truncate(value, 9)),
            );
        }
        sprite(&mut s, HEART, 860.0, 270.0, 2.4, &t.heart, "#ffc2cf");
        s.push_str("</svg>");
        s
    }
}
