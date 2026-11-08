//! The cards of the collection games: the Tide Pool Dex, a critter drop, a bottle washing ashore,
//! a haul (one find or a whole crab pot) and the crab pot itself.

use std::fmt::Write;

use pinchora_core::{i18n::Lang, t};

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{
        STAR, background, data_uri_png, escape, fit_laid_out, laid_out_width, mix, notched_path, panel, pixel_shadow,
        pixel_width, segment_bar, sprite, truncate_to, wrap, wrap_laid_out,
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

/// A name that has to fit `max` px on one line: at `size`, or at the largest whole size down to `min` that
/// holds it, so a long name (Turkish ones run long) is shrunk before it is cut; only a name too long for
/// `min` ends with an ellipsis. Returns the text and the size to draw it at.
fn fit_name(name: &str, size: f32, min: f32, max: f32) -> (String, f32) {
    let width = |at: f32| laid_out_width(name, SANS, true, at);
    let measured = width(size);
    if measured <= max {
        return (name.to_owned(), size);
    }
    let mut at = (size * max / measured).floor().clamp(min, size);
    while at > min && width(at) > max {
        at -= 1.0;
    }
    if width(at) <= max {
        (name.to_owned(), at)
    } else {
        (fit_laid_out(name, SANS, true, at, max), at)
    }
}

/// A name for a small box: one line at `size` when it fits `max` px, else two lines at the largest of `size - 1`
/// to `size - 3` that holds all of it, so a long name is read whole. Returns its lines and their size.
fn name_lines(name: &str, size: f32, max: f32) -> (Vec<String>, f32) {
    if laid_out_width(name, SANS, true, size) <= max {
        return (vec![name.to_owned()], size);
    }
    let whole = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let (mut lines, mut at) = (Vec::new(), size);
    for step in 1..=3 {
        at = size - step as f32;
        lines = wrap_laid_out(name, SANS, true, at, max, 2);
        if lines.join(" ") == whole {
            break;
        }
    }
    (lines, at)
}

/// Draws a name from [`name_lines`] centred on `cx`: a single line sits on `single`, two lines on `first` and
/// 14 px below it.
fn name_text(out: &mut String, cx: f32, (single, first): (f32, f32), size: f32, lines: &[String], ink: &str) {
    for (i, line) in lines.iter().enumerate() {
        let y = if lines.len() == 1 {
            single
        } else {
            first + i as f32 * 14.0
        };
        text(out, cx, y, size, SANS, true, ink, "middle", &escape(line));
    }
}

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
            if card.sparkles == 4 {
                // Four bright pixel corners and a second inset border identify Glimmer even at thumbnail size.
                let _ = write!(
                    s,
                    r##"<rect x="{}" y="{}" width="280" height="304" fill="none" stroke="#e8fffa" stroke-width="4" stroke-dasharray="12 8"/>"##,
                    x + 8.0,
                    y + 8.0
                );
                for (sx, sy) in [
                    (x + 22.0, y + 80.0),
                    (x + 254.0, y + 76.0),
                    (x + 26.0, y + 212.0),
                    (x + 250.0, y + 214.0),
                ] {
                    sprite(&mut s, STAR, sx, sy, 3.0, "#8ff6e9", "#ffffff");
                }
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
    /// The language of the labels; the title, tab and footer come written.
    pub lang: Lang,
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
            &format!(
                "{} {}/{}",
                self.tab,
                crate::svg::thousands_in(self.lang, self.found as i64),
                crate::svg::thousands_in(self.lang, self.total as i64)
            ),
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
            if cell.owned && cell.frame == 0x9af5ec {
                let _ = write!(
                    s,
                    r##"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="#e8fffa" stroke-width="2" stroke-dasharray="6 4"/>"##,
                    x + 5.0,
                    y + 5.0,
                    cw - 10.0,
                    ch - 10.0
                );
                sprite(&mut s, STAR, x + 8.0, y + 40.0, 2.0, "#8ff6e9", "#ffffff");
            }
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
                    &format!("x{}", crate::svg::thousands_in(self.lang, cell.caught as i64)),
                );
            }
            let cx = x + cw / 2.0;
            if cell.owned {
                pixel_shadow(&mut s, cx, y + 106.0, cell.art.w as f32 / 2.4, 6.0, 0.25);
            }
            stand(&mut s, &cell.art, cx, y + 102.0, (!cell.owned).then_some("unseen"));
            let (lines, size, ink) = if cell.owned {
                let (lines, size) = name_lines(&cell.name, 14.0, cw - 16.0);
                (lines, size, &t.ink)
            } else {
                (vec!["???".to_owned()], 14.0, &t.panel_edge)
            };
            name_text(&mut s, cx, (y + 132.0, y + 123.0), size, &lines, ink);
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
            &t!(
                self.lang,
                "dex.card.page",
                page = self.page + 1,
                pages = self.pages.max(1)
            ),
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
        if tile.frame == 0x9af5ec {
            let _ = write!(
                s,
                r##"<rect x="42" y="98" width="272" height="228" fill="none" stroke="#e8fffa" stroke-width="4" stroke-dasharray="10 6"/>"##
            );
            for (x, y) in [(48.0, 156.0), (272.0, 120.0), (270.0, 258.0)] {
                sprite(&mut s, STAR, x, y, 3.0, "#8ff6e9", "#ffffff");
            }
        }
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
        let (name, size) = fit_name(&tile.name, 32.0, 22.0, 372.0);
        text(&mut s, x, 138.0, size, SANS, true, &t.ink, "start", &escape(&name));
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
            if tile.frame == 0x9af5ec {
                let _ = write!(
                    s,
                    r##"<rect x="{}" y="{}" width="212" height="190" fill="none" stroke="#e8fffa" stroke-width="3" stroke-dasharray="7 5"/>"##,
                    x + 6.0,
                    y + 6.0
                );
                sprite(&mut s, STAR, x + 188.0, y + 58.0, 2.0, "#8ff6e9", "#ffffff");
            }
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
            let (lines, size) = name_lines(&tile.name, 16.0, 204.0);
            name_text(&mut s, cx, (y + 168.0, y + 158.0), size, &lines, &t.ink);
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
    /// The language of the labels; the rate and the upgrade come written.
    pub lang: Lang,
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
            &t!(self.lang, "crabpot.card.title"),
            &t!(
                self.lang,
                "crabpot.card.level",
                level = self.level,
                max = self.max_level
            ),
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
            PotState::Empty => (t!(self.lang, "crabpot.card.empty"), t.muted.clone()),
            PotState::Soaking { .. } => (t!(self.lang, "crabpot.card.soaking"), t.accent.clone()),
            PotState::Ready { .. } => (t!(self.lang, "crabpot.card.ready"), t.mint.clone()),
        };
        text(&mut s, 454.0, 140.0, 24.0, PIXEL, false, &color, "start", &status);
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
                    &escape(&t!(self.lang, "crabpot.card.set-hint")),
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
                    &escape(&t!(self.lang, "crabpot.card.soak-left", left = left, hours = hours)),
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
                    &escape(&t!(self.lang, "crabpot.card.soak-done", hours = hours)),
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
            &escape(&self.lang.upper(&self.rate)),
        );
        let upgrade = if self.upgrade.is_empty() {
            t!(self.lang, "crabpot.card.top-level")
        } else {
            self.lang.upper(&self.upgrade)
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

    #[test]
    fn glimmer_variants_have_a_visible_sparkle_frame() {
        let card = DropCard {
            title: "Critter drop".into(),
            subtitle: String::new(),
            cards: vec![DropCritter {
                art: picture(),
                name: "Glimmer crab".into(),
                rarity: "Rare".into(),
                frame: 0x9af5ec,
                habitat: "Shore".into(),
                sparkles: 4,
            }],
            footer: String::new(),
            theme: Theme::reef(),
        };
        assert!(card.to_svg().contains("stroke-dasharray=\"12 8\""));
        let dex = DexCard {
            title: "Dex".into(),
            tab: "CRITTERS".into(),
            owner: "Sim".into(),
            found: 1,
            total: 1,
            page: 0,
            pages: 1,
            cells: vec![DexCell {
                number: 1,
                name: "Glimmer crab".into(),
                art: picture(),
                frame: 0x9af5ec,
                owned: true,
                caught: 1,
            }],
            footer: String::new(),
            lang: Lang::En,
            theme: Theme::reef(),
        };
        assert!(dex.to_svg().contains("stroke-dasharray=\"6 4\""));
    }

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
            lang: Lang::En,
            theme: Theme::reef(),
        };
        let svg = card.to_svg();
        assert!(svg.contains("Pebblesnap Crab") && svg.contains("x3"));
        assert!(!svg.contains("Secret"), "a locked slot never gives its name away");
        assert!(svg.contains("???") && svg.contains(r##"filter="url(#unseen)""##));
    }

    #[test]
    fn a_long_name_is_shrunk_or_split_before_it_is_cut() {
        // English names keep the size they always had.
        assert_eq!(
            fit_name("Pebblesnap Crab", 32.0, 22.0, 372.0),
            ("Pebblesnap Crab".into(), 32.0)
        );
        assert_eq!(
            name_lines("Pebblesnap Crab", 14.0, 132.0),
            (vec!["Pebblesnap Crab".into()], 14.0)
        );
        // The longest Turkish ones are not cut: smaller on the big card, on two lines in a small box.
        for name in [
            "Thunderspine Deniz Kestanesi",
            "Cobbleshell Keşiş Yengeci",
            "Kapı Eşiği Keşiş Yengeci",
        ] {
            let (text, size) = fit_name(name, 32.0, 22.0, 372.0);
            assert!(text == name && (22.0..32.0).contains(&size), "{name}: {text} at {size}");
            assert!(laid_out_width(&text, SANS, true, size) <= 372.0, "{name} at {size}");
            for (box_width, base) in [(132.0, 14.0), (204.0, 16.0)] {
                let (lines, size) = name_lines(name, base, box_width);
                assert!(
                    lines.len() <= 2 && lines.join(" ") == name && !lines.iter().any(|l| l.contains('…')),
                    "{name} in {box_width}: {lines:?}"
                );
                for line in &lines {
                    assert!(laid_out_width(line, SANS, true, size) <= box_width, "{line} at {size}");
                }
            }
        }
        // A name that cannot fit even so is cut, never left to spill.
        let (cut, size) = fit_name(&"W".repeat(40), 32.0, 22.0, 372.0);
        assert!(cut.ends_with('…') && size == 22.0 && laid_out_width(&cut, SANS, true, size) <= 372.0);
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
