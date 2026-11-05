//! The showcase row of the profile card: a favourite fish, a pet, a piece of furniture and a
//! thumbnail of the house, each in a small panel of its own.

use std::fmt::Write;

use image::{Rgba, RgbaImage, imageops};

use super::{PIXEL, SANS, SpriteArt, text};
use crate::{
    Theme, encode_png,
    svg::{data_uri_png, escape, notched_path, truncate_to},
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
    pub const ART_BOX: (u32, u32) = (96, 46);
    /// The size of the house picture.
    pub const HOUSE_BOX: (u32, u32) = (160, 88);
    pub(crate) const HEIGHT: f32 = 96.0;
    /// Room between tiles.
    const GAP: f32 = 8.0;
    /// A tile with a sprite is at most this wide, however few there are.
    const WIDEST: f32 = 176.0;
}

impl ShowcaseSlot<'_> {
    /// A sprite fitted to [`ShowcaseSlot::ART_BOX`], as PNG with its size: trimmed of empty
    /// margin, scaled up by a whole factor when it is small (the pixels stay crisp) and
    /// smoothly down when it is big.
    pub fn fit_sprite(sprite: &RgbaImage) -> anyhow::Result<(Vec<u8>, u32, u32)> {
        let (box_w, box_h) = Self::ART_BOX;
        let sprite = trimmed(sprite);
        let (w, h) = (sprite.width(), sprite.height());
        let fitted = if w <= box_w && h <= box_h {
            let factor = (box_w / w).min(box_h / h).clamp(1, 4);
            imageops::resize(&sprite, w * factor, h * factor, imageops::FilterType::Nearest)
        } else {
            let k = (box_w as f32 / w as f32).min(box_h as f32 / h as f32);
            let size = |n: u32| ((n as f32 * k).round() as u32).max(1);
            resize_smooth(&sprite, size(w), size(h))
        };
        let (w, h) = (fitted.width(), fitted.height());
        Ok((encode_png(&fitted)?, w, h))
    }

    /// A picture of a room cut and scaled to fill [`ShowcaseSlot::HOUSE_BOX`], as PNG with its size.
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
/// Tiles with a sprite share what the house picture leaves, up to a width of their own.
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
        ((width - gaps - house * house_w) / others).min(ShowcaseSlot::WIDEST)
    } else {
        0.0
    };
    let total = others * tile_w + house * house_w + gaps;
    let mut at = x + (width - total) / 2.0;
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

/// A panel with the kind in its corner, the sprite in the middle and the name under it.
fn sprite_tile(out: &mut String, t: &Theme, slot: &ShowcaseSlot<'_>, x: f32, y: f32, w: f32) {
    t.panel(out, x, y, w, ShowcaseSlot::HEIGHT, 4.0);
    text(
        out,
        x + 14.0,
        y + 22.0,
        10.0,
        PIXEL,
        false,
        &t.muted,
        "start",
        slot.kind.caption(),
    );
    let (art_w, art_h) = (slot.art.w as f32, slot.art.h as f32);
    let (mid_x, floor) = (x + w / 2.0, y + 26.0 + ShowcaseSlot::ART_BOX.1 as f32);
    if slot.kind != ShowcaseKind::Fish {
        // Pets and furniture stand on something.
        let _ = write!(
            out,
            r##"<ellipse cx="{mid_x}" cy="{}" rx="{}" ry="5" fill="#000000" fill-opacity="0.3"/>"##,
            floor - 1.0,
            (art_w / 2.5).clamp(10.0, 40.0)
        );
    }
    // Fish swim in the middle of the box; everything else stands on its floor.
    let top = if slot.kind == ShowcaseKind::Fish {
        y + 26.0 + (ShowcaseSlot::ART_BOX.1 as f32 - art_h) / 2.0
    } else {
        floor - art_h
    };
    let _ = write!(
        out,
        r#"<image x="{}" y="{top}" width="{art_w}" height="{art_h}" href="{}"/>"#,
        mid_x - art_w / 2.0,
        data_uri_png(slot.art.png)
    );
    text(
        out,
        mid_x,
        y + 88.0,
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
    // The kind, top left.
    let _ = write!(
        out,
        r##"<path d="{}" fill="#0b1029" fill-opacity="0.78"/>"##,
        notched_path(x + 8.0, y + 8.0, 46.0, 18.0, 2.0)
    );
    text(
        out,
        x + 14.0,
        y + 21.0,
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

    #[test]
    fn small_sprites_grow_by_whole_factors_and_big_ones_shrink_to_the_box() {
        let (box_w, box_h) = ShowcaseSlot::ART_BOX;
        // 24×12 inside 4 px of empty margin: trimmed to 24×12, then doubled to fit 96×46 (factor 3: 72×36).
        let fish = opaque(32, 20, 4);
        let (png, w, h) = ShowcaseSlot::fit_sprite(&fish).expect("fits");
        assert_eq!((w, h), (72, 36), "factor 3, the margin gone");
        assert_eq!(size_of(&png), (w, h));
        // A sofa far bigger than the box shrinks to fit it, keeping its proportions.
        let (_, w, h) = ShowcaseSlot::fit_sprite(&opaque(160, 150, 0)).expect("fits");
        assert!(w <= box_w && h <= box_h && h == box_h, "{w}×{h}");
        assert!((w as f32 / h as f32 - 160.0 / 150.0).abs() < 0.05);
        // A tiny sprite is not blown up past four times its size.
        let (_, w, h) = ShowcaseSlot::fit_sprite(&opaque(6, 6, 0)).expect("fits");
        assert_eq!((w, h), (24, 24));
        // An empty picture is kept whole, not turned into nothing.
        assert!(ShowcaseSlot::fit_sprite(&RgbaImage::new(10, 10)).is_ok());
    }

    #[test]
    fn shrinking_does_not_smear_dark_edges_into_the_transparent_pixels() {
        let (png, _, _) = ShowcaseSlot::fit_sprite(&opaque(200, 160, 20)).expect("fits");
        let image = image::load_from_memory(&png).expect("a png").to_rgba8();
        for px in image.pixels().filter(|p| p[3] > 0) {
            assert!(px[0] >= 190 && px[1] >= 110, "an edge pixel went dark: {px:?}");
        }
    }

    #[test]
    fn a_room_fills_the_house_box_whatever_its_shape() {
        for (w, h) in [(640, 440), (300, 300), (900, 200)] {
            let room = RgbaImage::from_pixel(w, h, Rgba([40, 40, 80, 255]));
            let (png, pw, ph) = ShowcaseSlot::fit_house(&room).expect("fits");
            assert_eq!((pw, ph), ShowcaseSlot::HOUSE_BOX);
            assert_eq!(size_of(&png), ShowcaseSlot::HOUSE_BOX);
        }
    }
}
