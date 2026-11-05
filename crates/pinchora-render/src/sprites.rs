//! Sprite atlas, palette variants and procedural placeholders.

use std::{collections::HashMap, sync::Arc};

use image::{Rgba, RgbaImage};
use serde::Deserialize;

use crate::room::{SceneLayer, TILE};

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

/// A simple boxy stand-in for a furniture sprite: outline, lit top, darker front,
/// drawn in the base ramp so variants recolor it like real art.
pub fn placeholder_item(id: &str, w: u32, d: u32, layer: SceneLayer) -> RgbaImage {
    let outline = Rgba([0x1b, 0x1d, 0x2b, 255]);
    let tone = BASE_RAMP[2];
    let (pw, ph) = match layer {
        SceneLayer::Rug => (w * TILE - 2, d * TILE - 2),
        SceneLayer::Wall => (w * TILE - 4, d * TILE - 4),
        SceneLayer::Surface => (10, 10),
        SceneLayer::Floor | SceneLayer::Pet => (w * TILE - 2, d * TILE + 6 + (hash(id) % 10) as u32),
    };
    let mut img = RgbaImage::new(pw, ph);
    let top_h = match layer {
        SceneLayer::Floor => (d * TILE).min(ph) * 2 / 3,
        _ => ph,
    };
    for y in 0..ph {
        for x in 0..pw {
            let edge = x == 0 || y == 0 || x == pw - 1 || y == ph - 1;
            let px = if edge {
                outline
            } else if layer == SceneLayer::Rug {
                let stripe = (x / 3 + y / 3) % 2 == 0;
                if stripe {
                    shade(BASE_RAMP[3], 1.0)
                } else {
                    shade(tone, 1.0)
                }
            } else if y < top_h {
                if y == 1 {
                    shade(BASE_RAMP[4], 1.0)
                } else {
                    shade(BASE_RAMP[3], 1.0)
                }
            } else if y == top_h {
                outline
            } else {
                shade(BASE_RAMP[1], 1.0)
            };
            img.put_pixel(x, y, px);
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
    let mut img = RgbaImage::new(TILE, TILE);
    for y in 0..TILE {
        for x in 0..TILE {
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
