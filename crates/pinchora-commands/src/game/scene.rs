use std::{
    collections::HashMap,
    io::Cursor as IoCursor,
    sync::{Arc, Mutex},
};

use image::RgbaImage;
use pinchora_db::{PlacementRow, RoomRow};
use pinchora_game::{Content, Layer, house::WALL_ROWS};
use pinchora_render::{
    room::{Cursor, RoomScene, SceneItem, SceneLayer, render_room},
    sprites::{SpriteStore, placeholder_item, placeholder_pet, placeholder_tile, recolor, variant_ramp},
};

/// Sprites by name, with palette variants and placeholders made on first use.
pub struct Sprites {
    store: SpriteStore,
    made: Mutex<HashMap<String, Arc<RgbaImage>>>,
}

impl Default for Sprites {
    fn default() -> Self {
        Self {
            store: SpriteStore::embedded(),
            made: Mutex::new(HashMap::new()),
        }
    }
}

impl Sprites {
    fn cached(&self, key: &str, make: impl FnOnce() -> RgbaImage) -> Arc<RgbaImage> {
        if let Some(s) = self.made.lock().expect("sprite cache").get(key) {
            return s.clone();
        }
        let sprite = Arc::new(make());
        self.made
            .lock()
            .expect("sprite cache")
            .insert(key.to_owned(), sprite.clone());
        sprite
    }

    /// The sprite for a playable item id (`sofa__mint`).
    pub fn item(&self, content: &Content, item_id: &str) -> Option<Arc<RgbaImage>> {
        let item = content.item(item_id)?;
        let base = &item.base;
        Some(self.cached(item_id, || {
            let drawn = self.store.get(&format!("f_{}", base.id)).map(|s| (*s).clone());
            let [w, d] = base.footprint;
            let sprite = drawn.unwrap_or_else(|| placeholder_item(&base.id, w, d, scene_layer(base.layer)));
            match item.variant.and_then(variant_ramp) {
                Some(ramp) => recolor(&sprite, &ramp),
                None => sprite,
            }
        }))
    }

    pub fn pet(&self, species: &str) -> Arc<RgbaImage> {
        self.cached(&format!("p_{species}"), || {
            self.store
                .get(&format!("p_{species}"))
                .map_or_else(|| placeholder_pet(species), |s| (*s).clone())
        })
    }

    pub fn fish(&self, id: &str) -> Arc<RgbaImage> {
        self.cached(&format!("c_{id}"), || {
            self.store
                .get(&format!("c_{id}"))
                .map_or_else(|| placeholder_pet(id), |s| (*s).clone())
        })
    }

    pub fn wallpaper(&self, id: &str) -> Arc<RgbaImage> {
        self.cached(&format!("wp_{id}"), || {
            self.store
                .get(&format!("wp_{id}"))
                .map_or_else(|| placeholder_tile(id, true), |s| (*s).clone())
        })
    }

    pub fn floor(&self, id: &str) -> Arc<RgbaImage> {
        self.cached(&format!("fl_{id}"), || {
            self.store
                .get(&format!("fl_{id}"))
                .map_or_else(|| placeholder_tile(id, false), |s| (*s).clone())
        })
    }
}

pub fn scene_layer(layer: Layer) -> SceneLayer {
    match layer {
        Layer::Floor => SceneLayer::Floor,
        Layer::Rug => SceneLayer::Rug,
        Layer::Wall => SceneLayer::Wall,
        Layer::Surface => SceneLayer::Surface,
    }
}

/// Everything needed to draw one room; owned so it can move to the render thread.
pub struct RoomView {
    pub width: u32,
    pub height: u32,
    pub room: RoomRow,
    pub placements: Vec<PlacementRow>,
    pub cursor: Option<Cursor>,
    /// Placement under the cursor, drawn highlighted.
    pub selected: Option<i64>,
    pub edit: bool,
    /// Pet species to show wandering in the room (owner's and a visitor's).
    pub pets: Vec<String>,
    /// Varies where pets stand, e.g. the current hour.
    pub seed: u64,
}

/// Free floor tiles, so pets never stand inside furniture.
fn free_tiles(content: &Content, view: &RoomView) -> Vec<(u32, u32)> {
    let mut taken = vec![false; (view.width * view.height) as usize];
    for p in &view.placements {
        if let Some((layer, r)) = pinchora_game::house::footprint(content, &p.item_id, p.x as u32, p.y as u32)
            && layer == Layer::Floor
        {
            for y in r.y..(r.y + r.d).min(view.height) {
                for x in r.x..(r.x + r.w).min(view.width) {
                    taken[(y * view.width + x) as usize] = true;
                }
            }
        }
    }
    (WALL_ROWS..view.height)
        .flat_map(|y| (0..view.width).map(move |x| (x, y)))
        .filter(|&(x, y)| !taken[(y * view.width + x) as usize])
        .collect()
}

pub fn render_view(sprites: &Sprites, view: &RoomView) -> anyhow::Result<Vec<u8>> {
    let content = Content::get();
    let items: Vec<SceneItem> = view
        .placements
        .iter()
        .filter_map(|p| {
            let item = content.item(&p.item_id)?;
            let [w, d] = item.base.footprint;
            Some(SceneItem {
                sprite: sprites.item(content, &p.item_id)?,
                x: p.x as u32,
                y: p.y as u32,
                w,
                d,
                layer: scene_layer(item.base.layer),
                flipped: p.flipped,
                selected: view.selected == Some(p.id),
            })
        })
        .collect();
    let mut items: Vec<SceneItem> = items;
    let free = free_tiles(content, view);
    for (i, species) in view.pets.iter().enumerate() {
        if free.is_empty() {
            break;
        }
        let pick = (view
            .seed
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .wrapping_add(i as u64 * 7919)
            >> 16) as usize
            % free.len();
        let (x, y) = free[pick];
        items.push(SceneItem {
            sprite: sprites.pet(species),
            x,
            y,
            w: 1,
            d: 1,
            layer: SceneLayer::Pet,
            flipped: (view.seed + i as u64).is_multiple_of(2),
            selected: false,
        });
    }
    let scene = RoomScene {
        width: view.width,
        height: view.height,
        wall_rows: WALL_ROWS,
        wallpaper: sprites.wallpaper(&view.room.wallpaper),
        floor: sprites.floor(&view.room.floor),
        items,
        cursor: view.cursor,
        grid: view.edit,
        scale: 4,
    };
    let img = render_room(&scene);
    let mut png = IoCursor::new(Vec::new());
    img.write_to(&mut png, image::ImageFormat::Png)?;
    Ok(png.into_inner())
}

/// An item sprite scaled by the largest whole factor that fits `max_w`×`max_h`, as PNG.
pub fn slot_sprite(sprites: &Sprites, content: &Content, item_id: &str, max_w: u32, max_h: u32) -> (Vec<u8>, u32, u32) {
    let sprite = sprites
        .item(content, item_id)
        .unwrap_or_else(|| Arc::new(RgbaImage::from_pixel(8, 8, image::Rgba([0x8f, 0xa0, 0xd8, 255]))));
    let factor = (max_w / sprite.width().max(1))
        .min(max_h / sprite.height().max(1))
        .clamp(1, 8);
    let (w, h) = (sprite.width() * factor, sprite.height() * factor);
    let big = image::imageops::resize(sprite.as_ref(), w, h, image::imageops::FilterType::Nearest);
    let mut png = IoCursor::new(Vec::new());
    let _ = big.write_to(&mut png, image::ImageFormat::Png);
    (png.into_inner(), w, h)
}
