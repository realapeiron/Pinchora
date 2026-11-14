use super::*;

// /Constants

/// Files that map a content id to its text, per language; ids are unique across all of them.
const TR_FILES: [(&str, &str); 19] = [
    ("furniture", include_str!("../../../../content/i18n/tr/furniture.json")),
    ("sets", include_str!("../../../../content/i18n/tr/sets.json")),
    ("surfaces", include_str!("../../../../content/i18n/tr/surfaces.json")),
    ("rooms", include_str!("../../../../content/i18n/tr/rooms.json")),
    ("pets", include_str!("../../../../content/i18n/tr/pets.json")),
    ("fish", include_str!("../../../../content/i18n/tr/fish.json")),
    ("looks", include_str!("../../../../content/i18n/tr/looks.json")),
    (
        "accessories",
        include_str!("../../../../content/i18n/tr/accessories.json"),
    ),
    ("presets", include_str!("../../../../content/i18n/tr/presets.json")),
    (
        "backgrounds",
        include_str!("../../../../content/i18n/tr/backgrounds.json"),
    ),
    ("badges", include_str!("../../../../content/i18n/tr/badges.json")),
    ("titles", include_str!("../../../../content/i18n/tr/titles.json")),
    (
        "contest_themes",
        include_str!("../../../../content/i18n/tr/contest_themes.json"),
    ),
    ("critters", include_str!("../../../../content/i18n/tr/critters.json")),
    ("finds", include_str!("../../../../content/i18n/tr/finds.json")),
    ("pet_trips", include_str!("../../../../content/i18n/tr/pet_trips.json")),
    (
        "fishing_gear",
        include_str!("../../../../content/i18n/tr/fishing_gear.json"),
    ),
    ("terms", include_str!("../../../../content/i18n/tr/terms.json")),
    ("colors", include_str!("../../../../content/i18n/tr/colors.json")),
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
    pub(super) files: Vec<(&'static str, HashMap<String, Text>)>,
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
    pub(super) fn name_of(&self, id: &str) -> Option<Cow<'_, str>> {
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
pub const TR_GLOSSARY: &str = include_str!("../../../../content/i18n/tr/_glossary.json");
pub const TR_FURNITURE_NOUNS: &str = include_str!("../../../../content/i18n/tr/_furniture_nouns.json");

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
