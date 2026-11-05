use std::fmt::Write;

use super::{PIXEL, SANS, avatar_frame, text};
use crate::{
    Theme,
    svg::{CRAB, background, escape, sprite, thousands, truncate},
};

/// 1100×500 welcome card.
pub struct WelcomeCard<'a> {
    pub display_name: &'a str,
    /// PNG, 176×176.
    pub avatar_png: &'a [u8],
    pub server_name: &'a str,
    pub member_number: Option<u64>,
    /// Big headline, e.g. "WELCOME".
    pub title: &'a str,
    pub theme: Theme,
}

impl WelcomeCard<'_> {
    pub const AVATAR_SIZE: u32 = 176;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (1100.0, 500.0);
        let mut s = String::with_capacity(48 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);

        // Pixel "stage" floor.
        let _ = write!(
            s,
            r#"<rect x="0" y="440" width="{w}" height="60" fill="{}" fill-opacity="0.55"/>"#,
            t.panel
        );
        for i in 0..28 {
            let x = i as f32 * 40.0;
            let _ = write!(
                s,
                r#"<rect x="{x}" y="440" width="20" height="8" fill="{}" fill-opacity="0.6"/>"#,
                t.panel_edge
            );
        }
        sprite(&mut s, CRAB, 70.0, 392.0, 6.0, &t.accent, "#fff1e8");
        sprite(&mut s, CRAB, 976.0, 392.0, 6.0, &t.gold, "#fff1e8");

        avatar_frame(&mut s, self.avatar_png, 450.0, 36.0, 200.0, 12.0, &t.accent, "avatar");

        let title = escape(&truncate(self.title, 18)).to_uppercase();
        text(&mut s, 550.0, 316.0, 64.0, PIXEL, false, &t.ink, "middle", &title);
        let name = escape(&truncate(self.display_name, 28));
        text(&mut s, 550.0, 368.0, 36.0, SANS, true, &t.accent, "middle", &name);
        let subtitle = match self.member_number {
            Some(n) => format!(
                "to {} · member #{}",
                truncate(self.server_name, 32),
                thousands(n as i64)
            ),
            None => format!("to {}", truncate(self.server_name, 40)),
        };
        text(
            &mut s,
            550.0,
            410.0,
            22.0,
            SANS,
            false,
            &t.muted,
            "middle",
            &escape(&subtitle),
        );
        s.push_str("</svg>");
        s
    }
}
