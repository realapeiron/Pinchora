//! Accessories drawn in code as pixel art, placed on a look's anchors and sized to its head and
//! body, so one hat fits every look. Each is tinted from a five-shade ramp. When the atlas has
//! art for one (`acc_<id>`), that art is placed on the same anchor instead.

use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use super::{
    ACCESSORY_TINTS, Paint, atlas,
    look::{Anchors, OUTLINE},
    palette::{derive_ramp, ramp_of, rgb},
    tint_art,
};

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
    let a = super::look_anchors("shavebreeze");
    let canvas = (a.width as u32 + 120, a.height as u32 + 120);
    let origin = (60, 60);
    let skin = super::Colors::default().skin_ramp();
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
    let big = super::scale_up(&crop, k);
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

fn straw_hat(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, hx, b) = hat_span(c.a);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    p.part(false, |f| {
        f.dome(cx, b - 1, half - 1.0, 9.0, STRAW[2]);
        f.ellipse(cx, b as f32 + 0.5, half + 6.5, 2.4, STRAW[2]);
        f.bevel(STRAW[2], STRAW[3], STRAW[1]);
        // Woven straw: a light stitch every third pixel on the crown.
        for (x, y) in f.cells() {
            if y < b - 3 && (x + 2 * y).rem_euclid(4) == 0 && f.get(x, y) == Some(STRAW[2]) {
                f.put(x, y, STRAW[1]);
            }
        }
        // The ribbon round the crown and a little bow at its front.
        for x in (x0 + 1)..=(x1 - 1) {
            if f.has(x, b - 3) {
                f.put(x, b - 3, c.t[2]);
                f.put(x, b - 2, c.t[1]);
            }
        }
        let bx = x0 + 3;
        f.rows(bx - 1, b - 5, &[".ab.", "abba", ".bb."], |ch| match ch {
            'a' => Some(c.t[3]),
            'b' => Some(c.t[2]),
            _ => None,
        });
        // The brim's lower edge in shade.
        for x in (cx - half - 7.0) as i32..=(cx + half + 7.0) as i32 {
            if f.has(x, b + 2) && !f.has(x, b + 3) {
                f.put(x, b + 2, STRAW[1]);
            }
        }
    });
}

fn beanie(p: &mut Pens, c: &Ctx<'_>) {
    let (s0, s1, _, _) = hat_span(c.a);
    let (x0, x1) = (s0.max(c.a.skull.0 - 2), s1.min(c.a.skull.1 + 2));
    let (hx, b) = ((x0 + x1) / 2, c.a.brow);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    let tall = (half * 0.8).clamp(10.0, 15.0);
    p.part(false, |f| {
        f.dome(cx, b - 2, half - 0.5, tall, c.t[2]);
        f.bevel(c.t[2], c.t[3], c.t[1]);
        // Knit stripes.
        for (x, y) in f.cells() {
            if (b - 2 - y) % 4 == 2 && f.get(x, y) == Some(c.t[2]) {
                f.put(x, y, c.t[3]);
            }
        }
        // The folded cuff, ribbed.
        f.rect(x0 - 1, b - 2, x1 + 1, b + 1, c.t[1]);
        for x in (x0 - 1)..=(x1 + 1) {
            if (x - x0) % 2 == 0 {
                f.rect(x, b - 1, x, b + 1, c.t[0]);
            }
        }
        f.rect(x0 - 1, b - 2, x1 + 1, b - 2, c.t[2]);
        // The pompom, sitting on top.
        let top = b - 2 - tall as i32;
        f.ellipse(cx + 1.0, top as f32 - 1.5, 3.8, 3.4, WHITE[3]);
        f.put(hx, top - 3, WHITE[4]);
        f.put(hx + 1, top - 4, WHITE[4]);
        f.put(hx + 3, top, WHITE[1]);
        f.put(hx + 4, top - 1, WHITE[1]);
    });
}

fn sailor_cap(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, hx, b) = hat_span(c.a);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    p.part(false, |f| {
        f.dome(cx, b - 1, half - 1.0, 7.0, WHITE[2]);
        f.bevel(WHITE[2], WHITE[4], WHITE[1]);
        // The band round the bottom, with a gold anchor at the front.
        f.rect(x0 + 1, b - 1, x1 - 1, b + 1, c.t[2]);
        f.rect(x0 + 1, b + 1, x1 - 1, b + 1, c.t[1]);
        f.rect(x0 + 1, b - 1, x1 - 1, b - 1, c.t[3]);
        let ax = x0 + 5;
        f.rows(ax - 1, b - 1, &[".g.", "ggg", ".g."], |ch| {
            (ch == 'g').then_some(GOLD[2])
        });
    });
}

fn captain_hat(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, hx, b) = hat_span(c.a);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    p.part(false, |f| {
        // The white top sits wider than the band, like a plate.
        f.ellipse(cx + 1.0, b as f32 - 6.0, half + 2.0, 4.0, WHITE[2]);
        f.bevel(WHITE[2], WHITE[4], WHITE[1]);
        f.rect(x0 + 1, b - 3, x1 - 1, b, c.t[2]);
        f.rect(x0 + 1, b - 3, x1 - 1, b - 3, GOLD[2]);
        f.rect(x0 + 1, b, x1 - 1, b, c.t[1]);
        // The badge at the front.
        let ax = x0 + 6;
        f.rows(ax - 2, b - 5, &[".ggg.", "gwgwg", ".ggg."], |ch| match ch {
            'g' => Some(GOLD[2]),
            'w' => Some(GOLD[4]),
            _ => None,
        });
    });
    // The black visor over the front of the brow.
    p.part(false, |f| {
        f.ellipse(x0 as f32 + 5.0, b as f32 + 1.5, 8.0, 1.6, CHARCOAL[1]);
        f.rect(x0 - 1, b + 1, x0 + 7, b + 1, CHARCOAL[3]);
    });
}

fn top_hat(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, hx, b) = hat_span(c.a);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    let (l, r) = ((cx - half + 4.0) as i32, (cx + half - 4.0) as i32);
    p.part(false, |f| {
        f.rect(l, b - 17, r, b - 1, CHARCOAL[2]);
        f.bevel(CHARCOAL[2], CHARCOAL[3], CHARCOAL[1]);
        f.rect(l, b - 17, r, b - 17, CHARCOAL[3]);
        f.rect(l, b - 5, r, b - 3, c.t[2]);
        f.rect(l, b - 3, r, b - 3, c.t[1]);
        f.rect(l, b - 5, r, b - 5, c.t[3]);
        // A shine down the front of the tube.
        f.rect(l + 2, b - 15, l + 2, b - 8, CHARCOAL[4]);
        f.ellipse(cx, b as f32 + 0.5, half + 3.5, 1.8, CHARCOAL[1]);
        f.rect((cx - half - 2.0) as i32, b, (cx + half + 2.0) as i32, b, CHARCOAL[2]);
    });
}

fn witch_hat(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, hx, b) = hat_span(c.a);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    p.part(false, |f| {
        // A cone narrowing to a tip that flops back.
        let height = 22;
        for i in 0..=height {
            let y = b - 1 - i;
            let k = i as f32 / height as f32;
            let w = (half - 2.0) * (1.0 - k).powf(0.85) + 0.5;
            let bend = 6.0 * k * k;
            f.rect((cx - w + bend) as i32, y, (cx + w + bend) as i32, y, c.t[2]);
        }
        f.bevel(c.t[2], c.t[3], c.t[1]);
        f.ellipse(cx, b as f32 + 0.5, half + 7.0, 2.3, c.t[1]);
        f.rect((cx - half - 6.0) as i32, b - 1, (cx + half + 6.0) as i32, b - 1, c.t[2]);
        // The band and its buckle.
        for x in x0..=x1 {
            if f.get(x, b - 3) == Some(c.t[2]) || f.get(x, b - 3) == Some(c.t[3]) || f.get(x, b - 3) == Some(c.t[1]) {
                f.put(x, b - 3, c.t[0]);
                f.put(x, b - 2, c.t[0]);
            }
        }
        f.rows(hx - 4, b - 4, &["gggg", "g..g", "gggg"], |ch| {
            (ch == 'g').then_some(GOLD[2])
        });
    });
}

fn crown(p: &mut Pens, c: &Ctx<'_>) {
    let x = c.a.head_x() - 7;
    let y = c.a.crown - 4;
    p.part(false, |f| {
        f.rows(
            x,
            y,
            &[
                ".w.....w.....w.",
                ".g....ggg....g.",
                ".gg..ggggg..gg.",
                ".gggggggggggggg",
                ".hhhhhhhhhhhhhh",
                ".gg#rg##bg##rgg",
                ".ggg#ggrgg#gggg",
                ".ddddddddddddd.",
            ],
            |ch| match ch {
                'w' => Some(GOLD[4]),
                'g' => Some(GOLD[2]),
                'h' => Some(GOLD[3]),
                'd' => Some(GOLD[1]),
                'r' => Some([0xe2, 0x3d, 0x52]),
                'b' => Some([0x4a, 0xa8, 0xec]),
                '#' => Some(GOLD[1]),
                _ => None,
            },
        );
    });
}

fn bow(p: &mut Pens, c: &Ctx<'_>) {
    let x = c.a.skull.0 + 1;
    let y = c.a.crown + 2;
    p.part(false, |f| {
        f.rows(
            x,
            y,
            &[
                ".ll......ll.",
                "lmmm....mmml",
                "lmmmm..mmmml",
                "mmmmmkkmmmmm",
                "dmmmmkkmmmmd",
                ".ddm.dd.mdd.",
                "....d..d....",
            ],
            |ch| match ch {
                'l' => Some(c.t[3]),
                'm' => Some(c.t[2]),
                'd' => Some(c.t[1]),
                'k' => Some(c.t[0]),
                _ => None,
            },
        );
    });
}

fn flower_crown(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, _, b) = head(c.a);
    let y = b - 1;
    p.part(false, |f| {
        f.rect(x0, y, x1, y + 1, LEAF[1]);
        let mut x = x0;
        let mut i = 0;
        while x <= x1 {
            let petal = match i % 3 {
                0 => c.t,
                1 => [WHITE[1], WHITE[2], WHITE[3], WHITE[4], WHITE[4]],
                _ => [GOLD[0], GOLD[1], GOLD[2], GOLD[3], GOLD[4]],
            };
            f.rows(x - 1, y - 2, &[".p.", "pcp", ".p."], |ch| match ch {
                'p' => Some(petal[3]),
                'c' => Some(if i % 3 == 2 { [0xff, 0x8a, 0x30] } else { GOLD[2] }),
                _ => None,
            });
            f.put(x + 2, y, LEAF[3]);
            x += 5;
            i += 1;
        }
    });
}

// /Face

fn lens_box(a: &Anchors, i: usize, grow: i32) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = a.eyes[i];
    let near = i == 1;
    let (gx, gy) = if near { (grow + 1, grow) } else { (grow, grow) };
    (x0 - gx, y0 - gy + 1, x1 + gx, y1 + gy)
}

/// One lens: a box with its corners cut, `frame` round the edge and `fill` (glass, shine) inside.
fn lens(f: &mut Pen, (x0, y0, x1, y1): (i32, i32, i32, i32), frame: [u8; 3], fill: Option<([u8; 3], [u8; 3])>) {
    for y in y0..=y1 {
        for x in x0..=x1 {
            let edge = x == x0 || x == x1 || y == y0 || y == y1;
            let corner = (x == x0 || x == x1) && (y == y0 || y == y1);
            if corner {
                continue;
            }
            if edge {
                f.put(x, y, frame);
            } else if let Some((glass, shine)) = fill {
                f.put(x, y, if x == x0 + 1 && y == y0 + 1 { shine } else { glass });
            }
        }
    }
}

/// Rings round both eyes with a bridge and a temple to the ear; `fill` fills the lenses.
fn frames(f: &mut Pen, a: &Anchors, frame: [u8; 3], fill: Option<([u8; 3], [u8; 3])>) {
    for i in 0..2 {
        lens(f, lens_box(a, i, 1), frame, fill);
    }
    let (l, r) = (lens_box(a, 0, 1), lens_box(a, 1, 1));
    f.rect(l.2, l.1 + 1, r.0, l.1 + 1, frame);
    f.rect(r.2, r.1 + 1, a.skull.1 - 4, r.1 + 1, frame);
}

fn round_glasses(p: &mut Pens, c: &Ctx<'_>) {
    frames(&mut p.front, c.a, c.t[1], None);
    let (l, r) = (lens_box(c.a, 0, 1), lens_box(c.a, 1, 1));
    p.front.put(l.0 + 1, l.1 + 1, WHITE[4]);
    p.front.put(r.0 + 1, r.1 + 1, WHITE[4]);
}

/// Two dark lenses that span both eyes, so they read as glasses and not as a patch on the near
/// eye: both as tall as the taller eye, the far one at least six wide (on a turned head the far
/// eye is a sliver), and a short temple to the ear.
fn sunglasses(p: &mut Pens, c: &Ctx<'_>) {
    let a = c.a;
    let [far, near] = a.eyes;
    let rows = (far.3 - far.1).max(near.3 - near.1) + 2;
    let near_box = (near.0 - 2, near.1, near.2 + 1, near.1 + rows - 1);
    let far_right = far.2 + 1;
    let far_box = ((far.0 - 1).min(far_right - 5), far.1, far_right, far.1 + rows - 1);
    let f = &mut p.front;
    for lens_box in [far_box, near_box] {
        lens(f, lens_box, LINE, Some((c.t[1], c.t[3])));
    }
    f.rect(far_box.2, far_box.1 + 1, near_box.0, far_box.1 + 1, LINE);
    f.rect(
        near_box.2,
        near_box.1 + 1,
        (a.skull.1 - 4).min(near_box.2 + 4),
        near_box.1 + 1,
        LINE,
    );
    f.put(near_box.0 + 2, near_box.1 + 1, c.t[4]);
}

fn heart_glasses(p: &mut Pens, c: &Ctx<'_>) {
    let a = c.a;
    let f = &mut p.front;
    for i in 0..2 {
        let (cx, cy) = a.eye_centre(i);
        let x0 = cx - if i == 1 { 3 } else { 2 };
        let rows: &[&str] = if i == 1 {
            &["#.#.#", "#hm#m#", "#mmmmm#", ".#mmm#.", "..#m#..", "...#..."]
        } else {
            &[".#.#.", "#hm#m#", "#mmmm#", ".#mm#.", "..##.."]
        };
        f.rows(x0 - 1, cy - 3, rows, |ch| match ch {
            '#' => Some(LINE),
            'm' => Some(c.t[2]),
            'h' => Some(c.t[4]),
            _ => None,
        });
    }
    let (l, r) = (a.eye_centre(0), a.eye_centre(1));
    f.rect(l.0 + 3, l.1 - 2, r.0 - 4, l.1 - 2, LINE);
    f.rect(r.0 + 4, r.1 - 2, a.skull.1 - 4, r.1 - 2, LINE);
}

fn eyepatch(p: &mut Pens, c: &Ctx<'_>) {
    let a = c.a;
    let (x0, y0, x1, y1) = lens_box(a, 1, 1);
    // The strap runs over the brow to the back of the head and down past the far eye.
    p.front.line(x1, y0 + 1, a.skull.1 - 3, a.brow - 1, LINE);
    p.front.line(x0, y0 + 1, a.eyes[0].0 - 2, a.brow - 2, LINE);
    p.part(false, |f| {
        f.ellipse(
            (x0 + x1) as f32 / 2.0 + 0.5,
            (y0 + y1) as f32 / 2.0 + 0.5,
            (x1 - x0) as f32 / 2.0 + 0.6,
            (y1 - y0) as f32 / 2.0 + 0.6,
            CHARCOAL[1],
        );
        f.bevel(CHARCOAL[1], CHARCOAL[3], CHARCOAL[0]);
    });
}

fn freckles(p: &mut Pens, c: &Ctx<'_>) {
    let dot = c.skin[0];
    for i in 0..2 {
        let (x0, _, x1, y1) = c.a.eyes[i];
        let y = y1 + 2;
        let spots: &[(i32, i32)] = if i == 1 {
            &[(0, 0), (2, 0), (4, 0), (1, 1), (3, 1)]
        } else {
            &[(0, 0), (2, 0), (1, 1)]
        };
        for &(dx, dy) in spots {
            let x = if i == 1 { x0 - 1 + dx } else { x1 - 2 + dx };
            p.front.put(x, y + dy, dot);
        }
    }
}

fn blush(p: &mut Pens, c: &Ctx<'_>) {
    let pink = mix(c.skin[2], [0xff, 0x6f, 0x8a], 0.65);
    let light = mix(c.skin[3], [0xff, 0x9f, 0xb0], 0.55);
    let (_, _, nx1, ny1) = c.a.eyes[1];
    let (fx0, _, _, fy1) = c.a.eyes[0];
    p.front.rect(nx1 - 2, ny1 + 2, nx1 + 1, ny1 + 3, pink);
    p.front.rect(nx1 - 1, ny1 + 2, nx1, ny1 + 2, light);
    p.front.rect(fx0 - 2, fy1 + 2, fx0, fy1 + 3, pink);
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [0, 1, 2].map(|i| (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t).round() as u8)
}

// /Neck

/// The collar's left and right edge and its row.
fn collar(a: &Anchors) -> (i32, i32, i32) {
    let (nx, ny) = a.neck;
    let half = ((a.torso.2 - a.torso.0) / 4).clamp(5, 9);
    (nx - half, nx + half, ny)
}

fn scarf(p: &mut Pens, c: &Ctx<'_>) {
    let (l, r, y) = collar(c.a);
    p.part(false, |f| {
        f.rect(l - 1, y - 2, r + 1, y + 2, c.t[2]);
        f.rect(l - 1, y + 2, r + 1, y + 2, c.t[1]);
        f.rect(l, y - 2, r, y - 2, c.t[3]);
        for x in (l - 1..=r + 1).filter(|x| (x - l) % 4 == 1) {
            f.rect(x, y - 1, x + 1, y + 1, c.t[4]);
        }
        // The loose end hangs down the front.
        let tx = l + 1;
        f.rect(tx, y + 2, tx + 3, y + 10, c.t[2]);
        f.rect(tx, y + 5, tx + 3, y + 6, c.t[4]);
        f.rect(tx + 3, y + 2, tx + 3, y + 10, c.t[1]);
        for x in tx..=tx + 3 {
            if (x - tx) % 2 == 0 {
                f.put(x, y + 11, c.t[2]);
            }
        }
    });
}

fn necklace(p: &mut Pens, a: &Anchors, bead: impl Fn(usize) -> Option<[u8; 3]>, string: [u8; 3]) -> (i32, i32) {
    let (l, r, y) = collar(a);
    let (l, r) = (l + 1, r - 1);
    let mid = (l + r) as f32 / 2.0;
    let half = ((r - l) as f32 / 2.0).max(1.0);
    let mut low = (l, y);
    for (i, x) in (l..=r).enumerate() {
        let k = (x as f32 - mid) / half;
        let dy = (4.5 * (1.0 - k * k)).round() as i32;
        let py = y + dy;
        p.front.put(x, py, bead(i).unwrap_or(string));
        if dy >= low.1 - y {
            low = (x, py);
        }
    }
    low
}

fn shell_necklace(p: &mut Pens, c: &Ctx<'_>) {
    let beads = [[0xff, 0xf4, 0xe0], [0xff, 0xb3, 0x9a], [0x7f, 0xd8, 0xd0]];
    let (lx, ly) = necklace(
        p,
        c.a,
        |i| (i % 2 == 0).then_some(beads[(i / 2) % 3]),
        [0xc8, 0xa8, 0x80],
    );
    p.part(false, |f| {
        f.rows(lx - 2, ly + 1, &[".lml.", "lmlml", "mlmlm", ".ddd."], |ch| match ch {
            'l' => Some(c.t[4]),
            'm' => Some(c.t[2]),
            'd' => Some(c.t[1]),
            _ => None,
        });
    });
}

fn pearl_necklace(p: &mut Pens, c: &Ctx<'_>) {
    let (lx, ly) = necklace(p, c.a, |i| Some(if i % 2 == 0 { WHITE[4] } else { WHITE[1] }), WHITE[1]);
    p.part(false, |f| {
        f.ellipse(lx as f32 + 0.5, ly as f32 + 2.0, 1.6, 1.6, WHITE[3]);
        f.put(lx, ly + 1, WHITE[4]);
    });
}

fn bow_tie(p: &mut Pens, c: &Ctx<'_>) {
    let (nx, ny) = c.a.neck;
    p.part(false, |f| {
        f.rows(
            nx - 4,
            ny,
            &["ll...ll", "lmm.mml", "mmmkmmm", "dmm.mmd", "dd...dd"],
            |ch| match ch {
                'l' => Some(c.t[3]),
                'm' => Some(c.t[2]),
                'd' => Some(c.t[1]),
                'k' => Some(c.t[0]),
                _ => None,
            },
        );
    });
}

fn bandana(p: &mut Pens, c: &Ctx<'_>) {
    let (l, r, y) = collar(c.a);
    p.part(false, |f| {
        let mid = (l + r) / 2;
        for row in 0..=7 {
            let half = (r - l) / 2 + 1 - row;
            if half < 0 {
                break;
            }
            f.rect(mid - half, y - 1 + row, mid + half, y - 1 + row, c.t[2]);
        }
        f.rect(l - 1, y - 1, r + 1, y - 1, c.t[1]);
        for (x, yy) in f.cells() {
            if (x + yy * 2).rem_euclid(5) == 0 && f.get(x, yy) == Some(c.t[2]) {
                f.put(x, yy, WHITE[4]);
            }
        }
    });
}

// /Back

fn backpack(p: &mut Pens, c: &Ctx<'_>) {
    let (_, ty, _, _) = c.a.torso;
    let (nx, ny) = c.a.neck;
    let (x0, x1) = (nx + 5, nx + 17);
    let (y0, y1) = (ty + 1, ty + 18);
    p.part(true, |f| {
        f.rect(x0, y0 + 1, x1, y1, c.t[2]);
        f.rect(x0 + 1, y0, x1 - 1, y0, c.t[2]);
        f.bevel(c.t[2], c.t[3], c.t[1]);
        // The flap, its buckle and the pocket.
        f.rect(x0 + 1, y0 + 1, x1 - 1, y0 + 5, c.t[1]);
        f.rect(x0 + 2, y0 + 1, x1 - 2, y0 + 1, c.t[2]);
        f.rect(x0 + 1, y0 + 6, x1 - 1, y0 + 6, c.t[0]);
        f.rect(x0 + 4, y1 - 7, x1 - 1, y1 - 1, c.t[3]);
        f.rect(x0 + 4, y1 - 7, x1 - 1, y1 - 7, c.t[4]);
        f.rect(x0 + 4, y1 - 1, x1 - 1, y1 - 1, c.t[1]);
        f.rect(x0 + 7, y0 + 5, x0 + 8, y0 + 7, GOLD[2]);
    });
    // A strap over the front shoulder.
    p.part(false, |f| {
        f.rect(nx + 3, ny + 1, nx + 4, ty + 13, c.t[1]);
        f.put(nx + 3, ny + 1, c.t[2]);
        f.rect(nx + 3, ty + 9, nx + 4, ty + 9, GOLD[2]);
    });
}

/// A wing shape: an ellipse leaning `tilt` pixels per row, behind the back.
fn wing(f: &mut Pen, cx: f32, cy: f32, rx: f32, ry: f32, tilt: f32, c: [u8; 3]) {
    for y in (cy - ry).floor() as i32..=(cy + ry).ceil() as i32 {
        let shift = (y as f32 - cy) * tilt;
        for x in (cx - rx - ry * tilt.abs()).floor() as i32..=(cx + rx + ry * tilt.abs()).ceil() as i32 {
            let (dx, dy) = ((x as f32 + 0.5 - cx - shift) / rx, (y as f32 + 0.5 - cy) / ry);
            if dx * dx + dy * dy <= 1.0 {
                f.put(x, y, c);
            }
        }
    }
}

fn fairy_wings(p: &mut Pens, c: &Ctx<'_>) {
    let (tx0, ty, tx1, _) = c.a.torso;
    let mid = c.a.neck.0 as f32 + 3.0;
    let y = ty as f32 + 7.0;
    let span = ((tx1 - tx0) as f32 / 2.0).min(13.0);
    p.part(true, |f| {
        wing(f, mid + span + 5.0, y - 6.0, 7.5, 6.0, -0.5, c.t[3]);
        wing(f, mid - span - 5.0, y - 6.0, 7.5, 6.0, 0.5, c.t[3]);
        wing(f, mid + span + 2.0, y + 5.0, 5.0, 4.0, 0.4, c.t[2]);
        wing(f, mid - span - 2.0, y + 5.0, 5.0, 4.0, -0.4, c.t[2]);
        f.bevel(c.t[3], c.t[4], c.t[2]);
        // Veins from the shoulders out.
        f.line(
            (mid + span) as i32,
            y as i32,
            (mid + span + 10.0) as i32,
            (y - 9.0) as i32,
            c.t[1],
        );
        f.line(
            (mid - span) as i32,
            y as i32,
            (mid - span - 10.0) as i32,
            (y - 9.0) as i32,
            c.t[1],
        );
        f.put((mid + span + 8.0) as i32, (y - 9.0) as i32, WHITE[4]);
        f.put((mid - span - 8.0) as i32, (y - 9.0) as i32, WHITE[4]);
    });
}

fn bat_wings(p: &mut Pens, c: &Ctx<'_>) {
    let (tx0, ty, tx1, _) = c.a.torso;
    let mid = c.a.neck.0 as f32 + 3.0;
    let span = ((tx1 - tx0) as f32 / 2.0 - 2.0).min(11.0);
    let y = ty as f32 + 5.0;
    // One wing, outward along +x: a bony top edge to a hooked tip, scalloped below.
    let shape = [
        (0.0, 2.0),
        (5.0, -5.0),
        (11.0, -9.0),
        (17.0, -8.0),
        (18.5, -10.0),
        (18.0, 2.0),
        (14.5, 0.0),
        (11.5, 6.0),
        (8.0, 3.0),
        (4.5, 9.0),
        (0.0, 7.0),
    ];
    p.part(true, |f| {
        for side in [-1.0f32, 1.0] {
            let base = mid + side * span;
            let pts: Vec<(f32, f32)> = shape.iter().map(|&(dx, dy)| (base + side * dx, y + dy)).collect();
            f.poly(&pts, c.t[2]);
            for &(dx, dy) in &shape[5..10] {
                f.line(
                    base as i32,
                    y as i32 + 1,
                    (base + side * dx * 0.9) as i32,
                    (y + dy * 0.6 - 2.0) as i32,
                    c.t[1],
                );
            }
            f.line(
                base as i32,
                y as i32 + 1,
                (base + side * 11.0) as i32,
                (y - 8.0) as i32,
                c.t[3],
            );
            f.line(
                (base + side * 11.0) as i32,
                (y - 8.0) as i32,
                (base + side * 17.5) as i32,
                (y - 9.0) as i32,
                c.t[3],
            );
        }
    });
}

fn cape(p: &mut Pens, c: &Ctx<'_>) {
    let a = c.a;
    let (tx0, ty, tx1, _) = a.torso;
    let bottom = a.height - 18;
    let (l0, r0) = (tx0 as f32 + 3.0, tx1 as f32 - 2.0);
    let (l1, r1) = (tx0 as f32 - 4.0, tx1 as f32 + 8.0);
    p.part(true, |f| {
        let pts = [
            (l0, ty as f32),
            (r0, ty as f32),
            (r1, bottom as f32 - 2.0),
            ((l1 + r1) / 2.0 + 3.0, bottom as f32 + 1.0),
            (l1, bottom as f32 - 1.0),
        ];
        f.poly(&pts, c.t[2]);
        f.bevel(c.t[2], c.t[3], c.t[1]);
        // Folds running down from the shoulders, and the lining along the back edge.
        for k in [0.25f32, 0.55, 0.8] {
            let (xa, xb) = (l0 + (r0 - l0) * k, l1 + (r1 - l1) * k);
            f.line(xa as i32, ty + 8, xb as i32, bottom - 1, c.t[1]);
        }
        for y in ty + 4..bottom - 1 {
            let k = (y - ty) as f32 / (bottom - ty) as f32;
            let r = (r0 + (r1 - r0) * k) as i32;
            if f.has(r, y) {
                f.put(r, y, c.t[4]);
                f.put(r - 1, y, c.t[3]);
            }
        }
    });
    // The clasp at the collar.
    let (nx, ny) = a.neck;
    p.part(false, |f| {
        f.rows(nx - 1, ny, &["gg", "gw"], |ch| match ch {
            'g' => Some(GOLD[2]),
            'w' => Some(GOLD[4]),
            _ => None,
        });
    });
}

fn surfboard(p: &mut Pens, c: &Ctx<'_>) {
    let (_, ty, tx1, _) = c.a.torso;
    let (cx, cy) = (tx1 as f32 + 3.0, ty as f32 + 4.0);
    let lean = -0.3;
    p.part(true, |f| {
        // A long board leaning back: an ellipse whose rows are shifted as it leans.
        let (rx, ry) = (6.0, 23.0);
        for y in (cy - ry) as i32..=(cy + ry) as i32 {
            let shift = (y as f32 - cy) * lean;
            for x in (cx - rx - 8.0) as i32..=(cx + rx + 8.0) as i32 {
                let (dx, dy) = ((x as f32 + 0.5 - cx - shift) / rx, (y as f32 + 0.5 - cy) / ry);
                if dx * dx + dy * dy <= 1.0 {
                    f.put(x, y, c.t[2]);
                }
            }
        }
        f.bevel(c.t[2], c.t[3], c.t[1]);
        // A white stripe down the middle and a stripe of the dye's dark across it.
        for y in (cy - ry + 3.0) as i32..=(cy + ry - 3.0) as i32 {
            let x = (cx + (y as f32 - cy) * lean).round() as i32;
            f.put(x, y, WHITE[3]);
            f.put(x - 1, y, WHITE[2]);
        }
        let band = (cy + 8.0) as i32;
        for x in (cx - 12.0) as i32..=(cx + 12.0) as i32 {
            if f.has(x, band) {
                f.put(x, band, c.t[0]);
                f.put(x, band + 1, c.t[1]);
            }
        }
    });
}

// /Hand

fn lantern(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    let x = hx + 2;
    p.part(false, |f| {
        f.rows(
            x - 4,
            hy - 1,
            &[
                "...mm....",
                "..m..m...",
                "...mm....",
                ".mmmmmmm.",
                "mmmmmmmmm",
                ".m.wwy.m.",
                ".myyyyym.",
                ".myyyyom.",
                ".myyyoom.",
                ".myyoooom",
                ".mmmmmmm.",
                "mmmmmmmmm",
                ".mm...mm.",
            ],
            |ch| match ch {
                'm' => Some(CHARCOAL[2]),
                'w' => Some([0xff, 0xfb, 0xe0]),
                'y' => Some([0xff, 0xe0, 0x66]),
                'o' => Some([0xff, 0xb0, 0x3a]),
                _ => None,
            },
        );
        f.rect(x - 3, hy + 3, x + 3, hy + 3, CHARCOAL[3]);
    });
}

fn fishing_rod(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    let tip = (hx + 13, hy - 38);
    p.front.line(tip.0, tip.1, tip.0 + 1, tip.1 + 22, WHITE[1]);
    p.part(false, |f| {
        f.line(hx - 1, hy + 3, tip.0, tip.1, WOOD[2]);
        f.line(hx, hy + 3, tip.0 + 1, tip.1, WOOD[3]);
        f.rect(hx - 1, hy - 1, hx + 1, hy + 1, CHARCOAL[3]);
        f.put(hx + 2, hy, CHARCOAL[2]);
    });
    p.part(false, |f| {
        f.rows(tip.0, tip.1 + 22, &[".r.", "rrr", "www", ".w."], |ch| match ch {
            'r' => Some(c.t[2]),
            'w' => Some(WHITE[3]),
            _ => None,
        });
    });
}

fn ice_cream(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    let wafer = [[0xb8, 0x78, 0x3a], [0xe6, 0xae, 0x62], [0xf6, 0xcc, 0x86]];
    p.part(false, |f| {
        f.rows(
            hx - 4,
            hy - 9,
            &[
                "lwvwvwv", "lvwvwvw", ".wvwvw.", ".vwvwv.", "..wvw..", "..vwv..", "...w...", "...v...", "...w...",
            ],
            |ch| match ch {
                'l' => Some(wafer[2]),
                'w' => Some(wafer[1]),
                'v' => Some(wafer[0]),
                _ => None,
            },
        );
    });
    p.part(false, |f| {
        f.ellipse(hx as f32, hy as f32 - 13.0, 4.8, 4.4, c.t[2]);
        f.bevel(c.t[2], c.t[3], c.t[1]);
        // A drip over the cone's rim and a cherry on top.
        f.rect(hx - 3, hy - 10, hx - 3, hy - 8, c.t[2]);
        f.rect(hx + 1, hy - 10, hx + 1, hy - 9, c.t[2]);
        f.put(hx - 2, hy - 15, c.t[4]);
        f.put(hx - 1, hy - 16, c.t[4]);
    });
    p.part(false, |f| {
        f.rows(hx, hy - 20, &[".s", "rr", "rR"], |ch| match ch {
            's' => Some(LEAF[2]),
            'r' => Some([0xe2, 0x3d, 0x52]),
            'R' => Some([0xff, 0x9f, 0xa8]),
            _ => None,
        });
    });
}

fn balloon(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    let knot = (hx.max(c.a.skull.1 - 2) + 9, c.a.neck.1 - 8);
    p.part(false, |f| {
        f.ellipse(knot.0 as f32 + 0.5, knot.1 as f32 - 7.5, 7.0, 8.0, c.t[2]);
        f.bevel(c.t[2], c.t[3], c.t[1]);
        f.rect(knot.0 - 4, knot.1 - 12, knot.0 - 4, knot.1 - 9, c.t[4]);
        f.rect(knot.0 - 3, knot.1 - 13, knot.0 - 2, knot.1 - 13, c.t[4]);
        f.rows(knot.0 - 1, knot.1, &[".k.", "kkk"], |ch| (ch == 'k').then_some(c.t[1]));
    });
    // The string sways from the knot down to the hand.
    let steps = (hy - knot.1 - 2).max(1);
    for i in 0..=steps {
        let y = knot.1 + 2 + i;
        let k = i as f32 / steps as f32;
        let x = knot.0 as f32 + (hx - knot.0) as f32 * k + (k * 7.0).sin() * 1.2;
        p.front.put(x.round() as i32, y, WHITE[1]);
    }
}

fn flower(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    p.part(false, |f| {
        f.rect(hx, hy - 12, hx, hy + 2, LEAF[1]);
        f.rows(hx + 1, hy - 8, &["ll.", "lLl", ".l."], |ch| match ch {
            'l' => Some(LEAF[2]),
            'L' => Some(LEAF[3]),
            _ => None,
        });
        f.rows(hx - 3, hy - 5, &[".ll", "lL."], |ch| match ch {
            'l' => Some(LEAF[2]),
            'L' => Some(LEAF[3]),
            _ => None,
        });
    });
    p.part(false, |f| {
        f.rows(
            hx - 4,
            hy - 20,
            &[
                "..ppp..", ".pPpPp.", "ppPcPpp", "pPcCcPp", "ppPcPpp", ".pPpPp.", "..ppp..",
            ],
            |ch| match ch {
                'p' => Some(c.t[2]),
                'P' => Some(c.t[3]),
                'c' => Some(GOLD[2]),
                'C' => Some(GOLD[4]),
                _ => None,
            },
        );
    });
}

fn star_wand(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    p.part(false, |f| {
        f.line(hx, hy + 2, hx + 3, hy - 13, WHITE[3]);
        for k in [hy - 2, hy - 6, hy - 10] {
            let x = hx + (hy + 2 - k) * 3 / 15;
            f.put(x, k, [0xff, 0x8f, 0xb1]);
        }
    });
    p.part(false, |f| {
        f.rows(
            hx - 1,
            hy - 23,
            &[
                "....s....",
                "...sss...",
                "...sSs...",
                "sssssssss",
                ".ssSSsss.",
                "..sssss..",
                "..ss.ss..",
                ".ss...ss.",
            ],
            |ch| match ch {
                's' => Some(c.t[2]),
                'S' => Some(c.t[4]),
                _ => None,
            },
        );
        f.bevel(c.t[2], c.t[3], c.t[1]);
    });
    p.front.put(hx + 9, hy - 22, WHITE[4]);
    p.front.put(hx - 3, hy - 16, WHITE[4]);
}
