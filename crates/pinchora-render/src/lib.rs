#![allow(clippy::too_many_arguments)] // drawing primitives take coordinates

mod avatar;
mod cards;
pub mod character;
mod encode;
pub mod room;
pub mod social;
pub mod sprites;
mod style;
mod svg;
mod theme;

use std::sync::Arc;

use resvg::{tiny_skia, usvg};

pub use avatar::{placeholder_avatar, prepare_avatar};
pub use cards::{
    ABSENT, Backdrop, BadgeCell, BadgeCollectionCard, BadgeIcon, BadgeState, CORRECT, CharacterCard, Connect4Card,
    GridSlot, HangmanCard, HangmanLetter, ItemGridCard, LeaderboardCard, LeaderboardEntry, LevelUpCard, NextRole,
    PRESENT, PetCard, ProfileCard, RankCard, ShowcaseKind, ShowcaseSlot, SpriteArt, WalletCard, WelcomeCard,
    WordleCard, WordleRow,
};
pub use encode::encode_png;
pub use style::{ACCENT_PRESETS, AccentError, Frame, Opacity, parse_accent, readable};
pub use svg::{compact, thousands};
pub use theme::Theme;

/// Renders cards to PNG. Build once and share; rendering is CPU-bound, so call it
/// from `spawn_blocking`.
pub struct Renderer {
    fonts: Arc<usvg::fontdb::Database>,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        let mut db = usvg::fontdb::Database::new();
        for font in [
            &include_bytes!("../../../assets/fonts/Silkscreen-Regular.ttf")[..],
            &include_bytes!("../../../assets/fonts/Silkscreen-Bold.ttf")[..],
            &include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf")[..],
            &include_bytes!("../../../assets/fonts/NotoSans-Bold.ttf")[..],
        ] {
            db.load_font_data(font.to_vec());
        }
        db.set_sans_serif_family("Noto Sans");
        Self { fonts: Arc::new(db) }
    }

    pub fn svg_to_png(&self, svg: &str) -> anyhow::Result<Vec<u8>> {
        let options = usvg::Options {
            fontdb: self.fonts.clone(),
            font_family: "Noto Sans".into(),
            ..Default::default()
        };
        let tree = usvg::Tree::from_str(svg, &options)?;
        let size = tree.size().to_int_size();
        let mut pixmap =
            tiny_skia::Pixmap::new(size.width(), size.height()).ok_or_else(|| anyhow::anyhow!("empty canvas"))?;
        resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
        Ok(pixmap.encode_png()?)
    }

    pub fn profile_card(&self, card: &ProfileCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn rank_card(&self, card: &RankCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn badge_collection(&self, card: &BadgeCollectionCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn welcome_card(&self, card: &WelcomeCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn item_grid(&self, card: &ItemGridCard) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn wallet_card(&self, card: &WalletCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn level_up_card(&self, card: &LevelUpCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn pet_card(&self, card: &PetCard) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn character_card(&self, card: &CharacterCard) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn leaderboard_card(&self, card: &LeaderboardCard) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn connect4_card(&self, card: &Connect4Card<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn hangman_card(&self, card: &HangmanCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn wordle_card(&self, card: &WordleCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }
}
