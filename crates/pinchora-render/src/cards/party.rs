//! The party game cards: the hermit crab derby lobby, share or grab, the shell game, the treasure heist
//! and the storm surge.

use std::fmt::Write;

use super::{PIXEL, SANS, SpriteArt, text};
use crate::{
    Theme,
    svg::{
        CRAB, HEART, SHELL, STAR, background, compact, data_uri_png, escape, mix, notched_path, panel, pixel_width,
        segment_bar, sprite, thousands, truncate_to, wrap,
    },
};

fn open(w: f32, h: f32) -> String {
    let mut s = String::with_capacity(40 * 1024);
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
    );
    s
}

/// A small plate with a pixel-font label, whose right edge is at `right`.
fn chip(s: &mut String, right: f32, y: f32, label: &str, color: &str, theme: &Theme) {
    let w = pixel_width(label, 16.0) + 24.0;
    panel(s, right - w, y, w, 32.0, 4.0, &theme.panel, color);
    text(s, right - w / 2.0, y + 23.0, 16.0, PIXEL, false, color, "middle", label);
}

/// A sprite from art already scaled to the size it is drawn at.
fn art(s: &mut String, art: &SpriteArt<'_>, x: f32, y: f32) {
    let _ = write!(
        s,
        r#"<image x="{x}" y="{y}" width="{}" height="{}" image-rendering="optimizeSpeed" href="{}"/>"#,
        art.w,
        art.h,
        data_uri_png(art.png)
    );
}

fn hex(color: u32) -> String {
    format!("#{:06x}", color & 0xff_ffff)
}

// /Derby

/// One lane of the derby lobby.
pub struct DerbyLane<'a> {
    pub name: &'a str,
    /// The lane colour, `0xrrggbb`.
    pub accent: u32,
    /// The crab, already scaled 2×.
    pub art: SpriteArt<'a>,
    /// The names of the players who backed it, in the order they joined.
    pub backers: &'a [String],
}

/// 600×470 derby lobby: the six crabs on their lanes, who backs each, and what is at stake.
pub struct DerbyCard<'a> {
    /// The six crabs, in lane order.
    pub lanes: &'a [DerbyLane<'a>],
    /// Shells each racer put up; 0 for a friendly race.
    pub stake: i64,
    pub racers: usize,
    /// What the card says it is: `PICK YOUR CRAB`, `THEY ARE OFF`.
    pub status: &'a str,
    pub theme: Theme,
}

impl DerbyCard<'_> {
    pub const WIDTH: f32 = 600.0;
    pub const HEIGHT: f32 = 470.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH, Self::HEIGHT);
        let mut s = open(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(
            &mut s,
            28.0,
            52.0,
            24.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            "HERMIT CRAB DERBY",
        );
        if self.stake > 0 {
            let pool = self.stake * self.racers as i64;
            chip(&mut s, w - 28.0, 26.0, &format!("POOL {}", compact(pool)), &t.gold, t);
            sprite(&mut s, SHELL, 28.0, 68.0, 3.0, &t.gold, "#ffffff");
            text(
                &mut s,
                56.0,
                86.0,
                16.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &format!("ENTRY {} EACH, WINNERS SHARE THE POOL", compact(self.stake)),
            );
        } else {
            chip(&mut s, w - 28.0, 26.0, "FOR FUN", &t.mint, t);
            text(
                &mut s,
                28.0,
                86.0,
                16.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                "NO STAKES, JUST BRAGGING RIGHTS",
            );
        }
        for (n, lane) in self.lanes.iter().take(6).enumerate() {
            let (col, row) = ((n % 2) as f32, (n / 2) as f32);
            let (x, y) = (24.0 + col * 280.0, 104.0 + row * 112.0);
            let accent = hex(lane.accent);
            panel(
                &mut s,
                x,
                y,
                272.0,
                104.0,
                4.0,
                &t.panel,
                &mix(&accent, &t.panel_edge, 0.45),
            );
            let _ = write!(
                s,
                r#"<path d="{}" fill="{accent}"/>"#,
                notched_path(x + 6.0, y + 6.0, 10.0, 92.0, 2.0)
            );
            art(&mut s, &lane.art, x + 26.0, y + 18.0);
            text(
                &mut s,
                x + 112.0,
                y + 36.0,
                16.0,
                PIXEL,
                false,
                &accent,
                "start",
                &format!("LANE {}", n + 1),
            );
            text(
                &mut s,
                x + 112.0,
                y + 62.0,
                22.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(lane.name, 22.0, 150.0)),
            );
            let backers = lane.backers.len();
            let line = match backers {
                0 => "NO BACKERS".to_owned(),
                1 => "1 BACKER".to_owned(),
                n => format!("{n} BACKERS"),
            };
            let tone = if backers == 0 { &t.muted } else { &t.mint };
            text(&mut s, x + 112.0, y + 82.0, 14.0, PIXEL, false, tone, "start", &line);
            if let Some(first) = lane.backers.first() {
                let more = if backers > 1 {
                    format!(" +{}", backers - 1)
                } else {
                    String::new()
                };
                let shown = truncate_to(first, 13.0, 140.0 - 8.0 * more.len() as f32);
                text(
                    &mut s,
                    x + 112.0,
                    y + 98.0,
                    13.0,
                    SANS,
                    false,
                    &t.muted,
                    "start",
                    &escape(&format!("{shown}{more}")),
                );
            }
        }
        panel(&mut s, 24.0, 432.0, 552.0, 28.0, 4.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            w / 2.0,
            452.0,
            14.0,
            PIXEL,
            false,
            &t.gold,
            "middle",
            &escape(&self.status.to_uppercase()),
        );
        s.push_str("</svg>");
        s
    }
}

// /Share or grab

/// What one player has done, as the card shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickView {
    Thinking,
    /// Picked, and nobody knows what.
    Locked,
    Share,
    Grab,
}

/// How the banner under the picture reads: waiting, good news or bad news.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Wait,
    Good,
    Bad,
}

/// A treasure chest, drawn in two layers: the wood, then the gold trim and lock.
const CHEST_WOOD: &[&str] = &[
    "...############...",
    "..#oooooooooooo#..",
    ".#oooooooooooooo#.",
    ".################.",
    ".#oooooooooooooo#.",
    ".#oooooooooooooo#.",
    ".#oooooooooooooo#.",
    ".#oooooooooooooo#.",
    ".################.",
];
const CHEST_TRIM: &[&str] = &[
    "..................",
    "....#........#....",
    "....#........#....",
    "....#...##...#....",
    "....#...oo...#....",
    "....#...##...#....",
    "....#........#....",
    "....#........#....",
    "..................",
];

/// 580×350 share-or-grab card: both players, the pool between them, and what they picked.
pub struct ShareGrabCard<'a> {
    pub names: [&'a str; 2],
    pub picks: [PickView; 2],
    /// Shells in the pool.
    pub pool: i64,
    /// Whether the pool is the players' own stakes rather than the sea's.
    pub wager: bool,
    pub banner: &'a str,
    pub mood: Mood,
    pub theme: Theme,
}

impl ShareGrabCard<'_> {
    pub const WIDTH: f32 = 580.0;
    pub const HEIGHT: f32 = 350.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH, Self::HEIGHT);
        let mut s = open(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(&mut s, 28.0, 52.0, 24.0, PIXEL, false, &t.ink, "start", "SHARE OR GRAB");
        chip(
            &mut s,
            w - 28.0,
            26.0,
            if self.wager { "YOUR STAKES" } else { "THE SEA'S POOL" },
            if self.wager { &t.gold } else { &t.mint },
            t,
        );
        // The pool in the middle: a chest, and what is in it.
        panel(&mut s, 210.0, 84.0, 160.0, 168.0, 4.0, &t.panel, &t.gold);
        sprite(&mut s, CHEST_WOOD, 238.0, 102.0, 6.0, "#7a4a2a", "#b97f4a");
        sprite(&mut s, CHEST_TRIM, 238.0, 102.0, 6.0, "#ffd166", "#fff1e8");
        text(
            &mut s,
            290.0,
            198.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "middle",
            "IN THE POOL",
        );
        let pool = compact(self.pool);
        let group = 21.0 + 14.0 + pixel_width(&pool, 24.0);
        sprite(&mut s, SHELL, 290.0 - group / 2.0, 212.0, 3.0, &t.gold, "#ffffff");
        text(
            &mut s,
            290.0 + group / 2.0,
            232.0,
            24.0,
            PIXEL,
            false,
            &t.gold,
            "end",
            &pool,
        );
        for (i, (name, pick)) in self.names.iter().zip(self.picks).enumerate() {
            let x = if i == 0 { 28.0 } else { 392.0 };
            let (edge, label, tone) = match pick {
                PickView::Thinking => (&t.panel_edge, "THINKING...", &t.muted),
                PickView::Locked => (&t.mint, "LOCKED IN", &t.mint),
                PickView::Share => (&t.mint, "SHARE", &t.mint),
                PickView::Grab => (&t.accent, "GRAB", &t.accent),
            };
            panel(&mut s, x, 84.0, 160.0, 168.0, 4.0, &t.panel, edge);
            text(
                &mut s,
                x + 80.0,
                116.0,
                18.0,
                SANS,
                true,
                &t.ink,
                "middle",
                &escape(&truncate_to(name, 18.0, 140.0)),
            );
            let icon = match pick {
                PickView::Share => Some((HEART, &t.mint)),
                PickView::Grab => Some((CRAB, &t.accent)),
                _ => None,
            };
            match icon {
                Some((rows, color)) => {
                    let scale = 10.0;
                    let iw = rows[0].len() as f32 * scale;
                    sprite(&mut s, rows, x + 80.0 - iw / 2.0, 138.0, scale, color, "#fff1e8");
                }
                None => {
                    // A closed fist of a question mark for a pick nobody has seen.
                    let mark = if pick == PickView::Locked { "?" } else { "..." };
                    text(&mut s, x + 80.0, 188.0, 40.0, PIXEL, false, tone, "middle", mark);
                }
            }
            text(&mut s, x + 80.0, 236.0, 16.0, PIXEL, false, tone, "middle", label);
        }
        let (edge, ink) = match self.mood {
            Mood::Wait => (&t.panel_edge, &t.muted),
            Mood::Good => (&t.mint, &t.mint),
            Mood::Bad => (&t.accent, &t.accent),
        };
        panel(&mut s, 28.0, 272.0, 524.0, 54.0, 4.0, &t.panel, edge);
        text(
            &mut s,
            290.0,
            306.0,
            16.0,
            PIXEL,
            false,
            ink,
            "middle",
            &escape(&self.banner.to_uppercase()),
        );
        s.push_str("</svg>");
        s
    }
}

// /Shell game

/// A shell lying on the sand, one colour and a lighter one, 14 wide.
const CUP: &[&str] = &[
    "....######....",
    "..##oooooo##..",
    ".#oo#o##o#oo#.",
    ".#o#oo##oo#o#.",
    "#oo#oo##oo#oo#",
    "#o#ooo##ooo#o#",
    "##############",
];

/// A pearl.
const PEARL: &[&str] = &["..###..", ".#ooo#.", "#ooooo#", "#ooooo#", ".#ooo#.", "..###.."];

/// 520×320 shell game: three shells on a table, one with the pearl under it.
pub struct ShellCard<'a> {
    pub stake: i64,
    /// The shell the player picked.
    pub picked: Option<usize>,
    /// Where the pearl was, once it is shown.
    pub pearl: Option<usize>,
    pub status: &'a str,
    /// Shells paid for a right guess, shown when the game is over.
    pub payout: i64,
    pub theme: Theme,
}

impl ShellCard<'_> {
    pub const WIDTH: f32 = 520.0;
    pub const HEIGHT: f32 = 320.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH, Self::HEIGHT);
        let mut s = open(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(&mut s, 28.0, 52.0, 24.0, PIXEL, false, &t.ink, "start", "SHELL GAME");
        let label = format!("STAKE {} PAYS {}", compact(self.stake), compact(self.payout.max(0)));
        chip(&mut s, w - 28.0, 26.0, &label, &t.gold, t);
        // The table.
        panel(&mut s, 24.0, 84.0, 472.0, 150.0, 4.0, "#8a5a34", "#5b3a20");
        for plank in 0..4 {
            let y = 92.0 + plank as f32 * 36.0;
            let _ = write!(
                s,
                r##"<rect x="32" y="{y}" width="456" height="3" fill="#6f4627"/><rect x="32" y="{}" width="456" height="3" fill="#a06b3f"/>"##,
                y + 3.0
            );
        }
        for shell in 0..3usize {
            let x = 52.0 + shell as f32 * 150.0;
            let lifted = self.pearl.is_some();
            let mine = self.picked == Some(shell);
            let (body, light) = if mine {
                (t.accent.as_str(), "#ffb9a8")
            } else {
                ("#d8a7c8", "#f6e0ee")
            };
            let y = if lifted { 112.0 } else { 134.0 };
            if self.pearl == Some(shell) {
                // The pearl sits where the shell was.
                sprite(&mut s, PEARL, x + 33.0, 176.0, 6.0, "#9aa6c8", "#fff7e0");
            }
            // The shell shadow.
            let _ = write!(
                s,
                r##"<rect x="{}" y="214" width="{}" height="6" fill="#000000" fill-opacity="0.25"/>"##,
                x + 8.0,
                84.0
            );
            sprite(&mut s, CUP, x, y, 7.0, body, light);
            let number = (shell + 1).to_string();
            let tone = if mine { &t.gold } else { &t.muted };
            text(&mut s, x + 49.0, 262.0, 20.0, PIXEL, false, tone, "middle", &number);
            if mine {
                text(
                    &mut s,
                    x + 49.0,
                    282.0,
                    12.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "middle",
                    "YOUR PICK",
                );
            }
        }
        text(
            &mut s,
            w / 2.0,
            306.0,
            16.0,
            PIXEL,
            false,
            &t.ink,
            "middle",
            &escape(&self.status.to_uppercase()),
        );
        s.push_str("</svg>");
        s
    }
}

// /Heist

/// A member of the heist crew, as the card lists them.
pub struct CrewView<'a> {
    pub name: &'a str,
    /// The role's number: 0 lookout, 1 locksmith, 2 diver.
    pub role: usize,
}

/// One stage of the heist.
pub struct StageView<'a> {
    pub title: &'a str,
    /// The crew's chance, in percent.
    pub chance: u8,
    /// `None` before the heist runs.
    pub passed: Option<bool>,
    pub line: &'a str,
}

/// The crew's roles, as a short word and a pixel icon colour each.
const ROLE_TAGS: [(&str, &str); 3] = [("LOOKOUT", "#8fd3ff"), ("LOCKSMITH", "#ffd166"), ("DIVER", "#06d6a0")];

/// A round vault door in two layers: the steel, then the dial in the middle.
const VAULT_STEEL: &[&str] = &[
    "....#######....",
    "..###ooooo###..",
    ".##ooooooooo##.",
    ".#ooooooooooo#.",
    "##ooooooooooo##",
    "#ooooooooooooo#",
    "#ooooooooooooo#",
    "#ooooooooooooo#",
    "#ooooooooooooo#",
    "#ooooooooooooo#",
    "##ooooooooooo##",
    ".#ooooooooooo#.",
    ".##ooooooooo##.",
    "..###ooooo###..",
    "....#######....",
];
const VAULT_DIAL: &[&str] = &[
    "...............",
    "...............",
    "...............",
    "...o#######o...",
    "....#.....#....",
    "...#...#...#...",
    "...#...#...#...",
    "...#.#####.#...",
    "...#...#...#...",
    "...#...#...#...",
    "....#.....#....",
    "...o#######o...",
    "...............",
    "...............",
    "...............",
];

/// 600×560 heist card: the vault, the three stages with the crew's chance at each, and who is in.
pub struct HeistCard<'a> {
    pub crew: &'a [CrewView<'a>],
    pub stages: [StageView<'a>; 3],
    pub headline: &'a str,
    /// Shells each member takes home; 0 hides it.
    pub each: i64,
    pub theme: Theme,
}

impl HeistCard<'_> {
    pub const WIDTH: f32 = 600.0;
    pub const HEIGHT: f32 = 560.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH, Self::HEIGHT);
        let mut s = open(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(
            &mut s,
            28.0,
            52.0,
            24.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            "TREASURE HEIST",
        );
        let opened = self.stages.iter().all(|stage| stage.passed == Some(true));
        let done = self.stages.iter().all(|stage| stage.passed.is_some());
        chip(
            &mut s,
            w - 28.0,
            26.0,
            self.headline,
            if opened {
                &t.gold
            } else if done {
                &t.accent
            } else {
                &t.mint
            },
            t,
        );
        // The vault: steel, gold once it is open.
        panel(
            &mut s,
            24.0,
            84.0,
            188.0,
            280.0,
            4.0,
            &t.panel,
            if opened { &t.gold } else { &t.panel_edge },
        );
        let (steel, shine, dial) = if opened {
            ("#8a5a10", "#e3a21e", "#fff1e8")
        } else if done {
            ("#2a3566", "#3a4a85", "#8fa0d8")
        } else {
            ("#2e3d75", "#4a5ba3", "#ffd166")
        };
        sprite(&mut s, VAULT_STEEL, 38.0, 124.0, 10.0, steel, shine);
        sprite(&mut s, VAULT_DIAL, 38.0, 124.0, 10.0, dial, "#ffffff");
        text(
            &mut s,
            118.0,
            330.0,
            14.0,
            PIXEL,
            false,
            if opened { &t.gold } else { &t.muted },
            "middle",
            if opened {
                "OPEN!"
            } else if done {
                "STAYS SHUT"
            } else {
                "SEALED"
            },
        );
        if opened {
            // Loot spills out of an open vault.
            sprite(&mut s, STAR, 164.0, 100.0, 4.0, &t.gold, "#fff1e8");
            sprite(&mut s, SHELL, 40.0, 104.0, 3.0, &t.gold, "#ffffff");
            sprite(&mut s, SHELL, 176.0, 336.0, 3.0, &t.gold, "#ffffff");
        }
        // The stages, each with the crew's chance as a bar under its name.
        for (n, stage) in self.stages.iter().enumerate() {
            let y = 84.0 + n as f32 * 96.0;
            let (edge, mark, tone) = match stage.passed {
                Some(true) => (&t.mint, "PASSED", &t.mint),
                Some(false) => (&t.accent, "FAILED", &t.accent),
                None => (&t.panel_edge, "TO COME", &t.muted),
            };
            panel(&mut s, 224.0, y, 352.0, 88.0, 4.0, &t.panel, edge);
            text(
                &mut s,
                240.0,
                y + 22.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &format!("STAGE {}", n + 1),
            );
            text(&mut s, 560.0, y + 22.0, 14.0, PIXEL, false, tone, "end", mark);
            text(
                &mut s,
                240.0,
                y + 46.0,
                18.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(stage.title),
            );
            text(
                &mut s,
                560.0,
                y + 45.0,
                12.0,
                PIXEL,
                false,
                &t.muted,
                "end",
                &format!("{}% CHANCE", stage.chance),
            );
            let _ = write!(
                s,
                r##"<rect x="240" y="{by}" width="320" height="6" fill="#0b1029"/><rect x="240" y="{by}" width="{}" height="6" fill="{tone}"/>"##,
                320.0 * f32::from(stage.chance.min(100)) / 100.0,
                by = y + 52.0,
            );
            for (row, line) in wrap(stage.line, 46, 2).iter().enumerate() {
                text(
                    &mut s,
                    240.0,
                    y + 71.0 + row as f32 * 12.0,
                    12.0,
                    SANS,
                    false,
                    &t.muted,
                    "start",
                    &escape(&truncate_to(line, 12.0, 318.0)),
                );
            }
        }
        // The crew.
        panel(&mut s, 24.0, 378.0, 552.0, 158.0, 4.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            44.0,
            406.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &format!("THE CREW ({})", self.crew.len()),
        );
        if self.each > 0 {
            let label = format!("{} EACH", thousands(self.each));
            sprite(
                &mut s,
                SHELL,
                556.0 - pixel_width(&label, 14.0) - 30.0,
                392.0,
                2.0,
                &t.gold,
                "#ffffff",
            );
            text(&mut s, 556.0, 406.0, 14.0, PIXEL, false, &t.gold, "end", &label);
        }
        for (n, member) in self.crew.iter().take(8).enumerate() {
            let (col, row) = ((n % 2) as f32, (n / 2) as f32);
            let (x, y) = (44.0 + col * 266.0, 418.0 + row * 28.0);
            let (tag, color) = ROLE_TAGS[member.role.min(2)];
            let _ = write!(
                s,
                r#"<path d="{}" fill="{color}"/>"#,
                notched_path(x, y + 2.0, 10.0, 20.0, 2.0)
            );
            text(
                &mut s,
                x + 20.0,
                y + 18.0,
                16.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(member.name, 16.0, 120.0)),
            );
            text(&mut s, x + 250.0, y + 17.0, 12.0, PIXEL, false, color, "end", tag);
        }
        if self.crew.is_empty() {
            text(
                &mut s,
                300.0,
                470.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                "NOBODY HAS JOINED YET",
            );
        }
        s.push_str("</svg>");
        s
    }
}

// /Surge

/// How a storm stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurgeState {
    Rolling,
    Held,
    Broken,
}

/// A sandbag, 8 wide.
const BAG: &[&str] = &["..####..", ".#oooo#.", "#oooooo#", "#o####o#", ".######."];

/// 640×380 storm surge card: the sky, the sea, the sandbag wall and how high it stands against its goal.
pub struct SurgeCard<'a> {
    /// The wall's height in sandbags, and the height it needs.
    pub wall: i64,
    pub goal: i64,
    pub helpers: i64,
    /// The best helpers with their sandbags.
    pub top: &'a [(String, i64)],
    pub state: SurgeState,
    pub theme: Theme,
}

impl SurgeCard<'_> {
    pub const WIDTH: f32 = 640.0;
    pub const HEIGHT: f32 = 380.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH, Self::HEIGHT);
        let mut s = open(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(&mut s, 28.0, 52.0, 24.0, PIXEL, false, &t.ink, "start", "STORM SURGE");
        let (label, tone) = match self.state {
            SurgeState::Rolling => ("HOLD THE WALL", &t.accent),
            SurgeState::Held => ("THE WALL HELD", &t.mint),
            SurgeState::Broken => ("THE SEA WON", &t.gold),
        };
        chip(&mut s, w - 28.0, 26.0, label, tone, t);

        // The scene: sky, clouds, rain and sea behind the wall.
        let (sx, sy, sw, sh) = (24.0, 76.0, 592.0, 190.0);
        let (sky_top, sky_bottom) = match self.state {
            SurgeState::Held => ("#6fb6e8", "#cfeefa"),
            _ => ("#1a2347", "#46579a"),
        };
        let _ = write!(
            s,
            r##"<defs><linearGradient id="storm" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{sky_top}"/><stop offset="1" stop-color="{sky_bottom}"/></linearGradient><clipPath id="scene"><rect x="{sx}" y="{sy}" width="{sw}" height="{sh}"/></clipPath></defs><g clip-path="url(#scene)"><rect x="{sx}" y="{sy}" width="{sw}" height="{sh}" fill="url(#storm)"/>"##
        );
        if self.state == SurgeState::Held {
            let _ = write!(s, r##"<rect x="500" y="92" width="36" height="36" fill="#ffd166"/>"##);
        } else {
            // Cloud banks and rain, in the same places every time.
            for (cx, cy, cw) in [
                (30.0, 80.0, 150.0),
                (170.0, 92.0, 190.0),
                (360.0, 80.0, 170.0),
                (500.0, 94.0, 130.0),
            ] {
                let _ = write!(
                    s,
                    r##"<rect x="{cx}" y="{cy}" width="{cw}" height="22" fill="#2a3566"/><rect x="{}" y="{}" width="{}" height="14" fill="#2a3566"/><rect x="{cx}" y="{}" width="{cw}" height="4" fill="#3a4a85"/>"##,
                    cx + 14.0,
                    cy - 12.0,
                    cw - 40.0,
                    cy + 18.0
                );
            }
            for k in 0..46 {
                let x = 30.0 + (k * 53 % 560) as f32;
                let y = 118.0 + (k * 37 % 80) as f32;
                let _ = write!(
                    s,
                    r##"<rect x="{x}" y="{y}" width="2" height="8" fill="#8fa0d8" fill-opacity="0.8"/>"##
                );
            }
            if self.state == SurgeState::Rolling {
                // A bolt.
                for (bx, by, bw) in [
                    (300.0, 118.0, 10.0),
                    (294.0, 128.0, 10.0),
                    (302.0, 138.0, 8.0),
                    (296.0, 146.0, 8.0),
                ] {
                    let _ = write!(
                        s,
                        r##"<rect x="{bx}" y="{by}" width="{bw}" height="10" fill="#fff7a8"/>"##
                    );
                }
            }
        }
        // The sea: bands of water rising with the danger.
        let sea_top = if self.state == SurgeState::Broken { 190.0 } else { 214.0 };
        let _ = write!(
            s,
            r##"<rect x="{sx}" y="{sea_top}" width="{sw}" height="80" fill="#2c7fb8"/>"##
        );
        for k in 0..30 {
            let x = sx + k as f32 * 20.0;
            let _ = write!(
                s,
                r##"<rect x="{x}" y="{}" width="12" height="4" fill="#e8f7ff"/><rect x="{}" y="{}" width="12" height="4" fill="#3a9bdc"/>"##,
                sea_top - 4.0 + (k % 3) as f32 * 2.0,
                x + 6.0,
                sea_top + 10.0
            );
        }
        // The wall: rows of sandbags, as tall as the wall stands against its goal.
        let fraction = if self.goal > 0 {
            (self.wall as f32 / self.goal as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let rows = (fraction * 6.0).ceil() as i32;
        let (bag_w, bag_h) = (8.0 * 4.0, 5.0 * 4.0);
        for row in 0..rows {
            let y = sy + sh - bag_h * (row as f32 + 1.0);
            let offset = if row % 2 == 0 { 0.0 } else { bag_w / 2.0 };
            let mut x = sx + 120.0 - offset;
            while x < sx + sw - 120.0 {
                let shade = if (x as i32 / 32 + row) % 2 == 0 {
                    ("#c9ab6a", "#e8d3a3")
                } else {
                    ("#b99a5a", "#d9c28c")
                };
                sprite(&mut s, BAG, x, y, 4.0, shade.0, shade.1);
                x += bag_w;
            }
        }
        // Little crabs on the sand either side, working.
        sprite(&mut s, CRAB, sx + 40.0, sy + sh - 28.0, 3.0, &t.accent, "#fff1e8");
        sprite(
            &mut s,
            CRAB,
            sx + sw - 40.0 - 27.0,
            sy + sh - 28.0,
            3.0,
            "#ffd166",
            "#fff1e8",
        );
        s.push_str("</g>");
        let _ = write!(
            s,
            r##"<path d="{}" fill="none" stroke="{}" stroke-width="4"/>"##,
            notched_path(sx - 2.0, sy - 2.0, sw + 4.0, sh + 4.0, 4.0),
            t.panel_edge
        );

        // The bar, and the helpers.
        segment_bar(
            &mut s,
            24.0,
            280.0,
            592.0,
            32.0,
            fraction,
            24,
            tone,
            &t.panel_edge,
            &t.panel_edge,
        );
        text(
            &mut s,
            28.0,
            340.0,
            16.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &format!("WALL {} / {}", thousands(self.wall), thousands(self.goal)),
        );
        text(
            &mut s,
            28.0,
            364.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &format!(
                "{} {} HELPING",
                thousands(self.helpers),
                if self.helpers == 1 { "CRAB" } else { "CRABS" }
            ),
        );
        for (n, (name, bags)) in self.top.iter().take(3).enumerate() {
            let y = 328.0 + n as f32 * 18.0;
            let medal = [&t.gold, "#d6deee", "#db8f4c"][n];
            let _ = write!(
                s,
                r#"<rect x="316" y="{}" width="8" height="8" fill="{medal}"/>"#,
                y - 8.0
            );
            text(
                &mut s,
                332.0,
                y,
                13.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(name, 13.0, 200.0)),
            );
            text(
                &mut s,
                612.0,
                y,
                13.0,
                PIXEL,
                false,
                &t.muted,
                "end",
                &format!("{bags} BAGS"),
            );
        }
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use image::RgbaImage;

    use super::*;
    use crate::{Renderer, encode_png};

    fn png() -> Vec<u8> {
        encode_png(&RgbaImage::from_pixel(72, 68, image::Rgba([0xe0, 0x4a, 0x3f, 255]))).expect("encodes")
    }

    fn size(png: &[u8]) -> (u32, u32) {
        let image = image::load_from_memory(png).expect("a png");
        (image.width(), image.height())
    }

    #[test]
    fn the_derby_lobby_shows_every_lane_and_the_pool() {
        let sprite = png();
        let names: Vec<String> = ["Ann", "Bo", "Cy"].map(String::from).to_vec();
        let lanes: Vec<DerbyLane<'_>> = (0..6)
            .map(|n| DerbyLane {
                name: ["Pebble", "Scuttle", "Kelp", "Barnacle", "Clawdia", "Tidbit"][n],
                accent: 0xff6b4a,
                art: SpriteArt {
                    png: &sprite,
                    w: 72,
                    h: 68,
                },
                backers: if n == 1 { &names } else { &[] },
            })
            .collect();
        let card = DerbyCard {
            lanes: &lanes,
            stake: 50,
            racers: 3,
            status: "Pick your crab",
            theme: Theme::reef(),
        };
        let svg = card.to_svg();
        for name in [
            "Pebble",
            "Tidbit",
            "LANE 6",
            "NO BACKERS",
            "3 BACKERS",
            "POOL 150",
            "Ann +2",
        ] {
            assert!(svg.contains(name), "{name}");
        }
        assert_eq!(svg.matches("<image").count(), 6, "a crab per lane");
        assert_eq!(size(&Renderer::new().svg_to_png(&svg).expect("renders")), (600, 470));
        let free = DerbyCard { stake: 0, ..card }.to_svg();
        assert!(free.contains("FOR FUN") && !free.contains("POOL"));
    }

    #[test]
    fn share_or_grab_hides_the_picks_until_both_are_in() {
        let card = |picks, banner| ShareGrabCard {
            names: ["Ann", "A very very long display name"],
            picks,
            pool: 60,
            wager: false,
            banner,
            mood: Mood::Wait,
            theme: Theme::reef(),
        };
        let waiting = card([PickView::Locked, PickView::Thinking], "Waiting for both").to_svg();
        assert!(waiting.contains("LOCKED IN") && waiting.contains("THINKING...") && waiting.contains("?"));
        assert!(!waiting.contains(">SHARE<") && !waiting.contains(">GRAB<"));
        let shown = card([PickView::Share, PickView::Grab], "Ann was tricked").to_svg();
        assert!(shown.contains(">SHARE<") && shown.contains(">GRAB<"));
        assert_eq!(size(&Renderer::new().svg_to_png(&shown).expect("renders")), (580, 350));
    }

    #[test]
    fn the_shell_game_lifts_the_shells_to_show_the_pearl() {
        let card = |picked, pearl| ShellCard {
            stake: 20,
            picked,
            pearl,
            status: "Pick a shell",
            payout: 50,
            theme: Theme::reef(),
        };
        let hidden = card(None, None).to_svg();
        assert!(!hidden.contains("YOUR PICK"));
        let shown = card(Some(1), Some(1)).to_svg();
        assert!(hidden.contains("STAKE 20 PAYS 50"));
        assert!(shown.contains("YOUR PICK"));
        assert!(
            shown.matches("<rect").count() > hidden.matches("<rect").count(),
            "the pearl is drawn"
        );
        assert_eq!(size(&Renderer::new().svg_to_png(&shown).expect("renders")), (520, 320));
    }

    #[test]
    fn the_heist_card_shows_the_crew_and_how_each_stage_went() {
        let crew = [
            CrewView { name: "Ann", role: 0 },
            CrewView { name: "Bo", role: 1 },
            CrewView { name: "Cy", role: 2 },
        ];
        let stage = |passed| StageView {
            title: "Crack the clam lock",
            chance: 64,
            passed,
            line: "Click, click, clunk: the clam lock yawns open.",
        };
        let card = |passed: Option<bool>, headline| HeistCard {
            crew: &crew,
            stages: [stage(passed), stage(passed), stage(passed)],
            headline,
            each: if passed.is_some() { 60 } else { 0 },
            theme: Theme::reef(),
        };
        let lobby = card(None, "CREW 3/8").to_svg();
        assert!(lobby.contains("SEALED") && lobby.contains("TO COME") && !lobby.contains("EACH"));
        let won = card(Some(true), "THE VAULT IS OPEN").to_svg();
        assert!(won.contains("OPEN!") && won.contains("PASSED") && won.contains("60 EACH"));
        let lost = card(Some(false), "BUSTED").to_svg();
        assert!(lost.contains("STAYS SHUT") && lost.contains("FAILED"));
        assert_eq!(size(&Renderer::new().svg_to_png(&won).expect("renders")), (600, 560));
        assert!(card(None, "x").to_svg().contains("LOOKOUT") && card(None, "x").to_svg().contains("DIVER"));
    }

    #[test]
    fn the_storm_wall_rises_with_the_sandbags_and_the_card_says_how_it_ended() {
        let top = vec![("Ann".to_owned(), 12), ("Bo".to_owned(), 9)];
        let card = |wall, state| SurgeCard {
            wall,
            goal: 40,
            helpers: 2,
            top: &top,
            state,
            theme: Theme::reef(),
        };
        let bags = |svg: &str| svg.matches("fill=\"#c9ab6a\"").count() + svg.matches("fill=\"#b99a5a\"").count();
        let low = card(2, SurgeState::Rolling).to_svg();
        let high = card(36, SurgeState::Rolling).to_svg();
        assert!(bags(&high) > bags(&low) * 2, "{} then {}", bags(&low), bags(&high));
        assert!(low.contains("HOLD THE WALL") && low.contains("WALL 2 / 40") && low.contains("2 CRABS HELPING"));
        assert!(card(40, SurgeState::Held).to_svg().contains("THE WALL HELD"));
        assert!(card(3, SurgeState::Broken).to_svg().contains("THE SEA WON"));
        assert!(low.contains("Ann") && low.contains("12 BAGS"));
        assert_eq!(size(&Renderer::new().svg_to_png(&high).expect("renders")), (640, 380));
        // An empty wall and a goal of nothing do not break the drawing.
        let _ = card(0, SurgeState::Rolling).to_svg();
        let _ = SurgeCard {
            goal: 0,
            ..card(0, SurgeState::Rolling)
        }
        .to_svg();
    }
}
