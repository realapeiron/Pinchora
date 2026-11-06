//! Player characters: a look from `content/looks.json`, six colours and up to one accessory per
//! slot from `content/accessories.json`, saved as text. Ready-made characters come from
//! `content/character_presets.json`.

use std::sync::LazyLock;

use pinchora_core::stats::WardrobeFacts;
use pinchora_render::character::{ACCESSORY_TINTS, Channel, CharacterSpec, Colors, Paint, Pose, Slot, Worn};
use serde::Deserialize;

use crate::Content;

/// A designed character. Saved as `"<look>.<skin>.<hair>.<top>.<bottom>.<eyes>.<shoes>"` and an
/// optional `+<accessory>` (or `+<accessory>*<tint>`) for each accessory, e.g.
/// `tidewalker.1.1.0.2.7.0+straw_hat*3+balloon`. A colour is a swatch number or `#rrggbb`.
/// Codes saved before eyes and shoes existed (five parts) still read, with both as drawn.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Character {
    pub look: String,
    pub colors: Colors,
    /// At most one per slot, in slot order.
    pub accessories: Vec<Worn>,
}

/// One accessory a player can wear, from `content/accessories.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct Accessory {
    pub id: String,
    pub name: String,
    pub slot: Slot,
    /// Whether it can be dyed.
    pub tintable: bool,
    /// Shells to buy it; 0 means it is free once any requirement below is met.
    pub price: i64,
    /// The player level that opens it.
    #[serde(default)]
    pub level: Option<u32>,
    /// The badge that opens it.
    #[serde(default)]
    pub badge: Option<String>,
}

/// Whether a player may wear an accessory, and if not, what stands in the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    Owned,
    /// For sale at this many shells.
    Buy(i64),
    /// Opens at this level.
    Level(u32),
    /// Opens with this badge (its id).
    Badge(String),
}

impl Accessory {
    /// `bought`: it is in the player's `user_accessories`.
    pub fn access(&self, bought: bool, level: u32, has_badge: impl Fn(&str) -> bool) -> Access {
        if bought {
            return Access::Owned;
        }
        if let Some(at) = self.level.filter(|&at| level < at) {
            return Access::Level(at);
        }
        if let Some(badge) = self.badge.as_ref().filter(|b| !has_badge(b)) {
            return Access::Badge(badge.clone());
        }
        if self.price > 0 {
            Access::Buy(self.price)
        } else {
            Access::Owned
        }
    }
}

/// What a player may wear: what they bought, their level and the badges they have. The rule
/// behind the editor's Buy and Save, for any front end: `Db::wardrobe_facts` reads the data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shelf {
    /// Accessory ids they bought.
    pub bought: Vec<String>,
    pub level: u32,
    /// Ids of the badges they have, achievements and shop badges alike.
    pub badges: Vec<String>,
}

impl Shelf {
    pub fn new(content: &Content, facts: &WardrobeFacts) -> Self {
        Self {
            bought: facts.bought_accessories.clone(),
            level: u32::try_from(facts.stats.level).unwrap_or(0),
            badges: content
                .earned_badges(&facts.stats, &facts.bought_badges)
                .into_iter()
                .map(|b| b.id.clone())
                .collect(),
        }
    }

    pub fn access(&self, item: &Accessory) -> Access {
        item.access(self.bought.contains(&item.id), self.level, |b| {
            self.badges.iter().any(|x| x == b)
        })
    }

    /// The first thing worn that the player may not wear yet.
    pub fn blocker(&self, character: &Character) -> Option<(&'static Accessory, Access)> {
        character.accessories.iter().find_map(|w| {
            let item = accessory(&w.id)?;
            match self.access(item) {
                Access::Owned => None,
                other => Some((item, other)),
            }
        })
    }
}

static ACCESSORIES: LazyLock<Vec<Accessory>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../content/accessories.json"))
        .expect("embedded accessories must parse; run `cargo test -p pinchora-game`")
});

/// Every accessory, in the order of `content/accessories.json` (free ones first in each slot).
pub fn accessories() -> &'static [Accessory] {
    &ACCESSORIES
}

pub fn accessory(id: &str) -> Option<&'static Accessory> {
    ACCESSORIES.iter().find(|a| a.id == id)
}

/// The accessories of one slot.
pub fn accessories_in(slot: Slot) -> impl Iterator<Item = &'static Accessory> {
    ACCESSORIES.iter().filter(move |a| a.slot == slot)
}

/// A ready-made character from `content/character_presets.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub code: String,
}

static PRESETS: LazyLock<Vec<Preset>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../content/character_presets.json"))
        .expect("embedded presets must parse; run `cargo test -p pinchora-game`")
});

pub fn presets() -> &'static [Preset] {
    &PRESETS
}

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

impl Preset {
    pub fn character(&self, content: &Content) -> Option<Character> {
        Character::decode(content, &self.code)
    }
}

/// How many saved outfit slots a player has.
pub const OUTFIT_SLOTS: i16 = 3;
/// Longest outfit name.
pub const OUTFIT_NAME_MAX: usize = 32;

impl Character {
    /// Where the editor starts for a player without a saved character.
    pub fn starter(content: &Content) -> Self {
        Self {
            look: content.looks.first().map_or_else(String::new, |l| l.id.clone()),
            colors: Colors {
                skin: Paint::Swatch(1),
                hair: Paint::Swatch(1),
                eyes: Paint::Swatch(0),
                top: Paint::Swatch(0),
                bottom: Paint::Swatch(2),
                shoes: Paint::Swatch(0),
            },
            accessories: Vec::new(),
        }
    }

    /// What the renderer draws, standing as drawn.
    pub fn spec(&self) -> CharacterSpec {
        CharacterSpec {
            look: self.look.clone(),
            colors: self.colors,
            accessories: self.accessories.clone(),
            pose: Pose::default(),
        }
    }

    pub fn encode(&self) -> String {
        let c = self.colors;
        let mut code = format!(
            "{}.{}.{}.{}.{}.{}.{}",
            self.look,
            c.skin.code(),
            c.hair.code(),
            c.top.code(),
            c.bottom.code(),
            c.eyes.code(),
            c.shoes.code()
        );
        for worn in &self.accessories {
            code.push('+');
            code.push_str(&worn.id);
            if let Some(tint) = worn.tint {
                code.push('*');
                code.push_str(&tint.code());
            }
        }
        code
    }

    /// Parses a saved code; unknown looks or accessories, colours out of range, a dye on an
    /// accessory that takes none and two accessories in one slot all give `None`.
    pub fn decode(content: &Content, code: &str) -> Option<Self> {
        let mut pieces = code.split('+');
        let mut parts = pieces.next()?.split('.');
        let look = content.look(parts.next()?)?.id.clone();
        let mut paint = |channel: Channel| Paint::parse(parts.next()?, channel.swatches().len());
        let (skin, hair, top, bottom) = (
            paint(Channel::Skin)?,
            paint(Channel::Hair)?,
            paint(Channel::Top)?,
            paint(Channel::Bottom)?,
        );
        // Five-part codes are from before eyes and shoes.
        let rest: Vec<&str> = parts.collect();
        let (eyes, shoes) = match rest.as_slice() {
            [] => (Paint::Swatch(0), Paint::Swatch(0)),
            [eyes, shoes] => (
                Paint::parse(eyes, Channel::Eyes.swatches().len())?,
                Paint::parse(shoes, Channel::Shoes.swatches().len())?,
            ),
            _ => return None,
        };
        let mut character = Self {
            look,
            colors: Colors {
                skin,
                hair,
                eyes,
                top,
                bottom,
                shoes,
            },
            accessories: Vec::new(),
        };
        for piece in pieces {
            let (id, tint) = match piece.split_once('*') {
                Some((id, tint)) => (id, Some(Paint::parse(tint, ACCESSORY_TINTS.len())?)),
                None => (piece, None),
            };
            let item = accessory(id)?;
            if (tint.is_some() && !item.tintable) || character.worn(item.slot).is_some() {
                return None;
            }
            character.accessories.push(Worn {
                id: item.id.clone(),
                tint,
            });
        }
        character.sort();
        Some(character)
    }

    fn sort(&mut self) {
        self.accessories
            .sort_by_key(|w| accessory(&w.id).map_or(u8::MAX, |a| a.slot as u8));
    }

    /// What is worn in `slot`.
    pub fn worn(&self, slot: Slot) -> Option<&Worn> {
        self.accessories
            .iter()
            .find(|w| accessory(&w.id).is_some_and(|a| a.slot == slot))
    }

    /// Puts `id` in its slot (replacing what was there), or empties `slot` with `None`.
    pub fn wear(&mut self, slot: Slot, id: Option<&str>, tint: Option<Paint>) {
        self.accessories
            .retain(|w| accessory(&w.id).is_none_or(|a| a.slot != slot));
        if let Some(item) = id.and_then(accessory).filter(|a| a.slot == slot) {
            self.accessories.push(Worn {
                id: item.id.clone(),
                tint: tint.filter(|_| item.tintable),
            });
            self.sort();
        }
    }
}

#[cfg(test)]
mod tests {
    use pinchora_render::character::{ACCESSORIES, OUTFIT_COLORS, SKIN_TONES, accessory as art};

    use super::*;

    #[test]
    fn codes_round_trip() {
        let content = Content::get();
        let starter = Character::starter(content);
        assert!(content.look(&starter.look).is_some());
        assert_eq!(Character::decode(content, &starter.encode()), Some(starter.clone()));

        let last = content.looks.last().expect("looks exist").id.clone();
        let mut far = Character {
            look: last,
            colors: Colors {
                skin: Paint::Swatch(SKIN_TONES.len() as u8 - 1),
                hair: Paint::Custom(0x12ab3f),
                eyes: Paint::Swatch(19),
                top: Paint::Swatch(OUTFIT_COLORS.len() as u8 - 1),
                bottom: Paint::Custom(0xffffff),
                shoes: Paint::Swatch(7),
            },
            accessories: Vec::new(),
        };
        far.wear(Slot::Hand, Some("balloon"), Some(Paint::Swatch(15)));
        far.wear(Slot::Head, Some("crown"), Some(Paint::Swatch(3)));
        far.wear(Slot::Face, Some("round_glasses"), Some(Paint::Custom(0x101010)));
        let code = far.encode();
        assert_eq!(Character::decode(content, &code), Some(far.clone()), "{code}");
        assert!(
            code.contains("+crown") && !code.contains("crown*"),
            "the crown takes no dye: {code}"
        );
        assert!(code.len() <= 120, "{code}");
        // The worn things come out in slot order, whatever order they were put on.
        let slots: Vec<Slot> = far.accessories.iter().map(|w| accessory(&w.id).unwrap().slot).collect();
        assert!(slots.windows(2).all(|p| p[0] < p[1]), "{slots:?}");
    }

    #[test]
    fn old_codes_still_read() {
        let content = Content::get();
        let old = Character::decode(content, "curlcove.2.3.1.5").expect("a five-part code");
        assert_eq!(old.colors.skin, Paint::Swatch(2));
        assert_eq!(
            (old.colors.eyes, old.colors.shoes),
            (Paint::Swatch(0), Paint::Swatch(0))
        );
        assert!(old.accessories.is_empty());
    }

    #[test]
    fn bad_codes_are_refused() {
        let content = Content::get();
        let look = &content.looks[0].id;
        for code in [
            "",
            "tidewalker",
            "nobody.0.0.0.0",
            &format!("{look}.20.0.0.0"),
            &format!("{look}.0.20.0.0"),
            &format!("{look}.0.0.20.0"),
            &format!("{look}.0.0.0.20"),
            &format!("{look}.0.0.0"),
            &format!("{look}.0.0.0.0.0"),
            &format!("{look}.0.0.0.0.0.0.0"),
            &format!("{look}.0.0.0.0.20.0"),
            &format!("{look}.-1.0.0.0"),
            &format!("{look}.+1.0.0.0"),
            &format!("{look}.a.0.0.0"),
            &format!("{look}..0.0.0"),
            &format!("{look}.0.0.0.0 "),
            &format!("{look}.#12345.0.0.0"),
            &format!("{look}.0.0.0.0.0.0+nothing"),
            &format!("{look}.0.0.0.0.0.0+crown*1"),
            &format!("{look}.0.0.0.0.0.0+beanie*16"),
            &format!("{look}.0.0.0.0.0.0+beanie+crown"),
            &format!("{look}.0.0.0.0.0.0+"),
        ] {
            assert_eq!(Character::decode(content, code), None, "{code:?}");
        }
    }

    #[test]
    fn wearing_replaces_the_slot() {
        let content = Content::get();
        let mut c = Character::starter(content);
        c.wear(Slot::Head, Some("beanie"), Some(Paint::Swatch(2)));
        c.wear(Slot::Head, Some("crown"), Some(Paint::Swatch(2)));
        assert_eq!(c.accessories.len(), 1);
        assert_eq!(
            c.worn(Slot::Head).map(|w| (w.id.as_str(), w.tint)),
            Some(("crown", None))
        );
        c.wear(Slot::Head, Some("scarf"), None);
        assert!(
            c.worn(Slot::Head).is_none(),
            "a scarf is not a hat, so the slot is emptied"
        );
        c.wear(Slot::Head, None, None);
        assert!(c.accessories.is_empty());
    }

    #[test]
    fn every_accessory_has_art_and_access_rules_make_sense() {
        let content = Content::get();
        assert!((30..=40).contains(&accessories().len()));
        assert_eq!(
            accessories().len(),
            ACCESSORIES.len(),
            "content and art list the same accessories"
        );
        for a in accessories() {
            let drawn = art(&a.id).unwrap_or_else(|| panic!("{} has no art", a.id));
            assert_eq!(drawn.slot, a.slot, "{}", a.id);
            assert_eq!(drawn.tintable, a.tintable, "{}", a.id);
            assert!(a.name.chars().count() <= 20 && a.price >= 0, "{}", a.id);
            if let Some(badge) = &a.badge {
                assert!(content.badge(badge).is_some(), "{}: no badge {badge}", a.id);
            }
            assert_eq!(a.access(true, 1, |_| false), Access::Owned);
        }
        for slot in Slot::ALL {
            let mut free = accessories_in(slot).filter(|a| a.access(false, 1, |_| false) == Access::Owned);
            assert!(free.next().is_some(), "{slot:?} has nothing free");
        }
        let crown = accessory("crown").expect("a crown");
        assert_eq!(crown.access(false, 29, |_| true), Access::Level(30));
        assert_eq!(crown.access(false, 30, |_| false), Access::Owned);
        let rod = accessory("fishing_rod").expect("a rod");
        assert!(matches!(rod.access(false, 50, |_| false), Access::Badge(_)));
        assert_eq!(rod.access(false, 1, |b| b == "fin_finder"), Access::Owned);
        let captain = accessory("captain_hat").expect("a captain's hat");
        assert_eq!(captain.access(false, 9, |_| true), Access::Level(10));
        assert_eq!(captain.access(false, 10, |_| true), Access::Buy(1200));
    }

    #[test]
    fn the_shelf_works_out_what_a_player_may_wear_from_the_database_facts() {
        use pinchora_core::stats::{PlayerStats, WardrobeFacts};

        use crate::{BadgeKind, BadgeStat};

        let content = Content::get();
        let finder = content.badge("fin_finder").expect("the rod's badge");
        let BadgeKind::Achievement { stat, at } = finder.kind else {
            panic!("fin_finder is an achievement");
        };
        let mut facts = WardrobeFacts {
            stats: PlayerStats {
                level: 12,
                ..PlayerStats::default()
            },
            ..WardrobeFacts::default()
        };
        let mut character = Character::starter(content);
        character.wear(Slot::Head, Some("captain_hat"), None);
        character.wear(Slot::Hand, Some("fishing_rod"), None);

        let shelf = Shelf::new(content, &facts);
        assert_eq!(shelf.level, 12);
        assert!(!shelf.badges.iter().any(|b| b == "fin_finder"));
        let (blocked, access) = shelf.blocker(&character).expect("two things are not yet theirs");
        assert_eq!((blocked.id.as_str(), access), ("captain_hat", Access::Buy(1200)));

        // Bought, the hat is theirs; the rod still waits for its badge.
        facts.bought_accessories.push("captain_hat".to_owned());
        let shelf = Shelf::new(content, &facts);
        let (blocked, access) = shelf.blocker(&character).expect("the rod is still locked");
        assert_eq!(blocked.id, "fishing_rod");
        assert!(matches!(access, Access::Badge(id) if id == "fin_finder"));

        // The badge comes from the numbers the database counted.
        match stat {
            BadgeStat::FishSpecies => facts.stats.fish_species = at,
            other => panic!("update this test: fin_finder counts {other:?}"),
        }
        let shelf = Shelf::new(content, &facts);
        assert!(shelf.badges.iter().any(|b| b == "fin_finder"));
        assert!(shelf.blocker(&character).is_none());
    }

    #[test]
    fn presets_are_ten_real_characters() {
        let content = Content::get();
        assert_eq!(presets().len(), 10);
        for (i, p) in presets().iter().enumerate() {
            let c = p
                .character(content)
                .unwrap_or_else(|| panic!("{}: bad code {}", p.id, p.code));
            assert!(!c.accessories.is_empty(), "{}", p.id);
            assert_eq!(c.encode(), p.code, "{}: write the code the way it is saved", p.id);
            assert!(presets()[..i].iter().all(|q| q.id != p.id && q.name != p.name));
            assert_eq!(preset(&p.id).map(|q| q.name.as_str()), Some(p.name.as_str()));
        }
        assert!(presets().iter().any(|p| p.name == "Beach Surfer"));
    }
}
