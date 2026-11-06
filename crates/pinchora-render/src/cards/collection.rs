//! The cards of the collection games: the Tide Pool Dex, a critter drop, a bottle washing ashore,
//! a haul (one find or a whole crab pot) and the crab pot itself.

use std::fmt::Write;

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{
        STAR, background, data_uri_png, escape, mix, notched_path, panel, pixel_shadow, pixel_width, segment_bar,
        sprite, truncate_to, wrap,
    },
};

/// A critter or find as a picture: already scaled up with nearest-neighbour.
#[derive(Clone)]
pub struct EntryPicture {
    pub png: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

/// Draws `art` standing on `bottom`, centred on `cx`.
fn stand(out: &mut String, art: &EntryPicture, cx: f32, bottom: f32, filter: Option<&str>) {
    let filter = filter.map_or(String::new(), |f| format!(r#" filter="url(#{f})""#));
    let _ = write!(
        out,
        r#"<image x="{}" y="{}" width="{}" height="{}"{filter} href="{}"/>"#,
        (cx - art.w as f32 / 2.0).round(),
        (bottom - art.h as f32).round(),
        art.w,
        art.h,
        data_uri_png(&art.png)
    );
}

/// The dark silhouette a creature not found yet is drawn as.
const SILHOUETTE: &str = r#"<defs><filter id="unseen" x="0" y="0" width="1" height="1" color-interpolation-filters="sRGB"><feColorMatrix type="matrix" values="0.14 0.14 0.14 0 0.02  0.14 0.14 0.18 0 0.03  0.18 0.18 0.26 0 0.07  0 0 0 0.95 0"/></filter></defs>"#;

fn hex(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0xff_ffff)
}

/// The title row every collection card shares.
fn header(s: &mut String, t: &Theme, title: &str, right: &str, right_x: f32) {
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
        text(s, right_x, 60.0, 18.0, PIXEL, false, &t.gold, "end", &escape(right));
    }
}

fn open_svg(w: f32, h: f32) -> String {
    let mut s = String::with_capacity(96 * 1024);
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
    );
    s
}

/// A pixel picture from a grid of palette letters; `.` is empty.
fn pixels(out: &mut String, rows: &[&str], x: f32, y: f32, scale: f32, palette: &[(char, &str)]) {
    for (r, row) in rows.iter().enumerate() {
        for (c, ch) in row.chars().enumerate() {
            let Some((_, fill)) = palette.iter().find(|(key, _)| *key == ch) else {
                continue;
            };
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{scale}" height="{scale}" fill="{fill}"/>"#,
                x + c as f32 * scale,
                y + r as f32 * scale
            );
        }
    }
}

// /Drop

/// One of the cards of a critter drop.
pub struct DropCritter {
    pub art: EntryPicture,
    pub name: String,
    /// The rarity as a word.
    pub rarity: String,
    /// The colour of the rarity, `0xrrggbb`.
    pub frame: u32,
    /// Where it lives, as a short phrase.
    pub habitat: String,
    /// Stars around the card: 0 for the common ones, up to 3 for a legendary.
    pub sparkles: u8,
}

/// 1000×460 card with up to three critters side by side, numbered for the buttons under it.
pub struct DropCard {
    pub title: String,
    pub subtitle: String,
    pub cards: Vec<DropCritter>,
    pub footer: String,
    pub theme: Theme,
}

impl DropCard {
    pub const ART_BOX: (u32, u32) = (200, 150);

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (1000.0, 460.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        header(&mut s, t, &self.title, &self.subtitle, 964.0);
        for (i, card) in self.cards.iter().take(3).enumerate() {
            let (x, y) = (36.0 + i as f32 * 312.0, 96.0);
            let frame = hex(card.frame);
            panel(&mut s, x, y, 296.0, 320.0, 4.0, &t.panel, &frame);
            let _ = write!(
                s,
                r#"<rect x="{}" y="{}" width="288" height="6" fill="{frame}"/>"#,
                x + 4.0,
                y + 4.0
            );
            // The number the button carries.
            panel(&mut s, x + 16.0, y + 22.0, 48.0, 48.0, 4.0, "#0b1029", &frame);
            text(
                &mut s,
                x + 40.0,
                y + 56.0,
                28.0,
                PIXEL,
                false,
                &t.ink,
                "middle",
                &(i + 1).to_string(),
            );
            text(
                &mut s,
                x + 280.0,
                y + 52.0,
                14.0,
                PIXEL,
                false,
                &frame,
                "end",
                &escape(&card.rarity.to_uppercase()),
            );
            let cx = x + 148.0;
            pixel_shadow(&mut s, cx, y + 238.0, card.art.w as f32 / 2.2, 8.0, 0.28);
            stand(&mut s, &card.art, cx, y + 232.0, None);
            let stars = [(x + 36.0, y + 118.0), (x + 252.0, y + 98.0), (x + 232.0, y + 190.0)];
            for (sx, sy) in stars.iter().take(card.sparkles.min(3) as usize) {
                sprite(&mut s, STAR, *sx, *sy, 3.0, &t.gold, "#fff7d6");
            }
            text(
                &mut s,
                cx,
                y + 276.0,
                24.0,
                SANS,
                true,
                &t.ink,
                "middle",
                &escape(&truncate_to(&card.name, 24.0, 264.0)),
            );
            text(
                &mut s,
                cx,
                y + 304.0,
                13.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &escape(&card.habitat.to_uppercase()),
            );
        }
        if !self.footer.is_empty() {
            text(
                &mut s,
                w / 2.0,
                446.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &escape(&self.footer),
            );
        }
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}

// /Bottle

/// 800×340 scene: a bottle with a rolled-up note bobbing on the waves, a different glass and cork per
/// `seed`.
pub struct BottleCard {
    pub title: String,
    pub subtitle: String,
    pub seed: u32,
    pub theme: Theme,
}

impl BottleCard {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (800.0, 340.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        header(&mut s, t, &self.title, &self.subtitle, 764.0);
        // The water: bands of waves, lighter towards the surface.
        let sea = "#2c7fb8";
        let _ = write!(
            s,
            r#"<rect x="0" y="190" width="{w}" height="150" fill="{}"/>"#,
            mix(sea, &t.bg_bottom, 0.45)
        );
        for row in 0..6u32 {
            let y = 190.0 + row as f32 * 26.0;
            let shade = mix("#8fd3ff", &t.bg_bottom, 0.45 + row as f32 * 0.08);
            let shift = ((self.seed >> (row * 3)) % 8) as f32 * 8.0;
            let mut x = -shift;
            while x < w {
                let len = 24.0 + ((x as u32 / 8 + row) % 3) as f32 * 8.0;
                let _ = write!(s, r#"<rect x="{x}" y="{y}" width="{len}" height="4" fill="{shade}"/>"#);
                x += len + 40.0 + ((x as u32 / 8 + row * 5) % 4) as f32 * 8.0;
            }
        }
        // The bottle, bobbing a little differently per seed.
        let glass = [
            ("#8fd3ff", "#d6f1ff"),
            ("#6ff0c8", "#c9fff0"),
            ("#c2b3e6", "#e8e0fb"),
            ("#ffd166", "#fff0b8"),
        ][(self.seed % 4) as usize];
        let cork = ["#b97f4a", "#d2b77f", "#ff9e7a"][((self.seed / 4) % 3) as usize];
        let bob = ((self.seed / 12) % 3) as f32 * 4.0;
        let (bx, by) = (150.0, 118.0 + bob);
        pixel_shadow(&mut s, bx + 150.0, 196.0 + bob, 150.0, 6.0, 0.25);
        let ink = "#1b1d2b";
        // The outlines first, so the glass of the body and of the neck meet without a seam.
        let _ = write!(
            s,
            r#"<path d="{}" fill="{ink}"/><rect x="{}" y="{}" width="72" height="56" fill="{ink}"/><rect x="{}" y="{}" width="48" height="64" fill="{ink}"/>"#,
            notched_path(bx - 8.0, by - 8.0, 200.0, 96.0, 16.0),
            bx + 176.0,
            by + 12.0,
            bx + 240.0,
            by + 8.0
        );
        let _ = write!(
            s,
            r#"<path d="{}" fill="{}"/><rect x="{}" y="{}" width="64" height="40" fill="{}"/><rect x="{}" y="{}" width="32" height="48" fill="{cork}"/>"#,
            notched_path(bx, by, 184.0, 80.0, 16.0),
            glass.0,
            bx + 176.0,
            by + 20.0,
            glass.0,
            bx + 248.0,
            by + 16.0
        );
        // The shine, and the note rolled up inside.
        let _ = write!(
            s,
            r##"<rect x="{}" y="{}" width="136" height="8" fill="{}"/><rect x="{}" y="{}" width="64" height="8" fill="{}"/><rect x="{}" y="{}" width="120" height="32" fill="#fff1e8"/><rect x="{}" y="{}" width="96" height="4" fill="#b8a08f"/><rect x="{}" y="{}" width="72" height="4" fill="#b8a08f"/><rect x="{}" y="{}" width="32" height="8" fill="#ffffff"/>"##,
            bx + 24.0,
            by + 12.0,
            glass.1,
            bx + 188.0,
            by + 28.0,
            glass.1,
            bx + 28.0,
            by + 24.0,
            bx + 40.0,
            by + 32.0,
            bx + 40.0,
            by + 44.0,
            bx + 252.0,
            by + 16.0
        );
        // Foam in front of it, and a few sparkles.
        for (fx, fy, fw) in [(150.0, 202.0, 72.0), (330.0, 208.0, 56.0), (260.0, 196.0, 40.0)] {
            let _ = write!(
                s,
                r##"<rect x="{fx}" y="{fy}" width="{fw}" height="6" fill="#ffffff" fill-opacity="0.55"/>"##
            );
        }
        for (sx, sy) in [(560.0, 120.0), (640.0, 150.0), (600.0, 90.0)] {
            sprite(&mut s, STAR, sx, sy, 3.0, &t.gold, "#fff7d6");
        }
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}

// /Dex

/// One slot of a dex page.
pub struct DexCell {
    /// The slot's number in the whole book, from 1.
    pub number: usize,
    pub name: String,
    pub art: EntryPicture,
    /// The colour of its rarity, `0xrrggbb`.
    pub frame: u32,
    /// Whether the player has it; locked slots are dark and nameless.
    pub owned: bool,
    /// How many times they met it.
    pub caught: i32,
}

/// 1000×620 card with up to 18 slots in a 6×3 grid: one page of the Tide Pool Dex.
pub struct DexCard {
    pub title: String,
    /// The tab, e.g. `CRITTERS`.
    pub tab: String,
    /// The owner's name.
    pub owner: String,
    pub found: usize,
    pub total: usize,
    pub page: usize,
    pub pages: usize,
    pub cells: Vec<DexCell>,
    pub footer: String,
    pub theme: Theme,
}

impl DexCard {
    pub const PER_PAGE: usize = 18;
    pub const ART_BOX: (u32, u32) = (96, 80);
    const COLUMNS: usize = 6;
    const CELL: (f32, f32) = (148.0, 148.0);
    const GAP: f32 = 8.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (1000.0, 620.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        s.push_str(SILHOUETTE);
        header(
            &mut s,
            t,
            &self.title,
            &format!("{} {}/{}", self.tab, self.found, self.total),
            964.0,
        );
        let fraction = if self.total == 0 {
            0.0
        } else {
            self.found as f32 / self.total as f32
        };
        segment_bar(
            &mut s,
            36.0,
            70.0,
            300.0,
            20.0,
            fraction,
            15,
            &t.mint,
            "#1d2b53",
            &t.panel_edge,
        );
        text(
            &mut s,
            964.0,
            84.0,
            16.0,
            SANS,
            true,
            &t.muted,
            "end",
            &escape(&truncate_to(&self.owner, 16.0, 420.0)),
        );

        let (cw, ch) = Self::CELL;
        for (i, cell) in self.cells.iter().take(Self::PER_PAGE).enumerate() {
            let (col, row) = ((i % Self::COLUMNS) as f32, (i / Self::COLUMNS) as f32);
            let (x, y) = (36.0 + col * (cw + Self::GAP), 100.0 + row * (ch + Self::GAP));
            let frame = if cell.owned {
                hex(cell.frame)
            } else {
                t.panel_edge.clone()
            };
            panel(&mut s, x, y, cw, ch, 4.0, &t.panel, &frame);
            if cell.owned {
                let _ = write!(
                    s,
                    r#"<rect x="{}" y="{}" width="{}" height="5" fill="{frame}"/>"#,
                    x + 4.0,
                    y + 4.0,
                    cw - 8.0
                );
            }
            text(
                &mut s,
                x + 12.0,
                y + 28.0,
                12.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &format!("#{:02}", cell.number),
            );
            if cell.owned && cell.caught > 1 {
                text(
                    &mut s,
                    x + cw - 12.0,
                    y + 28.0,
                    12.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "end",
                    &format!("x{}", cell.caught),
                );
            }
            let cx = x + cw / 2.0;
            if cell.owned {
                pixel_shadow(&mut s, cx, y + 106.0, cell.art.w as f32 / 2.4, 6.0, 0.25);
            }
            stand(&mut s, &cell.art, cx, y + 102.0, (!cell.owned).then_some("unseen"));
            let (name, ink) = if cell.owned {
                (truncate_to(&cell.name, 14.0, cw - 16.0), &t.ink)
            } else {
                ("???".to_owned(), &t.panel_edge)
            };
            text(&mut s, cx, y + 132.0, 14.0, SANS, true, ink, "middle", &escape(&name));
        }
        if !self.footer.is_empty() {
            text(
                &mut s,
                36.0,
                598.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &escape(&self.footer),
            );
        }
        text(
            &mut s,
            964.0,
            598.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &format!("PAGE {}/{}", self.page + 1, self.pages.max(1)),
        );
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}

// /Haul

/// One thing in a haul.
pub struct HaulTile {
    pub art: EntryPicture,
    pub name: String,
    /// The rarity as a word.
    pub rarity: String,
    /// The colour of its rarity, `0xrrggbb`.
    pub frame: u32,
    /// `NEW`, `x3` and the like; empty for nothing.
    pub tag: String,
    /// What it paid, e.g. `+14 shells`; empty for nothing.
    pub line: String,
    /// Shown only when the haul is this one thing.
    pub flavor: String,
}

/// What a beachcomb, a grabbed card or a crab pot brought: one thing drawn big, or up to eight in a
/// grid.
pub struct HaulCard {
    pub title: String,
    pub subtitle: String,
    pub tiles: Vec<HaulTile>,
    pub footer: String,
    pub theme: Theme,
}

/// The plate behind a haul grid's subtitle as `(x, y, w, h)`: it ends at the card's right margin and holds
/// the text, which is 18 px and ends at 958 on the baseline 60.
fn subtitle_plate(subtitle: &str) -> (f32, f32, f32, f32) {
    let w = pixel_width(subtitle, 18.0) + 24.0;
    (970.0 - w, 38.0, w, 30.0)
}

impl HaulCard {
    pub const SINGLE_ART_BOX: (u32, u32) = (224, 176);
    pub const GRID_ART_BOX: (u32, u32) = (120, 96);
    pub const MAX_TILES: usize = 8;

    pub fn to_svg(&self) -> String {
        if self.tiles.len() == 1 {
            self.single_svg()
        } else {
            self.grid_svg()
        }
    }

    fn single_svg(&self) -> String {
        let t = &self.theme;
        let tile = &self.tiles[0];
        let (w, h) = (760.0, 380.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        header(&mut s, t, &self.title, "", 724.0);
        text(
            &mut s,
            724.0,
            60.0,
            18.0,
            PIXEL,
            false,
            &t.gold,
            "end",
            &escape(&self.subtitle),
        );
        let frame = hex(tile.frame);
        panel(&mut s, 36.0, 92.0, 284.0, 240.0, 4.0, &t.panel, &frame);
        let _ = write!(s, r#"<rect x="40" y="96" width="276" height="6" fill="{frame}"/>"#);
        let cx = 36.0 + 142.0;
        pixel_shadow(&mut s, cx, 306.0, tile.art.w as f32 / 2.3, 8.0, 0.28);
        stand(&mut s, &tile.art, cx, 300.0, None);
        if !tile.tag.is_empty() {
            let chip = pixel_width(&tile.tag, 16.0) + 20.0;
            panel(&mut s, 52.0, 112.0, chip, 30.0, 4.0, "#0b1029", &t.gold);
            text(
                &mut s,
                52.0 + chip / 2.0,
                133.0,
                16.0,
                PIXEL,
                false,
                &t.gold,
                "middle",
                &escape(&tile.tag),
            );
        }
        let x = 352.0;
        text(
            &mut s,
            x,
            138.0,
            32.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&truncate_to(&tile.name, 32.0, 372.0)),
        );
        text(
            &mut s,
            x,
            170.0,
            16.0,
            PIXEL,
            false,
            &frame,
            "start",
            &escape(&tile.rarity.to_uppercase()),
        );
        for (i, line) in wrap(&tile.flavor, 30, 4).iter().enumerate() {
            text(
                &mut s,
                x,
                210.0 + i as f32 * 28.0,
                18.0,
                SANS,
                false,
                &t.muted,
                "start",
                &escape(&truncate_to(line, 18.0, 372.0)),
            );
        }
        if !tile.line.is_empty() {
            panel(&mut s, x, 290.0, 372.0, 42.0, 4.0, "#0b1029", &t.panel_edge);
            text(
                &mut s,
                x + 186.0,
                318.0,
                18.0,
                PIXEL,
                false,
                &t.gold,
                "middle",
                &escape(&tile.line.to_uppercase()),
            );
        }
        if !self.footer.is_empty() {
            text(
                &mut s,
                w / 2.0,
                362.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &escape(&self.footer),
            );
        }
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }

    fn grid_svg(&self) -> String {
        let t = &self.theme;
        let tiles = &self.tiles[..self.tiles.len().min(Self::MAX_TILES)];
        let rows = tiles.len().div_ceil(4).max(1) as f32;
        let (w, h) = (1000.0, 108.0 + rows * 214.0 + 40.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        header(&mut s, t, &self.title, "", 964.0);
        if !self.subtitle.is_empty() {
            // On a plate, because the background's fixed sparkles can land right on the text of a tall card.
            let (px, py, pw, ph) = subtitle_plate(&self.subtitle);
            panel(&mut s, px, py, pw, ph, 4.0, "#0b1029", &t.panel_edge);
            text(
                &mut s,
                958.0,
                60.0,
                18.0,
                PIXEL,
                false,
                &t.gold,
                "end",
                &escape(&self.subtitle),
            );
        }
        for (i, tile) in tiles.iter().enumerate() {
            let in_row = (tiles.len() - (i / 4) * 4).min(4) as f32;
            let row_w = in_row * 224.0 + (in_row - 1.0) * 12.0;
            let x = (w - row_w) / 2.0 + (i % 4) as f32 * 236.0;
            let y = 92.0 + (i / 4) as f32 * 214.0;
            let frame = hex(tile.frame);
            panel(&mut s, x, y, 224.0, 202.0, 4.0, &t.panel, &frame);
            let _ = write!(
                s,
                r#"<rect x="{}" y="{}" width="216" height="6" fill="{frame}"/>"#,
                x + 4.0,
                y + 4.0
            );
            if !tile.tag.is_empty() {
                text(
                    &mut s,
                    x + 14.0,
                    y + 32.0,
                    14.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "start",
                    &escape(&tile.tag),
                );
            }
            text(
                &mut s,
                x + 212.0,
                y + 32.0,
                12.0,
                PIXEL,
                false,
                &frame,
                "end",
                &escape(&tile.rarity.to_uppercase()),
            );
            let cx = x + 112.0;
            pixel_shadow(&mut s, cx, y + 142.0, tile.art.w as f32 / 2.3, 6.0, 0.25);
            stand(&mut s, &tile.art, cx, y + 136.0, None);
            text(
                &mut s,
                cx,
                y + 168.0,
                16.0,
                SANS,
                true,
                &t.ink,
                "middle",
                &escape(&truncate_to(&tile.name, 16.0, 204.0)),
            );
            if !tile.line.is_empty() {
                text(
                    &mut s,
                    cx,
                    y + 190.0,
                    13.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "middle",
                    &escape(&tile.line.to_uppercase()),
                );
            }
        }
        if !self.footer.is_empty() {
            text(
                &mut s,
                w / 2.0,
                h - 16.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &escape(&self.footer),
            );
        }
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}

// /Pot

/// What the crab pot is doing.
#[derive(Debug, Clone, PartialEq)]
pub enum PotState {
    /// Nothing in the water.
    Empty,
    /// Soaking: `left` as a phrase, `fraction` of the way done, `hours` the soak was set for.
    Soaking { left: String, fraction: f32, hours: u32 },
    /// Done; waiting to be hauled in.
    Ready { hours: u32 },
}

/// 800×340 scene: the crab pot on the sea floor with a buoy on the surface, and a panel saying what
/// it is up to.
pub struct PotCard {
    pub level: u8,
    pub max_level: u8,
    pub state: PotState,
    /// How fast this pot catches, e.g. `1.5 catches an hour`.
    pub rate: String,
    /// The next upgrade, e.g. `Level 2 for 400 shells`; empty at the top.
    pub upgrade: String,
    pub theme: Theme,
}

/// The pot: `o` outline, `w` wire, `r` rope, `s` the shadow on the wire, `b` a bait bag.
const POT: &[&str] = &[
    "......oooooooooooo......",
    "....oowwwwwwwwwwwwoo....",
    "...owwwwwwooooowwwwwo...",
    "..owwwwwwo.....owwwwwo..",
    "..owwswwwo.....owwswwo..",
    ".owwwwwwwwoooooowwwwwwo.",
    ".owwswwwwwwwwwwwwwwswwo.",
    ".owwwwwwwwwwwwwwwwwwwwo.",
    ".owswwwwwwwwbbwwwwwwswo.",
    ".owwwwwwwwwwbbwwwwwwwwo.",
    ".oswwwwwwwwwwwwwwwwwwso.",
    "..oooooooooooooooooooo..",
];

const BUOY: &[&str] = &["..oo..", ".orro.", "orrrro", "owwwwo", "orrrro", ".orro.", "..oo.."];

impl PotCard {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (800.0, 340.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        header(
            &mut s,
            t,
            "Crab pot",
            &format!("LEVEL {}/{}", self.level, self.max_level),
            764.0,
        );
        // The sea: a lighter band at the top, the sea floor in sand at the bottom.
        let _ = write!(
            s,
            r##"<rect x="0" y="96" width="{w}" height="150" fill="#2c7fb8" fill-opacity="0.35"/><rect x="0" y="246" width="{w}" height="94" fill="#d2b77f"/><rect x="0" y="246" width="{w}" height="6" fill="#a88d5f"/>"##
        );
        for (x, y) in [
            (60.0, 276.0),
            (150.0, 308.0),
            (250.0, 286.0),
            (300.0, 318.0),
            (120.0, 260.0),
        ] {
            let _ = write!(s, r##"<rect x="{x}" y="{y}" width="10" height="4" fill="#a88d5f"/>"##);
        }
        // Sea weed on the floor.
        for (x, hgt) in [(30.0, 40.0), (46.0, 56.0), (340.0, 48.0), (356.0, 36.0)] {
            let _ = write!(
                s,
                r##"<rect x="{x}" y="{}" width="8" height="{hgt}" fill="#4f9f5a"/>"##,
                250.0 - hgt
            );
        }
        let rope = "#c9a46a";
        let pot_x = 110.0;
        let pot_y = 246.0 - 12.0 * 8.0 + 4.0;
        let soaking = !matches!(self.state, PotState::Empty);
        if soaking {
            // Rope up to the buoy on the surface.
            let _ = write!(
                s,
                r#"<rect x="{}" y="108" width="4" height="{}" fill="{rope}"/>"#,
                pot_x + 94.0,
                pot_y - 108.0
            );
            pixels(
                &mut s,
                BUOY,
                pot_x + 94.0 - 10.0,
                86.0,
                4.0,
                &[('o', "#1b1d2b"), ('r', "#ff6b4a"), ('w', "#fff1e8")],
            );
        }
        pixel_shadow(&mut s, pot_x + 96.0, 252.0, 90.0, 6.0, 0.25);
        pixels(
            &mut s,
            POT,
            pot_x,
            pot_y,
            8.0,
            &[('o', "#1b1d2b"), ('w', "#c9a46a"), ('s', "#8a6b3a"), ('b', "#ff9e7a")],
        );
        if matches!(self.state, PotState::Ready { .. }) {
            for (sx, sy) in [
                (pot_x + 40.0, pot_y - 30.0),
                (pot_x + 130.0, pot_y - 40.0),
                (pot_x + 190.0, pot_y - 16.0),
            ] {
                sprite(&mut s, STAR, sx, sy, 3.0, &t.gold, "#fff7d6");
            }
        }
        // The panel on the right.
        panel(&mut s, 430.0, 96.0, 334.0, 214.0, 4.0, &t.panel, &t.panel_edge);
        let (status, color) = match &self.state {
            PotState::Empty => ("EMPTY", t.muted.clone()),
            PotState::Soaking { .. } => ("SOAKING", t.accent.clone()),
            PotState::Ready { .. } => ("READY TO HAUL", t.mint.clone()),
        };
        text(&mut s, 454.0, 140.0, 24.0, PIXEL, false, &color, "start", status);
        match &self.state {
            PotState::Empty => {
                text(
                    &mut s,
                    454.0,
                    176.0,
                    18.0,
                    SANS,
                    false,
                    &t.ink,
                    "start",
                    "Set it for 1, 4 or 8 hours.",
                );
            }
            PotState::Soaking { left, fraction, hours } => {
                text(
                    &mut s,
                    454.0,
                    176.0,
                    18.0,
                    SANS,
                    false,
                    &t.ink,
                    "start",
                    &escape(&format!("{left} left of {hours} h")),
                );
                segment_bar(
                    &mut s,
                    454.0,
                    190.0,
                    286.0,
                    28.0,
                    *fraction,
                    12,
                    &t.accent,
                    "#1d2b53",
                    &t.panel_edge,
                );
            }
            PotState::Ready { hours } => {
                text(
                    &mut s,
                    454.0,
                    176.0,
                    18.0,
                    SANS,
                    false,
                    &t.ink,
                    "start",
                    &escape(&format!("A {hours} h soak is done.")),
                );
            }
        }
        text(
            &mut s,
            454.0,
            252.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &escape(&self.rate.to_uppercase()),
        );
        let upgrade = if self.upgrade.is_empty() {
            "TOP LEVEL POT".to_owned()
        } else {
            self.upgrade.to_uppercase()
        };
        text(
            &mut s,
            454.0,
            286.0,
            14.0,
            PIXEL,
            false,
            &t.gold,
            "start",
            &escape(&upgrade),
        );
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture() -> EntryPicture {
        EntryPicture {
            png: vec![],
            w: 32,
            h: 32,
        }
    }

    #[test]
    fn a_dex_page_draws_locked_slots_dark_and_nameless() {
        let card = DexCard {
            title: "Tide pool dex".into(),
            tab: "CRITTERS".into(),
            owner: "Sim".into(),
            found: 1,
            total: 2,
            page: 0,
            pages: 1,
            cells: vec![
                DexCell {
                    number: 1,
                    name: "Pebblesnap Crab".into(),
                    art: picture(),
                    frame: 0x8fa0d8,
                    owned: true,
                    caught: 3,
                },
                DexCell {
                    number: 2,
                    name: "Secret".into(),
                    art: picture(),
                    frame: 0xffd166,
                    owned: false,
                    caught: 0,
                },
            ],
            footer: String::new(),
            theme: Theme::reef(),
        };
        let svg = card.to_svg();
        assert!(svg.contains("Pebblesnap Crab") && svg.contains("x3"));
        assert!(!svg.contains("Secret"), "a locked slot never gives its name away");
        assert!(svg.contains("???") && svg.contains(r##"filter="url(#unseen)""##));
    }

    #[test]
    fn a_haul_of_one_is_drawn_big_and_several_in_a_grid() {
        let tile = |name: &str| HaulTile {
            art: picture(),
            name: name.into(),
            rarity: "Rare".into(),
            frame: 0x3a9bdc,
            tag: "NEW".into(),
            line: "+14 shells".into(),
            flavor: "Sleeps through every storm.".into(),
        };
        let one = HaulCard {
            title: "Beachcomb".into(),
            subtitle: String::new(),
            tiles: vec![tile("Sunburst Star")],
            footer: String::new(),
            theme: Theme::reef(),
        };
        let svg = one.to_svg();
        assert!(svg.contains("width=\"760\"") && svg.contains("Sleeps through every storm."));
        let many = HaulCard {
            tiles: (0..11).map(|n| tile(&format!("Critter {n}"))).collect(),
            ..one
        };
        let svg = many.to_svg();
        assert!(
            svg.contains("Critter 7") && !svg.contains("Critter 8"),
            "eight tiles at most"
        );
    }

    #[test]
    fn a_hauls_subtitle_sits_on_a_plate_that_covers_the_backgrounds_sparkle() {
        // The background puts a 4 px sparkle at 93% of the width and 8% of the height. On a card two rows
        // tall that is right where the 18 px subtitle is, so the text has a plate that covers it.
        let subtitle = "8 H SOAK";
        let (px, py, pw, ph) = subtitle_plate(subtitle);
        let text_width = pixel_width(subtitle, 18.0);
        for rows in 1..=2 {
            let (w, h) = (1000.0_f32, 108.0 + rows as f32 * 214.0 + 40.0);
            let (sx, sy) = ((w * 0.93).round(), (h * 0.08).round());
            let on_text = sx + 4.0 > 958.0 - text_width && sx < 958.0 && sy + 4.0 > 47.0 && sy < 60.0;
            let under_plate = sx >= px && sx + 4.0 <= px + pw && sy >= py && sy + 4.0 <= py + ph;
            assert!(
                !on_text || under_plate,
                "{rows} rows: the sparkle at {sx},{sy} shows on the subtitle"
            );
        }
        assert!(px + pw <= 1000.0 && px > 600.0, "the plate stays on the card");
        let tile = || HaulTile {
            art: picture(),
            name: "Pebblesnap Crab".into(),
            rarity: "Common".into(),
            frame: 0x8fa0d8,
            tag: String::new(),
            line: String::new(),
            flavor: String::new(),
        };
        let card = HaulCard {
            title: "Crab pot".into(),
            subtitle: subtitle.into(),
            tiles: vec![tile(), tile()],
            footer: String::new(),
            theme: Theme::reef(),
        };
        assert!(card.to_svg().contains(subtitle));
    }
}
