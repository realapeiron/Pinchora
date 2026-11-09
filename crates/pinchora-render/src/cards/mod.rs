mod achievements;
mod badges;
mod birthdays;
mod boards;
mod character;
mod checkers;
mod chess;
mod collection;
mod color;
mod connect4;
mod designed;
mod fleet;
mod frame;
mod grid;
mod hangman;
mod help;
mod info;
mod leaderboard;
mod levelup;
mod nursery;
mod party;
mod party_games;
mod pet;
mod picross;
mod poll;
mod profile;
mod quick;
mod rank;
mod showcase;
pub(crate) mod stats;
mod tide2048;
mod wallet;
mod welcome;
mod wordle;
mod wrapped;

pub use achievements::{AchievementCell, AchievementsCard};
pub use badges::{BadgeCell, BadgeCollectionCard, BadgeState};
pub use birthdays::BirthdayCard;
pub use character::{CharacterCard, EditorCell, EditorSection};
pub use checkers::CheckersCard;
pub use chess::ChessCard;
pub use collection::{
    BottleCard, DexCard, DexCell, DropCard, DropCritter, EntryPicture, HaulCard, HaulTile, PotCard, PotState,
};
pub use color::{ColorCard, ColorCell};
pub use connect4::Connect4Card;
pub use designed::{CardFill, DesignedCard, fill_placeholders, ordinal};
pub use fleet::{FleetCard, RadarCard, SHIP_COLORS, SHIP_NAMES, SeaView, ShipView};
pub use grid::{GridSlot, ItemGridCard};
pub use hangman::{HangmanCard, HangmanLetter};
pub use help::{HelpCard, HelpRow};
pub use info::{InfoCard, InfoRow};
pub use leaderboard::{LastWeekEntry, LeaderboardCard, LeaderboardEntry};
pub use levelup::LevelUpCard;
pub use nursery::{NurseryCard, NurseryPet};
pub use party::{
    CrewView, DerbyCard, DerbyLane, HeistCard, Mood, PickView, ShareGrabCard, ShellCard, StageView, SurgeCard,
    SurgeState,
};
pub use party_games::{
    EntriesCard, EntryHeader, EntryRow, LettersCard, PartyLobbyCard, PearlCard, PearlSeat, PostcardCard, PromptCard,
    PromptTone, RaceCard, Racer, RacerState, RatherCard, StandingRow, StandingsCard,
};
pub use pet::{PetCard, PetStatus};
pub use picross::{PicrossCard, Reveal};
pub use poll::{PollCard, PollOption};
pub use profile::ProfileCard;
pub use quick::{QuickCard, QuickDetails, QuickHeat, QuickKind};
pub use rank::{NextRole, RankCard};
pub use showcase::{ShowcaseKind, ShowcaseSlot};
pub use tide2048::Tide2048Card;
pub use wallet::WalletCard;
pub use welcome::WelcomeCard;
pub use wordle::{ABSENT, CORRECT, IDLE_KEY, PRESENT, WRONG_LETTER, WordleCard, WordleRow};
pub use wrapped::{WrappedCard, WrappedFish, WrappedHouse, WrappedTotals};

use std::fmt::Write;

pub(crate) use frame::draw_frame;

use crate::svg::{data_uri_png, escape, mix, notched_path, panel};

/// A sprite already scaled up with nearest-neighbour, as PNG, with its pixel size.
pub struct SpriteArt<'a> {
    pub png: &'a [u8],
    pub w: u32,
    pub h: u32,
}

/// The picture behind a card (a profile background).
pub struct Backdrop<'a> {
    /// The 240×135 sprite scaled [`Backdrop::SCALE`]× to 960×540. It covers the card, cropped
    /// where the card is not 16:9. `None` draws a gradient from `accent` until the art exists.
    pub art: Option<SpriteArt<'a>>,
    /// `#rrggbb`.
    pub accent: &'a str,
}

impl Backdrop<'_> {
    pub const SCALE: u32 = 4;
}

/// A badge worn on the profile card.
pub struct BadgeIcon<'a> {
    /// The 24 px-wide sprite scaled [`BadgeIcon::SCALE`]×. `None` draws a coloured circle with
    /// `letter` in it until the art exists.
    pub art: Option<SpriteArt<'a>>,
    pub letter: char,
    /// `0xrrggbb`, the placeholder circle.
    pub color: u32,
    /// `0xrrggbb`, the tint of the plate the badge sits on: the colour of its rarity.
    pub rarity: u32,
}

impl BadgeIcon<'_> {
    pub const SCALE: u32 = 2;
    /// What a drawn badge fills of its plate.
    pub const SIZE: f32 = 48.0;
    /// The plate behind a badge: the art with a 2 px frame around it.
    pub const PLATE: f32 = 52.0;

    /// Draws the badge on its rarity plate, whose top-left is `(x, y)`.
    pub(crate) fn draw(&self, out: &mut String, x: f32, y: f32) {
        let edge = format!("#{:06x}", self.rarity & 0xff_ffff);
        let fill = mix(&edge, "#0b1029", 0.74);
        self.plate(out, x, y, &edge, &fill);
        // The classic top highlight row of the pixel bevel.
        let _ = write!(
            out,
            r##"<rect x="{}" y="{}" width="{}" height="2" fill="#ffffff" fill-opacity="0.22"/>"##,
            x + 6.0,
            y + 2.0,
            Self::PLATE - 12.0
        );
        self.art(out, x, y, false);
    }

    /// Draws the badge as a dark silhouette on a dim plate: not earned yet.
    pub(crate) fn draw_locked(&self, out: &mut String, x: f32, y: f32) {
        let edge = badges::locked_tint(self.rarity);
        self.plate(out, x, y, &mix(&edge, "#2e3d75", 0.5), "#0b1029");
        self.art(out, x, y, true);
    }

    fn plate(&self, out: &mut String, x: f32, y: f32, edge: &str, fill: &str) {
        let _ = write!(
            out,
            r#"<path d="{}" fill="{edge}"/><path d="{}" fill="{fill}"/>"#,
            notched_path(x, y, Self::PLATE, Self::PLATE, 4.0),
            notched_path(x + 2.0, y + 2.0, Self::PLATE - 4.0, Self::PLATE - 4.0, 2.0),
        );
    }

    /// The art (or its placeholder) centred on the plate whose top-left is `(x, y)`.
    fn art(&self, out: &mut String, x: f32, y: f32, locked: bool) {
        let (mid_x, mid_y) = (x + Self::PLATE / 2.0, y + Self::PLATE / 2.0);
        match &self.art {
            Some(art) => {
                // Sprites keep their proportions, shrink to fit when taller than wide, and sit
                // in the middle of the plate.
                let (w, h) = (art.w as f32, art.h as f32);
                let k = (Self::SIZE / w.max(h)).min(1.0);
                let (w, h) = (w * k, h * k);
                let filter = if locked { r#" filter="url(#locked)""# } else { "" };
                let _ = write!(
                    out,
                    r#"<image x="{}" y="{}" width="{w}" height="{h}" image-rendering="optimizeSpeed"{filter} href="{}"/>"#,
                    mid_x - w / 2.0,
                    mid_y - h / 2.0,
                    data_uri_png(art.png)
                );
            }
            None => {
                let (fill, ink) = if locked {
                    ("#1b1d2b".to_owned(), "#45495c")
                } else {
                    (format!("#{:06x}", self.color & 0xff_ffff), "#0b1029")
                };
                let _ = write!(
                    out,
                    r##"<circle cx="{mid_x}" cy="{mid_y}" r="21" fill="{fill}" stroke="#1b1d2b" stroke-width="3"/>"##
                );
                text(
                    out,
                    mid_x,
                    mid_y + 8.5,
                    24.0,
                    PIXEL,
                    true,
                    ink,
                    "middle",
                    &escape(&self.letter.to_string()),
                );
            }
        }
    }
}

/// Draws a [`Backdrop`] over the whole card, then a dark scrim of opacity `scrim` that keeps
/// panels and text readable.
pub(crate) fn backdrop(out: &mut String, w: f32, h: f32, b: &Backdrop<'_>, scrim: f32) {
    match &b.art {
        Some(art) => {
            let _ = write!(
                out,
                r#"<image x="0" y="0" width="{w}" height="{h}" preserveAspectRatio="xMidYMid slice" href="{}"/>"#,
                data_uri_png(art.png)
            );
        }
        None => {
            let _ = write!(
                out,
                r#"<defs><linearGradient id="backdrop" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient></defs><rect width="{w}" height="{h}" fill="url(#backdrop)"/>"#,
                crate::svg::escape(b.accent),
                mix(b.accent, "#0f1631", 0.7)
            );
        }
    }
    let _ = write!(
        out,
        r##"<rect width="{w}" height="{h}" fill="#0b1029" fill-opacity="{scrim}"/>"##
    );
}

/// Avatar in a notched frame. `png` should already be `size - 2*border` square.
pub(crate) fn avatar_frame(out: &mut String, png: &[u8], x: f32, y: f32, size: f32, border: f32, edge: &str, id: &str) {
    panel(out, x, y, size, size, border, "#0b1029", edge);
    let inner = size - 2.0 * border;
    let _ = write!(
        out,
        r#"<clipPath id="{id}"><path d="{clip}"/></clipPath><image x="{ix}" y="{iy}" width="{inner}" height="{inner}" preserveAspectRatio="none" clip-path="url(#{id})" href="{uri}"/>"#,
        clip = notched_path(x + border, y + border, inner, inner, border),
        ix = x + border,
        iy = y + border,
        uri = data_uri_png(png),
    );
}

pub(crate) fn text(
    out: &mut String,
    x: f32,
    y: f32,
    size: f32,
    family: &str,
    bold: bool,
    fill: &str,
    anchor: &str,
    body: &str,
) {
    let weight = if bold { "bold" } else { "normal" };
    let _ = write!(
        out,
        r#"<text x="{x}" y="{y}" font-family="{family}" font-size="{size}" font-weight="{weight}" fill="{fill}" text-anchor="{anchor}">{body}</text>"#
    );
}

pub(crate) const PIXEL: &str = "Silkscreen";
pub(crate) const SANS: &str = "Noto Sans";

#[cfg(test)]
mod tests {
    use image::RgbaImage;

    use crate::cards::*;
    use crate::{Frame, Opacity, ProfileCard, RankCard, Renderer, Theme, WalletCard, encode_png, placeholder_avatar};

    fn scene() -> Vec<u8> {
        encode_png(&RgbaImage::from_pixel(960, 540, image::Rgba([200, 220, 180, 255]))).expect("encodes")
    }

    fn badge(letter: char, art: Option<&[u8]>) -> BadgeIcon<'_> {
        BadgeIcon {
            art: art.map(|png| SpriteArt { png, w: 48, h: 48 }),
            letter,
            color: 0x06d6a0,
            rarity: 0x3a9bdc,
        }
    }

    fn profile<'a>(backdrop: Option<Backdrop<'a>>, badges: &'a [BadgeIcon<'a>]) -> ProfileCard<'a> {
        ProfileCard {
            lang: pinchora_core::infra::i18n::Lang::En,
            display_name: "Captain Pinch",
            title: None,
            username: "captain.pinch",
            avatar_png: &[],
            bio: "",
            level: 3,
            xp_current: 10,
            xp_needed: 100,
            total_xp: 410,
            credits: 5,
            reputation: 1,
            streak: 2,
            global_rank: 9,
            server_rank: None,
            character: None,
            backdrop,
            badges,
            showcase: &[],
            partner: None,
            birthday_today: false,
            theme: Theme::reef(),
        }
    }

    fn rank(title: Option<&str>) -> RankCard<'_> {
        RankCard {
            lang: pinchora_core::infra::i18n::Lang::En,
            display_name: "Captain Pinch",
            title,
            username: "captain.pinch",
            avatar_png: &[],
            level: 7,
            xp_current: 5,
            xp_needed: 50,
            rank: 3,
            next_role: None,
            backdrop: None,
            theme: Theme::reef(),
        }
    }

    #[path = "layout.rs"]
    mod layout;
    #[path = "showcase.rs"]
    mod showcase_tests;
}
pub mod verification;

mod reef_loop;
pub use reef_loop::{ReefCard, ReefScene, ReefTile};
