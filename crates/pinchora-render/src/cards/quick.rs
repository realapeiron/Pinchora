//! Illustrated quick games. Rules supply visible cells; this module only presents them.
use super::{PIXEL, SANS, boards, checkers, text};
use crate::{
    Theme,
    sprites::SpriteStore,
    svg::{SHELL, background, escape, mix, notched_path, panel, sprite, truncate_to, wrap_laid_out},
};
use pinchora_core::i18n::Lang;
use std::{fmt::Write, sync::OnceLock};

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
    fn words<'a>(&self, en: &'a str, tr: &'a str) -> &'a str {
        if self.lang == Lang::Tr { tr } else { en }
    }

    fn label(&self, s: &mut String, x: f32, y: f32, width: f32, body: &str, color: &str, anchor: &str) {
        boards::fitted_pixel_text(s, (x, y), width, color, anchor, &self.lang.upper(body));
    }

    fn prose(&self, s: &mut String, x: f32, y: f32, width: f32, body: &str, size: f32, color: &str, lines: usize) {
        for (i, line) in wrap_laid_out(body, SANS, true, size, width, lines).iter().enumerate() {
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
                self.words("Your reef collection", "Resif koleksiyonun")
            } else {
                self.words("Find the matching reef friends", "Resif dostlarını eşleştir")
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
                &format!("{}{}", (b'A' + (i / 4) as u8) as char, i % 4 + 1),
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
            self.words("Flag / reveal", "İşaretle / aç"),
            &t.ink,
            "start",
        );
        self.label(
            s,
            536.0,
            137.0,
            210.0,
            if self.details.finished {
                self.words("Reef revealed", "Resif açıldı")
            } else {
                self.words("Watch your step", "Adımına dikkat")
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
                self.words("Sea mines revealed", "Deniz mayınları açıldı").to_owned()
            } else {
                format!(
                    "A–{} + {}",
                    (b'A' + n as u8 - 1) as char,
                    self.words("row · choose a coordinate", "satır · bir koordinat seç")
                )
            },
            &t.muted,
            "middle",
        );
    }

    fn coordinates(&self, s: &mut String, x: f32, y: f32, size: f32, n: usize) {
        let color = boards::coordinate_color(&self.theme);
        for i in 0..n {
            text(
                s,
                x + (i as f32 + 0.5) * size,
                y - 10.0,
                14.0,
                PIXEL,
                false,
                &color,
                "middle",
                &((b'A' + i as u8) as char).to_string(),
            );
            text(
                s,
                x - 10.0,
                y + (i as f32 + 0.5) * size + 5.0,
                14.0,
                PIXEL,
                false,
                &color,
                "middle",
                &(i + 1).to_string(),
            );
        }
    }

    fn othello(&self, s: &mut String) {
        let t = &self.theme;
        for side in 0..2 {
            let count = self
                .cells
                .iter()
                .filter(|c| c.as_str() == if side == 0 { "O" } else { "X" })
                .count();
            let active = self.details.turn == Some(side);
            let won = self.details.winner == Some(side);
            let state = if won {
                self.words("Winner", "Kazanan")
            } else if active {
                self.words("Your move", "Sıra sende")
            } else {
                self.words("Pearls", "İnci")
            };
            let note = format!("{count:02} · {}", self.lang.upper(state));
            let name = self.details.names[side as usize];
            boards::player_panel(
                s,
                t,
                (24.0 + side as f32 * 270.0, 110.0),
                (active, won),
                if name.is_empty() {
                    if side == 0 {
                        self.words("White pearl", "Beyaz inci")
                    } else {
                        self.words("Black pearl", "Siyah inci")
                    }
                } else {
                    name
                },
                (&escape(&note), if active { &t.mint } else { &t.gold }),
                |s, x, y| pearl(s, x, y + 3.0, 3.0, side == 0),
            );
        }
        panel(s, 30.0, 202.0, 520.0, 496.0, 6.0, "#1a2a66", &t.panel_edge);
        let (ox, oy, size) = (58.0, 230.0, 58.0);
        self.coordinates(s, ox, oy, size, 8);
        for (i, cell) in self.cells.iter().take(64).enumerate() {
            let (x, y) = (ox + (i % 8) as f32 * size, oy + (i / 8) as f32 * size);
            let base = if (i / 8 + i % 8) % 2 == 0 { "#245658" } else { "#1c454c" };
            rect(s, x, y, size, size, base);
            rect(s, x, y, size, 1.0, "#387074");
            rect(s, x, y, 1.0, size, "#387074");
            if self.details.last == Some(i) {
                rect(s, x, y, size, size, &mix(base, &t.gold, 0.4));
            }
            match cell.as_str() {
                "O" | "X" => pearl(s, x + 8.0, y + 8.0, 3.0, cell == "O"),
                "*" if !self.details.finished => {
                    let _ = write!(s, r#"<g opacity="0.45">"#);
                    pearl(s, x + 22.0, y + 22.0, 1.0, self.details.turn == Some(0));
                    s.push_str("</g>");
                    rim(s, x + 20.0, y + 20.0, 18.0, &t.mint);
                }
                _ => {}
            }
        }
        self.label(
            s,
            290.0,
            714.0,
            500.0,
            if self.details.finished {
                self.words("Final pearl count", "Son inci sayısı")
            } else {
                self.words("Glowing dots are legal moves", "Parlayan noktalar geçerli hamleler")
            },
            &t.muted,
            "middle",
        );
    }

    fn higher(&self, s: &mut String) {
        let t = &self.theme;
        panel(s, 132.0, 124.0, 202.0, 248.0, 6.0, "#a88d5f", &t.gold);
        panel(s, 128.0, 120.0, 202.0, 244.0, 6.0, PAPER, &t.gold);
        self.label(s, 147.0, 152.0, 80.0, self.prompt, INK, "start");
        self.label(s, 309.0, 345.0, 80.0, self.prompt, INK, "end");
        let face = self.prompt.parse::<usize>().unwrap_or(1).saturating_sub(1) % FACES.len();
        art(s, FACES[face], 157.0, 168.0, 144.0, 110.0);
        text(s, 229.0, 323.0, 46.0, PIXEL, false, INK, "middle", &escape(self.prompt));
        self.back(s, 410.0, 120.0, 202.0, 244.0);
        sprite(
            s,
            &[
                "...#...", "..###..", ".#####.", "#######", "..###..", "..###..", "..###..",
            ],
            353.0,
            181.0,
            5.0,
            &t.mint,
            PAPER,
        );
        sprite(
            s,
            &[
                "..###..", "..###..", "..###..", "#######", ".#####.", "..###..", "...#...",
            ],
            353.0,
            268.0,
            5.0,
            &t.accent,
            PAPER,
        );
        self.label(
            s,
            229.0,
            395.0,
            230.0,
            self.words("Current card", "Açık kart"),
            &t.muted,
            "middle",
        );
        self.label(
            s,
            511.0,
            395.0,
            230.0,
            self.words("Higher or lower?", "Yüksek mi, düşük mü?"),
            &t.mint,
            "middle",
        );
        for i in 0..8 {
            rect(
                s,
                342.0 + i as f32 * 8.0,
                335.0,
                5.0,
                8.0,
                if i < self.details.progress {
                    &t.gold
                } else {
                    &t.panel_edge
                },
            );
        }
    }

    fn anagram(&self, s: &mut String) {
        let t = &self.theme;
        self.label(
            s,
            370.0,
            140.0,
            620.0,
            self.words("A word washed ashore", "Kıyıya vuran bir kelime"),
            &t.gold,
            "middle",
        );
        let word = self.lang.upper(self.prompt);
        let letters: Vec<char> = word.chars().take(20).collect();
        let count = letters.len().max(1);
        let cols = count.min(10);
        let size = (620.0 / cols as f32).min(82.0).floor();
        for (row, chunk) in letters.chunks(cols).enumerate() {
            let x0 = (740.0 - size * chunk.len() as f32) / 2.0;
            for (i, c) in chunk.iter().enumerate() {
                let (x, y) = (x0 + i as f32 * size, 176.0 + row as f32 * 100.0);
                panel(s, x + 3.0, y + 6.0, size - 6.0, 78.0, 4.0, "#a88d5f", "#7d6a4a");
                panel(s, x + 3.0, y, size - 6.0, 76.0, 4.0, PAPER, "#e8d3a3");
                text(
                    s,
                    x + size / 2.0,
                    y + 49.0,
                    36.0,
                    PIXEL,
                    false,
                    INK,
                    "middle",
                    &escape(&c.to_string()),
                );
                rect(s, x + 10.0, y + 66.0, size - 23.0, 2.0, "#d2b77f");
                if count <= 10 {
                    rect(s, x + 14.0, 315.0, size - 28.0, 4.0, &t.muted);
                }
            }
        }
        self.label(
            s,
            370.0,
            391.0,
            620.0,
            self.words("Rearrange every letter", "Tüm harfleri yeniden sırala"),
            &t.mint,
            "middle",
        );
    }

    fn typing(&self, s: &mut String) {
        let t = &self.theme;
        panel(s, 45.0, 143.0, 656.0, 162.0, 5.0, "#a88d5f", "#7d6a4a");
        panel(s, 40.0, 136.0, 656.0, 162.0, 5.0, PAPER, "#e8d3a3");
        for i in 0..4 {
            rect(s, 58.0, 170.0 + i as f32 * 32.0, 620.0, 1.0, "#d2b77f");
        }
        rect(s, 76.0, 147.0, 2.0, 140.0, "#e6a99b");
        let lines = wrap_laid_out(self.prompt, SANS, true, 23.0, 574.0, 3);
        for (row, line) in lines.iter().enumerate() {
            let y = 226.0 - lines.len().saturating_sub(1) as f32 * 19.0 + row as f32 * 38.0;
            // Keep one shaped text run so spaces, kerning and Turkish glyph advances remain intact.
            // SVG's per-character rotations and relative offsets add a gentle wave to the paper.
            let mut rotations = String::new();
            let mut offsets = String::new();
            let mut previous = 0;
            for (i, _) in line.chars().enumerate() {
                let angle = if self.distort {
                    (i as i32 % 5 - 2) as f32 * 0.7
                } else {
                    0.0
                };
                let offset = if self.distort { i as i32 % 3 - 1 } else { 0 };
                let _ = write!(rotations, "{angle} ");
                let _ = write!(offsets, "{} ", offset - previous);
                previous = offset;
            }
            let _ = write!(
                s,
                r#"<text x="96" y="{y}" font-family="{SANS}" font-size="23" font-weight="bold" fill="{INK}" rotate="{rotations}" dy="{offsets}" xml:space="preserve">{}</text>"#,
                escape(line)
            );
        }
        self.label(
            s,
            44.0,
            337.0,
            480.0,
            self.words("Type the sentence exactly", "Cümleyi aynen yaz"),
            &t.mint,
            "start",
        );
        let speed = self.details.wpm.map_or_else(|| "— WPM".into(), |n| format!("{n} WPM"));
        self.label(s, 695.0, 337.0, 170.0, &speed, &t.gold, "end");
        for i in 0..30 {
            rect(
                s,
                44.0 + i as f32 * 22.0,
                354.0,
                17.0,
                18.0,
                if self.details.wpm.is_some_and(|n| i * 4 < n) {
                    if i < 15 { &t.mint } else { &t.gold }
                } else {
                    &t.panel_edge
                },
            );
        }
        self.label(s, 44.0, 397.0, 120.0, "0", &t.muted, "start");
        self.label(s, 695.0, 397.0, 160.0, "120 WPM", &t.muted, "end");
    }

    fn guess(&self, s: &mut String) {
        let t = &self.theme;
        panel(s, 50.0, 123.0, 208.0, 277.0, 5.0, &t.panel, &t.panel_edge);
        // Stepped glass thermometer with a square bulb and calibrated ticks.
        panel(s, 125.0, 158.0, 40.0, 173.0, 6.0, PAPER, "#8fa0d8");
        panel(s, 112.0, 313.0, 66.0, 54.0, 10.0, PAPER, "#8fa0d8");
        let (level, color, label) = match self.details.heat {
            Some(QuickHeat::Cold) => (30.0, "#5bc0eb", self.words("Cold", "Soğuk")),
            Some(QuickHeat::Warm) => (86.0, "#ffd166", self.words("Warm", "Ilık")),
            Some(QuickHeat::Hot) => (142.0, "#ff6b4a", self.words("Hot", "Sıcak")),
            Some(QuickHeat::Found) => (154.0, "#06d6a0", self.words("Found!", "Buldun!")),
            None => (0.0, "#8fa0d8", self.words("Awaiting a guess", "Tahmin bekleniyor")),
        };
        rect(s, 136.0, 324.0 - level, 18.0, level + 10.0, color);
        panel(s, 123.0, 325.0, 44.0, 30.0, 6.0, color, color);
        rect(s, 128.0, 330.0, 6.0, 8.0, PAPER);
        for i in 0..7 {
            rect(
                s,
                174.0,
                173.0 + i as f32 * 22.0,
                if i % 2 == 0 { 18.0 } else { 10.0 },
                3.0,
                &t.muted,
            );
        }
        self.label(s, 154.0, 147.0, 180.0, self.words("Hot", "Sıcak"), &t.accent, "middle");
        self.label(s, 154.0, 390.0, 180.0, self.words("Cold", "Soğuk"), "#5bc0eb", "middle");
        self.label(
            s,
            478.0,
            159.0,
            370.0,
            self.words("A secret between 1–100", "1–100 arasında bir sır"),
            &t.muted,
            "middle",
        );
        text(
            s,
            478.0,
            265.0,
            76.0,
            PIXEL,
            false,
            &t.ink,
            "middle",
            &escape(self.prompt),
        );
        self.label(s, 478.0, 319.0, 370.0, label, color, "middle");
        self.prose(
            s,
            478.0,
            365.0,
            360.0,
            self.words("Every guess brings a new clue.", "Her tahmin yeni bir ipucu getirir."),
            18.0,
            &t.muted,
            2,
        );
    }

    fn question(&self, s: &mut String) {
        let t = &self.theme;
        panel(s, 69.0, 141.0, 610.0, 220.0, 6.0, "#2e3d75", &t.panel_edge);
        panel(s, 60.0, 132.0, 610.0, 220.0, 6.0, PAPER, "#e8d3a3");
        sprite(
            s,
            &["###.###", "#...#..", "#...#.."],
            84.0,
            154.0,
            5.0,
            "#c98a1e",
            PAPER,
        );
        self.prose(s, 370.0, 221.0, 518.0, self.prompt, 28.0, INK, 3);
        self.label(
            s,
            228.0,
            392.0,
            260.0,
            self.words("I have", "Yaptım"),
            &t.accent,
            "middle",
        );
        self.label(
            s,
            512.0,
            392.0,
            260.0,
            self.words("I have never", "Hiç yapmadım"),
            &t.mint,
            "middle",
        );
    }

    fn fortune(&self, s: &mut String) {
        let t = &self.theme;
        panel(s, 40.0, 120.0, 240.0, 280.0, 6.0, "#243066", &t.panel_edge);
        if self.kind == QuickKind::Roll {
            let n = self
                .prompt
                .split('/')
                .next()
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(0);
            panel(s, 91.0, 191.0, 144.0, 140.0, 10.0, "#a88d5f", "#7d6a4a");
            panel(s, 83.0, 179.0, 144.0, 140.0, 10.0, PAPER, "#e8d3a3");
            if (1..=6).contains(&n) {
                let dots: &[(f32, f32)] = match n {
                    1 => &[(1.0, 1.0)],
                    2 => &[(0.0, 0.0), (2.0, 2.0)],
                    3 => &[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)],
                    4 => &[(0.0, 0.0), (2.0, 0.0), (0.0, 2.0), (2.0, 2.0)],
                    5 => &[(0.0, 0.0), (2.0, 0.0), (1.0, 1.0), (0.0, 2.0), (2.0, 2.0)],
                    _ => &[(0.0, 0.0), (2.0, 0.0), (0.0, 1.0), (2.0, 1.0), (0.0, 2.0), (2.0, 2.0)],
                };
                for (x, y) in dots {
                    panel(s, 106.0 + x * 38.0, 202.0 + y * 36.0, 20.0, 20.0, 3.0, INK, INK);
                }
            } else {
                text(s, 155.0, 270.0, 48.0, PIXEL, false, INK, "middle", &n.to_string());
            }
        } else {
            for (x, y) in [(67.0, 170.0), (237.0, 190.0), (225.0, 325.0)] {
                sprite(s, &[".#.", "###", ".#."], x, y, 4.0, &t.gold, PAPER);
            }
            art(s, "c_creamcurl_whelk", 64.0, 177.0, 192.0, 163.0);
        }
        self.label(
            s,
            160.0,
            375.0,
            208.0,
            match self.kind {
                QuickKind::Roll => self.words("Dice roll", "Zar atışı"),
                QuickKind::Choose => self.words("Chosen for you", "Senin için seçildi"),
                _ => self.words("The conch speaks", "Kabuk konuşuyor"),
            },
            &t.gold,
            "middle",
        );
        self.label(
            s,
            496.0,
            156.0,
            380.0,
            self.words("The reef has answered", "Resif yanıt verdi"),
            &t.mint,
            "middle",
        );
        self.prose(s, 496.0, 230.0, 370.0, self.prompt, 30.0, &t.ink, 4);
        sprite(s, SHELL, 478.0, 380.0, 4.0, &t.gold, PAPER);
    }
}
