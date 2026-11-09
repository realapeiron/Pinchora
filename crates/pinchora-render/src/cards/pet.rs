use std::fmt::Write;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::text;
use crate::{
    Theme,
    svg::{HEART, STAR, background, data_uri_png, escape, panel, pixel_width, plate, segment_bar, sprite, truncate},
};

/// Whether a pet is up and about, asleep, away on a trip, or back with a souvenir to claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetStatus {
    Awake,
    Asleep,
    Away,
    Back,
}

/// 860×420 pet status card.
pub struct PetCard {
    /// The language of the labels; the names, the mood and the time left come translated.
    pub lang: Lang,
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
    /// XP to the next level; 0 for a pet at the top level.
    pub xp_needed: i32,
    /// hunger, fun, energy, hygiene (0..=100).
    pub needs: [i16; 4],
    pub mood: String,
    pub personality: String,
    pub status: PetStatus,
    /// The trip's destination, for a pet that is away or just back.
    pub place: String,
    /// When an away pet is back, e.g. `in 2 h 5 min`.
    pub back: String,
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
        self.draw_portrait(&mut s, t);
        self.draw_identity_and_experience(&mut s, t);
        self.draw_needs_and_mood(&mut s, t);
        s.push_str("</svg>");
        s
    }
    fn draw_portrait(&self, s: &mut String, t: &Theme) {
        let frame = format!("#{:06x}", self.rarity_color);
        panel(s, 32.0, 32.0, 300.0, 356.0, 8.0, &t.panel, &frame);
        // A sleeping pet is a little faded, a pet on a trip is only a ghost of itself.
        let opacity = match self.status {
            PetStatus::Awake | PetStatus::Back => 1.0,
            PetStatus::Asleep => 0.55,
            PetStatus::Away => 0.18,
        };
        let _ = write!(
            s,
            r##"<ellipse cx="182" cy="{}" rx="{}" ry="10" fill="#000000" fill-opacity="{}"/><image x="{}" y="{}" width="{}" height="{}" opacity="{opacity}" href="{}"/>"##,
            300.0,
            (self.sprite_w as f32 / 2.5).max(30.0),
            0.3 * opacity,
            182.0 - self.sprite_w as f32 / 2.0,
            300.0 - self.sprite_h as f32,
            self.sprite_w,
            self.sprite_h,
            data_uri_png(&self.sprite_png)
        );
        match self.status {
            PetStatus::Asleep => {
                // Zs rising in the corner of the panel, bigger as they climb.
                for (n, size) in [20.0, 28.0, 38.0].into_iter().enumerate() {
                    let (x, y) = (256.0 + n as f32 * 18.0, 124.0 - [0.0, 26.0, 54.0][n]);
                    text(s, x, y, size, PIXEL, false, "#b8c4ff", "start", "Z");
                }
            }
            PetStatus::Away => {
                plate(s, 52.0, 148.0, 260.0, 80.0, 0.7);
                text(
                    s,
                    182.0,
                    180.0,
                    24.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "middle",
                    &t!(self.lang, "card.pet.on-trip"),
                );
                let place = escape(&truncate(&self.place, 26));
                text(s, 182.0, 212.0, 15.0, SANS, true, &t.ink, "middle", &place);
            }
            PetStatus::Awake | PetStatus::Back => {}
        }
        text(
            s,
            182.0,
            352.0,
            16.0,
            PIXEL,
            false,
            &frame,
            "middle",
            &escape(&self.lang.upper(&self.rarity)),
        );
    }

    fn draw_identity_and_experience(&self, s: &mut String, t: &Theme) {
        text(
            s,
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
            s,
            366.0,
            112.0,
            19.0,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&truncate(&sub, 44)),
        );

        text(
            s,
            364.0,
            152.0,
            16.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &t!(self.lang, "card.level"),
        );
        let level = self.level.to_string();
        text(s, 440.0, 154.0, 22.0, PIXEL, false, &t.ink, "start", &level);
        // The star follows the digits at a fixed gap, so a two-digit level does not crowd it.
        let star_x = 440.0 + pixel_width(&level, 22.0) + 22.0;
        sprite(s, STAR, star_x, 136.0, 3.4, &t.gold, "#fff7d6");
        let maxed = self.xp_needed <= 0;
        let xp = match (maxed, self.status) {
            (true, _) => t!(self.lang, "card.pet.max-level"),
            (false, PetStatus::Asleep) => t!(self.lang, "card.pet.no-xp-asleep"),
            _ => t!(self.lang, "card.pet.xp", xp = self.xp, needed = self.xp_needed),
        };
        text(s, 828.0, 152.0, 14.0, PIXEL, false, &t.ink, "end", &xp);
        segment_bar(
            s,
            364.0,
            164.0,
            464.0,
            28.0,
            if maxed {
                1.0
            } else {
                self.xp as f32 / self.xp_needed.max(1) as f32
            },
            16,
            &t.gold,
            "#1f2a57",
            &t.panel_edge,
        );
    }

    fn draw_needs_and_mood(&self, s: &mut String, t: &Theme) {
        let labels = [
            t!(self.lang, "card.pet.hunger"),
            t!(self.lang, "card.pet.fun"),
            t!(self.lang, "card.pet.energy"),
            t!(self.lang, "card.pet.clean"),
        ];
        let colors = [&t.accent, &t.heart, &t.mint, "#8fd3ff"];
        for i in 0..4 {
            let y = 216.0 + i as f32 * 40.0;
            text(s, 364.0, y + 22.0, 14.0, PIXEL, false, &t.muted, "start", &labels[i]);
            segment_bar(
                s,
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
        sprite(s, HEART, 364.0, 380.0, 3.4, &t.heart, "#ffc2cf");
        let line = match self.status {
            PetStatus::Awake => t!(self.lang, "card.pet.feeling", mood = self.lang.upper(&self.mood)),
            PetStatus::Asleep => t!(self.lang, "card.pet.asleep-line"),
            PetStatus::Away => t!(self.lang, "card.pet.away-line", back = self.lang.upper(&self.back)),
            PetStatus::Back => t!(self.lang, "card.pet.back-line"),
        };
        text(s, 396.0, 398.0, 16.0, PIXEL, false, &t.ink, "start", &escape(&line));
    }
}
