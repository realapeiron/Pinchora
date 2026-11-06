//! Sprite atlas, palette variants and procedural placeholders.

use std::{collections::HashMap, sync::Arc};

use image::{Rgba, RgbaImage};
use serde::Deserialize;

use crate::room::{LEVEL_H, SceneLayer};

/// Wallpaper and floor textures are square tiles of this size.
const TEX: u32 = 16;

/// The coral ramp every base sprite uses for its main colour; variants replace it.
pub const BASE_RAMP: [[u8; 3]; 5] = [
    [0x7a, 0x2a, 0x3a],
    [0xc2, 0x41, 0x3f],
    [0xff, 0x6b, 0x4a],
    [0xff, 0x9e, 0x7a],
    [0xff, 0xd2, 0xc0],
];

/// Dark → light ramps for palette variants.
pub fn variant_ramp(name: &str) -> Option<[[u8; 3]; 5]> {
    let hex = |s: &str| -> [u8; 3] {
        let v = u32::from_str_radix(s, 16).unwrap_or(0);
        [(v >> 16) as u8, (v >> 8) as u8, v as u8]
    };
    let ramp = |a: [&str; 5]| Some(a.map(hex));
    match name {
        "coral" => Some(BASE_RAMP),
        "mint" => ramp(["0b5e4c", "079b74", "06d6a0", "6ff0c8", "c9fff0"]),
        "navy" => ramp(["0f1631", "1d2b53", "2e3d75", "4a5ba3", "8fa0d8"]),
        "cream" => ramp(["7d6a5f", "b8a08f", "e6d3c3", "f4e6da", "fff1e8"]),
        "plum" => ramp(["3a2350", "5a3a7a", "7d55a3", "9b6bc4", "cfb2ea"]),
        "gold" => ramp(["6b4410", "a06a14", "c98a1e", "ffd166", "fff0b8"]),
        "sky" => ramp(["1f5f8b", "2c7fb8", "3a9bdc", "8fd3ff", "d6f1ff"]),
        "moss" => ramp(["1f4d3a", "2f7d4f", "4f9f5a", "5bbf6a", "a6e39a"]),
        "rose" => ramp(["7a2a4f", "b8436f", "e86a92", "ff8fb1", "ffd0df"]),
        "charcoal" => ramp(["1b1d2b", "2c2f3f", "45495c", "6a6f86", "a3a8bd"]),
        "sand" => ramp(["7d6a4a", "a88d5f", "d2b77f", "e8d3a3", "f8edd0"]),
        "lilac" => ramp(["4b3d6b", "6f5c99", "9a86c7", "c2b3e6", "e8e0fb"]),
        _ => None,
    }
}

/// Swaps the base ramp for `ramp`; every other colour is kept.
pub fn recolor(sprite: &RgbaImage, ramp: &[[u8; 3]; 5]) -> RgbaImage {
    let mut out = sprite.clone();
    for px in out.pixels_mut() {
        if px[3] == 0 {
            continue;
        }
        if let Some(i) = BASE_RAMP.iter().position(|c| c[..] == px.0[..3]) {
            px.0[..3].copy_from_slice(&ramp[i]);
        }
    }
    out
}

#[derive(Debug, Deserialize)]
struct AtlasEntry {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Sprites cut from `assets/sprites/atlas.png` by `assets/sprites/atlas.json`
/// (`{"name": {"x":..,"y":..,"w":..,"h":..}}`). Missing names fall back to placeholders.
pub struct SpriteStore {
    sprites: HashMap<String, Arc<RgbaImage>>,
}

impl SpriteStore {
    pub fn embedded() -> Self {
        Self::from_atlas(
            include_bytes!("../../../assets/sprites/atlas.png"),
            include_str!("../../../assets/sprites/atlas.json"),
        )
        .unwrap_or_else(|_| Self {
            sprites: HashMap::new(),
        })
    }

    pub fn from_atlas(png: &[u8], index: &str) -> anyhow::Result<Self> {
        let atlas = image::load_from_memory(png)?.to_rgba8();
        let entries: HashMap<String, AtlasEntry> = serde_json::from_str(index)?;
        let mut sprites = HashMap::with_capacity(entries.len());
        for (name, e) in entries {
            if e.x + e.w <= atlas.width() && e.y + e.h <= atlas.height() {
                let img = image::imageops::crop_imm(&atlas, e.x, e.y, e.w, e.h).to_image();
                sprites.insert(name, Arc::new(img));
            }
        }
        Ok(Self { sprites })
    }

    pub fn get(&self, name: &str) -> Option<Arc<RgbaImage>> {
        self.sprites.get(name).cloned()
    }

    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }
}

fn hash(s: &str) -> u64 {
    // FNV-1a: stable across runs, so placeholders never change look.
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn shade(c: [u8; 3], f: f32) -> Rgba<u8> {
    Rgba([
        (f32::from(c[0]) * f).min(255.0) as u8,
        (f32::from(c[1]) * f).min(255.0) as u8,
        (f32::from(c[2]) * f).min(255.0) as u8,
        255,
    ])
}

/// A stand-in for furniture art, drawn in the base ramp so variants recolor it like real
/// art: an isometric box for floor and surface items, a framed panel for wall items and a
/// striped flat mat for rugs (rugs and wall art are flat; the renderer projects them).
pub fn placeholder_item(id: &str, w: u32, d: u32, layer: SceneLayer, iso_width: Option<u32>) -> RgbaImage {
    let outline = Rgba([0x1b, 0x1d, 0x2b, 255]);
    match layer {
        SceneLayer::Rug => {
            let (pw, ph) = (w * 32, d * 32);
            RgbaImage::from_fn(pw, ph, |x, y| {
                if x == 0 || y == 0 || x == pw - 1 || y == ph - 1 {
                    outline
                } else if (x / 6 + y / 6) % 2 == 0 {
                    shade(BASE_RAMP[3], 1.0)
                } else {
                    shade(BASE_RAMP[2], 1.0)
                }
            })
        }
        SceneLayer::Wall => {
            let (pw, ph) = (w * 32 - 4, d * LEVEL_H - 4);
            RgbaImage::from_fn(pw, ph, |x, y| {
                let edge = x.min(y).min(pw - 1 - x).min(ph - 1 - y);
                match edge {
                    0 => outline,
                    1..=2 => shade(BASE_RAMP[1], 1.0),
                    _ => shade(BASE_RAMP[3], 1.0),
                }
            })
        }
        SceneLayer::Floor | SceneLayer::Surface | SceneLayer::Figure => {
            let full = (w + d) * 32;
            let sw = iso_width
                .unwrap_or(if layer == SceneLayer::Surface { 24 } else { full })
                .clamp(8, full);
            let k = sw as f32 / full as f32;
            let tall = if layer == SceneLayer::Surface {
                10 + hash(id) % 8
            } else {
                18 + hash(id) % 34
            };
            let height = (tall as f32 * k.max(0.5)) as u32;
            iso_box(w, d, sw, height)
        }
    }
}

/// An outlined isometric box with a `w`×`d` footprint, `sw` pixels wide and `height` tall.
fn iso_box(w: u32, d: u32, sw: u32, height: u32) -> RgbaImage {
    let outline = Rgba([0x1b, 0x1d, 0x2b, 255]);
    let k = sw as f32 / ((w + d) * 32) as f32;
    let sh = (16.0 * (w + d) as f32 * k) as u32 + height + 1;
    let mut img = RgbaImage::new(sw, sh);
    let (wf, df, hf) = (w as f32, d as f32, height as f32 / k);
    for py in 0..sh {
        for px in 0..sw {
            // Back to footprint units, with the left corner at x = 0.
            let (u, v) = (px as f32 / k, py as f32 / k);
            let (a, b) = (u - 32.0 * df + 2.0 * v, 2.0 * v - u + 32.0 * df);
            let top = a >= 0.0 && b >= 0.0 && a < 64.0 * wf && b < 64.0 * df;
            let edge = if u < 32.0 * wf {
                16.0 * df + u / 2.0
            } else {
                16.0 * (wf + df) - (u - 32.0 * wf) / 2.0
            };
            let c = if top {
                BASE_RAMP[3]
            } else if v >= edge && v < edge + hf && u < 32.0 * wf {
                BASE_RAMP[2]
            } else if v >= edge && v < edge + hf {
                BASE_RAMP[1]
            } else {
                continue;
            };
            img.put_pixel(px, py, shade(c, 1.0));
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
            nx < 0 || ny < 0 || nx >= sw as i32 || ny >= sh as i32 || solid.get_pixel(nx as u32, ny as u32)[3] == 0
        };
        if open(-1, 0) || open(1, 0) || open(0, -1) || open(0, 1) {
            *px = outline;
        }
    }
    img
}

/// A 16×16 stand-in tile: planks for floors, stripes for wallpapers.
pub fn placeholder_tile(id: &str, wall: bool) -> RgbaImage {
    let h = hash(id);
    let palette: [[u8; 3]; 6] = [
        [0xb9, 0x7f, 0x4a],
        [0x8f, 0xa0, 0xd8],
        [0xe6, 0xd3, 0xc3],
        [0x5b, 0xbf, 0x6a],
        [0xff, 0x9e, 0x7a],
        [0x9b, 0x6b, 0xc4],
    ];
    let base = palette[(h % palette.len() as u64) as usize];
    let mut img = RgbaImage::new(TEX, TEX);
    for y in 0..TEX {
        for x in 0..TEX {
            let f = if wall {
                if x % 8 < 4 { 1.0 } else { 0.9 }
            } else if y % 8 == 7 || (x + (y / 8) * 8) % 16 == 0 {
                0.7
            } else {
                1.0
            };
            img.put_pixel(x, y, shade(base, f));
        }
    }
    img
}

/// The fill of a badge's placeholder circle, `0xrrggbb`; the same badge always gets the same one.
pub fn placeholder_badge_color(id: &str) -> u32 {
    const COLORS: [u32; 6] = [0xff6b4a, 0x06d6a0, 0xffd166, 0x9b6bc4, 0x3a9bdc, 0xff8fb1];
    COLORS[(hash(id) % COLORS.len() as u64) as usize]
}

/// A small round critter with eyes, tinted per species, until real pet art exists.
pub fn placeholder_pet(id: &str) -> RgbaImage {
    let colors: [[u8; 3]; 6] = [
        [0xff, 0x6b, 0x4a],
        [0x06, 0xd6, 0xa0],
        [0xff, 0xd1, 0x66],
        [0x9b, 0x6b, 0xc4],
        [0x3a, 0x9b, 0xdc],
        [0xff, 0x8f, 0xb1],
    ];
    let c = colors[(hash(id) % colors.len() as u64) as usize];
    let rows = [
        "....####....",
        "..#oooooo#..",
        ".#oooooooo#.",
        "#oo##oo##oo#",
        "#oo#wo#woo.#",
        "#oooooooooo#",
        "#oooo##oooo#",
        ".#oooooooo#.",
        "..##.##.##..",
    ];
    let mut img = RgbaImage::new(12, rows.len() as u32);
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let px = match ch {
                '#' => Rgba([0x1b, 0x1d, 0x2b, 255]),
                'o' => shade(c, 1.0),
                'w' => Rgba([0xff, 0xf1, 0xe8, 255]),
                _ => continue,
            };
            img.put_pixel(x as u32, y as u32, px);
        }
    }
    image::imageops::resize(&img, 36, rows.len() as u32 * 3, image::imageops::FilterType::Nearest)
}

/// A small "z", and a bigger one: the snore drawn above a sleeping pet.
const ZZ: [&[&str]; 2] = [
    &["#####", "...#.", "..#..", ".#...", "#####"],
    &[
        "#######", ".....#.", "....#..", "...#...", "..#....", ".#.....", "#######",
    ],
];
/// Rows above the pet the "z z" gets.
const ZZ_ROOM: u32 = 17;

/// `sprite` as a pet asleep, facing left or right: an upright one (taller than wide) lies on its
/// side, a wide one flattens to two thirds of its height, both a little dimmed and trimmed to their
/// pixels so they rest on the floor, with a pixel "z z" rising above. Nothing is drawn by hand, so
/// it works for every species.
pub fn sleeping_pet(sprite: &RgbaImage, left: bool) -> RgbaImage {
    let upright = sprite.height() >= sprite.width();
    let turned = match (upright, left) {
        (true, true) => image::imageops::rotate270(sprite),
        (true, false) => image::imageops::rotate90(sprite),
        (false, _) => sprite.clone(),
    };
    let (mut x0, mut y0, mut x1, mut y1) = (turned.width(), turned.height(), 0, 0);
    for (x, y, px) in turned.enumerate_pixels() {
        if px[3] > 0 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    let mut body = if x1 < x0 {
        turned
    } else {
        image::imageops::crop_imm(&turned, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image()
    };
    if !upright {
        body = flatten(&body);
        if left {
            body = image::imageops::flip_horizontal(&body);
        }
    }
    for px in body.pixels_mut().filter(|px| px[3] > 0) {
        for channel in &mut px.0[..3] {
            *channel = (u16::from(*channel) * 7 / 8) as u8;
        }
    }
    let width = body.width().max(24);
    let mut out = RgbaImage::new(width, body.height() + ZZ_ROOM);
    image::imageops::overlay(
        &mut out,
        &body,
        i64::from((width - body.width()) / 2),
        i64::from(ZZ_ROOM),
    );
    // The small one low and to the left, the big one high and to the right.
    let middle = width / 2;
    for (glyph, (x, y)) in ZZ.iter().zip([(middle.saturating_sub(9), 10), (middle + 1, 1)]) {
        draw_glyph(&mut out, glyph, x, y);
    }
    out
}

/// `sprite` without every third row: two thirds as tall, its pixels as sharp as before.
fn flatten(sprite: &RgbaImage) -> RgbaImage {
    let kept: Vec<u32> = (0..sprite.height()).filter(|y| y % 3 != 2).collect();
    RgbaImage::from_fn(sprite.width(), kept.len() as u32, |x, y| {
        *sprite.get_pixel(x, kept[y as usize])
    })
}

/// `glyph` (`#` pixels) in a soft blue with a dark outline, its top-left corner at (`x`, `y`).
fn draw_glyph(img: &mut RgbaImage, glyph: &[&str], x: u32, y: u32) {
    let lit = |gx: i64, gy: i64| {
        glyph
            .get(usize::try_from(gy).unwrap_or(usize::MAX))
            .and_then(|row| row.as_bytes().get(usize::try_from(gx).unwrap_or(usize::MAX)))
            == Some(&b'#')
    };
    let (w, h) = (glyph[0].len() as i64, glyph.len() as i64);
    for gy in -1..=h {
        for gx in -1..=w {
            let near = (-1..=1).any(|dy| (-1..=1).any(|dx| lit(gx + dx, gy + dy)));
            let (px, py) = (i64::from(x) + gx, i64::from(y) + gy);
            if !near || px < 0 || py < 0 || px >= i64::from(img.width()) || py >= i64::from(img.height()) {
                continue;
            }
            let color = if lit(gx, gy) {
                Rgba([0xcf, 0xd8, 0xff, 255])
            } else {
                Rgba([0x1b, 0x1d, 0x2b, 255])
            };
            img.put_pixel(px as u32, py as u32, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A solid `w`×`h` pet in a larger frame with an eye near its top-left corner (on the second row,
    /// which flattening keeps), so a turn or a mirror shows.
    fn pet(frame: (u32, u32), at: (u32, u32), w: u32, h: u32) -> RgbaImage {
        let mut img = RgbaImage::new(frame.0, frame.1);
        for y in at.1..at.1 + h {
            for x in at.0..at.0 + w {
                img.put_pixel(x, y, Rgba([200, 90, 60, 255]));
            }
        }
        img.put_pixel(at.0 + 2, at.1 + 1, Rgba([255, 255, 255, 255]));
        img
    }

    /// Rows of `img` from `from` down that have a visible pixel.
    fn drawn_rows(img: &RgbaImage, from: u32) -> usize {
        (from..img.height())
            .filter(|&y| (0..img.width()).any(|x| img.get_pixel(x, y)[3] > 0))
            .count()
    }

    /// Pixels of the "z z" colour in rows `ys`.
    fn snore(img: &RgbaImage, ys: std::ops::Range<u32>) -> usize {
        ys.flat_map(|y| (0..img.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| img.get_pixel(x, y).0 == [0xcf, 0xd8, 0xff, 255])
            .count()
    }

    #[test]
    fn an_upright_pet_lies_on_its_side_with_zs_above() {
        // 12 wide and 26 tall in a 20×30 frame: turned, it is 26 wide and 12 tall.
        let standing = pet((20, 30), (4, 2), 12, 26);
        let lying = sleeping_pet(&standing, true);
        assert_eq!((lying.width(), lying.height()), (26, 12 + ZZ_ROOM));
        assert_eq!(
            drawn_rows(&lying, ZZ_ROOM),
            12,
            "the body fills its rows, resting on the bottom edge"
        );
        assert_eq!(
            snore(&lying, 0..ZZ_ROOM),
            13 + 19,
            "a small z (13 pixels) and a big one (19)"
        );
        assert_eq!(snore(&lying, ZZ_ROOM..lying.height()), 0, "nothing on the body");
        // Turned the other way it is the same size, the other way round.
        let right = sleeping_pet(&standing, false);
        assert_eq!((right.width(), right.height()), (lying.width(), lying.height()));
        assert_ne!(lying, right);
    }

    #[test]
    fn a_wide_pet_flattens_instead_of_standing_on_its_head() {
        // 26 wide and 16 tall: two thirds of the rows stay (11 of 16).
        let standing = pet((30, 20), (2, 4), 26, 16);
        let lying = sleeping_pet(&standing, false);
        assert_eq!((lying.width(), lying.height()), (26, 11 + ZZ_ROOM));
        assert_eq!(drawn_rows(&lying, ZZ_ROOM), 11);
        assert_eq!(snore(&lying, 0..ZZ_ROOM), 13 + 19);
        let mirrored = sleeping_pet(&standing, true);
        assert_ne!(lying, mirrored, "it can face either way, its z's always read as z's");
        assert_eq!(snore(&mirrored, 0..ZZ_ROOM), 13 + 19);
    }

    #[test]
    fn a_sleeper_is_a_little_darker_than_the_pet_standing() {
        let standing = pet((30, 20), (2, 4), 26, 16);
        let lying = sleeping_pet(&standing, false);
        let body = lying.get_pixel(lying.width() / 2, lying.height() - 3);
        assert_eq!(body.0, [175, 78, 52, 255], "200, 90, 60 at seven eighths");
    }

    #[test]
    fn an_empty_sprite_still_gets_its_zs() {
        let lying = sleeping_pet(&RgbaImage::new(8, 8), false);
        assert_eq!((lying.width(), lying.height()), (24, 8 + ZZ_ROOM));
    }
}
