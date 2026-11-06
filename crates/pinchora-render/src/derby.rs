//! The hermit crab derby as an animated GIF: six crabs on a beach track, drawn at the sprites' own
//! resolution with every frame within a GIF's 256 colours, so the colours never shimmer.
//!
//! The race itself is decided elsewhere; this only draws the positions it is given. The picture plays
//! once (a countdown, the run, the finish with each crab's place) and stays on its last frame.

use image::{Delay, Frame, Rgba, RgbaImage, codecs::gif::GifEncoder, codecs::gif::Repeat};

use crate::social::{blit, hex, mix, put};

/// Lanes on the track.
pub const LANES: usize = 6;
/// The picture's size in pixels.
pub const WIDTH: i32 = 480;
pub const HEIGHT: i32 = 288;

const TOP: i32 = 44;
const LANE_H: i32 = 38;
/// Where a crab's sprite starts, and how far its left edge walks to put its nose on the finish line.
const START_X: i32 = 42;
const FINISH_X: i32 = 440;
const SPRITE_W: i32 = 36;
const TRAVEL: f32 = (FINISH_X - START_X - SPRITE_W) as f32;
/// How far past the line a finished crab trots before it stops, as a share of the track.
const OVERSHOOT: f32 = 0.05;
/// Frame times in hundredths of a second.
const READY: u16 = 70;
const GO: u16 = 40;
const STEP: u16 = 9;
const REST: u16 = 500;

const NAVY: [u8; 3] = [0x1b, 0x1d, 0x2b];

/// What the GIF draws: the six crabs as sprites (facing right), their lane colours and a finished
/// race.
pub struct DerbyScene<'a> {
    pub crabs: &'a [RgbaImage],
    /// Each lane's colour as `0xrrggbb`, behind its number.
    pub accents: [u32; LANES],
    /// How far along the track each crab is at every step: 0.0 at the start line, 1.0 at the finish.
    pub frames: &'a [[f32; LANES]],
    /// The crabs from first across the line to last.
    pub order: [usize; LANES],
}

/// The hermit crab sprite recoloured: its shell takes the three shades (dark, middle, light,
/// `0xrrggbb`) while the face keeps its own colours, and it is turned to face right, the way the
/// race runs. The pet is drawn facing left with its face in the lower left corner.
pub fn tint_crab(base: &RgbaImage, shell: [u32; 3]) -> RgbaImage {
    let (dark, mid, light) = (rgb(shell[0]), rgb(shell[1]), rgb(shell[2]));
    let shade = |c: [u8; 3]| -> Option<[u8; 3]> {
        match c {
            [0xff, 0xf1, 0xe8] => Some(mix(light, [255, 255, 255], 0.55)),
            [0xff, 0xd2, 0xc0] => Some(light),
            [0xe6, 0xd3, 0xc3] => Some(mix(light, mid, 0.35)),
            [0xb8, 0xa0, 0x8f] => Some(mix(light, mid, 0.6)),
            [0xe0, 0xac, 0x6e] => Some(mix(mid, light, 0.5)),
            [0xb9, 0x7f, 0x4a] => Some(mid),
            [0x8a, 0x5a, 0x34] | [0x7d, 0x6a, 0x5f] => Some(dark),
            [0x4a, 0x3f, 0x3b] => Some(mix(dark, [0, 0, 0], 0.4)),
            _ => None,
        }
    };
    let (w, h) = base.dimensions();
    let mut out = RgbaImage::new(w, h);
    for (x, y, px) in base.enumerate_pixels() {
        let mut px = *px;
        // The face is the lower left; the rest is shell.
        let shell_part = x >= 12 || y < 11;
        if let Some(to) = shell_part.then(|| shade([px[0], px[1], px[2]])).flatten() {
            px.0[..3].copy_from_slice(&to);
        }
        out.put_pixel(w - 1 - x, y, px);
    }
    out
}

// /Font

/// A 3×5 pixel font for the digits, capitals and a few signs; each glyph is five rows.
fn glyph(c: char) -> [&'static str; 5] {
    match c {
        'A' => [".#.", "#.#", "###", "#.#", "#.#"],
        'B' => ["##.", "#.#", "##.", "#.#", "##."],
        'C' => [".##", "#..", "#..", "#..", ".##"],
        'D' => ["##.", "#.#", "#.#", "#.#", "##."],
        'E' => ["###", "#..", "##.", "#..", "###"],
        'F' => ["###", "#..", "##.", "#..", "#.."],
        'G' => [".##", "#..", "#.#", "#.#", ".##"],
        'H' => ["#.#", "#.#", "###", "#.#", "#.#"],
        'I' => ["###", ".#.", ".#.", ".#.", "###"],
        'J' => ["..#", "..#", "..#", "#.#", ".#."],
        'K' => ["#.#", "#.#", "##.", "#.#", "#.#"],
        'L' => ["#..", "#..", "#..", "#..", "###"],
        'M' => ["#.#", "###", "###", "#.#", "#.#"],
        'N' => ["###", "#.#", "#.#", "#.#", "#.#"],
        'O' => [".#.", "#.#", "#.#", "#.#", ".#."],
        'P' => ["##.", "#.#", "##.", "#..", "#.."],
        'Q' => [".#.", "#.#", "#.#", "###", ".##"],
        'R' => ["##.", "#.#", "##.", "#.#", "#.#"],
        'S' => [".##", "#..", ".#.", "..#", "##."],
        'T' => ["###", ".#.", ".#.", ".#.", ".#."],
        'U' => ["#.#", "#.#", "#.#", "#.#", "###"],
        'V' => ["#.#", "#.#", "#.#", "#.#", ".#."],
        'W' => ["#.#", "#.#", "###", "###", "#.#"],
        'X' => ["#.#", "#.#", ".#.", "#.#", "#.#"],
        'Y' => ["#.#", "#.#", ".#.", ".#.", ".#."],
        'Z' => ["###", "..#", ".#.", "#..", "###"],
        '0' => ["###", "#.#", "#.#", "#.#", "###"],
        '1' => [".#.", "##.", ".#.", ".#.", "###"],
        '2' => ["##.", "..#", ".#.", "#..", "###"],
        '3' => ["##.", "..#", ".#.", "..#", "##."],
        '4' => ["#.#", "#.#", "###", "..#", "..#"],
        '5' => ["###", "#..", "##.", "..#", "##."],
        '6' => [".##", "#..", "###", "#.#", "###"],
        '7' => ["###", "..#", ".#.", ".#.", ".#."],
        '8' => ["###", "#.#", "###", "#.#", "###"],
        '9' => ["###", "#.#", "###", "..#", "##."],
        '!' => [".#.", ".#.", ".#.", "...", ".#."],
        _ => ["...", "...", "...", "...", "..."],
    }
}

/// How wide `text` is at `scale`, in pixels.
fn text_width(text: &str, scale: i32) -> i32 {
    (text.chars().count() as i32 * 4 - 1).max(0) * scale
}

/// Draws `text` with its top-left at `(x, y)`.
fn text(canvas: &mut RgbaImage, x: i32, y: i32, scale: i32, text: &str, color: Rgba<u8>) {
    for (n, c) in text.chars().enumerate() {
        for (row, bits) in glyph(c).iter().enumerate() {
            for (col, bit) in bits.chars().enumerate() {
                if bit == '#' {
                    rect(
                        canvas,
                        x + (n as i32 * 4 + col as i32) * scale,
                        y + row as i32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
    }
}

/// Draws `text` over a one-pixel-down shadow, centred on `cx`.
fn caption(canvas: &mut RgbaImage, cx: i32, y: i32, scale: i32, label: &str, color: Rgba<u8>) {
    let x = cx - text_width(label, scale) / 2;
    text(canvas, x, y + (scale / 3).max(1), scale, label, hex(NAVY));
    text(canvas, x, y, scale, label, color);
}

fn rect(canvas: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, color: Rgba<u8>) {
    for yy in y..y + h {
        for xx in x..x + w {
            put(canvas, xx, yy, color);
        }
    }
}

// /Scenery

fn rgb(v: u32) -> [u8; 3] {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

/// Everything that does not move: sky and sea, the sand lanes, their numbers, the start and the finish.
fn backdrop(accents: &[u32; LANES]) -> RgbaImage {
    let mut img = RgbaImage::new(WIDTH as u32, HEIGHT as u32);
    // Sky in three bands, then the sea.
    for (band, color) in [[0xc4, 0xea, 0xfc], [0xa8, 0xde, 0xfa], [0x8f, 0xd3, 0xff]]
        .into_iter()
        .enumerate()
    {
        rect(&mut img, 0, band as i32 * 8, WIDTH, 8, hex(color));
    }
    rect(&mut img, 0, 24, WIDTH, TOP - 24, hex([0x3a, 0x9b, 0xdc]));
    // Sand: two tones of lane, a darker edge between lanes, a shore at the bottom.
    for lane in 0..LANES as i32 {
        let tone = if lane % 2 == 0 {
            [0xf2, 0xdc, 0xa6]
        } else {
            [0xea, 0xd0, 0x94]
        };
        rect(&mut img, 0, TOP + lane * LANE_H, WIDTH, LANE_H, hex(tone));
        rect(
            &mut img,
            0,
            TOP + (lane + 1) * LANE_H - 1,
            WIDTH,
            1,
            hex([0xcb, 0xb0, 0x78]),
        );
    }
    let shore = TOP + LANES as i32 * LANE_H;
    rect(&mut img, 0, shore, WIDTH, HEIGHT - shore, hex([0xb9, 0x9a, 0x62]));
    rect(&mut img, 0, shore, WIDTH, 2, hex([0x8e, 0x73, 0x45]));
    // A few pebbles and shells on the shore, always in the same places.
    for k in 0..14 {
        let (x, y) = ((k * 37 + 11) % (WIDTH - 8), shore + 5 + (k * 7) % 11);
        let tone = if k % 3 == 0 {
            [0xe8, 0xd8, 0xc0]
        } else {
            [0x8e, 0x73, 0x45]
        };
        rect(&mut img, x, y, 3, 2, hex(tone));
    }
    // Lane plates with their number, in the lane's colour.
    for (lane, accent) in accents.iter().enumerate() {
        let y = TOP + lane as i32 * LANE_H;
        rect(&mut img, 0, y, 30, LANE_H - 1, hex(rgb(*accent)));
        rect(&mut img, 0, y, 30, 2, hex(mix(rgb(*accent), [255, 255, 255], 0.4)));
        rect(&mut img, 28, y, 2, LANE_H - 1, hex(mix(rgb(*accent), NAVY, 0.45)));
        let digit = char::from_digit(lane as u32 + 1, 10).unwrap_or('?');
        text(&mut img, 11, y + 14, 2, &digit.to_string(), hex(NAVY));
    }
    // The start line, and the finish: a chequered column with a banner over it.
    for row in 0..(LANES as i32 * LANE_H / 4) {
        let y = TOP + row * 4;
        let tone = if row % 2 == 0 { [255, 255, 255] } else { NAVY };
        rect(&mut img, START_X - 4, y, 3, 4, hex(tone));
        for col in 0..3 {
            let tone = if (row + col) % 2 == 0 { [255, 255, 255] } else { NAVY };
            rect(&mut img, FINISH_X + col * 4, y, 4, 4, hex(tone));
        }
    }
    rect(&mut img, FINISH_X - 2, 12, 2, TOP - 12, hex(NAVY));
    rect(&mut img, FINISH_X + 12, 12, 2, TOP - 12, hex(NAVY));
    rect(&mut img, FINISH_X - 2, 12, 16, 8, hex([0xff, 0xd1, 0x66]));
    caption(&mut img, FINISH_X + 6, 13, 1, "WIN", hex(NAVY));
    img
}

/// The sea's foam line, which slides a little every step.
fn waves(canvas: &mut RgbaImage, step: usize) {
    let foam = hex([0xe8, 0xf7, 0xff]);
    let deep = hex([0x2c, 0x7f, 0xb8]);
    for x in 0..WIDTH {
        let phase = (x + step as i32 * 3) % 24;
        if phase < 10 {
            put(canvas, x, 30, foam);
        }
        if (x + step as i32 * 2) % 20 < 8 {
            put(canvas, x, 36, deep);
        }
        if (x + 12 + step as i32 * 3) % 24 < 6 {
            put(canvas, x, TOP - 3, foam);
        }
    }
}

/// A small gold crown.
const CROWN: &[&str] = &["#.#.#", "#####", "#####"];

fn crown(canvas: &mut RgbaImage, x: i32, y: i32) {
    let gold = hex([0xff, 0xd1, 0x66]);
    // Outlined in navy, so it shows on the lit lane too.
    for (colour, grow) in [(hex(NAVY), 1), (gold, 0)] {
        for (r, row) in CROWN.iter().enumerate() {
            for (c, ch) in row.chars().enumerate() {
                if ch == '#' {
                    rect(
                        canvas,
                        x + c as i32 * 2 - grow,
                        y + r as i32 * 2 - grow,
                        2 + 2 * grow,
                        2 + 2 * grow,
                        colour,
                    );
                }
            }
        }
    }
}

/// A place badge: a disc with the place in it, gold, silver, bronze and then slate.
fn badge(canvas: &mut RgbaImage, cx: i32, cy: i32, place: usize) {
    let tone = match place {
        0 => [0xff, 0xd1, 0x66],
        1 => [0xd6, 0xde, 0xee],
        2 => [0xdb, 0x8f, 0x4c],
        _ => [0x7d, 0x8a, 0xa8],
    };
    crate::social::disc(canvas, cx as f32, cy as f32, 9.0, hex(NAVY));
    crate::social::disc(canvas, cx as f32, cy as f32, 7.5, hex(tone));
    let digit = char::from_digit(place as u32 + 1, 10).unwrap_or('?');
    text(canvas, cx - 3, cy - 5, 2, &digit.to_string(), hex(NAVY));
}

/// What the picture shows besides the race: the countdown before it, or nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Banner {
    Ready,
    Go,
    None,
}

/// One picture of the race at `step` (an index into the scene's frames).
fn draw(scene: &DerbyScene<'_>, base: &RgbaImage, step: usize, banner: Banner, finished: bool) -> RgbaImage {
    let mut img = base.clone();
    waves(&mut img, step);
    let last = scene.frames.len().saturating_sub(1);
    let now = scene.frames.get(step.min(last)).copied().unwrap_or([0.0; LANES]);
    let before = scene.frames.get(step.saturating_sub(1)).copied().unwrap_or(now);
    // The winner's lane lights up once they are across.
    let winner = scene.order[0];
    if now[winner] >= 1.0 {
        let tone = if step % 2 == 0 {
            [0xff, 0xe9, 0xa8]
        } else {
            [0xff, 0xf6, 0xd6]
        };
        let y = TOP + winner as i32 * LANE_H;
        rect(&mut img, 31, y, FINISH_X - 33, LANE_H - 1, hex(tone));
        rect(&mut img, START_X - 4, y, 3, LANE_H - 1, hex([255, 255, 255]));
    }
    for lane in 0..LANES {
        let y = TOP + lane as i32 * LANE_H;
        let progress = now[lane].min(1.0 + OVERSHOOT);
        let x = START_X + (progress * TRAVEL).round() as i32;
        let moving = (now[lane] - before[lane]) > 0.0005 && !finished;
        // The crab hops while it runs; a crab at rest stands.
        let hop = if moving {
            ((step as f32 * 1.1 + lane as f32 * 1.9).sin().abs() * 3.0).round() as i32
        } else {
            0
        };
        // A shadow, and dust behind a crab that is going fast.
        rect(&mut img, x + 6, y + LANE_H - 4, 24, 2, hex([0xb8, 0x9e, 0x68]));
        if moving && now[lane] - before[lane] > 0.027 {
            for (dx, dy) in [(-3, 26), (-7, 22), (-5, 29)] {
                rect(&mut img, x + dx, y + dy, 3, 2, hex([0xfa, 0xee, 0xcf]));
            }
        }
        if let Some(sprite) = scene.crabs.get(lane) {
            blit(&mut img, sprite, x, y + 2 - hop);
        }
        // Past the line the crab has a place.
        if now[lane] >= 1.0 || finished {
            let place = scene.order.iter().position(|&c| c == lane).unwrap_or(LANES - 1);
            badge(&mut img, FINISH_X + 30, y + LANE_H / 2, place);
            if place == 0 && now[lane] >= 1.0 {
                crown(&mut img, x + 13, y - 3 - hop);
            }
        }
    }
    caption(&mut img, WIDTH / 2, 4, 2, "HERMIT CRAB DERBY", hex([255, 255, 255]));
    match banner {
        Banner::Ready => caption(&mut img, WIDTH / 2, 136, 6, "READY", hex([0xff, 0xd1, 0x66])),
        Banner::Go => caption(&mut img, WIDTH / 2, 136, 8, "GO!", hex([0x06, 0xd6, 0xa0])),
        Banner::None => {}
    }
    img
}

/// The race as a GIF that plays once: a countdown, every step, then the finish held.
pub fn derby_gif(scene: &DerbyScene<'_>) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(scene.crabs.len() == LANES, "the derby has {LANES} crabs");
    anyhow::ensure!(!scene.frames.is_empty(), "a race has frames");
    let base = backdrop(&scene.accents);
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut out, 30);
        // Without a repeat count a GIF plays once and stays on its last picture.
        encoder.set_repeat(Repeat::Finite(0))?;
        let mut add = |image: RgbaImage, ticks: u16| {
            encoder.encode_frame(Frame::from_parts(
                image,
                0,
                0,
                Delay::from_numer_denom_ms(u32::from(ticks) * 10, 1),
            ))
        };
        add(draw(scene, &base, 0, Banner::Ready, false), READY)?;
        add(draw(scene, &base, 0, Banner::Go, false), GO)?;
        let last = scene.frames.len() - 1;
        for step in 1..=last {
            add(draw(scene, &base, step, Banner::None, false), STEP)?;
        }
        add(draw(scene, &base, last, Banner::None, true), REST)?;
    }
    Ok(out)
}

/// One picture of the race, for the lobby card to show the crabs lined up or for a test to look at.
pub fn derby_still(scene: &DerbyScene<'_>, step: usize, finished: bool) -> RgbaImage {
    draw(scene, &backdrop(&scene.accents), step, Banner::None, finished)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use image::{AnimationDecoder, codecs::gif::GifDecoder};

    use super::*;

    /// Six distinct stand-in crabs: a body block and a shell, in the pet sprite's shell colours.
    fn crabs() -> Vec<RgbaImage> {
        (0..LANES)
            .map(|n| {
                let mut img = RgbaImage::new(36, 34);
                for (x, y, px) in img.enumerate_pixels_mut() {
                    if (4..32).contains(&x) && (8..30).contains(&y) {
                        let shell = x > 16;
                        *px = if shell {
                            Rgba([0xb9, 0x7f, 0x4a, 255])
                        } else {
                            Rgba([0xff, 0x6b, 0x4a, 255])
                        };
                    }
                }
                tint_crab(&img, [0x303030 + n as u32 * 0x101010, 0x808080, 0xc0c0c0])
            })
            .collect()
    }

    fn scene<'a>(crabs: &'a [RgbaImage], frames: &'a [[f32; LANES]]) -> DerbyScene<'a> {
        DerbyScene {
            crabs,
            accents: [0xff6b4a, 0x3a9bdc, 0x06d6a0, 0xffd166, 0xb48ae0, 0xf08a24],
            frames,
            order: [2, 0, 1, 3, 4, 5],
        }
    }

    fn frames() -> Vec<[f32; LANES]> {
        (0..=40)
            .map(|step| {
                let t = step as f32 / 36.0;
                [0.0f32, 1.0, 2.0, 3.0, 4.0, 5.0].map(|lane| (t * (1.0 + (2.0 - lane).abs() * -0.03 + 0.12)).min(1.2))
            })
            .collect()
    }

    #[test]
    fn the_shell_takes_its_colour_and_the_crab_turns_to_face_right() {
        let mut base = RgbaImage::new(4, 1);
        base.put_pixel(0, 0, Rgba([0xb9, 0x7f, 0x4a, 255]));
        base.put_pixel(3, 0, Rgba([0xff, 0x6b, 0x4a, 255]));
        let tinted = tint_crab(&base, [0x112233, 0x445566, 0x778899]);
        assert_eq!(
            tinted.get_pixel(3, 0).0,
            [0x44, 0x55, 0x66, 255],
            "the shell, now on the right"
        );
        assert_eq!(
            tinted.get_pixel(0, 0).0,
            [0xff, 0x6b, 0x4a, 255],
            "the body is left alone"
        );
    }

    #[test]
    fn the_gif_plays_once_decodes_and_stays_small() {
        let crabs = crabs();
        let frames = frames();
        let gif = derby_gif(&scene(&crabs, &frames)).expect("encodes");
        assert!(gif.len() < 1_000_000, "{} KB", gif.len() / 1024);
        // No NETSCAPE loop extension: a GIF without it plays once.
        assert!(!gif.windows(11).any(|w| w == b"NETSCAPE2.0"));
        let decoded = GifDecoder::new(std::io::Cursor::new(&gif)).expect("a GIF");
        let steps: Vec<_> = decoded.into_frames().collect::<Result<_, _>>().expect("frames");
        assert_eq!(
            steps.len(),
            frames.len() + 2,
            "countdown, every step after the first, the rest"
        );
        let first = steps[0].buffer();
        assert_eq!((first.width(), first.height()), (WIDTH as u32, HEIGHT as u32));
        let finish = steps.last().expect("a last frame");
        assert!(finish.delay().numer_denom_ms().0 >= 3_000, "the finish is held");
    }

    #[test]
    fn every_frame_fits_a_gifs_palette_so_colours_never_shimmer() {
        let crabs = crabs();
        let frames = frames();
        let scene = scene(&crabs, &frames);
        let base = backdrop(&scene.accents);
        for step in [0, 7, 20, frames.len() - 1] {
            let image = draw(&scene, &base, step, Banner::Go, step == frames.len() - 1);
            let colours: HashSet<[u8; 4]> = image.pixels().map(|p| p.0).collect();
            assert!(colours.len() <= 256, "step {step} has {} colours", colours.len());
        }
    }

    #[test]
    fn crabs_walk_right_and_the_winner_gets_a_crown_and_the_first_badge() {
        let crabs = crabs();
        let frames = frames();
        let scene = scene(&crabs, &frames);
        let early = derby_still(&scene, 5, false);
        let late = derby_still(&scene, frames.len() - 1, true);
        // Lane 1's sprite is further right late than early: find its leftmost non-sand pixel in its lane.
        let lane_y = (TOP + LANE_H + 10) as u32;
        let first_crab_pixel =
            |img: &RgbaImage| (31..WIDTH as u32).find(|&x| img.get_pixel(x, lane_y).0[..3] == [0x80, 0x80, 0x80]);
        let (a, b) = (first_crab_pixel(&early), first_crab_pixel(&late));
        assert!(a.is_some() && b.is_some() && b > a, "{a:?} then {b:?}");
        // Lane 2 won: its badge is gold, and lane 0, second, has the silver one. Early on there are none.
        let badge = |img: &RgbaImage, lane: i32| {
            img.get_pixel((FINISH_X + 35) as u32, (TOP + lane * LANE_H + LANE_H / 2 + 3) as u32)
                .0[..3]
                .to_vec()
        };
        assert_eq!(badge(&late, 2), vec![0xff, 0xd1, 0x66]);
        assert_eq!(badge(&late, 0), vec![0xd6, 0xde, 0xee]);
        assert_ne!(badge(&early, 2), badge(&late, 2));
    }

    #[test]
    fn the_real_hermit_crab_in_six_tints_differs_and_fits_the_palette() {
        let base = crate::sprites::SpriteStore::embedded()
            .get("p_pebblepouch_hermit")
            .expect("the hermit crab is in the atlas");
        let shells = [
            [0x9a2f33, 0xe04a3f, 0xff8a6a],
            [0x1f5f8b, 0x3a9bdc, 0x8fd3ff],
            [0x0b5e4c, 0x06b58a, 0x6ff0c8],
            [0x8a5a10, 0xe3a21e, 0xffd166],
            [0x4b2f6b, 0x7d55a3, 0xc2a1ea],
            [0xa84b13, 0xf08a24, 0xffc27a],
        ];
        let crabs: Vec<RgbaImage> = shells.iter().map(|shell| tint_crab(&base, *shell)).collect();
        for (n, a) in crabs.iter().enumerate() {
            assert_eq!(a.dimensions(), base.dimensions());
            for b in &crabs[n + 1..] {
                assert_ne!(a, b, "every lane has its own colours");
            }
        }
        let frames = frames();
        let scene = scene(&crabs, &frames);
        let base = backdrop(&scene.accents);
        for step in [0, 20, frames.len() - 1] {
            let image = draw(&scene, &base, step, Banner::Go, step == frames.len() - 1);
            let colours: HashSet<[u8; 4]> = image.pixels().map(|p| p.0).collect();
            assert!(colours.len() <= 256, "step {step} has {} colours", colours.len());
        }
    }

    #[test]
    fn a_race_needs_six_crabs_and_some_frames() {
        let crabs = crabs();
        let frames = frames();
        assert!(derby_gif(&scene(&crabs[..5], &frames)).is_err());
        assert!(derby_gif(&scene(&crabs, &[])).is_err());
    }
}
