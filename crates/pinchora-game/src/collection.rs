//! The collection games: the critter and find catalogue, what each is worth, and every roll of
//! Critter Drop, Message in a Bottle, Beachcomb and the Crab Pot. Nothing here touches the database
//! or Discord; dice come in as a closure returning numbers in `0..1`, so a draw can be checked
//! without a random source.

use std::{collections::HashMap, sync::LazyLock};

use pinchora_core::infra::i18n::Lang;
use serde::Deserialize;

use crate::{Availability, Content, Currency, Furniture, Rarity, fishing};

// /Constants

/// Critters on a drop.
pub const DROP_CARDS: usize = 3;
/// How long a critter drop stays, in seconds.
pub const DROP_SECS: i64 = 90;
/// Independent chance for each Critter Drop card to be a Glimmer variant.
pub const GLIMMER_SHARE: f64 = 0.02;
pub const SHARD_CRAFT_BASE: i64 = 12;

pub fn recycle_shards(rarity: Rarity) -> i64 {
    match rarity {
        Rarity::Common => 2,
        Rarity::Uncommon => 4,
        Rarity::Rare => 9,
        Rarity::Epic => 20,
        Rarity::Legendary => 45,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HabitatSetProgress {
    pub habitat: Habitat,
    pub found: usize,
    pub total: usize,
}

impl HabitatSetProgress {
    pub fn complete(self) -> bool {
        self.total > 0 && self.found == self.total
    }
}

pub fn habitat_sets(owned: &std::collections::HashSet<String>) -> Vec<HabitatSetProgress> {
    [
        Habitat::Shore,
        Habitat::Tidepool,
        Habitat::Kelp,
        Habitat::Reef,
        Habitat::Deep,
    ]
    .into_iter()
    .map(|habitat| {
        let entries: Vec<_> = Collection::get()
            .critters
            .iter()
            .filter(|entry| entry.habitat == Some(habitat))
            .collect();
        HabitatSetProgress {
            habitat,
            found: entries.iter().filter(|entry| owned.contains(&entry.id)).count(),
            total: entries.len(),
        }
    })
    .collect()
}

pub fn shard_craft_cost(owned: &std::collections::HashSet<String>) -> i64 {
    (SHARD_CRAFT_BASE - habitat_sets(owned).iter().filter(|set| set.complete()).count() as i64).max(7)
}
/// How long a bottle floats, in seconds.
pub const BOTTLE_SECS: i64 = 600;
/// Messages (from at least [`SPAWN_CHATTERS`] people) a server counts before something washes up.
pub const SPAWN_MESSAGES: i64 = 25;
pub const SPAWN_CHATTERS: usize = 2;
/// The least time between two chat-driven spawns in a server, in seconds.
pub const SPAWN_COOLDOWN_SECS: u64 = 720;
/// How often one person's messages count towards a spawn, in seconds.
pub const COUNT_EVERY_SECS: u64 = 20;
/// Of the chat-driven spawns, how many are bottles rather than critter drops.
pub const BOTTLE_SHARE: f64 = 0.3;
/// How often one player can call a critter drop by hand, in seconds.
pub const MANUAL_DROP_COOLDOWN_SECS: u64 = 1800;
/// How often a channel gets a hand-called critter drop, whoever calls it, in seconds.
pub const MANUAL_CHANNEL_COOLDOWN_SECS: u64 = 600;
// A channel waits longer than a drop lasts (so hand-called drops never overlap) and less than one caller does.
const _: () = assert!(
    MANUAL_CHANNEL_COOLDOWN_SECS > DROP_SECS as u64 && MANUAL_CHANNEL_COOLDOWN_SECS < MANUAL_DROP_COOLDOWN_SECS
);
/// The wait between two beachcombs, in minutes.
pub const COMB_COOLDOWN_MINUTES: i64 = 20;
/// Of the things a beachcomb turns up, how many are living critters rather than finds.
const COMB_CRITTER_SHARE: f64 = 0.22;
/// Of the things a crab pot catches, how many are finds rather than critters.
const POT_FIND_SHARE: f64 = 0.25;

// /Catalogue

/// Where a critter lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Habitat {
    Shore,
    Tidepool,
    Kelp,
    Reef,
    Deep,
}

impl Habitat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Shore => "Shore",
            Self::Tidepool => "Tide pool",
            Self::Kelp => "Kelp forest",
            Self::Reef => "Reef",
            Self::Deep => "Deep water",
        }
    }
}

/// The two halves of the dex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Kind {
    #[default]
    Critter,
    Find,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Critter => "Critters",
            Self::Find => "Finds",
        }
    }
}

/// A critter or a find, from `content/critters.json` and `content/finds.json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub rarity: Rarity,
    /// Where a critter lives; finds have none.
    #[serde(default)]
    pub habitat: Option<Habitat>,
    /// `any`, or the part of the day it turns up in a beachcomb (as in `content/fish.json`).
    pub time: String,
    /// `any`, or the season it turns up in a beachcomb.
    pub season: String,
    /// Fragments a find brings on top of what it pays, every time.
    #[serde(default)]
    pub salvage: u32,
    pub flavor: String,
    /// A sprite borrowed until the entry has its own: `fish:<id>`, `pet:<id>` or `furniture:<id>`.
    #[serde(default)]
    pub art: Option<String>,
    /// Otherwise a drawn look: a shape, a body and an accent palette and a pattern, see
    /// `pinchora_render::critters`.
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(skip)]
    pub kind: Kind,
}

/// How an entry is pictured when the atlas has no `cr_<id>` sprite for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art<'a> {
    Fish(&'a str),
    Pet(&'a str),
    Furniture(&'a str),
    Drawn {
        shape: &'a str,
        body: &'a str,
        accent: &'a str,
        pattern: &'a str,
    },
}

impl Entry {
    /// The picture the entry falls back to; `None` for an entry with neither a borrowed sprite nor a look.
    pub fn art(&self) -> Option<Art<'_>> {
        if let Some((kind, id)) = self.art.as_deref().and_then(|a| a.split_once(':')) {
            return match kind {
                "fish" => Some(Art::Fish(id)),
                "pet" => Some(Art::Pet(id)),
                "furniture" => Some(Art::Furniture(id)),
                _ => None,
            };
        }
        Some(Art::Drawn {
            shape: self.shape.as_deref()?,
            body: self.body.as_deref()?,
            accent: self.accent.as_deref()?,
            pattern: self.pattern.as_deref().unwrap_or("plain"),
        })
    }

    /// The atlas sprite that replaces the fallback once it is drawn.
    pub fn sprite_name(&self) -> String {
        format!("cr_{}", self.id)
    }

    /// Where a critter lives, or what a find is, as a short phrase for a card.
    pub fn place(&self) -> &'static str {
        self.habitat.map_or("Washed up", Habitat::label)
    }

    /// The name in `lang`: the translation when the language has one, else the English name.
    pub fn name_in(&self, lang: Lang) -> String {
        match lang {
            Lang::En => self.name.clone(),
            _ => Content::get().name(&self.id, lang).into_owned(),
        }
    }

    /// The flavour line in `lang`: the translation when the language has one, else the English line.
    pub fn flavor_in(&self, lang: Lang) -> String {
        match lang {
            Lang::En => self.flavor.clone(),
            _ => Content::get()
                .description(&self.id, lang)
                .map_or_else(|| self.flavor.clone(), std::borrow::Cow::into_owned),
        }
    }

    pub fn pay(&self) -> Pay {
        pay(self.rarity)
    }

    /// Whether a beachcomb at this UTC `hour` in this `month` can turn it up.
    fn fits(&self, tod: &str, season: &str) -> bool {
        (self.time == "any" || self.time == tod) && (self.season == "any" || self.season == season)
    }
}

/// A message in a bottle, from `content/bottles.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct Bottle {
    pub id: String,
    pub text: String,
}

/// Every critter, find and bottle message, loaded once.
pub struct Collection {
    pub critters: Vec<Entry>,
    pub finds: Vec<Entry>,
    pub bottles: Vec<Bottle>,
    index: HashMap<String, (Kind, usize)>,
}

static EMBEDDED: LazyLock<Collection> = LazyLock::new(|| {
    Collection::from_json(
        include_str!("../../../content/critters.json"),
        include_str!("../../../content/finds.json"),
        include_str!("../../../content/bottles.json"),
    )
    .expect("embedded collection content must parse; run `cargo test -p pinchora-game`")
});

/// The Turkish text of each bottle message, by the id the English one has.
static BOTTLES_TR: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str::<Vec<Bottle>>(include_str!("../../../content/bottles_tr.json"))
        .expect("embedded Turkish bottle messages must parse; run `cargo test -p pinchora-game`")
        .into_iter()
        .map(|b| (b.id, b.text))
        .collect()
});

impl Collection {
    pub fn get() -> &'static Collection {
        &EMBEDDED
    }

    /// The text of bottle `id` in `lang`; the English one when there is no translation.
    pub fn bottle_text(&self, id: &str, lang: Lang) -> Option<&str> {
        let english = self.bottle(id)?;
        match lang {
            Lang::En => Some(english.text.as_str()),
            Lang::Tr => Some(BOTTLES_TR.get(id).map_or(english.text.as_str(), String::as_str)),
        }
    }

    pub fn from_json(critters: &str, finds: &str, bottles: &str) -> anyhow::Result<Self> {
        let mut critters: Vec<Entry> = serde_json::from_str(critters)?;
        let mut finds: Vec<Entry> = serde_json::from_str(finds)?;
        for entry in &mut critters {
            entry.kind = Kind::Critter;
        }
        for entry in &mut finds {
            entry.kind = Kind::Find;
        }
        let mut index = HashMap::new();
        for (kind, list) in [(Kind::Critter, &critters), (Kind::Find, &finds)] {
            for (i, entry) in list.iter().enumerate() {
                index.insert(entry.id.clone(), (kind, i));
            }
        }
        Ok(Self {
            critters,
            finds,
            bottles: serde_json::from_str(bottles)?,
            index,
        })
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        let (kind, i) = *self.index.get(id)?;
        Some(&self.list(kind)[i])
    }

    /// Every entry of a half of the dex, in dex order.
    pub fn list(&self, kind: Kind) -> &[Entry] {
        match kind {
            Kind::Critter => &self.critters,
            Kind::Find => &self.finds,
        }
    }

    pub fn len(&self) -> usize {
        self.critters.len() + self.finds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn bottle(&self, id: &str) -> Option<&Bottle> {
        self.bottles.iter().find(|b| b.id == id)
    }
}

// /Value

/// What an entry pays when a player gets it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pay {
    /// Shells for the first one of its kind (the discovery).
    pub new_shells: i64,
    /// Shells for one the player has already (the spare is recycled).
    pub dup_shells: i64,
    /// Fragments a recycled spare leaves.
    pub dup_fragments: i64,
}

pub fn pay(rarity: Rarity) -> Pay {
    let (new_shells, dup_shells, dup_fragments) = match rarity {
        Rarity::Common => (15, 6, 1),
        Rarity::Uncommon => (35, 14, 2),
        Rarity::Rare => (80, 35, 5),
        Rarity::Epic => (200, 90, 12),
        Rarity::Legendary => (500, 250, 30),
    };
    Pay {
        new_shells,
        dup_shells,
        dup_fragments,
    }
}

/// How likely a rarity is to be drawn; `luck` lifts the rare half of the table (a better pot).
fn weight(rarity: Rarity, luck: f64) -> f64 {
    let (base, rare_side) = match rarity {
        Rarity::Common => (60.0, false),
        Rarity::Uncommon => (25.0, false),
        Rarity::Rare => (10.0, true),
        Rarity::Epic => (4.0, true),
        Rarity::Legendary => (1.0, true),
    };
    if rare_side { base * (1.0 + luck) } else { base }
}

/// The shown chance of each rarity on a plain draw, as a percentage.
pub fn odds(rarity: Rarity) -> f64 {
    let total: f64 = Rarity::ALL.iter().map(|r| weight(*r, 0.0)).sum();
    weight(rarity, 0.0) * 100.0 / total
}

/// A random entry of `pool`: first a rarity, by the table (only rarities the pool has take part),
/// then one of that rarity, evenly. So a tier's chance never depends on how many entries it has.
pub fn pick<'a>(pool: &[&'a Entry], luck: f64, rng: &mut impl FnMut() -> f64) -> Option<&'a Entry> {
    let present: Vec<Rarity> = Rarity::ALL
        .into_iter()
        .filter(|r| pool.iter().any(|e| e.rarity == *r))
        .collect();
    let total: f64 = present.iter().map(|r| weight(*r, luck)).sum();
    if total <= 0.0 {
        return None;
    }
    let mut target = rng().clamp(0.0, 0.999_999) * total;
    let mut rarity = *present.last()?;
    for r in &present {
        let w = weight(*r, luck);
        if target < w {
            rarity = *r;
            break;
        }
        target -= w;
    }
    let tier: Vec<&Entry> = pool.iter().copied().filter(|e| e.rarity == rarity).collect();
    let at = (rng().clamp(0.0, 0.999_999) * tier.len() as f64) as usize;
    tier.get(at).copied()
}

// /Critter Drop

/// Three different critters for a drop at this UTC `hour` in this `month`, rarity first like every draw. A
/// critter tied to a time of day or a season only washes up then, wherever it lives.
pub fn drop_cards(hour: u32, month: u32, rng: &mut impl FnMut() -> f64) -> Vec<&'static Entry> {
    let (tod, season) = (fishing::time_of_day(hour), fishing::season(month));
    let all: Vec<&Entry> = Collection::get()
        .critters
        .iter()
        .filter(|c| c.fits(tod, season))
        .collect();
    let mut cards: Vec<&Entry> = Vec::with_capacity(DROP_CARDS);
    // A tier can hold fewer than three, so a repeat is simply drawn again, a few times at most.
    for _ in 0..DROP_CARDS * 20 {
        if cards.len() == DROP_CARDS {
            break;
        }
        let Some(entry) = pick(&all, 0.0, rng) else { break };
        if !cards.iter().any(|c| c.id == entry.id) {
            cards.push(entry);
        }
    }
    // Dice that never change their mind still get a full drop.
    for entry in &all {
        if cards.len() == DROP_CARDS {
            break;
        }
        if !cards.iter().any(|c| c.id == entry.id) {
            cards.push(entry);
        }
    }
    cards
}

// /Beachcomb

/// What a walk along the shore turns up at this UTC `hour` in this `month`: mostly finds, now and
/// then a critter of the shore or the tide pools. Entries tied to a time of day or a season only
/// show up then.
pub fn comb(hour: u32, month: u32, rng: &mut impl FnMut() -> f64) -> Option<&'static Entry> {
    let (tod, season) = (fishing::time_of_day(hour), fishing::season(month));
    let collection = Collection::get();
    let critter = rng() < COMB_CRITTER_SHARE;
    let pool: Vec<&Entry> = if critter {
        collection
            .critters
            .iter()
            .filter(|c| matches!(c.habitat, Some(Habitat::Shore | Habitat::Tidepool)))
            .filter(|c| c.fits(tod, season))
            .collect()
    } else {
        collection.finds.iter().filter(|f| f.fits(tod, season)).collect()
    };
    pick(&pool, 0.0, rng)
}

// /Crab pot

pub const POT_MAX_LEVEL: i16 = 5;
/// The soaks a pot can be set for, in hours.
pub const POT_HOURS: [i16; 3] = [1, 4, 8];

/// What it costs to take a pot from `level` to the next, `None` at the top.
pub fn pot_upgrade_cost(level: i16) -> Option<i64> {
    match level {
        1 => Some(400),
        2 => Some(1_000),
        3 => Some(2_500),
        4 => Some(6_000),
        _ => None,
    }
}

/// How many catches an hour a pot of `level` makes.
pub fn pot_rate(level: i16) -> f64 {
    1.0 + 0.2 * f64::from(level.clamp(1, POT_MAX_LEVEL) - 1)
}

/// A long soak catches a little more per hour than a short one.
fn pot_bonus(hours: i16) -> f64 {
    match hours {
        ..=1 => 1.0,
        2..=4 => 1.1,
        _ => 1.25,
    }
}

/// The catches in a soak of `hours` hours in a pot of `level`.
pub fn pot_catches(level: i16, hours: i16) -> u32 {
    (f64::from(hours) * pot_rate(level) * pot_bonus(hours)).floor() as u32
}

/// One thing a pot of `level` brings up at this UTC `hour` in this `month`: a find now and then, otherwise
/// a critter of the shallows (the deep ones stay out of reach). A better pot is luckier, and what is tied to
/// a time of day or a season only comes up then.
pub fn pot_catch(level: i16, hour: u32, month: u32, rng: &mut impl FnMut() -> f64) -> Option<&'static Entry> {
    let (tod, season) = (fishing::time_of_day(hour), fishing::season(month));
    let collection = Collection::get();
    let luck = 0.15 * f64::from(level.clamp(1, POT_MAX_LEVEL) - 1);
    let pool: Vec<&Entry> = if rng() < POT_FIND_SHARE {
        collection.finds.iter().filter(|f| f.fits(tod, season)).collect()
    } else {
        collection
            .critters
            .iter()
            .filter(|c| c.habitat != Some(Habitat::Deep) && c.fits(tod, season))
            .collect()
    };
    pick(&pool, luck, rng)
}

// /Message in a bottle

/// What is in a bottle besides its message.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Prize {
    Shells(i64),
    Fragments(i64),
    /// A find for the dex.
    Find(&'static Entry),
    /// A bottle with a little treasure in it.
    Treasure(i64),
}

/// The contents of a bottle that was just opened.
pub fn open_bottle(rng: &mut impl FnMut() -> f64) -> Prize {
    let span = |rng: &mut dyn FnMut() -> f64, low: i64, high: i64| {
        low + (rng().clamp(0.0, 0.999_999) * (high - low + 1) as f64) as i64
    };
    let roll = rng();
    if roll < 0.62 {
        Prize::Shells(span(rng, 20, 60))
    } else if roll < 0.77 {
        Prize::Fragments(span(rng, 4, 10))
    } else if roll < 0.97 {
        let finds: Vec<&Entry> = Collection::get().finds.iter().collect();
        pick(&finds, 0.0, rng).map_or(Prize::Shells(40), Prize::Find)
    } else {
        Prize::Treasure(span(rng, 150, 300))
    }
}

// /Fragments

/// What one fragment is worth in shells of catalogue price. A spare pays 16.7 shells and 2.4 fragments on
/// average (the draw odds against [`pay`]); the fragments are meant to be worth as much as the shells beside
/// them, so one fragment is worth 16.7 / 2.4 = 7 shells.
pub const FRAGMENT_SHELLS: f64 = 7.0;

/// What a decor piece of a rarity costs in fragments: the mean catalogue price of the pieces fragments can
/// buy at that rarity ([`exchange_pool`]) over [`FRAGMENT_SHELLS`], rounded.
///
/// | rarity    | mean price | fragments | spares of average luck |
/// |-----------|-----------:|----------:|-----------------------:|
/// | common    |         83 |        12 |                      5 |
/// | uncommon  |        458 |        65 |                     27 |
/// | rare      |      2,599 |       370 |                    155 |
/// | epic      |     13,658 |     1,950 |                    819 |
/// | legendary |     62,166 |     8,900 |                  3,739 |
///
/// The pieces are soulbound (`pinchora_core::games::items`): they decorate the buyer's house but are never sold,
/// gifted or donated, so fragments cannot be turned into shells at a better rate than spares pay.
/// `fragment_prices_follow_the_catalogue` fails when the catalogue moves away from this table.
pub fn exchange_cost(rarity: Rarity) -> i64 {
    match rarity {
        Rarity::Common => 12,
        Rarity::Uncommon => 65,
        Rarity::Rare => 370,
        Rarity::Epic => 1_950,
        Rarity::Legendary => 8_900,
    }
}

/// The pieces fragments can buy at `rarity`: free-to-earn catalog furniture (`reward` items paid in
/// shells), never what the shop sells, never premium. The same pool the pet trips draw from.
pub fn exchange_pool(content: &Content, rarity: Rarity) -> Vec<&Furniture> {
    content
        .furniture
        .iter()
        .filter(|f| f.availability == Availability::Reward && f.currency == Currency::Shells && f.rarity == rarity)
        .collect()
}

/// An item id (`sofa` or `sofa__mint`) of the pool at `rarity`; `roll` picks the piece and its colour.
pub fn exchange_pick(content: &Content, rarity: Rarity, roll: f64) -> Option<String> {
    let pool = exchange_pool(content, rarity);
    if pool.is_empty() {
        return None;
    }
    let scaled = roll.clamp(0.0, 0.999_999) * pool.len() as f64;
    let furniture = pool[scaled as usize];
    Some(match furniture.variants.len() {
        0 => furniture.id.clone(),
        n => {
            let at = ((scaled.fract()) * n as f64) as usize;
            format!("{}__{}", furniture.id, furniture.variants[at.min(n - 1)])
        }
    })
}

#[cfg(test)]
#[path = "collection/tests.rs"]
mod tests;
