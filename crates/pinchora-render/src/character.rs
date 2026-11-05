//! Player characters: GPT-drawn looks whose key colour ramps (skin, hair, top, bottom) are
//! swapped for the colours the player picked. The keys match `assets/sprites/avatar_palette.hex`.

use std::collections::HashMap;

use image::{Rgba, RgbaImage, imageops};

use crate::sprites::{BASE_RAMP, variant_ramp};

pub const SKIN_KEY: [[u8; 3]; 4] = [
    [0x8a, 0x4b, 0x3b],
    [0xc9, 0x7a, 0x5c],
    [0xf0, 0xa9, 0x8a],
    [0xff, 0xd8, 0xc2],
];
pub const HAIR_KEY: [[u8; 3]; 4] = [
    [0x3b, 0x1f, 0x6b],
    [0x5e, 0x35, 0xa8],
    [0x8a, 0x5a, 0xd8],
    [0xb9, 0x94, 0xf0],
];
pub const BOTTOM_KEY: [[u8; 3]; 4] = [
    [0x1d, 0x2b, 0x53],
    [0x2e, 0x3d, 0x75],
    [0x4a, 0x5b, 0xa3],
    [0x8f, 0xa0, 0xd8],
];
// Tops use the furniture recolour ramp, `BASE_RAMP`.

pub const SKIN_TONES: [(&str, [u32; 4]); 8] = [
    ("Porcelain", [0x9a6a5a, 0xd9a08a, 0xf6cdb6, 0xfff0e6]),
    ("Peach", [0x8a4b3b, 0xc97a5c, 0xf0a98a, 0xffd8c2]),
    ("Sand", [0x7d5238, 0xb9805a, 0xe0ab7f, 0xf5d2ad]),
    ("Honey", [0x6e4426, 0xa8693c, 0xd09359, 0xebbd86]),
    ("Caramel", [0x5a3520, 0x8c5530, 0xb7774a, 0xd9a170]),
    ("Cocoa", [0x3f2418, 0x6b3e26, 0x93603c, 0xb98457]),
    ("Espresso", [0x2a1812, 0x4a2c1e, 0x6e4530, 0x8f6146]),
    ("Seafoam", [0x2f6b5e, 0x4f9f8a, 0x7fcdb5, 0xbdf0e0]),
];

pub const HAIR_COLORS: [(&str, [u32; 4]); 12] = [
    ("Ink", [0x14141c, 0x26283a, 0x3b3f58, 0x5a6080]),
    ("Chestnut", [0x2e1a12, 0x4f2e1e, 0x744630, 0x9a6448]),
    ("Hazel", [0x4a2e18, 0x75502a, 0xa0743c, 0xc99a5a]),
    ("Auburn", [0x4a1a14, 0x7a2e1e, 0xa8472c, 0xd06a44]),
    ("Ginger", [0x7a3a10, 0xb85a1a, 0xe8822e, 0xffb066]),
    ("Honey blonde", [0x8a6420, 0xc0922e, 0xe8c25a, 0xfde59a]),
    ("Platinum", [0x8f8a80, 0xc2bdb0, 0xe8e4d8, 0xfffcf2]),
    ("Silver", [0x45495c, 0x6a6f86, 0xa3a8bd, 0xdfe3f0]),
    ("Bubblegum", [0x7a2a4f, 0xb8436f, 0xe86a92, 0xffb3cb]),
    ("Ocean", [0x1f3f6b, 0x2c6fb8, 0x4a9bdc, 0x9fd8ff]),
    ("Kelp", [0x1f4d3a, 0x2f7d4f, 0x4f9f5a, 0x8fd88a]),
    ("Violet", [0x3b1f6b, 0x5e35a8, 0x8a5ad8, 0xb994f0]),
];

/// Outfit colours are the furniture variant ramps.
pub const OUTFIT_COLORS: [&str; 12] = [
    "coral", "mint", "navy", "cream", "plum", "gold", "sky", "moss", "rose", "charcoal", "sand", "lilac",
];

/// Indices into `SKIN_TONES`, `HAIR_COLORS` and `OUTFIT_COLORS`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Colors {
    pub skin: usize,
    pub hair: usize,
    pub top: usize,
    pub bottom: usize,
}

fn rgb(v: u32) -> [u8; 3] {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// Swaps every key ramp in a look for the chosen colours; other colours stay.
pub fn recolor_character(sprite: &RgbaImage, colors: Colors) -> RgbaImage {
    let mut map: HashMap<[u8; 3], [u8; 3]> = HashMap::new();
    let skin = SKIN_TONES[colors.skin % SKIN_TONES.len()].1;
    let hair = HAIR_COLORS[colors.hair % HAIR_COLORS.len()].1;
    for i in 0..4 {
        map.insert(SKIN_KEY[i], rgb(skin[i]));
        map.insert(HAIR_KEY[i], rgb(hair[i]));
    }
    if let Some(top) = variant_ramp(OUTFIT_COLORS[colors.top % OUTFIT_COLORS.len()]) {
        for (key, to) in BASE_RAMP.iter().zip(top) {
            map.insert(*key, to);
        }
    }
    if let Some(bottom) = variant_ramp(OUTFIT_COLORS[colors.bottom % OUTFIT_COLORS.len()]) {
        for (key, to) in BOTTOM_KEY.iter().zip(bottom) {
            map.insert(*key, to);
        }
    }
    let mut out = sprite.clone();
    for px in out.pixels_mut() {
        if px[3] > 0
            && let Some(to) = map.get(&[px[0], px[1], px[2]])
        {
            px.0[..3].copy_from_slice(to);
        }
    }
    out
}

/// A simple figure in the key colours, until a look has real art.
pub fn placeholder_character() -> RgbaImage {
    const ROWS: [&str; 24] = [
        "....hhhh....",
        "...hhhhhh...",
        "..hhhhhhhh..",
        "..hssssssh..",
        "..hsessesh..",
        "..ssssssss..",
        "...ssssss...",
        "....ssss....",
        "..tttttttt..",
        ".tttttttttt.",
        ".stttttttts.",
        ".stttttttts.",
        ".stttttttts.",
        ".s.tttttt.s.",
        "...tttttt...",
        "...bbbbbb...",
        "...bbbbbb...",
        "...bb..bb...",
        "...bb..bb...",
        "...bb..bb...",
        "...bb..bb...",
        "...bb..bb...",
        "...oo..oo...",
        "..ooo..ooo..",
    ];
    let outline = Rgba([0x1b, 0x1d, 0x2b, 255]);
    let mut img = RgbaImage::new(14, ROWS.len() as u32 + 2);
    for (y, row) in ROWS.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let c = match ch {
                'h' => HAIR_KEY[2],
                's' => SKIN_KEY[2],
                't' => BASE_RAMP[2],
                'b' => BOTTOM_KEY[2],
                'o' => [0x3b, 0x2a, 0x24],
                'e' => [0x1b, 0x1d, 0x2b],
                _ => continue,
            };
            img.put_pixel(x as u32 + 1, y as u32 + 1, Rgba([c[0], c[1], c[2], 255]));
        }
    }
    let solid = img.clone();
    for (x, y, px) in img.enumerate_pixels_mut() {
        if px[3] > 0 {
            continue;
        }
        let near = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            nx >= 0
                && ny >= 0
                && (nx as u32) < solid.width()
                && (ny as u32) < solid.height()
                && solid.get_pixel(nx as u32, ny as u32)[3] > 0
        });
        if near {
            *px = outline;
        }
    }
    let (w, h) = (img.width() * 2, img.height() * 2);
    imageops::resize(&img, w, h, imageops::FilterType::Nearest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaps_only_key_colours() {
        let mut look = RgbaImage::new(3, 1);
        look.put_pixel(0, 0, Rgba([0x8a, 0x5a, 0xd8, 255]));
        look.put_pixel(1, 0, Rgba([0xff, 0x6b, 0x4a, 255]));
        look.put_pixel(2, 0, Rgba([0x12, 0x34, 0x56, 255]));
        let out = recolor_character(
            &look,
            Colors {
                skin: 0,
                hair: 4,
                top: 1,
                bottom: 0,
            },
        );
        assert_eq!(out.get_pixel(0, 0).0[..3], rgb(HAIR_COLORS[4].1[2]));
        assert_eq!(out.get_pixel(1, 0).0[..3], variant_ramp("mint").unwrap()[2]);
        assert_eq!(out.get_pixel(2, 0).0[..3], [0x12, 0x34, 0x56]);
        assert!(placeholder_character().height() > 40);
    }
}
