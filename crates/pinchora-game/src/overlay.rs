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

mod accessors;
mod english;
mod translations;

pub use accessors::{season_label, slot_label, time_label};
use english::english;
pub use translations::{Overlay, TR_FURNITURE_NOUNS, TR_GLOSSARY, Text, color_name, overlay, term};

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
