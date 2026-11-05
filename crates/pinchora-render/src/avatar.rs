use std::io::Cursor;

use image::{ImageFormat, Rgba, RgbaImage, imageops::FilterType};

/// Decodes an avatar and resizes it to `size`×`size` PNG.
/// With `pixel_grid`, it is first shrunk to that many pixels per side and blown up
/// with nearest-neighbour, which gives the pixel-art look.
pub fn prepare_avatar(bytes: &[u8], size: u32, pixel_grid: Option<u32>) -> anyhow::Result<Vec<u8>> {
    let img = image::load_from_memory(bytes)?.to_rgba8();
    let out = match pixel_grid {
        Some(grid) => {
            let small = image::imageops::resize(&img, grid, grid, FilterType::Triangle);
            image::imageops::resize(&small, size, size, FilterType::Nearest)
        }
        None => image::imageops::resize(&img, size, size, FilterType::Lanczos3),
    };
    encode(&out)
}

/// A symmetric 8×8 pixel creature derived from `seed`, for users without an avatar
/// or when the CDN is unreachable.
pub fn placeholder_avatar(seed: u64, size: u32) -> Vec<u8> {
    const PALETTES: [([u8; 3], [u8; 3]); 6] = [
        ([255, 107, 74], [29, 43, 83]),
        ([6, 214, 160], [20, 27, 61]),
        ([255, 209, 102], [46, 61, 117]),
        ([131, 56, 236], [15, 22, 49]),
        ([58, 134, 255], [20, 27, 61]),
        ([255, 77, 109], [29, 43, 83]),
    ];
    let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let (fg, bg) = PALETTES[(next() % PALETTES.len() as u64) as usize];
    let mut grid = RgbaImage::from_pixel(8, 8, Rgba([bg[0], bg[1], bg[2], 255]));
    for y in 1..7 {
        for x in 1..4 {
            if next() % 100 < 55 {
                let px = Rgba([fg[0], fg[1], fg[2], 255]);
                grid.put_pixel(x, y, px);
                grid.put_pixel(7 - x, y, px);
            }
        }
    }
    // Eyes, so every placeholder reads as a little creature.
    for (x, y) in [(2, 3), (5, 3)] {
        grid.put_pixel(x, y, Rgba([255, 241, 232, 255]));
    }
    let big = image::imageops::resize(&grid, size, size, FilterType::Nearest);
    encode(&big).expect("PNG encoding of an in-memory image cannot fail")
}

fn encode(img: &RgbaImage) -> anyhow::Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png)?;
    Ok(out.into_inner())
}
