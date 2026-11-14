//! Illustrated quick games. Rules supply visible cells; this module only presents them.
use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::boards;
use crate::cards::checkers;
use crate::cards::text;
use crate::{
    Theme,
    sprites::SpriteStore,
    svg::{SHELL, background, escape, mix, notched_path, panel, sprite, truncate_to, wrap_laid_out},
};
use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;
use std::{fmt::Write, sync::OnceLock};

mod board;
mod chance;
mod words;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickKind {
    Memory,
    Minesweeper,
    HigherLower,
    Anagram,
    Typing,
    GuessNumber,
    Othello,
    Nhie,
    Conch,
    Choose,
    Roll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickHeat {
    Cold,
    Warm,
    Hot,
    Found,
}

/// Presentation metadata, independent of the rules and translated strings.
#[derive(Clone, Default)]
pub struct QuickDetails<'a> {
    /// Othello order: white (O), black (X); turn/winner use zero-based indexes.
    pub names: [&'a str; 2],
    pub turn: Option<u8>,
    pub winner: Option<u8>,
    pub last: Option<usize>,
    pub matched: &'a [usize],
    pub finished: bool,
    pub wpm: Option<u32>,
    pub heat: Option<QuickHeat>,
    /// Higher/lower streak, out of eight.
    pub progress: u8,
}

#[derive(Clone)]
pub struct QuickCard<'a> {
    pub kind: QuickKind,
    pub details: QuickDetails<'a>,
    pub lang: Lang,
    pub title: &'a str,
    pub subtitle: &'a str,
    /// Only visible information: memory face IDs 1–8 or ?, mines 1–8 / · / F / * / ?,
    /// Othello O / X / * (legal) / ·. Hidden answers must never be passed here.
    pub cells: &'a [String],
    pub columns: usize,
    pub prompt: &'a str,
    pub status: &'a str,
    pub theme: Theme,
    pub distort: bool,
}

const PAPER: &str = "#f8edd0";
const INK: &str = "#243052";
const FACES: [&str; 8] = [
    "c_pearlpatch_koi",
    "c_plumspot_puffer",
    "c_creamcurl_whelk",
    "p_crumbclaw_crab",
    "p_buttonbell_jelly",
    "c_petalwake_manta",
    "p_pebblepouch_hermit",
    "p_sprinklearm_octopus",
];
const FLAG: &[&str] = &[
    "..x........",
    "..xrrrrrr..",
    "..xrrrrr...",
    "..xrrrr....",
    "..xrrr.....",
    "..x........",
    "..x........",
    "..x........",
    ".xxx.......",
    "xxxxx......",
];
const MINE: &[&str] = &[
    ".....xx......",
    "..x..xx..x...",
    "...xxxxxx....",
    "..xoo####x...",
    ".xoo######x..",
    "xxo#######xxx",
    ".x########x..",
    ".x########x..",
    "..x######x...",
    "...xxxxxx....",
    "..x..xx..x...",
    ".....xx......",
];

fn rect(s: &mut String, x: f32, y: f32, w: f32, h: f32, fill: &str) {
    let _ = write!(s, r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}"/>"#);
}

/// Atlas pixels become integer SVG rectangles, preserving the source palette and hard edges.
/// Cache the atlas once, rather than decoding it for each face of each card.
fn art(s: &mut String, name: &str, x: f32, y: f32, w: f32, h: f32) {
    static STORE: OnceLock<SpriteStore> = OnceLock::new();
    let Some(img) = STORE.get_or_init(SpriteStore::embedded).get(name) else {
        return;
    };
    let scale = (w / img.width() as f32).min(h / img.height() as f32).floor().max(1.0);
    let ox = x + ((w - img.width() as f32 * scale) / 2.0).floor();
    let oy = y + ((h - img.height() as f32 * scale) / 2.0).floor();
    for row in 0..img.height() {
        let mut col = 0;
        while col < img.width() {
            let px = img.get_pixel(col, row);
            let start = col;
            col += 1;
            while col < img.width() && img.get_pixel(col, row) == px {
                col += 1;
            }
            if px[3] == 0 {
                continue;
            }
            let _ = write!(
                s,
                r##"<rect x="{}" y="{}" width="{}" height="{scale}" fill="#{:02x}{:02x}{:02x}" fill-opacity="{}"/>"##,
                ox + start as f32 * scale,
                oy + row as f32 * scale,
                (col - start) as f32 * scale,
                px[0],
                px[1],
                px[2],
                f32::from(px[3]) / 255.0
            );
        }
    }
}

fn pearl(s: &mut String, x: f32, y: f32, scale: f32, white: bool) {
    let rows = checkers::pearl();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let colors = if white {
        ("#fff1e8", "#ffffff", "#d3bccf")
    } else {
        ("#45495c", "#a3a8bd", "#1b1d2b")
    };
    boards::bitmap(s, &rows, x, y, scale, &|c| checkers::pearl_color(c, colors));
}

fn rim(s: &mut String, x: f32, y: f32, size: f32, color: &str) {
    let _ = write!(
        s,
        r#"<path d="{} {}" fill="{color}" fill-rule="evenodd"/>"#,
        notched_path(x, y, size, size, 3.0),
        notched_path(x + 3.0, y + 3.0, size - 6.0, size - 6.0, 2.0)
    );
}

impl QuickCard<'_> {
    fn label(&self, s: &mut String, x: f32, y: f32, width: f32, body: impl AsRef<str>, color: &str, anchor: &str) {
        boards::fitted_pixel_text(s, (x, y), width, color, anchor, &self.lang.upper(body.as_ref()));
    }

    fn prose(
        &self,
        s: &mut String,
        x: f32,
        y: f32,
        width: f32,
        body: impl AsRef<str>,
        size: f32,
        color: &str,
        lines: usize,
    ) {
        for (i, line) in wrap_laid_out(body.as_ref(), SANS, true, size, width, lines)
            .iter()
            .enumerate()
        {
            text(
                s,
                x,
                y + i as f32 * size * 1.45,
                size,
                SANS,
                true,
                color,
                "middle",
                &escape(line),
            );
        }
    }

    pub fn to_svg(&self) -> String {
        let (w, h) = match self.kind {
            QuickKind::Memory => (580.0, 710.0),
            QuickKind::Minesweeper | QuickKind::Othello => (580.0, 800.0),
            _ => (740.0, 500.0),
        };
        let t = &self.theme;
        let mut s = String::with_capacity(64 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        // The same notched frames, pixel lettering and shell emblem as the established game cards.
        panel(&mut s, 24.0, 20.0, w - 48.0, 78.0, 4.0, &t.panel, &t.panel_edge);
        sprite(&mut s, SHELL, w - 78.0, 39.0, 4.0, &t.gold, PAPER);
        let title = self.lang.upper(self.title);
        let title_size = [24.0, 22.0, 20.0, 18.0]
            .into_iter()
            .find(|&n| crate::svg::pixel_width(&title, n) < w - 140.0)
            .unwrap_or(18.0);
        text(
            &mut s,
            42.0,
            53.0,
            title_size,
            PIXEL,
            false,
            &t.ink,
            "start",
            &escape(&crate::svg::truncate_pixel_to(&title, title_size, w - 140.0)),
        );
        text(
            &mut s,
            42.0,
            79.0,
            14.0,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&truncate_to(self.subtitle, 14.0, w - 140.0)),
        );
        match self.kind {
            QuickKind::Memory => self.memory(&mut s),
            QuickKind::Minesweeper => self.mines(&mut s),
            QuickKind::Othello => self.othello(&mut s),
            QuickKind::HigherLower => self.higher(&mut s),
            QuickKind::Anagram => self.anagram(&mut s),
            QuickKind::Typing => self.typing(&mut s),
            QuickKind::GuessNumber => self.guess(&mut s),
            QuickKind::Nhie => self.question(&mut s),
            QuickKind::Conch | QuickKind::Choose | QuickKind::Roll => self.fortune(&mut s),
        }
        panel(&mut s, 24.0, h - 80.0, w - 48.0, 56.0, 4.0, &t.panel, &t.panel_edge);
        rect(
            &mut s,
            40.0,
            h - 60.0,
            5.0,
            16.0,
            if self.details.finished { &t.gold } else { &t.mint },
        );
        self.label(&mut s, 59.0, h - 45.0, w - 102.0, self.status, &t.ink, "start");
        s.push_str("</svg>");
        s
    }

    fn back(&self, s: &mut String, x: f32, y: f32, w: f32, h: f32) {
        panel(s, x, y, w, h, 4.0, "#243066", "#8fa0d8");
        panel(s, x + 7.0, y + 7.0, w - 14.0, h - 14.0, 3.0, "#2e3d75", "#4a5ba3");
        for r in 0..3 {
            for c in 0..3 {
                let (px, py) = (
                    x + 15.0 + c as f32 * (w - 30.0) / 2.0,
                    y + 15.0 + r as f32 * (h - 30.0) / 2.0,
                );
                rect(s, px, py, 3.0, 3.0, "#8fa0d8");
            }
        }
        let scale = if w > 150.0 { 8.0 } else { 4.0 };
        sprite(
            s,
            SHELL,
            x + (w - 7.0 * scale) / 2.0,
            y + (h - 6.0 * scale) / 2.0,
            scale,
            "#ffd166",
            PAPER,
        );
    }

    fn memory(&self, s: &mut String) {
        let t = &self.theme;
        self.label(
            s,
            40.0,
            130.0,
            500.0,
            if self.details.finished {
                t!(self.lang, "quick-card.your-reef-collection")
            } else {
                t!(self.lang, "quick-card.find-the-matching-reef-friends")
            },
            &t.mint,
            "start",
        );
        panel(s, 36.0, 146.0, 508.0, 462.0, 6.0, "#1a2a66", &t.panel_edge);
        for (i, cell) in self.cells.iter().take(16).enumerate() {
            let (x, y) = (54.0 + (i % 4) as f32 * 120.0, 160.0 + (i / 4) as f32 * 108.0);
            let matched = self.details.matched.contains(&i);
            if let Some(face) = cell
                .parse::<usize>()
                .ok()
                .and_then(|n| n.checked_sub(1))
                .filter(|&n| n < FACES.len())
            {
                panel(
                    s,
                    x,
                    y,
                    112.0,
                    98.0,
                    4.0,
                    PAPER,
                    if matched { &t.mint } else { &t.gold },
                );
                art(s, FACES[face], x + 8.0, y + 20.0, 96.0, 74.0);
                if matched {
                    rect(s, x + 94.0, y + 8.0, 8.0, 4.0, &t.mint);
                }
            } else {
                self.back(s, x, y, 112.0, 98.0);
            }
            self.label(
                s,
                x + 9.0,
                y + 15.0,
                38.0,
                format!("{}{}", (b'A' + (i / 4) as u8) as char, i % 4 + 1),
                if cell == "?" { &t.ink } else { INK },
                "start",
            );
            if self.details.last == Some(i) {
                // Four corners mark the selected card without crossing its illustration.
                for (dx, dy) in [(0.0, 0.0), (100.0, 0.0), (0.0, 86.0), (100.0, 86.0)] {
                    rect(s, x + dx, y + dy, 12.0, 4.0, &t.gold);
                    rect(s, x + dx, y + dy, 4.0, 12.0, &t.gold);
                }
            }
        }
    }

    fn mines(&self, s: &mut String) {
        let t = &self.theme;
        let n = self.columns.clamp(1, 10);
        let size = 480.0 / n as f32;
        boards::bitmap(s, FLAG, 43.0, 116.0, 2.0, &|c| match c {
            'x' => Some(PAPER.into()),
            'r' => Some(t.accent.clone()),
            _ => None,
        });
        self.label(
            s,
            76.0,
            137.0,
            210.0,
            t!(self.lang, "quick-card.flag-reveal"),
            &t.ink,
            "start",
        );
        self.label(
            s,
            536.0,
            137.0,
            210.0,
            if self.details.finished {
                t!(self.lang, "quick-card.reef-revealed")
            } else {
                t!(self.lang, "quick-card.watch-your-step")
            },
            &t.gold,
            "end",
        );
        panel(s, 30.0, 158.0, 520.0, 532.0, 6.0, "#1a2a66", &t.panel_edge);
        self.coordinates(s, 50.0, 190.0, size, n);
        let colors = [
            "#245b98", "#216f50", "#af3843", "#573a88", "#925019", "#126e77", "#243052", "#6c4563",
        ];
        for (i, cell) in self.cells.iter().take(n * n).enumerate() {
            let (x, y) = (50.0 + (i % n) as f32 * size, 190.0 + (i / n) as f32 * size);
            let raised = cell == "?" || cell == "F";
            rect(s, x, y, size, size, "#7d6a4a");
            rect(
                s,
                x + 2.0,
                y + 2.0,
                size - 4.0,
                size - 4.0,
                if raised { "#e8d3a3" } else { "#c5b58e" },
            );
            if raised {
                rect(s, x + 2.0, y + 2.0, size - 4.0, 4.0, PAPER);
                rect(s, x + 2.0, y + 2.0, 4.0, size - 4.0, PAPER);
                rect(s, x + 6.0, y + size - 6.0, size - 8.0, 4.0, "#a88d5f");
                rect(s, x + size - 6.0, y + 6.0, 4.0, size - 8.0, "#a88d5f");
                if cell == "?" {
                    rect(s, x + size - 15.0, y + size - 17.0, 3.0, 3.0, "#d2b77f");
                }
            }
            if cell == "F" {
                boards::bitmap(
                    s,
                    FLAG,
                    x + size / 2.0 - 11.0,
                    y + size / 2.0 - 15.0,
                    3.0,
                    &|c| match c {
                        'x' => Some(INK.into()),
                        'r' => Some("#c2413f".into()),
                        _ => None,
                    },
                );
            } else if cell == "*" {
                if self.details.last == Some(i) {
                    rect(s, x + 2.0, y + 2.0, size - 4.0, size - 4.0, "#ff9e7a");
                }
                boards::bitmap(
                    s,
                    MINE,
                    x + (size - 39.0) / 2.0,
                    y + (size - 36.0) / 2.0,
                    3.0,
                    &|c| match c {
                        'x' => Some(INK.into()),
                        '#' => Some("#6a6f86".into()),
                        'o' => Some("#d6f1ff".into()),
                        _ => None,
                    },
                );
            } else if let Ok(number) = cell.parse::<usize>()
                && (1..=8).contains(&number)
            {
                text(
                    s,
                    x + size / 2.0,
                    y + size / 2.0 + 11.0,
                    32.0,
                    PIXEL,
                    false,
                    colors[number - 1],
                    "middle",
                    cell,
                );
            }
            if self.details.last == Some(i) {
                rim(s, x + 1.0, y + 1.0, size - 2.0, &t.mint);
            }
        }
        self.label(
            s,
            290.0,
            710.0,
            500.0,
            &if self.details.finished {
                t!(self.lang, "quick-card.sea-mines-revealed")
            } else {
                format!(
                    "A–{} + {}",
                    (b'A' + n as u8 - 1) as char,
                    t!(self.lang, "quick-card.row-choose-a-coordinate")
                )
            },
            &t.muted,
            "middle",
        );
    }
}
