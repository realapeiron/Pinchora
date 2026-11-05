use std::fmt::Write;

use super::{BadgeIcon, PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{CRAB, SHELL, background, compact, escape, mix, notched_path, segment_bar, sprite, thousands, wrap},
};

/// Where one badge of the collection stands for the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeState {
    /// Theirs. `new`: earned since they last looked. `worn`: the 1-based place on their profile card.
    Earned { new: bool, worn: Option<u8> },
    /// An achievement not reached yet: `have` of `at`.
    Locked { have: i64, at: i64 },
    /// A shop badge not bought yet.
    ForSale { price: i64 },
}

/// One badge of the collection card.
pub struct BadgeCell<'a> {
    pub icon: BadgeIcon<'a>,
    pub name: &'a str,
    /// The rarity as a word, e.g. `Rare`.
    pub rarity: &'a str,
    pub state: BadgeState,
}

/// The badge collection: every badge in a grid, earned ones in colour, locked ones dark with
/// their progress, a counter of how many are earned and a NEW marker on the latest. 960 px wide.
pub struct BadgeCollectionCard<'a> {
    pub cells: &'a [BadgeCell<'a>],
    /// Rarity words and their plate colours (`0xrrggbb`), in the order the footer lists them.
    pub legend: &'a [(&'a str, u32)],
    pub theme: Theme,
}

impl BadgeCollectionCard<'_> {
    pub const WIDTH: u32 = 960;
    const COLUMNS: usize = 8;
    const MARGIN: f32 = 24.0;
    const CELL_W: f32 = 114.0;
    const CELL_H: f32 = 134.0;
    const TOP: f32 = 100.0;
    const FOOT: f32 = 56.0;
    /// Where a cell's plate starts below the cell's top.
    const PLATE_TOP: f32 = 8.0;

    /// The card's height for `cells` badges.
    pub fn height_for(cells: usize) -> u32 {
        (Self::TOP + cells.div_ceil(Self::COLUMNS) as f32 * Self::CELL_H + Self::FOOT) as u32
    }

    /// The top-left of the badge at `index`'s cell, in pixels.
    pub fn cell_origin(index: usize) -> (f32, f32) {
        (
            Self::MARGIN + (index % Self::COLUMNS) as f32 * Self::CELL_W,
            Self::TOP + (index / Self::COLUMNS) as f32 * Self::CELL_H,
        )
    }

    /// The pixel box where the NEW tag of the badge at `index` is drawn.
    pub fn new_tag_box(index: usize) -> (std::ops::Range<u32>, std::ops::Range<u32>) {
        let (x0, y0) = Self::cell_origin(index);
        let (x, y) = (
            x0 + Self::CELL_W / 2.0 + BadgeIcon::PLATE / 2.0 - 24.0,
            y0 + Self::PLATE_TOP - 7.0,
        );
        ((x - 2.0) as u32..(x + 30.0) as u32, (y - 2.0) as u32..(y + 16.0) as u32)
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH as f32, Self::height_for(self.cells.len()) as f32);
        let mut s = String::with_capacity(96 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        s.push_str(LOCKED_FILTER);

        let earned = self
            .cells
            .iter()
            .filter(|c| matches!(c.state, BadgeState::Earned { .. }))
            .count();
        text(
            &mut s,
            Self::MARGIN,
            50.0,
            28.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            "BADGES",
        );
        text(
            &mut s,
            w - Self::MARGIN,
            50.0,
            40.0,
            PIXEL,
            false,
            &t.gold,
            "end",
            &format!("{earned}/{}", self.cells.len()),
        );
        segment_bar(
            &mut s,
            Self::MARGIN,
            64.0,
            w - 2.0 * Self::MARGIN,
            24.0,
            earned as f32 / self.cells.len().max(1) as f32,
            40,
            &t.accent,
            "#1f2a57",
            &t.panel_edge,
        );

        for (i, cell) in self.cells.iter().enumerate() {
            let (x0, y0) = Self::cell_origin(i);
            self.cell(&mut s, cell, x0 + Self::CELL_W / 2.0, y0);
        }

        // Footer: what the plate colours mean.
        let foot = h - Self::FOOT + 20.0;
        for (i, (label, color)) in self.legend.iter().enumerate() {
            let x = Self::MARGIN + i as f32 * 134.0;
            let _ = write!(
                s,
                r##"<path d="{}" fill="#{:06x}"/>"##,
                notched_path(x, foot, 14.0, 14.0, 2.0),
                color & 0xff_ffff
            );
            text(
                &mut s,
                x + 22.0,
                foot + 13.0,
                13.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &escape(&label.to_uppercase()),
            );
        }
        sprite(&mut s, CRAB, w - 140.0, foot - 2.0, 3.0, &t.accent, "#fff1e8");
        text(
            &mut s,
            w - Self::MARGIN,
            foot + 13.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            "PINCHORA",
        );
        s.push_str("</svg>");
        s
    }

    /// One badge: its plate, name, and what is left to do about it. `cx` is the cell's middle.
    fn cell(&self, s: &mut String, cell: &BadgeCell<'_>, cx: f32, y0: f32) {
        let t = &self.theme;
        let plate_x = cx - BadgeIcon::PLATE / 2.0;
        let plate_y = y0 + Self::PLATE_TOP;
        let (ink, locked) = match cell.state {
            BadgeState::Earned { .. } => (t.ink.as_str(), false),
            _ => (t.muted.as_str(), true),
        };
        if locked {
            cell.icon.draw_locked(s, plate_x, plate_y);
        } else {
            cell.icon.draw(s, plate_x, plate_y);
        }
        for (n, line) in wrap(cell.name, 13, 2).iter().enumerate() {
            text(
                s,
                cx,
                plate_y + BadgeIcon::PLATE + 18.0 + n as f32 * 16.0,
                14.5,
                SANS,
                !locked,
                ink,
                "middle",
                &escape(line),
            );
        }
        let status_y = plate_y + BadgeIcon::PLATE + 46.0;
        match cell.state {
            BadgeState::Earned { new, worn } => {
                let tint = format!("#{:06x}", cell.icon.rarity & 0xff_ffff);
                text(
                    s,
                    cx,
                    status_y + 8.0,
                    12.0,
                    PIXEL,
                    false,
                    &tint,
                    "middle",
                    &escape(&cell.rarity.to_uppercase()),
                );
                if new {
                    let (nx, ny) = (plate_x + BadgeIcon::PLATE - 24.0, plate_y - 7.0);
                    let _ = write!(
                        s,
                        r##"<path d="{}" fill="#0b1029"/><path d="{}" fill="{}"/>"##,
                        notched_path(nx - 2.0, ny - 2.0, 32.0, 18.0, 3.0),
                        notched_path(nx, ny, 28.0, 14.0, 3.0),
                        t.gold,
                    );
                    text(s, nx + 14.0, ny + 11.0, 10.0, PIXEL, true, "#0b1029", "middle", "NEW");
                }
                if let Some(place) = worn {
                    let (wx, wy) = (plate_x - 5.0, plate_y + BadgeIcon::PLATE - 14.0);
                    let _ = write!(
                        s,
                        r##"<path d="{}" fill="#0b1029"/><path d="{}" fill="{}"/>"##,
                        notched_path(wx - 2.0, wy - 2.0, 22.0, 22.0, 3.0),
                        notched_path(wx, wy, 18.0, 18.0, 3.0),
                        t.mint,
                    );
                    text(
                        s,
                        wx + 9.0,
                        wy + 14.0,
                        12.0,
                        PIXEL,
                        true,
                        "#0b1029",
                        "middle",
                        &place.to_string(),
                    );
                }
            }
            BadgeState::Locked { have, at } => {
                let (bar_w, bar_x) = (84.0, cx - 42.0);
                let fraction = (have.max(0) as f32 / at.max(1) as f32).clamp(0.0, 1.0);
                let tint = format!("#{:06x}", cell.icon.rarity & 0xff_ffff);
                let _ = write!(
                    s,
                    r##"<path d="{}" fill="#2e3d75"/><path d="{}" fill="#0b1029"/>"##,
                    notched_path(bar_x, status_y - 4.0, bar_w, 12.0, 2.0),
                    notched_path(bar_x + 2.0, status_y - 2.0, bar_w - 4.0, 8.0, 1.0),
                );
                let lit = ((bar_w - 6.0) * fraction).floor();
                if lit > 0.0 {
                    let _ = write!(
                        s,
                        r#"<rect x="{}" y="{}" width="{lit}" height="4" fill="{tint}"/>"#,
                        bar_x + 3.0,
                        status_y,
                    );
                }
                text(
                    s,
                    cx,
                    status_y + 20.0,
                    11.0,
                    PIXEL,
                    false,
                    &t.muted,
                    "middle",
                    &format!("{}/{}", compact(have.max(0)), compact(at)),
                );
            }
            BadgeState::ForSale { price } => {
                let label = thousands(price);
                let width = label.chars().count() as f32 * 7.6 + 22.0;
                let x = cx - width / 2.0;
                sprite(s, SHELL, x, status_y - 5.0, 2.4, &t.gold, "#fff7d6");
                text(
                    s,
                    x + 22.0,
                    status_y + 8.0,
                    12.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "start",
                    &label,
                );
            }
        }
    }
}

/// Paints a badge's art as a dark silhouette that keeps its outline and shading.
const LOCKED_FILTER: &str = r#"<defs><filter id="locked" x="0" y="0" width="1" height="1" color-interpolation-filters="sRGB"><feColorMatrix type="matrix" values="0.16 0.16 0.16 0 0.02  0.16 0.16 0.20 0 0.03  0.20 0.20 0.28 0 0.07  0 0 0 0.9 0"/></filter></defs>"#;

/// The plate colour for a locked badge: a hint of its rarity in near-black.
pub(crate) fn locked_tint(rarity: u32) -> String {
    mix(&format!("#{:06x}", rarity & 0xff_ffff), "#0b1029", 0.78)
}
