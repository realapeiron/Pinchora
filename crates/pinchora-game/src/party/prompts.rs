//! The questions of Would You Rather and the truths and dares of Truth or Dare, from `content/party.json`.
//! Everything in it is written to be family friendly.

use std::sync::LazyLock;

use pinchora_core::{i18n::Lang, t};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Silly,
    Food,
    Adventure,
    Ocean,
    Brainy,
    Cozy,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Self::Silly,
        Self::Food,
        Self::Adventure,
        Self::Ocean,
        Self::Brainy,
        Self::Cozy,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Silly => "silly",
            Self::Food => "food",
            Self::Adventure => "adventure",
            Self::Ocean => "ocean",
            Self::Brainy => "brainy",
            Self::Cozy => "cozy",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Silly => "Silly",
            Self::Food => "Food",
            Self::Adventure => "Adventure",
            Self::Ocean => "Ocean",
            Self::Brainy => "Brainy",
            Self::Cozy => "Cozy",
        }
    }

    /// The label in `lang` (`party.topic.<key>` in the catalogs); in English it is [`Category::label`].
    pub fn label_in(self, lang: Lang) -> String {
        t!(lang, &format!("party.topic.{}", self.key()))
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }
}

/// A would-you-rather question: two things to choose between.
#[derive(Debug, Clone, Deserialize)]
pub struct Rather {
    pub id: String,
    pub category: Category,
    pub a: String,
    pub b: String,
}

/// A truth to answer or a dare to do.
#[derive(Debug, Clone, Deserialize)]
pub struct Prompt {
    pub id: String,
    pub category: Category,
    pub text: String,
}

#[derive(Deserialize)]
pub struct PartyContent {
    pub wyr: Vec<Rather>,
    pub truths: Vec<Prompt>,
    pub dares: Vec<Prompt>,
}

static EMBEDDED: LazyLock<PartyContent> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../../content/party.json"))
        .expect("embedded party content must parse; run `cargo test -p pinchora-game`")
});

static EMBEDDED_TR: LazyLock<PartyContent> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../../content/party_tr.json"))
        .expect("embedded Turkish party content must parse; run `cargo test -p pinchora-game`")
});

/// Truth or dare: what the player picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Truth,
    Dare,
}

impl Pick {
    pub fn key(self) -> &'static str {
        match self {
            Self::Truth => "truth",
            Self::Dare => "dare",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "truth" => Some(Self::Truth),
            "dare" => Some(Self::Dare),
            _ => None,
        }
    }
}

impl PartyContent {
    pub fn get() -> &'static PartyContent {
        &EMBEDDED
    }

    /// The content for `lang`.
    pub fn for_lang(lang: Lang) -> &'static PartyContent {
        match lang {
            Lang::En => &EMBEDDED,
            Lang::Tr => &EMBEDDED_TR,
        }
    }

    /// A question of any language by its id (a running poll only remembers the id).
    pub fn find_rather(id: &str) -> Option<&'static Rather> {
        Lang::ALL.into_iter().find_map(|lang| Self::for_lang(lang).rather(id))
    }

    /// A truth or dare of any language by its id.
    pub fn find_prompt(id: &str) -> Option<&'static Prompt> {
        Lang::ALL.into_iter().find_map(|lang| Self::for_lang(lang).prompt(id))
    }

    pub fn rather(&self, id: &str) -> Option<&Rather> {
        self.wyr.iter().find(|q| q.id == id)
    }

    pub fn prompt(&self, id: &str) -> Option<&Prompt> {
        self.truths.iter().chain(&self.dares).find(|p| p.id == id)
    }

    /// The pool of truths or dares, in `category` when one is asked for.
    pub fn prompts(&self, pick: Pick, category: Option<Category>) -> Vec<&Prompt> {
        let all = match pick {
            Pick::Truth => &self.truths,
            Pick::Dare => &self.dares,
        };
        all.iter()
            .filter(|p| category.is_none_or(|c| p.category == c))
            .collect()
    }

    pub fn questions(&self, category: Option<Category>) -> Vec<&Rather> {
        self.wyr
            .iter()
            .filter(|q| category.is_none_or(|c| q.category == c))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::party::filter::is_profane_for;

    #[test]
    fn there_are_about_two_hundred_of_each_in_every_category() {
        for lang in Lang::ALL {
            each_category(PartyContent::for_lang(lang));
        }
    }

    fn each_category(c: &PartyContent) {
        assert!(c.wyr.len() >= 200, "{} would-you-rather questions", c.wyr.len());
        assert!(
            c.truths.len() + c.dares.len() >= 200,
            "{} truths and dares",
            c.truths.len() + c.dares.len()
        );
        for category in Category::ALL {
            assert!(c.questions(Some(category)).len() >= 25, "{category:?}");
            assert!(c.prompts(Pick::Truth, Some(category)).len() >= 10, "{category:?}");
            assert!(c.prompts(Pick::Dare, Some(category)).len() >= 10, "{category:?}");
        }
        assert_eq!(c.questions(None).len(), c.wyr.len());
        assert_eq!(c.prompts(Pick::Dare, None).len(), c.dares.len());
    }

    #[test]
    fn entries_are_well_formed_unique_and_clean_even_for_strict_servers() {
        let mut ids = HashSet::new();
        let mut texts = HashSet::new();
        for lang in Lang::ALL {
            clean_entries(PartyContent::for_lang(lang), lang, &mut ids, &mut texts);
        }
    }

    fn clean_entries(c: &PartyContent, lang: Lang, ids: &mut HashSet<String>, texts: &mut HashSet<String>) {
        for q in &c.wyr {
            assert!(ids.insert(q.id.clone()), "duplicate id {}", q.id);
            assert!(q.id.starts_with(&format!("wyr_{}_", q.category.key())), "{}", q.id);
            assert!(
                texts.insert(format!("{}|{}", q.a, q.b).to_lowercase()),
                "{}: duplicate",
                q.id
            );
            for side in [&q.a, &q.b] {
                assert!((8..=60).contains(&side.chars().count()), "{}: {side}", q.id);
                assert!(!is_profane_for(side, true, lang), "{}: {side}", q.id);
            }
            assert_ne!(q.a.to_lowercase(), q.b.to_lowercase(), "{}", q.id);
        }
        for (prompts, kind) in [(&c.truths, "truth"), (&c.dares, "dare")] {
            for p in prompts {
                assert!(ids.insert(p.id.clone()), "duplicate id {}", p.id);
                assert!(p.id.starts_with(&format!("{kind}_{}_", p.category.key())), "{}", p.id);
                assert!(texts.insert(p.text.to_lowercase()), "{}: duplicate", p.id);
                assert!((15..=110).contains(&p.text.chars().count()), "{}: {}", p.id, p.text);
                assert!(!is_profane_for(&p.text, true, lang), "{}: {}", p.id, p.text);
                assert!(p.text.ends_with('.') || p.text.ends_with('?'), "{}: {}", p.id, p.text);
            }
        }
    }

    #[test]
    fn lookups_find_what_the_pools_hold() {
        let c = PartyContent::get();
        let q = &c.wyr[3];
        assert_eq!(c.rather(&q.id).map(|r| r.a.as_str()), Some(q.a.as_str()));
        assert!(c.rather("nope").is_none());
        assert!(c.prompt(&c.dares[0].id).is_some() && c.prompt(&c.truths[0].id).is_some());
        assert_eq!(Pick::from_key("dare"), Some(Pick::Dare));
        assert_eq!(Pick::from_key("nope"), None);
        assert_eq!(Category::from_key("ocean"), Some(Category::Ocean));
    }

    #[test]
    fn topics_are_named_in_every_language() {
        for category in Category::ALL {
            assert_eq!(category.label_in(Lang::En), category.label());
            assert_ne!(category.label_in(Lang::Tr), category.label());
        }
        assert_eq!(Category::Silly.label_in(Lang::Tr), "Saçma");
    }
}
