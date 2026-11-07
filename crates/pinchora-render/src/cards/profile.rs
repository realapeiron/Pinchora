use std::fmt::Write;

use pinchora_core::{i18n::Lang, t};

use super::{
    Backdrop, BadgeIcon, PIXEL, SANS, ShowcaseSlot, SpriteArt, avatar_frame, backdrop, showcase::draw_row, text,
};
use crate::{
    Theme,
    svg::{
        CRAB, HEART, SHELL, STAR, background, compact_in, data_uri_png, escape, fit_laid_out, pixel_shadow,
        pixel_width, segment_bar, sprite, thousands_in, truncate, truncate_pixel_to, truncate_to, wrap,
    },
};

/// 960×540 profile card.
pub struct ProfileCard<'a> {
    /// The language of the labels.
    pub lang: Lang,
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
    /// Up to [`ShowcaseSlot::MAX`] tiles in a row under the about-me panel. With none the card
    /// keeps the layout it had before the showcase existed; with some, the about-me panel and
    /// the stats give up height to make room.
    pub showcase: &'a [ShowcaseSlot<'a>],
    /// Who the player is with, drawn as "♥ name" in the corner of the about-me panel.
    pub partner: Option<String>,
    pub theme: Theme,
}

/// Where the right-hand side of the card keeps its parts. The classic rows are the card as it
/// was before the showcase; the showcase rows squeeze the about-me panel and the stats to fit a
/// row of tiles. Both end clear of a frame's band at the bottom edge.
pub(super) struct Rows {
    bio_h: f32,
    bio_label_dy: f32,
    bio_first_dy: f32,
    bio_leading: f32,
    bio_size: f32,
    /// Top of the showcase row, if there is one.
    show_y: Option<f32>,
    stats_y: f32,
    stats_h: f32,
    stats_label_dy: f32,
    stats_label_size: f32,
    stats_value_dy: f32,
    stats_value_size: f32,
    stats_icon_dx: f32,
    stats_icon_dy: f32,
    stats_icon_scale: f32,
    xp_y: f32,
    bar_y: f32,
    bar_h: f32,
    /// Top of the badge plates; the total, the crab and the name hang off it.
    pub(super) foot_y: f32,
    /// The plate behind the footer text on a background picture: top and height.
    pub(super) plate: (f32, f32),
}

impl Rows {
    /// The card as it always was; `lift` raises the footer for a frame's band.
    pub(super) fn classic(lift: f32) -> Self {
        Self {
            bio_h: 132.0,
            bio_label_dy: 30.0,
            bio_first_dy: 60.0,
            bio_leading: 28.0,
            bio_size: 21.0,
            show_y: None,
            stats_y: 288.0,
            stats_h: 96.0,
            stats_label_dy: 30.0,
            stats_label_size: 14.0,
            stats_value_dy: 74.0,
            stats_value_size: 30.0,
            stats_icon_dx: 142.0,
            stats_icon_dy: 16.0,
            stats_icon_scale: 4.0,
            xp_y: 424.0 - lift,
            bar_y: 436.0 - lift,
            bar_h: 44.0,
            foot_y: 482.0 - lift,
            plate: (400.0 - lift, 136.0),
        }
    }

    pub(super) fn showcase() -> Self {
        Self {
            bio_h: 108.0,
            bio_label_dy: 22.0,
            bio_first_dy: 46.0,
            bio_leading: 24.0,
            bio_size: 20.0,
            show_y: Some(254.0),
            stats_y: 374.0,
            stats_h: 48.0,
            // A label and a value with the same room round them: 6 px above, 6 between, 5 below.
            stats_label_dy: 18.0,
            stats_label_size: 12.0,
            stats_value_dy: 39.0,
            stats_value_size: 24.0,
            stats_icon_dx: 150.0,
            stats_icon_dy: 15.0,
            stats_icon_scale: 3.0,
            xp_y: 438.0,
            bar_y: 444.0,
            bar_h: 28.0,
            foot_y: 476.0,
            plate: (422.0, 108.0),
        }
    }
}

impl ProfileCard<'_> {
    pub const AVATAR_SIZE: u32 = 208;
    pub const CHARACTER_SCALE: u32 = 2;
    pub const MAX_BADGES: usize = 6;
    /// Letters of a partner's name that fit the corner of the about-me panel.
    pub const PARTNER_CHARS: usize = 18;
    /// The room a partner's name has there, in pixels as drawn: the names that are wider, wide
    /// letters or letters the pixel font lacks, are cut to this.
    pub const PARTNER_WIDTH: f32 = 224.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (960.0, 540.0);
        let rows = if self.showcase.is_empty() {
            // A frame's band takes the last few pixels of the footer's room.
            Rows::classic(if t.frame == crate::Frame::Plain { 0.0 } else { 6.0 })
        } else {
            Rows::showcase()
        };
        let mut s = String::with_capacity(64 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        match &self.backdrop {
            Some(b) => {
                backdrop(&mut s, w, h, b, t.opacity.scrim());
                // Text that is not inside a panel gets a plate of its own.
                if self.title.is_some() {
                    t.plate(&mut s, 288.0, 40.0, 640.0, 96.0);
                } else {
                    t.plate(&mut s, 288.0, 44.0, 640.0, 88.0);
                }
                t.plate(&mut s, 288.0, rows.plate.0, 640.0, rows.plate.1);
                // The footer's small text and busy art get a second plate of their own.
                let footer = rows.foot_y - 4.0;
                t.footer_plate(&mut s, 288.0, footer, 640.0, rows.plate.0 + rows.plate.1 - footer);
            }
            None => background(&mut s, w, h, &t.bg_top, &t.bg_bottom),
        }

        // Left column: avatar, level, rep, streak.
        avatar_frame(&mut s, self.avatar_png, 40.0, 40.0, 224.0, 8.0, &t.accent, "avatar");

        t.panel(&mut s, 40.0, 280.0, 224.0, 88.0, 4.0);
        text(
            &mut s,
            60.0,
            312.0,
            18.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &t!(self.lang, "card.level"),
        );
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

        t.panel(&mut s, 40.0, 384.0, 224.0, 56.0, 4.0);
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
            &t!(
                self.lang,
                "card.rep",
                count = compact_in(self.lang, i64::from(self.reputation))
            ),
        );

        t.panel(&mut s, 40.0, 456.0, 224.0, 56.0, 4.0);
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
            &t!(self.lang, "card.days", count = self.streak.max(0)),
        );

        // Right column: name, bio, stats, XP. With a character, text keeps clear of its corner.
        let (name_room, bio_chars, bio_room) = if self.character.is_some() {
            (516.0, 38, 490.0)
        } else {
            (620.0, 46, 584.0)
        };
        // Cut to the pixels the bold name takes, not to a number of letters: wide letters and
        // the scripts the fonts lack are not as wide as an average one.
        let name = escape(&fit_laid_out(
            &truncate(self.display_name, 32),
            SANS,
            true,
            44.0,
            name_room,
        ));
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
                    &escape(&truncate(&self.lang.upper(title), 24)),
                );
                text(&mut s, 298.0, 130.0, 20.0, SANS, false, &t.muted, "start", &handle);
            }
            None => {
                text(&mut s, 296.0, 88.0, 44.0, SANS, true, &t.ink, "start", &name);
                text(&mut s, 298.0, 122.0, 22.0, SANS, false, &t.muted, "start", &handle);
            }
        }

        let bio_top = 140.0;
        t.panel(&mut s, 296.0, bio_top, 624.0, rows.bio_h, 4.0);
        text(
            &mut s,
            316.0,
            bio_top + rows.bio_label_dy,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &t!(self.lang, "card.about-me"),
        );
        if let Some(partner) = &self.partner {
            // The corner of the label row; a character takes the panel's right edge.
            let right = if self.character.is_some() { 806.0 } else { 900.0 };
            self.partner_line(&mut s, partner, right, bio_top + rows.bio_label_dy);
        }
        let no_bio = t!(self.lang, "card.no-bio");
        let bio = if self.bio.trim().is_empty() { &no_bio } else { self.bio };
        for (i, line) in wrap(bio, bio_chars, 3).iter().enumerate() {
            text(
                &mut s,
                316.0,
                bio_top + rows.bio_first_dy + i as f32 * rows.bio_leading,
                rows.bio_size,
                SANS,
                false,
                &t.ink,
                "start",
                // A line of wide letters is cut at the panel's edge, or at the character's.
                &escape(&truncate_to(line, rows.bio_size, bio_room)),
            );
        }

        if let Some(art) = &self.character {
            // Feet near the bottom edge of the about-me panel, head up into the title row.
            let (cx, feet) = (862.0, bio_top + rows.bio_h - 10.0);
            pixel_shadow(&mut s, cx, feet, (art.w as f32 / 2.5).max(30.0), 6.0, 0.3);
            let _ = write!(
                s,
                r#"<image x="{}" y="{}" width="{}" height="{}" image-rendering="optimizeSpeed" href="{}"/>"#,
                cx - (art.w / 2) as f32,
                feet - art.h as f32,
                art.w,
                art.h,
                data_uri_png(art.png)
            );
        }

        if let Some(y) = rows.show_y {
            draw_row(&mut s, t, self.lang, self.showcase, 296.0, y, 624.0);
        }

        let server_rank = self
            .server_rank
            .map_or_else(|| "-".to_owned(), |r| format!("#{}", thousands_in(self.lang, r)));
        let stats = [
            (
                t!(self.lang, "card.shells"),
                compact_in(self.lang, self.credits),
                Some((SHELL, t.gold.as_str())),
            ),
            (
                t!(self.lang, "card.global"),
                format!("#{}", compact_in(self.lang, self.global_rank)),
                None,
            ),
            (t!(self.lang, "card.server"), server_rank, None),
        ];
        for (i, (label, value, icon)) in stats.iter().enumerate() {
            let x = 296.0 + i as f32 * 216.0;
            t.panel(&mut s, x, rows.stats_y, 192.0, rows.stats_h, 4.0);
            text(
                &mut s,
                x + 18.0,
                rows.stats_y + rows.stats_label_dy,
                rows.stats_label_size,
                PIXEL,
                false,
                &t.muted,
                "start",
                label,
            );
            text(
                &mut s,
                x + 18.0,
                rows.stats_y + rows.stats_value_dy,
                rows.stats_value_size,
                PIXEL,
                false,
                &t.ink,
                "start",
                &escape(value),
            );
            if let Some((sprite_rows, color)) = icon {
                sprite(
                    &mut s,
                    sprite_rows,
                    x + rows.stats_icon_dx,
                    rows.stats_y + rows.stats_icon_dy,
                    rows.stats_icon_scale,
                    color,
                    "#fff7d6",
                );
            }
        }

        text(
            &mut s,
            296.0,
            rows.xp_y,
            16.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &t!(self.lang, "card.experience"),
        );
        let xp_label = format!(
            "{} / {} XP",
            thousands_in(self.lang, self.xp_current as i64),
            thousands_in(self.lang, self.xp_needed as i64)
        );
        text(&mut s, 920.0, rows.xp_y, 16.0, PIXEL, false, &t.ink, "end", &xp_label);
        let fraction = self.xp_current as f32 / self.xp_needed.max(1) as f32;
        segment_bar(
            &mut s,
            296.0,
            rows.bar_y,
            624.0,
            rows.bar_h,
            fraction,
            24,
            &t.accent,
            "#1f2a57",
            &t.panel_edge,
        );
        // Seven digits would run into the badges.
        let total = if self.total_xp >= 1_000_000 {
            t!(self.lang, "card.total-xp", xp = compact_in(self.lang, self.total_xp))
        } else {
            t!(self.lang, "card.total-xp", xp = thousands_in(self.lang, self.total_xp))
        };
        text(
            &mut s,
            296.0,
            rows.foot_y + 30.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &total,
        );

        // Badges run along the footer between the total and the crab, each on its rarity plate.
        for (i, badge) in self.badges.iter().take(Self::MAX_BADGES).enumerate() {
            badge.draw(&mut s, 450.0 + i as f32 * (BadgeIcon::PLATE + 3.0), rows.foot_y);
        }

        sprite(&mut s, CRAB, 804.0, rows.foot_y + 16.0, 3.0, &t.accent, "#fff1e8");
        text(
            &mut s,
            920.0,
            rows.foot_y + 34.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            "PINCHORA",
        );
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }

    /// "♥ name", its right end at `right`, on the text baseline `y`. The name is measured as it
    /// is drawn, so a name with accents, another script or emoji keeps the heart next to it, and
    /// one that does not fit [`ProfileCard::PARTNER_WIDTH`] is cut.
    fn partner_line(&self, s: &mut String, partner: &str, right: f32, y: f32) {
        let t = &self.theme;
        let name = truncate_pixel_to(
            &truncate(&self.lang.upper(partner), Self::PARTNER_CHARS),
            14.0,
            Self::PARTNER_WIDTH,
        );
        let width = pixel_width(&name, 14.0);
        text(s, right, y, 14.0, PIXEL, false, &t.ink, "end", &escape(&name));
        // On a whole pixel, so the heart's blocks are not blended.
        let heart = (right - width - 31.0).floor();
        sprite(s, HEART, heart, y - 12.0, 3.0, &t.heart, "#ffc2cf");
    }
}
