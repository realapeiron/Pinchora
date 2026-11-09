//! Isometric room renderer: a 2:1 pixel-art projection composed at native resolution,
//! then scaled up with nearest-neighbour so every art pixel stays crisp.
//!
//! Floor tile (x, y) has its top corner at screen (32·(x−y), 16·(x+y)): x runs down-right,
//! y runs down-left. The right wall stands on the y = 0 edge, the left wall on the x = 0 edge.
//! Floor items are drawn facing the lower-left; flipping mirrors them, which in this
//! projection is the same as rotating the footprint a quarter turn.

use std::{cmp::Reverse, collections::BinaryHeap, sync::Arc};

use image::{Rgba, RgbaImage, imageops};

/// Floor tile size in native pixels (a 2:1 diamond).
pub const TILE_W: u32 = 64;
pub const TILE_H: u32 = 32;
/// Wall items snap to this many rows on a wall, `LEVEL_H` pixels each.
pub const WALL_LEVELS: u32 = 3;
pub const LEVEL_H: u32 = 32;
const WALL_PAD: i32 = 8;
pub const WALL_H: i32 = WALL_LEVELS as i32 * LEVEL_H as i32 + WALL_PAD;
const SLAB: i32 = 8;
const MARGIN: i32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneLayer {
    Rug,
    Wall,
    Floor,
    Surface,
    /// Pets and characters standing on a tile.
    Figure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallSide {
    Right,
    Left,
}

#[derive(Clone)]
pub struct SceneItem {
    pub sprite: Arc<RgbaImage>,
    /// Floor layers: footprint origin tile. Wall items: `x` is the offset along the wall
    /// from the corner in tiles and `y` the level (0 = top).
    pub x: u32,
    pub y: u32,
    /// Footprint as placed, already rotated when flipped. Wall items: width along the
    /// wall in tiles and height in levels.
    pub w: u32,
    pub d: u32,
    pub layer: SceneLayer,
    /// Mirrored: floor items face the other way, wall items hang on the left wall.
    pub flipped: bool,
    /// Draws a highlight outline (the item under the cursor while editing).
    pub selected: bool,
    /// Surface items: height of the top they stand on, in native pixels.
    pub lift: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub d: u32,
    /// `Some` when the cursor sits on a wall.
    pub wall: Option<WallSide>,
    /// Green when the item fits here, red when it does not.
    pub ok: bool,
}

pub struct RoomScene {
    pub width: u32,
    pub depth: u32,
    pub wallpaper: Arc<RgbaImage>,
    pub floor: Arc<RgbaImage>,
    pub items: Vec<SceneItem>,
    pub cursor: Option<Cursor>,
    /// Tile grid on the floor and walls, for edit mode.
    pub grid: bool,
    pub scale: u32,
}

const OUTLINE: Rgba<u8> = Rgba([0x1b, 0x1d, 0x2b, 255]);
const GOLD: Rgba<u8> = Rgba([0xff, 0xd1, 0x66, 255]);
const OK: Rgba<u8> = Rgba([0x06, 0xd6, 0xa0, 255]);
const BAD: Rgba<u8> = Rgba([0xff, 0x4d, 0x6d, 255]);

/// Floor-plane coordinates of a screen point, 64 units per tile along each axis.
fn plane(lx: i32, ly: i32) -> (i32, i32) {
    (lx + 2 * ly, 2 * ly - lx - 1)
}

/// The first floor row of a screen column; the back walls end right above it.
fn floor_top(lx: i32) -> i32 {
    if lx >= 0 { (lx + 2) / 2 } else { (1 - lx) / 2 }
}

/// Screen column of the `m`-th pixel along a wall, counted from the corner.
fn wall_column(side: WallSide, m: i32) -> i32 {
    match side {
        WallSide::Right => m,
        WallSide::Left => -1 - m,
    }
}

/// Top row of wall level 0 in a screen column.
fn level_top(lx: i32) -> i32 {
    floor_top(lx) - WALL_H + WALL_PAD
}

fn side_of(item: &SceneItem) -> WallSide {
    if item.flipped { WallSide::Left } else { WallSide::Right }
}

/// Where a floor, surface or figure sprite goes: its top-left corner in scene space.
fn anchor(item: &SceneItem, sprite: &RgbaImage) -> (i32, i32) {
    let (x, y, w, d) = (item.x as i32, item.y as i32, item.w as i32, item.d as i32);
    let full = (w + d) * 32;
    let sw = sprite.width() as i32;
    let cx = 32 * (x - y) + 16 * (w - d);
    let cy = 16 * (x + y) + 8 * (w + d);
    let bottom = match item.layer {
        SceneLayer::Figure => cy + 6,
        // A sprite narrower than its footprint stands on a smaller diamond in the middle.
        _ => cy + sw.min(full) / 4 - item.lift as i32,
    };
    (cx - sw / 2, bottom - sprite.height() as i32)
}

/// Pixel bounds (min_x, min_y, max_x, max_y) of everything the scene draws.
fn bounds(scene: &RoomScene, sprites: &[RgbaImage]) -> (i32, i32, i32, i32) {
    let (w, d) = (scene.width as i32, scene.depth as i32);
    let mut b = (-32 * d - 2, 1 - WALL_H - 6, 32 * w + 2, 16 * (w + d) + SLAB + 2);
    if scene.grid {
        b = (b.0 - LABEL_GAP, b.1, b.2 + LABEL_GAP, b.3 + LABEL_GAP + 4 * GLYPH_PX);
    }
    for (item, sprite) in scene.items.iter().zip(sprites) {
        if matches!(item.layer, SceneLayer::Floor | SceneLayer::Surface | SceneLayer::Figure) {
            let (left, top) = anchor(item, sprite);
            b.0 = b.0.min(left);
            b.1 = b.1.min(top);
            b.2 = b.2.max(left + sprite.width() as i32);
            b.3 = b.3.max(top + sprite.height() as i32);
        }
    }
    b
}

struct Canvas {
    img: RgbaImage,
    ox: i32,
    oy: i32,
}

impl Canvas {
    fn get(&self, lx: i32, ly: i32) -> Option<Rgba<u8>> {
        let (x, y) = (lx + self.ox, ly + self.oy);
        (x >= 0 && y >= 0 && (x as u32) < self.img.width() && (y as u32) < self.img.height())
            .then(|| *self.img.get_pixel(x as u32, y as u32))
    }

    fn set(&mut self, lx: i32, ly: i32, c: Rgba<u8>) {
        let (x, y) = (lx + self.ox, ly + self.oy);
        if x >= 0 && y >= 0 && (x as u32) < self.img.width() && (y as u32) < self.img.height() {
            self.img.put_pixel(x as u32, y as u32, c);
        }
    }

    fn blend(&mut self, lx: i32, ly: i32, c: Rgba<u8>) {
        if c[3] == 255 {
            return self.set(lx, ly, c);
        }
        if let Some(mut px) = self.get(lx, ly) {
            let a = u32::from(c[3]);
            for i in 0..3 {
                px[i] = ((u32::from(px[i]) * (255 - a) + u32::from(c[i]) * a) / 255) as u8;
            }
            self.set(lx, ly, px);
        }
    }

    fn sprite(&mut self, sprite: &RgbaImage, left: i32, top: i32) {
        imageops::overlay(
            &mut self.img,
            sprite,
            i64::from(left + self.ox),
            i64::from(top + self.oy),
        );
    }
}

fn shade(c: Rgba<u8>, f: f32) -> Rgba<u8> {
    Rgba([
        (f32::from(c[0]) * f).min(255.0) as u8,
        (f32::from(c[1]) * f).min(255.0) as u8,
        (f32::from(c[2]) * f).min(255.0) as u8,
        255,
    ])
}

fn average(img: &RgbaImage) -> Rgba<u8> {
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for px in img.pixels().filter(|p| p[3] > 0) {
        for i in 0..3 {
            sum[i] += u64::from(px[i]);
        }
        n += 1;
    }
    let n = n.max(1);
    Rgba([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, 255])
}

/// Texture lookup that wraps in both directions.
fn texel(tex: &RgbaImage, u: i32, v: i32) -> Rgba<u8> {
    let (tw, th) = (tex.width().max(1) as i32, tex.height().max(1) as i32);
    *tex.get_pixel(u.rem_euclid(tw) as u32, v.rem_euclid(th) as u32)
}

pub fn render_room(scene: &RoomScene) -> RgbaImage {
    let sprites: Vec<RgbaImage> = scene
        .items
        .iter()
        .map(|item| {
            if item.flipped && matches!(item.layer, SceneLayer::Floor | SceneLayer::Surface | SceneLayer::Figure) {
                imageops::flip_horizontal(item.sprite.as_ref())
            } else {
                item.sprite.as_ref().clone()
            }
        })
        .collect();
    let (min_x, min_y, max_x, max_y) = bounds(scene, &sprites);
    let (cw, ch) = ((max_x - min_x + 2 * MARGIN) as u32, (max_y - min_y + 2 * MARGIN) as u32);
    let mut img = RgbaImage::new(cw, ch);
    for (y, row) in img.rows_mut().enumerate() {
        let t = y as f32 / ch as f32;
        let mix = |a: u8, b: u8| (f32::from(a) * (1.0 - t) + f32::from(b) * t) as u8;
        let c = Rgba([mix(0x2a, 0x12), mix(0x2c, 0x14), mix(0x48, 0x20), 255]);
        row.for_each(|px| *px = c);
    }
    let mut cv = Canvas {
        img,
        ox: MARGIN - min_x,
        oy: MARGIN - min_y,
    };

    draw_walls(&mut cv, scene);
    draw_floor(&mut cv, scene);
    for item in scene.items.iter().filter(|i| i.layer == SceneLayer::Rug) {
        draw_rug(&mut cv, item);
    }
    if scene.grid {
        draw_grid(&mut cv, scene);
    }
    for item in &scene.items {
        draw_shadow(&mut cv, item);
    }
    for (item, sprite) in scene.items.iter().zip(&sprites) {
        if item.layer == SceneLayer::Wall {
            draw_wall_item(&mut cv, item, sprite);
        }
    }
    let standing: Vec<usize> = (0..scene.items.len())
        .filter(|&i| {
            matches!(
                scene.items[i].layer,
                SceneLayer::Floor | SceneLayer::Surface | SceneLayer::Figure
            )
        })
        .collect();
    for i in draw_order(&scene.items, &standing) {
        let (left, top) = anchor(&scene.items[i], &sprites[i]);
        cv.sprite(&sprites[i], left, top);
    }
    for item in scene.items.iter().filter(|i| i.selected) {
        outline_item(&mut cv, item, GOLD);
    }
    if let Some(c) = scene.cursor {
        draw_cursor(&mut cv, c);
    }
    if scene.grid {
        draw_labels(&mut cv, scene);
    }

    let s = scene.scale.max(1);
    imageops::resize(&cv.img, cw * s, ch * s, imageops::FilterType::Nearest)
}

fn draw_walls(cv: &mut Canvas, scene: &RoomScene) {
    let paper = scene.wallpaper.as_ref();
    let cap = shade(average(paper), 1.18);
    for (side, len, light) in [(WallSide::Left, scene.depth, 1.0), (WallSide::Right, scene.width, 0.84)] {
        let span = len as i32 * 32;
        for m in 0..span {
            let lx = wall_column(side, m);
            let base = floor_top(lx);
            let top = base - WALL_H;
            for ly in top..base {
                let mut c = shade(texel(paper, m / 2, (ly - top) / 2), light);
                if base - ly <= 3 {
                    c = shade(c, 0.62);
                }
                cv.set(lx, ly, c);
            }
            // Wall top: a light cap with an outline above it.
            cv.set(lx, top - 1, cap);
            cv.set(lx, top - 2, cap);
            cv.set(lx, top - 3, OUTLINE);
            if m == span - 1 {
                for ly in top - 3..base {
                    cv.set(lx, ly, OUTLINE);
                }
            }
        }
    }
    // The corner where the walls meet.
    for ly in floor_top(0) - WALL_H..floor_top(0) {
        cv.blend(0, ly, Rgba([0, 0, 0, 50]));
    }
}

fn draw_floor(cv: &mut Canvas, scene: &RoomScene) {
    let (w, d) = (scene.width as i32, scene.depth as i32);
    let tex = scene.floor.as_ref();
    let (tw, th) = (tex.width().max(1) as i32, tex.height().max(1) as i32);
    let slab = average(tex);
    let front_x = 32 * (w - d);
    for lx in -32 * d - 1..32 * w + 1 {
        let mut bottom = None;
        for ly in 0..16 * (w + d) + 2 {
            let (a, b) = plane(lx, ly);
            if a < 0 || b < 0 || a >= 64 * w || b >= 64 * d {
                continue;
            }
            let mut c = texel(tex, a * tw / 64, b * th / 64);
            // Soft shade where the floor meets the walls.
            if ly - floor_top(lx) < 3 {
                c = shade(c, 0.8);
            }
            cv.set(lx, ly, c);
            bottom = Some(ly);
        }
        if let Some(bottom) = bottom {
            let f = if lx < front_x { 0.62 } else { 0.48 };
            for ly in bottom + 1..=bottom + SLAB {
                cv.set(lx, ly, shade(slab, f));
            }
            cv.set(lx, bottom + SLAB + 1, OUTLINE);
        }
    }
}

/// Visits every screen pixel over a footprint with its floor-plane coordinates relative
/// to the footprint origin.
fn each_footprint_pixel(x: i32, y: i32, w: i32, d: i32, mut f: impl FnMut(i32, i32, i32, i32)) {
    for lx in 32 * (x - y - d) - 1..=32 * (x + w - y) {
        for ly in 16 * (x + y) - 1..=16 * (x + y + w + d) {
            let (a, b) = plane(lx, ly);
            if a >= 64 * x && a < 64 * (x + w) && b >= 64 * y && b < 64 * (y + d) {
                f(lx, ly, a - 64 * x, b - 64 * y);
            }
        }
    }
}

/// Rugs are flat top-down textures projected onto their footprint.
fn draw_rug(cv: &mut Canvas, item: &SceneItem) {
    let tex = item.sprite.as_ref();
    let (tw, th) = (tex.width().max(1) as i32, tex.height().max(1) as i32);
    let (w, d) = (item.w as i32, item.d as i32);
    each_footprint_pixel(item.x as i32, item.y as i32, w, d, |lx, ly, a, b| {
        // A flipped rug is turned a quarter: its own width now runs along y.
        let (u, v) = if item.flipped {
            (b * tw / (64 * d), a * th / (64 * w))
        } else {
            (a * tw / (64 * w), b * th / (64 * d))
        };
        let px = *tex.get_pixel(u.clamp(0, tw - 1) as u32, v.clamp(0, th - 1) as u32);
        if px[3] > 0 {
            cv.blend(lx, ly, px);
        }
    });
}

fn draw_grid(cv: &mut Canvas, scene: &RoomScene) {
    let (w, d) = (scene.width as i32, scene.depth as i32);
    each_footprint_pixel(0, 0, w, d, |lx, ly, a, b| {
        if a % 64 < 2 || b % 64 < 2 {
            cv.blend(lx, ly, Rgba([255, 255, 255, 80]));
        }
    });
    for (side, len) in [(WallSide::Left, d), (WallSide::Right, w)] {
        for m in 0..len * 32 {
            let lx = wall_column(side, m);
            let top = level_top(lx);
            for ly in top..floor_top(lx) {
                if m % 32 == 0 || (ly - top) % LEVEL_H as i32 == 0 {
                    cv.blend(lx, ly, Rgba([255, 255, 255, 28]));
                }
            }
        }
    }
}

/// Size of one font pixel in scene units, and how far labels sit outside the floor edge.
const GLYPH_PX: i32 = 3;
const LABEL_GAP: i32 = 14;

/// 3×5 glyphs for the tile coordinates, one row per string, `#` for ink.
fn glyph(c: char) -> Option<[&'static str; 5]> {
    Some(match c {
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
        'N' => ["##.", "#.#", "#.#", "#.#", "#.#"],
        _ => return None,
    })
}

/// Column letter of a floor tile's x: A, B, C…
pub fn column_letter(x: u32) -> char {
    char::from(b'A' + (x % 26) as u8)
}

/// White text with a dark outline, centred on (`cx`, `cy`).
fn draw_text(cv: &mut Canvas, text: &str, cx: i32, cy: i32) {
    let chars: Vec<[&str; 5]> = text.chars().filter_map(glyph).collect();
    let width = chars.len() as i32 * 4 - 1;
    let (left, top) = (cx - width * GLYPH_PX / 2, cy - 5 * GLYPH_PX / 2);
    let ink = |gx: i32, gy: i32| {
        let (i, col) = (gx.div_euclid(4), gx.rem_euclid(4));
        col < 3
            && (0..5).contains(&gy)
            && gx >= 0
            && chars
                .get(i as usize)
                .is_some_and(|g| g[gy as usize].as_bytes()[col as usize] == b'#')
    };
    for gy in -1..6 {
        for gx in -1..=width {
            let on = ink(gx, gy);
            let edge = !on && (-1..=1).any(|dy| (-1..=1).any(|dx| ink(gx + dx, gy + dy)));
            if on || edge {
                let color = if on { Rgba([0xff, 0xff, 0xff, 255]) } else { OUTLINE };
                for py in 0..GLYPH_PX {
                    for px in 0..GLYPH_PX {
                        cv.set(left + gx * GLYPH_PX + px, top + gy * GLYPH_PX + py, color);
                    }
                }
            }
        }
    }
}

/// Tile coordinates under the two front edges: letters for x along the left edge, numbers
/// for y along the right one, so a spot reads like a chess square ("C4").
fn draw_labels(cv: &mut Canvas, scene: &RoomScene) {
    let (w, d) = (scene.width as i32, scene.depth as i32);
    let drop = SLAB + LABEL_GAP / 2 + 5 * GLYPH_PX / 2;
    for x in 0..w {
        let (mx, my) = (32 * (x - d) + 16, 16 * (x + d) + 8);
        draw_text(cv, &column_letter(x as u32).to_string(), mx - LABEL_GAP / 2, my + drop);
    }
    for y in 0..d {
        let (mx, my) = (32 * (w - y) - 16, 16 * (w + y) + 8);
        draw_text(cv, &(y + 1).to_string(), mx + LABEL_GAP / 2, my + drop);
    }
}

fn draw_shadow(cv: &mut Canvas, item: &SceneItem) {
    match item.layer {
        SceneLayer::Floor => {
            let (w, d) = (item.w as i32, item.d as i32);
            each_footprint_pixel(item.x as i32, item.y as i32, w, d, |lx, ly, a, b| {
                if a > 4 && b > 4 && a < 64 * w - 4 && b < 64 * d - 4 {
                    cv.blend(lx, ly, Rgba([0, 0, 0, 44]));
                }
            });
        }
        SceneLayer::Figure => {
            let cx = 32 * (item.x as i32 - item.y as i32);
            let cy = 16 * (item.x as i32 + item.y as i32) + 20;
            for dy in -4i32..=4 {
                for dx in -14i32..=14 {
                    if dx * dx * 16 + dy * dy * 196 <= 14 * 14 * 16 {
                        cv.blend(cx + dx, cy + dy, Rgba([0, 0, 0, 60]));
                    }
                }
            }
        }
        _ => {}
    }
}

/// Wall items are drawn flat and sheared onto the wall, one pixel down every two across.
fn draw_wall_item(cv: &mut Canvas, item: &SceneItem, sprite: &RgbaImage) {
    let side = side_of(item);
    // Centre the art in its cells if it is a little smaller than them.
    let dx = (item.w as i32 * 32 - sprite.width() as i32).max(0) / 2;
    let dy = (item.d as i32 * LEVEL_H as i32 - sprite.height() as i32).max(0) / 2;
    for (sx, sy, px) in sprite.enumerate_pixels() {
        if px[3] == 0 {
            continue;
        }
        let m = item.x as i32 * 32 + dx + sx as i32;
        let lx = wall_column(side, m);
        let ly = level_top(lx) + item.y as i32 * LEVEL_H as i32 + dy + sy as i32;
        cv.blend(lx, ly, *px);
    }
}

#[allow(clippy::too_many_arguments)]
fn wall_cells(cv: &mut Canvas, side: WallSide, x: u32, y: u32, w: u32, d: u32, color: Rgba<u8>, fill: u8) {
    let (m0, m1) = (x as i32 * 32, (x + w) as i32 * 32);
    for m in m0..m1 {
        let lx = wall_column(side, m);
        let top = level_top(lx) + y as i32 * LEVEL_H as i32;
        let bottom = top + d as i32 * LEVEL_H as i32;
        for ly in top..bottom {
            if m == m0 || m == m1 - 1 || ly == top || ly == bottom - 1 {
                cv.set(lx, ly, color);
            } else if fill > 0 {
                cv.blend(lx, ly, Rgba([color[0], color[1], color[2], fill]));
            }
        }
    }
}

fn floor_cells(cv: &mut Canvas, x: u32, y: u32, w: u32, d: u32, color: Rgba<u8>, fill: u8) {
    let (w, d) = (w as i32, d as i32);
    each_footprint_pixel(x as i32, y as i32, w, d, |lx, ly, a, b| {
        if a < 2 || b < 2 || a >= 64 * w - 2 || b >= 64 * d - 2 {
            cv.set(lx, ly, color);
        } else if fill > 0 {
            cv.blend(lx, ly, Rgba([color[0], color[1], color[2], fill]));
        }
    });
}

fn outline_item(cv: &mut Canvas, item: &SceneItem, color: Rgba<u8>) {
    match item.layer {
        SceneLayer::Wall => wall_cells(cv, side_of(item), item.x, item.y, item.w, item.d, color, 0),
        _ => floor_cells(cv, item.x, item.y, item.w, item.d, color, 0),
    }
}

fn draw_cursor(cv: &mut Canvas, c: Cursor) {
    let color = if c.ok { OK } else { BAD };
    match c.wall {
        Some(side) => wall_cells(cv, side, c.x, c.y, c.w, c.d, color, 70),
        None => floor_cells(cv, c.x, c.y, c.w, c.d, color, 70),
    }
}

/// Painter's order for things standing on the floor: an item is drawn before another when
/// it lies entirely behind it along x or y. Overlaps (a lamp on a table, a pet on a free
/// tile) go floor, then surface, then figures.
fn draw_order(items: &[SceneItem], idx: &[usize]) -> Vec<usize> {
    let rank = |l: SceneLayer| match l {
        SceneLayer::Floor => 0u8,
        SceneLayer::Surface => 1,
        _ => 2,
    };
    let rect = |i: usize| {
        let it = &items[i];
        (it.x as i32, it.y as i32, (it.x + it.w) as i32, (it.y + it.d) as i32)
    };
    let before = |i: usize, j: usize| {
        let (ax, ay, ax2, ay2) = rect(i);
        let (bx, by, bx2, by2) = rect(j);
        if ax < bx2 && bx < ax2 && ay < by2 && by < ay2 {
            return rank(items[i].layer) < rank(items[j].layer);
        }
        (ax2 <= bx && by2 > ay) || (ay2 <= by && bx2 > ax)
    };
    let n = idx.len();
    let mut next = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for i in 0..n {
        for j in 0..n {
            if i != j && before(idx[i], idx[j]) {
                next[i].push(j);
                indeg[j] += 1;
            }
        }
    }
    let key = |k: usize| {
        let (x, y, _, _) = rect(idx[k]);
        Reverse((x + y, rank(items[idx[k]].layer), k))
    };
    let mut ready: BinaryHeap<_> = (0..n).filter(|&k| indeg[k] == 0).map(key).collect();
    let mut out = Vec::with_capacity(n);
    let mut done = vec![false; n];
    while let Some(Reverse((_, _, k))) = ready.pop() {
        out.push(idx[k]);
        done[k] = true;
        for &j in &next[k] {
            indeg[j] -= 1;
            if indeg[j] == 0 {
                ready.push(key(j));
            }
        }
    }
    // A cycle should not happen; if it does, the rest still gets drawn back to front.
    let mut rest: Vec<usize> = (0..n).filter(|&k| !done[k]).collect();
    rest.sort_by_key(|&k| {
        let (_, _, x2, y2) = rect(idx[k]);
        x2 + y2
    });
    out.extend(rest.into_iter().map(|k| idx[k]));
    out
}

#[cfg(test)]
mod tests {
    use crate::room::*;

    fn item(x: u32, y: u32, w: u32, d: u32, layer: SceneLayer) -> SceneItem {
        SceneItem {
            sprite: Arc::new(RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 255]))),
            x,
            y,
            w,
            d,
            layer,
            flipped: false,
            selected: false,
            lift: 0,
        }
    }

    #[test]
    fn walls_meet_the_floor() {
        // Every column starts its floor right below its wall: no gap, no overlap.
        for lx in -64..64 {
            let top = floor_top(lx);
            let (a, b) = plane(lx, top);
            assert!(a >= 0 && b >= 0, "column {lx} starts on the floor");
            let (a, b) = plane(lx, top - 1);
            assert!(a < 0 || b < 0, "column {lx} has floor above its wall");
        }
    }

    #[test]
    fn nearer_items_draw_later() {
        let items = vec![
            item(2, 2, 1, 1, SceneLayer::Floor),
            item(0, 0, 2, 2, SceneLayer::Floor),
            item(0, 0, 1, 1, SceneLayer::Surface),
            item(4, 0, 1, 3, SceneLayer::Floor),
            item(3, 3, 1, 1, SceneLayer::Figure),
        ];
        let order = draw_order(&items, &[0, 1, 2, 3, 4]);
        let pos = |i| order.iter().position(|&k| k == i).unwrap();
        assert!(pos(1) < pos(0), "back table before the chair in front");
        assert!(pos(1) < pos(2), "table before the lamp on it");
        assert!(pos(0) < pos(4), "chair before the pet in front of it");
        assert_eq!(order.len(), 5);
    }

    #[test]
    fn renders_a_room() {
        let tex = Arc::new(RgbaImage::from_pixel(16, 16, Rgba([200, 180, 160, 255])));
        let mut chair = item(1, 1, 1, 1, SceneLayer::Floor);
        chair.selected = true;
        let mut poster = item(2, 0, 1, 1, SceneLayer::Wall);
        poster.flipped = true;
        let scene = RoomScene {
            width: 4,
            depth: 3,
            wallpaper: tex.clone(),
            floor: tex,
            items: vec![chair, poster, item(0, 1, 2, 2, SceneLayer::Rug)],
            cursor: Some(Cursor {
                x: 3,
                y: 2,
                w: 1,
                d: 1,
                wall: None,
                ok: true,
            }),
            grid: true,
            scale: 2,
        };
        let img = render_room(&scene);
        assert!(img.width() >= (4 + 3) * 32 * 2);
    }
}
