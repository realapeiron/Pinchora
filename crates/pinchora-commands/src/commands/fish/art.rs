//! Pictures for the fishing cards: fish in the grid (or as shadows until caught), and the little
//! pixel icons of rods and baits, which have no atlas art and are drawn here.

use std::io::Cursor;

use image::{Rgba, RgbaImage};
use pinchora_game::{
    Rarity,
    fishing::{Bait, Boost},
};
use pinchora_render::{GridSlot, ItemGridCard, Theme};

use crate::{App, game::scene::Sprites};

/// What a grid slot shows.
#[derive(Debug, Clone)]
pub(super) enum Art {
    /// A fish from the atlas; `hidden` draws only its shadow, for species not caught yet.
    Fish {
        id: String,
        hidden: bool,
    },
    /// The `n`th rod, 0 being the starter.
    Rod(usize),
    Bait(BaitArt),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BaitArt {
    Worm,
    Lure,
    Crumbs,
}

impl BaitArt {
    pub(super) fn of(bait: &Bait) -> Self {
        match bait.boost {
            Boost::Rarity {
                rarity: Rarity::Uncommon,
                ..
            } => Self::Worm,
            Boost::Rarity { .. } => Self::Lure,
            Boost::Specials { .. } => Self::Crumbs,
        }
    }
}

/// One cell of a fishing grid, before its art is drawn.
pub(super) struct Slot {
    pub art: Art,
    pub name: String,
    pub line1: String,
    pub line2: String,
    pub bar: Option<f32>,
    pub frame: u32,
    pub badge: String,
    pub shell_price: bool,
}

/// An 8-slot item grid card as PNG. The art is drawn on the render thread.
pub(super) async fn grid_png(
    app: &App,
    title: String,
    subtitle: String,
    footer: String,
    slots: Vec<Slot>,
) -> anyhow::Result<Vec<u8>> {
    let sprites = app.sprites.clone();
    app.render(move |r| {
        let (bw, bh) = ItemGridCard::SPRITE_BOX;
        let slots = slots
            .into_iter()
            .map(|slot| {
                let (sprite_png, sprite_w, sprite_h) = draw(&sprites, &slot.art, bw, bh);
                GridSlot {
                    sprite_png,
                    sprite_w,
                    sprite_h,
                    name: slot.name,
                    line1: slot.line1,
                    line2: slot.line2,
                    bar: slot.bar,
                    frame: slot.frame,
                    badge: slot.badge,
                    shell_price: slot.shell_price,
                }
            })
            .collect();
        r.item_grid(&ItemGridCard {
            title,
            subtitle,
            slots,
            footer,
            theme: Theme::reef(),
        })
    })
    .await
}

/// The art scaled up by the largest whole factor that fits `max_w`×`max_h`, as PNG with its size.
fn draw(sprites: &Sprites, art: &Art, max_w: u32, max_h: u32) -> (Vec<u8>, u32, u32) {
    let sprite = match art {
        Art::Fish { id, hidden } => {
            let fish = sprites.fish(id);
            if *hidden { shadow(&fish) } else { (*fish).clone() }
        }
        Art::Rod(n) => rod_icon(*n),
        Art::Bait(kind) => bait_icon(*kind),
    };
    let factor = (max_w / sprite.width().max(1))
        .min(max_h / sprite.height().max(1))
        .clamp(1, 8);
    let (w, h) = (sprite.width() * factor, sprite.height() * factor);
    let big = image::imageops::resize(&sprite, w, h, image::imageops::FilterType::Nearest);
    let mut png = Cursor::new(Vec::new());
    let _ = big.write_to(&mut png, image::ImageFormat::Png);
    (png.into_inner(), w, h)
}

/// The fish as a dark blue shape, for a species not in the book yet.
fn shadow(sprite: &RgbaImage) -> RgbaImage {
    let mut out = sprite.clone();
    for pixel in out.pixels_mut() {
        if pixel[3] > 0 {
            *pixel = Rgba([0x1b, 0x25, 0x4f, pixel[3]]);
        }
    }
    out
}

// /Icons

const SIZE: u32 = 24;

fn put(img: &mut RgbaImage, x: i32, y: i32, color: [u8; 3]) {
    if (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) {
        img.put_pixel(x as u32, y as u32, Rgba([color[0], color[1], color[2], 255]));
    }
}

/// A line of pixels from one point to another.
fn line(img: &mut RgbaImage, from: (i32, i32), to: (i32, i32), color: [u8; 3]) {
    let (dx, dy) = ((to.0 - from.0).abs(), -(to.1 - from.1).abs());
    let (sx, sy) = (if from.0 < to.0 { 1 } else { -1 }, if from.1 < to.1 { 1 } else { -1 });
    let (mut x, mut y, mut err) = (from.0, from.1, dx + dy);
    loop {
        put(img, x, y, color);
        if (x, y) == to {
            break;
        }
        let doubled = 2 * err;
        if doubled >= dy {
            err += dy;
            x += sx;
        }
        if doubled <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// A filled ellipse.
fn blob(img: &mut RgbaImage, centre: (f32, f32), radii: (f32, f32), color: [u8; 3]) {
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let (dx, dy) = ((x as f32 - centre.0) / radii.0, (y as f32 - centre.1) / radii.1);
            if dx * dx + dy * dy <= 1.0 {
                put(img, x, y, color);
            }
        }
    }
}

/// A rod in a rising set of colours: plain wood, reed, pearl, tide and starlight.
fn rod_icon(n: usize) -> RgbaImage {
    const BODY: [[u8; 3]; 5] = [
        [0xb8, 0x8a, 0x5a],
        [0x7f, 0xb0, 0x5e],
        [0xe8, 0xdc, 0xe0],
        [0x4f, 0xb8, 0xc4],
        [0x9b, 0x7b, 0xe0],
    ];
    const GEM: [[u8; 3]; 5] = [
        [0xd8, 0xc0, 0x98],
        [0xe8, 0xf0, 0x9a],
        [0xff, 0xb8, 0xd0],
        [0xb8, 0xf4, 0xff],
        [0xff, 0xe0, 0x70],
    ];
    let (body, gem) = (BODY[n % BODY.len()], GEM[n % GEM.len()]);
    let mut img = RgbaImage::new(SIZE, SIZE);
    let dark = [0x2a, 0x2f, 0x5c];
    // The rod, with a lighter edge above it, and a dark grip at the low end.
    line(&mut img, (3, 21), (19, 5), body);
    line(&mut img, (4, 21), (20, 5), body);
    line(&mut img, (3, 20), (18, 5), gem);
    line(&mut img, (3, 21), (8, 16), dark);
    line(&mut img, (4, 21), (9, 16), dark);
    // A reel, the gem at the tip, and the line dropping to a little hook.
    blob(&mut img, (9.5, 17.0), (2.2, 2.2), [0xc9, 0xd1, 0xe6]);
    put(&mut img, 9, 17, dark);
    blob(&mut img, (20.0, 4.0), (1.6, 1.6), gem);
    line(&mut img, (21, 5), (21, 15), [0xf2, 0xf4, 0xff]);
    line(&mut img, (21, 15), (21, 18), [0xc9, 0xd1, 0xe6]);
    put(&mut img, 20, 19, [0xc9, 0xd1, 0xe6]);
    put(&mut img, 19, 18, [0xc9, 0xd1, 0xe6]);
    // A star-bright sparkle for every rod past the first.
    for k in 0..n.min(4) {
        let (x, y) = (3 + 3 * k as i32, 3 + (k as i32 % 2) * 3);
        put(&mut img, x, y, gem);
        put(&mut img, x - 1, y, gem);
        put(&mut img, x + 1, y, gem);
        put(&mut img, x, y - 1, gem);
        put(&mut img, x, y + 1, gem);
    }
    img
}

fn bait_icon(kind: BaitArt) -> RgbaImage {
    let mut img = RgbaImage::new(SIZE, SIZE);
    match kind {
        BaitArt::Worm => {
            // A soft glow, then a wriggling worm in segments with a bright head.
            blob(&mut img, (12.0, 12.0), (10.5, 8.5), [0x3d, 0x3a, 0x5c]);
            blob(&mut img, (12.0, 12.0), (8.5, 6.5), [0x6a, 0x55, 0x62]);
            for (i, x) in (3..21).enumerate() {
                let y = 12 + ((i as f32 * 0.7).sin() * 3.0).round() as i32;
                let color = if i % 3 == 0 {
                    [0xff, 0xb0, 0xc0]
                } else {
                    [0xff, 0x8f, 0xa8]
                };
                for dy in -1..=1 {
                    put(&mut img, x, y + dy, color);
                }
            }
            blob(
                &mut img,
                (20.0, 12.0 + (17.0f32 * 0.7).sin() * 3.0),
                (2.0, 2.0),
                [0xff, 0xe0, 0x70],
            );
            put(&mut img, 21, 11, [0x2a, 0x2f, 0x5c]);
            for (x, y) in [(5, 4), (18, 5), (8, 20), (16, 19)] {
                put(&mut img, x, y, [0xff, 0xf0, 0xa0]);
            }
        }
        BaitArt::Lure => {
            // A silver fish-shaped lure with a coral stripe, a hook, and a flash.
            blob(&mut img, (11.0, 12.0), (8.0, 4.5), [0xc9, 0xd1, 0xe6]);
            blob(&mut img, (11.0, 11.0), (7.0, 2.5), [0xea, 0xee, 0xff]);
            for y in 8..=16 {
                put(&mut img, 6, y, [0xff, 0x6b, 0x4a]);
            }
            line(&mut img, (19, 12), (22, 8), [0xc9, 0xd1, 0xe6]);
            line(&mut img, (19, 12), (22, 16), [0xc9, 0xd1, 0xe6]);
            put(&mut img, 15, 10, [0x2a, 0x2f, 0x5c]);
            put(&mut img, 15, 9, [0xff, 0xff, 0xff]);
            line(&mut img, (11, 17), (11, 20), [0x8f, 0xa0, 0xd8]);
            put(&mut img, 12, 20, [0x8f, 0xa0, 0xd8]);
            put(&mut img, 10, 20, [0x8f, 0xa0, 0xd8]);
            for (x, y) in [(3, 5), (20, 4)] {
                put(&mut img, x, y, [0xff, 0xff, 0xff]);
                put(&mut img, x - 1, y, [0xea, 0xee, 0xff]);
                put(&mut img, x + 1, y, [0xea, 0xee, 0xff]);
                put(&mut img, x, y - 1, [0xea, 0xee, 0xff]);
                put(&mut img, x, y + 1, [0xea, 0xee, 0xff]);
            }
        }
        BaitArt::Crumbs => {
            // A little heap of golden crumbs with a sparkle.
            for (x, y, r, c) in [
                (12.0, 16.0, 4.0, [0xd8, 0xa8, 0x5a]),
                (7.5, 17.5, 3.0, [0xc8, 0x92, 0x48]),
                (16.5, 17.5, 3.2, [0xe6, 0xbc, 0x70]),
                (10.0, 12.0, 2.6, [0xe6, 0xbc, 0x70]),
                (14.5, 12.5, 2.4, [0xd8, 0xa8, 0x5a]),
                (12.5, 9.0, 2.0, [0xf2, 0xd2, 0x88]),
            ] {
                blob(&mut img, (x, y), (r, r * 0.8), c);
            }
            for (x, y) in [(5, 8), (19, 8), (12, 4)] {
                put(&mut img, x, y, [0xff, 0xf0, 0xa0]);
                put(&mut img, x, y + 1, [0xff, 0xf0, 0xa0]);
                put(&mut img, x - 1, y, [0xff, 0xf0, 0xa0]);
                put(&mut img, x + 1, y, [0xff, 0xf0, 0xa0]);
            }
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_drawn_and_all_different() {
        let mut drawn = Vec::new();
        for n in 0..5 {
            drawn.push(rod_icon(n));
        }
        for kind in [BaitArt::Worm, BaitArt::Lure, BaitArt::Crumbs] {
            drawn.push(bait_icon(kind));
        }
        for (i, icon) in drawn.iter().enumerate() {
            assert!(
                icon.pixels().filter(|p| p[3] > 0).count() > 40,
                "icon {i} is nearly empty"
            );
            for other in &drawn[i + 1..] {
                assert_ne!(icon, other);
            }
        }
    }

    #[test]
    fn a_shadow_keeps_the_shape_and_loses_the_colour() {
        let sprites = Sprites::default();
        let fish = sprites.fish("toaststripe_minnow");
        let dark = shadow(&fish);
        assert_eq!(dark.dimensions(), fish.dimensions());
        for (a, b) in fish.pixels().zip(dark.pixels()) {
            assert_eq!(a[3], b[3]);
            if a[3] > 0 {
                assert_eq!(&b.0[..3], &[0x1b, 0x25, 0x4f]);
            }
        }
    }

    #[test]
    fn art_fits_the_grid_box() {
        let sprites = Sprites::default();
        let (bw, bh) = ItemGridCard::SPRITE_BOX;
        for art in [
            Art::Rod(2),
            Art::Bait(BaitArt::Lure),
            Art::Fish {
                id: "aurorawoven_leviathan".to_owned(),
                hidden: true,
            },
        ] {
            let (png, w, h) = draw(&sprites, &art, bw, bh);
            assert!(w <= bw && h <= bh && png.starts_with(b"\x89PNG"), "{art:?}");
        }
    }
}
