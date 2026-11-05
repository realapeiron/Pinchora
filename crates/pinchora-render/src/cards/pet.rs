use std::fmt::Write;

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{HEART, STAR, background, data_uri_png, escape, panel, segment_bar, sprite, truncate},
};

/// 860×420 pet status card.
pub struct PetCard {
    /// Sprite scaled up with nearest-neighbour, at most 220×220.
    pub sprite_png: Vec<u8>,
    pub sprite_w: u32,
    pub sprite_h: u32,
    pub name: String,
    pub species: String,
    pub rarity: String,
    pub rarity_color: u32,
    pub level: i32,
    pub xp: i32,
    pub xp_needed: i32,
    /// hunger, fun, energy, hygiene (0..=100).
    pub needs: [i16; 4],
    pub mood: String,
    pub personality: String,
    pub theme: Theme,
}

impl PetCard {
    pub const SPRITE_BOX: u32 = 220;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (860.0, 420.0);
        let mut s = String::with_capacity(64 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        let frame = format!("#{:06x}", self.rarity_color);
        panel(&mut s, 32.0, 32.0, 300.0, 356.0, 8.0, &t.panel, &frame);
        let _ = write!(
            s,
            r##"<ellipse cx="182" cy="{}" rx="{}" ry="10" fill="#000000" fill-opacity="0.3"/><image x="{}" y="{}" width="{}" height="{}" href="{}"/>"##,
            300.0,
            (self.sprite_w as f32 / 2.5).max(30.0),
            182.0 - self.sprite_w as f32 / 2.0,
            300.0 - self.sprite_h as f32,
            self.sprite_w,
            self.sprite_h,
            data_uri_png(&self.sprite_png)
        );
        text(
            &mut s,
            182.0,
            352.0,
            16.0,
            PIXEL,
            false,
            &frame,
            "middle",
            &escape(&self.rarity.to_uppercase()),
        );

        text(
            &mut s,
            364.0,
            80.0,
            40.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&truncate(&self.name, 18)),
        );
        let sub = format!("{} · {}", self.species, self.personality);
        text(
            &mut s,
            366.0,
            112.0,
            19.0,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&truncate(&sub, 44)),
        );

        text(&mut s, 364.0, 152.0, 16.0, PIXEL, false, &t.muted, "start", "LEVEL");
        text(
            &mut s,
            440.0,
            154.0,
            22.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &self.level.to_string(),
        );
        sprite(&mut s, STAR, 486.0, 136.0, 3.4, &t.gold, "#fff7d6");
        let xp = format!("{} / {} XP", self.xp, self.xp_needed);
        text(&mut s, 828.0, 152.0, 14.0, PIXEL, false, &t.ink, "end", &xp);
        segment_bar(
            &mut s,
            364.0,
            164.0,
            464.0,
            28.0,
            self.xp as f32 / self.xp_needed.max(1) as f32,
            16,
            &t.gold,
            "#1f2a57",
            &t.panel_edge,
        );

        let labels = ["HUNGER", "FUN", "ENERGY", "CLEAN"];
        let colors = [&t.accent, &t.heart, &t.mint, "#8fd3ff"];
        for i in 0..4 {
            let y = 216.0 + i as f32 * 40.0;
            text(
                &mut s,
                364.0,
                y + 22.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                labels[i],
            );
            segment_bar(
                &mut s,
                470.0,
                y,
                358.0,
                30.0,
                f32::from(self.needs[i]) / 100.0,
                10,
                colors[i],
                "#1f2a57",
                &t.panel_edge,
            );
        }
        sprite(&mut s, HEART, 364.0, 380.0, 3.4, &t.heart, "#ffc2cf");
        text(
            &mut s,
            396.0,
            398.0,
            16.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &format!("FEELING {}", escape(&self.mood.to_uppercase())),
        );
        s.push_str("</svg>");
        s
    }
}
