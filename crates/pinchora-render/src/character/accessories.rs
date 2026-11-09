//! Accessories drawn in code as pixel art, placed on a look's anchors and sized to its head and
//! body, so one hat fits every look. Each is tinted from a five-shade ramp. When the atlas has
//! art for one (`acc_<id>`), that art is placed on the same anchor instead.

use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::character::ACCESSORY_TINTS;
use crate::character::Paint;
use crate::character::atlas;
use crate::character::look::Anchors;
use crate::character::look::OUTLINE;
use crate::character::palette::derive_ramp;
use crate::character::palette::ramp_of;
use crate::character::palette::rgb;
use crate::character::tint_art;

/// Where an accessory is worn; one per slot. Ordered as they are drawn over each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Back,
    Neck,
    Face,
    Head,
    Hand,
}

impl Slot {
    pub const ALL: [Self; 5] = [Self::Head, Self::Face, Self::Neck, Self::Back, Self::Hand];

    pub fn key(self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Face => "face",
            Self::Neck => "neck",
            Self::Back => "back",
            Self::Hand => "hand",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Head => "Head",
            Self::Face => "Face",
            Self::Neck => "Neck",
            Self::Back => "Back",
            Self::Hand => "Hand",
        }
    }
}

/// How one accessory is drawn.
pub struct AccessoryArt {
    pub id: &'static str,
    pub slot: Slot,
    /// Its own main colour, `0xrrggbb`.
    pub tint: u32,
    /// Whether players can dye it.
    pub tintable: bool,
    /// A hat that hides the hair above it.
    pub cuts_hair: bool,
    /// Drawn behind the body (capes, wings, a balloon on its string).
    pub behind: bool,
    /// Held in the back hand: the fingers are drawn again over it.
    pub grip: bool,
    draw: fn(&mut Pens, &Ctx<'_>),
}

const fn art(
    id: &'static str,
    slot: Slot,
    tint: u32,
    tintable: bool,
    flags: (bool, bool, bool),
    draw: fn(&mut Pens, &Ctx<'_>),
) -> AccessoryArt {
    AccessoryArt {
        id,
        slot,
        tint,
        tintable,
        cuts_hair: flags.0,
        behind: flags.1,
        grip: flags.2,
        draw,
    }
}

const PLAIN: (bool, bool, bool) = (false, false, false);
const HAT: (bool, bool, bool) = (true, false, false);
const BEHIND: (bool, bool, bool) = (false, true, false);
const HELD: (bool, bool, bool) = (false, false, true);

/// Every accessory there is art for, by slot.
pub const ACCESSORIES: [AccessoryArt; 31] = [
    art("straw_hat", Slot::Head, 0xe23d52, true, HAT, straw_hat),
    art("beanie", Slot::Head, 0x4aa8ec, true, HAT, beanie),
    art("sailor_cap", Slot::Head, 0x34458a, true, HAT, sailor_cap),
    art("captain_hat", Slot::Head, 0x2b3870, false, HAT, captain_hat),
    art("top_hat", Slot::Head, 0xe23d52, true, HAT, top_hat),
    art("witch_hat", Slot::Head, 0x7d55a3, true, HAT, witch_hat),
    art("crown", Slot::Head, 0xffd166, false, PLAIN, crown),
    art("bow", Slot::Head, 0xff7fa8, true, PLAIN, bow),
    art("flower_crown", Slot::Head, 0xff7fa8, true, PLAIN, flower_crown),
    art("round_glasses", Slot::Face, 0x4a4e62, true, PLAIN, round_glasses),
    art("sunglasses", Slot::Face, 0x2c2f3f, true, PLAIN, sunglasses),
    art("heart_glasses", Slot::Face, 0xff5c8a, true, PLAIN, heart_glasses),
    art("eyepatch", Slot::Face, 0x2c2f3f, false, PLAIN, eyepatch),
    art("freckles", Slot::Face, 0xb0684a, false, PLAIN, freckles),
    art("blush", Slot::Face, 0xff7f96, false, PLAIN, blush),
    art("scarf", Slot::Neck, 0xe23d52, true, PLAIN, scarf),
    art("shell_necklace", Slot::Neck, 0xff9e8a, false, PLAIN, shell_necklace),
    art("pearl_necklace", Slot::Neck, 0xf0eef8, false, PLAIN, pearl_necklace),
    art("bow_tie", Slot::Neck, 0xe23d52, true, PLAIN, bow_tie),
    art("bandana", Slot::Neck, 0x4a78c0, true, PLAIN, bandana),
    art("backpack", Slot::Back, 0xff8a30, true, PLAIN, backpack),
    art("fairy_wings", Slot::Back, 0x9fe6ff, true, BEHIND, fairy_wings),
    art("bat_wings", Slot::Back, 0x5a3a7a, true, BEHIND, bat_wings),
    art("cape", Slot::Back, 0xe23d52, true, PLAIN, cape),
    art("surfboard", Slot::Back, 0x4aa8ec, true, BEHIND, surfboard),
    art("lantern", Slot::Hand, 0xffd166, false, HELD, lantern),
    art("fishing_rod", Slot::Hand, 0xe23d52, false, HELD, fishing_rod),
    art("ice_cream", Slot::Hand, 0xff8fb1, true, HELD, ice_cream),
    art("balloon", Slot::Hand, 0xe23d52, true, HELD, balloon),
    art("flower", Slot::Hand, 0xff7fa8, true, HELD, flower),
    art("star_wand", Slot::Hand, 0xffd166, true, HELD, star_wand),
];

pub fn accessory(id: &str) -> Option<&'static AccessoryArt> {
    ACCESSORIES.iter().find(|a| a.id == id)
}

// /Colours

type Ramp = [[u8; 3]; 5];

const GOLD: Ramp = [
    [0x7a, 0x4a, 0x10],
    [0xc9, 0x8a, 0x1e],
    [0xff, 0xd1, 0x66],
    [0xff, 0xe9, 0xa0],
    [0xff, 0xf7, 0xda],
];
const WHITE: Ramp = [
    [0x8a, 0x90, 0xaa],
    [0xbe, 0xc3, 0xd6],
    [0xe6, 0xe9, 0xf2],
    [0xf6, 0xf7, 0xfc],
    [0xff, 0xff, 0xff],
];
const CHARCOAL: Ramp = [
    [0x26, 0x28, 0x38],
    [0x34, 0x37, 0x4a],
    [0x47, 0x4b, 0x61],
    [0x66, 0x6b, 0x84],
    [0x9c, 0xa1, 0xb8],
];
const STRAW: Ramp = [
    [0x7d, 0x5a, 0x24],
    [0xb3, 0x8a, 0x3c],
    [0xe2, 0xbb, 0x62],
    [0xf2, 0xd8, 0x8c],
    [0xff, 0xf0, 0xc4],
];
const WOOD: Ramp = [
    [0x4a, 0x2e, 0x1c],
    [0x6e, 0x46, 0x28],
    [0x9a, 0x6a, 0x3c],
    [0xc0, 0x8e, 0x58],
    [0xe0, 0xb8, 0x84],
];
const LEAF: Ramp = [
    [0x1f, 0x5a, 0x34],
    [0x2f, 0x7d, 0x4f],
    [0x4f, 0xae, 0x5a],
    [0x7f, 0xd4, 0x7a],
    [0xc0, 0xf0, 0xa0],
];
const LINE: [u8; 3] = OUTLINE;

fn ramp5(v: u32) -> Ramp {
    let r = derive_ramp(rgb(v), 5, 2);
    [r[0], r[1], r[2], r[3], r[4]]
}

// /Drawing

/// A layer the size of the canvas, drawn in the look's own coordinates.
pub(crate) struct Pen {
    img: RgbaImage,
    ox: i32,
    oy: i32,
    /// The box around everything drawn, in canvas pixels, so scans stay small.
    drawn: Option<(u32, u32, u32, u32)>,
}

impl Pen {
    fn new((w, h): (u32, u32), (ox, oy): (i32, i32)) -> Self {
        Self {
            img: RgbaImage::new(w, h),
            ox,
            oy,
            drawn: None,
        }
    }

    fn inside(&self, x: i32, y: i32) -> Option<(u32, u32)> {
        let (cx, cy) = (x + self.ox, y + self.oy);
        (cx >= 0 && cy >= 0 && cx < self.img.width() as i32 && cy < self.img.height() as i32)
            .then_some((cx as u32, cy as u32))
    }

    fn put(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if let Some((cx, cy)) = self.inside(x, y) {
            self.img.put_pixel(cx, cy, Rgba([c[0], c[1], c[2], 255]));
            self.drawn = Some(match self.drawn {
                None => (cx, cy, cx, cy),
                Some((x0, y0, x1, y1)) => (x0.min(cx), y0.min(cy), x1.max(cx), y1.max(cy)),
            });
        }
    }

    fn get(&self, x: i32, y: i32) -> Option<[u8; 3]> {
        let (cx, cy) = self.inside(x, y)?;
        let p = self.img.get_pixel(cx, cy);
        (p[3] > 0).then_some([p[0], p[1], p[2]])
    }

    fn has(&self, x: i32, y: i32) -> bool {
        self.get(x, y).is_some()
    }

    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 3]) {
        for y in y0.min(y1)..=y0.max(y1) {
            for x in x0.min(x1)..=x0.max(x1) {
                self.put(x, y, c);
            }
        }
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 3]) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = ((x1 - x0).signum(), (y1 - y0).signum());
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.put(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Pixels whose centres are inside the ellipse.
    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: [u8; 3]) {
        for y in (cy - ry).floor() as i32..=(cy + ry).ceil() as i32 {
            for x in (cx - rx).floor() as i32..=(cx + rx).ceil() as i32 {
                let (dx, dy) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
                if dx * dx + dy * dy <= 1.0 {
                    self.put(x, y, c);
                }
            }
        }
    }

    /// A filled polygon: pixels whose centres are inside it (even-odd rule).
    fn poly(&mut self, pts: &[(f32, f32)], c: [u8; 3]) {
        let (y0, y1) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let (x0, x1) = pts
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        for y in y0.floor() as i32..=y1.ceil() as i32 {
            for x in x0.floor() as i32..=x1.ceil() as i32 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let mut inside = false;
                let mut j = pts.len() - 1;
                for i in 0..pts.len() {
                    let ((xi, yi), (xj, yj)) = (pts[i], pts[j]);
                    if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
                        inside = !inside;
                    }
                    j = i;
                }
                if inside {
                    self.put(x, y, c);
                }
            }
        }
    }

    /// The top half of an ellipse whose widest row is `base`.
    fn dome(&mut self, cx: f32, base: i32, rx: f32, ry: f32, c: [u8; 3]) {
        for y in (base as f32 - ry).floor() as i32..=base {
            for x in (cx - rx).floor() as i32..=(cx + rx).ceil() as i32 {
                let (dx, dy) = ((x as f32 + 0.5 - cx) / rx, (y - base) as f32 / ry);
                if dx * dx + dy * dy <= 1.0 {
                    self.put(x, y, c);
                }
            }
        }
    }

    /// Text-row pixel art with its top-left at (x0, y0); `key` gives each character's colour.
    fn rows(&mut self, x0: i32, y0: i32, rows: &[&str], key: impl Fn(char) -> Option<[u8; 3]>) {
        for (dy, row) in rows.iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                if let Some(c) = key(ch) {
                    self.put(x0 + dx as i32, y0 + dy as i32, c);
                }
            }
        }
    }

    /// Light on the top and left edges of every pixel of colour `base`, shade on the bottom and right.
    fn bevel(&mut self, base: [u8; 3], light: [u8; 3], dark: [u8; 3]) {
        let mut changes = Vec::new();
        for (x, y) in self.cells() {
            if self.get(x, y) != Some(base) {
                continue;
            }
            if !self.has(x, y - 1) || !self.has(x - 1, y) {
                changes.push((x, y, light));
            } else if !self.has(x, y + 1) || !self.has(x + 1, y) {
                changes.push((x, y, dark));
            }
        }
        for (x, y, c) in changes {
            self.put(x, y, c);
        }
    }

    /// Every drawn pixel, in look coordinates.
    fn cells(&self) -> Vec<(i32, i32)> {
        let Some((x0, y0, x1, y1)) = self.drawn else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                if self.img.get_pixel(x, y)[3] > 0 {
                    out.push((x as i32 - self.ox, y as i32 - self.oy));
                }
            }
        }
        out
    }

    /// A one-pixel outline around everything drawn so far.
    fn outline(&mut self) {
        let cells = self.cells();
        for (x, y) in cells {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if !self.has(x + dx, y + dy) {
                    self.put(x + dx, y + dy, LINE);
                }
            }
        }
    }

    /// Draws `other` over this layer.
    fn absorb(&mut self, other: &Pen) {
        for (x, y) in other.cells() {
            if let Some(c) = other.get(x, y) {
                self.put(x, y, c);
            }
        }
    }

    fn blank(&self) -> Pen {
        Pen {
            img: RgbaImage::new(self.img.width(), self.img.height()),
            ox: self.ox,
            oy: self.oy,
            drawn: None,
        }
    }
}

/// The two layers an accessory draws into.
pub(crate) struct Pens {
    behind: Pen,
    front: Pen,
}

impl Pens {
    /// A part drawn on its own and outlined, then laid on `front` (or `behind`).
    fn part(&mut self, behind: bool, paint: impl FnOnce(&mut Pen)) {
        let mut part = self.front.blank();
        paint(&mut part);
        part.outline();
        if behind {
            self.behind.absorb(&part);
        } else {
            self.front.absorb(&part);
        }
    }
}

/// What a drawing knows: where the body is, its own colours and the skin's.
pub(crate) struct Ctx<'a> {
    a: &'a Anchors,
    t: Ramp,
    skin: &'a [[u8; 3]; 4],
}

/// An accessory drawn for one character.
pub(crate) struct Drawn {
    pub behind: RgbaImage,
    pub front: RgbaImage,
    /// For a hat: the top of the hat in each column, above which the hair goes.
    pub cut: Option<Vec<(i32, i32)>>,
    pub grip: bool,
}

/// Where the atlas art of a slot is pinned on the body, and which point of the art goes there.
fn pin(slot: Slot, a: &Anchors) -> ((i32, i32), (f32, f32)) {
    match slot {
        Slot::Head => ((a.head_x(), a.brow + 1), (0.5, 1.0)),
        Slot::Face => {
            let (l, r) = (a.eye_centre(0), a.eye_centre(1));
            (((l.0 + r.0) / 2, (l.1 + r.1) / 2), (0.5, 0.5))
        }
        Slot::Neck => (a.neck, (0.5, 0.2)),
        Slot::Back => ((a.torso.2 - 2, a.torso.1 + 8), (0.5, 0.5)),
        Slot::Hand => (a.hands[1], (0.5, 0.9)),
    }
}

/// The dye ramp of an accessory: its tint if it takes one, else its own colour.
fn ramp_for(art: &AccessoryArt, tint: Option<Paint>) -> Ramp {
    let picked = tint
        .filter(|_| art.tintable)
        .and_then(|paint| ramp_of(&ACCESSORY_TINTS, paint, 5, 2));
    match picked {
        Some(r) => [r[0], r[1], r[2], r[3], r[4]],
        None => ramp5(art.tint),
    }
}

pub(crate) fn draw(
    art: &AccessoryArt,
    tint: Option<Paint>,
    a: &Anchors,
    skin: &[[u8; 3]; 4],
    size: (u32, u32),
    origin: (i32, i32),
) -> Drawn {
    let t = ramp_for(art, tint);
    let mut pens = Pens {
        behind: Pen::new(size, origin),
        front: Pen::new(size, origin),
    };
    if let Some(sprite) = atlas().get(&format!("acc_{}", art.id)) {
        let sprite = if art.tintable {
            tint_art(&sprite, &t)
        } else {
            (*sprite).clone()
        };
        let ((px, py), (fx, fy)) = pin(art.slot, a);
        let x0 = px - (sprite.width() as f32 * fx).round() as i32;
        let y0 = py - (sprite.height() as f32 * fy).round() as i32;
        let pen = if art.behind || art.slot == Slot::Back {
            &mut pens.behind
        } else {
            &mut pens.front
        };
        for (x, y, p) in sprite.enumerate_pixels() {
            if p[3] > 127 {
                pen.put(x0 + x as i32, y0 + y as i32, [p[0], p[1], p[2]]);
            }
        }
    } else {
        (art.draw)(&mut pens, &Ctx { a, t, skin });
    }
    let cut = art.cuts_hair.then(|| hair_line(&pens.front, a, origin));
    Drawn {
        behind: pens.behind.img,
        front: pens.front.img,
        cut,
        grip: art.grip,
    }
}

/// Where the hair stops under a hat, per column in canvas pixels: the top of the hat over it,
/// and beside it the top of the brim, so no hair sticks up past the hat's sides.
fn hair_line(hat: &Pen, a: &Anchors, origin: (i32, i32)) -> Vec<(i32, i32)> {
    let mut tops: Vec<(i32, i32)> = Vec::new();
    for (x, y) in hat.cells() {
        match tops.iter_mut().find(|(tx, _)| *tx == x) {
            Some((_, top)) => *top = (*top).min(y),
            None => tops.push((x, y)),
        }
    }
    tops.sort_unstable();
    if let (Some(&(l, lt)), Some(&(r, rt))) = (tops.first(), tops.last()) {
        let reach = (a.hair.1 - a.hair.0) / 3 + 6;
        tops.extend((l - reach..l).map(|x| (x, lt + 1)));
        tops.extend((r + 1..=r + reach).map(|x| (x, rt + 1)));
    }
    tops.into_iter().map(|(x, y)| (x + origin.0, y + origin.1)).collect()
}

/// An accessory on its own, `size`×`size`, for menus and cards: drawn on a stand-in head and
/// body, then cropped and centred.
pub fn accessory_icon(id: &str, tint: Option<Paint>, size: u32) -> Option<RgbaImage> {
    let art = accessory(id)?;
    // The narrowest head, so hats come out small enough to show at twice their size.
    let a = crate::character::look_anchors("shavebreeze");
    let canvas = (a.width as u32 + 120, a.height as u32 + 120);
    let origin = (60, 60);
    let skin = crate::character::Colors::default().skin_ramp();
    let drawn = draw(art, tint, &a, &skin, canvas, origin);
    let mut img = RgbaImage::new(canvas.0, canvas.1);
    for layer in [&drawn.behind, &drawn.front] {
        for (dst, src) in img.pixels_mut().zip(layer.pixels()) {
            if src[3] > 0 {
                *dst = *src;
            }
        }
    }
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 0 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    if x0 > x1 {
        return None;
    }
    let crop = image::imageops::crop_imm(&img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image();
    let k = (size / crop.width().max(crop.height())).max(1);
    let big = crate::character::scale_up(&crop, k);
    let mut out = RgbaImage::new(size.max(big.width()), size.max(big.height()));
    let (ox, oy) = ((out.width() - big.width()) / 2, (out.height() - big.height()) / 2);
    image::imageops::overlay(&mut out, &big, i64::from(ox), i64::from(oy));
    Some(out)
}

// /Head

/// Head width and middle for things that sit on the head.
fn head(a: &Anchors) -> (i32, i32, i32, i32) {
    let (x0, x1) = a.skull;
    (x0, x1, a.head_x(), a.brow)
}

/// Like [`head`] for hats that cover it: wide enough for big hair too.
fn hat_span(a: &Anchors) -> (i32, i32, i32, i32) {
    let (s0, s1) = a.skull;
    let (h0, h1) = a.hair;
    let (x0, x1) = (s0.min(h0.max(s0 - 6)), s1.max(h1.min(s1 + 7)));
    (x0, x1, (x0 + x1) / 2, a.brow)
}

mod eyewear;
mod headwear;
mod neckwear;
mod props;
use eyewear::*;
use headwear::*;
use neckwear::*;
use props::*;
