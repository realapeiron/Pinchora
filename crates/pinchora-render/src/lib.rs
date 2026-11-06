#![allow(clippy::too_many_arguments)] // drawing primitives take coordinates

mod avatar;
mod cards;
pub mod character;
pub mod critters;
mod encode;
pub mod room;
pub mod social;
pub mod sprites;
mod style;
mod svg;
mod theme;

use std::sync::{Arc, LazyLock};

use resvg::{tiny_skia, usvg};

pub use avatar::{placeholder_avatar, prepare_avatar};
pub use cards::{
    ABSENT, Backdrop, BadgeCell, BadgeCollectionCard, BadgeIcon, BadgeState, CORRECT, CharacterCard, Connect4Card,
    EditorCell, EditorSection, GridSlot, HangmanCard, HangmanLetter, ItemGridCard, LastWeekEntry, LeaderboardCard,
    LeaderboardEntry, LevelUpCard, NextRole, NurseryCard, NurseryPet, PRESENT, PetCard, PetStatus, PollCard,
    PollOption, ProfileCard, RankCard, ShowcaseKind, ShowcaseSlot, SpriteArt, WalletCard, WelcomeCard, WordleCard,
    WordleRow,
};
pub use cards::{
    BottleCard, DexCard, DexCell, DropCard, DropCritter, EntryPicture, HaulCard, HaulTile, PotCard, PotState,
};
pub use cards::{
    EntriesCard, EntryHeader, EntryRow, LettersCard, PartyLobbyCard, PearlCard, PearlSeat, PostcardCard, PromptCard,
    PromptTone, RaceCard, Racer, RacerState, RatherCard, StandingRow, StandingsCard,
};
pub use encode::encode_png;
pub use style::{ACCENT_PRESETS, AccentError, Frame, Opacity, parse_accent, readable};
pub use svg::{compact, thousands};
pub use theme::Theme;

/// The fonts the cards are drawn in, loaded once: the renderer draws with them and the text
/// measuring in [`svg`] lays names out with them.
pub(crate) fn fonts() -> &'static Arc<usvg::fontdb::Database> {
    static FONTS: LazyLock<Arc<usvg::fontdb::Database>> = LazyLock::new(|| {
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
        Arc::new(db)
    });
    &FONTS
}

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
        Self { fonts: fonts().clone() }
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

    pub fn nursery_card(&self, card: &NurseryCard) -> anyhow::Result<Vec<u8>> {
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

    pub fn poll_card(&self, card: &PollCard) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn dex_card(&self, card: &DexCard) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn drop_card(&self, card: &DropCard) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn bottle_card(&self, card: &BottleCard) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn haul_card(&self, card: &HaulCard) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn pot_card(&self, card: &PotCard) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn party_lobby_card(&self, card: &PartyLobbyCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn pearl_card(&self, card: &PearlCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn standings_card(&self, card: &StandingsCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn letters_card(&self, card: &LettersCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn entries_card(&self, card: &EntriesCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn race_card(&self, card: &RaceCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn postcard_card(&self, card: &PostcardCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn rather_card(&self, card: &RatherCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }

    pub fn prompt_card(&self, card: &PromptCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }
}
