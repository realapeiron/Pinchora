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
    sprites::{SpriteStore, placeholder_item, placeholder_tile, recolor, variant_ramp},
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
}

pub fn render_view(sprites: &Sprites, view: &RoomView) -> anyhow::Result<Vec<u8>> {
    let content = Content::get();
    let items = view
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
