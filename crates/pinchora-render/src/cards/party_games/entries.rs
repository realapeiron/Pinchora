use super::*;

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
    /// The language of the labels on the card.
    pub lang: Lang,
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
        background(&mut s, WIDTH, h, t);
        heading(&mut s, t, self.lang, self.title, self.chip);
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
                let label = t!(self.lang, "party.card.votes", count = votes);
                text(&mut s, 846.0, y + 56.0, 14.0, PIXEL, false, &t.mint, "end", &label);
            }
            if row.winner {
                sprite(&mut s, STAR, 600.0, y + 24.0, 3.0, &t.gold, "#fff1e8");
            }
        }
        footer(&mut s, t, self.lang, h - 16.0, self.footer);
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
    /// The language of the labels on the card.
    pub lang: Lang,
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
        background(&mut s, w, h, t);
        heading(&mut s, t, self.lang, self.title, "");
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
                RacerState::Solved(tries) => (t!(self.lang, "wordrace.card.solved", count = tries), &t.mint),
                RacerState::Failed => (t!(self.lang, "wordrace.card.failed"), &t.muted),
                RacerState::Playing => (
                    t!(
                        self.lang,
                        "wordrace.card.left",
                        count = 6usize.saturating_sub(racer.rows.len())
                    ),
                    &t.gold,
                ),
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
        footer(&mut s, t, self.lang, 420.0, self.footer);
        s.push_str("</svg>");
        s
    }
}

// /Postcard

/// A postcard: the scene (960×540 PNG) on cream paper with a stamp, a postmark and the place it greets from.
pub struct PostcardCard<'a> {
    /// The language of the labels on the card.
    pub lang: Lang,
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
        text(
            &mut s,
            44.0,
            58.0,
            18.0,
            PIXEL,
            false,
            red,
            "start",
            &escape(&t!(self.lang, "postcard.card.greetings")),
        );
        text(
            &mut s,
            44.0,
            112.0,
            48.0,
            PIXEL,
            true,
            ink,
            "start",
            &escape(&truncate_pixel(&self.lang.upper(self.place), 48.0, 640.0)),
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
            &escape(&self.lang.upper(self.stamp)),
        );
        // A postmark with wavy lines beside it.
        let _ = write!(
            s,
            r##"<circle cx="830" cy="58" r="34" fill="none" stroke="{ink}" stroke-opacity="0.55" stroke-width="3"/><path d="M650,40 h120 M650,56 h120 M650,72 h120" stroke="{ink}" stroke-opacity="0.45" stroke-width="3" stroke-dasharray="12 6"/>"##
        );
        text(&mut s, 830.0, 54.0, 10.0, PIXEL, false, ink, "middle", "PINCHORA");
        text(
            &mut s,
            830.0,
            70.0,
            10.0,
            PIXEL,
            false,
            ink,
            "middle",
            &escape(&t!(self.lang, "postcard.card.post")),
        );
        s.push_str("</svg>");
        s
    }
}
