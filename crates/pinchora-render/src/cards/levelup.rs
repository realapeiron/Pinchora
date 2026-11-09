use std::fmt::Write;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::avatar_frame;
use crate::cards::text;
use crate::{
    Theme,
    svg::{STAR, background, escape, sprite, truncate},
};

/// 720×220 level-up banner.
pub struct LevelUpCard<'a> {
    pub display_name: &'a str,
    /// PNG, 136×136.
    pub avatar_png: &'a [u8],
    pub from: u32,
    pub to: u32,
    /// The language of the banner.
    pub lang: Lang,
    pub theme: Theme,
}

impl LevelUpCard<'_> {
    pub const AVATAR_SIZE: u32 = 136;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (720.0, 220.0);
        let mut s = String::with_capacity(32 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        avatar_frame(&mut s, self.avatar_png, 34.0, 34.0, 152.0, 8.0, &t.gold, "avatar");
        for (x, y, scale) in [(560.0, 30.0, 6.0), (640.0, 120.0, 4.0), (214.0, 150.0, 3.0)] {
            sprite(&mut s, STAR, x, y, scale, &t.gold, "#fff7d6");
        }
        text(
            &mut s,
            214.0,
            92.0,
            46.0,
            PIXEL,
            false,
            &t.gold,
            "start",
            &t!(self.lang, "card.levelup.title"),
        );
        text(
            &mut s,
            216.0,
            136.0,
            26.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&truncate(self.display_name, 22)),
        );
        let jump = t!(self.lang, "card.levelup.jump", from = self.from, to = self.to);
        text(&mut s, 250.0, 186.0, 26.0, PIXEL, false, &t.accent, "start", &jump);
        s.push_str("</svg>");
        s
    }
}
