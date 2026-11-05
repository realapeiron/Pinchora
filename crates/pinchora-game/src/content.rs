use std::{collections::HashMap, sync::LazyLock};

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    pub const ALL: [Rarity; 5] = [Self::Common, Self::Uncommon, Self::Rare, Self::Epic, Self::Legendary];

    pub fn label(self) -> &'static str {
        match self {
            Self::Common => "Common",
            Self::Uncommon => "Uncommon",
            Self::Rare => "Rare",
            Self::Epic => "Epic",
            Self::Legendary => "Legendary",
        }
    }

    /// Frame colour on cards.
    pub fn color(self) -> u32 {
        match self {
            Self::Common => 0x8fa0d8,
            Self::Uncommon => 0x5bbf6a,
            Self::Rare => 0x3a9bdc,
            Self::Epic => 0x9b6bc4,
            Self::Legendary => 0xffd166,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Currency {
    Shells,
    Pearls,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Seating,
    Tables,
    Beds,
    Storage,
    Lighting,
    Kitchen,
    Bathroom,
    Electronics,
    Rugs,
    WallDecor,
    WindowsDoors,
    Plants,
    PetItems,
    ToysGames,
    Music,
    Outdoor,
    Seasonal,
    Office,
    Cafe,
    Hobbies,
    Fitness,
    Partitions,
    Nautical,
}

impl Category {
    pub const ALL: [Category; 23] = [
        Self::Seating,
        Self::Tables,
        Self::Beds,
        Self::Storage,
        Self::Lighting,
        Self::Kitchen,
        Self::Bathroom,
        Self::Electronics,
        Self::Rugs,
        Self::WallDecor,
        Self::WindowsDoors,
        Self::Plants,
        Self::PetItems,
        Self::ToysGames,
        Self::Music,
        Self::Outdoor,
        Self::Seasonal,
        Self::Office,
        Self::Cafe,
        Self::Hobbies,
        Self::Fitness,
        Self::Partitions,
        Self::Nautical,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Seating => "Seating",
            Self::Tables => "Tables",
            Self::Beds => "Beds",
            Self::Storage => "Storage",
            Self::Lighting => "Lighting",
            Self::Kitchen => "Kitchen",
            Self::Bathroom => "Bathroom",
            Self::Electronics => "Electronics",
            Self::Rugs => "Rugs",
            Self::WallDecor => "Wall decor",
            Self::WindowsDoors => "Windows & doors",
            Self::Plants => "Plants",
            Self::PetItems => "Pet items",
            Self::ToysGames => "Toys & games",
            Self::Music => "Music",
            Self::Outdoor => "Outdoor",
            Self::Seasonal => "Seasonal",
            Self::Office => "Office",
            Self::Cafe => "Cafe & shop",
            Self::Hobbies => "Hobbies",
            Self::Fitness => "Fitness",
            Self::Partitions => "Walls & partitions",
            Self::Nautical => "Nautical",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Seating => "seating",
            Self::Tables => "tables",
            Self::Beds => "beds",
            Self::Storage => "storage",
            Self::Lighting => "lighting",
            Self::Kitchen => "kitchen",
            Self::Bathroom => "bathroom",
            Self::Electronics => "electronics",
            Self::Rugs => "rugs",
            Self::WallDecor => "wall_decor",
            Self::WindowsDoors => "windows_doors",
            Self::Plants => "plants",
            Self::PetItems => "pet_items",
            Self::ToysGames => "toys_games",
            Self::Music => "music",
            Self::Outdoor => "outdoor",
            Self::Seasonal => "seasonal",
            Self::Office => "office",
            Self::Cafe => "cafe",
            Self::Hobbies => "hobbies",
            Self::Fitness => "fitness",
            Self::Partitions => "partitions",
            Self::Nautical => "nautical",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    Floor,
    Rug,
    Wall,
    Surface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Permanent,
    Rotating,
    Event,
    Craft,
    Reward,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Set {
    pub id: String,
    pub name: String,
    pub description: String,
    pub mood: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Furniture {
    pub id: String,
    pub name: String,
    pub category: Category,
    pub layer: Layer,
    pub footprint: [u32; 2],
    pub rarity: Rarity,
    pub price: i64,
    pub currency: Currency,
    pub set: Option<String>,
    pub tags: Vec<String>,
    pub variants: Vec<String>,
    pub availability: Availability,
    pub description: String,
    pub sprite_brief: String,
    /// Native sprite width for floor and surface items; defaults to the footprint width.
    #[serde(default)]
    pub iso_width: Option<u32>,
    /// For items tagged `surface`: how high their top is, in native pixels.
    #[serde(default)]
    pub surface_height: Option<u32>,
}

impl Furniture {
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Surface {
    pub id: String,
    pub name: String,
    pub rarity: Rarity,
    pub price: i64,
    pub currency: Currency,
    pub set: Option<String>,
    pub pattern_brief: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Surfaces {
    pub wallpapers: Vec<Surface>,
    pub floors: Vec<Surface>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoomType {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_wallpaper: String,
    pub default_floor: String,
    pub bonus_categories: Vec<Category>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PetSpecies {
    pub id: String,
    pub name: String,
    pub rarity: Rarity,
    pub capsule_weight: u32,
    pub habitat: String,
    pub personality: String,
    pub favorite_food: String,
    pub sprite_brief: String,
    #[serde(default)]
    pub starter: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Look {
    pub id: String,
    pub name: String,
    pub sprite_brief: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Fish {
    pub id: String,
    pub name: String,
    pub rarity: Rarity,
    pub min_cm: f32,
    pub max_cm: f32,
    pub base_value: i64,
    pub time: String,
    pub season: String,
    pub weight: u32,
    pub flavor: String,
    pub sprite_brief: String,
}

/// One playable item: a base design, optionally in a palette variant.
#[derive(Debug, Clone, Copy)]
pub struct Item<'a> {
    pub base: &'a Furniture,
    pub variant: Option<&'a str>,
}

impl Item<'_> {
    /// `sofa` or `sofa__mint`.
    pub fn id(&self) -> String {
        match self.variant {
            Some(v) => format!("{}__{v}", self.base.id),
            None => self.base.id.clone(),
        }
    }

    pub fn name(&self) -> String {
        match self.variant {
            Some(v) => {
                let mut c = v.chars();
                let cap: String = c
                    .next()
                    .map(|f| f.to_uppercase().chain(c).collect())
                    .unwrap_or_default();
                format!("{} ({cap})", self.base.name)
            }
            None => self.base.name.clone(),
        }
    }
}

/// All game content, loaded once.
pub struct Content {
    pub sets: Vec<Set>,
    pub furniture: Vec<Furniture>,
    pub surfaces: Surfaces,
    pub rooms: Vec<RoomType>,
    pub pets: Vec<PetSpecies>,
    pub fish: Vec<Fish>,
    pub looks: Vec<Look>,
    furniture_index: HashMap<String, usize>,
}

static EMBEDDED: LazyLock<Content> = LazyLock::new(|| {
    Content::from_json(
        include_str!("../../../content/sets.json"),
        include_str!("../../../content/furniture.json"),
        include_str!("../../../content/surfaces.json"),
        include_str!("../../../content/rooms.json"),
        include_str!("../../../content/pets.json"),
        include_str!("../../../content/fish.json"),
        include_str!("../../../content/looks.json"),
    )
    .expect("embedded content must parse; run `cargo test -p pinchora-game`")
});

impl Content {
    /// The content compiled into this binary.
    pub fn get() -> &'static Content {
        &EMBEDDED
    }

    pub fn from_json(
        sets: &str,
        furniture: &str,
        surfaces: &str,
        rooms: &str,
        pets: &str,
        fish: &str,
        looks: &str,
    ) -> anyhow::Result<Self> {
        let furniture: Vec<Furniture> = serde_json::from_str(furniture)?;
        let furniture_index = furniture.iter().enumerate().map(|(i, f)| (f.id.clone(), i)).collect();
        Ok(Self {
            sets: serde_json::from_str(sets)?,
            furniture,
            surfaces: serde_json::from_str(surfaces)?,
            rooms: serde_json::from_str(rooms)?,
            pets: serde_json::from_str(pets)?,
            fish: serde_json::from_str(fish)?,
            looks: serde_json::from_str(looks)?,
            furniture_index,
        })
    }

    pub fn furniture(&self, base_id: &str) -> Option<&Furniture> {
        self.furniture_index.get(base_id).map(|&i| &self.furniture[i])
    }

    /// Resolves `sofa` or `sofa__mint`.
    pub fn item(&self, id: &str) -> Option<Item<'_>> {
        let (base_id, variant) = match id.split_once("__") {
            Some((b, v)) => (b, Some(v)),
            None => (id, None),
        };
        let base = self.furniture(base_id)?;
        match variant {
            None if base.variants.is_empty() => Some(Item { base, variant: None }),
            Some(v) => base.variants.iter().find(|x| x.as_str() == v).map(|x| Item {
                base,
                variant: Some(x.as_str()),
            }),
            None => None,
        }
    }

    /// Every playable item (variants expanded).
    pub fn items(&self) -> impl Iterator<Item = Item<'_>> {
        self.furniture.iter().flat_map(|base| {
            let variants: Vec<Option<&str>> = if base.variants.is_empty() {
                vec![None]
            } else {
                base.variants.iter().map(|v| Some(v.as_str())).collect()
            };
            variants.into_iter().map(move |variant| Item { base, variant })
        })
    }

    pub fn wallpaper(&self, id: &str) -> Option<&Surface> {
        self.surfaces.wallpapers.iter().find(|s| s.id == id)
    }

    pub fn floor(&self, id: &str) -> Option<&Surface> {
        self.surfaces.floors.iter().find(|s| s.id == id)
    }

    pub fn room_type(&self, id: &str) -> Option<&RoomType> {
        self.rooms.iter().find(|r| r.id == id)
    }

    pub fn pet(&self, id: &str) -> Option<&PetSpecies> {
        self.pets.iter().find(|p| p.id == id)
    }

    pub fn fish(&self, id: &str) -> Option<&Fish> {
        self.fish.iter().find(|f| f.id == id)
    }

    pub fn look(&self, id: &str) -> Option<&Look> {
        self.looks.iter().find(|l| l.id == id)
    }
}
