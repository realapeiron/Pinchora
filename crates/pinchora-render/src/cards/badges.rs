use std::fmt::Write;

use super::{BadgeIcon, PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{
        CRAB, SHELL, background, compact, escape, mix, notched_path, pixel_width, segment_bar, sprite, thousands, wrap,
    },
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
    /// The NEW tag's gold: the pixel font at 8 px (a font pixel is a pixel of the card) spells NEW
    /// in 17×5 pixels; the gold leaves 6 pixels either side and 4 above and below.
    const NEW_W: f32 = 29.0;
    const NEW_H: f32 = 13.0;
    /// The disc under the worn badge's place number, which is set at 16 px: a digit is 10 pixels
    /// tall, and the disc leaves 6 around it.
    const WORN_DISC: f32 = 22.0;

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
        let (x, y) = Self::new_tag_origin(x0 + Self::CELL_W / 2.0 - BadgeIcon::PLATE / 2.0, y0 + Self::PLATE_TOP);
        (
            (x - 2.0) as u32..(x + Self::NEW_W + 2.0) as u32,
            (y - 2.0) as u32..(y + Self::NEW_H + 2.0) as u32,
        )
    }

    /// The top-left of the NEW tag's gold, for a plate whose top-left is `(plate_x, plate_y)`: on the
    /// plate's top-right corner, reaching a little past it. Whole pixels, so the text stays crisp.
    fn new_tag_origin(plate_x: f32, plate_y: f32) -> (f32, f32) {
        (plate_x + BadgeIcon::PLATE - 24.0, plate_y - 7.0)
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
                    let (nx, ny) = Self::new_tag_origin(plate_x, plate_y);
                    let _ = write!(
                        s,
                        r##"<path d="{}" fill="#0b1029"/><path d="{}" fill="{}"/>"##,
                        notched_path(nx - 2.0, ny - 2.0, Self::NEW_W + 4.0, Self::NEW_H + 4.0, 3.0),
                        notched_path(nx, ny, Self::NEW_W, Self::NEW_H, 3.0),
                        t.gold,
                    );
                    // The ink starts one font pixel after the text's x, and sits on its baseline.
                    text(s, nx + 5.0, ny + 9.0, 8.0, PIXEL, false, "#0b1029", "start", "NEW");
                }
                if let Some(place) = worn {
                    let disc = Self::WORN_DISC;
                    let (wx, wy) = (plate_x - 7.0, plate_y + BadgeIcon::PLATE - 16.0);
                    let _ = write!(
                        s,
                        r##"<path d="{}" fill="#0b1029"/><path d="{}" fill="{}"/>"##,
                        notched_path(wx - 2.0, wy - 2.0, disc + 4.0, disc + 4.0, 3.0),
                        notched_path(wx, wy, disc, disc, 3.0),
                        t.mint,
                    );
                    // Centred on whole pixels: a digit's ink is its advance less a font pixel of
                    // space on either side, and starts one font pixel (two pixels at 16 px) in.
                    let digit = place.to_string();
                    let ink = pixel_width(&digit, 16.0) - 4.0;
                    let x = wx + ((disc - ink) / 2.0).floor() - 2.0;
                    text(s, x, wy + 16.0, 16.0, PIXEL, false, "#0b1029", "start", &digit);
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::Renderer;

    fn cell(state: BadgeState) -> BadgeCell<'static> {
        BadgeCell {
            icon: BadgeIcon {
                art: None,
                letter: 'L',
                color: 0x06d6a0,
                rarity: 0x8fa0d8,
            },
            name: "Little Beacon",
            rarity: "Common",
            state,
        }
    }

    /// The colours inside a box of the rendered collection.
    fn colours(image: &image::RgbaImage, x: std::ops::Range<u32>, y: std::ops::Range<u32>) -> HashSet<[u8; 4]> {
        y.flat_map(|y| x.clone().map(move |x| image.get_pixel(x, y).0))
            .collect()
    }

    #[test]
    fn the_new_tag_and_the_worn_number_are_set_in_crisp_pixels() {
        let cells = [
            cell(BadgeState::Earned {
                new: true,
                worn: Some(4),
            }),
            cell(BadgeState::Earned {
                new: false,
                worn: Some(1),
            }),
        ];
        let card = BadgeCollectionCard {
            cells: &cells,
            legend: &[],
            theme: Theme::reef(),
        };
        let png = Renderer::new().badge_collection(&card).expect("renders");
        let image = image::load_from_memory(&png).expect("a png").to_rgba8();
        let gold = [255, 209, 102, 255];
        let mint = [6, 214, 160, 255];
        let ink = [11, 16, 41, 255];

        // The tag's gold holds its text and nothing else: no blended pixels, so nothing muddy.
        let (xs, ys) = BadgeCollectionCard::new_tag_box(0);
        let inside = (xs.start + 5..xs.end - 5, ys.start + 5..ys.end - 5);
        assert_eq!(colours(&image, inside.0, inside.1), HashSet::from([gold, ink]));

        // The same for the worn number's disc, for a wide digit and a narrow one.
        for (index, place) in [(0, 4u8), (1, 1)] {
            let (x0, y0) = BadgeCollectionCard::cell_origin(index);
            let plate_x = x0 + BadgeCollectionCard::CELL_W / 2.0 - BadgeIcon::PLATE / 2.0;
            let plate_y = y0 + BadgeCollectionCard::PLATE_TOP;
            let (wx, wy) = (plate_x - 7.0, plate_y + BadgeIcon::PLATE - 16.0);
            let (xs, ys) = ((wx as u32 + 3)..(wx as u32 + 19), (wy as u32 + 3)..(wy as u32 + 19));
            let shades = colours(&image, xs.clone(), ys.clone());
            assert_eq!(shades, HashSet::from([mint, ink]), "the disc of place {place}");
            // The digit is centred: as much mint to its left as to its right.
            let dark = |x: u32| ys.clone().any(|y| image.get_pixel(x, y).0 == ink);
            let (first, last) = (
                xs.clone().find(|x| dark(*x)).expect("ink"),
                xs.clone().rev().find(|x| dark(*x)).expect("ink"),
            );
            let (left, right) = (first - xs.start, xs.end - 1 - last);
            assert!(
                left.abs_diff(right) <= 1,
                "place {place}: {left} px left, {right} px right"
            );
        }
    }
}
