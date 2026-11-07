use std::fmt::Write;

use pinchora_core::{i18n::Lang, t};

use super::{Backdrop, PIXEL, SANS, avatar_frame, backdrop, text};
use crate::{
    Theme,
    svg::{STAR, background, compact_in, escape, segment_bar, sprite, thousands_in, truncate},
};

/// The next level reward, shown as "Next role: <name> at level N".
pub struct NextRole<'a> {
    pub name: &'a str,
    pub level: u32,
}

/// 934×282 server rank card.
pub struct RankCard<'a> {
    /// The language of the labels.
    pub lang: Lang,
    pub display_name: &'a str,
    /// The title under the name, e.g. `Tide Angler`.
    pub title: Option<&'a str>,
    pub username: &'a str,
    /// PNG, 200×200.
    pub avatar_png: &'a [u8],
    pub level: u32,
    pub xp_current: u64,
    pub xp_needed: u64,
    pub rank: i64,
    pub next_role: Option<NextRole<'a>>,
    /// The picture behind the card; `None` keeps the plain night-sea look.
    pub backdrop: Option<Backdrop<'a>>,
    pub theme: Theme,
}

impl RankCard<'_> {
    pub const AVATAR_SIZE: u32 = 200;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (934.0, 282.0);
        let mut s = String::with_capacity(48 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        match &self.backdrop {
            Some(b) => {
                backdrop(&mut s, w, h, b, t.opacity.scrim());
                // Everything beside the avatar is loose text: one plate under all of it.
                t.plate(&mut s, 264.0, 33.0, 652.0, 216.0);
            }
            None => background(&mut s, w, h, &t.bg_top, &t.bg_bottom),
        }
        avatar_frame(&mut s, self.avatar_png, 33.0, 33.0, 216.0, 8.0, &t.accent, "avatar");

        text(
            &mut s,
            900.0,
            74.0,
            18.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &t!(self.lang, "card.rank"),
        );
        let rank = format!("#{}", compact_in(self.lang, self.rank));
        let level = self.level.to_string();
        // Right-aligned "RANK #n  LEVEL n" cluster.
        text(&mut s, 900.0, 118.0, 44.0, PIXEL, false, &t.ink, "end", &escape(&rank));
        text(
            &mut s,
            720.0,
            74.0,
            18.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &t!(self.lang, "card.level"),
        );
        text(&mut s, 720.0, 118.0, 44.0, PIXEL, false, &t.accent, "end", &level);

        let name = escape(&truncate(self.display_name, 18));
        let handle = escape(&truncate(&format!("@{}", self.username), 28));
        match self.title {
            // The title takes a line of its own between the name and the handle.
            Some(title) => {
                text(&mut s, 280.0, 98.0, 38.0, SANS, true, &t.ink, "start", &name);
                sprite(&mut s, STAR, 282.0, 109.0, 2.4, &t.gold, "#fff7d6");
                text(
                    &mut s,
                    306.0,
                    123.0,
                    16.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "start",
                    &escape(&truncate(&self.lang.upper(title), 24)),
                );
                text(&mut s, 282.0, 152.0, 20.0, SANS, false, &t.muted, "start", &handle);
            }
            None => {
                text(&mut s, 280.0, 110.0, 38.0, SANS, true, &t.ink, "start", &name);
                text(&mut s, 282.0, 144.0, 20.0, SANS, false, &t.muted, "start", &handle);
            }
        }

        let xp = format!(
            "{} / {} XP",
            thousands_in(self.lang, self.xp_current as i64),
            thousands_in(self.lang, self.xp_needed as i64)
        );
        text(&mut s, 900.0, 186.0, 16.0, PIXEL, false, &t.ink, "end", &xp);
        if let Some(next) = &self.next_role {
            let line = t!(
                self.lang,
                "card.next-role",
                role = truncate(next.name, 18),
                level = next.level
            );
            text(
                &mut s,
                282.0,
                184.0,
                18.0,
                SANS,
                false,
                &t.muted,
                "start",
                &escape(&line),
            );
        }
        let fraction = self.xp_current as f32 / self.xp_needed.max(1) as f32;
        segment_bar(
            &mut s,
            280.0,
            198.0,
            620.0,
            48.0,
            fraction,
            20,
            &t.accent,
            "#1f2a57",
            &t.panel_edge,
        );
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}
