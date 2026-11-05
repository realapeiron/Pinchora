use std::{
    collections::HashMap,
    io::Cursor as IoCursor,
    sync::{Arc, Mutex},
};

use image::RgbaImage;
use pinchora_db::{PlacementRow, RoomRow};
use pinchora_game::{
    Content, Layer,
    character::Character,
    house::{self, Placed},
};
use pinchora_render::{
    character::{placeholder_character, recolor_character},
    room::{Cursor, RoomScene, SceneItem, SceneLayer, render_room},
    sprites::{SpriteStore, placeholder_item, placeholder_pet, placeholder_tile, recolor, variant_ramp},
};

/// Recoloured characters kept at once; players can pick far more combinations than this.
const CHARACTER_CACHE: usize = 512;

/// Sprites by name, with palette variants and placeholders made on first use.
pub struct Sprites {
    store: SpriteStore,
    made: Mutex<HashMap<String, Arc<RgbaImage>>>,
    /// Characters by saved code; emptied when it outgrows [`CHARACTER_CACHE`].
    characters: Mutex<HashMap<String, Arc<RgbaImage>>>,
}

impl Default for Sprites {
    fn default() -> Self {
        Self {
            store: SpriteStore::embedded(),
            made: Mutex::new(HashMap::new()),
            characters: Mutex::new(HashMap::new()),
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
            let sprite =
                drawn.unwrap_or_else(|| placeholder_item(&base.id, w, d, scene_layer(base.layer), base.iso_width));
            match item.variant.and_then(variant_ramp) {
                Some(ramp) => recolor(&sprite, &ramp),
                None => sprite,
            }
        }))
    }

    pub fn pet(&self, species: &str) -> Arc<RgbaImage> {
        self.cached(&format!("p_{species}"), || {
            let sprite = self
                .store
                .get(&format!("p_{species}"))
                .map_or_else(|| placeholder_pet(species), |s| (*s).clone());
            // Art drawn for the old 16 px tiles: doubled so it matches the 64 px rooms.
            if sprite.width() < 28 {
                let (w, h) = (sprite.width() * 2, sprite.height() * 2);
                image::imageops::resize(&sprite, w, h, image::imageops::FilterType::Nearest)
            } else {
                sprite
            }
        })
    }

    /// The character's look in its colours: the atlas sprite `a_<look>`, or a placeholder
    /// figure until that art exists.
    pub fn character(&self, character: &Character) -> Arc<RgbaImage> {
        let code = character.encode();
        if let Some(s) = self.characters.lock().expect("character cache").get(&code) {
            return s.clone();
        }
        let look = self
            .store
            .get(&format!("a_{}", character.look))
            .map_or_else(placeholder_character, |s| (*s).clone());
        let sprite = Arc::new(recolor_character(&look, character.colors));
        let mut cache = self.characters.lock().expect("character cache");
        if cache.len() >= CHARACTER_CACHE {
            cache.clear();
        }
        cache.insert(code, sprite.clone());
        sprite
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
    pub depth: u32,
    pub room: RoomRow,
    pub placements: Vec<PlacementRow>,
    pub cursor: Option<Cursor>,
    /// Placement under the cursor, drawn highlighted.
    pub selected: Option<i64>,
    pub edit: bool,
    /// Pet species to show wandering in the room (owner's and a visitor's).
    pub pets: Vec<String>,
    /// Characters standing in the room (owner's and a visitor's).
    pub characters: Vec<Character>,
    /// Varies where pets and characters stand, e.g. the current hour.
    pub seed: u64,
}

fn placed(view: &RoomView) -> Vec<Placed<'_>> {
    view.placements
        .iter()
        .map(|p| Placed {
            item_id: &p.item_id,
            x: p.x as u32,
            y: p.y as u32,
            flipped: p.flipped,
        })
        .collect()
}

/// Free floor tiles, so pets and characters never stand inside furniture.
fn free_tiles(content: &Content, view: &RoomView) -> Vec<(u32, u32)> {
    let mut taken = vec![false; (view.width * view.depth) as usize];
    for p in &view.placements {
        if let Some((layer, r)) = house::footprint(content, &p.item_id, p.x as u32, p.y as u32, p.flipped)
            && layer == Layer::Floor
        {
            for y in r.y..(r.y + r.d).min(view.depth) {
                for x in r.x..(r.x + r.w).min(view.width) {
                    taken[(y * view.width + x) as usize] = true;
                }
            }
        }
    }
    (0..view.depth)
        .flat_map(|y| (0..view.width).map(move |x| (x, y)))
        .filter(|&(x, y)| !taken[(y * view.width + x) as usize])
        .collect()
}

pub fn render_view(sprites: &Sprites, view: &RoomView) -> anyhow::Result<Vec<u8>> {
    let content = Content::get();
    let placed = placed(view);
    let mut items: Vec<SceneItem> = view
        .placements
        .iter()
        .filter_map(|p| {
            let item = content.item(&p.item_id)?;
            let (layer, r) = house::footprint(content, &p.item_id, p.x as u32, p.y as u32, p.flipped)?;
            Some(SceneItem {
                sprite: sprites.item(content, &p.item_id)?,
                x: r.x,
                y: r.y,
                w: r.w,
                d: r.d,
                layer: scene_layer(item.base.layer),
                flipped: p.flipped,
                selected: view.selected == Some(p.id),
                lift: if layer == Layer::Surface {
                    house::surface_lift(content, &placed, r.x, r.y)
                } else {
                    0
                },
            })
        })
        .collect();
    let mut free = free_tiles(content, view);
    let figures = view
        .pets
        .iter()
        .map(|species| sprites.pet(species))
        .chain(view.characters.iter().map(|c| sprites.character(c)));
    for (i, sprite) in figures.enumerate() {
        if free.is_empty() {
            break;
        }
        let pick = (view
            .seed
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .wrapping_add(i as u64 * 7919)
            >> 16) as usize
            % free.len();
        // Each figure takes its own tile.
        let (x, y) = free.swap_remove(pick);
        items.push(SceneItem {
            sprite,
            x,
            y,
            w: 1,
            d: 1,
            layer: SceneLayer::Figure,
            flipped: (view.seed + i as u64).is_multiple_of(2),
            selected: false,
            lift: 0,
        });
    }
    let scene = RoomScene {
        width: view.width,
        depth: view.depth,
        wallpaper: sprites.wallpaper(&view.room.wallpaper),
        floor: sprites.floor(&view.room.floor),
        items,
        cursor: view.cursor,
        grid: view.edit,
        scale: 2,
    };
    pinchora_render::encode_png(&render_room(&scene))
}

/// A sprite scaled up by a whole `factor` with nearest-neighbour, as PNG with its new size.
pub fn scaled_png(sprite: &RgbaImage, factor: u32) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let (w, h) = (sprite.width() * factor, sprite.height() * factor);
    let big = image::imageops::resize(sprite, w, h, image::imageops::FilterType::Nearest);
    let mut png = IoCursor::new(Vec::new());
    big.write_to(&mut png, image::ImageFormat::Png)?;
    Ok((png.into_inner(), w, h))
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

#[cfg(test)]
mod tests {
    use pinchora_render::character::Colors;

    use super::*;

    fn character(content: &Content, skin: usize, top: usize) -> Character {
        let mut c = Character::starter(content);
        c.colors = Colors { skin, top, ..c.colors };
        c
    }

    #[test]
    fn characters_are_recoloured_and_kept() {
        let content = Content::get();
        let sprites = Sprites::default();
        let (a, b) = (character(content, 1, 0), character(content, 5, 4));
        let (art_a, art_b) = (sprites.character(&a), sprites.character(&b));
        assert_ne!(art_a.as_ref(), art_b.as_ref(), "different colours, different pixels");
        assert!(Arc::ptr_eq(&art_a, &sprites.character(&a)), "the second call is cached");
        let (png, w, h) = scaled_png(&art_a, 3).expect("encodes");
        assert_eq!((w, h), (art_a.width() * 3, art_a.height() * 3));
        assert!(png.starts_with(b"\x89PNG"));
    }

    #[test]
    fn rooms_draw_pets_and_characters() {
        let content = Content::get();
        let pet = content.pets[0].id.clone();
        let view = |characters: Vec<Character>| RoomView {
            width: 8,
            depth: 8,
            room: RoomRow {
                idx: 0,
                room_type: "living".to_owned(),
                wallpaper: content.surfaces.wallpapers[0].id.clone(),
                floor: content.surfaces.floors[0].id.clone(),
            },
            placements: Vec::new(),
            cursor: None,
            selected: None,
            edit: false,
            pets: vec![pet.clone()],
            characters,
            seed: 7,
        };
        let sprites = Sprites::default();
        let bare = render_view(&sprites, &view(Vec::new())).expect("renders");
        let people = render_view(
            &sprites,
            &view(vec![character(content, 1, 0), character(content, 5, 4)]),
        )
        .expect("renders");
        assert!(bare.starts_with(b"\x89PNG") && people.starts_with(b"\x89PNG"));
        assert_ne!(bare, people, "the characters are in the picture");
    }

    /// Renders a furnished room with real catalog items to check the art in place:
    /// `cargo test -p pinchora-commands preview_room -- --ignored`, then open
    /// `target/room-preview.png`.
    #[test]
    #[ignore]
    fn preview_room() {
        let content = Content::get();
        let items: [(&str, i16, i16, bool); 21] = [
            ("pebblepane_window__sand", 1, 0, false),
            ("hemhill_painting__cream", 4, 0, false),
            ("pebblehour_clock__navy", 7, 0, false),
            ("dunearc_window__coral", 2, 0, true),
            ("stillframe_door__cream", 6, 1, true),
            ("stillpanel_wardrobe__cream", 0, 0, false),
            ("stillstep_nightstand__cream", 6, 0, false),
            ("pebblecap_lamp__sand", 6, 0, false),
            ("patchpillow_bed__cream", 7, 0, false),
            ("humskein_upright__plum", 0, 3, true),
            ("rivetgrid_rug__sand", 2, 4, false),
            ("crumbcorner_table__moss", 3, 4, false),
            ("pagepair_books__moss", 3, 4, false),
            ("sprigcup_fern__moss", 4, 5, false),
            ("plainwhorl_chair__sand", 5, 4, false),
            ("plainwhorl_chair__sand", 3, 6, true),
            ("crumbrail_bench__sand", 6, 7, false),
            ("stillstem_rubber_tree__cream", 0, 7, false),
            ("rivetarm_floor_lamp__charcoal", 9, 4, false),
            ("blinkslot_cabinet__plum", 8, 7, false),
            ("duneline_runner__coral", 6, 3, false),
        ];
        let placements = items
            .iter()
            .enumerate()
            .map(|(i, &(item_id, x, y, flipped))| PlacementRow {
                id: i as i64,
                room_idx: 0,
                item_id: item_id.to_owned(),
                x,
                y,
                flipped,
            })
            .collect();
        let view = RoomView {
            width: 10,
            depth: 9,
            room: RoomRow {
                idx: 0,
                room_type: "living".to_owned(),
                wallpaper: content.surfaces.wallpapers[0].id.clone(),
                floor: content.surfaces.floors[0].id.clone(),
            },
            placements,
            cursor: None,
            selected: None,
            edit: false,
            pets: vec![content.pets[0].id.clone()],
            characters: vec![character(content, 2, 1)],
            seed: 3,
        };
        let png = render_view(&Sprites::default(), &view).expect("renders");
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/room-preview.png"),
            png,
        )
        .unwrap();
    }
}
