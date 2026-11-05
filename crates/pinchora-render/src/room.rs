//! Tile-grid room renderer: composes pixel sprites at native resolution, then scales
//! up with nearest-neighbour so every art pixel stays crisp.

use std::sync::Arc;

use image::{Rgba, RgbaImage, imageops};

pub const TILE: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneLayer {
    Rug,
    Wall,
    Floor,
    Surface,
    Pet,
}

#[derive(Clone)]
pub struct SceneItem {
    pub sprite: Arc<RgbaImage>,
    /// Footprint origin and size in tiles.
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub d: u32,
    pub layer: SceneLayer,
    pub flipped: bool,
    /// Draws a highlight frame (the item under the cursor while editing).
    pub selected: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub d: u32,
    /// Green when the item fits here, red when it does not.
    pub ok: bool,
}

pub struct RoomScene {
    pub width: u32,
    pub height: u32,
    pub wall_rows: u32,
    pub wallpaper: Arc<RgbaImage>,
    pub floor: Arc<RgbaImage>,
    pub items: Vec<SceneItem>,
    pub cursor: Option<Cursor>,
    /// Faint tile grid, for edit mode.
    pub grid: bool,
    pub scale: u32,
}

const OUTLINE: Rgba<u8> = Rgba([0x1b, 0x1d, 0x2b, 255]);

pub fn render_room(scene: &RoomScene) -> RgbaImage {
    let (w, h) = (scene.width * TILE, scene.height * TILE);
    let mut canvas = RgbaImage::from_pixel(w, h, OUTLINE);

    for ty in 0..scene.height {
        let tile = if ty < scene.wall_rows {
            &scene.wallpaper
        } else {
            &scene.floor
        };
        for tx in 0..scene.width {
            imageops::overlay(&mut canvas, tile.as_ref(), i64::from(tx * TILE), i64::from(ty * TILE));
        }
    }
    // Baseboard where wall meets floor, and a soft shadow under it.
    let base_y = scene.wall_rows * TILE;
    for x in 0..w {
        canvas.put_pixel(x, base_y - 1, OUTLINE);
        canvas.put_pixel(x, base_y - 2, Rgba([0x4a, 0x3f, 0x3b, 255]));
        blend(&mut canvas, x, base_y, Rgba([0, 0, 0, 60]));
        blend(&mut canvas, x, base_y + 1, Rgba([0, 0, 0, 30]));
    }

    if scene.grid {
        for ty in scene.wall_rows..scene.height {
            for tx in 0..scene.width {
                for i in 0..TILE {
                    if i % 2 == 0 {
                        blend(&mut canvas, tx * TILE + i, ty * TILE, Rgba([255, 255, 255, 40]));
                        blend(&mut canvas, tx * TILE, ty * TILE + i, Rgba([255, 255, 255, 40]));
                    }
                }
            }
        }
    }

    let mut order: Vec<&SceneItem> = scene.items.iter().collect();
    // Painter's order: rugs, wall items, then everything standing on the floor by its
    // bottom row so nearer things cover farther ones.
    order.sort_by_key(|item| {
        let group = match item.layer {
            SceneLayer::Rug => 0,
            SceneLayer::Wall => 1,
            _ => 2,
        };
        let sub = match item.layer {
            SceneLayer::Surface => 2,
            SceneLayer::Pet => 1,
            _ => 0,
        };
        (group, item.y + item.d, sub, item.x)
    });
    for item in order {
        draw_item(&mut canvas, item);
    }

    if let Some(c) = scene.cursor {
        let color = if c.ok {
            Rgba([0x06, 0xd6, 0xa0, 255])
        } else {
            Rgba([0xff, 0x4d, 0x6d, 255])
        };
        frame(&mut canvas, c.x * TILE, c.y * TILE, c.w * TILE, c.d * TILE, color, 70);
    }

    border(&mut canvas);
    let s = scene.scale.max(1);
    imageops::resize(&canvas, w * s, h * s, imageops::FilterType::Nearest)
}

fn draw_item(canvas: &mut RgbaImage, item: &SceneItem) {
    let sprite: RgbaImage = if item.flipped {
        imageops::flip_horizontal(item.sprite.as_ref())
    } else {
        item.sprite.as_ref().clone()
    };
    let foot_w = item.w * TILE;
    let foot_bottom = (item.y + item.d) * TILE;
    // Sprites stand on the bottom edge of their footprint, centred horizontally, and may
    // rise above it (tall wardrobes cover the wall behind them).
    let px = i64::from(item.x * TILE) + (i64::from(foot_w) - i64::from(sprite.width())) / 2;
    let py = match item.layer {
        SceneLayer::Surface => i64::from(foot_bottom) - i64::from(sprite.height()) - 8,
        _ => i64::from(foot_bottom) - i64::from(sprite.height()),
    };
    if matches!(item.layer, SceneLayer::Floor | SceneLayer::Pet) {
        // Contact shadow.
        for x in 0..foot_w.saturating_sub(4) {
            blend(
                canvas,
                item.x * TILE + 2 + x,
                foot_bottom.saturating_sub(1),
                Rgba([0, 0, 0, 50]),
            );
        }
    }
    imageops::overlay(canvas, &sprite, px, py);
    if item.selected {
        frame(
            canvas,
            item.x * TILE,
            item.y * TILE,
            foot_w,
            item.d * TILE,
            Rgba([0xff, 0xd1, 0x66, 255]),
            40,
        );
    }
}

fn frame(canvas: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>, fill_alpha: u8) {
    let fill = Rgba([color[0], color[1], color[2], fill_alpha]);
    for yy in y..y + h {
        for xx in x..x + w {
            let edge = yy < y + 1 || yy >= y + h - 1 || xx < x + 1 || xx >= x + w - 1;
            if edge {
                set(canvas, xx, yy, color);
            } else {
                blend(canvas, xx, yy, fill);
            }
        }
    }
}

fn border(canvas: &mut RgbaImage) {
    let (w, h) = canvas.dimensions();
    for x in 0..w {
        canvas.put_pixel(x, 0, OUTLINE);
        canvas.put_pixel(x, h - 1, OUTLINE);
    }
    for y in 0..h {
        canvas.put_pixel(0, y, OUTLINE);
        canvas.put_pixel(w - 1, y, OUTLINE);
    }
}

fn set(canvas: &mut RgbaImage, x: u32, y: u32, color: Rgba<u8>) {
    if x < canvas.width() && y < canvas.height() {
        canvas.put_pixel(x, y, color);
    }
}

fn blend(canvas: &mut RgbaImage, x: u32, y: u32, color: Rgba<u8>) {
    if x >= canvas.width() || y >= canvas.height() {
        return;
    }
    let a = u32::from(color[3]);
    let px = canvas.get_pixel_mut(x, y);
    for c in 0..3 {
        px[c] = ((u32::from(px[c]) * (255 - a) + u32::from(color[c]) * a) / 255) as u8;
    }
}
