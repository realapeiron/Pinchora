//! The `/stats` cards: a server's, a member's or a channel's numbers over a stretch of days, with a
//! column for each day, joins against leaves, the busiest hours and the busiest channels and members.
//! Every label comes in already worded (and in capitals where the pixel font draws it), so the card
//! itself knows no language.

use pinchora_core::infra::i18n::Lang;
use std::fmt::Write;

use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::text;
use crate::{
    Renderer, Theme,
    svg::{CRAB, background, compact_in, escape, pixel_width, sprite, thousands_in, truncate_pixel_to, truncate_to},
};

mod charts;
mod details;
mod layout;

/// The chart colours, checked for colour-blind separation on the card's navy panel: messages (and
/// any single series), joins, leaves.
pub const SERIES: [&str; 3] = ["#4a8fe8", "#0f9f77", "#ec5f40"];
/// The busiest-hours shades, quiet to busy, one hue; an hour with nothing is [`HEAT_EMPTY`].
pub const HEAT: [&str; 5] = ["#184f95", "#256abf", "#3987e5", "#6da7ec", "#b7d3f6"];
pub const HEAT_EMPTY: &str = "#1f2a55";
/// Hairlines: grid and baselines.
const GRID: &str = "#24305e";

/// One number at the top of the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatTile {
    /// In capitals, short.
    pub label: String,
    pub value: String,
    /// A short line under the value, e.g. `of 40 members`.
    pub note: Option<String>,
}

/// One row of a ranked list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatRow {
    pub name: String,
    /// What the bar is drawn from.
    pub value: i64,
    /// What the row says on the right, e.g. `1,204`.
    pub shown: String,
}

/// A ranked list: the busiest channels or members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatList {
    pub title: String,
    pub rows: Vec<StatRow>,
    /// Shown when there are no rows.
    pub empty: String,
}

/// Joins above the line and leaves below it, a pair of columns a day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatFlows {
    pub title: String,
    pub joins: Vec<i64>,
    pub leaves: Vec<i64>,
    /// The legend, with the totals in it: `JOINS 12`.
    pub joins_label: String,
    pub leaves_label: String,
}

/// Messages by weekday and hour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatHeat {
    pub title: String,
    /// `[weekday][hour]`, Monday first.
    pub grid: [[i64; 24]; 7],
    /// Short weekday names, Monday first.
    pub weekdays: [String; 7],
    pub less: String,
    pub more: String,
}

/// 900-wide stats card. The member and channel cards have the numbers and the daily columns; the
/// server card adds joins and leaves, the busiest hours and two ranked lists.
#[derive(Debug, Clone)]
pub struct StatsCard {
    pub lang: Lang,
    /// Above the title, in capitals: `SERVER STATS`.
    pub kicker: String,
    pub title: String,
    /// In the chip at the top right, in capitals: `LAST 30 DAYS`.
    pub period: String,
    /// Under the title: the days covered.
    pub range: String,
    /// Up to four.
    pub tiles: Vec<StatTile>,
    pub chart_title: String,
    /// One value a day, oldest first.
    pub days: Vec<i64>,
    /// Under the first and the last column.
    pub first_day: String,
    pub last_day: String,
    /// Drawn over an empty chart.
    pub empty: String,
    pub flows: Option<StatFlows>,
    pub heat: Option<StatHeat>,
    /// None, or two side by side.
    pub lists: Vec<StatList>,
    pub theme: Theme,
}

const W: f32 = 900.0;
const LEFT: f32 = 36.0;
const INNER: f32 = 828.0;
const GAP: f32 = 16.0;
const TILE_H: f32 = 92.0;
const CHART_H: f32 = 216.0;
const FLOWS_H: f32 = 176.0;
const HEAT_H: f32 = 236.0;
const LIST_ROW: f32 = 44.0;
const LIST_ROWS: usize = 5;

/// The smallest of 1, 2, 4, 6 or 8 times a power of ten that is at least `n`: the top of a chart's
/// scale, even (but for 1) so the halfway line is a whole number too.
pub fn nice_max(n: i64) -> i64 {
    if n <= 1 {
        return 1;
    }
    let mut step = 1i64;
    loop {
        for m in [1, 2, 4, 6, 8] {
            if step.saturating_mul(m) >= n {
                return step.saturating_mul(m);
            }
        }
        step = step.saturating_mul(10);
    }
}

/// Which of the [`HEAT`] shades an hour with `n` messages gets when the busiest has `max`; `None`
/// for an hour with none.
pub fn heat_step(n: i64, max: i64) -> Option<usize> {
    if n <= 0 || max <= 0 {
        return None;
    }
    let step = (n as f64 / max as f64 * HEAT.len() as f64).ceil() as usize;
    Some(step.clamp(1, HEAT.len()) - 1)
}

impl StatsCard {
    pub fn height(&self) -> f32 {
        let mut h = 140.0 + TILE_H + GAP + CHART_H + GAP;
        if self.flows.is_some() {
            h += FLOWS_H + GAP;
        }
        if self.heat.is_some() {
            h += HEAT_H + GAP;
        }
        if !self.lists.is_empty() {
            h += Self::list_height() + GAP;
        }
        h + 34.0
    }

    fn list_height() -> f32 {
        56.0 + LIST_ROW * LIST_ROWS as f32 + 8.0
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let h = self.height();
        let mut s = String::with_capacity(64 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{h}" viewBox="0 0 {W} {h}">"#
        );
        background(&mut s, W, h, &t.bg_top, &t.bg_bottom);
        self.header(&mut s);
        let mut y = 140.0;
        self.tiles(&mut s, y);
        y += TILE_H + GAP;
        self.columns(&mut s, y);
        y += CHART_H + GAP;
        if let Some(flows) = &self.flows {
            self.flows(&mut s, flows, y);
            y += FLOWS_H + GAP;
        }
        if let Some(heat) = &self.heat {
            self.heat(&mut s, heat, y);
            y += HEAT_H + GAP;
        }
        if !self.lists.is_empty() {
            let width = (INNER - GAP) / 2.0;
            for (i, list) in self.lists.iter().take(2).enumerate() {
                self.list(&mut s, list, LEFT + i as f32 * (width + GAP), y, width);
            }
        }
        let footer = h - 16.0;
        sprite(&mut s, CRAB, 752.0, footer - 12.0, 2.0, &t.accent, "#fff1e8");
        text(&mut s, 864.0, footer, 14.0, PIXEL, false, &t.muted, "end", "PINCHORA");
        t.draw_frame(&mut s, W, h);
        s.push_str("</svg>");
        s
    }
}
/// A column growing from its baseline at the bottom, its top corners cut by a pixel and lit by a
/// highlight row, the cards' pixel bevel.
fn column(s: &mut String, x: f32, y: f32, w: f32, h: f32, color: &str) {
    let cut = if w >= 6.0 && h >= 4.0 { 2.0 } else { 0.0 };
    let _ = write!(
        s,
        r#"<path d="M{a},{y} H{b} V{c} H{r} V{bot} H{x} V{c} H{a} Z" fill="{color}"/>"#,
        a = x + cut,
        b = x + w - cut,
        c = y + cut,
        r = x + w,
        bot = y + h,
    );
    if h >= 6.0 {
        let _ = write!(
            s,
            r##"<rect x="{}" y="{}" width="{}" height="2" fill="#ffffff" fill-opacity="0.25"/>"##,
            x + cut,
            y,
            (w - 2.0 * cut).max(1.0)
        );
    }
}

impl Renderer {
    pub fn stats_card(&self, card: &StatsCard) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }
}

#[cfg(test)]
mod tests {
    use crate::cards::stats::*;

    /// The sample the golden image and the samples example draw: a busy month in a mid-sized server.
    pub(crate) fn sample() -> StatsCard {
        let days: Vec<i64> = (0..30)
            .map(|d| 40 + (d * 37 % 23) * 9 + if d % 7 == 5 { 160 } else { 0 })
            .collect();
        let mut grid = [[0i64; 24]; 7];
        for (d, hours) in grid.iter_mut().enumerate() {
            for (h, n) in hours.iter_mut().enumerate() {
                *n = if (8..23).contains(&h) {
                    ((h as i64 - 8) * (24 - h as i64) + d as i64 * 3) / 2
                } else {
                    i64::from(h % 3 == 0)
                };
            }
        }
        StatsCard {
            lang: Lang::En,
            kicker: "SERVER STATS".into(),
            title: "Pinchora Reef".into(),
            period: "LAST 30 DAYS".into(),
            range: "8 Oct – 6 Nov 2026, UTC".into(),
            tiles: vec![
                StatTile {
                    label: "MESSAGES".into(),
                    value: "4,321".into(),
                    note: Some("38 members wrote".into()),
                },
                StatTile {
                    label: "VOICE".into(),
                    value: "212h".into(),
                    note: None,
                },
                StatTile {
                    label: "JOINS".into(),
                    value: "57".into(),
                    note: None,
                },
                StatTile {
                    label: "LEAVES".into(),
                    value: "12".into(),
                    note: None,
                },
            ],
            chart_title: "MESSAGES PER DAY".into(),
            days,
            first_day: "8 OCT".into(),
            last_day: "6 NOV".into(),
            empty: "Nothing counted yet".into(),
            flows: Some(StatFlows {
                title: "JOINS AND LEAVES".into(),
                joins: (0..30).map(|d| (d * 5 % 7) as i64).collect(),
                leaves: (0..30).map(|d| (d * 3 % 4 == 0) as i64).collect(),
                joins_label: "JOINS 57".into(),
                leaves_label: "LEAVES 12".into(),
            }),
            heat: Some(StatHeat {
                title: "BUSIEST HOURS, UTC".into(),
                grid,
                weekdays: ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"].map(String::from),
                less: "QUIET".into(),
                more: "BUSY".into(),
            }),
            lists: vec![
                StatList {
                    title: "TOP CHANNELS".into(),
                    rows: [
                        ("general", 2_204),
                        ("memes", 941),
                        ("fishing-spot", 602),
                        ("art", 311),
                        ("help", 263),
                    ]
                    .into_iter()
                    .map(|(name, n)| StatRow {
                        name: format!("#{name}"),
                        value: n,
                        shown: thousands_in(Lang::En, n),
                    })
                    .collect(),
                    empty: "No messages yet".into(),
                },
                StatList {
                    title: "TOP MEMBERS".into(),
                    rows: [
                        ("Coral", 812),
                        ("Bubbles the Very Long Named Hermit Crab", 640),
                        ("Pinch", 233),
                    ]
                    .into_iter()
                    .map(|(name, n)| StatRow {
                        name: name.into(),
                        value: n,
                        shown: thousands_in(Lang::En, n),
                    })
                    .collect(),
                    empty: "No messages yet".into(),
                },
            ],
            theme: Theme::reef(),
        }
    }

    fn golden_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/stats-server.png")
    }

    #[test]
    fn scales_round_up_to_one_two_or_five() {
        assert_eq!(nice_max(0), 1);
        assert_eq!(nice_max(1), 1);
        assert_eq!(nice_max(3), 4);
        assert_eq!(nice_max(5), 6);
        assert_eq!(nice_max(9), 10);
        assert_eq!(nice_max(11), 20);
        assert_eq!(nice_max(200), 200);
        assert_eq!(nice_max(201), 400);
        assert_eq!(nice_max(371), 400);
        assert_eq!(nice_max(i64::MAX), i64::MAX);
    }

    #[test]
    fn busier_hours_get_brighter_shades() {
        assert_eq!(heat_step(0, 10), None);
        assert_eq!(heat_step(5, 0), None);
        assert_eq!(heat_step(1, 100), Some(0));
        assert_eq!(heat_step(10, 10), Some(4));
        assert_eq!(heat_step(50, 100), Some(2));
        assert_eq!(heat_step(61, 100), Some(3));
    }

    #[test]
    fn the_server_card_matches_its_golden_image() {
        let png = Renderer::new().stats_card(&sample()).expect("renders");
        let golden = std::fs::read(golden_path()).expect(
            "tests/golden/stats-server.png is missing: cargo test -p pinchora-render write_stats_golden -- --ignored",
        );
        let (ours, theirs) = (
            image::load_from_memory(&png).unwrap().to_rgba8(),
            image::load_from_memory(&golden).unwrap().to_rgba8(),
        );
        assert_eq!(ours.dimensions(), theirs.dimensions(), "the card changed size");
        assert!(
            ours == theirs,
            "the stats card no longer looks like tests/golden/stats-server.png; if the change is wanted, \
             rewrite it with: cargo test -p pinchora-render write_stats_golden -- --ignored"
        );
    }

    #[test]
    #[ignore = "writes the golden image"]
    fn write_stats_golden() {
        let png = Renderer::new().stats_card(&sample()).expect("renders");
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        std::fs::write(golden_path(), png).unwrap();
    }

    #[test]
    fn small_cards_and_empty_ones_render() {
        let r = Renderer::new();
        let mut me = sample();
        me.flows = None;
        me.heat = None;
        me.lists.clear();
        me.days = vec![0; 7];
        let png = r.stats_card(&me).expect("renders");
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!(image.width(), 900);
        assert!((image.height() as f32 - me.height()).abs() < 1.0);
        assert!(me.height() < sample().height(), "the member card is the short one");
        assert!(me.to_svg().contains("Nothing counted yet"));
        let mut one = sample();
        one.days = vec![5];
        one.lists[1].rows.clear();
        one.title = "<script> & \"friends\"".into();
        let svg = one.to_svg();
        assert!(svg.contains("&lt;script&gt; &amp;"));
        assert!(svg.contains("No messages yet"));
        assert!(r.stats_card(&one).is_ok());
    }
}
