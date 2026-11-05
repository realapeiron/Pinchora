use std::fmt::Write;

use image::{Rgba, RgbaImage, imageops};

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    character::{Colors, HAIR_COLORS, OUTFIT_COLORS, SKIN_TONES, outfit_name},
    sprites::variant_ramp,
    svg::{background, data_uri_png, escape, panel, truncate},
};

/// Thickness of the floor tile's side, in native pixels.
const SLAB: u32 = 8;
const OUTLINE: Rgba<u8> = Rgba([0x1b, 0x1d, 0x2b, 255]);

/// 860×420 character card: the character standing on a floor tile, with its look and colours.
pub struct CharacterCard {
    /// Sprite scaled up [`Self::SCALE`]× with nearest-neighbour.
    pub sprite_png: Vec<u8>,
    pub sprite_w: u32,
    pub sprite_h: u32,
    pub name: String,
    /// Display name of the look.
    pub look: String,
    pub colors: Colors,
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

fn tile_png(scale: u32) -> Vec<u8> {
    let tile = floor_tile();
    let big = imageops::resize(
        &tile,
        tile.width() * scale,
        tile.height() * scale,
        imageops::FilterType::Nearest,
    );
    let mut png = std::io::Cursor::new(Vec::new());
    big.write_to(&mut png, image::ImageFormat::Png)
        .expect("PNG encoding of an in-memory image cannot fail");
    png.into_inner()
}

impl CharacterCard {
    /// The sprite is pre-scaled by this factor, and the floor tile matches it.
    pub const SCALE: u32 = 3;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (860.0, 420.0);
        let scale = Self::SCALE as f32;
        let mut s = String::with_capacity(64 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        panel(&mut s, 32.0, 32.0, 300.0, 356.0, 8.0, &t.panel, &t.accent);

        // The tile's top face is centred on `centre`; figures stand 6 native pixels below it.
        let (cx, centre) = (182.0, 292.0);
        let _ = write!(
            s,
            r##"<image x="{}" y="{}" width="{}" height="{}" href="{}"/>"##,
            cx - 32.0 * scale,
            centre - 16.0 * scale,
            64.0 * scale,
            (32 + SLAB + 1) as f32 * scale,
            data_uri_png(&tile_png(Self::SCALE)),
        );
        let feet = centre + 6.0 * scale;
        let _ = write!(
            s,
            r##"<ellipse cx="{cx}" cy="{}" rx="{}" ry="{}" fill="#000000" fill-opacity="0.3"/><image x="{}" y="{}" width="{}" height="{}" href="{}"/>"##,
            centre + 4.0 * scale,
            14.0 * scale,
            4.0 * scale,
            cx - self.sprite_w as f32 / 2.0,
            feet - self.sprite_h as f32,
            self.sprite_w,
            self.sprite_h,
            data_uri_png(&self.sprite_png)
        );

        text(
            &mut s,
            364.0,
            80.0,
            40.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&truncate(&self.name, 18)),
        );
        text(
            &mut s,
            366.0,
            110.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            "YOUR CHARACTER",
        );

        let c = self.colors;
        let outfit = |i: usize| {
            let ramp = variant_ramp(OUTFIT_COLORS[i % OUTFIT_COLORS.len()]).map_or(0, |r| {
                (u32::from(r[2][0]) << 16) | (u32::from(r[2][1]) << 8) | u32::from(r[2][2])
            });
            (outfit_name(i), ramp)
        };
        let (top, bottom) = (outfit(c.top), outfit(c.bottom));
        let rows = [
            ("LOOK", self.look.clone(), None),
            (
                "SKIN",
                SKIN_TONES[c.skin % SKIN_TONES.len()].0.to_owned(),
                Some(SKIN_TONES[c.skin % SKIN_TONES.len()].1[2]),
            ),
            (
                "HAIR",
                HAIR_COLORS[c.hair % HAIR_COLORS.len()].0.to_owned(),
                Some(HAIR_COLORS[c.hair % HAIR_COLORS.len()].1[2]),
            ),
            ("TOP", top.0, Some(top.1)),
            ("BOTTOM", bottom.0, Some(bottom.1)),
        ];
        for (i, (label, value, swatch)) in rows.iter().enumerate() {
            let y = 140.0 + i as f32 * 48.0;
            panel(&mut s, 364.0, y, 464.0, 40.0, 4.0, &t.panel, &t.panel_edge);
            text(&mut s, 382.0, y + 25.0, 13.0, PIXEL, false, &t.muted, "start", label);
            text(
                &mut s,
                500.0,
                y + 28.0,
                22.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate(value, 20)),
            );
            if let Some(rgb) = swatch {
                panel(
                    &mut s,
                    828.0 - 52.0,
                    y + 6.0,
                    28.0,
                    28.0,
                    4.0,
                    &format!("#{rgb:06x}"),
                    &t.ink,
                );
            }
        }
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Renderer, character::placeholder_character};

    #[test]
    fn renders_every_colour_choice() {
        let r = Renderer::new();
        let sprite = placeholder_character();
        let mut png = std::io::Cursor::new(Vec::new());
        sprite
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("encode sprite");
        let card = |colors| CharacterCard {
            sprite_png: png.get_ref().clone(),
            sprite_w: sprite.width(),
            sprite_h: sprite.height(),
            name: "<Pinch> & co".to_owned(),
            look: "Tidewalker".to_owned(),
            colors,
            theme: Theme::reef(),
        };
        // The last index of every table, and one past it (indices wrap rather than panic).
        for colors in [
            Colors::default(),
            Colors {
                skin: SKIN_TONES.len() - 1,
                hair: HAIR_COLORS.len() - 1,
                top: OUTFIT_COLORS.len() - 1,
                bottom: OUTFIT_COLORS.len() - 1,
            },
            Colors {
                skin: SKIN_TONES.len(),
                hair: HAIR_COLORS.len(),
                top: OUTFIT_COLORS.len(),
                bottom: OUTFIT_COLORS.len(),
            },
        ] {
            assert!(
                r.character_card(&card(colors))
                    .expect("renders")
                    .starts_with(b"\x89PNG")
            );
        }
        let svg = card(Colors::default()).to_svg();
        assert!(svg.contains("&lt;Pinch&gt; &amp; co"));
        assert!(svg.contains("Porcelain") && svg.contains("Ink") && svg.contains("Coral"));
    }
}
