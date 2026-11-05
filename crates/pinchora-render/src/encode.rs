//! PNG output. Pixel art rarely uses more than 256 colours, and an indexed PNG of it is about
//! a fifth of the RGBA size, which is most of the upload time for every editor click.

use std::collections::HashMap;

use image::{ImageFormat, RgbaImage};

/// Encodes an image as PNG: indexed when it has at most 256 distinct colours, RGBA otherwise.
pub fn encode_png(img: &RgbaImage) -> anyhow::Result<Vec<u8>> {
    match palette(img) {
        Some((colors, indices)) => indexed(img.width(), img.height(), &colors, &indices),
        None => {
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, ImageFormat::Png)?;
            Ok(out.into_inner())
        }
    }
}

/// The colours in first-seen order and each pixel's index, or `None` past 256 colours.
fn palette(img: &RgbaImage) -> Option<(Vec<[u8; 4]>, Vec<u8>)> {
    let mut colors: Vec<[u8; 4]> = Vec::new();
    let mut lookup: HashMap<[u8; 4], u8> = HashMap::new();
    let mut indices = Vec::with_capacity(img.as_raw().len() / 4);
    let mut last: Option<([u8; 4], u8)> = None;
    for px in img.pixels() {
        let c = px.0;
        let index = match last {
            Some((prev, i)) if prev == c => i,
            _ => match lookup.get(&c) {
                Some(&i) => i,
                None => {
                    if colors.len() == 256 {
                        return None;
                    }
                    let i = colors.len() as u8;
                    colors.push(c);
                    lookup.insert(c, i);
                    i
                }
            },
        };
        last = Some((c, index));
        indices.push(index);
    }
    Some((colors, indices))
}

fn indexed(width: u32, height: u32, colors: &[[u8; 4]], indices: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Balanced);
        encoder.set_palette(colors.iter().flat_map(|c| [c[0], c[1], c[2]]).collect::<Vec<u8>>());
        let alpha: Vec<u8> = colors.iter().map(|c| c[3]).collect();
        if let Some(last_clear) = alpha.iter().rposition(|&a| a < 255) {
            encoder.set_trns(alpha[..=last_clear].to_vec());
        }
        let mut writer = encoder.write_header()?;
        writer.write_image_data(indices)?;
        writer.finish()?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;

    #[test]
    fn indexed_png_round_trips() {
        let mut img = RgbaImage::new(40, 30);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 6) as u8, (y * 8) as u8, 90, if x < 3 { 0 } else { 255 }]);
        }
        let png = encode_png(&img).unwrap();
        let back = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(back, img);
    }

    #[test]
    fn many_colours_fall_back_to_rgba() {
        let mut img = RgbaImage::new(64, 64);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([x as u8 * 4, y as u8 * 4, 7, 255]);
        }
        let png = encode_png(&img).unwrap();
        assert_eq!(image::load_from_memory(&png).unwrap().to_rgba8(), img);
    }
}
