//! Animated hug, pat, wave and high five: two players' pixel characters, drawn from their real
//! look and colours, on a soft background. Everything is composed at the sprites' native
//! resolution and blown up with nearest-neighbour, so every art pixel stays crisp.
//!
//! A look is one flat sprite, so the motion is procedural: figures step, hop, lean (a shear that
//! shifts the upper rows) and squash, and what the sprite cannot do itself is drawn over it in
//! the player's own colours: sleeves, hands, hearts, sparkles and the clap flash.

use std::f32::consts::{PI, TAU};

use image::{Delay, Frame, Rgba, RgbaImage, codecs::gif::GifEncoder, codecs::gif::Repeat, imageops};

use crate::character::{CharacterSpec, compose};

/// Native canvas, in art pixels; the picture is [`SCALE`]× that.
const W: i32 = 240;
const H: i32 = 160;
const SCALE: u32 = 2;
/// Where the figures' feet stand.
const FEET: i32 = 140;
const OUTLINE: Rgba<u8> = Rgba([0x1b, 0x1d, 0x2b, 255]);
const FRAMES: usize = 14;
/// Frame time in hundredths of a second, and how long the first frame (everyone at home) rests.
const TICK: u16 = 9;
const HOLD: u16 = 12;

/// What one player does to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Interaction {
    Hug,
    Pat,
    Wave,
    HighFive,
}

impl Interaction {
    pub const ALL: [Self; 4] = [Self::Hug, Self::Pat, Self::Wave, Self::HighFive];

    /// The name stored with its counters.
    pub fn key(self) -> &'static str {
        match self {
            Self::Hug => "hug",
            Self::Pat => "pat",
            Self::Wave => "wave",
            Self::HighFive => "highfive",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|i| i.key() == key)
    }

    /// "hugs": "A hugs B".
    pub fn verb(self) -> &'static str {
        match self {
            Self::Hug => "hugs",
            Self::Pat => "pats",
            Self::Wave => "waves at",
            Self::HighFive => "high-fives",
        }
    }

    /// "hug": "That's hug #3".
    pub fn noun(self) -> &'static str {
        match self {
            Self::Hug => "hug",
            Self::Pat => "pat",
            Self::Wave => "wave",
            Self::HighFive => "high five",
        }
    }

    pub fn emoji(self) -> &'static str {
        match self {
            Self::Hug => "🤗",
            Self::Pat => "🫳",
            Self::Wave => "👋",
            Self::HighFive => "🙌",
        }
    }

    /// Sky, floor and shadow colours of the scene.
    fn palette(self) -> ([u8; 3], [u8; 3], [u8; 3], [u8; 3]) {
        match self {
            Self::Hug => (
                [0xff, 0xe2, 0xec],
                [0xff, 0xbd, 0xd2],
                [0xf4, 0x9b, 0xb9],
                [0xd9, 0x7b, 0x9b],
            ),
            Self::Pat => (
                [0xdc, 0xf7, 0xea],
                [0xac, 0xe8, 0xd0],
                [0x80, 0xcf, 0xae],
                [0x5f, 0xad, 0x8d],
            ),
            Self::Wave => (
                [0xdc, 0xf0, 0xff],
                [0xae, 0xd9, 0xff],
                [0x86, 0xbf, 0xf0],
                [0x66, 0x9d, 0xcf],
            ),
            Self::HighFive => (
                [0xff, 0xf3, 0xc4],
                [0xff, 0xd6, 0x85],
                [0xf2, 0xb8, 0x55],
                [0xcf, 0x93, 0x34],
            ),
        }
    }
}

/// One of the two players: their whole character (look, colours, accessories). The hands and
/// sleeves drawn over the sprite take its skin and top colours.
#[derive(Clone, Copy)]
pub struct Person<'a> {
    pub spec: &'a CharacterSpec,
}

/// A looping GIF of `from` doing `kind` to `to`, 480×320.
pub fn interaction_gif(kind: Interaction, from: Person<'_>, to: Person<'_>) -> anyhow::Result<Vec<u8>> {
    let (a, b) = (Figure::new(from, 1), Figure::new(to, -1));
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut out, 30);
        encoder.set_repeat(Repeat::Infinite)?;
        for i in 0..FRAMES {
            let frame = scale_up(&scene(kind, &a, &b, i as f32 / FRAMES as f32));
            // The scene rests for a moment before it starts again.
            let ticks = if i == 0 { HOLD } else { TICK };
            encoder.encode_frame(Frame::from_parts(
                frame,
                0,
                0,
                Delay::from_numer_denom_ms(u32::from(ticks) * 10, 1),
            ))?;
        }
    }
    Ok(out)
}

/// One picture of the scene `at` (0 up to 1) through its loop, 480×320: the same art as the GIF.
pub fn interaction_still(kind: Interaction, from: Person<'_>, to: Person<'_>, at: f32) -> RgbaImage {
    scale_up(&scene(kind, &Figure::new(from, 1), &Figure::new(to, -1), at))
}

/// The frame with every pixel made a `SCALE`×`SCALE` block.
fn scale_up(frame: &RgbaImage) -> RgbaImage {
    let (w, h, k) = (frame.width() as usize, frame.height() as usize, SCALE as usize);
    let row = w * k * 4;
    let mut out = vec![0u8; row * h * k];
    for (y, source) in frame.as_raw().chunks_exact(w * 4).enumerate() {
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
    RgbaImage::from_raw(w as u32 * SCALE, h as u32 * SCALE, out)
        .expect("the buffer has exactly the size of the picture")
}

// /Math

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

pub(crate) fn smooth(x: f32) -> f32 {
    let x = clamp01(x);
    x * x * (3.0 - 2.0 * x)
}

/// 0 before `a`, 1 after `b`, eased between.
pub(crate) fn between(x: f32, a: f32, b: f32) -> f32 {
    smooth((x - a) / (b - a))
}

pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub(crate) fn hex(c: [u8; 3]) -> Rgba<u8> {
    Rgba([c[0], c[1], c[2], 255])
}

pub(crate) fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [0, 1, 2].map(|i| lerp(f32::from(a[i]), f32::from(b[i]), t).round() as u8)
}

// /Canvas

/// Draws `color` at (x, y) if it is on the canvas.
pub(crate) fn put(canvas: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x >= 0 && y >= 0 && x < canvas.width() as i32 && y < canvas.height() as i32 {
        canvas.put_pixel(x as u32, y as u32, color);
    }
}

/// Draws a sprite with its top-left at (x0, y0); transparent pixels leave the canvas alone.
pub(crate) fn blit(canvas: &mut RgbaImage, sprite: &RgbaImage, x0: i32, y0: i32) {
    for (x, y, px) in sprite.enumerate_pixels() {
        if px[3] > 127 {
            put(canvas, x0 + x as i32, y0 + y as i32, Rgba([px[0], px[1], px[2], 255]));
        }
    }
}

/// A sprite drawn from text rows: each character is a key into `palette`, anything else is clear.
pub(crate) fn picture(rows: &[&str], palette: &[(char, Rgba<u8>)]) -> RgbaImage {
    let width = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as u32;
    let mut img = RgbaImage::new(width, rows.len() as u32);
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if let Some((_, color)) = palette.iter().find(|(k, _)| *k == ch) {
                img.put_pixel(x as u32, y as u32, *color);
            }
        }
    }
    img
}

/// Filled circle of pixels whose centres are within `r` of (cx, cy).
pub(crate) fn disc(canvas: &mut RgbaImage, cx: f32, cy: f32, r: f32, color: Rgba<u8>) {
    let (x0, x1) = ((cx - r).floor() as i32, (cx + r).ceil() as i32);
    let (y0, y1) = ((cy - r).floor() as i32, (cy + r).ceil() as i32);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if dx * dx + dy * dy <= r * r {
                put(canvas, x, y, color);
            }
        }
    }
}

/// The scene's backdrop: sky in even steps, a floor, and a light edge on it.
fn backdrop(kind: Interaction) -> RgbaImage {
    let (top, bottom, floor, _) = kind.palette();
    let ground = FEET - 6;
    let mut img = RgbaImage::new(W as u32, H as u32);
    for y in 0..H {
        let color = if y < ground {
            // Eight soft bands, so the GIF's palette stays small.
            let step = (y * 8 / ground).min(7) as f32 / 7.0;
            hex(mix(top, bottom, step))
        } else if y == ground {
            hex(mix(floor, [255, 255, 255], 0.35))
        } else if y < ground + 3 {
            hex(mix(floor, [255, 255, 255], 0.15))
        } else {
            hex(floor)
        };
        for x in 0..W {
            img.put_pixel(x as u32, y as u32, color);
        }
    }
    img
}

/// A four-pointed twinkle: `r` is the length of its arms.
fn sparkle(canvas: &mut RgbaImage, cx: i32, cy: i32, r: i32, core: Rgba<u8>, tip: Rgba<u8>) {
    if r <= 0 {
        return;
    }
    put(canvas, cx, cy, core);
    for d in 1..=r {
        let color = if d == r && r > 1 { tip } else { core };
        for (dx, dy) in [(d, 0), (-d, 0), (0, d), (0, -d)] {
            put(canvas, cx + dx, cy + dy, color);
        }
    }
    if r >= 2 {
        for (dx, dy) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
            put(canvas, cx + dx, cy + dy, core);
        }
    }
}

/// Draws a mask of coloured pixels with a one-pixel outline around it.
fn stamp(canvas: &mut RgbaImage, mask: &[(i32, i32, Rgba<u8>)], outline: Rgba<u8>) {
    let mut taken = std::collections::HashSet::with_capacity(mask.len());
    for &(x, y, _) in mask {
        taken.insert((x, y));
    }
    for &(x, y, _) in mask {
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            if !taken.contains(&(x + dx, y + dy)) {
                put(canvas, x + dx, y + dy, outline);
            }
        }
    }
    for &(x, y, color) in mask {
        put(canvas, x, y, color);
    }
}

// /Hearts

fn heart(size: u8) -> RgbaImage {
    let line = Rgba([0xc2, 0x2e, 0x5e, 255]);
    let body = Rgba([0xff, 0x5c, 0x8a, 255]);
    let shine = Rgba([0xff, 0xc8, 0xd8, 255]);
    let palette = [('#', line), ('r', body), ('h', shine)];
    match size {
        0 => picture(
            &[".##.##.", "#hr#rr#", "#rrrrr#", ".#rrr#.", "..#r#..", "...#..."],
            &palette,
        ),
        1 => picture(
            &[
                "..###.###..",
                ".#hhr#rrr#.",
                "#hrrrrrrrr#",
                "#rrrrrrrrr#",
                ".#rrrrrrr#.",
                "..#rrrrr#..",
                "...#rrr#...",
                "....#r#....",
                ".....#.....",
            ],
            &palette,
        ),
        _ => picture(
            &[
                "..###...###..",
                ".#hhr#.#rrr#.",
                "#hhrrrrrrrrr#",
                "#hrrrrrrrrrr#",
                "#rrrrrrrrrrr#",
                ".#rrrrrrrrr#.",
                "..#rrrrrrr#..",
                "...#rrrrr#...",
                "....#rrr#....",
                ".....#r#.....",
                "......#......",
            ],
            &palette,
        ),
    }
}

/// A heart that rises from (x, y), centred on x, at its age `a` (0 up to 1 over its flight).
fn float_heart(canvas: &mut RgbaImage, x: f32, y: f32, a: f32, sway: f32, phase: f32) {
    if !(0.0..1.0).contains(&a) {
        return;
    }
    let size = if a < 0.12 {
        0
    } else if a < 0.6 {
        2
    } else {
        1
    };
    let sprite = heart(size);
    let x = x + (a * 9.0 + phase).sin() * sway;
    // Below a tall head there is less sky: the heart rises less, so the biggest never leaves it.
    let rise = (y - 13.0).clamp(0.0, 40.0);
    blit(
        canvas,
        &sprite,
        (x - sprite.width() as f32 / 2.0).round() as i32,
        (y - a * rise - sprite.height() as f32).round() as i32,
    );
}

// /Figures

/// A player's sprite with what the animation needs to know about it.
struct Figure {
    /// Trimmed to its pixels, facing `dir`.
    sprite: RgbaImage,
    /// +1 when it faces right, -1 when it faces left.
    dir: i32,
    /// Box around the pixels that are the top (shirt and sleeves), `(x0, y0, x1, y1)` inclusive.
    top: (i32, i32, i32, i32),
    skin: [[u8; 3]; 4],
    sleeve: [[u8; 3]; 5],
}

/// How a figure stands on one frame.
#[derive(Clone, Copy)]
struct Pose {
    /// Where the feet are centred.
    x: f32,
    /// Lifted this far off the ground (hops).
    lift: f32,
    /// Squash and stretch about the feet.
    sx: f32,
    sy: f32,
    /// How far the head leans forward of the feet, in pixels.
    lean: f32,
}

/// A figure in a pose: its scaled sprite and where its parts are on the canvas.
struct Placed {
    scaled: RgbaImage,
    left: i32,
    top: i32,
    /// Row shift at the top of the figure, forward.
    lean: f32,
    dir: i32,
    /// Where the near shoulder (toward the other figure) is.
    near: (f32, f32),
    /// Where the far shoulder is.
    far: (f32, f32),
    head_top: f32,
    centre: f32,
}

impl Figure {
    /// The looks are drawn facing left, so a figure facing right is the mirrored character.
    fn new(person: Person<'_>, dir: i32) -> Self {
        let mut spec = person.spec.clone();
        spec.pose.mirrored = dir > 0;
        let composed = compose(&spec);
        let (look, left, top) = trim(&composed.image);
        let a = composed.anchors;
        // The shirt's shoulders, where the drawn arms start, in the trimmed picture.
        let (mut x0, mut x1) = (a.torso.0, a.torso.2);
        if spec.pose.mirrored {
            (x0, x1) = (a.width - 1 - x1, a.width - 1 - x0);
        }
        let (ox, oy) = (composed.origin.0 - left, composed.origin.1 - top);
        let y0 = a.torso.1 + oy;
        let shirt = (x0 + ox, y0, x1 + ox, (a.torso.1 + 15).min(a.torso.3) + oy);
        let colors = person.spec.colors;
        Self {
            sprite: look,
            dir,
            top: shirt,
            skin: colors.skin_ramp(),
            sleeve: colors.top_ramp(),
        }
    }

    /// The figure in `pose`: scaled about its feet, ready to be sheared so the head leans.
    fn place(&self, pose: Pose) -> Placed {
        let (w0, h0) = (self.sprite.width() as f32, self.sprite.height() as f32);
        let (w, h) = ((w0 * pose.sx).round().max(1.0), (h0 * pose.sy).round().max(1.0));
        let scaled = if (w, h) == (w0, h0) {
            self.sprite.clone()
        } else {
            imageops::resize(&self.sprite, w as u32, h as u32, imageops::FilterType::Nearest)
        };
        let left = (pose.x - w / 2.0).round() as i32;
        let top = FEET - h as i32 - pose.lift.round() as i32;
        let mut placed = Placed {
            scaled,
            left,
            top,
            lean: pose.lean * self.dir as f32,
            dir: self.dir,
            near: (0.0, 0.0),
            far: (0.0, 0.0),
            head_top: top as f32,
            centre: pose.x,
        };
        // The shoulders sit just inside the top's edges, a little below its first row.
        let (x0, y0, x1, _) = self.top;
        let (near_x, far_x) = if self.dir > 0 {
            (x1 - 3, x0 + 3)
        } else {
            (x0 + 3, x1 - 3)
        };
        let y = (y0 + 4) as f32;
        let (sx, sy) = (w / w0, h / h0);
        placed.near = placed.map(near_x as f32 + 0.5, y, sx, sy);
        placed.far = placed.map(far_x as f32 + 0.5, y, sx, sy);
        placed
    }
}

impl Placed {
    /// Where a point of the unscaled sprite is on the canvas.
    fn map(&self, x: f32, y: f32, sx: f32, sy: f32) -> (f32, f32) {
        let h = self.scaled.height() as f32;
        let shift = (self.lean * (1.0 - y * sy / h)).round();
        (self.left as f32 + x * sx + shift, self.top as f32 + y * sy)
    }

    /// Whether the figure has a pixel at this spot of the canvas.
    fn covers(&self, x: i32, y: i32) -> bool {
        let (row, h) = (y - self.top, self.scaled.height() as i32);
        if row < 0 || row >= h {
            return false;
        }
        let shift = (self.lean * (1.0 - row as f32 / h as f32)).round() as i32;
        let col = x - self.left - shift;
        col >= 0 && col < self.scaled.width() as i32 && self.scaled.get_pixel(col as u32, row as u32)[3] > 127
    }

    /// Draws the figure, each row shifted so the head leans.
    fn draw(&self, canvas: &mut RgbaImage) {
        let h = self.scaled.height() as f32;
        for (x, y, px) in self.scaled.enumerate_pixels() {
            if px[3] > 127 {
                let shift = (self.lean * (1.0 - y as f32 / h)).round() as i32;
                put(
                    canvas,
                    self.left + x as i32 + shift,
                    self.top + y as i32,
                    Rgba([px[0], px[1], px[2], 255]),
                );
            }
        }
    }

    fn shadow(&self, canvas: &mut RgbaImage, kind: Interaction, lift: f32) {
        let color = hex(kind.palette().3);
        let (cx, cy) = (self.centre, FEET as f32 + 1.0);
        let grow = 1.0 - (lift / 30.0).min(0.4);
        let (rx, ry) = (16.0 * grow, 3.5 * grow);
        for y in (cy - ry).floor() as i32..=(cy + ry).ceil() as i32 {
            for x in (cx - rx).floor() as i32..=(cx + rx).ceil() as i32 {
                let (dx, dy) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
                if dx * dx + dy * dy <= 1.0 {
                    put(canvas, x, y, color);
                }
            }
        }
    }
}

/// The sprite cropped to the box around its visible pixels, and where that box starts.
fn trim(sprite: &RgbaImage) -> (RgbaImage, i32, i32) {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (x, y, px) in sprite.enumerate_pixels() {
        if px[3] > 127 {
            bounds = Some(match bounds {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            });
        }
    }
    match bounds {
        Some((x0, y0, x1, y1)) => (
            imageops::crop_imm(sprite, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image(),
            x0 as i32,
            y0 as i32,
        ),
        None => (RgbaImage::new(1, 1), 0, 0),
    }
}

// /Arms and hands

/// A point on a quadratic curve from `a` through the pull of `c` to `b`.
fn curve(a: (f32, f32), c: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    (
        u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0,
        u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1,
    )
}

/// A sleeve from the shoulder to the wrist, bent toward `pull`, with a cuff at the wrist.
fn arm(canvas: &mut RgbaImage, figure: &Figure, from: (f32, f32), pull: (f32, f32), wrist: (f32, f32)) {
    let length = ((wrist.0 - from.0).hypot(wrist.1 - from.1)).max(1.0);
    let steps = (length * 1.6) as usize + 2;
    let at = |i: usize| curve(from, pull, wrist, i as f32 / steps as f32);
    for i in 0..=steps {
        let (x, y) = at(i);
        disc(canvas, x, y, 4.0, OUTLINE);
    }
    for i in 0..=steps {
        let (x, y) = at(i);
        disc(canvas, x, y, 3.0, hex(figure.sleeve[2]));
    }
    // Light along the top edge of the sleeve, dark along the bottom.
    for i in 0..=steps {
        let (x, y) = at(i);
        disc(canvas, x - 0.8, y - 1.1, 1.1, hex(figure.sleeve[3]));
    }
    for i in (steps / 3..=steps).step_by(2) {
        let (x, y) = at(i);
        disc(canvas, x + 1.2, y + 1.5, 0.9, hex(figure.sleeve[1]));
    }
    // The cuff.
    for i in steps.saturating_sub(3)..=steps {
        let (x, y) = at(i);
        disc(canvas, x, y, 3.2, hex(figure.sleeve[4]));
    }
}

/// An open hand at the wrist, turned `angle` radians clockwise from fingers-up and drawn `size`
/// times its natural size. The hand is a palm, four fingers and a thumb, built from shapes so it
/// can turn without going ragged.
fn hand(canvas: &mut RgbaImage, figure: &Figure, wrist: (f32, f32), angle: f32, thumb_left: bool, size: f32) {
    let (sin, cos) = angle.sin_cos();
    let [dark, base, light] = [figure.skin[1], figure.skin[2], figure.skin[3]].map(hex);
    let flip = if thumb_left { 1.0 } else { -1.0 };
    // Fingers: (across, from, length).
    let fingers = [(-3.0, 6.0, 3.4), (-1.0, 6.4, 4.6), (1.0, 6.4, 4.4), (3.0, 6.0, 3.2)];
    let reach = 16.0 * size;
    let mut mask = Vec::new();
    for y in (wrist.1 - reach) as i32..=(wrist.1 + reach) as i32 {
        for x in (wrist.0 - reach) as i32..=(wrist.0 + reach) as i32 {
            // The pixel in the hand's own frame: u across (thumb side negative), v along the fingers.
            let (dx, dy) = ((x as f32 + 0.5 - wrist.0) / size, (y as f32 + 0.5 - wrist.1) / size);
            let u = (dx * cos + dy * sin) * flip;
            let v = -(-dx * sin + dy * cos);
            let in_palm = {
                let (a, b) = (u / 4.4, (v - 4.0) / 3.7);
                a * a + b * b <= 1.0
            };
            let in_finger = fingers.iter().any(|&(fu, from, len)| {
                let t = v.clamp(from, from + len);
                let (du, dv) = (u - fu, v - t);
                du * du + dv * dv <= 0.95 * 0.95
            });
            let in_thumb = {
                let (ax, ay, bx, by) = (-3.8, 2.6, -6.8, 5.6);
                let (px, py) = (u - ax, v - ay);
                let (qx, qy) = (bx - ax, by - ay);
                let t = ((px * qx + py * qy) / (qx * qx + qy * qy)).clamp(0.0, 1.0);
                let (du, dv) = (px - qx * t, py - qy * t);
                du * du + dv * dv <= 1.2 * 1.2
            };
            if in_palm || in_finger || in_thumb {
                let color = if u > 2.0 && v < 6.0 {
                    dark
                } else if v > 2.5 && u < -0.5 && v < 6.5 {
                    light
                } else {
                    base
                };
                mask.push((x, y, color));
            }
        }
    }
    stamp(canvas, &mask, OUTLINE);
}

/// Draws what `paint` makes on the canvas except where `other` stands in front of it.
fn behind(canvas: &mut RgbaImage, other: &Placed, paint: impl FnOnce(&mut RgbaImage)) {
    let mut layer = RgbaImage::new(canvas.width(), canvas.height());
    paint(&mut layer);
    for (x, y, px) in layer.enumerate_pixels() {
        if px[3] > 0 && !other.covers(x as i32, y as i32) {
            canvas.put_pixel(x, y, *px);
        }
    }
}

/// Short lines fanning out of a point, like the ones that show a hand is moving.
fn ticks(canvas: &mut RgbaImage, centre: (f32, f32), side: f32, color: Rgba<u8>) {
    for degrees in [8.0f32, 42.0, 76.0] {
        let angle = degrees.to_radians();
        let dir = (side * angle.cos(), -angle.sin());
        for step in 0..6 {
            let d = 12.0 + step as f32;
            let (x, y) = (
                (centre.0 + dir.0 * d).round() as i32,
                (centre.1 + dir.1 * d).round() as i32,
            );
            put(canvas, x, y, color);
            put(canvas, x, y + 1, color);
        }
    }
}

// /Scenes

/// Draws the scene `at` through its loop: `a` on the left facing right, `b` facing it.
mod scene;
use scene::scene;
#[cfg(test)]
use scene::{stand, twinkles};

#[cfg(test)]
#[path = "social/tests.rs"]
mod tests;
