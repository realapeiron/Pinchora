use std::fmt::Write;

use super::{Backdrop, BadgeIcon, PIXEL, SANS, SpriteArt, avatar_frame, backdrop, text};
use crate::{
    Theme,
    svg::{
        CRAB, HEART, SHELL, STAR, background, compact, data_uri_png, escape, panel, plate, segment_bar, sprite,
        thousands, truncate, wrap,
    },
};

/// 960×540 profile card.
pub struct ProfileCard<'a> {
    pub display_name: &'a str,
    /// The title under the name, e.g. `Tide Angler`.
    pub title: Option<&'a str>,
    pub username: &'a str,
    /// PNG, 208×208 (see [`crate::prepare_avatar`]).
    pub avatar_png: &'a [u8],
    pub bio: &'a str,
    pub level: u32,
    pub xp_current: u64,
    pub xp_needed: u64,
    pub total_xp: i64,
    pub credits: i64,
    pub reputation: i32,
    pub streak: i32,
    pub global_rank: i64,
    pub server_rank: Option<i64>,
    /// The player's character, scaled [`ProfileCard::CHARACTER_SCALE`]×; it stands in the
    /// top-right corner when set.
    pub character: Option<SpriteArt<'a>>,
    /// The picture behind the card; `None` keeps the plain night-sea look.
    pub backdrop: Option<Backdrop<'a>>,
    /// Badges in order; only the first [`ProfileCard::MAX_BADGES`] are drawn.
    pub badges: &'a [BadgeIcon<'a>],
    pub theme: Theme,
}

impl ProfileCard<'_> {
    pub const AVATAR_SIZE: u32 = 208;
    pub const CHARACTER_SCALE: u32 = 2;
    pub const MAX_BADGES: usize = 6;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (960.0, 540.0);
        let mut s = String::with_capacity(64 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        match &self.backdrop {
            Some(b) => {
                backdrop(&mut s, w, h, b);
                // Text that is not inside a panel gets a plate of its own.
                if self.title.is_some() {
                    plate(&mut s, 288.0, 40.0, 640.0, 96.0);
                } else {
                    plate(&mut s, 288.0, 44.0, 640.0, 88.0);
                }
                plate(&mut s, 288.0, 400.0, 640.0, 136.0);
            }
            None => background(&mut s, w, h, &t.bg_top, &t.bg_bottom),
        }

        // Left column: avatar, level, rep, streak.
        avatar_frame(&mut s, self.avatar_png, 40.0, 40.0, 224.0, 8.0, &t.accent, "avatar");

        panel(&mut s, 40.0, 280.0, 224.0, 88.0, 4.0, &t.panel, &t.panel_edge);
        text(&mut s, 60.0, 312.0, 18.0, PIXEL, false, &t.muted, "start", "LEVEL");
        text(
            &mut s,
            60.0,
            352.0,
            40.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &self.level.to_string(),
        );
        sprite(&mut s, STAR, 200.0, 300.0, 6.0, &t.gold, "#fff7d6");

        panel(&mut s, 40.0, 384.0, 224.0, 56.0, 4.0, &t.panel, &t.panel_edge);
        sprite(&mut s, HEART, 58.0, 396.0, 4.6, &t.heart, "#ffc2cf");
        text(
            &mut s,
            104.0,
            421.0,
            20.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &format!("{} REP", compact(i64::from(self.reputation))),
        );

        panel(&mut s, 40.0, 456.0, 224.0, 56.0, 4.0, &t.panel, &t.panel_edge);
        sprite(&mut s, STAR, 58.0, 468.0, 4.6, &t.mint, "#c9fff0");
        text(
            &mut s,
            104.0,
            493.0,
            20.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &format!("{} DAY", self.streak.max(0)),
        );

        // Right column: name, bio, stats, XP. With a character, text keeps clear of its corner.
        let (name_chars, bio_chars) = if self.character.is_some() { (17, 38) } else { (22, 46) };
        let name = escape(&truncate(self.display_name, name_chars));
        let handle = escape(&truncate(&format!("@{}", self.username), 34));
        match self.title {
            // The title takes a line of its own between the name and the handle.
            Some(title) => {
                text(&mut s, 296.0, 80.0, 44.0, SANS, true, &t.ink, "start", &name);
                sprite(&mut s, STAR, 298.0, 92.0, 2.4, &t.gold, "#fff7d6");
                text(
                    &mut s,
                    322.0,
                    106.0,
                    16.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "start",
                    &escape(&truncate(&title.to_uppercase(), 24)),
                );
                text(&mut s, 298.0, 130.0, 20.0, SANS, false, &t.muted, "start", &handle);
            }
            None => {
                text(&mut s, 296.0, 88.0, 44.0, SANS, true, &t.ink, "start", &name);
                text(&mut s, 298.0, 122.0, 22.0, SANS, false, &t.muted, "start", &handle);
            }
        }

        panel(&mut s, 296.0, 140.0, 624.0, 132.0, 4.0, &t.panel, &t.panel_edge);
        text(&mut s, 316.0, 170.0, 14.0, PIXEL, false, &t.muted, "start", "ABOUT ME");
        let bio = if self.bio.trim().is_empty() {
            "No bio yet. Set one with /bio."
        } else {
            self.bio
        };
        for (i, line) in wrap(bio, bio_chars, 3).iter().enumerate() {
            text(
                &mut s,
                316.0,
                200.0 + i as f32 * 28.0,
                21.0,
                SANS,
                false,
                &t.ink,
                "start",
                &escape(line),
            );
        }

        if let Some(art) = &self.character {
            // Feet on the bottom edge of the about-me panel, head up into the title row.
            let (cx, feet) = (862.0, 262.0);
            let _ = write!(
                s,
                r##"<ellipse cx="{cx}" cy="{feet}" rx="{}" ry="9" fill="#000000" fill-opacity="0.3"/><image x="{}" y="{}" width="{}" height="{}" href="{}"/>"##,
                (art.w as f32 / 2.5).max(30.0),
                cx - art.w as f32 / 2.0,
                feet - art.h as f32,
                art.w,
                art.h,
                data_uri_png(art.png)
            );
        }

        let server_rank = self
            .server_rank
            .map_or_else(|| "-".to_owned(), |r| format!("#{}", thousands(r)));
        let stats = [
            ("SHELLS", compact(self.credits), Some((SHELL, t.gold.as_str()))),
            ("GLOBAL", format!("#{}", compact(self.global_rank)), None),
            ("SERVER", server_rank, None),
        ];
        for (i, (label, value, icon)) in stats.iter().enumerate() {
            let x = 296.0 + i as f32 * 216.0;
            panel(&mut s, x, 288.0, 192.0, 96.0, 4.0, &t.panel, &t.panel_edge);
            text(&mut s, x + 18.0, 318.0, 14.0, PIXEL, false, &t.muted, "start", label);
            text(
                &mut s,
                x + 18.0,
                362.0,
                30.0,
                PIXEL,
                false,
                &t.ink,
                "start",
                &escape(value),
            );
            if let Some((rows, color)) = icon {
                sprite(&mut s, rows, x + 142.0, 304.0, 4.0, color, "#fff7d6");
            }
        }

        text(
            &mut s,
            296.0,
            424.0,
            16.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            "EXPERIENCE",
        );
        let xp_label = format!(
            "{} / {} XP",
            thousands(self.xp_current as i64),
            thousands(self.xp_needed as i64)
        );
        text(&mut s, 920.0, 424.0, 16.0, PIXEL, false, &t.ink, "end", &xp_label);
        let fraction = self.xp_current as f32 / self.xp_needed.max(1) as f32;
        segment_bar(
            &mut s,
            296.0,
            436.0,
            624.0,
            44.0,
            fraction,
            24,
            &t.accent,
            "#1f2a57",
            &t.panel_edge,
        );
        // Seven digits would run into the badges.
        let total = if self.total_xp >= 1_000_000 {
            format!("TOTAL {} XP", compact(self.total_xp))
        } else {
            format!("TOTAL {} XP", thousands(self.total_xp))
        };
        text(&mut s, 296.0, 512.0, 14.0, PIXEL, false, &t.muted, "start", &total);

        // Badges run along the footer between the total and the crab, each on its rarity plate.
        for (i, badge) in self.badges.iter().take(Self::MAX_BADGES).enumerate() {
            badge.draw(&mut s, 450.0 + i as f32 * (BadgeIcon::PLATE + 3.0), 482.0);
        }

        sprite(&mut s, CRAB, 804.0, 498.0, 3.0, &t.accent, "#fff1e8");
        text(&mut s, 920.0, 516.0, 14.0, PIXEL, false, &t.muted, "end", "PINCHORA");
        s.push_str("</svg>");
        s
    }
}
