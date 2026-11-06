use std::fmt::Write;

use pinchora_core::{i18n::Lang, t};

use super::{Backdrop, PIXEL, SANS, avatar_frame, backdrop, text};
use crate::{
    Theme,
    svg::{HEART, SHELL, STAR, background, compact, escape, sprite, truncate},
};

/// 900×300 wallet card.
pub struct WalletCard<'a> {
    /// The language of the labels; the timers come translated.
    pub lang: Lang,
    pub display_name: &'a str,
    /// PNG, 136×136.
    pub avatar_png: &'a [u8],
    pub shells: i64,
    pub pearls: i64,
    pub streak: i32,
    /// How long until each reward can be collected again, e.g. `3h 20m`, or `time.ready` in [`Self::lang`].
    pub next_daily: &'a str,
    pub next_weekly: &'a str,
    pub next_monthly: &'a str,
    pub pets: i64,
    pub fish_found: i64,
    pub fish_total: i64,
    /// The picture behind the card; `None` keeps the plain night-sea look.
    pub backdrop: Option<Backdrop<'a>>,
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
        match &self.backdrop {
            Some(b) => {
                backdrop(&mut s, w, h, b, t.opacity.scrim());
                // The name under the avatar is the only text outside a panel.
                t.plate(&mut s, 24.0, 196.0, 168.0, 68.0);
            }
            None => background(&mut s, w, h, &t.bg_top, &t.bg_bottom),
        }
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
        text(
            &mut s,
            108.0,
            250.0,
            13.0,
            PIXEL,
            false,
            &t.muted,
            "middle",
            &t!(self.lang, "card.wallet"),
        );

        let big = [
            (
                t!(self.lang, "card.shells"),
                compact(self.shells),
                Some((SHELL, t.gold.as_str())),
            ),
            (
                t!(self.lang, "card.pearls"),
                compact(self.pearls),
                Some((STAR, "#8fd3ff")),
            ),
        ];
        for (i, (label, value, icon)) in big.iter().enumerate() {
            let x = 216.0 + i as f32 * 336.0;
            t.panel(&mut s, x, 32.0, 320.0, 112.0, 4.0);
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
        // Four tiles: the three counts, and the time left on each reward next to one another.
        let small = [
            (
                t!(self.lang, "card.streak"),
                t!(self.lang, "card.days", count = self.streak.max(0)),
            ),
            (t!(self.lang, "card.pets"), self.pets.to_string()),
            (
                t!(self.lang, "card.fish-book"),
                format!("{}/{}", self.fish_found, self.fish_total),
            ),
        ];
        let widths = [136.0, 88.0, 120.0];
        let mut x = 216.0;
        for ((label, value), width) in small.iter().zip(widths) {
            t.panel(&mut s, x, 160.0, width, 104.0, 4.0);
            text(&mut s, x + 16.0, 190.0, 13.0, PIXEL, false, &t.muted, "start", label);
            text(
                &mut s,
                x + 16.0,
                236.0,
                18.0,
                PIXEL,
                false,
                &t.ink,
                "start",
                &escape(&truncate(value, 8)),
            );
            x += width + 16.0;
        }
        // The timers: a reward that can be collected now stands out in green.
        let width = 868.0 - x;
        t.panel(&mut s, x, 160.0, width, 104.0, 4.0);
        let timers = [
            (t!(self.lang, "card.daily"), self.next_daily),
            (t!(self.lang, "card.weekly"), self.next_weekly),
            (t!(self.lang, "card.monthly"), self.next_monthly),
        ];
        let ready = t!(self.lang, "time.ready");
        for (i, (label, value)) in timers.iter().enumerate() {
            let y = 189.0 + i as f32 * 28.0;
            let color = if *value == ready { &t.mint } else { &t.ink };
            text(&mut s, x + 16.0, y, 13.0, PIXEL, false, &t.muted, "start", label);
            text(
                &mut s,
                x + width - 16.0,
                y,
                15.0,
                PIXEL,
                false,
                color,
                "end",
                &escape(&truncate(&self.lang.upper(value), 9)),
            );
        }
        sprite(&mut s, HEART, 860.0, 270.0, 2.4, &t.heart, "#ffc2cf");
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}
