//! The collection games: the critter and find catalogue, what each is worth, and every roll of
//! Critter Drop, Message in a Bottle, Beachcomb and the Crab Pot. Nothing here touches the database
//! or Discord; dice come in as a closure returning numbers in `0..1`, so a draw can be checked
//! without a random source.

use std::{collections::HashMap, sync::LazyLock};

use pinchora_core::i18n::Lang;
use serde::Deserialize;

use crate::{Availability, Content, Currency, Furniture, Rarity, fishing};

// /Constants

/// Critters on a drop.
pub const DROP_CARDS: usize = 3;
/// How long a critter drop stays, in seconds.
pub const DROP_SECS: i64 = 90;
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
/// The pieces are soulbound (`pinchora_core::items`): they decorate the buyer's house but are never sold,
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
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::play::Game;

    /// A fixed list of dice, repeated.
    fn dice(rolls: &'static [f64]) -> impl FnMut() -> f64 {
        let mut n = 0;
        move || {
            let roll = rolls[n % rolls.len()];
            n += 1;
            roll
        }
    }

    /// A deterministic spread of dice, for odds checks.
    fn spread() -> impl FnMut() -> f64 {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    #[test]
    fn the_shipped_catalogue_is_well_formed() {
        let c = Collection::get();
        let content = Content::get();
        assert_eq!(c.critters.len(), 48);
        assert_eq!(c.finds.len(), 36);
        let mut ids = HashSet::new();
        for e in c.critters.iter().chain(&c.finds) {
            assert!(ids.insert(e.id.as_str()), "duplicate id {}", e.id);
            assert!(
                !e.id.is_empty() && e.id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{}",
                e.id
            );
            assert!(!e.name.is_empty() && e.name.len() <= 24, "{}: name", e.id);
            assert!(e.flavor.len() <= 72 && e.flavor.ends_with('.'), "{}: flavor", e.id);
            assert!(
                matches!(e.time.as_str(), "any" | "dawn" | "day" | "dusk" | "night"),
                "{}",
                e.id
            );
            assert!(
                matches!(e.season.as_str(), "any" | "spring" | "summer" | "autumn" | "winter"),
                "{}",
                e.id
            );
            // Every entry can be pictured: a borrowed sprite that exists, or a look the renderer draws.
            match e.art().unwrap_or_else(|| panic!("{} has no art", e.id)) {
                Art::Fish(id) => assert!(content.fish(id).is_some(), "{}: fish {id}", e.id),
                Art::Pet(id) => assert!(content.pet(id).is_some(), "{}: pet {id}", e.id),
                Art::Furniture(id) => assert!(content.furniture(id).is_some(), "{}: furniture {id}", e.id),
                Art::Drawn {
                    shape,
                    body,
                    accent,
                    pattern,
                } => {
                    assert!(
                        pinchora_render::critters::draw(shape, body, accent, pattern).is_some(),
                        "{}: cannot draw {shape}/{body}/{accent}",
                        e.id
                    );
                    assert!(pinchora_render::critters::PATTERNS.contains(&pattern), "{}", e.id);
                }
            }
        }
        for e in &c.critters {
            assert!(
                e.habitat.is_some() && e.salvage == 0,
                "{}: a critter lives somewhere",
                e.id
            );
        }
        for e in &c.finds {
            assert!(e.habitat.is_none(), "{}: a find has no habitat", e.id);
        }
        // Every rarity has something in both halves, the common ones the most.
        for kind in [Kind::Critter, Kind::Find] {
            let count = |r: Rarity| c.list(kind).iter().filter(|e| e.rarity == r).count();
            for r in Rarity::ALL {
                assert!(count(r) >= 2, "{kind:?} {r:?}");
            }
            assert!(count(Rarity::Common) > count(Rarity::Uncommon) && count(Rarity::Uncommon) > count(Rarity::Rare));
        }
        for b in &c.bottles {
            assert!(b.text.len() <= 120 && !b.text.is_empty(), "{}", b.id);
        }
        assert!(c.bottles.len() >= 40);
        let texts: HashSet<_> = c.bottles.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts.len(), c.bottles.len(), "no message twice");
        // Each message has a Turkish text, written for Turkish readers and clean for a strict server.
        let mut turkish = HashSet::new();
        for b in &c.bottles {
            let tr = c.bottle_text(&b.id, Lang::Tr).expect("a Turkish text");
            assert_ne!(tr, b.text, "{}: not translated", b.id);
            assert!(tr.chars().count() <= 95 && turkish.insert(tr), "{}: {tr}", b.id);
            assert!(
                !crate::party::filter::is_profane_for(tr, true, Lang::Tr),
                "{}: {tr}",
                b.id
            );
            assert_eq!(c.bottle_text(&b.id, Lang::En), Some(b.text.as_str()));
        }
        assert_eq!(c.bottle_text("nope", Lang::Tr), None);
        assert_eq!(c.entry("pebblesnap_crab").map(|e| e.kind), Some(Kind::Critter));
        assert_eq!(c.entry("sand_dollar").map(|e| e.kind), Some(Kind::Find));
        assert!(c.entry("nonsense").is_none());
    }

    #[test]
    fn rarer_things_pay_more_and_cost_more() {
        let rarities = Rarity::ALL;
        for pair in rarities.windows(2) {
            let (a, b) = (pay(pair[0]), pay(pair[1]));
            assert!(a.new_shells < b.new_shells && a.dup_shells < b.dup_shells && a.dup_fragments < b.dup_fragments);
            assert!(exchange_cost(pair[0]) < exchange_cost(pair[1]));
        }
        for r in rarities {
            let p = pay(r);
            assert!(p.dup_shells < p.new_shells, "a discovery pays more than a spare");
        }
        let total: f64 = rarities.iter().map(|r| odds(*r)).sum();
        assert!((total - 100.0).abs() < 1e-9);
        assert!(odds(Rarity::Common) > odds(Rarity::Legendary) * 30.0);
    }

    #[test]
    fn a_draw_follows_the_table() {
        let pool: Vec<&Entry> = Collection::get().critters.iter().collect();
        // The lowest dice give a common critter, the highest a legendary one.
        let low = pick(&pool, 0.0, &mut dice(&[0.0, 0.0])).expect("a critter");
        assert_eq!(low.rarity, Rarity::Common);
        let high = pick(&pool, 0.0, &mut dice(&[0.999, 0.5])).expect("a critter");
        assert_eq!(high.rarity, Rarity::Legendary);
        // Many draws land close to the table, whatever the number of entries in a tier.
        let mut rng = spread();
        let mut seen = std::collections::BTreeMap::new();
        let n = 20_000;
        for _ in 0..n {
            *seen
                .entry(pick(&pool, 0.0, &mut rng).expect("a critter").rarity)
                .or_insert(0usize) += 1;
        }
        for r in Rarity::ALL {
            let share = seen[&r] as f64 * 100.0 / n as f64;
            assert!(
                (share - odds(r)).abs() < 1.5,
                "{r:?}: {share:.1}% against {:.1}%",
                odds(r)
            );
        }
        // Luck lifts the rare side only.
        let mut rng = spread();
        let lucky = (0..n)
            .filter(|_| pick(&pool, 1.0, &mut rng).expect("a critter").rarity >= Rarity::Rare)
            .count();
        assert!(lucky as f64 / n as f64 > 0.17, "{lucky}");
        assert!(pick(&[], 0.0, &mut spread()).is_none());
    }

    #[test]
    fn a_drop_has_three_different_critters() {
        let mut rng = spread();
        for _ in 0..500 {
            let cards = drop_cards(12, 6, &mut rng);
            assert_eq!(cards.len(), DROP_CARDS);
            let ids: HashSet<_> = cards.iter().map(|c| c.id.as_str()).collect();
            assert_eq!(ids.len(), DROP_CARDS);
            assert!(cards.iter().all(|c| c.kind == Kind::Critter));
        }
        // Even dice that keep asking for the same legendary critter end up with three cards.
        assert_eq!(drop_cards(12, 6, &mut dice(&[0.999, 0.0])).len(), DROP_CARDS);
    }

    #[test]
    fn a_beachcomb_follows_the_clock_and_always_finds_something() {
        let mut rng = spread();
        for hour in 0..24 {
            for month in 1..=12 {
                for _ in 0..40 {
                    let e = comb(hour, month, &mut rng).expect("something washes up");
                    let (tod, season) = (fishing::time_of_day(hour), fishing::season(month));
                    assert!(e.fits(tod, season), "{} at hour {hour} in month {month}", e.id);
                    if e.kind == Kind::Critter {
                        assert!(matches!(e.habitat, Some(Habitat::Shore | Habitat::Tidepool)));
                    }
                }
            }
        }
        // A critter roll below the share, then dice for a common one; the moon pearl needs the night.
        let critter = comb(12, 6, &mut dice(&[0.1, 0.0, 0.0])).expect("a critter");
        assert_eq!(critter.kind, Kind::Critter);
        let find = comb(12, 6, &mut dice(&[0.9, 0.999, 0.99])).expect("a find");
        assert_ne!(find.id, "moon_pearl", "no moon pearl at noon");
        let mut rng = spread();
        let pearl = (0..4_000).any(|_| comb(23, 3, &mut rng).is_some_and(|e| e.id == "moon_pearl"));
        assert!(pearl, "the moon pearl turns up at night");
    }

    #[test]
    fn crab_pots_catch_more_with_level_and_longer_soaks() {
        for level in 1..POT_MAX_LEVEL {
            assert!(pot_rate(level) < pot_rate(level + 1));
            assert!(pot_upgrade_cost(level).is_some());
            assert!(pot_upgrade_cost(level) < pot_upgrade_cost(level + 1).or(Some(i64::MAX)));
        }
        assert_eq!(pot_upgrade_cost(POT_MAX_LEVEL), None);
        assert_eq!(pot_catches(1, 1), 1);
        assert_eq!(pot_catches(1, 4), 4);
        assert_eq!(pot_catches(1, 8), 10);
        assert_eq!(pot_catches(5, 8), 18);
        for level in 1..=POT_MAX_LEVEL {
            // Per hour, a longer soak is worth a little more.
            let per_hour = |h: i16| f64::from(pot_catches(level, h)) / f64::from(h);
            assert!(per_hour(8) >= per_hour(4) && per_hour(4) >= per_hour(1));
        }
        // The deep stays out of a pot's reach, however lucky it is.
        let mut rng = spread();
        for _ in 0..3_000 {
            let e = pot_catch(5, 23, 12, &mut rng).expect("a catch");
            assert_ne!(e.habitat, Some(Habitat::Deep), "{}", e.id);
        }
        // A better pot finds rare things more often.
        let rares = |level: i16| {
            let mut rng = spread();
            (0..6_000)
                .filter(|_| pot_catch(level, 12, 6, &mut rng).is_some_and(|e| e.rarity >= Rarity::Rare))
                .count()
        };
        assert!(rares(5) > rares(1), "{} against {}", rares(5), rares(1));
    }

    #[test]
    fn a_bottle_holds_a_prize_by_the_dice() {
        assert_eq!(open_bottle(&mut dice(&[0.0, 0.0])), Prize::Shells(20));
        assert_eq!(open_bottle(&mut dice(&[0.61, 0.999])), Prize::Shells(60));
        assert_eq!(open_bottle(&mut dice(&[0.7, 0.0])), Prize::Fragments(4));
        assert!(matches!(open_bottle(&mut dice(&[0.9, 0.5, 0.5])), Prize::Find(_)));
        assert_eq!(open_bottle(&mut dice(&[0.99, 0.999])), Prize::Treasure(300));
        // On average a bottle pays well under a hundred shells.
        let mut rng = spread();
        let n = 10_000;
        let worth: i64 = (0..n)
            .map(|_| match open_bottle(&mut rng) {
                Prize::Shells(n) | Prize::Treasure(n) => n,
                Prize::Find(e) => e.pay().dup_shells,
                Prize::Fragments(_) => 0,
            })
            .sum();
        assert!((20..60).contains(&(worth / n)), "{}", worth / n);
    }

    #[test]
    fn no_collection_game_comes_near_the_hourly_limit() {
        // What a spare pays on average: the table's odds against what each rarity pays.
        let spare: f64 = Rarity::ALL
            .iter()
            .map(|r| odds(*r) / 100.0 * pay(*r).dup_shells as f64)
            .sum();
        assert!((14.0..20.0).contains(&spare), "{spare}");
        // The most plays an hour each game can have, and what they pay on average.
        let beachcomb = 60.0 / COMB_COOLDOWN_MINUTES as f64 * spare;
        // A server gets one spawn per cooldown, and a player one hand-called drop per half hour.
        let spawns_per_hour = 3600.0 / SPAWN_COOLDOWN_SECS as f64;
        let critter_drops = (spawns_per_hour + 3600.0 / MANUAL_DROP_COOLDOWN_SECS as f64) * spare;
        let bottles = spawns_per_hour * 34.0;
        let pot = f64::from(pot_catches(POT_MAX_LEVEL, 8)) * spare * 1.2 / 8.0;
        for (game, per_hour) in [
            ("beachcomb", beachcomb),
            ("critter drop", critter_drops),
            ("bottle", bottles),
            ("crab pot", pot),
        ] {
            assert!(per_hour < 200.0, "{game} pays {per_hour:.0} shells an hour");
        }
        // All four at once, a player who never sleeps, still under a tenth of the 1,500 an hour the games may pay.
        assert!(beachcomb + critter_drops + bottles + pot < 150.0 * 4.0);
        // Only so many plays a day pay at all.
        for game in [Game::CritterDrop, Game::Bottle, Game::Beachcomb, Game::CrabPot] {
            assert!((3..=24).contains(&game.daily_cap()), "{game:?}");
        }
    }

    #[test]
    fn fragments_buy_free_to_earn_decor_at_every_rarity() {
        let content = Content::get();
        for r in Rarity::ALL {
            let pool = exchange_pool(content, r);
            assert!(!pool.is_empty(), "{r:?} has nothing to buy");
            for f in &pool {
                assert_eq!(f.availability, Availability::Reward);
                assert_eq!(f.currency, Currency::Shells);
                assert_eq!(f.rarity, r);
            }
            for roll in [0.0, 0.3, 0.6, 0.999] {
                let id = exchange_pick(content, r, roll).expect("a piece");
                assert!(content.item(&id).is_some(), "{id} is not an item");
            }
        }
        // Different dice reach different pieces.
        let picks: HashSet<_> = (0..40)
            .filter_map(|n| exchange_pick(content, Rarity::Common, f64::from(n) / 40.0))
            .collect();
        assert!(picks.len() >= 3, "{picks:?}");
    }
    /// An hour and a month in which `e` can turn up.
    fn when(e: &Entry) -> (u32, u32) {
        let hour = match e.time.as_str() {
            "dawn" => 5,
            "dusk" => 18,
            "night" => 23,
            _ => 12,
        };
        let month = match e.season.as_str() {
            "spring" => 4,
            "summer" => 7,
            "autumn" => 10,
            "winter" => 1,
            _ => 6,
        };
        (hour, month)
    }

    #[test]
    fn what_is_tied_to_a_time_or_season_turns_up_in_drops_and_pots_only_then() {
        let c = Collection::get();
        let tied: Vec<&Entry> = c
            .critters
            .iter()
            .filter(|e| e.time != "any" || e.season != "any")
            .collect();
        assert!(tied.len() >= 8, "{} critters are tied to a time", tied.len());
        // Away from their time none of them is ever dealt, in a drop or a pot, whatever the dice say.
        let mut rng = spread();
        for hour in 0..24 {
            for month in 1..=12 {
                let (tod, season) = (fishing::time_of_day(hour), fishing::season(month));
                for _ in 0..30 {
                    for card in drop_cards(hour, month, &mut rng) {
                        assert!(card.fits(tod, season), "{} at hour {hour} in month {month}", card.id);
                    }
                    let e = pot_catch(3, hour, month, &mut rng).expect("a catch");
                    assert!(e.fits(tod, season), "{} at hour {hour} in month {month}", e.id);
                }
            }
        }
        // At their time each can be dealt in a drop (the deep ones only come that way), and the ones a
        // pot reaches in a pot.
        for e in &tied {
            let (hour, month) = when(e);
            assert!(e.fits(fishing::time_of_day(hour), fishing::season(month)), "{}", e.id);
            let mut rng = spread();
            let dealt = (0..60_000).any(|_| drop_cards(hour, month, &mut rng).iter().any(|c| c.id == e.id));
            assert!(
                dealt,
                "{} never washes up in a drop at hour {hour} in month {month}",
                e.id
            );
            if e.habitat != Some(Habitat::Deep) {
                let caught = (0..60_000).any(|_| pot_catch(5, hour, month, &mut rng).is_some_and(|c| c.id == e.id));
                assert!(
                    caught,
                    "{} is never caught in a pot at hour {hour} in month {month}",
                    e.id
                );
            }
        }
        // The moon pearl is a night find: a pot brings it up at night and never at noon.
        let mut rng = spread();
        assert!((0..30_000).any(|_| pot_catch(5, 23, 6, &mut rng).is_some_and(|e| e.id == "moon_pearl")));
        assert!(!(0..30_000).any(|_| pot_catch(5, 12, 6, &mut rng).is_some_and(|e| e.id == "moon_pearl")));
    }

    #[test]
    fn fragment_prices_follow_the_catalogue() {
        let content = Content::get();
        // A spare pays this many shells and fragments on average, so a fragment is worth about this many shells.
        let (mut shells, mut fragments) = (0.0, 0.0);
        for r in Rarity::ALL {
            shells += odds(r) / 100.0 * pay(r).dup_shells as f64;
            fragments += odds(r) / 100.0 * pay(r).dup_fragments as f64;
        }
        assert!(
            (16.6..16.8).contains(&shells) && (2.3..2.5).contains(&fragments),
            "{shells} {fragments}"
        );
        assert!(
            (shells / fragments - FRAGMENT_SHELLS).abs() < 0.2,
            "a fragment is worth {:.2} shells beside a spare's shells",
            shells / fragments
        );
        // And a piece costs what its tier is worth in catalogue price at that rate, give or take a tenth.
        for r in Rarity::ALL {
            let pool = exchange_pool(content, r);
            let mean = pool.iter().map(|f| f.price as f64).sum::<f64>() / pool.len() as f64;
            let worth = mean / FRAGMENT_SHELLS / exchange_cost(r) as f64;
            assert!(
                (0.9..1.1).contains(&worth),
                "{r:?}: pieces average {mean:.0} shells, {} fragments is {:.0}% of that at {FRAGMENT_SHELLS} a fragment",
                exchange_cost(r),
                worth * 100.0
            );
        }
        // Common pieces are within a day's play, the legendary ones are a long goal.
        assert!(exchange_cost(Rarity::Common) < 20 && exchange_cost(Rarity::Legendary) > 5_000);
    }
}
