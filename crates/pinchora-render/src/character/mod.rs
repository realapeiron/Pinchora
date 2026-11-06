//! Player characters, drawn by one function: [`render_character`] takes a [`CharacterSpec`]
//! (look, colours, accessories, pose) and gives the finished picture. The house, visits, the
//! profile and partner cards, the hug GIFs and the editor all draw through it.
//!
//! A look is GPT-drawn art whose key colour ramps (skin, hair, top, bottom, the brown of the
//! shoes) are swapped for the player's colours; the eyes are found in the art and tinted. Each
//! accessory is drawn in code against anchor points found in the look (brow, eyes, collar,
//! torso, hands), unless the atlas has art for it (`acc_<id>`), which then wins.

mod accessories;
mod look;
mod palette;

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

use image::{Rgba, RgbaImage, imageops};
use serde::{Deserialize, Serialize};

pub use accessories::{ACCESSORIES, AccessoryArt, Slot, accessory, accessory_icon};
pub use look::{Anchors, BOTTOM_KEY, HAIR_KEY, OUTLINE, SHOE_KEY, SKIN_KEY, TOP_KEY};
pub use palette::{
    ACCESSORY_TINTS, EYE_COLORS, HAIR_COLORS, OUTFIT_COLORS, Paint, SHOE_COLORS, SKIN_TONES, Swatch, derive_ramp,
    display_rgb, parse_hex, ramp_of, shadeable,
};

use crate::sprites::{BASE_RAMP, SpriteStore};
use look::{Part, Prepared};

/// A part of the character the player colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Skin,
    Hair,
    Eyes,
    Top,
    Bottom,
    Shoes,
}

impl Channel {
    pub const ALL: [Self; 6] = [Self::Skin, Self::Hair, Self::Eyes, Self::Top, Self::Bottom, Self::Shoes];

    pub fn swatches(self) -> &'static [Swatch] {
        match self {
            Self::Skin => &SKIN_TONES,
            Self::Hair => &HAIR_COLORS,
            Self::Eyes => &EYE_COLORS,
            Self::Top | Self::Bottom => &OUTFIT_COLORS,
            Self::Shoes => &SHOE_COLORS,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Skin => "skin",
            Self::Hair => "hair",
            Self::Eyes => "eyes",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Shoes => "shoes",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Skin => "Skin",
            Self::Hair => "Hair",
            Self::Eyes => "Eyes",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Shoes => "Shoes",
        }
    }

    /// Name of a paint on this channel: the swatch's, or the hex code.
    pub fn name_of(self, paint: Paint) -> String {
        match paint {
            Paint::Swatch(i) => self
                .swatches()
                .get(usize::from(i))
                .map_or_else(|| "?".to_owned(), |s| s.name.to_owned()),
            Paint::Custom(v) => format!("#{v:06x}"),
        }
    }
}

/// The colour of every channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Colors {
    pub skin: Paint,
    pub hair: Paint,
    pub eyes: Paint,
    pub top: Paint,
    pub bottom: Paint,
    pub shoes: Paint,
}

impl Colors {
    pub fn get(&self, channel: Channel) -> Paint {
        match channel {
            Channel::Skin => self.skin,
            Channel::Hair => self.hair,
            Channel::Eyes => self.eyes,
            Channel::Top => self.top,
            Channel::Bottom => self.bottom,
            Channel::Shoes => self.shoes,
        }
    }

    pub fn set(&mut self, channel: Channel, paint: Paint) {
        let slot = match channel {
            Channel::Skin => &mut self.skin,
            Channel::Hair => &mut self.hair,
            Channel::Eyes => &mut self.eyes,
            Channel::Top => &mut self.top,
            Channel::Bottom => &mut self.bottom,
            Channel::Shoes => &mut self.shoes,
        };
        *slot = paint;
    }

    /// The four skin shades, dark to light (the art's own peach for "as drawn").
    pub fn skin_ramp(&self) -> [[u8; 3]; 4] {
        to_array(ramp_of(&SKIN_TONES, self.skin, 4, 2), SKIN_KEY)
    }

    /// Five shirt shades, dark to light.
    pub fn top_ramp(&self) -> [[u8; 3]; 5] {
        to_array(ramp_of(&OUTFIT_COLORS, self.top, 5, 2), TOP_KEY)
    }
}

fn to_array<const N: usize>(ramp: Option<Vec<[u8; 3]>>, fallback: [[u8; 3]; N]) -> [[u8; 3]; N] {
    let mut out = fallback;
    if let Some(ramp) = ramp {
        for (o, c) in out.iter_mut().zip(ramp) {
            *o = c;
        }
    }
    out
}

/// An accessory being worn, and the colour it is dyed (`None`: its own).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Worn {
    pub id: String,
    pub tint: Option<Paint>,
}

/// How the character stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pose {
    /// Facing right; the looks are drawn facing left.
    pub mirrored: bool,
    /// The hands are doing something else, so a held item is put away.
    pub hands_busy: bool,
}

/// Everything that decides how a character looks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharacterSpec {
    /// A look id from `content/looks.json`, drawn from the atlas sprite `a_<look>`.
    pub look: String,
    pub colors: Colors,
    /// At most one per slot; unknown ids are skipped.
    pub accessories: Vec<Worn>,
    pub pose: Pose,
}

/// A drawn character and where its look sits in the picture.
pub struct Composed {
    pub image: RgbaImage,
    /// The look's top-left pixel in `image`, at scale 1.
    pub origin: (i32, i32),
    pub anchors: Anchors,
}

/// The character as a picture, every art pixel `scale`×`scale`. The look is centred
/// horizontally and its feet are on the bottom row, whatever the accessories add around it.
pub fn render_character(spec: &CharacterSpec, scale: u32) -> RgbaImage {
    let composed = compose(spec);
    scale_up(&composed.image, scale.max(1))
}

/// The art behind looks and accessories: the atlas compiled into the binary, decoded once.
pub fn atlas() -> &'static SpriteStore {
    static STORE: LazyLock<SpriteStore> = LazyLock::new(SpriteStore::embedded);
    &STORE
}

/// The prepared look, made once per look id.
fn prepared(look: &str) -> Arc<Prepared> {
    static LOOKS: LazyLock<Mutex<HashMap<String, Arc<Prepared>>>> = LazyLock::new(Mutex::default);
    if let Some(p) = LOOKS.lock().expect("look cache").get(look) {
        return p.clone();
    }
    let sprite = atlas()
        .get(&format!("a_{look}"))
        .map_or_else(placeholder_character, |s| (*s).clone());
    let p = Arc::new(Prepared::new(&sprite));
    LOOKS.lock().expect("look cache").insert(look.to_owned(), p.clone());
    p
}

/// The anchor points of a look, for placing things next to the character.
pub fn look_anchors(look: &str) -> Anchors {
    prepared(look).anchors
}

/// The look in the chosen colours, before accessories.
fn paint_look(prepared: &Prepared, colors: &Colors) -> RgbaImage {
    let skin = ramp_of(&SKIN_TONES, colors.skin, 4, 2);
    let hair = ramp_of(&HAIR_COLORS, colors.hair, 4, 2);
    let top = ramp_of(&OUTFIT_COLORS, colors.top, 5, 2);
    let bottom = ramp_of(&OUTFIT_COLORS, colors.bottom, 5, 2);
    let eyes = ramp_of(&EYE_COLORS, colors.eyes, 4, 1);
    let shoes = ramp_of(&SHOE_COLORS, colors.shoes, 5, 2);
    let pick = |ramp: &Option<Vec<[u8; 3]>>, i: u8| ramp.as_ref().map(|r| r[usize::from(i).min(r.len() - 1)]);
    look::recolor(prepared, |part| match part {
        Part::Skin(i) => pick(&skin, i),
        Part::Hair(i) => pick(&hair, i),
        Part::Top(i) => pick(&top, i),
        Part::Bottom(i) => pick(&bottom, i),
        Part::Eye(i) => pick(&eyes, i),
        Part::Shoe(i) => pick(&shoes, i + 1),
        // Laces and soles: the light end of the shoe colour; as drawn while the shoes are.
        Part::ShoeShine(i) => match &shoes {
            Some(r) => Some(r[if i >= 2 { 4 } else { 3 }]),
            None => Some(SKIN_KEY[usize::from(i)]),
        },
        Part::Clear | Part::Keep => None,
    })
}

/// Room around the look for accessories, at scale 1.
const MARGIN_X: i32 = 44;
const MARGIN_TOP: i32 = 56;

/// Draws the character at scale 1 and says where the look is in the picture.
pub fn compose(spec: &CharacterSpec) -> Composed {
    let prepared = prepared(&spec.look);
    let a = prepared.anchors;
    let (w, h) = (prepared.width as i32, prepared.height as i32);
    let body = paint_look(&prepared, &spec.colors);
    let mut canvas = RgbaImage::new((w + 2 * MARGIN_X) as u32, (h + MARGIN_TOP) as u32);
    let origin = (MARGIN_X, MARGIN_TOP);

    // Each accessory in its slot's order, at most one per slot.
    let mut worn: Vec<(&AccessoryArt, Option<Paint>)> = Vec::new();
    for item in &spec.accessories {
        if let Some(art) = accessory(&item.id)
            && !worn.iter().any(|(w, _)| w.slot == art.slot)
            && !(spec.pose.hands_busy && art.slot == Slot::Hand)
        {
            worn.push((art, item.tint));
        }
    }
    worn.sort_by_key(|(art, _)| art.slot);
    let skin = spec.colors.skin_ramp();
    let drawn: Vec<accessories::Drawn> = worn
        .iter()
        .map(|(art, tint)| accessories::draw(art, *tint, &a, &skin, canvas.dimensions(), origin))
        .collect();

    // Behind the body: capes, wings, backpacks, balloons and lanterns.
    for d in &drawn {
        overlay(&mut canvas, &d.behind);
    }
    // The body, with the hair a hat covers taken away above it.
    let mut body_layer = RgbaImage::new(canvas.width(), canvas.height());
    imageops::overlay(&mut body_layer, &body, i64::from(origin.0), i64::from(origin.1));
    for d in &drawn {
        if let Some(tops) = &d.cut {
            cut_above(&mut body_layer, tops);
        }
    }
    overlay(&mut canvas, &body_layer);
    for d in &drawn {
        overlay(&mut canvas, &d.front);
        if d.grip {
            regrip(
                &mut canvas,
                &body_layer,
                (a.hands[1].0 + origin.0, a.hands[1].1 + origin.1),
            );
        }
    }

    // Keep the look centred: as much room on its left as on its right.
    let (mut x0, mut y0, mut x1) = (origin.0, origin.1, origin.0 + w - 1);
    for (x, y, px) in canvas.enumerate_pixels() {
        if px[3] > 0 {
            let (x, y) = (x as i32, y as i32);
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
        }
    }
    let pad = (origin.0 - x0).max(x1 - (origin.0 + w - 1)).max(0);
    let left = origin.0 - pad;
    let cropped = imageops::crop_imm(
        &canvas,
        left as u32,
        y0 as u32,
        (w + 2 * pad) as u32,
        (origin.1 + h - y0) as u32,
    )
    .to_image();
    let mut composed = Composed {
        image: cropped,
        origin: (pad, origin.1 - y0),
        anchors: a,
    };
    if spec.pose.mirrored {
        composed.image = imageops::flip_horizontal(&composed.image);
        composed.origin.0 = composed.image.width() as i32 - pad - w;
    }
    composed
}

/// Draws the opaque pixels of `layer` over `canvas`.
fn overlay(canvas: &mut RgbaImage, layer: &RgbaImage) {
    for (dst, src) in canvas.pixels_mut().zip(layer.pixels()) {
        if src[3] > 0 {
            *dst = *src;
        }
    }
}

/// Takes away what is above `tops[x]` in each column a hat covers, then outlines whatever of
/// the hair was cut open at the sides.
fn cut_above(layer: &mut RgbaImage, tops: &[(i32, i32)]) {
    let (w, h) = (layer.width() as i32, layer.height() as i32);
    let mut cut = vec![false; (w * h) as usize];
    for &(x, top) in tops {
        if x < 0 || x >= w {
            continue;
        }
        for y in 0..top.clamp(0, h) {
            if layer.get_pixel(x as u32, y as u32)[3] > 0 {
                layer.put_pixel(x as u32, y as u32, Rgba([0, 0, 0, 0]));
                cut[(y * w + x) as usize] = true;
            }
        }
    }
    let line = Rgba([OUTLINE[0], OUTLINE[1], OUTLINE[2], 255]);
    let snapshot = layer.clone();
    for y in 0..h {
        for x in 0..w {
            let px = snapshot.get_pixel(x as u32, y as u32);
            if px[3] == 0 || px.0[..3] == OUTLINE {
                continue;
            }
            let opened = [(1, 0), (-1, 0), (0, -1)].iter().any(|(dx, dy)| {
                let (nx, ny) = (x + dx, y + dy);
                nx >= 0 && ny >= 0 && nx < w && ny < h && cut[(ny * w + nx) as usize]
            });
            if opened {
                layer.put_pixel(x as u32, y as u32, line);
            }
        }
    }
}

/// Puts the fingers of the back hand back over what it holds.
fn regrip(canvas: &mut RgbaImage, body: &RgbaImage, (hx, hy): (i32, i32)) {
    for y in hy - 3..=hy {
        for x in hx - 2..=hx + 2 {
            if x < 0 || y < 0 || x >= body.width() as i32 || y >= body.height() as i32 {
                continue;
            }
            let px = *body.get_pixel(x as u32, y as u32);
            if px[3] > 0 {
                canvas.put_pixel(x as u32, y as u32, px);
            }
        }
    }
}

/// The picture with every pixel made a `scale`×`scale` block.
pub fn scale_up(img: &RgbaImage, scale: u32) -> RgbaImage {
    if scale == 1 {
        return img.clone();
    }
    let (w, h, k) = (img.width() as usize, img.height() as usize, scale as usize);
    let row = w * k * 4;
    let mut out = vec![0u8; row * h * k];
    for (y, source) in img.as_raw().chunks_exact(w * 4).enumerate() {
        let first = y * k * row;
        for (x, px) in source.as_chunks::<4>().0.iter().enumerate() {
            for dx in 0..k {
                out[first + (x * k + dx) * 4..][..4].copy_from_slice(px);
            }
        }
        for dy in 1..k {
            out.copy_within(first..first + row, first + dy * row);
        }
    }
    RgbaImage::from_raw(img.width() * scale, img.height() * scale, out).expect("the buffer fits the picture")
}

/// Swaps an atlas accessory's coral base ramp for the tint, like furniture variants.
pub(crate) fn tint_art(sprite: &RgbaImage, ramp: &[[u8; 3]; 5]) -> RgbaImage {
    crate::sprites::recolor(sprite, ramp)
}

/// The coral ramp atlas accessories are drawn in.
pub const ACCESSORY_BASE_RAMP: [[u8; 3]; 5] = BASE_RAMP;

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
    let mut img = RgbaImage::new(14, ROWS.len() as u32 + 2);
    for (y, row) in ROWS.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let c = match ch {
                'h' => HAIR_KEY[2],
                's' => SKIN_KEY[2],
                't' => TOP_KEY[2],
                'b' => BOTTOM_KEY[2],
                'o' => SHOE_KEY[1],
                'e' => OUTLINE,
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
            *px = Rgba([OUTLINE[0], OUTLINE[1], OUTLINE[2], 255]);
        }
    }
    let (w, h) = (img.width() * 4, img.height() * 4);
    imageops::resize(&img, w, h, imageops::FilterType::Nearest)
}

#[cfg(test)]
mod tests;
