//! The house as the web dashboard shows it: the same rooms, pets and characters `/house view`
//! draws, handed over as data the browser can draw itself, plus the bot's own picture of the room
//! for anyone who wants to compare. Nothing here writes: looking at a house never moves anyone in.

use chrono::{DateTime, Utc};
use pinchora_db::{HouseRow, PlacementRow, RoomRow};
use pinchora_game::{
    Content, Layer,
    character::Character,
    house::{self, TIERS},
};
use pinchora_render::{
    encode_png,
    sprites::{BASE_RAMP, variant_ramp},
};
use serde_json::{Value, json};

use crate::{
    App,
    commands::house::home_pets,
    game::scene::{RoomPet, RoomView, room_image},
};

/// Seconds the pets of a room keep their tiles; the same window `/house view` uses.
const SEED_WINDOW: i64 = 600;

/// What varies where the pets and characters stand: the ten-minute window `now` is in.
pub fn seed_at(now: DateTime<Utc>) -> u64 {
    (now.timestamp() / SEED_WINDOW) as u64
}

/// One room with everything standing in it, as the browser needs to draw it.
pub struct RoomData {
    pub width: u32,
    pub depth: u32,
    pub room: RoomRow,
    pub placements: Vec<PlacementRow>,
    pub pets: Vec<RoomPet>,
    pub characters: Vec<Character>,
    pub seed: u64,
}

impl RoomData {
    /// The room of `owner` at `idx`, with their pets (asleep ones lying down) and their character.
    /// `None` when the house has no such room.
    pub async fn load(
        app: &App,
        owner: u64,
        house: &HouseRow,
        rooms: &[RoomRow],
        idx: i16,
    ) -> anyhow::Result<Option<Self>> {
        let Some(room) = rooms.iter().find(|r| r.idx == idx).cloned() else {
            return Ok(None);
        };
        let tier = house::tier(house.tier);
        let placements = app.db.placements(owner, idx).await?;
        let now = app.now();
        let pets = home_pets(app.db.pets(owner).await?, now);
        let content = Content::get();
        let characters = app
            .db
            .character(owner)
            .await?
            .and_then(|code| Character::decode(content, &code))
            .into_iter()
            .collect();
        Ok(Some(Self {
            width: tier.width,
            depth: tier.depth,
            room,
            placements,
            pets,
            characters,
            seed: seed_at(now),
        }))
    }

    /// The scene `/house view` would draw for this room.
    pub fn view(&self) -> RoomView {
        RoomView {
            width: self.width,
            depth: self.depth,
            room: self.room.clone(),
            placements: self.placements.clone(),
            cursor: None,
            selected: None,
            edit: false,
            pets: self.pets.clone(),
            characters: self.characters.clone(),
            seed: self.seed,
        }
    }

    /// The room as JSON. Ids are strings, as everywhere in the dashboard API.
    pub fn json(&self) -> Value {
        json!({
            "idx": self.room.idx,
            "room_type": self.room.room_type,
            "width": self.width,
            "depth": self.depth,
            "wallpaper": self.room.wallpaper,
            "floor": self.room.floor,
            "placements": self.placements.iter().map(|p| json!({
                "id": p.id.to_string(),
                "item_id": p.item_id,
                "x": p.x,
                "y": p.y,
                "flipped": p.flipped,
            })).collect::<Vec<_>>(),
            "pets": self.pets.iter().map(|p| json!({ "species": p.species, "asleep": p.asleep })).collect::<Vec<_>>(),
            "characters": self.characters.iter().map(Character::encode).collect::<Vec<_>>(),
            "seed": self.seed,
        })
    }
}

/// The room as the bot draws it in Discord: a PNG, each art pixel `scale` pixels wide.
pub async fn render_png(app: &App, view: RoomView, scale: u32) -> anyhow::Result<Vec<u8>> {
    let sprites = app.sprites.clone();
    app.render(move |_| encode_png(&room_image(&sprites, &view, scale)))
        .await
}

/// A character's sprite as `render_character` draws it at scale 1, for the code the dashboard
/// hands out in [`RoomData::json`]. `None` when the code is not a character.
pub async fn character_png(app: &App, code: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(character) = Character::decode(Content::get(), code) else {
        return Ok(None);
    };
    let sprites = app.sprites.clone();
    let png = app.render(move |_| encode_png(&sprites.character(&character))).await?;
    Ok(Some(png))
}

fn layer_key(layer: Layer) -> &'static str {
    match layer {
        Layer::Floor => "floor",
        Layer::Rug => "rug",
        Layer::Wall => "wall",
        Layer::Surface => "surface",
    }
}

fn ramp_json(ramp: &[[u8; 3]; 5]) -> Value {
    json!(ramp)
}

/// Everything the browser needs to know about the catalogue to place and name what it draws:
/// furniture with footprints and variants, the surfaces, the pets, the tiers and the palette ramps
/// the variants recolour with. Built from the same [`Content`] the bot runs on.
pub fn catalog() -> Value {
    let content = Content::get();
    let furniture: Vec<Value> = content
        .furniture
        .iter()
        .map(|f| {
            json!({
                "id": f.id,
                "name": f.name,
                "category": f.category.key(),
                "layer": layer_key(f.layer),
                "footprint": f.footprint,
                "rarity": f.rarity.label(),
                "iso_width": f.iso_width,
                "surface_height": f.surface_height,
                "tags": f.tags,
                "variants": f.variants,
            })
        })
        .collect();
    let named = |list: &[pinchora_game::Surface]| -> Vec<Value> {
        list.iter().map(|s| json!({ "id": s.id, "name": s.name })).collect()
    };
    let mut ramps = serde_json::Map::new();
    for variant in content.furniture.iter().flat_map(|f| &f.variants) {
        if let Some(ramp) = variant_ramp(variant) {
            ramps.insert(variant.clone(), ramp_json(&ramp));
        }
    }
    json!({
        "furniture": furniture,
        "wallpapers": named(&content.surfaces.wallpapers),
        "floors": named(&content.surfaces.floors),
        "room_types": content.rooms.iter().map(|r| json!({ "id": r.id, "name": r.name })).collect::<Vec<_>>(),
        "pets": content.pets.iter().map(|p| json!({ "id": p.id, "name": p.name })).collect::<Vec<_>>(),
        "tiers": TIERS.iter().map(|t| json!({
            "tier": t.tier,
            "name": t.name,
            "rooms": t.rooms,
            "width": t.width,
            "depth": t.depth,
        })).collect::<Vec<_>>(),
        "base_ramp": ramp_json(&BASE_RAMP),
        "variant_ramps": ramps,
    })
}

/// The name of a room type, or its id when the catalogue has no such room.
pub fn room_name(room: &RoomRow) -> String {
    Content::get()
        .room_type(&room.room_type)
        .map_or_else(|| room.room_type.clone(), |r| r.name.clone())
}

#[cfg(test)]
mod fixtures;
