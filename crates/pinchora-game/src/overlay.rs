//! Translated names and descriptions of game content.
//!
//! English lives in `content/*.json`; every other language is an overlay in `content/i18n/<code>/*.json`
//! that maps a content id to `{name, description?}` (pets also carry a `food`, pet trips their `stories`).
//! Ask [`Content::name`] / [`Content::description`] for any id, in any [`Lang`]: a language with no overlay, or an
//! id the overlay lacks, gives the English text. Colour variants (`sofa__mint`) are not stored: they are the
//! translated base name plus the translated colour word, as the English `Item::name` does.

use std::{borrow::Cow, collections::HashMap, sync::LazyLock};

use pinchora_core::games::items::plain;
use pinchora_core::infra::i18n::Lang;
use pinchora_render::character::Slot;
use serde::Deserialize;

use crate::{
    BadgeStat, Category, Content, Item, Rarity, character,
    collection::{Collection, Entry as CollectionEntry, Habitat, Kind},
    contest,
    house::Tier,
    quests::Quest,
    trips::Trip,
};

// /Constants

/// Files that map a content id to its text, per language; ids are unique across all of them.
const TR_FILES: [(&str, &str); 19] = [
    ("furniture", include_str!("../../../content/i18n/tr/furniture.json")),
    ("sets", include_str!("../../../content/i18n/tr/sets.json")),
    ("surfaces", include_str!("../../../content/i18n/tr/surfaces.json")),
    ("rooms", include_str!("../../../content/i18n/tr/rooms.json")),
    ("pets", include_str!("../../../content/i18n/tr/pets.json")),
    ("fish", include_str!("../../../content/i18n/tr/fish.json")),
    ("looks", include_str!("../../../content/i18n/tr/looks.json")),
    ("accessories", include_str!("../../../content/i18n/tr/accessories.json")),
    ("presets", include_str!("../../../content/i18n/tr/presets.json")),
    ("backgrounds", include_str!("../../../content/i18n/tr/backgrounds.json")),
    ("badges", include_str!("../../../content/i18n/tr/badges.json")),
    ("titles", include_str!("../../../content/i18n/tr/titles.json")),
    (
        "contest_themes",
        include_str!("../../../content/i18n/tr/contest_themes.json"),
    ),
    ("critters", include_str!("../../../content/i18n/tr/critters.json")),
    ("finds", include_str!("../../../content/i18n/tr/finds.json")),
    ("pet_trips", include_str!("../../../content/i18n/tr/pet_trips.json")),
    (
        "fishing_gear",
        include_str!("../../../content/i18n/tr/fishing_gear.json"),
    ),
    ("terms", include_str!("../../../content/i18n/tr/terms.json")),
    ("colors", include_str!("../../../content/i18n/tr/colors.json")),
];

// /Types

/// The text of one content id in one language.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Text {
    pub name: String,
    /// The one descriptive line: a description, flavour text, blurb or personality.
    #[serde(default)]
    pub description: Option<String>,
    /// A pet species' favourite food.
    #[serde(default)]
    pub food: Option<String>,
    /// A pet trip's story lines (`{pet}` stands for the pet's name), in the order of the English ones.
    #[serde(default)]
    pub stories: Vec<String>,
}

/// One language's text for every content id, term and colour.
pub struct Overlay {
    /// File name (without `.json`) → id → text, in the order of [`TR_FILES`].
    files: Vec<(&'static str, HashMap<String, Text>)>,
    /// Content id → index into `files`.
    index: HashMap<String, usize>,
}

impl Overlay {
    fn parse(files: &[(&'static str, &str)]) -> anyhow::Result<Self> {
        let mut parsed = Vec::with_capacity(files.len());
        let mut index = HashMap::new();
        for (i, (file, json)) in files.iter().enumerate() {
            let map: HashMap<String, Text> =
                serde_json::from_str(json).map_err(|e| anyhow::anyhow!("content/i18n/{file}.json: {e}"))?;
            if !matches!(*file, "terms" | "colors") {
                for id in map.keys() {
                    if let Some(first) = index.insert(id.clone(), i) {
                        anyhow::bail!("id {id} is in both {} and {file}", files[first].0);
                    }
                }
            }
            parsed.push((*file, map));
        }
        Ok(Self { files: parsed, index })
    }

    /// The text of a content id (a base id: no colour variant, no `~bound`).
    pub fn text(&self, id: &str) -> Option<&Text> {
        self.files[*self.index.get(id)?].1.get(id)
    }

    /// Every `(id, text)` of one file, e.g. `"pets"`.
    pub fn file(&self, file: &str) -> impl Iterator<Item = (&str, &Text)> {
        self.files
            .iter()
            .filter(move |(name, _)| *name == file)
            .flat_map(|(_, map)| map.iter().map(|(id, text)| (id.as_str(), text)))
    }

    fn lookup(&self, file: &str, key: &str) -> Option<&str> {
        let (_, map) = self.files.iter().find(|(name, _)| *name == file)?;
        map.get(key).map(|t| t.name.as_str())
    }

    /// A game word: `rarity.rare`, `category.beds`, `tier.3`, `quest.fish`, `stat.level.noun`, `time.dawn` ...
    pub fn term(&self, key: &str) -> Option<&str> {
        self.lookup("terms", key)
    }

    /// A colour word by its English name, lower-cased with `_` for spaces (`honey_blonde`).
    pub fn color(&self, key: &str) -> Option<&str> {
        self.lookup("colors", key)
    }

    /// The name of a base id, or of `base__colour`, as shown to players.
    fn name_of(&self, id: &str) -> Option<Cow<'_, str>> {
        let id = plain(id);
        match id.split_once("__") {
            Some((base, colour)) => {
                let base = &self.text(base)?.name;
                let colour = self.color(colour)?;
                Some(Cow::Owned(format!("{base} ({colour})")))
            }
            None => self.text(id).map(|t| Cow::Borrowed(t.name.as_str())),
        }
    }
}

static TR: LazyLock<Overlay> =
    LazyLock::new(|| Overlay::parse(&TR_FILES).expect("content/i18n/tr must parse; run `cargo test -p pinchora-game`"));

/// The glossary and the furniture nouns the Turkish names were built from; the tests hold the data to them.
pub const TR_GLOSSARY: &str = include_str!("../../../content/i18n/tr/_glossary.json");
pub const TR_FURNITURE_NOUNS: &str = include_str!("../../../content/i18n/tr/_furniture_nouns.json");

/// The overlay for a language; `None` for English, which is the content itself.
pub fn overlay(lang: Lang) -> Option<&'static Overlay> {
    match lang {
        Lang::En => None,
        Lang::Tr => Some(&TR),
    }
}

/// A game word in `lang`, or `english` when the language lacks it.
pub fn term(key: &str, english: &'static str, lang: Lang) -> &'static str {
    overlay(lang).and_then(|o| o.term(key)).unwrap_or(english)
}

/// A colour word (`Mint`, `Honey blonde`) in `lang`, or the name as given.
pub fn color_name(name: &str, lang: Lang) -> Cow<'_, str> {
    let key = name.to_lowercase().replace(' ', "_");
    match overlay(lang).and_then(|o| o.color(&key)) {
        Some(tr) => Cow::Owned(tr.to_owned()),
        None => Cow::Borrowed(name),
    }
}

// /English

/// What the English content says about an id.
struct English<'a> {
    name: Cow<'a, str>,
    description: Option<&'a str>,
}

fn named<'a>(name: &'a str, description: Option<&'a str>) -> Option<English<'a>> {
    Some(English {
        name: Cow::Borrowed(name),
        description,
    })
}

/// The English name and description of any content id (a colour variant and `~bound` included).
fn english<'a>(content: &'a Content, id: &str) -> Option<English<'a>> {
    if let Some(item) = content.item(id) {
        return Some(English {
            name: Cow::Owned(item.name()),
            description: Some(item.base.description.as_str()),
        });
    }
    let id = plain(id);
    if let Some(f) = content.furniture(id) {
        return named(&f.name, Some(&f.description));
    }
    if let Some(s) = content.sets.iter().find(|s| s.id == id) {
        return named(&s.name, Some(&s.description));
    }
    if let Some(s) = content.wallpaper(id).or_else(|| content.floor(id)) {
        return named(&s.name, None);
    }
    if let Some(r) = content.room_type(id) {
        return named(&r.name, Some(&r.description));
    }
    if let Some(p) = content.pet(id) {
        return named(&p.name, Some(&p.personality));
    }
    if let Some(f) = content.fish(id) {
        return named(&f.name, Some(&f.flavor));
    }
    if let Some(l) = content.look(id) {
        return named(&l.name, None);
    }
    if let Some(b) = content.background(id) {
        return named(&b.name, None);
    }
    if let Some(b) = content.badge(id) {
        return named(&b.name, Some(&b.description));
    }
    if let Some(t) = content.title(id) {
        return named(&t.name, Some(&t.description));
    }
    if let Some(s) = content.fishing.spot(id) {
        return named(&s.name, Some(&s.blurb));
    }
    if let Some(r) = content.fishing.rod(id) {
        return named(&r.name, Some(&r.blurb));
    }
    if let Some(b) = content.fishing.bait(id) {
        return named(&b.name, Some(&b.blurb));
    }
    if let Some(a) = character::accessory(id) {
        return named(&a.name, None);
    }
    if let Some(p) = character::preset(id) {
        return named(&p.name, None);
    }
    if let Some(e) = Collection::get().entry(id) {
        return named(&e.name, Some(&e.flavor));
    }
    if let Some(t) = crate::trips::get(id) {
        return named(&t.name, Some(&t.blurb));
    }
    if let Some(t) = contest::themes().iter().find(|t| t.id == id) {
        return named(&t.name, Some(&t.description));
    }
    None
}

// /Accessors

impl Content {
    /// The name players see for any content id (furniture and its colour variants, wallpapers, floors, sets,
    /// rooms, pets, fish, looks, backgrounds, badges, titles, fishing spots, rods and baits, accessories,
    /// presets, critters, finds, pet trips, contest themes). English when `lang` has no text for it; the id
    /// itself when it is unknown.
    pub fn name<'a>(&'a self, id: &'a str, lang: Lang) -> Cow<'a, str> {
        if let Some(name) = overlay(lang).and_then(|o| o.name_of(id)) {
            return name;
        }
        english(self, id).map_or(Cow::Borrowed(id), |e| e.name)
    }

    /// The description, flavour text, blurb or personality line of an id, if it has one. A colour variant
    /// describes like its base design.
    pub fn description<'a>(&'a self, id: &str, lang: Lang) -> Option<Cow<'a, str>> {
        let base = plain(id).split_once("__").map_or(plain(id), |(base, _)| base);
        if let Some(text) = overlay(lang)
            .and_then(|o| o.text(base))
            .and_then(|t| t.description.as_deref())
        {
            return Some(Cow::Borrowed(text));
        }
        english(self, base).and_then(|e| e.description).map(Cow::Borrowed)
    }

    /// A pet species' favourite food.
    pub fn pet_food<'a>(&'a self, pet_id: &str, lang: Lang) -> Option<Cow<'a, str>> {
        if let Some(food) = overlay(lang)
            .and_then(|o| o.text(pet_id))
            .and_then(|t| t.food.as_deref())
        {
            return Some(Cow::Borrowed(food));
        }
        self.pet(pet_id).map(|p| Cow::Borrowed(p.favorite_food.as_str()))
    }
}

impl Item<'_> {
    /// The name in `lang`: `Item::name` for English, the translated base name and colour otherwise.
    pub fn name_in(&self, lang: Lang) -> String {
        match overlay(lang).and_then(|o| o.name_of(&self.id())) {
            Some(name) => name.into_owned(),
            None => self.name(),
        }
    }
}

impl Trip {
    /// The story lines in `lang`; they pair up one to one with `stories`, so an index picks the same story.
    pub fn stories_in(&self, lang: Lang) -> &[String] {
        match overlay(lang).and_then(|o| o.text(&self.id)) {
            Some(t) if t.stories.len() == self.stories.len() => &t.stories,
            _ => &self.stories,
        }
    }
}

impl Rarity {
    pub fn label_in(self, lang: Lang) -> &'static str {
        let key = format!("rarity.{}", self.label().to_lowercase());
        term(&key, self.label(), lang)
    }
}

impl Category {
    pub fn label_in(self, lang: Lang) -> &'static str {
        term(&format!("category.{}", self.key()), self.label(), lang)
    }
}

impl Habitat {
    pub fn label_in(self, lang: Lang) -> &'static str {
        let key = match self {
            Self::Shore => "shore",
            Self::Tidepool => "tidepool",
            Self::Kelp => "kelp",
            Self::Reef => "reef",
            Self::Deep => "deep",
        };
        term(&format!("habitat.{key}"), self.label(), lang)
    }
}

impl Kind {
    pub fn label_in(self, lang: Lang) -> &'static str {
        let key = match self {
            Self::Critter => "critter",
            Self::Find => "find",
        };
        term(&format!("kind.{key}"), self.label(), lang)
    }
}

impl CollectionEntry {
    /// Where a critter lives, or "Washed up" for a find, in `lang`.
    pub fn place_in(&self, lang: Lang) -> &'static str {
        self.habitat
            .map_or_else(|| term("habitat.washed_up", "Washed up", lang), |h| h.label_in(lang))
    }
}

impl BadgeStat {
    fn key(self) -> &'static str {
        match self {
            Self::Level => "level",
            Self::FishSpecies => "fish_species",
            Self::HouseLikes => "house_likes",
            Self::Pets => "pets",
            Self::Streak => "streak",
            Self::Placed => "placed",
            Self::Reputation => "reputation",
            Self::Shells => "shells",
        }
    }

    pub fn label_in(self, lang: Lang) -> &'static str {
        term(&format!("stat.{}", self.key()), self.label(), lang)
    }

    pub fn noun_in(self, lang: Lang) -> &'static str {
        term(&format!("stat.{}.noun", self.key()), self.noun(), lang)
    }
}

impl Tier {
    /// The house tier's name in `lang`.
    pub fn name_in(&self, lang: Lang) -> &'static str {
        term(&format!("tier.{}", self.tier), self.name, lang)
    }
}

impl Quest {
    /// The quest's line in `lang`.
    pub fn text_in(&self, lang: Lang) -> &'static str {
        term(&format!("quest.{}", self.kind.key()), self.text, lang)
    }
}

/// The name of a character slot in `lang`.
pub fn slot_label(slot: Slot, lang: Lang) -> &'static str {
    term(&format!("slot.{}", slot.key()), slot.label(), lang)
}

/// A part of the day (`dawn`, `day`, `dusk`, `night`, `any`) in `lang`, capitalised.
pub fn time_label(time: &str, lang: Lang) -> Cow<'_, str> {
    word_label("time", time, lang)
}

/// A season (`spring` ... `winter`, `any`) in `lang`, capitalised.
pub fn season_label(season: &str, lang: Lang) -> Cow<'_, str> {
    word_label("season", season, lang)
}

fn word_label<'a>(kind: &str, word: &'a str, lang: Lang) -> Cow<'a, str> {
    match overlay(lang).and_then(|o| o.term(&format!("{kind}.{word}"))) {
        Some(tr) => Cow::Owned(tr.to_owned()),
        None => Cow::Borrowed(word),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use pinchora_core::games::items::soulbound;
    use pinchora_render::character::{
        ACCESSORY_TINTS, EYE_COLORS, HAIR_COLORS, OUTFIT_COLORS, SHOE_COLORS, SKIN_TONES,
    };
    use serde_json::Value;

    use crate::overlay::*;
    use crate::{fishing, house, quests};

    fn tr() -> &'static Overlay {
        overlay(Lang::Tr).unwrap()
    }

    fn content() -> &'static Content {
        Content::get()
    }

    /// Turkish lower-casing: `I` → `ı`, `İ` → `i`.
    fn lower(s: &str) -> String {
        s.chars()
            .flat_map(|c| match c {
                'I' => vec!['ı'],
                'İ' => vec!['i'],
                c => c.to_lowercase().collect(),
            })
            .collect()
    }

    /// An English id with its name and description.
    type Row = (String, String, Option<String>);

    /// Every English id with its name and description, per overlay file.
    fn english_by_file() -> Vec<(&'static str, Vec<Row>)> {
        let c = content();
        let d = |s: &str| Some(s.to_owned());
        let col = Collection::get();
        let rows = |entries: &[CollectionEntry]| -> Vec<Row> {
            entries
                .iter()
                .map(|e| (e.id.clone(), e.name.clone(), d(&e.flavor)))
                .collect()
        };
        vec![
            (
                "furniture",
                c.furniture
                    .iter()
                    .map(|f| (f.id.clone(), f.name.clone(), d(&f.description)))
                    .collect(),
            ),
            (
                "sets",
                c.sets
                    .iter()
                    .map(|s| (s.id.clone(), s.name.clone(), d(&s.description)))
                    .collect(),
            ),
            (
                "surfaces",
                c.surfaces
                    .wallpapers
                    .iter()
                    .chain(&c.surfaces.floors)
                    .map(|s| (s.id.clone(), s.name.clone(), None))
                    .collect(),
            ),
            (
                "rooms",
                c.rooms
                    .iter()
                    .map(|r| (r.id.clone(), r.name.clone(), d(&r.description)))
                    .collect(),
            ),
            (
                "pets",
                c.pets
                    .iter()
                    .map(|p| (p.id.clone(), p.name.clone(), d(&p.personality)))
                    .collect(),
            ),
            (
                "fish",
                c.fish
                    .iter()
                    .map(|f| (f.id.clone(), f.name.clone(), d(&f.flavor)))
                    .collect(),
            ),
            (
                "looks",
                c.looks.iter().map(|l| (l.id.clone(), l.name.clone(), None)).collect(),
            ),
            (
                "accessories",
                character::accessories()
                    .iter()
                    .map(|a| (a.id.clone(), a.name.clone(), None))
                    .collect(),
            ),
            (
                "presets",
                character::presets()
                    .iter()
                    .map(|p| (p.id.clone(), p.name.clone(), None))
                    .collect(),
            ),
            (
                "backgrounds",
                c.backgrounds
                    .iter()
                    .map(|b| (b.id.clone(), b.name.clone(), None))
                    .collect(),
            ),
            (
                "badges",
                c.badges
                    .iter()
                    .map(|b| (b.id.clone(), b.name.clone(), d(&b.description)))
                    .collect(),
            ),
            (
                "titles",
                c.titles
                    .iter()
                    .map(|t| (t.id.clone(), t.name.clone(), d(&t.description)))
                    .collect(),
            ),
            (
                "contest_themes",
                contest::themes()
                    .iter()
                    .map(|t| (t.id.clone(), t.name.clone(), d(&t.description)))
                    .collect(),
            ),
            ("critters", rows(&col.critters)),
            ("finds", rows(&col.finds)),
            (
                "pet_trips",
                crate::trips::all()
                    .iter()
                    .map(|t| (t.id.clone(), t.name.clone(), d(&t.blurb)))
                    .collect(),
            ),
            (
                "fishing_gear",
                c.fishing
                    .spots
                    .iter()
                    .map(|s| (s.id.clone(), s.name.clone(), d(&s.blurb)))
                    .chain(
                        c.fishing
                            .rods
                            .iter()
                            .map(|r| (r.id.clone(), r.name.clone(), d(&r.blurb))),
                    )
                    .chain(
                        c.fishing
                            .baits
                            .iter()
                            .map(|b| (b.id.clone(), b.name.clone(), d(&b.blurb))),
                    )
                    .collect(),
            ),
        ]
    }

    #[test]
    fn every_content_id_has_a_turkish_name_and_nothing_else_is_there() {
        for (file, english) in english_by_file() {
            let tr_ids: BTreeSet<&str> = tr().file(file).map(|(id, _)| id).collect();
            let en_ids: BTreeSet<&str> = english.iter().map(|(id, _, _)| id.as_str()).collect();
            assert_eq!(
                en_ids.difference(&tr_ids).collect::<Vec<_>>(),
                Vec::<&&str>::new(),
                "{file}: ids with no Turkish text"
            );
            assert_eq!(
                tr_ids.difference(&en_ids).collect::<Vec<_>>(),
                Vec::<&&str>::new(),
                "{file}: Turkish text for ids that do not exist"
            );
        }
    }

    #[test]
    fn texts_are_clean_and_fit_the_cards_and_selects() {
        for (file, english) in english_by_file() {
            let longest = english.iter().map(|(_, n, _)| n.chars().count()).max().unwrap_or(0);
            // English's longest name plus 30%, and two characters for the suffix a Turkish noun takes.
            let limit = longest * 13 / 10 + 2;
            let mut seen: HashSet<String> = HashSet::new();
            for (id, en_name, en_desc) in &english {
                let text = tr().text(id).unwrap_or_else(|| panic!("{file}/{id}"));
                for (what, s) in [("name", Some(&text.name)), ("description", text.description.as_ref())] {
                    let Some(s) = s else { continue };
                    assert!(!s.is_empty(), "{file}/{id}: empty {what}");
                    assert_eq!(s.trim(), s, "{file}/{id}: untrimmed {what}");
                    assert!(!s.contains('\n'), "{file}/{id}: multi-line {what}");
                    assert!(
                        s.chars().count() <= 100,
                        "{file}/{id}: {what} longer than a select label"
                    );
                }
                let n = text.name.chars().count();
                assert!(
                    n <= limit,
                    "{file}/{id}: name {:?} is {n} chars, limit {limit} (English {en_name:?})",
                    text.name
                );
                assert!(
                    seen.insert(lower(&text.name)),
                    "{file}/{id}: duplicate name {:?}",
                    text.name
                );
                assert_eq!(
                    text.description.is_some(),
                    en_desc.is_some(),
                    "{file}/{id}: description present in one language only"
                );
                if let (Some(tr_d), Some(en_d)) = (&text.description, en_desc) {
                    assert!(
                        tr_d.chars().count() <= en_d.chars().count() * 14 / 10 + 12,
                        "{file}/{id}: description far longer than the English"
                    );
                }
            }
        }
    }

    #[test]
    fn pets_have_a_food_and_trips_keep_their_stories() {
        for pet in &content().pets {
            let food = tr().text(&pet.id).and_then(|t| t.food.as_deref());
            assert!(
                food.is_some_and(|f| !f.is_empty() && f.chars().count() <= 40),
                "{}: food",
                pet.id
            );
        }
        for trip in crate::trips::all() {
            let text = tr().text(&trip.id).unwrap();
            assert_eq!(text.stories.len(), trip.stories.len(), "{}: story count", trip.id);
            for story in &text.stories {
                assert_eq!(story.matches("{pet}").count(), 1, "{}: {story}", trip.id);
                assert!(story.chars().count() <= 100, "{}: {story}", trip.id);
            }
            let seen: HashSet<&String> = text.stories.iter().collect();
            assert_eq!(seen.len(), text.stories.len(), "{}: repeated story", trip.id);
            assert_eq!(trip.stories_in(Lang::Tr), text.stories.as_slice());
            assert_eq!(trip.stories_in(Lang::En), trip.stories.as_slice());
        }
    }

    #[test]
    fn every_item_variant_has_a_distinct_composed_turkish_name() {
        let mut seen: HashMap<String, String> = HashMap::new();
        let mut count = 0;
        for item in content().items() {
            let name = item.name_in(Lang::Tr);
            let english = item.name();
            assert_ne!(name, english, "{}", item.id());
            if let Some(v) = item.variant {
                let colour = tr()
                    .color(v)
                    .unwrap_or_else(|| panic!("no Turkish word for colour {v}"));
                assert!(name.ends_with(&format!("({colour})")), "{name}");
                assert!(name.starts_with(&tr().text(&item.base.id).unwrap().name), "{name}");
            } else {
                assert_eq!(name, tr().text(&item.base.id).unwrap().name);
            }
            assert!(name.chars().count() <= 100);
            if let Some(other) = seen.insert(lower(&name), item.id()) {
                panic!("{name} names both {other} and {}", item.id());
            }
            count += 1;
        }
        assert!(count >= 1800, "{count} items");
    }

    #[test]
    fn every_colour_word_is_translated() {
        let mut names: BTreeSet<String> = BTreeSet::new();
        for list in [
            SKIN_TONES.as_slice(),
            HAIR_COLORS.as_slice(),
            OUTFIT_COLORS.as_slice(),
            EYE_COLORS.as_slice(),
            SHOE_COLORS.as_slice(),
            ACCESSORY_TINTS.as_slice(),
        ] {
            names.extend(list.iter().map(|s| s.name.to_owned()));
        }
        for f in &content().furniture {
            names.extend(f.variants.iter().cloned());
        }
        assert!(names.len() >= 55, "{} colour names", names.len());
        for name in names {
            let tr = color_name(&name, Lang::Tr);
            let same_word = ["Espresso"].contains(&name.as_str());
            assert!(same_word || tr != name, "{name} has no Turkish word");
            assert_eq!(color_name(&name, Lang::En), name);
        }
        assert_eq!(color_name("Honey blonde", Lang::Tr), "Bal Sarısı");
    }

    #[test]
    fn game_words_are_all_translated() {
        let tr = tr();
        let mut words: Vec<(String, String)> = Vec::new();
        for r in Rarity::ALL {
            words.push((r.label_in(Lang::Tr).to_owned(), r.label().to_owned()));
        }
        for c in Category::ALL {
            words.push((c.label_in(Lang::Tr).to_owned(), c.label().to_owned()));
        }
        for s in Slot::ALL {
            words.push((slot_label(s, Lang::Tr).to_owned(), s.label().to_owned()));
        }
        for h in [
            Habitat::Shore,
            Habitat::Tidepool,
            Habitat::Kelp,
            Habitat::Reef,
            Habitat::Deep,
        ] {
            words.push((h.label_in(Lang::Tr).to_owned(), h.label().to_owned()));
        }
        for k in [Kind::Critter, Kind::Find] {
            words.push((k.label_in(Lang::Tr).to_owned(), k.label().to_owned()));
        }
        for s in BadgeStat::ALL {
            words.push((s.label_in(Lang::Tr).to_owned(), s.label().to_owned()));
            words.push((s.noun_in(Lang::Tr).to_owned(), s.noun().to_owned()));
        }
        for t in house::TIERS {
            words.push((t.name_in(Lang::Tr).to_owned(), t.name.to_owned()));
        }
        for q in quests::POOL {
            let line = q.text_in(Lang::Tr);
            words.push((line.to_owned(), q.text.to_owned()));
            let numbers = |s: &str| s.split(|c: char| !c.is_ascii_digit()).filter(|n| !n.is_empty()).count();
            assert_eq!(numbers(line), numbers(q.text), "{line}: numbers differ from {}", q.text);
            assert!(line.contains(&q.target.to_string()) || q.target == 1, "{line}");
        }
        for (tr_word, en_word) in &words {
            assert_ne!(tr_word, en_word, "{en_word} is not translated");
            assert!(!tr_word.is_empty());
        }
        for key in ["time.any", "time.dawn", "time.day", "time.dusk", "time.night"] {
            assert!(tr.term(key).is_some(), "{key}");
        }
        for key in [
            "season.any",
            "season.spring",
            "season.summer",
            "season.autumn",
            "season.winter",
        ] {
            assert!(tr.term(key).is_some(), "{key}");
        }
        assert_eq!(time_label("dawn", Lang::Tr), "Şafak");
        assert_eq!(season_label("winter", Lang::En), "winter");
        assert_eq!(fishing::time_of_day(12), "day");
        // terms.json holds exactly the words the code above can ask for: no stray or missing keys.
        let mut expected: BTreeSet<String> = BTreeSet::new();
        expected.extend(Rarity::ALL.map(|r| format!("rarity.{}", r.label().to_lowercase())));
        expected.extend(Category::ALL.map(|c| format!("category.{}", c.key())));
        expected.extend(Slot::ALL.map(|s| format!("slot.{}", s.key())));
        expected.extend(["shore", "tidepool", "kelp", "reef", "deep", "washed_up"].map(|h| format!("habitat.{h}")));
        expected.extend(["critter", "find"].map(|k| format!("kind.{k}")));
        for s in BadgeStat::ALL {
            expected.insert(format!("stat.{}", s.key()));
            expected.insert(format!("stat.{}.noun", s.key()));
        }
        expected.extend(["any", "dawn", "day", "dusk", "night"].map(|t| format!("time.{t}")));
        expected.extend(["any", "spring", "summer", "autumn", "winter"].map(|t| format!("season.{t}")));
        expected.extend(house::TIERS.map(|t| format!("tier.{}", t.tier)));
        expected.extend(quests::POOL.map(|q| format!("quest.{}", q.kind.key())));
        expected.extend(["shells", "pearls", "fragments"].map(|c| format!("currency.{c}")));
        let present: BTreeSet<String> = tr
            .files
            .iter()
            .find(|(f, _)| *f == "terms")
            .unwrap()
            .1
            .keys()
            .cloned()
            .collect();
        assert_eq!(present, expected);
    }

    #[test]
    fn turkish_capitals_keep_the_dot_of_the_i() {
        assert_eq!(Lang::Tr.upper("Nadir"), "NADİR");
        assert_eq!(Lang::Tr.upper("Efsanevi"), "EFSANEVİ");
        assert_eq!(Lang::Tr.upper("Yaygın"), "YAYGIN");
        assert_eq!(Lang::Tr.upper("Kafe ve dükkân"), "KAFE VE DÜKKÂN");
        assert_eq!(Lang::En.upper("Rare"), "RARE");
    }

    #[test]
    fn the_accessors_fall_back_to_english() {
        let c = content();
        assert_eq!(c.name("hemloop_chair", Lang::En), "Hemloop Chair");
        assert_eq!(c.name("hemloop_chair", Lang::Tr), "Hemloop Sandalyesi");
        assert_eq!(c.name("pebblerest_stool__sand", Lang::En), "Pebblerest Stool (Sand)");
        assert_eq!(c.name("pebblerest_stool__sand", Lang::Tr), "Pebblerest Taburesi (Kum)");
        let bound = soulbound("pebblerest_stool__sand");
        assert_eq!(c.name(&bound, Lang::Tr), "Pebblerest Taburesi (Kum)");
        assert_eq!(c.name("no_such_thing", Lang::Tr), "no_such_thing");
        assert_eq!(c.name("no_such_thing", Lang::En), "no_such_thing");
        assert_eq!(c.name("pebblerest_stool__nope", Lang::Tr), "pebblerest_stool__nope");
        assert_eq!(c.name("shore", Lang::En), "Shore");
        assert_eq!(c.name("shore", Lang::Tr), "Kıyı");
        assert_eq!(
            c.description("hemloop_chair", Lang::En).as_deref(),
            Some("A woven seat that knows every breakfast story.")
        );
        assert_ne!(
            c.description("hemloop_chair", Lang::Tr),
            c.description("hemloop_chair", Lang::En)
        );
        assert_eq!(
            c.description("pebblerest_stool__sand", Lang::Tr),
            c.description("pebblerest_stool", Lang::Tr)
        );
        assert_eq!(
            c.description("creamcalm_plaster", Lang::Tr),
            None,
            "surfaces have no description"
        );
        assert_eq!(
            c.pet_food("crumbclaw_crab", Lang::En).as_deref(),
            Some("Kelp Crunchies")
        );
        assert_eq!(
            c.pet_food("crumbclaw_crab", Lang::Tr).as_deref(),
            Some("Deniz Yosunu Çıtırları")
        );
        assert_eq!(Rarity::Rare.label_in(Lang::Tr), "Nadir");
        assert_eq!(Rarity::Rare.label_in(Lang::En), "Rare");
        assert_eq!(Category::WallDecor.label_in(Lang::Tr), "Duvar süsleri");
        assert_eq!(house::tier(6).name_in(Lang::En), "Sunken Palace");
        assert_eq!(Lang::from_discord("tr"), Lang::Tr);
    }

    // /Glossary

    fn words(text: &str) -> String {
        let tokens: Vec<String> = text
            .split(|c: char| !c.is_alphabetic())
            .filter(|t| !t.is_empty())
            .map(str::to_lowercase)
            .collect();
        format!(" {} ", tokens.join(" "))
    }

    fn has_term(words: &str, term: &str) -> bool {
        ["", "s", "es"]
            .iter()
            .any(|plural| words.contains(&format!(" {term}{plural} ")))
    }

    #[test]
    fn the_glossary_is_respected() {
        let glossary: Value = serde_json::from_str(TR_GLOSSARY).unwrap();
        let terms = glossary["terms"].as_array().unwrap();
        let exempt = glossary["exempt"].as_object().unwrap();
        assert!(terms.len() >= 100, "{} glossary terms", terms.len());
        for id in exempt.keys() {
            assert!(tr().text(id).is_some(), "exempt id {id} does not exist");
        }
        let mut checked = 0;
        for (file, english) in english_by_file() {
            for (id, en_name, en_desc) in &english {
                if exempt.contains_key(id) {
                    continue;
                }
                let text = tr().text(id).unwrap();
                for term in terms {
                    let en = term["en"].as_str().unwrap();
                    let stems: Vec<&str> = term["match"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|m| m.as_str().unwrap())
                        .collect();
                    let all = term["scope"] == "all";
                    let pairs = [
                        (Some(en_name.as_str()), Some(text.name.as_str())),
                        (en_desc.as_deref().filter(|_| all), text.description.as_deref()),
                    ];
                    for (e, t) in pairs {
                        let (Some(e), Some(t)) = (e, t) else { continue };
                        if has_term(&words(e), en) {
                            checked += 1;
                            let t = lower(t);
                            assert!(
                                stems.iter().any(|s| t.contains(s)),
                                "{file}/{id}: English {e:?} has \"{en}\", Turkish {t:?} lacks {stems:?}"
                            );
                        }
                    }
                }
            }
        }
        assert!(checked > 150, "only {checked} glossary checks ran");
    }

    /// Real English adjectives that open a furniture name: translated, not coined.
    const TRANSLATED_FIRST_WORDS: [(&str, &str); 1] = [("Revolving", "Dönen")];

    #[test]
    fn furniture_names_are_the_coined_word_plus_the_agreed_noun() {
        let nouns: Value = serde_json::from_str(TR_FURNITURE_NOUNS).unwrap();
        let nouns = nouns.as_object().unwrap();
        let mut used: HashSet<&str> = HashSet::new();
        for f in &content().furniture {
            let (first, tail) = f.name.split_once(' ').unwrap();
            let first = TRANSLATED_FIRST_WORDS
                .iter()
                .find(|(english, _)| *english == first)
                .map_or(first, |(_, turkish)| *turkish);
            let entry = &nouns[tail];
            used.insert(tail);
            let noun = match entry {
                Value::String(s) => s.as_str(),
                Value::Object(by_category) => by_category
                    .get(f.category.key())
                    .or_else(|| by_category.get("*"))
                    .and_then(Value::as_str)
                    .unwrap(),
                other => panic!("{tail}: {other}"),
            };
            assert_eq!(tr().text(&f.id).unwrap().name, format!("{first} {noun}"), "{}", f.id);
        }
        let all: HashSet<&str> = nouns.keys().map(String::as_str).collect();
        assert_eq!(all, used, "_furniture_nouns.json lists a tail no design uses");
    }

    #[test]
    fn no_english_word_is_left_in_a_turkish_description() {
        // Function words that never appear in the Turkish lines.
        let stop = [
            " the ", " and ", " with ", " for ", " your ", " that ", " into ", " every ",
        ];
        for (file, english) in english_by_file() {
            for (id, _, _) in &english {
                let text = tr().text(id).unwrap();
                for s in [Some(text.name.as_str()), text.description.as_deref()]
                    .into_iter()
                    .flatten()
                {
                    let w = words(s);
                    for word in stop {
                        assert!(!w.contains(word), "{file}/{id}: English word {word:?} in {s:?}");
                    }
                }
            }
        }
    }
}
