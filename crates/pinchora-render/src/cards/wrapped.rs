use std::fmt::Write;

use super::{Backdrop, PIXEL, SANS, SpriteArt, avatar_frame, backdrop, text};
use crate::{
    Theme,
    svg::{CRAB, HEART, SHELL, STAR, background, compact, data_uri_png, escape, sprite, thousands, truncate},
};

/// A speech bubble: messages.
const BUBBLE: &[&str] = &[".#####.", "#######", "#o#o#o#", "#######", ".#####.", "##....."];
/// A fish facing left: fish caught.
const FISH: &[&str] = &["..###...#", ".#o###.##", "#########", ".#####.##", "..###...#"];
/// A cottage: houses visited.
const HOUSE: &[&str] = &["...#...", "..###..", ".#####.", "#######", ".##o##.", ".##o##."];
/// An empty heart: a like given. The full [`HEART`] is the one received.
const HEART_OUTLINE: &[&str] = &[".##.##.", "#..#..#", "#.....#", ".#...#.", "..#.#..", "...#..."];

/// What a year came to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WrappedTotals {
    pub messages: i64,
    pub xp: i64,
    pub shells: i64,
    pub fish: i64,
    pub visits: i64,
    pub likes_given: i64,
    pub likes_received: i64,
    pub pet_care: i64,
}

/// The fish they caught most.
pub struct WrappedFish<'a> {
    pub name: &'a str,
    pub caught: i64,
    /// A sprite no larger than 128x72 (as a showcase tile has it).
    pub art: Option<SpriteArt<'a>>,
}

/// The house they went to most.
pub struct WrappedHouse<'a> {
    pub name: &'a str,
    pub visits: i64,
    /// Whether the owner is one of their friends.
    pub friend: bool,
}

/// 640×1164 Wrapped card: the year in numbers.
pub struct WrappedCard<'a> {
    pub year: i32,
    pub display_name: &'a str,
    /// PNG, 128×128.
    pub avatar_png: &'a [u8],
    /// The line under the name, e.g. `Tide Whisperer`.
    pub title: &'a str,
    pub totals: WrappedTotals,
    /// How much they did in each month of the year (January first): the bars.
    pub months: [i64; 12],
    pub fish: Option<WrappedFish<'a>>,
    pub house: Option<WrappedHouse<'a>>,
    pub backdrop: Option<Backdrop<'a>>,
    pub theme: Theme,
}

const MONTHS: [&str; 12] = [
    "JANUARY",
    "FEBRUARY",
    "MARCH",
    "APRIL",
    "MAY",
    "JUNE",
    "JULY",
    "AUGUST",
    "SEPTEMBER",
    "OCTOBER",
    "NOVEMBER",
    "DECEMBER",
];

impl WrappedCard<'_> {
    pub const AVATAR_SIZE: u32 = 128;
    pub const WIDTH: f32 = 640.0;
    pub const HEIGHT: f32 = 1164.0;

    /// The busiest month (1-12) and what it held; the latest wins a tie. `None` for a year with nothing.
    pub fn top_month(months: &[i64; 12]) -> Option<(usize, i64)> {
        months
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0)
            .max_by_key(|(i, n)| (**n, *i))
            .map(|(i, n)| (i + 1, *n))
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH, Self::HEIGHT);
        let mut s = String::with_capacity(64 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        match &self.backdrop {
            Some(b) => backdrop(&mut s, w, h, b, t.opacity.scrim()),
            None => background(&mut s, w, h, &t.bg_top, &t.bg_bottom),
        }

        // The heading: the year in big letters.
        if self.backdrop.is_some() {
            t.plate(&mut s, 160.0, 24.0, 320.0, 112.0);
        }
        text(
            &mut s,
            w / 2.0,
            58.0,
            16.0,
            PIXEL,
            false,
            &t.muted,
            "middle",
            "PINCHORA WRAPPED",
        );
        text(
            &mut s,
            w / 2.0,
            122.0,
            68.0,
            PIXEL,
            false,
            &t.accent,
            "middle",
            &self.year.to_string(),
        );
        for (x, y, scale) in [(56.0, 44.0, 4.0), (548.0, 70.0, 3.0), (96.0, 96.0, 2.4)] {
            sprite(&mut s, STAR, x, y, scale, &t.gold, "#fff7d6");
        }

        // Who it is about.
        t.panel(&mut s, 20.0, 150.0, 600.0, 152.0, 4.0);
        avatar_frame(&mut s, self.avatar_png, 40.0, 164.0, 124.0, 6.0, &t.gold, "avatar");
        text(
            &mut s,
            184.0,
            212.0,
            30.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&truncate(self.display_name, 18)),
        );
        sprite(&mut s, STAR, 186.0, 232.0, 2.4, &t.gold, "#fff7d6");
        text(
            &mut s,
            210.0,
            246.0,
            16.0,
            PIXEL,
            false,
            &t.gold,
            "start",
            &escape(&truncate(&self.title.to_uppercase(), 22)),
        );
        text(
            &mut s,
            186.0,
            280.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            "YOUR YEAR ON THE REEF",
        );

        // The busiest month, with a bar for each month under it.
        t.panel(&mut s, 20.0, 316.0, 600.0, 218.0, 4.0);
        text(
            &mut s,
            44.0,
            350.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            "BUSIEST MONTH",
        );
        match Self::top_month(&self.months) {
            Some((month, n)) => {
                text(
                    &mut s,
                    44.0,
                    398.0,
                    38.0,
                    PIXEL,
                    false,
                    &t.accent,
                    "start",
                    MONTHS[month - 1],
                );
                text(
                    &mut s,
                    596.0,
                    398.0,
                    15.0,
                    PIXEL,
                    false,
                    &t.ink,
                    "end",
                    &format!("{} THINGS DONE", compact(n)),
                );
            }
            None => text(&mut s, 44.0, 398.0, 28.0, PIXEL, false, &t.ink, "start", "A QUIET YEAR"),
        }
        let top = Self::top_month(&self.months).map(|(m, _)| m);
        let tallest = self.months.iter().copied().max().unwrap_or(0).max(1) as f32;
        let (base, chart_h) = (506.0, 70.0);
        for (i, n) in self.months.iter().enumerate() {
            let x = 44.0 + i as f32 * 46.0;
            // A month with anything shows at least a step; the busiest one reaches the top.
            let bar = if *n > 0 {
                (chart_h * *n as f32 / tallest).round().max(4.0)
            } else {
                0.0
            };
            let best = top == Some(i + 1);
            let color = if best { &t.gold } else { &t.accent };
            let _ = write!(
                s,
                r##"<rect x="{x}" y="{}" width="34" height="{chart_h}" fill="{}" fill-opacity="0.55"/>"##,
                base - chart_h - 16.0,
                t.panel_edge,
            );
            if bar > 0.0 {
                let _ = write!(
                    s,
                    r##"<rect x="{x}" y="{}" width="34" height="{bar}" fill="{color}"/><rect x="{x}" y="{}" width="34" height="2" fill="#ffffff" fill-opacity="0.35"/>"##,
                    base - 16.0 - bar,
                    base - 16.0 - bar,
                );
            }
            let letter = &MONTHS[i][..1];
            text(
                &mut s,
                x + 17.0,
                base,
                12.0,
                PIXEL,
                false,
                if best { &t.gold } else { &t.muted },
                "middle",
                letter,
            );
        }

        // The numbers, two to a row.
        let m = &self.totals;
        // Each number has a picture of its own: (what it counts, how many, the icon and its colours).
        let tiles: [(&str, i64, &[&str], &str, &str); 8] = [
            ("MESSAGES", m.messages, BUBBLE, &t.accent, "#ffffff"),
            ("SERVER XP", m.xp, STAR, &t.gold, "#fff7d6"),
            ("SHELLS EARNED", m.shells, SHELL, &t.gold, "#ffffff"),
            ("FISH CAUGHT", m.fish, FISH, &t.mint, "#ffffff"),
            ("HOUSES VISITED", m.visits, HOUSE, &t.gold, "#ffffff"),
            ("PETS CARED FOR", m.pet_care, CRAB, &t.accent, "#ffffff"),
            ("LIKES GIVEN", m.likes_given, HEART_OUTLINE, &t.heart, "#ffc2cf"),
            ("LIKES RECEIVED", m.likes_received, HEART, &t.heart, "#ffc2cf"),
        ];
        for (i, (label, value, icon, color, highlight)) in tiles.iter().enumerate() {
            let (x, y) = (20.0 + (i % 2) as f32 * 308.0, 548.0 + (i / 2) as f32 * 88.0);
            t.panel(&mut s, x, y, 292.0, 76.0, 4.0);
            text(&mut s, x + 18.0, y + 28.0, 12.0, PIXEL, false, &t.muted, "start", label);
            text(
                &mut s,
                x + 18.0,
                y + 62.0,
                28.0,
                PIXEL,
                false,
                &t.ink,
                "start",
                &escape(&compact(*value)),
            );
            // Centred in a 40 x 28 box at the right of the tile, whatever its shape.
            let (cols, rows) = (icon[0].chars().count() as f32, icon.len() as f32);
            sprite(
                &mut s,
                icon,
                x + 266.0 - cols * 2.0,
                y + 38.0 - rows * 2.0,
                4.0,
                color,
                highlight,
            );
        }

        // The favourite fish, with its picture.
        let y = 908.0;
        t.panel(&mut s, 20.0, y, 600.0, 100.0, 4.0);
        text(
            &mut s,
            44.0,
            y + 32.0,
            13.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            "FAVOURITE CATCH",
        );
        match &self.fish {
            Some(fish) => {
                text(
                    &mut s,
                    44.0,
                    y + 66.0,
                    22.0,
                    SANS,
                    true,
                    &t.ink,
                    "start",
                    &escape(&truncate(fish.name, 22)),
                );
                text(
                    &mut s,
                    44.0,
                    y + 88.0,
                    14.0,
                    SANS,
                    false,
                    &t.muted,
                    "start",
                    &format!(
                        "caught {} time{}",
                        thousands(fish.caught),
                        if fish.caught == 1 { "" } else { "s" }
                    ),
                );
                if let Some(art) = &fish.art {
                    // Centred in the box on the right; a sprite never grows here.
                    let (box_w, box_h) = (140.0, 76.0);
                    let (aw, ah) = (art.w as f32, art.h as f32);
                    let k = (box_w / aw).min(box_h / ah).min(1.0);
                    let (aw, ah) = (aw * k, ah * k);
                    let _ = write!(
                        s,
                        r#"<image x="{}" y="{}" width="{aw}" height="{ah}" image-rendering="optimizeSpeed" href="{}"/>"#,
                        596.0 - box_w / 2.0 - aw / 2.0,
                        y + 50.0 - ah / 2.0,
                        data_uri_png(art.png)
                    );
                }
            }
            None => text(
                &mut s,
                44.0,
                y + 70.0,
                20.0,
                PIXEL,
                false,
                &t.ink,
                "start",
                "NO FISH YET",
            ),
        }

        // The house they visited most.
        let y = 1020.0;
        t.panel(&mut s, 20.0, y, 600.0, 98.0, 4.0);
        let label = match &self.house {
            Some(house) if house.friend => "MOST VISITED FRIEND'S HOUSE",
            _ => "MOST VISITED HOUSE",
        };
        text(&mut s, 44.0, y + 30.0, 13.0, PIXEL, false, &t.muted, "start", label);
        match &self.house {
            Some(house) => {
                text(
                    &mut s,
                    44.0,
                    y + 60.0,
                    22.0,
                    SANS,
                    true,
                    &t.ink,
                    "start",
                    &escape(&truncate(house.name, 22)),
                );
                text(
                    &mut s,
                    44.0,
                    y + 82.0,
                    14.0,
                    SANS,
                    false,
                    &t.muted,
                    "start",
                    &format!(
                        "{} visit{}",
                        thousands(house.visits),
                        if house.visits == 1 { "" } else { "s" }
                    ),
                );
                sprite(&mut s, HEART, 548.0, y + 34.0, 6.0, &t.heart, "#ffc2cf");
            }
            None => text(
                &mut s,
                44.0,
                y + 64.0,
                20.0,
                PIXEL,
                false,
                &t.ink,
                "start",
                "NO VISITS YET",
            ),
        }
        text(
            &mut s,
            w / 2.0,
            1146.0,
            12.0,
            PIXEL,
            false,
            &t.muted,
            "middle",
            "/WRAPPED · SEE YOU NEXT YEAR",
        );
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}
