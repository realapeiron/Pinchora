use std::fmt::Write;

use super::{PIXEL, SANS, avatar_frame, text};
use crate::{
    Theme,
    svg::{background, compact, escape, segment_bar, thousands, truncate},
};

/// 934×282 server rank card.
pub struct RankCard<'a> {
    pub display_name: &'a str,
    pub username: &'a str,
    /// PNG, 200×200.
    pub avatar_png: &'a [u8],
    pub level: u32,
    pub xp_current: u64,
    pub xp_needed: u64,
    pub rank: i64,
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
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        avatar_frame(&mut s, self.avatar_png, 33.0, 33.0, 216.0, 8.0, &t.accent, "avatar");

        text(&mut s, 900.0, 74.0, 18.0, PIXEL, false, &t.muted, "end", "RANK");
        let rank = format!("#{}", compact(self.rank));
        let level = self.level.to_string();
        // Right-aligned "RANK #n  LEVEL n" cluster.
        text(&mut s, 900.0, 118.0, 44.0, PIXEL, false, &t.ink, "end", &escape(&rank));
        text(&mut s, 720.0, 74.0, 18.0, PIXEL, false, &t.muted, "end", "LEVEL");
        text(&mut s, 720.0, 118.0, 44.0, PIXEL, false, &t.accent, "end", &level);

        let name = escape(&truncate(self.display_name, 18));
        text(&mut s, 280.0, 110.0, 38.0, SANS, true, &t.ink, "start", &name);
        let handle = escape(&truncate(&format!("@{}", self.username), 28));
        text(&mut s, 282.0, 144.0, 20.0, SANS, false, &t.muted, "start", &handle);

        let xp = format!(
            "{} / {} XP",
            thousands(self.xp_current as i64),
            thousands(self.xp_needed as i64)
        );
        text(&mut s, 900.0, 186.0, 16.0, PIXEL, false, &t.ink, "end", &xp);
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
        s.push_str("</svg>");
        s
    }
}
