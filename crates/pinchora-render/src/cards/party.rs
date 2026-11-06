//! The cards of the party games: lobbies, the Hot Pearl table, standings, acronym letters, anonymous
//! entries and their reveal, the Wordle Race boards, the postcard, Would You Rather and Truth or Dare.

use std::fmt::Write;

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{
        CRAB, HEART, SHELL, STAR, background, data_uri_png, escape, mix, panel, pixel_width, sprite, truncate_to, wrap,
    },
};

const WIDTH: f32 = 900.0;

fn open(w: f32, h: f32) -> String {
    let mut s = String::with_capacity(24 * 1024);
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
    );
    s
}

fn heading(s: &mut String, t: &Theme, title: &str, right: &str) {
    text(
        s,
        36.0,
        62.0,
        30.0,
        PIXEL,
        false,
        &t.ink,
        "start",
        &escape(&title.to_uppercase()),
    );
    if !right.is_empty() {
        let chip = pixel_width(right, 16.0) + 28.0;
        panel(s, WIDTH - 36.0 - chip, 30.0, chip, 36.0, 4.0, &t.panel, &t.gold);
        text(
            s,
            WIDTH - 36.0 - chip / 2.0,
            55.0,
            16.0,
            PIXEL,
            false,
            &t.gold,
            "middle",
            &escape(&right.to_uppercase()),
        );
    }
}

fn footer(s: &mut String, t: &Theme, y: f32, line: &str) {
    text(
        s,
        WIDTH / 2.0,
        y,
        15.0,
        PIXEL,
        false,
        &t.muted,
        "middle",
        &escape(&line.to_uppercase()),
    );
}

/// A filled disc in pixel steps: rows of `step` pixels, each as wide as the disc is there.
fn pixel_disc(out: &mut String, cx: f32, cy: f32, r: f32, step: f32, fill: &str) {
    let rows = (2.0 * r / step).round() as i32;
    for k in 0..rows {
        let y = -r + (k as f32 + 0.5) * step;
        let half = ((r * r - y * y).max(0.0).sqrt() / step).round() * step;
        if half <= 0.0 {
            continue;
        }
        let _ = write!(
            out,
            r#"<rect x="{}" y="{}" width="{}" height="{step}" fill="{fill}"/>"#,
            cx - half,
            cy - r + k as f32 * step,
            half * 2.0
        );
    }
}

fn rect(out: &mut String, x: f32, y: f32, w: f32, h: f32, fill: &str) {
    let _ = write!(out, r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}"/>"#);
}

/// Hearts for lives left, dim ones for lives lost, right-aligned to `x_end`.
fn hearts(out: &mut String, t: &Theme, x_end: f32, y: f32, left: i32, total: i32) {
    for k in 0..total {
        let x = x_end - (total - k) as f32 * 20.0;
        if k < left {
            sprite(out, HEART, x, y, 2.4, &t.heart, "#ffd0da");
        } else {
            sprite(out, HEART, x, y, 2.4, "#2e3d75", "#3b4676");
        }
    }
}

// /Lobby

/// 900-wide lobby: what the game is, who has joined, and how many more it needs.
pub struct PartyLobbyCard<'a> {
    pub title: &'a str,
    pub tagline: &'a str,
    pub players: &'a [String],
    pub min: usize,
    pub max: usize,
    pub footer: &'a str,
    pub theme: Theme,
}

impl PartyLobbyCard<'_> {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let max = self.max.clamp(2, 10);
        let rows = max.div_ceil(2);
        let lines = wrap(self.tagline, 62, 2);
        let top = 96.0 + lines.len() as f32 * 28.0 + 12.0;
        let h = top + rows as f32 * 56.0 + 70.0;
        let mut s = open(WIDTH, h);
        background(&mut s, WIDTH, h, &t.bg_top, &t.bg_bottom);
        heading(&mut s, t, self.title, &format!("{}/{}", self.players.len(), max));
        for (i, line) in lines.iter().enumerate() {
            text(
                &mut s,
                36.0,
                100.0 + i as f32 * 28.0,
                19.0,
                SANS,
                false,
                &t.muted,
                "start",
                &escape(&truncate_to(line, 19.0, 828.0)),
            );
        }
        for seat in 0..max {
            let (col, row) = ((seat % 2) as f32, (seat / 2) as f32);
            let (x, y) = (36.0 + col * 424.0, top + row * 56.0);
            let name = self.players.get(seat);
            let edge = if seat == 0 && name.is_some() {
                &t.gold
            } else {
                &t.panel_edge
            };
            panel(&mut s, x, y, 404.0, 48.0, 4.0, &t.panel, edge);
            text(
                &mut s,
                x + 18.0,
                y + 32.0,
                16.0,
                PIXEL,
                false,
                if name.is_some() { &t.gold } else { &t.panel_edge },
                "start",
                &format!("{}", seat + 1),
            );
            match name {
                Some(name) => {
                    text(
                        &mut s,
                        x + 52.0,
                        y + 32.0,
                        20.0,
                        SANS,
                        true,
                        &t.ink,
                        "start",
                        &escape(&truncate_to(name, 20.0, 300.0)),
                    );
                    if seat == 0 {
                        sprite(&mut s, STAR, x + 404.0 - 40.0, y + 14.0, 3.0, &t.gold, "#fff1e8");
                    }
                }
                None => text(
                    &mut s,
                    x + 52.0,
                    y + 31.0,
                    17.0,
                    SANS,
                    false,
                    &mix(&t.muted, &t.bg_bottom, 0.5),
                    "start",
                    "open seat",
                ),
            }
        }
        let need = self.min.saturating_sub(self.players.len());
        let line = if need == 0 {
            self.footer.to_owned()
        } else {
            format!("Waiting for {need} more player{}", if need == 1 { "" } else { "s" })
        };
        footer(&mut s, t, h - 26.0, &line);
        s.push_str("</svg>");
        s
    }
}

// /Hot Pearl

/// One player at the Hot Pearl table.
pub struct PearlSeat<'a> {
    pub name: &'a str,
    pub lives: i32,
    pub max_lives: i32,
    pub out: bool,
    /// Holds the pearl.
    pub current: bool,
}

/// 900×560 Hot Pearl table: the pearl with its syllable in the middle, the players round it.
pub struct PearlCard<'a> {
    pub syllable: &'a str,
    pub seats: &'a [PearlSeat<'a>],
    /// The line under the pearl: whose turn it is and for how long, or what just happened.
    pub banner: &'a str,
    /// The last word played, if any.
    pub last_word: Option<&'a str>,
    pub theme: Theme,
}

impl PearlCard<'_> {
    const CHIP: (f32, f32) = (204.0, 56.0);

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (WIDTH, 560.0);
        let mut s = open(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        let (cx, cy) = (450.0, 262.0);

        // A table of dusk-coloured water under the pearl.
        let _ = write!(
            s,
            r##"<ellipse cx="{cx}" cy="{cy}" rx="250" ry="170" fill="#0b1029" fill-opacity="0.45"/><ellipse cx="{cx}" cy="{cy}" rx="250" ry="170" fill="none" stroke="{}" stroke-width="4" stroke-dasharray="10 10"/>"##,
            t.panel_edge
        );

        // The pearl: a pixel disc, a lit side, a shadow and a few glints, with a fuse burning on top.
        pixel_disc(&mut s, cx, cy + 6.0, 82.0, 6.0, "#8f84c4");
        pixel_disc(&mut s, cx - 2.0, cy + 2.0, 78.0, 6.0, "#d9d0f0");
        pixel_disc(&mut s, cx - 8.0, cy - 6.0, 62.0, 6.0, "#f1ecfb");
        rect(&mut s, cx - 52.0, cy - 54.0, 18.0, 12.0, "#ffffff");
        rect(&mut s, cx - 34.0, cy - 60.0, 12.0, 6.0, "#ffffff");
        rect(&mut s, cx - 62.0, cy - 36.0, 6.0, 12.0, "#ffffff");
        rect(&mut s, cx - 6.0, cy - 112.0, 6.0, 30.0, "#c98a1e");
        rect(&mut s, cx - 6.0, cy - 112.0, 6.0, 6.0, "#ffd166");
        sprite(&mut s, STAR, cx - 15.0, cy - 138.0, 4.0, &t.accent, "#ffd166");
        let syllable = self.syllable.to_uppercase();
        text(
            &mut s,
            cx - 2.0,
            cy + 18.0,
            46.0,
            PIXEL,
            true,
            "#2e3d75",
            "middle",
            &escape(&syllable),
        );

        // The players on an ellipse, starting at the top and going clockwise.
        let n = self.seats.len().max(1) as f32;
        for (i, seat) in self.seats.iter().enumerate() {
            let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / n;
            let (px, py) = (cx + 330.0 * angle.cos(), cy + 190.0 * angle.sin());
            let (cw, ch) = Self::CHIP;
            let (x, y) = ((px - cw / 2.0).round(), (py - ch / 2.0).round());
            let (fill, edge) = match (seat.out, seat.current) {
                (true, _) => (mix(&t.panel, "#000000", 0.35), "#262f55".to_owned()),
                (_, true) => (mix(&t.panel, &t.gold, 0.25), t.gold.clone()),
                _ => (t.panel.clone(), t.panel_edge.clone()),
            };
            panel(&mut s, x, y, cw, ch, 4.0, &fill, &edge);
            let ink = if seat.out { &t.muted } else { &t.ink };
            text(
                &mut s,
                x + 14.0,
                y + 25.0,
                17.0,
                SANS,
                true,
                ink,
                "start",
                &escape(&truncate_to(seat.name, 17.0, cw - 28.0)),
            );
            if seat.out {
                text(&mut s, x + 14.0, y + 46.0, 12.0, PIXEL, false, &t.muted, "start", "OUT");
            } else {
                hearts(&mut s, t, x + cw - 10.0, y + 31.0, seat.lives, seat.max_lives);
                if seat.current {
                    text(
                        &mut s,
                        x + 14.0,
                        y + 47.0,
                        11.0,
                        PIXEL,
                        false,
                        &t.gold,
                        "start",
                        "HOLDS THE PEARL",
                    );
                }
            }
        }

        // Under the table, where the players' seats cannot reach.
        text(
            &mut s,
            cx,
            516.0,
            15.0,
            PIXEL,
            false,
            &t.gold,
            "middle",
            &escape(&truncate_pixel(&self.banner.to_uppercase(), 15.0, 840.0)),
        );
        if let Some(word) = self.last_word {
            text(
                &mut s,
                cx,
                540.0,
                13.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &escape(&format!("LAST WORD {}", word.to_uppercase())),
            );
        }
        s.push_str("</svg>");
        s
    }
}

fn truncate_pixel(text: &str, size: f32, max: f32) -> String {
    crate::svg::truncate_pixel_to(text, size, max)
}

// /Standings

/// One line of the standings.
pub struct StandingRow<'a> {
    pub rank: u32,
    pub name: &'a str,
    /// What they scored: `5 pts`, `lives 2`, `SOLVED IN 3`.
    pub score: &'a str,
    /// What it paid, or a note: `+34 shells`.
    pub note: &'a str,
    pub winner: bool,
}

/// 900-wide standings: a ranked list with a star on the winner, the end of every party game.
pub struct StandingsCard<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub rows: &'a [StandingRow<'a>],
    pub footer: &'a str,
    pub theme: Theme,
}

impl StandingsCard<'_> {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let h = 150.0 + self.rows.len().min(10) as f32 * 62.0 + 40.0;
        let mut s = open(WIDTH, h);
        background(&mut s, WIDTH, h, &t.bg_top, &t.bg_bottom);
        heading(&mut s, t, self.title, "");
        text(
            &mut s,
            36.0,
            102.0,
            19.0,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&truncate_to(self.subtitle, 19.0, 828.0)),
        );
        for (i, row) in self.rows.iter().take(10).enumerate() {
            let y = 126.0 + i as f32 * 62.0;
            let edge = if row.winner { &t.gold } else { &t.panel_edge };
            let fill = if row.winner {
                mix(&t.panel, &t.gold, 0.12)
            } else {
                t.panel.clone()
            };
            panel(&mut s, 36.0, y, 828.0, 54.0, 4.0, &fill, edge);
            text(
                &mut s,
                70.0,
                y + 36.0,
                22.0,
                PIXEL,
                false,
                edge,
                "middle",
                &format!("{}", row.rank),
            );
            let mut x = 106.0;
            if row.winner {
                sprite(&mut s, STAR, x, y + 14.0, 3.4, &t.gold, "#fff1e8");
                x += 34.0;
            }
            text(
                &mut s,
                x,
                y + 35.0,
                22.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(row.name, 22.0, 400.0 - (x - 106.0))),
            );
            text(
                &mut s,
                840.0,
                y + 24.0,
                16.0,
                PIXEL,
                false,
                &t.ink,
                "end",
                &escape(&row.score.to_uppercase()),
            );
            if !row.note.is_empty() {
                text(
                    &mut s,
                    840.0,
                    y + 45.0,
                    13.0,
                    PIXEL,
                    false,
                    &t.mint,
                    "end",
                    &escape(&row.note.to_uppercase()),
                );
            }
        }
        footer(&mut s, t, h - 16.0, self.footer);
        s.push_str("</svg>");
        s
    }
}

// /Acronym letters

/// 900×380 prompt of Acronym Tide: the letters to build a phrase from.
pub struct LettersCard<'a> {
    pub title: &'a str,
    pub round: &'a str,
    pub letters: &'a [char],
    pub hint: &'a str,
    pub status: &'a str,
    pub theme: Theme,
}

impl LettersCard<'_> {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (WIDTH, 380.0);
        let mut s = open(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        heading(&mut s, t, self.title, self.round);
        let n = self.letters.len().max(1) as f32;
        let (tile, gap) = (120.0, 22.0);
        let x0 = (w - (n * tile + (n - 1.0) * gap)) / 2.0;
        for (i, letter) in self.letters.iter().enumerate() {
            let x = x0 + i as f32 * (tile + gap);
            panel(&mut s, x, 100.0, tile, tile, 6.0, &t.panel, &t.gold);
            rect(&mut s, x + 6.0, 106.0, tile - 12.0, 6.0, "#ffffff");
            let _ = write!(
                s,
                r##"<rect x="{}" y="106" width="{}" height="6" fill="#ffffff" fill-opacity="0.18"/>"##,
                x + 6.0,
                tile - 12.0
            );
            text(
                &mut s,
                x + tile / 2.0,
                100.0 + 84.0,
                68.0,
                PIXEL,
                true,
                &t.gold,
                "middle",
                &escape(&letter.to_ascii_uppercase().to_string()),
            );
            if i + 1 < self.letters.len() {
                sprite(&mut s, SHELL, x + tile + 3.0, 148.0, 2.4, &t.accent, "#ffd0c0");
            }
        }
        for (i, line) in wrap(self.hint, 66, 2).iter().enumerate() {
            text(
                &mut s,
                w / 2.0,
                270.0 + i as f32 * 28.0,
                20.0,
                SANS,
                false,
                &t.ink,
                "middle",
                &escape(&truncate_to(line, 20.0, 828.0)),
            );
        }
        footer(&mut s, t, 350.0, self.status);
        s.push_str("</svg>");
        s
    }
}

// /Entries

/// A picture to put above the entries (the postcard), as a PNG with its pixel size.
pub struct EntryHeader<'a> {
    pub png: &'a [u8],
    pub w: u32,
    pub h: u32,
}

/// One entry in a vote or a reveal.
pub struct EntryRow<'a> {
    pub text: &'a str,
    /// Shown on a reveal; `None` while it is anonymous.
    pub author: Option<&'a str>,
    pub votes: Option<u32>,
    pub winner: bool,
}

/// 900-wide list of numbered entries: anonymous while people vote, with names and votes at the reveal.
pub struct EntriesCard<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    /// What the right-hand chip says, e.g. `ROUND 2/3`.
    pub chip: &'a str,
    pub header: Option<EntryHeader<'a>>,
    pub rows: &'a [EntryRow<'a>],
    pub footer: &'a str,
    pub theme: Theme,
}

impl EntriesCard<'_> {
    /// Twice the 240-pixel backdrop, so every pixel of the scene stays a 2×2 block.
    const HEADER_W: f32 = 480.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let header_h = self.header.as_ref().map_or(0.0, |h| {
            (h.h as f32 * Self::HEADER_W / h.w.max(1) as f32).round() + 16.0
        });
        let top = 126.0 + header_h;
        let rows = self.rows.len().min(10);
        let h = top + rows as f32 * 78.0 + 44.0;
        let mut s = open(WIDTH, h);
        background(&mut s, WIDTH, h, &t.bg_top, &t.bg_bottom);
        heading(&mut s, t, self.title, self.chip);
        text(
            &mut s,
            36.0,
            102.0,
            19.0,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&truncate_to(self.subtitle, 19.0, 828.0)),
        );
        if let Some(header) = &self.header {
            let x = (WIDTH - Self::HEADER_W) / 2.0;
            let hh = header_h - 16.0;
            panel(
                &mut s,
                x - 6.0,
                122.0,
                Self::HEADER_W + 12.0,
                hh + 12.0,
                4.0,
                &t.panel,
                &t.panel_edge,
            );
            let _ = write!(
                s,
                r#"<image x="{x}" y="128" width="{}" height="{hh}" image-rendering="optimizeSpeed" href="{}"/>"#,
                Self::HEADER_W,
                data_uri_png(header.png)
            );
        }
        for (i, row) in self.rows.iter().take(10).enumerate() {
            let y = top + i as f32 * 78.0;
            let edge = if row.winner { &t.gold } else { &t.panel_edge };
            let fill = if row.winner {
                mix(&t.panel, &t.gold, 0.12)
            } else {
                t.panel.clone()
            };
            panel(&mut s, 36.0, y, 828.0, 70.0, 4.0, &fill, edge);
            panel(&mut s, 48.0, y + 10.0, 50.0, 50.0, 4.0, "#0b1029", edge);
            text(
                &mut s,
                73.0,
                y + 44.0,
                24.0,
                PIXEL,
                true,
                edge,
                "middle",
                &format!("{}", i + 1),
            );
            let right = if row.author.is_some() { 640.0 } else { 828.0 };
            let max_chars = if row.author.is_some() { 36 } else { 52 };
            let lines = wrap(row.text, max_chars, 2);
            let base = if lines.len() > 1 { y + 31.0 } else { y + 42.0 };
            for (j, line) in lines.iter().enumerate() {
                text(
                    &mut s,
                    116.0,
                    base + j as f32 * 25.0,
                    20.0,
                    SANS,
                    true,
                    &t.ink,
                    "start",
                    &escape(&truncate_to(line, 20.0, right - 116.0)),
                );
            }
            if let Some(author) = row.author {
                text(
                    &mut s,
                    846.0,
                    y + 30.0,
                    16.0,
                    SANS,
                    true,
                    &t.gold,
                    "end",
                    &escape(&truncate_to(author, 16.0, 190.0)),
                );
            }
            if let Some(votes) = row.votes {
                let label = format!("{votes} VOTE{}", if votes == 1 { "" } else { "S" });
                text(&mut s, 846.0, y + 56.0, 14.0, PIXEL, false, &t.mint, "end", &label);
            }
            if row.winner {
                sprite(&mut s, STAR, 600.0, y + 24.0, 3.0, &t.gold, "#fff1e8");
            }
        }
        footer(&mut s, t, h - 16.0, self.footer);
        s.push_str("</svg>");
        s
    }
}

// /Wordle Race

/// How a racer is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RacerState {
    Playing,
    Solved(usize),
    Failed,
}

pub struct Racer<'a> {
    pub name: &'a str,
    /// The colours of each guess so far (1 absent, 2 present, 3 correct); no letters.
    pub rows: &'a [[u8; 5]],
    pub state: RacerState,
}

/// 900×440 Wordle Race: a small board for each racer showing colours only.
pub struct RaceCard<'a> {
    pub title: &'a str,
    pub racers: &'a [Racer<'a>],
    pub footer: &'a str,
    pub theme: Theme,
}

impl RaceCard<'_> {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (WIDTH, 440.0);
        let mut s = open(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        heading(&mut s, t, self.title, "");
        let n = self.racers.len().clamp(1, 4) as f32;
        let col_w = (w - 72.0 - (n - 1.0) * 16.0) / n;
        let (tile, gap) = (24.0, 4.0);
        let board_w = 5.0 * tile + 4.0 * gap;
        for (i, racer) in self.racers.iter().take(4).enumerate() {
            let x = 36.0 + i as f32 * (col_w + 16.0);
            let edge = match racer.state {
                RacerState::Solved(_) => &t.mint,
                RacerState::Failed => &t.panel_edge,
                RacerState::Playing => &t.gold,
            };
            panel(&mut s, x, 90.0, col_w, 300.0, 4.0, &t.panel, edge);
            text(
                &mut s,
                x + col_w / 2.0,
                124.0,
                19.0,
                SANS,
                true,
                &t.ink,
                "middle",
                &escape(&truncate_to(racer.name, 19.0, col_w - 24.0)),
            );
            let bx = x + (col_w - board_w) / 2.0;
            for row in 0..6 {
                for col in 0..5 {
                    let mark = racer.rows.get(row).map_or(0, |r| r[col]);
                    let (fill, line) = match mark {
                        3 => (t.mint.clone(), mix(&t.mint, "#000000", 0.4)),
                        2 => (t.gold.clone(), mix(&t.gold, "#000000", 0.4)),
                        1 => ("#3b4676".to_owned(), "#262f55".to_owned()),
                        _ => ("#0b1029".to_owned(), "#1d2b53".to_owned()),
                    };
                    panel(
                        &mut s,
                        bx + col as f32 * (tile + gap),
                        146.0 + row as f32 * (tile + gap),
                        tile,
                        tile,
                        2.0,
                        &fill,
                        &line,
                    );
                }
            }
            let (label, colour) = match racer.state {
                RacerState::Solved(tries) => (format!("SOLVED IN {tries}"), &t.mint),
                RacerState::Failed => ("OUT OF TRIES".to_owned(), &t.muted),
                RacerState::Playing => (format!("{} LEFT", 6usize.saturating_sub(racer.rows.len())), &t.gold),
            };
            text(
                &mut s,
                x + col_w / 2.0,
                372.0,
                14.0,
                PIXEL,
                false,
                colour,
                "middle",
                &label,
            );
        }
        footer(&mut s, t, 420.0, self.footer);
        s.push_str("</svg>");
        s
    }
}

// /Postcard

/// A postcard: the scene (960×540 PNG) on cream paper with a stamp, a postmark and the place it greets from.
pub struct PostcardCard<'a> {
    pub scene: &'a [u8],
    pub place: &'a str,
    /// Stamped in the corner: `ROUND 2/3`.
    pub stamp: &'a str,
    pub theme: Theme,
}

impl PostcardCard<'_> {
    pub const WIDTH: f32 = 1040.0;
    pub const HEIGHT: f32 = 700.0;

    pub fn to_svg(&self) -> String {
        let (w, h) = (Self::WIDTH, Self::HEIGHT);
        let mut s = open(w, h);
        let (paper, ink, red, blue) = ("#f4e9d1", "#4a2f17", "#d6483f", "#3a6fb0");
        rect(&mut s, 0.0, 0.0, w, h, paper);
        // The air mail border: red and blue dashes all the way round.
        let mut x = 0.0;
        let mut flip = false;
        while x < w {
            for y in [0.0, h - 14.0] {
                rect(&mut s, x, y, 28.0, 14.0, if flip { blue } else { red });
            }
            x += 28.0;
            flip = !flip;
        }
        let mut y = 0.0;
        flip = false;
        while y < h {
            for x in [0.0, w - 14.0] {
                rect(&mut s, x, y, 14.0, 28.0, if flip { blue } else { red });
            }
            y += 28.0;
            flip = !flip;
        }
        // The title: GREETINGS FROM, then the place in big letters.
        text(&mut s, 44.0, 58.0, 18.0, PIXEL, false, red, "start", "GREETINGS FROM");
        text(
            &mut s,
            44.0,
            112.0,
            48.0,
            PIXEL,
            true,
            ink,
            "start",
            &escape(&truncate_pixel(&self.place.to_uppercase(), 48.0, 640.0)),
        );
        // The picture, one image pixel to one card pixel, in a white mount.
        rect(&mut s, 34.0, 128.0, 972.0, 552.0, "#ffffff");
        rect(&mut s, 40.0, 134.0, 960.0, 540.0, "#0b1029");
        let _ = write!(
            s,
            r#"<image x="40" y="134" width="960" height="540" image-rendering="optimizeSpeed" href="{}"/>"#,
            data_uri_png(self.scene)
        );
        // The stamp: perforated edge, a crab, the round.
        let (sx, sy) = (874.0, 28.0);
        rect(&mut s, sx - 6.0, sy - 6.0, 132.0, 104.0, "#ffffff");
        for k in 0..11 {
            let o = k as f32 * 12.0;
            rect(&mut s, sx - 6.0 + o, sy - 6.0, 6.0, 6.0, paper);
            rect(&mut s, sx - 6.0 + o, sy + 92.0, 6.0, 6.0, paper);
        }
        for k in 0..9 {
            let o = k as f32 * 12.0;
            rect(&mut s, sx - 6.0, sy - 6.0 + o, 6.0, 6.0, paper);
            rect(&mut s, sx + 120.0, sy - 6.0 + o, 6.0, 6.0, paper);
        }
        rect(&mut s, sx, sy, 120.0, 92.0, "#ffe3a8");
        sprite(&mut s, CRAB, sx + 12.0, sy + 8.0, 4.4, red, "#ffd0c0");
        text(
            &mut s,
            sx + 60.0,
            sy + 80.0,
            12.0,
            PIXEL,
            false,
            ink,
            "middle",
            &escape(&self.stamp.to_uppercase()),
        );
        // A postmark with wavy lines beside it.
        let _ = write!(
            s,
            r##"<circle cx="830" cy="58" r="34" fill="none" stroke="{ink}" stroke-opacity="0.55" stroke-width="3"/><path d="M650,40 h120 M650,56 h120 M650,72 h120" stroke="{ink}" stroke-opacity="0.45" stroke-width="3" stroke-dasharray="12 6"/>"##
        );
        text(&mut s, 830.0, 54.0, 10.0, PIXEL, false, ink, "middle", "PINCHORA");
        text(&mut s, 830.0, 70.0, 10.0, PIXEL, false, ink, "middle", "REEF POST");
        s.push_str("</svg>");
        s
    }
}

// /Would You Rather

/// 900×470 Would You Rather: the two sides, and the split once it is shown.
pub struct RatherCard<'a> {
    pub category: &'a str,
    pub a: &'a str,
    pub b: &'a str,
    /// Votes for each side, shown as bars when the card is a result.
    pub votes: Option<(i64, i64)>,
    pub closed: bool,
    pub theme: Theme,
}

impl RatherCard<'_> {
    pub fn percent(votes: i64, total: i64) -> i64 {
        if total <= 0 {
            0
        } else {
            (votes * 100 + total / 2) / total
        }
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (WIDTH, 470.0);
        let mut s = open(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        heading(&mut s, t, "Would you rather", self.category);
        let total = self.votes.map_or(0, |(a, b)| a + b);
        for (i, (label, tone)) in [(self.a, &t.accent), (self.b, &t.mint)].into_iter().enumerate() {
            let x = 36.0 + i as f32 * 424.0;
            let ahead = self
                .votes
                .is_some_and(|(a, b)| total > 0 && if i == 0 { a > b } else { b > a });
            panel(
                &mut s,
                x,
                92.0,
                404.0,
                300.0,
                6.0,
                &mix(&t.panel, tone, 0.14),
                if ahead { &t.gold } else { tone },
            );
            panel(&mut s, x + 16.0, 108.0, 54.0, 54.0, 4.0, "#0b1029", tone);
            text(
                &mut s,
                x + 43.0,
                148.0,
                32.0,
                PIXEL,
                true,
                tone,
                "middle",
                if i == 0 { "A" } else { "B" },
            );
            if ahead {
                sprite(&mut s, STAR, x + 404.0 - 52.0, 118.0, 4.0, &t.gold, "#fff1e8");
            }
            for (j, line) in wrap(label, 21, 5).iter().enumerate() {
                text(
                    &mut s,
                    x + 202.0,
                    208.0 + j as f32 * 34.0,
                    26.0,
                    SANS,
                    true,
                    &t.ink,
                    "middle",
                    &escape(&truncate_to(line, 26.0, 372.0)),
                );
            }
            if let Some((a, b)) = self.votes {
                let votes = if i == 0 { a } else { b };
                let share = Self::percent(votes, total);
                text(
                    &mut s,
                    x + 202.0,
                    372.0,
                    34.0,
                    // The pixel font has no good percent sign at this size.
                    SANS,
                    true,
                    tone,
                    "middle",
                    &format!("{share}%"),
                );
                let bar = 372.0 * (share as f32 / 100.0);
                rect(&mut s, x + 16.0, 380.0, 372.0, 6.0, "#0b1029");
                rect(&mut s, x + 16.0, 380.0, bar, 6.0, tone);
            }
        }
        let line = match self.votes {
            Some((a, b)) if self.closed => format!("Final: {} vote{}", a + b, if a + b == 1 { "" } else { "s" }),
            Some(_) => "Results so far".to_owned(),
            None => "Pick a side with the buttons".to_owned(),
        };
        footer(&mut s, t, 430.0, &line);
        s.push_str("</svg>");
        s
    }
}

// /Truth or Dare

/// What a prompt card is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTone {
    /// Somebody's turn to pick.
    Turn,
    Truth,
    Dare,
}

/// 900-wide card of Truth or Dare: whose turn it is, and the truth or the dare they got.
pub struct PromptCard<'a> {
    pub tone: PromptTone,
    pub name: &'a str,
    /// The truth or dare; for a turn card, a line asking which they pick.
    pub text: &'a str,
    pub category: &'a str,
    pub footer: &'a str,
    pub theme: Theme,
}

impl PromptCard<'_> {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (badge, tone) = match self.tone {
            PromptTone::Turn => ("YOUR TURN", t.gold.clone()),
            PromptTone::Truth => ("TRUTH", t.mint.clone()),
            PromptTone::Dare => ("DARE", t.accent.clone()),
        };
        let lines = wrap(self.text, 36, 4);
        let h = 250.0 + lines.len() as f32 * 44.0;
        let mut s = open(WIDTH, h);
        background(&mut s, WIDTH, h, &t.bg_top, &t.bg_bottom);
        let chip = pixel_width(badge, 22.0) + 40.0;
        panel(&mut s, 36.0, 32.0, chip, 48.0, 4.0, &mix(&t.panel, &tone, 0.2), &tone);
        text(
            &mut s,
            36.0 + chip / 2.0,
            65.0,
            22.0,
            PIXEL,
            true,
            &tone,
            "middle",
            badge,
        );
        if !self.category.is_empty() {
            let c = pixel_width(self.category, 15.0) + 28.0;
            panel(&mut s, WIDTH - 36.0 - c, 38.0, c, 36.0, 4.0, &t.panel, &t.panel_edge);
            text(
                &mut s,
                WIDTH - 36.0 - c / 2.0,
                62.0,
                15.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &escape(&self.category.to_uppercase()),
            );
        }
        text(
            &mut s,
            36.0,
            132.0,
            26.0,
            SANS,
            true,
            &t.gold,
            "start",
            &escape(&truncate_to(self.name, 26.0, 760.0)),
        );
        panel(
            &mut s,
            36.0,
            152.0,
            828.0,
            24.0 + lines.len() as f32 * 44.0 + 20.0,
            4.0,
            &t.panel,
            &tone,
        );
        for (i, line) in lines.iter().enumerate() {
            text(
                &mut s,
                60.0,
                200.0 + i as f32 * 44.0,
                30.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(line, 30.0, 780.0)),
            );
        }
        footer(&mut s, t, h - 20.0, self.footer);
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use image::RgbaImage;

    use super::*;
    use crate::{Renderer, encode_png};

    fn size(svg: &str) -> (u32, u32) {
        let png = Renderer::new().svg_to_png(svg).expect("renders");
        let image = image::load_from_memory(&png).expect("a png");
        (image.width(), image.height())
    }

    fn scene() -> Vec<u8> {
        encode_png(&RgbaImage::from_pixel(960, 540, image::Rgba([120, 180, 220, 255]))).expect("encodes")
    }

    fn names(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("Player number {i}")).collect()
    }

    #[test]
    fn the_lobby_shows_seats_and_what_is_missing() {
        let players = names(2);
        let card = PartyLobbyCard {
            title: "Hot Pearl",
            tagline: "Type a word with the syllable before the pearl pops.",
            players: &players,
            min: 3,
            max: 8,
            footer: "Press start",
            theme: Theme::reef(),
        };
        let svg = card.to_svg();
        assert!(svg.contains("HOT PEARL") && svg.contains("2/8") && svg.contains("Player number 2"));
        assert!(svg.contains("open seat") && svg.contains("WAITING FOR 1 MORE PLAYER"));
        let (w, h) = size(&svg);
        assert_eq!(w, 900);
        assert!(h > 300);
        let ready = PartyLobbyCard { min: 2, ..card };
        assert!(ready.to_svg().contains("PRESS START"));
    }

    #[test]
    fn the_pearl_table_seats_everyone_and_marks_the_holder() {
        for n in [2usize, 3, 5, 8] {
            let seats: Vec<PearlSeat> = (0..n)
                .map(|i| PearlSeat {
                    name: "Somebody with a very long name indeed",
                    lives: 1 + (i as i32 % 2),
                    max_lives: 2,
                    out: i == 1 && n > 2,
                    current: i == 0,
                })
                .collect();
            let card = PearlCard {
                syllable: "ear",
                seats: &seats,
                banner: "Somebody has 25 seconds",
                last_word: Some("pearl"),
                theme: Theme::reef(),
            };
            let svg = card.to_svg();
            assert!(svg.contains(">EAR<") && svg.contains("HOLDS THE PEARL") && svg.contains("LAST WORD PEARL"));
            assert_eq!(svg.matches("HOLDS THE PEARL").count(), 1);
            assert_eq!(svg.contains(">OUT<"), n > 2);
            assert_eq!(size(&svg), (900, 560));
        }
    }

    #[test]
    fn standings_rank_the_winner_first_and_list_what_it_paid() {
        let rows = [
            StandingRow {
                rank: 1,
                name: "Ada",
                score: "7 pts",
                note: "+25 shells",
                winner: true,
            },
            StandingRow {
                rank: 2,
                name: "Bo",
                score: "3 pts",
                note: "",
                winner: false,
            },
        ];
        let card = StandingsCard {
            title: "Acronym Tide",
            subtitle: "Final scores after 3 rounds",
            rows: &rows,
            footer: "Thanks for playing",
            theme: Theme::reef(),
        };
        let svg = card.to_svg();
        assert!(svg.contains("7 PTS") && svg.contains("+25 SHELLS") && svg.contains(">Ada<"));
        assert_eq!(size(&svg).0, 900);
    }

    #[test]
    fn letters_and_entries_draw_what_players_need_to_see() {
        let letters = LettersCard {
            title: "Acronym Tide",
            round: "Round 2/3",
            letters: &['T', 'W', 'R', 'S'],
            hint: "One word per letter, in order.",
            status: "2 of 5 phrases in",
            theme: Theme::reef(),
        };
        let svg = letters.to_svg();
        assert!(svg.contains(">T<") && svg.contains(">S<") && svg.contains("ROUND 2/3"));
        assert_eq!(size(&svg), (900, 380));

        let rows = [
            EntryRow {
                text: "Tiny whales ride sleds",
                author: None,
                votes: None,
                winner: false,
            },
            EntryRow {
                text: "Terrific wizards refuse sandwiches, politely, while hopping on one leg, and singing loudly",
                author: None,
                votes: None,
                winner: false,
            },
        ];
        let anonymous = EntriesCard {
            title: "Which is best",
            subtitle: "Pick the best",
            chip: "Round 1/3",
            header: None,
            rows: &rows,
            footer: "Closing soon",
            theme: Theme::reef(),
        };
        let svg = anonymous.to_svg();
        assert!(
            svg.contains("Tiny whales ride sleds") && !svg.contains("VOTE"),
            "no tallies while anonymous"
        );
        let png = scene();
        let revealed_rows = [
            EntryRow {
                text: "Tiny whales ride sleds",
                author: Some("Ada"),
                votes: Some(2),
                winner: true,
            },
            EntryRow {
                text: "Nothing",
                author: Some("Bo"),
                votes: Some(0),
                winner: false,
            },
        ];
        let reveal = EntriesCard {
            header: Some(EntryHeader {
                png: &png,
                w: 960,
                h: 540,
            }),
            rows: &revealed_rows,
            title: "Results",
            ..anonymous
        };
        let svg = reveal.to_svg();
        assert!(svg.contains("2 VOTES") && svg.contains("0 VOTES") && svg.contains(">Ada<") && svg.contains("<image"));
        let (w, h) = size(&svg);
        assert_eq!(w, 900);
        assert!(h > 520, "the picture adds height: {h}");
    }

    #[test]
    fn the_race_shows_colours_but_never_letters() {
        let rows = [[1u8, 2, 3, 1, 1], [3, 3, 3, 3, 3]];
        let racers = [
            Racer {
                name: "Ada",
                rows: &rows,
                state: RacerState::Solved(2),
            },
            Racer {
                name: "Bo",
                rows: &rows[..1],
                state: RacerState::Playing,
            },
            Racer {
                name: "Cy",
                rows: &[],
                state: RacerState::Failed,
            },
        ];
        let card = RaceCard {
            title: "Wordle Race",
            racers: &racers,
            footer: "First to solve wins",
            theme: Theme::reef(),
        };
        let svg = card.to_svg();
        assert!(svg.contains("SOLVED IN 2") && svg.contains("5 LEFT") && svg.contains("OUT OF TRIES"));
        assert_eq!(size(&svg), (900, 440));
    }

    #[test]
    fn a_postcard_keeps_its_picture_pixel_for_pixel() {
        let png = scene();
        let svg = PostcardCard {
            scene: &png,
            place: "Driftwood Dunes",
            stamp: "Round 1/3",
            theme: Theme::reef(),
        }
        .to_svg();
        assert!(svg.contains("GREETINGS FROM") && svg.contains("DRIFTWOOD DUNES") && svg.contains("ROUND 1/3"));
        assert!(svg.contains(r#"width="960" height="540""#));
        assert_eq!(size(&svg), (1040, 700));
    }

    #[test]
    fn would_you_rather_shows_the_split_only_when_asked() {
        let open = RatherCard {
            category: "Silly",
            a: "Have hiccups that sound like a duck",
            b: "Sneeze glitter",
            votes: None,
            closed: false,
            theme: Theme::reef(),
        };
        let svg = open.to_svg();
        assert!(svg.contains("PICK A SIDE") && !svg.contains('%'));
        let result = RatherCard {
            votes: Some((3, 1)),
            closed: true,
            ..open
        };
        let svg = result.to_svg();
        assert!(svg.contains("75%") && svg.contains("25%") && svg.contains("FINAL: 4 VOTES"));
        assert_eq!(size(&svg), (900, 470));
        assert_eq!(RatherCard::percent(0, 0), 0);
        assert_eq!(RatherCard::percent(1, 3), 33);
    }

    #[test]
    fn prompt_cards_grow_with_their_text() {
        let short = PromptCard {
            tone: PromptTone::Turn,
            name: "Ada",
            text: "Truth or dare?",
            category: "",
            footer: "Only Ada can pick",
            theme: Theme::reef(),
        };
        let long = PromptCard {
            tone: PromptTone::Dare,
            text: "Describe your perfect cozy night in four sentences, and make every one of them rhyme with the last.",
            category: "Cozy",
            ..PromptCard { name: "Ada", ..short }
        };
        let (short_svg, long_svg) = (
            PromptCard {
                tone: PromptTone::Truth,
                name: "Ada",
                text: "Truth or dare?",
                category: "Ocean",
                footer: "x",
                theme: Theme::reef(),
            }
            .to_svg(),
            long.to_svg(),
        );
        assert!(short_svg.contains(">TRUTH<") && long_svg.contains(">DARE<") && long_svg.contains("COZY"));
        assert!(size(&long_svg).1 > size(&short_svg).1);
    }
}
