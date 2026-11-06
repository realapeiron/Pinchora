//! The showcase row of the profile card: a favourite fish, a pet, a piece of furniture and a
//! thumbnail of the house, each in a small panel of its own.

use std::fmt::Write;

use image::{Rgba, RgbaImage, imageops};

use super::{PIXEL, SANS, SpriteArt, text};
use crate::{
    Theme, encode_png,
    svg::{data_uri_png, escape, notched_path, pixel_shadow, truncate_to},
};

/// What a showcase tile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowcaseKind {
    Fish,
    Pet,
    Furniture,
    /// The first room of the player's house.
    House,
}

impl ShowcaseKind {
    fn caption(self) -> &'static str {
        match self {
            Self::Fish => "FISH",
            Self::Pet => "PET",
            Self::Furniture => "FURNITURE",
            Self::House => "HOME",
        }
    }
}

/// One tile of the showcase.
pub struct ShowcaseSlot<'a> {
    pub kind: ShowcaseKind,
    /// A sprite no larger than [`ShowcaseSlot::ART_BOX`]; for the house a picture of exactly
    /// [`ShowcaseSlot::HOUSE_BOX`] pixels.
    pub art: SpriteArt<'a>,
    /// The fish, the pet's name, the furniture, the house.
    pub name: &'a str,
}

impl ShowcaseSlot<'_> {
    /// At most this many tiles fit in a row.
    pub const MAX: usize = 4;
    /// The largest sprite a tile has room for.
    pub const ART_BOX: (u32, u32) = (128, 72);
    /// The size of the house picture: what a house tile has inside its border.
    pub const HOUSE_BOX: (u32, u32) = (160, 106);
    pub(crate) const HEIGHT: f32 = 114.0;
    /// Room between tiles.
    const GAP: f32 = 8.0;
    /// A tile with a sprite is at most this wide, however few there are.
    const WIDEST: f32 = 176.0;
    /// A small sprite is not blown up more than this: a fish 24 px wide at 4× is as wide as a
    /// big pet.
    const MAX_SCALE: u32 = 4;
    /// Inside a tile, from its top: the baseline of the kind, the top of the art box (the kind sits
    /// in the corner beside it, above the sprites that are not at their tallest), the floor the
    /// sprites stand on (the bottom of the art box) and how far the name's baseline is above the
    /// tile's bottom.
    const KIND_BASELINE: f32 = 17.0;
    const ART_TOP: f32 = 18.0;
    const FLOOR: f32 = 90.0;
    const NAME_RISE: f32 = 9.0;
}

impl ShowcaseSlot<'_> {
    /// A sprite fitted to [`ShowcaseSlot::ART_BOX`], as PNG with its size: trimmed of empty
    /// margin, then scaled by a whole factor only, so every pixel of every sprite stays a whole,
    /// crisp block. Small sprites grow by the biggest factor that fits (at most
    /// [`ShowcaseSlot::MAX_SCALE`]); a sprite too big at 1× shrinks by the smallest whole
    /// divisor that fits, each new pixel the commonest colour of the block it replaces, so no
    /// colour is invented and no edge is smoothed.
    pub fn fit_sprite(sprite: &RgbaImage) -> anyhow::Result<(Vec<u8>, u32, u32)> {
        let (box_w, box_h) = Self::ART_BOX;
        let sprite = trimmed(sprite);
        let (w, h) = (sprite.width().max(1), sprite.height().max(1));
        let fitted = if w <= box_w && h <= box_h {
            let factor = (box_w / w).min(box_h / h).clamp(1, Self::MAX_SCALE);
            imageops::resize(&sprite, w * factor, h * factor, imageops::FilterType::Nearest)
        } else {
            shrunk(&sprite, w.div_ceil(box_w).max(h.div_ceil(box_h)))
        };
        let (w, h) = (fitted.width(), fitted.height());
        Ok((encode_png(&fitted)?, w, h))
    }

    /// A picture of a room cut and scaled to fill [`ShowcaseSlot::HOUSE_BOX`], as PNG with its size.
    /// A room is a scene, not a sprite: it is shrunk smoothly so the whole of it fits the tile.
    pub fn fit_house(room: &RgbaImage) -> anyhow::Result<(Vec<u8>, u32, u32)> {
        let (box_w, box_h) = Self::HOUSE_BOX;
        let k = (box_w as f32 / room.width() as f32).max(box_h as f32 / room.height() as f32);
        let covered = resize_smooth(
            room,
            ((room.width() as f32 * k).round() as u32).max(box_w),
            ((room.height() as f32 * k).round() as u32).max(box_h),
        );
        // The middle of the room: its walls and floor, not the empty corners.
        let (x, y) = ((covered.width() - box_w) / 2, (covered.height() - box_h) / 2);
        let cut = imageops::crop_imm(&covered, x, y, box_w, box_h).to_image();
        Ok((encode_png(&cut)?, box_w, box_h))
    }
}

/// The sprite without the transparent rows and columns around it; kept whole if empty.
fn trimmed(sprite: &RgbaImage) -> RgbaImage {
    let (mut x0, mut y0, mut x1, mut y1) = (sprite.width(), sprite.height(), 0, 0);
    for (x, y, px) in sprite.enumerate_pixels() {
        if px[3] > 0 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    if x1 < x0 {
        return sprite.clone();
    }
    imageops::crop_imm(sprite, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image()
}

/// `sprite` made `divisor` times smaller, each pixel the commonest opaque colour of its
/// `divisor`×`divisor` block (the first met on a tie), and transparent when less than half of the
/// block is opaque. The colours stay the sprite's own, and so do its hard edges.
fn shrunk(sprite: &RgbaImage, divisor: u32) -> RgbaImage {
    let divisor = divisor.max(1);
    let (w, h) = (sprite.width().div_ceil(divisor), sprite.height().div_ceil(divisor));
    RgbaImage::from_fn(w, h, |bx, by| {
        let mut colours: Vec<([u8; 3], u32)> = Vec::new();
        let (mut opaque, mut total) = (0, 0);
        for y in by * divisor..(by * divisor + divisor).min(sprite.height()) {
            for x in bx * divisor..(bx * divisor + divisor).min(sprite.width()) {
                let px = sprite.get_pixel(x, y);
                total += 1;
                if px[3] < 128 {
                    continue;
                }
                opaque += 1;
                let rgb = [px[0], px[1], px[2]];
                match colours.iter_mut().find(|(c, _)| *c == rgb) {
                    Some((_, n)) => *n += 1,
                    None => colours.push((rgb, 1)),
                }
            }
        }
        let best = colours
            .iter()
            .fold(None, |best: Option<&([u8; 3], u32)>, c| match best {
                Some(b) if b.1 >= c.1 => Some(b),
                _ => Some(c),
            });
        match best {
            Some(([r, g, b], _)) if opaque * 2 >= total => Rgba([*r, *g, *b, 255]),
            _ => Rgba([0, 0, 0, 0]),
        }
    })
}

/// A smooth resize that does not smear dark edges into transparent pixels: the colours are
/// weighted by their alpha while they are averaged.
fn resize_smooth(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let mut premultiplied = img.clone();
    for px in premultiplied.pixels_mut() {
        let a = u32::from(px[3]);
        for c in &mut px.0[..3] {
            *c = (u32::from(*c) * a / 255) as u8;
        }
    }
    let mut out = imageops::resize(&premultiplied, w, h, imageops::FilterType::Lanczos3);
    for px in out.pixels_mut() {
        let a = u32::from(px[3]);
        if a == 0 {
            *px = Rgba([0, 0, 0, 0]);
            continue;
        }
        for c in &mut px.0[..3] {
            *c = (u32::from(*c) * 255 / a).min(255) as u8;
        }
    }
    out
}

/// Draws up to [`ShowcaseSlot::MAX`] tiles centred in the `width` pixels from `x`, tops at `y`.
/// Tiles with a sprite share what the house picture leaves, up to a width of their own. Every
/// edge lies on a whole pixel, so nothing is blended.
pub(crate) fn draw_row(out: &mut String, t: &Theme, slots: &[ShowcaseSlot<'_>], x: f32, y: f32, width: f32) {
    let slots = &slots[..slots.len().min(ShowcaseSlot::MAX)];
    if slots.is_empty() {
        return;
    }
    let house = slots.iter().filter(|s| s.kind == ShowcaseKind::House).count() as f32;
    let others = slots.len() as f32 - house;
    let gaps = (slots.len() - 1) as f32 * ShowcaseSlot::GAP;
    let house_w = ShowcaseSlot::HOUSE_BOX.0 as f32 + 8.0;
    let tile_w = if others > 0.0 {
        ((width - gaps - house * house_w) / others)
            .min(ShowcaseSlot::WIDEST)
            .floor()
    } else {
        0.0
    };
    let total = others * tile_w + house * house_w + gaps;
    let mut at = x + ((width - total) / 2.0).floor();
    for slot in slots {
        let w = if slot.kind == ShowcaseKind::House {
            house_w
        } else {
            tile_w
        };
        match slot.kind {
            ShowcaseKind::House => house_tile(out, t, slot, at, y, w),
            _ => sprite_tile(out, t, slot, at, y, w),
        }
        at += w + ShowcaseSlot::GAP;
    }
}

/// A panel with the kind in its corner, the sprite in the middle and the name under it. Fish
/// swim in the middle of the art box; everything else stands on the floor of it, on a shadow.
fn sprite_tile(out: &mut String, t: &Theme, slot: &ShowcaseSlot<'_>, x: f32, y: f32, w: f32) {
    t.panel(out, x, y, w, ShowcaseSlot::HEIGHT, 4.0);
    text(
        out,
        x + 14.0,
        y + ShowcaseSlot::KIND_BASELINE,
        10.0,
        PIXEL,
        false,
        &t.muted,
        "start",
        slot.kind.caption(),
    );
    let (art_w, art_h) = (slot.art.w as f32, slot.art.h as f32);
    let (mid_x, floor) = ((x + w / 2.0).floor(), y + ShowcaseSlot::FLOOR);
    let top = if slot.kind == ShowcaseKind::Fish {
        let room = ShowcaseSlot::FLOOR - ShowcaseSlot::ART_TOP;
        y + ShowcaseSlot::ART_TOP + ((room - art_h) / 2.0).floor()
    } else {
        pixel_shadow(out, mid_x, floor, (art_w * 0.4).clamp(10.0, 40.0), 3.0, 0.3);
        floor - art_h
    };
    let _ = write!(
        out,
        r#"<image x="{}" y="{top}" width="{art_w}" height="{art_h}" image-rendering="optimizeSpeed" href="{}"/>"#,
        mid_x - (art_w / 2.0).floor(),
        data_uri_png(slot.art.png)
    );
    text(
        out,
        mid_x,
        y + ShowcaseSlot::HEIGHT - ShowcaseSlot::NAME_RISE,
        13.0,
        SANS,
        false,
        &t.ink,
        "middle",
        &escape(&truncate_to(slot.name, 13.0, w - 20.0)),
    );
}

/// The house picture fills its panel; the kind and the name sit on dark chips over it.
fn house_tile(out: &mut String, t: &Theme, slot: &ShowcaseSlot<'_>, x: f32, y: f32, w: f32) {
    let h = ShowcaseSlot::HEIGHT;
    t.panel(out, x, y, w, h, 4.0);
    let _ = write!(
        out,
        r#"<clipPath id="house"><path d="{}"/></clipPath><image x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="xMidYMid slice" clip-path="url(#house)" href="{}"/>"#,
        notched_path(x + 4.0, y + 4.0, w - 8.0, h - 8.0, 4.0),
        x + 4.0,
        y + 4.0,
        w - 8.0,
        h - 8.0,
        data_uri_png(slot.art.png)
    );
    // The kind, top left, on the same line as the kind of the sprite tiles beside it.
    let _ = write!(
        out,
        r##"<path d="{}" fill="#0b1029" fill-opacity="0.78"/>"##,
        notched_path(x + 8.0, y + 6.0, 46.0, 16.0, 2.0)
    );
    text(
        out,
        x + 14.0,
        y + ShowcaseSlot::KIND_BASELINE,
        10.0,
        PIXEL,
        false,
        &t.muted,
        "start",
        slot.kind.caption(),
    );
    // The name, along the bottom.
    let _ = write!(
        out,
        r##"<rect x="{}" y="{}" width="{}" height="22" fill="#0b1029" fill-opacity="0.78"/>"##,
        x + 4.0,
        y + h - 26.0,
        w - 8.0
    );
    text(
        out,
        x + w / 2.0,
        y + h - 10.0,
        13.0,
        SANS,
        false,
        &t.ink,
        "middle",
        &escape(&truncate_to(slot.name, 13.0, w - 28.0)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(w: u32, h: u32, margin: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| {
            if x < margin || y < margin || x >= w - margin || y >= h - margin {
                Rgba([0, 0, 0, 0])
            } else {
                Rgba([200, 120, 60, 255])
            }
        })
    }

    fn size_of(png: &[u8]) -> (u32, u32) {
        let image = image::load_from_memory(png).expect("a png");
        (image.width(), image.height())
    }

    /// Whether `fitted` is `sprite` made a whole number of times bigger or smaller.
    fn is_whole_scale(sprite: (u32, u32), fitted: (u32, u32)) -> bool {
        (1..=ShowcaseSlot::MAX_SCALE).any(|k| fitted == (sprite.0 * k, sprite.1 * k))
            || (1..=sprite.0.max(sprite.1)).any(|d| fitted == (sprite.0.div_ceil(d), sprite.1.div_ceil(d)))
    }

    #[test]
    fn small_sprites_grow_by_whole_factors_and_big_ones_shrink_by_whole_divisors() {
        let (box_w, box_h) = ShowcaseSlot::ART_BOX;
        // 24×12 inside 4 px of empty margin: trimmed to 24×12, then grown as far as fits, up to 4×.
        let fish = opaque(32, 20, 4);
        let (png, w, h) = ShowcaseSlot::fit_sprite(&fish).expect("fits");
        assert_eq!((w, h), (96, 48), "factor 4, the margin gone");
        assert_eq!(size_of(&png), (w, h));
        // A 24 px pet doubles, and triples where the box allows: it is no longer left at 1×.
        let (_, w, h) = ShowcaseSlot::fit_sprite(&opaque(24, 24, 0)).expect("fits");
        assert_eq!((w, h), (72, 72));
        // A 36×34 pet takes the box's whole height twice.
        let (_, w, h) = ShowcaseSlot::fit_sprite(&opaque(36, 34, 0)).expect("fits");
        assert_eq!((w, h), (72, 68));
        // Too tall for a second copy: it stays as drawn.
        let (_, w, h) = ShowcaseSlot::fit_sprite(&opaque(52, 44, 0)).expect("fits");
        assert_eq!((w, h), (52, 44));
        // A sofa far bigger than the box shrinks by the smallest divisor that fits.
        let (_, w, h) = ShowcaseSlot::fit_sprite(&opaque(160, 150, 0)).expect("fits");
        assert_eq!((w, h), (54, 50), "a third of it");
        // A tiny sprite is not blown up past four times its size.
        let (_, w, h) = ShowcaseSlot::fit_sprite(&opaque(6, 6, 0)).expect("fits");
        assert_eq!((w, h), (24, 24));
        // Whatever the size, the sprite fits and is a whole multiple or a whole divisor of itself.
        for (sw, sh) in [
            (1, 1),
            (7, 90),
            (90, 7),
            (128, 72),
            (129, 72),
            (200, 30),
            (256, 298),
            (300, 20),
            (24, 8),
        ] {
            let (png, w, h) = ShowcaseSlot::fit_sprite(&opaque(sw, sh, 0)).expect("fits");
            assert!(w <= box_w && h <= box_h, "{sw}×{sh} came out {w}×{h}");
            assert!(is_whole_scale((sw, sh), (w, h)), "{sw}×{sh} came out {w}×{h}");
            assert_eq!(size_of(&png), (w, h));
        }
        // An empty picture is kept whole, not turned into nothing.
        assert!(ShowcaseSlot::fit_sprite(&RgbaImage::new(10, 10)).is_ok());
    }

    #[test]
    fn scaling_invents_no_colour_and_no_half_transparent_pixel() {
        // Two colours in a fine checker, a hard-edged shape with a see-through margin: shrunk by
        // 3, the result holds only those colours, each fully opaque or fully empty.
        let checker = RgbaImage::from_fn(300, 240, |x, y| {
            if !(20..280).contains(&x) || !(20..220).contains(&y) {
                Rgba([0, 0, 0, 0])
            } else if (x / 2 + y / 2) % 2 == 0 {
                Rgba([200, 120, 60, 255])
            } else {
                Rgba([30, 40, 90, 255])
            }
        });
        let (png, w, h) = ShowcaseSlot::fit_sprite(&checker).expect("fits");
        assert_eq!((w, h), (87, 67), "260×200 shrunk by 3");
        let image = image::load_from_memory(&png).expect("a png").to_rgba8();
        for px in image.pixels() {
            assert!(
                px.0 == [0, 0, 0, 0] || px.0 == [200, 120, 60, 255] || px.0 == [30, 40, 90, 255],
                "an invented pixel: {px:?}"
            );
        }
        assert!(image.pixels().any(|p| p[3] == 255), "and the shape is still there");
        // The smallest sprites are only repeated, never blended.
        let small = RgbaImage::from_fn(5, 5, |x, y| Rgba([(x * 50) as u8, (y * 50) as u8, 7, 255]));
        let (png, _, _) = ShowcaseSlot::fit_sprite(&small).expect("fits");
        let big = image::load_from_memory(&png).expect("a png").to_rgba8();
        assert!(
            big.pixels()
                .all(|p| p[2] == 7 && p[3] == 255 && p[0] % 50 == 0 && p[1] % 50 == 0)
        );
    }

    #[test]
    fn a_room_fills_the_house_box_whatever_its_shape() {
        for (w, h) in [(640, 440), (300, 300), (900, 200)] {
            let room = RgbaImage::from_pixel(w, h, Rgba([40, 40, 80, 255]));
            let (png, pw, ph) = ShowcaseSlot::fit_house(&room).expect("fits");
            assert_eq!((pw, ph), ShowcaseSlot::HOUSE_BOX);
            assert_eq!(size_of(&png), ShowcaseSlot::HOUSE_BOX);
        }
        // The picture is drawn as it is, not stretched to the tile's inside.
        assert_eq!(ShowcaseSlot::HOUSE_BOX.1 as f32, ShowcaseSlot::HEIGHT - 8.0);
    }
}
