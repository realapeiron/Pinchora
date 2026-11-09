use super::*;

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
    /// The language of the labels; the banner comes translated, and in capitals.
    pub lang: pinchora_core::infra::i18n::Lang,
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
        text(
            &mut s,
            28.0,
            52.0,
            24.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &pinchora_core::t!(self.lang, "share.card.title"),
        );
        chip(
            &mut s,
            w - 28.0,
            26.0,
            &pinchora_core::t!(
                self.lang,
                if self.wager {
                    "share.card.stakes"
                } else {
                    "share.card.sea-pool"
                }
            ),
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
            &pinchora_core::t!(self.lang, "share.card.in-pool"),
        );
        let pool = compact_in(self.lang, self.pool);
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
                PickView::Thinking => (
                    &t.panel_edge,
                    pinchora_core::t!(self.lang, "share.card.thinking"),
                    &t.muted,
                ),
                PickView::Locked => (&t.mint, pinchora_core::t!(self.lang, "share.card.locked"), &t.mint),
                PickView::Share => (&t.mint, pinchora_core::t!(self.lang, "share.card.share"), &t.mint),
                PickView::Grab => (&t.accent, pinchora_core::t!(self.lang, "share.card.grab"), &t.accent),
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
            text(&mut s, x + 80.0, 236.0, 16.0, PIXEL, false, tone, "middle", &label);
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
            &escape(&self.lang.upper(self.banner)),
        );
        s.push_str("</svg>");
        s
    }
}
