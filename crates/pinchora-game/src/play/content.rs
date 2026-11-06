//! Trivia questions, hangman words and wordle lists, loaded from `content/*.json`.

use std::{collections::HashSet, sync::LazyLock};

use pinchora_core::i18n::Lang;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriviaCategory {
    Ocean,
    Animals,
    Nature,
    Science,
    Geography,
    Food,
    History,
    Home,
}

impl TriviaCategory {
    pub const ALL: [TriviaCategory; 8] = [
        Self::Ocean,
        Self::Animals,
        Self::Nature,
        Self::Science,
        Self::Geography,
        Self::Food,
        Self::History,
        Self::Home,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Ocean => "ocean",
            Self::Animals => "animals",
            Self::Nature => "nature",
            Self::Science => "science",
            Self::Geography => "geography",
            Self::Food => "food",
            Self::History => "history",
            Self::Home => "home",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ocean => "Ocean",
            Self::Animals => "Animals",
            Self::Nature => "Nature",
            Self::Science => "Science",
            Self::Geography => "Geography",
            Self::Food => "Food",
            Self::History => "History",
            Self::Home => "Home",
        }
    }

    /// The label in `lang`.
    pub fn label_in(self, lang: Lang) -> &'static str {
        if lang == Lang::En {
            return self.label();
        }
        match self {
            Self::Ocean => "Okyanus",
            Self::Animals => "Hayvanlar",
            Self::Nature => "Doğa",
            Self::Science => "Bilim",
            Self::Geography => "Coğrafya",
            Self::Food => "Yemek",
            Self::History => "Tarih",
            Self::Home => "Ev",
        }
    }

    pub fn emoji(self) -> &'static str {
        match self {
            Self::Ocean => "🌊",
            Self::Animals => "🦊",
            Self::Nature => "🌿",
            Self::Science => "🔬",
            Self::Geography => "🗺️",
            Self::Food => "🍰",
            Self::History => "📜",
            Self::Home => "🏡",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Self::Easy, Self::Medium, Self::Hard];

    pub fn label(self) -> &'static str {
        match self {
            Self::Easy => "Easy",
            Self::Medium => "Medium",
            Self::Hard => "Hard",
        }
    }

    /// The label in `lang`.
    pub fn label_in(self, lang: Lang) -> &'static str {
        if lang == Lang::En {
            return self.label();
        }
        match self {
            Self::Easy => "Kolay",
            Self::Medium => "Orta",
            Self::Hard => "Zor",
        }
    }

    /// Shells for a right answer.
    pub fn reward(self) -> i64 {
        match self {
            Self::Easy => 8,
            Self::Medium => 12,
            Self::Hard => 18,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Question {
    pub id: String,
    pub category: TriviaCategory,
    pub difficulty: Difficulty,
    pub question: String,
    /// Four options in the order the file lists them; rounds show them shuffled.
    pub options: Vec<String>,
    /// One of `options`.
    pub answer: String,
    /// The language the question is written in; set when the file is loaded.
    #[serde(skip)]
    pub lang: Lang,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HangmanCategory {
    SeaLife,
    HouseItems,
    Food,
    Nature,
    Pets,
}

impl HangmanCategory {
    pub const ALL: [HangmanCategory; 5] = [Self::SeaLife, Self::HouseItems, Self::Food, Self::Nature, Self::Pets];

    pub fn label(self) -> &'static str {
        match self {
            Self::SeaLife => "Sea life",
            Self::HouseItems => "House items",
            Self::Food => "Food",
            Self::Nature => "Nature",
            Self::Pets => "Pets",
        }
    }

    /// The label in `lang`.
    pub fn label_in(self, lang: Lang) -> &'static str {
        if lang == Lang::En {
            return self.label();
        }
        match self {
            Self::SeaLife => "Deniz canlıları",
            Self::HouseItems => "Ev eşyaları",
            Self::Food => "Yemek",
            Self::Nature => "Doğa",
            Self::Pets => "Evcil hayvanlar",
        }
    }

    pub fn emoji(self) -> &'static str {
        match self {
            Self::SeaLife => "🐚",
            Self::HouseItems => "🪑",
            Self::Food => "🥞",
            Self::Nature => "🌻",
            Self::Pets => "🐾",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HangmanWord {
    pub category: HangmanCategory,
    /// Lowercase letters with at most one space.
    pub word: String,
}

#[derive(Deserialize)]
struct WordFile {
    answers: Vec<String>,
    allowed: Vec<String>,
}

/// The wordle lists: the words a puzzle can be, and every word a guess may be.
pub struct Words {
    pub answers: Vec<String>,
    pub allowed: Vec<String>,
    allowed_set: HashSet<String>,
}

impl Words {
    pub fn is_allowed(&self, word: &str) -> bool {
        self.allowed_set.contains(word)
    }
}

pub struct PlayContent {
    pub lang: Lang,
    pub trivia: Vec<Question>,
    pub hangman: Vec<HangmanWord>,
    pub words: Words,
}

static EMBEDDED: LazyLock<PlayContent> = LazyLock::new(|| {
    PlayContent::from_json(
        include_str!("../../../../content/trivia.json"),
        include_str!("../../../../content/hangman.json"),
        include_str!("../../../../content/words.json"),
    )
    .expect("embedded game content must parse; run `cargo test -p pinchora-game`")
});

static EMBEDDED_TR: LazyLock<PlayContent> = LazyLock::new(|| {
    PlayContent::from_json_in(
        Lang::Tr,
        include_str!("../../../../content/trivia_tr.json"),
        include_str!("../../../../content/hangman_tr.json"),
        include_str!("../../../../content/words_tr.json"),
    )
    .expect("embedded Turkish game content must parse; run `cargo test -p pinchora-game`")
});

impl PlayContent {
    /// The English content compiled into this binary.
    pub fn get() -> &'static PlayContent {
        &EMBEDDED
    }

    /// The content compiled into this binary for `lang`.
    pub fn for_lang(lang: Lang) -> &'static PlayContent {
        match lang {
            Lang::En => &EMBEDDED,
            Lang::Tr => &EMBEDDED_TR,
        }
    }

    pub fn from_json(trivia: &str, hangman: &str, words: &str) -> anyhow::Result<Self> {
        Self::from_json_in(Lang::En, trivia, hangman, words)
    }

    pub fn from_json_in(lang: Lang, trivia: &str, hangman: &str, words: &str) -> anyhow::Result<Self> {
        let file: WordFile = serde_json::from_str(words)?;
        let allowed_set = file.allowed.iter().cloned().collect();
        let mut trivia: Vec<Question> = serde_json::from_str(trivia)?;
        for question in &mut trivia {
            question.lang = lang;
        }
        Ok(Self {
            lang,
            trivia,
            hangman: serde_json::from_str(hangman)?,
            words: Words {
                answers: file.answers,
                allowed: file.allowed,
                allowed_set,
            },
        })
    }

    pub fn question(&self, id: &str) -> Option<&Question> {
        self.trivia.iter().find(|q| q.id == id)
    }
}

/// A question of any language by its id (the ids of the languages never collide): a running round only
/// remembers the id.
pub fn find_question(id: &str) -> Option<&'static Question> {
    Lang::ALL
        .into_iter()
        .find_map(|lang| PlayContent::for_lang(lang).question(id))
}

/// The hangman entry of a word in any language.
pub fn find_hangman(word: &str) -> Option<&'static HangmanWord> {
    Lang::ALL
        .into_iter()
        .find_map(|lang| PlayContent::for_lang(lang).hangman.iter().find(|h| h.word == word))
}
