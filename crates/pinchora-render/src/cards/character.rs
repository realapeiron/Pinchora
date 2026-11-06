use std::fmt::Write;

use image::{Rgba, RgbaImage, imageops};

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    character::scale_up,
    encode_png,
    svg::{background, data_uri_png, escape, panel, pixel_shadow, sprite, truncate_to},
};

/// Thickness of the floor tile's side, in native pixels.
const SLAB: u32 = 8;
const OUTLINE: Rgba<u8> = Rgba([0x1b, 0x1d, 0x2b, 255]);
const LOCK: &[&str] = &[".###.", "#...#", "#...#", "#####", "##o##", "#####"];

const W: f32 = 960.0;
const H: f32 = 540.0;
/// Where the right-hand column starts and how wide it is.
const COL_X: f32 = 400.0;
const COL_W: f32 = 536.0;
/// The grid area under the tabs and caption.
const GRID_Y: f32 = 200.0;
const GRID_BOTTOM: f32 = 506.0;
const GAP: f32 = 8.0;

/// One choice in the editor's grid: a colour ramp or a little picture, a name and a note.
#[derive(Debug, Clone, Default)]
pub struct EditorCell {
    /// Art at its own size; drawn as large as whole pixels allow.
    pub art: Option<RgbaImage>,
    /// Colours `0xrrggbb`, dark to light, shown as a striped swatch.
    pub swatch: Option<Vec<u32>>,
    pub label: String,
    /// A short note in the corner: a price, a level, "on".
    pub note: String,
    pub current: bool,
    pub locked: bool,
}

/// A titled grid of cells.
#[derive(Debug, Clone, Default)]
pub struct EditorSection {
    pub title: String,
    pub cols: u32,
    pub cells: Vec<EditorCell>,
}

/// 960×540 character editor: the character large on a floor tile, the categories as tabs, and the
/// choices of the open category as a grid of swatches or pictures.
pub struct CharacterCard {
    /// The character at scale 1, as `render_character` draws it.
    pub character: RgbaImage,
    pub name: String,
    /// A word in the corner ("SAVED", "DRAFT") and whether it is good news.
    pub status: (String, bool),
    pub tabs: Vec<String>,
    pub tab: usize,
    /// The open category and what is picked in it.
    pub caption: String,
    pub sections: Vec<EditorSection>,
    pub hint: String,
    pub theme: Theme,
}

/// One isometric floor tile at native resolution: a two-tone 2×2 checker with a slab edge,
/// drawn with the same 2:1 projection as the rooms.
fn floor_tile() -> RgbaImage {
    let tones = [[0x58, 0x6a, 0xb8], [0x3d, 0x4c, 0x92]];
    let mut img = RgbaImage::new(64, 32 + SLAB + 1);
    for lx in -32i32..32 {
        let mut bottom = None;
        for ly in 0..32i32 {
            let (a, b) = (lx + 2 * ly, 2 * ly - lx - 1);
            if !(0..64).contains(&a) || !(0..64).contains(&b) {
                continue;
            }
            let [r, g, bl] = tones[((a / 32 + b / 32) % 2) as usize];
            img.put_pixel((lx + 32) as u32, ly as u32, Rgba([r, g, bl, 255]));
            bottom = Some(ly);
        }
        if let Some(bottom) = bottom {
            let f = if lx < 0 { 0.62 } else { 0.48 };
            let [r, g, bl] = tones[1].map(|c| (f32::from(c) * f) as u8);
            for ly in bottom + 1..=bottom + SLAB as i32 {
                img.put_pixel((lx + 32) as u32, ly as u32, Rgba([r, g, bl, 255]));
            }
        }
    }
    // Outline every pixel that borders the background.
    let solid = img.clone();
    for (x, y, px) in img.enumerate_pixels_mut() {
        if px[3] == 0 {
            continue;
        }
        let open = |dx: i32, dy: i32| {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            nx < 0
                || ny < 0
                || nx >= solid.width() as i32
                || ny >= solid.height() as i32
                || solid.get_pixel(nx as u32, ny as u32)[3] == 0
        };
        if open(-1, 0) || open(1, 0) || open(0, -1) || open(0, 1) {
            *px = OUTLINE;
        }
    }
    img
}

fn hex(v: u32) -> String {
    format!("#{:06x}", v & 0xff_ffff)
}

/// The cell size of a section with `rows` rows sharing `height` with its title.
fn cell_size(cols: u32, rows: u32, height: f32) -> (f32, f32) {
    let cols = cols.max(1) as f32;
    let rows = rows.max(1) as f32;
    let w = ((COL_W - GAP * (cols - 1.0)) / cols).floor();
    let h = ((height - 20.0 - GAP * (rows - 1.0)) / rows).floor().min(w * 2.1);
    (w, h)
}

impl CharacterCard {
    /// The scale the character is drawn at: as large as fits the stage, at most 4.
    pub fn scale_for(character: &RgbaImage) -> u32 {
        let by_w = 330 / character.width().max(1);
        let by_h = 412 / character.height().max(1);
        by_w.min(by_h).clamp(1, 4)
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let mut s = String::with_capacity(96 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">"#
        );
        background(&mut s, W, H, &t.bg_top, &t.bg_bottom);
        self.stage(&mut s);

        // The name and the status.
        let status_w = 24.0 + 10.0 * self.status.0.chars().count() as f32;
        text(
            &mut s,
            COL_X,
            62.0,
            32.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&truncate_to(&self.name, 32.0, COL_W - status_w - 16.0)),
        );
        let fill = if self.status.1 {
            t.mint.as_str()
        } else {
            t.gold.as_str()
        };
        panel(
            &mut s,
            COL_X + COL_W - status_w,
            36.0,
            status_w,
            28.0,
            3.0,
            fill,
            "#0f1631",
        );
        text(
            &mut s,
            COL_X + COL_W - status_w / 2.0,
            55.0,
            12.0,
            PIXEL,
            false,
            "#0f1631",
            "middle",
            &escape(&self.status.0),
        );

        // The tabs, five a row.
        let per_row = 5usize;
        let tab_w = (COL_W - GAP * (per_row as f32 - 1.0)) / per_row as f32;
        for (i, label) in self.tabs.iter().enumerate() {
            let (x, y) = (
                COL_X + (i % per_row) as f32 * (tab_w + GAP),
                82.0 + (i / per_row) as f32 * 34.0,
            );
            let on = i == self.tab;
            let (fill, edge, ink) = if on {
                (t.accent.as_str(), t.ink.as_str(), "#0f1631")
            } else {
                (t.panel.as_str(), t.panel_edge.as_str(), t.muted.as_str())
            };
            panel(&mut s, x, y, tab_w, 28.0, 3.0, fill, edge);
            text(
                &mut s,
                x + tab_w / 2.0,
                y + 19.0,
                11.0,
                PIXEL,
                on,
                ink,
                "middle",
                &escape(&label.to_uppercase()),
            );
        }
        text(
            &mut s,
            COL_X,
            180.0,
            20.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&truncate_to(&self.caption, 20.0, COL_W)),
        );

        self.grid(&mut s);
        text(
            &mut s,
            COL_X,
            528.0,
            11.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &escape(&self.hint.to_uppercase()),
        );
        s.push_str("</svg>");
        s
    }

    /// The character on its floor tile in the left panel.
    fn stage(&self, s: &mut String) {
        let t = &self.theme;
        panel(s, 24.0, 24.0, 352.0, 492.0, 8.0, &t.panel, &t.accent);
        let tile_scale = 3.0;
        let (cx, centre) = (200.0, 430.0);
        let tile = scale_up(&floor_tile(), 3);
        let _ = write!(
            s,
            r##"<image x="{}" y="{}" width="{}" height="{}" href="{}"/>"##,
            cx - 32.0 * tile_scale,
            centre - 16.0 * tile_scale,
            tile.width(),
            tile.height(),
            data_uri_png(&encode_png(&tile).unwrap_or_default()),
        );
        let k = Self::scale_for(&self.character);
        let feet = centre + 6.0 * tile_scale;
        pixel_shadow(s, cx, feet - 2.0, 12.0 * k as f32, 2.0 * k as f32, 0.3);
        let (w, h) = (self.character.width() * k, self.character.height() * k);
        let _ = write!(
            s,
            r##"<image x="{}" y="{}" width="{w}" height="{h}" image-rendering="optimizeSpeed" href="{}"/>"##,
            cx - (w / 2) as f32,
            feet - h as f32,
            data_uri_png(&encode_png(&self.character).unwrap_or_default())
        );
    }

    /// The sections' cells. Their pictures are laid on one sheet so the card embeds one image.
    fn grid(&self, s: &mut String) {
        let t = &self.theme;
        let height = GRID_BOTTOM - GRID_Y;
        let rows: Vec<u32> = self
            .sections
            .iter()
            .map(|sec| (sec.cells.len() as u32).div_ceil(sec.cols.max(1)).max(1))
            .collect();
        let total_rows: u32 = rows.iter().sum::<u32>().max(1);
        let mut sheet = RgbaImage::new(COL_W as u32, height as u32);
        let mut y = GRID_Y;
        for (sec, &n) in self.sections.iter().zip(&rows) {
            let share = height * n as f32 / total_rows as f32;
            let (cw, ch) = cell_size(sec.cols, n, share);
            text(
                s,
                COL_X,
                y + 12.0,
                11.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &escape(&sec.title.to_uppercase()),
            );
            let top = y + 20.0;
            for (i, cell) in sec.cells.iter().enumerate() {
                let (x, cy) = (
                    COL_X + (i as u32 % sec.cols.max(1)) as f32 * (cw + GAP),
                    top + (i as u32 / sec.cols.max(1)) as f32 * (ch + GAP),
                );
                let edge = if cell.current {
                    t.ink.as_str()
                } else {
                    t.panel_edge.as_str()
                };
                let border = if cell.current { 3.0 } else { 2.0 };
                panel(s, x, cy, cw, ch, border, &t.panel, edge);
                let label_room = if cell.label.is_empty() { 0.0 } else { 15.0 };
                let (bx, by, bw, bh) = (x + 5.0, cy + 5.0, cw - 10.0, ch - 10.0 - label_room);
                if let Some(ramp) = &cell.swatch {
                    let band = (bh / ramp.len().max(1) as f32).floor();
                    let top = by + bh - band * ramp.len() as f32;
                    for (k, c) in ramp.iter().rev().enumerate() {
                        let _ = write!(
                            s,
                            r#"<rect x="{bx}" y="{}" width="{bw}" height="{band}" fill="{}"/>"#,
                            top + k as f32 * band,
                            hex(*c)
                        );
                    }
                } else if let Some(art) = &cell.art {
                    let k = ((bw / art.width().max(1) as f32).min(bh / art.height().max(1) as f32) as u32).max(1);
                    let big = scale_up(art, k);
                    let ax = (bx - COL_X + (bw - big.width() as f32) / 2.0).round() as i64;
                    let ay = (by - GRID_Y + (bh - big.height() as f32) / 2.0).round() as i64;
                    imageops::overlay(&mut sheet, &big, ax, ay);
                }
                if !cell.label.is_empty() {
                    let ink = if cell.locked { t.muted.as_str() } else { t.ink.as_str() };
                    text(
                        s,
                        x + cw / 2.0,
                        cy + ch - 7.0,
                        11.0,
                        SANS,
                        cell.current,
                        ink,
                        "middle",
                        &escape(&truncate_to(&cell.label, 11.0, cw - 8.0)),
                    );
                }
                if cell.locked {
                    sprite(s, LOCK, x + 5.0, cy + 5.0, 2.0, &t.gold, &t.panel);
                }
                if !cell.note.is_empty() {
                    let nw = 10.0 + 7.0 * cell.note.chars().count() as f32;
                    panel(s, x + cw - nw - 3.0, cy + 3.0, nw, 16.0, 2.0, "#0f1631", &t.panel_edge);
                    text(
                        s,
                        x + cw - nw / 2.0 - 3.0,
                        cy + 14.5,
                        9.0,
                        PIXEL,
                        false,
                        &t.gold,
                        "middle",
                        &escape(&cell.note),
                    );
                }
            }
            y += share;
        }
        if sheet.pixels().any(|p| p[3] > 0) {
            let _ = write!(
                s,
                r##"<image x="{COL_X}" y="{GRID_Y}" width="{}" height="{}" image-rendering="optimizeSpeed" href="{}"/>"##,
                sheet.width(),
                sheet.height(),
                data_uri_png(&encode_png(&sheet).unwrap_or_default())
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Renderer,
        character::{CharacterSpec, render_character},
    };

    #[test]
    fn renders_swatches_and_pictures() {
        let r = Renderer::new();
        let character = render_character(
            &CharacterSpec {
                look: "tidewalker".to_owned(),
                ..Default::default()
            },
            1,
        );
        let swatches = EditorSection {
            title: "Skin".to_owned(),
            cols: 10,
            cells: (0..20)
                .map(|i| EditorCell {
                    swatch: Some(vec![0x8a4b3b, 0xc97a5c, 0xf0a98a + i, 0xffd8c2]),
                    label: format!("Tone {i}"),
                    current: i == 3,
                    ..Default::default()
                })
                .collect(),
        };
        let pictures = EditorSection {
            title: "Head".to_owned(),
            cols: 5,
            cells: (0..7)
                .map(|i| EditorCell {
                    art: Some(character.clone()),
                    label: "<Straw> & hat".to_owned(),
                    note: if i % 2 == 0 { "1,200".to_owned() } else { String::new() },
                    locked: i == 4,
                    current: i == 1,
                    ..Default::default()
                })
                .collect(),
        };
        let card = CharacterCard {
            character,
            name: "<Pinch> & co".to_owned(),
            status: ("DRAFT".to_owned(), false),
            tabs: [
                "Look", "Skin", "Hair", "Eyes", "Top", "Bottom", "Shoes", "Gear", "Presets", "Outfits",
            ]
            .map(String::from)
            .into(),
            tab: 1,
            caption: "Skin · Peach".to_owned(),
            sections: vec![swatches, pictures],
            hint: "< > step · menu jumps".to_owned(),
            theme: Theme::reef(),
        };
        let svg = card.to_svg();
        assert!(svg.contains("&lt;Pinch&gt; &amp; co"));
        assert!(svg.contains("SKIN") && svg.contains("DRAFT"));
        assert!(r.character_card(&card).expect("renders").starts_with(b"\x89PNG"));
    }
}
