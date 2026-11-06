use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
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
    Backdrop, BadgeIcon, ShowcaseSlot,
    character::render_character,
    encode_png,
    room::{Cursor, RoomScene, SceneItem, SceneLayer, render_room},
    sprites::{SpriteStore, placeholder_item, placeholder_pet, placeholder_tile, recolor, sleeping_pet, variant_ramp},
};

/// How much bigger than the card's 2× a badge's art is drawn for an unlock notice's thumbnail.
const BADGE_LARGE_SCALE: u32 = 4;

/// Recoloured characters kept at once; players can pick far more combinations than this.
const CHARACTER_CACHE: usize = 512;

/// Showcase tiles kept at once: fish, pets, furniture and house thumbnails.
const TILE_CACHE: usize = 512;

/// A sprite scaled up whole and encoded once, ready to go into a card.
pub struct ScaledArt {
    pub png: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

/// Sprites by name, with palette variants and placeholders made on first use.
pub struct Sprites {
    /// The atlas the character renderer uses too, decoded once for the process.
    store: &'static SpriteStore,
    made: Mutex<HashMap<String, Arc<RgbaImage>>>,
    /// Background and badge art, scaled for cards; `None` while the art is not drawn yet.
    scaled: Mutex<HashMap<String, Option<Arc<ScaledArt>>>>,
    /// Characters by saved code; emptied when it outgrows [`CHARACTER_CACHE`].
    characters: Mutex<HashMap<String, Arc<RgbaImage>>>,
    /// The art of profile showcase tiles by what they show (`fish:<id>`, `house:<hash of the room>`);
    /// emptied when it outgrows [`TILE_CACHE`].
    tiles: Mutex<HashMap<String, Arc<ScaledArt>>>,
}

impl Default for Sprites {
    fn default() -> Self {
        Self {
            store: pinchora_render::character::atlas(),
            made: Mutex::new(HashMap::new()),
            scaled: Mutex::new(HashMap::new()),
            characters: Mutex::new(HashMap::new()),
            tiles: Mutex::new(HashMap::new()),
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

    /// The atlas sprite `name` as drawn, or `None` while it is not drawn yet.
    pub fn atlas(&self, name: &str) -> Option<Arc<RgbaImage>> {
        self.store.get(name)
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

    /// A pet lying asleep with a "z z" above it, facing left or right.
    pub fn sleeping_pet(&self, species: &str, left: bool) -> Arc<RgbaImage> {
        self.cached(&format!("ps_{species}_{left}"), || {
            sleeping_pet(&self.pet(species), left)
        })
    }

    /// The character as `render_character` draws it at scale 1: its look in its colours with
    /// its accessories on, the feet on the bottom row and the look centred.
    pub fn character(&self, character: &Character) -> Arc<RgbaImage> {
        let code = character.encode();
        if let Some(s) = self.characters.lock().expect("character cache").get(&code) {
            return s.clone();
        }
        let sprite = Arc::new(render_character(&character.spec(), 1));
        let mut cache = self.characters.lock().expect("character cache");
        if cache.len() >= CHARACTER_CACHE {
            cache.clear();
        }
        cache.insert(code, sprite.clone());
        sprite
    }

    /// The tile art kept under `key`, made by `make` the first time. A failure is not kept.
    fn tile(&self, key: String, make: impl FnOnce() -> anyhow::Result<(Vec<u8>, u32, u32)>) -> Option<Arc<ScaledArt>> {
        if let Some(art) = self.tiles.lock().expect("tile cache").get(&key) {
            return Some(art.clone());
        }
        let (png, w, h) = make().ok()?;
        let art = Arc::new(ScaledArt { png, w, h });
        let mut cache = self.tiles.lock().expect("tile cache");
        if cache.len() >= TILE_CACHE {
            cache.clear();
        }
        cache.insert(key, art.clone());
        Some(art)
    }

    /// A fish's picture for a showcase tile.
    pub fn showcase_fish(&self, id: &str) -> Option<Arc<ScaledArt>> {
        self.tile(format!("fish:{id}"), || ShowcaseSlot::fit_sprite(&self.fish(id)))
    }

    /// A pet species' picture for a showcase tile.
    pub fn showcase_pet(&self, species: &str) -> Option<Arc<ScaledArt>> {
        self.tile(format!("pet:{species}"), || {
            ShowcaseSlot::fit_sprite(&self.pet(species))
        })
    }

    /// A furniture item's picture for a showcase tile, in the colour of its variant.
    pub fn showcase_item(&self, content: &Content, item_id: &str) -> Option<Arc<ScaledArt>> {
        let sprite = self.item(content, item_id)?;
        self.tile(format!("item:{item_id}"), || ShowcaseSlot::fit_sprite(&sprite))
    }

    /// A thumbnail of the room as `view` draws it, without pets or characters. Rooms are kept by
    /// what is in them, so a profile is not drawn again until its owner changes something.
    pub fn house_thumbnail(&self, view: &RoomView) -> Option<Arc<ScaledArt>> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (view.width, view.depth, &view.room.wallpaper, &view.room.floor).hash(&mut hasher);
        let mut placements: Vec<_> = view
            .placements
            .iter()
            .map(|p| (p.id, &p.item_id, p.x, p.y, p.flipped))
            .collect();
        placements.sort();
        placements.hash(&mut hasher);
        self.tile(format!("house:{:016x}", hasher.finish()), || {
            ShowcaseSlot::fit_house(&room_image(self, view, 1))
        })
    }

    /// The atlas sprite `name` scaled `factor`×, or `None` while it is not drawn yet.
    fn scaled_art(&self, name: String, factor: u32) -> Option<Arc<ScaledArt>> {
        self.scaled_art_as(&name, name.clone(), factor)
    }

    /// Like [`Sprites::scaled_art`], kept under `key` so one sprite can be scaled more than one way.
    fn scaled_art_as(&self, name: &str, key: String, factor: u32) -> Option<Arc<ScaledArt>> {
        if let Some(art) = self.scaled.lock().expect("scaled cache").get(&key) {
            return art.clone();
        }
        let art = self.store.get(name).and_then(|sprite| {
            let (w, h) = (sprite.width() * factor, sprite.height() * factor);
            let big = image::imageops::resize(sprite.as_ref(), w, h, image::imageops::FilterType::Nearest);
            encode_png(&big).ok().map(|png| Arc::new(ScaledArt { png, w, h }))
        });
        self.scaled.lock().expect("scaled cache").insert(key, art.clone());
        art
    }

    /// A profile background's art (`bg_<id>`, 240×135) scaled to 960×540.
    pub fn background(&self, id: &str) -> Option<Arc<ScaledArt>> {
        self.scaled_art(format!("bg_{id}"), Backdrop::SCALE)
    }

    /// A badge's art (`b_<id>`, 24 px wide) scaled to 48 px.
    pub fn badge(&self, id: &str) -> Option<Arc<ScaledArt>> {
        self.scaled_art(format!("b_{id}"), BadgeIcon::SCALE)
    }

    /// A badge's art scaled to 96 px, for the thumbnail of an unlock notice.
    pub fn badge_large(&self, id: &str) -> Option<Arc<ScaledArt>> {
        self.scaled_art_as(&format!("b_{id}"), format!("b4_{id}"), BADGE_LARGE_SCALE)
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

/// A pet in a room: awake it wanders from tile to tile, asleep it lies still on one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomPet {
    pub species: String,
    pub asleep: bool,
}

impl RoomPet {
    pub fn awake(species: impl Into<String>) -> Self {
        Self {
            species: species.into(),
            asleep: false,
        }
    }

    pub fn asleep(species: impl Into<String>) -> Self {
        Self {
            species: species.into(),
            asleep: true,
        }
    }

    /// The same number for the same species on every draw: where a sleeping pet lies and which
    /// way, so it does not move with the hour as the wanderers do.
    fn key(&self) -> u64 {
        self.species.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        })
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
    /// Pets to show in the room, wandering or asleep (owner's and a visitor's).
    pub pets: Vec<RoomPet>,
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
    pinchora_render::encode_png(&room_image(sprites, view, 2))
}

/// The room as a picture, each art pixel `scale` pixels wide.
fn room_image(sprites: &Sprites, view: &RoomView, scale: u32) -> RgbaImage {
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
    // Sleepers take their tiles first, each on a spot that stays the same.
    for pet in view.pets.iter().filter(|p| p.asleep) {
        if free.is_empty() {
            break;
        }
        let key = pet.key();
        let (x, y) = free.swap_remove((key >> 16) as usize % free.len());
        items.push(SceneItem {
            sprite: sprites.sleeping_pet(&pet.species, key & 1 == 0),
            x,
            y,
            w: 1,
            d: 1,
            layer: SceneLayer::Figure,
            // Drawn already turned the way it lies: a mirrored "z" is not a "z".
            flipped: false,
            selected: false,
            lift: 0,
        });
    }
    let figures = view
        .pets
        .iter()
        .filter(|p| !p.asleep)
        .map(|p| sprites.pet(&p.species))
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
        scale,
    };
    render_room(&scene)
}

/// A sprite scaled up by a whole `factor` with nearest-neighbour, as PNG with its new size.
pub fn scaled_png(sprite: &RgbaImage, factor: u32) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let (w, h) = (sprite.width() * factor, sprite.height() * factor);
    let big = image::imageops::resize(sprite, w, h, image::imageops::FilterType::Nearest);
    let mut png = IoCursor::new(Vec::new());
    big.write_to(&mut png, image::ImageFormat::Png)?;
    Ok((png.into_inner(), w, h))
}

/// An item sprite scaled by the largest whole factor that fits `max_w`×`max_h`, as PNG. A sprite that
/// is bigger than the box even at its own size (a tall headboard, a locker) is shrunk to fit instead
/// of poking out of its slot.
pub fn slot_sprite(sprites: &Sprites, content: &Content, item_id: &str, max_w: u32, max_h: u32) -> (Vec<u8>, u32, u32) {
    let sprite = sprites
        .item(content, item_id)
        .unwrap_or_else(|| Arc::new(RgbaImage::from_pixel(8, 8, image::Rgba([0x8f, 0xa0, 0xd8, 255]))));
    let (sw, sh) = (sprite.width().max(1), sprite.height().max(1));
    let (w, h) = if sw > max_w || sh > max_h {
        let fit = (max_w as f32 / sw as f32).min(max_h as f32 / sh as f32);
        (
            ((sw as f32 * fit) as u32).clamp(1, max_w),
            ((sh as f32 * fit) as u32).clamp(1, max_h),
        )
    } else {
        let factor = (max_w / sw).min(max_h / sh).clamp(1, 8);
        (sw * factor, sh * factor)
    };
    let big = image::imageops::resize(sprite.as_ref(), w, h, image::imageops::FilterType::Nearest);
    let mut png = IoCursor::new(Vec::new());
    let _ = big.write_to(&mut png, image::ImageFormat::Png);
    (png.into_inner(), w, h)
}

#[cfg(test)]
mod tests {
    use pinchora_render::character::{Paint, Slot};

    use super::*;

    fn character(content: &Content, skin: u8, top: u8) -> Character {
        let mut c = Character::starter(content);
        c.colors.skin = Paint::Swatch(skin);
        c.colors.top = Paint::Swatch(top);
        c
    }

    #[test]
    fn no_slot_sprite_pokes_out_of_its_box() {
        let (sprites, content) = (Sprites::default(), Content::get());
        let (bw, bh) = pinchora_render::ItemGridCard::SPRITE_BOX;
        let mut seen = std::collections::HashSet::new();
        for item in content.items().filter(|i| seen.insert(i.base.id.clone())) {
            let (_, w, h) = slot_sprite(&sprites, content, &item.id(), bw, bh);
            assert!(
                w <= bw && h <= bh && w > 0 && h > 0,
                "{} is drawn {w}x{h} in a {bw}x{bh} box",
                item.id()
            );
        }
    }

    #[test]
    fn characters_are_recoloured_and_kept() {
        let content = Content::get();
        let sprites = Sprites::default();
        let (a, b) = (character(content, 1, 0), character(content, 5, 4));
        let (art_a, art_b) = (sprites.character(&a), sprites.character(&b));
        assert_ne!(art_a.as_ref(), art_b.as_ref(), "different colours, different pixels");
        assert!(Arc::ptr_eq(&art_a, &sprites.character(&a)), "the second call is cached");
        let mut hatted = a.clone();
        hatted.wear(Slot::Head, Some("top_hat"), None);
        let art_hat = sprites.character(&hatted);
        assert_ne!(art_hat.as_ref(), art_a.as_ref(), "a top hat is on the head");
        let (png, w, h) = scaled_png(&art_a, 3).expect("encodes");
        assert_eq!((w, h), (art_a.width() * 3, art_a.height() * 3));
        assert!(png.starts_with(b"\x89PNG"));
    }

    #[test]
    fn card_art_is_scaled_once_and_missing_art_is_none() {
        let content = Content::get();
        let sprites = Sprites::default();
        assert!(sprites.background("no_such_background").is_none());
        assert!(sprites.badge("no_such_badge").is_none());
        // Real art, once it is in the atlas: backgrounds fill a 960×540 card, badges are 48 px wide.
        for b in &content.backgrounds {
            if let Some(art) = sprites.background(&b.id) {
                assert_eq!((art.w, art.h), (960, 540), "{}", b.id);
                assert!(art.png.starts_with(b"\x89PNG"));
                assert!(Arc::ptr_eq(&art, &sprites.background(&b.id).expect("cached")));
            }
        }
        for b in &content.badges {
            if let Some(art) = sprites.badge(&b.id) {
                // Taller badges shrink to fit the 48 px slot when drawn.
                assert_eq!(art.w, 48, "{}", b.id);
            }
        }
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
            pets: vec![RoomPet::awake(pet.clone())],
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

    #[test]
    fn a_sleeping_pet_lies_still_while_the_wanderers_move() {
        let content = Content::get();
        let sprites = Sprites::default();
        let (sleeper, walker) = (content.pets[0].id.clone(), content.pets[1].id.clone());
        let view = |seed: u64, pets: Vec<RoomPet>| RoomView {
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
            pets,
            characters: Vec::new(),
            seed,
        };
        let draw = |seed: u64, pets: Vec<RoomPet>| render_view(&sprites, &view(seed, pets)).expect("renders");
        let asleep = |species: &str| vec![RoomPet::asleep(species)];
        assert_eq!(
            draw(1, asleep(&sleeper)),
            draw(2, asleep(&sleeper)),
            "a sleeper does not move with the hour"
        );
        assert_ne!(
            draw(1, asleep(&sleeper)),
            draw(1, vec![RoomPet::awake(sleeper.clone())]),
            "asleep is drawn differently from awake"
        );
        let mixed = |seed| {
            draw(
                seed,
                vec![RoomPet::asleep(sleeper.clone()), RoomPet::awake(walker.clone())],
            )
        };
        assert_ne!(mixed(1), mixed(2), "the awake one still wanders");
        // The lying sprite is cached per species and side, and is not the standing one.
        let lying = sprites.sleeping_pet(&sleeper, true);
        assert!(Arc::ptr_eq(&lying, &sprites.sleeping_pet(&sleeper, true)));
        assert_ne!(lying.as_ref(), sprites.pet(&sleeper).as_ref());
        assert_ne!(lying.as_ref(), sprites.sleeping_pet(&sleeper, false).as_ref());
    }

    #[test]
    fn showcase_art_is_fitted_and_kept_and_a_room_is_drawn_again_only_when_it_changes() {
        let content = Content::get();
        let sprites = Sprites::default();
        let fish = sprites.showcase_fish(&content.fish[0].id).expect("a fish");
        let (box_w, box_h) = ShowcaseSlot::ART_BOX;
        assert!(fish.w <= box_w && fish.h <= box_h && fish.png.starts_with(b"\x89PNG"));
        assert!(Arc::ptr_eq(
            &fish,
            &sprites.showcase_fish(&content.fish[0].id).expect("kept")
        ));
        let pet = sprites.showcase_pet(&content.pets[0].id).expect("a pet");
        assert!(pet.w <= box_w && pet.h <= box_h);
        let sofa = content.items().find(|i| i.base.id.contains("sofa")).expect("a sofa");
        let art = sprites.showcase_item(content, &sofa.id()).expect("a sofa's picture");
        assert!(
            art.w <= box_w && art.h <= box_h,
            "a big sofa is shrunk to the box: {}x{}",
            art.w,
            art.h
        );
        assert!(sprites.showcase_item(content, "no_such_item").is_none());

        let room = |placements: Vec<PlacementRow>| RoomView {
            width: 8,
            depth: 8,
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
            pets: vec![RoomPet::awake(content.pets[0].id.clone())],
            characters: Vec::new(),
            seed: 1,
        };
        let placement = |x: i16| PlacementRow {
            id: 1,
            room_idx: 0,
            item_id: sofa.id(),
            x,
            y: 2,
            flipped: false,
        };
        let first = sprites.house_thumbnail(&room(vec![placement(1)])).expect("a thumbnail");
        assert_eq!((first.w, first.h), ShowcaseSlot::HOUSE_BOX);
        let again = sprites.house_thumbnail(&room(vec![placement(1)])).expect("kept");
        assert!(Arc::ptr_eq(&first, &again), "the same room is not drawn twice");
        let mut other_seed = room(vec![placement(1)]);
        other_seed.seed = 99;
        other_seed.pets.clear();
        assert!(
            Arc::ptr_eq(&first, &sprites.house_thumbnail(&other_seed).expect("kept")),
            "pets and the hour do not change the thumbnail"
        );
        let moved = sprites.house_thumbnail(&room(vec![placement(4)])).expect("a thumbnail");
        assert!(!Arc::ptr_eq(&first, &moved), "moving a sofa draws the room again");
        assert_ne!(first.png, moved.png);
        let empty = sprites.house_thumbnail(&room(Vec::new())).expect("a thumbnail");
        assert_ne!(first.png, empty.png);
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
            pets: vec![RoomPet::awake(content.pets[0].id.clone())],
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
