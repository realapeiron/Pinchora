use super::*;

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
    pub(super) fn key(self) -> &'static str {
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
