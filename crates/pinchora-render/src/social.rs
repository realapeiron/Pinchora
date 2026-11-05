//! Animated hug, pat, wave and high five: two players' pixel characters, drawn from their real
//! look and colours, on a soft background. Everything is composed at the sprites' native
//! resolution and blown up with nearest-neighbour, so every art pixel stays crisp.
//!
//! A look is one flat sprite, so the motion is procedural: figures step, hop, lean (a shear that
//! shifts the upper rows) and squash, and what the sprite cannot do itself is drawn over it in
//! the player's own colours: sleeves, hands, hearts, sparkles and the clap flash.

use std::f32::consts::{PI, TAU};

use image::{Delay, Frame, Rgba, RgbaImage, codecs::gif::GifEncoder, codecs::gif::Repeat, imageops};

use crate::{
    character::{Colors, OUTFIT_COLORS, SKIN_TONES, TOP_KEY, recolor_character},
    sprites::variant_ramp,
};

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

/// One of the two players: their character's look as drawn (in the key colours, before the
/// player's own are swapped in) and the colours they chose, which the hands and sleeves drawn
/// over the sprite use too.
#[derive(Clone, Copy)]
pub struct Person<'a> {
    pub look: &'a RgbaImage,
    pub colors: Colors,
}

/// A looping GIF of `from` doing `kind` to `to`, 480×320.
pub fn interaction_gif(kind: Interaction, from: Person<'_>, to: Person<'_>) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut out, 30);
        encoder.set_repeat(Repeat::Infinite)?;
        for i in 0..FRAMES {
            let frame = scale_up(&scene(kind, from, to, i as f32 / FRAMES as f32));
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
    scale_up(&scene(kind, from, to, at))
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

fn smooth(x: f32) -> f32 {
    let x = clamp01(x);
    x * x * (3.0 - 2.0 * x)
}

/// 0 before `a`, 1 after `b`, eased between.
fn between(x: f32, a: f32, b: f32) -> f32 {
    smooth((x - a) / (b - a))
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hex(c: [u8; 3]) -> Rgba<u8> {
    Rgba([c[0], c[1], c[2], 255])
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [0, 1, 2].map(|i| lerp(f32::from(a[i]), f32::from(b[i]), t).round() as u8)
}

fn rgb(v: u32) -> [u8; 3] {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

// /Canvas

/// Draws `color` at (x, y) if it is on the canvas.
fn put(canvas: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x >= 0 && y >= 0 && x < canvas.width() as i32 && y < canvas.height() as i32 {
        canvas.put_pixel(x as u32, y as u32, color);
    }
}

/// Draws a sprite with its top-left at (x0, y0); transparent pixels leave the canvas alone.
fn blit(canvas: &mut RgbaImage, sprite: &RgbaImage, x0: i32, y0: i32) {
    for (x, y, px) in sprite.enumerate_pixels() {
        if px[3] > 127 {
            put(canvas, x0 + x as i32, y0 + y as i32, Rgba([px[0], px[1], px[2], 255]));
        }
    }
}

/// A sprite drawn from text rows: each character is a key into `palette`, anything else is clear.
fn picture(rows: &[&str], palette: &[(char, Rgba<u8>)]) -> RgbaImage {
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
fn disc(canvas: &mut RgbaImage, cx: f32, cy: f32, r: f32, color: Rgba<u8>) {
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
    blit(
        canvas,
        &sprite,
        (x - sprite.width() as f32 / 2.0).round() as i32,
        (y - a * 40.0 - sprite.height() as f32).round() as i32,
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
    fn new(person: Person<'_>, dir: i32) -> Self {
        let look = trim(person.look);
        let look = if dir < 0 {
            imageops::flip_horizontal(&look)
        } else {
            look
        };
        let c = person.colors;
        let skin = SKIN_TONES[c.skin % SKIN_TONES.len()].1.map(rgb);
        let sleeve = variant_ramp(OUTFIT_COLORS[c.top % OUTFIT_COLORS.len()]).unwrap_or([[0x19, 0xb8, 0xc8]; 5]);
        let top = shirt_box(&look).unwrap_or((
            look.width() as i32 / 5,
            look.height() as i32 * 2 / 5,
            look.width() as i32 * 4 / 5,
            look.height() as i32 * 3 / 4,
        ));
        Self {
            sprite: recolor_character(&look, c),
            dir,
            top,
            skin,
            sleeve,
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

/// The sprite cropped to the box around its visible pixels.
fn trim(sprite: &RgbaImage) -> RgbaImage {
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
        Some((x0, y0, x1, y1)) => imageops::crop_imm(sprite, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image(),
        None => RgbaImage::new(1, 1),
    }
}

/// Where the shirt is in a look drawn in the key colours: from its first row (the first with a
/// few shirt pixels, so a stray one in the hair does not count) the box around the shirt pixels
/// of the next fifteen rows, which is the shoulders and sleeves. `(x0, y0, x1, y1)`, inclusive.
fn shirt_box(look: &RgbaImage) -> Option<(i32, i32, i32, i32)> {
    let shirt = |px: &Rgba<u8>| px[3] > 127 && TOP_KEY[1..].contains(&[px[0], px[1], px[2]]);
    let first =
        (0..look.height()).find(|&y| (0..look.width()).filter(|&x| shirt(look.get_pixel(x, y))).count() >= 4)?;
    let (mut x0, mut x1) = (i32::MAX, i32::MIN);
    let last = (first + 15).min(look.height() - 1);
    for y in first..=last {
        for x in 0..look.width() {
            if shirt(look.get_pixel(x, y)) {
                x0 = x0.min(x as i32);
                x1 = x1.max(x as i32);
            }
        }
    }
    (x0 <= x1).then_some((x0, first as i32, x1, last as i32))
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

/// Draws the scene `at` through its loop.
fn scene(kind: Interaction, from: Person<'_>, to: Person<'_>, at: f32) -> RgbaImage {
    let mut canvas = backdrop(kind);
    twinkles(&mut canvas, at);
    let a = Figure::new(from, 1);
    let b = Figure::new(to, -1);
    match kind {
        Interaction::Hug => hug(&mut canvas, &a, &b, at),
        Interaction::Pat => pat(&mut canvas, &a, &b, at),
        Interaction::Wave => wave(&mut canvas, &a, &b, at),
        Interaction::HighFive => high_five(&mut canvas, &a, &b, at),
    }
    canvas
}

/// Little stars in the sky that blink at their own times.
fn twinkles(canvas: &mut RgbaImage, at: f32) {
    let white = Rgba([255, 255, 255, 255]);
    for (i, &(x, y)) in [(24, 24), (206, 18), (118, 14), (44, 66), (196, 62), (150, 34), (84, 30)]
        .iter()
        .enumerate()
    {
        let phase = (at * 2.0 + i as f32 * 0.29).fract();
        let r = match (phase * 4.0) as i32 {
            0 => 1,
            1 => 2,
            2 => 1,
            _ => 0,
        };
        if r > 0 {
            sparkle(canvas, x, y, r, white, white);
        } else {
            put(canvas, x, y, white);
        }
    }
}

/// A hop that peaks at `height` while `p` goes from 0 to 1 and back to 0; flat when `p` is at rest.
fn hop(p: f32, height: f32) -> f32 {
    if p <= 0.0 || p >= 1.0 {
        0.0
    } else {
        height * (p * 2.0 * PI).sin().abs()
    }
}

/// Puts both figures down: shadows first, so a shadow never covers a foot.
fn stand(canvas: &mut RgbaImage, kind: Interaction, a: (&Figure, Pose), b: (&Figure, Pose)) -> (Placed, Placed) {
    let (pa, pb) = (a.0.place(a.1), b.0.place(b.1));
    pa.shadow(canvas, kind, a.1.lift);
    pb.shadow(canvas, kind, b.1.lift);
    pa.draw(canvas);
    pb.draw(canvas);
    (pa, pb)
}

fn spark(canvas: &mut RgbaImage, x: i32, y: i32, r: i32) {
    sparkle(canvas, x, y, r, Rgba([255, 255, 255, 255]), Rgba([255, 240, 150, 255]));
}

fn hug(canvas: &mut RgbaImage, a: &Figure, b: &Figure, at: f32) {
    // Steps in, holds the hug, steps back out.
    let walk = between(at, 0.04, 0.34) - between(at, 0.70, 0.98);
    let squeeze = between(at, 0.30, 0.42) - between(at, 0.68, 0.78);
    let pulse = (at * TAU * 4.0).sin();
    let pose = |home: f32, to: f32| Pose {
        x: lerp(home, to, walk),
        lift: hop(walk, 7.0) + squeeze * pulse.abs() * 1.5,
        sx: 1.0 - 0.03 * squeeze * pulse,
        sy: 1.0 + 0.03 * squeeze * pulse,
        lean: 6.0 * squeeze + 2.0 * (1.0 - squeeze) * walk,
    };
    let (pa, pb) = (pose(64.0, 102.0), pose(176.0, 138.0));
    let (placed_a, placed_b) = stand(canvas, Interaction::Hug, (a, pa), (b, pb));

    // Arms: held open while they come together, around each other once they meet. The left one
    // reaches round the other's back in front; the right one goes behind.
    let open = 1.0 - squeeze;
    if walk > 0.05 || squeeze > 0.0 {
        for (me, other, figure, drop) in [(&placed_a, &placed_b, a, 3.0), (&placed_b, &placed_a, b, 11.0)] {
            let dir = me.dir as f32;
            let wrap = (other.centre + dir * 9.0, other.far.1 + drop);
            let spread = (me.near.0 + dir * 21.0, me.near.1 + 1.0);
            let wrist = (lerp(wrap.0, spread.0, open), lerp(wrap.1, spread.1, open));
            let pull = (me.near.0 + dir * 8.0, me.near.1 + lerp(8.0, 11.0, open));
            let paint = |layer: &mut RgbaImage| {
                arm(layer, figure, me.near, pull, wrist);
                hand(layer, figure, wrist, dir * lerp(1.7, 0.7, open), me.dir > 0, 1.2);
            };
            if me.dir < 0 && squeeze > 0.5 {
                behind(canvas, other, paint);
            } else {
                paint(canvas);
            }
        }
    }

    // Hearts drift up from between them while they hold on.
    for j in 0..4 {
        let born = 0.36 + 0.085 * j as f32;
        let age = (at - born).rem_euclid(1.0) / 0.34;
        if at >= 0.34 {
            float_heart(canvas, 120.0 + (j as f32 - 1.5) * 10.0, 46.0, age, 5.0, j as f32 * 1.7);
        }
    }
    if squeeze > 0.5 {
        for (i, &(dx, dy)) in [(-40, -4), (42, 0), (-12, -26), (18, -34)].iter().enumerate() {
            let k = ((at * 3.0 + i as f32 * 0.25).fract() * 3.0) as usize;
            spark(canvas, 120 + dx, 60 + dy, [1, 2, 1][k % 3]);
        }
    }
}

fn pat(canvas: &mut RgbaImage, a: &Figure, b: &Figure, at: f32) {
    let walk = between(at, 0.04, 0.30) - between(at, 0.80, 0.98);
    // The one patted ducks a little to meet the hand, then three pats, the hand coming down and
    // lifting again.
    let bow = between(at, 0.22, 0.32) - between(at, 0.78, 0.90);
    let pats = between(at, 0.30, 0.34) - between(at, 0.76, 0.80);
    let beat = ((at - 0.32) / 0.44 * 3.0).clamp(0.0, 3.0);
    let down = if pats > 0.0 { (beat * PI).sin().abs() } else { 0.0 };
    let squash = 0.05 * pats * down.powi(3);

    let pa = Pose {
        x: lerp(62.0, 112.0, walk),
        lift: hop(walk, 6.0) + 3.0 * bow,
        sx: 1.0,
        sy: 1.0 + 0.03 * bow,
        lean: 4.0 * pats + 2.0 * walk,
    };
    let pb = Pose {
        x: lerp(176.0, 148.0, walk),
        lift: 0.0,
        sx: 1.0 + 0.04 * bow + squash * 0.6,
        sy: 1.0 - 0.11 * bow - squash,
        lean: -3.0 * bow,
    };
    let (placed_a, placed_b) = stand(canvas, Interaction::Pat, (a, pa), (b, pb));

    // The hand rests on the other's head and presses down on each pat.
    let reach = between(at, 0.16, 0.34) - between(at, 0.78, 0.92);
    if reach > 0.0 {
        let rest = (placed_a.near.0 + 8.0, placed_a.near.1 + 14.0);
        let lifted = 7.0 * (1.0 - down) * pats;
        let target = (
            placed_b.centre - 9.0,
            placed_b.head_top + 2.0 - lifted - 6.0 * (1.0 - pats),
        );
        let wrist = (lerp(rest.0, target.0, reach), lerp(rest.1, target.1, reach));
        let pull = (wrist.0 - 4.0, placed_a.near.1 - 14.0 * reach + 6.0);
        arm(canvas, a, placed_a.near, pull, wrist);
        hand(
            canvas,
            a,
            wrist,
            lerp(0.3, 1.72 + 0.12 * (1.0 - down), reach),
            true,
            1.25,
        );
    }
    // A puff and a heart for each pat that lands.
    if pats > 0.5 {
        let n = beat.floor() as usize;
        if down > 0.75 {
            let (cx, cy) = (placed_b.centre.round() as i32, placed_b.head_top.round() as i32);
            for (dx, dy) in [(-20, 8), (20, 8), (-16, -2), (16, -2)] {
                spark(canvas, cx + dx, cy + dy, 2);
            }
        }
        float_heart(
            canvas,
            placed_b.centre + [-8.0, 9.0, 0.0][n % 3],
            placed_b.head_top - 6.0,
            beat.fract() * 0.9,
            4.0,
            n as f32,
        );
    }
}

fn wave(canvas: &mut RgbaImage, a: &Figure, b: &Figure, at: f32) {
    // A hops hello, waves, and B hops back, delighted.
    let raise = between(at, 0.12, 0.20) - between(at, 0.84, 0.92);
    let swing = if raise > 0.0 {
        ((at - 0.12) * TAU * 3.6).sin()
    } else {
        0.0
    };
    let pa = Pose {
        x: 76.0 + 4.0 * raise,
        lift: if (0.03..0.12).contains(&at) {
            7.0 * ((at - 0.03) / 0.09 * PI).sin()
        } else {
            0.0
        },
        sx: 1.0,
        sy: 1.0,
        lean: 3.0 * raise,
    };
    let hello = ((at - 0.40) / 0.14).clamp(0.0, 1.0);
    let pb = Pose {
        x: 164.0,
        lift: if (0.40..0.54).contains(&at) {
            9.0 * (hello * PI).sin()
        } else if (0.62..0.72).contains(&at) {
            5.0 * (((at - 0.62) / 0.10) * PI).sin()
        } else {
            0.0
        },
        sx: 1.0,
        sy: 1.0,
        lean: -3.0 * between(at, 0.38, 0.5),
    };
    let (placed_a, placed_b) = stand(canvas, Interaction::Wave, (a, pa), (b, pb));

    if raise > 0.3 {
        let wrist = (
            lerp(placed_a.near.0 + 6.0, placed_a.near.0 + 17.0, raise),
            lerp(placed_a.near.1 + 18.0, placed_a.near.1 - 20.0, raise),
        );
        let pull = (placed_a.near.0 + 16.0, placed_a.near.1 + 2.0);
        arm(canvas, a, placed_a.near, pull, wrist);
        hand(canvas, a, wrist, swing * 0.5, true, 1.3);
        // Lines beside the hand show which way it is going.
        if swing.abs() > 0.4 && raise > 0.8 {
            let color = hex(mix(Interaction::Wave.palette().3, [0x1b, 0x1d, 0x2b], 0.45));
            ticks(canvas, (wrist.0, wrist.1 - 6.0), swing.signum(), color);
        }
    }
    // B answers with a heart.
    if at > 0.46 && at < 0.9 {
        float_heart(
            canvas,
            placed_b.centre,
            placed_b.head_top - 4.0,
            (at - 0.46) / 0.44,
            3.0,
            1.0,
        );
    }
}

fn high_five(canvas: &mut RgbaImage, a: &Figure, b: &Figure, at: f32) {
    // Both run in with an arm up, slap hands in the middle, and drift back.
    let walk = between(at, 0.02, 0.32) - between(at, 0.74, 0.98);
    let clap = between(at, 0.28, 0.34) - between(at, 0.62, 0.74);
    let kick = (between(at, 0.32, 0.38) - between(at, 0.38, 0.56)).max(0.0);
    let pose = |home: f32, to: f32| Pose {
        x: lerp(home, to, walk),
        lift: hop(walk, 8.0) + 4.0 * kick,
        sx: 1.0 - 0.03 * kick,
        sy: 1.0 + 0.04 * kick,
        lean: 4.0 * walk - 3.0 * kick,
    };
    let (pa, pb) = (pose(56.0, 88.0), pose(184.0, 152.0));
    let (placed_a, placed_b) = stand(canvas, Interaction::HighFive, (a, pa), (b, pb));

    let meet = (120.0, 58.0);
    let raised = walk.max(0.15);
    for (me, figure) in [(&placed_a, a), (&placed_b, b)] {
        let dir = me.dir as f32;
        let up = (me.near.0 + dir * 12.0, me.near.1 - 22.0);
        let wrist = (
            lerp(me.near.0 + dir * 6.0, lerp(up.0, meet.0 - dir * 3.0, clap), raised),
            lerp(me.near.1 + 18.0, lerp(up.1, meet.1 + 4.0, clap), raised),
        );
        let pull = (me.near.0 + dir * 12.0, me.near.1 + 2.0 - 8.0 * raised);
        arm(canvas, figure, me.near, pull, wrist);
        hand(canvas, figure, wrist, dir * lerp(0.1, 0.35, clap), me.dir > 0, 1.3);
    }
    // The flash where the palms meet.
    let burst = between(at, 0.30, 0.34) * (1.0 - between(at, 0.44, 0.58));
    if burst > 0.02 {
        flash(canvas, (meet.0.round() as i32, meet.1.round() as i32 - 10), burst);
        let age = ((at - 0.32) / 0.3).clamp(0.0, 1.0);
        for k in 0..8 {
            let angle = k as f32 / 8.0 * TAU + 0.3;
            let dist = 16.0 + 28.0 * age;
            spark(
                canvas,
                (meet.0 + angle.cos() * dist).round() as i32,
                (meet.1 - 10.0 + angle.sin() * dist * 0.8).round() as i32,
                if age < 0.6 { 2 } else { 1 },
            );
        }
    }
}

/// A star-shaped flash of light, `size` from 0 to 1.
fn flash(canvas: &mut RgbaImage, centre: (i32, i32), size: f32) {
    let radius = 5.0 + 11.0 * size;
    let mut mask = Vec::new();
    let reach = radius as i32 + 2;
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let (fx, fy) = (dx as f32 + 0.5, dy as f32 + 0.5);
            let (rho, theta) = (fx.hypot(fy), fy.atan2(fx));
            let spikes = (theta * 4.0).cos().abs().powf(3.0);
            if rho <= radius * (0.28 + 0.72 * spikes) {
                let color = if rho < radius * 0.32 {
                    Rgba([255, 255, 255, 255])
                } else if rho < radius * 0.62 {
                    Rgba([255, 247, 180, 255])
                } else {
                    Rgba([255, 226, 102, 255])
                };
                mask.push((centre.0 + dx, centre.1 + dy, color));
            }
        }
    }
    stamp(canvas, &mask, Rgba([0xff, 0x9f, 0x1c, 255]));
}

#[cfg(test)]
mod tests {
    use image::{AnimationDecoder, codecs::gif::GifDecoder};

    use super::*;
    use crate::{
        character::{HAIR_COLORS, placeholder_character},
        sprites::SpriteStore,
    };

    fn person(look: &str, colors: Colors) -> (RgbaImage, Colors) {
        let store = SpriteStore::embedded();
        let sprite = store
            .get(&format!("a_{look}"))
            .map_or_else(placeholder_character, |s| (*s).clone());
        (sprite, colors)
    }

    #[test]
    fn every_kind_makes_a_short_looping_gif_that_decodes() {
        let (sa, ca) = person(
            "tidewalker",
            Colors {
                skin: 1,
                hair: 1,
                top: 0,
                bottom: 2,
            },
        );
        let (sb, cb) = person(
            "longreef",
            Colors {
                skin: 5,
                hair: 8,
                top: 4,
                bottom: 3,
            },
        );
        let (a, b) = (Person { look: &sa, colors: ca }, Person { look: &sb, colors: cb });
        for kind in Interaction::ALL {
            let started = std::time::Instant::now();
            let gif = interaction_gif(kind, a, b).expect("encodes");
            let took = started.elapsed();
            println!("{kind:?}: {} KB in {took:?}", gif.len() / 1024);
            if !cfg!(debug_assertions) {
                assert!(took < std::time::Duration::from_millis(200), "{kind:?} took {took:?}");
            }
            assert!(gif.len() < 1_000_000, "{kind:?} is {} bytes", gif.len());
            let decoder = GifDecoder::new(std::io::Cursor::new(&gif)).expect("a GIF");
            let frames = decoder.into_frames().collect_frames().expect("frames");
            assert_eq!(frames.len(), FRAMES, "{kind:?}");
            assert_eq!(frames[0].buffer().dimensions(), (W as u32 * SCALE, H as u32 * SCALE));
            for i in 0..FRAMES {
                let colours: std::collections::HashSet<_> = scene(kind, a, b, i as f32 / FRAMES as f32)
                    .pixels()
                    .map(|p| p.0)
                    .collect();
                assert!(colours.len() <= 256, "{kind:?} frame {i} has {} colours", colours.len());
            }
            assert_ne!(frames[0].buffer(), frames[FRAMES / 2].buffer(), "{kind:?} moves");
        }
    }

    #[test]
    fn where_the_tops_are() {
        let store = SpriteStore::embedded();
        for look in ["tidewalker", "longreef", "capewind", "curlcove", "puffcloud", "bobbin"] {
            let colors = Colors {
                skin: 1,
                hair: 1,
                top: 0,
                bottom: 2,
            };
            let (sprite, _) = person(look, colors);
            let f = Figure::new(Person { look: &sprite, colors }, 1);
            println!(
                "{look}: sprite {}x{} top box {:?}",
                f.sprite.width(),
                f.sprite.height(),
                f.top
            );
            assert!(store.get(&format!("a_{look}")).is_some());
        }
    }

    #[test]
    fn every_look_poses_every_scene_within_a_gifs_palette() {
        let store = SpriteStore::embedded();
        let mut looks: Vec<String> = ["tidewalker", "longreef", "capewind", "curlcove", "puffcloud", "bobbin"]
            .map(String::from)
            .into();
        looks.extend((0..2).map(|n| format!("missing{n}")));
        for (n, look) in looks.iter().enumerate() {
            let colors = Colors {
                skin: n % SKIN_TONES.len(),
                hair: n * 5 % HAIR_COLORS.len(),
                top: n * 7 % OUTFIT_COLORS.len(),
                bottom: n * 11 % OUTFIT_COLORS.len(),
            };
            let (sprite, _) = person(look, colors);
            // A look with no art yet is drawn as the stand-in figure.
            assert!(store.get(&format!("a_{look}")).is_some() || look.starts_with("missing"));
            let (a, b) = (Person { look: &sprite, colors }, Person { look: &sprite, colors });
            for kind in Interaction::ALL {
                for at in [0.0, 0.35, 0.6] {
                    let frame = scene(kind, a, b, at);
                    let used: std::collections::HashSet<_> = frame.pixels().map(|p| p.0).collect();
                    assert!(used.len() <= 256, "{look} {kind:?} at {at}: {} colours", used.len());
                    assert!(
                        frame.pixels().all(|p| p[3] == 255),
                        "{look} {kind:?} at {at} has clear pixels"
                    );
                }
            }
        }
    }

    #[test]
    fn keys_round_trip() {
        for kind in Interaction::ALL {
            assert_eq!(Interaction::from_key(kind.key()), Some(kind));
        }
        assert_eq!(Interaction::from_key("kiss"), None);
    }

    /// Writes a contact sheet of every kind's frames for eyeballing:
    /// `cargo test -p pinchora-render social_preview -- --ignored --nocapture`, then open
    /// `target/social-preview/*.png`.
    #[test]
    #[ignore]
    fn social_preview() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/social-preview");
        std::fs::create_dir_all(&dir).unwrap();
        let looks = [
            ("tidewalker", 1, 1, 0, 2, "longreef", 5, 8, 4, 3),
            ("curlcove", 2, 3, 1, 5, "puffcloud", 6, 9, 6, 8),
            ("capewind", 3, 10, 2, 4, "bobbin", 0, 4, 7, 9),
        ];
        for (n, &(la, sa, ha, ta, ba, lb, sb, hb, tb, bb)) in looks.iter().enumerate() {
            let ca = Colors {
                skin: sa,
                hair: ha,
                top: ta,
                bottom: ba,
            };
            let cb = Colors {
                skin: sb,
                hair: hb,
                top: tb,
                bottom: bb,
            };
            assert!(ha < HAIR_COLORS.len() && hb < HAIR_COLORS.len());
            let (pa, pb) = (person(la, ca), person(lb, cb));
            let (a, b) = (
                Person {
                    look: &pa.0,
                    colors: ca,
                },
                Person {
                    look: &pb.0,
                    colors: cb,
                },
            );
            for kind in Interaction::ALL {
                let cols = 4u32;
                let rows = (FRAMES as u32).div_ceil(cols);
                let (fw, fh) = (W as u32, H as u32);
                let mut sheet = RgbaImage::new(fw * cols, fh * rows);
                for i in 0..FRAMES {
                    let frame = scene(kind, a, b, i as f32 / FRAMES as f32);
                    imageops::overlay(
                        &mut sheet,
                        &frame,
                        (i as u32 % cols * fw) as i64,
                        (i as u32 / cols * fh) as i64,
                    );
                }
                sheet.save(dir.join(format!("{n}-{}.png", kind.key()))).unwrap();
                std::fs::write(
                    dir.join(format!("{n}-{}.gif", kind.key())),
                    interaction_gif(kind, a, b).unwrap(),
                )
                .unwrap();
            }
        }
    }
}
