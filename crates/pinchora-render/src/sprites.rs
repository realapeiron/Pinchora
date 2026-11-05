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
