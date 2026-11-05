mod character;
mod connect4;
mod grid;
mod hangman;
mod leaderboard;
mod levelup;
mod pet;
mod profile;
mod rank;
mod wallet;
mod welcome;
mod wordle;

pub use character::CharacterCard;
pub use connect4::Connect4Card;
pub use grid::{GridSlot, ItemGridCard};
pub use hangman::{HangmanCard, HangmanLetter};
pub use leaderboard::{LeaderboardCard, LeaderboardEntry};
pub use levelup::LevelUpCard;
pub use pet::PetCard;
pub use profile::ProfileCard;
pub use rank::{NextRole, RankCard};
pub use wallet::WalletCard;
pub use welcome::WelcomeCard;
pub use wordle::{ABSENT, CORRECT, PRESENT, WordleCard, WordleRow};

use std::fmt::Write;

use crate::svg::{data_uri_png, mix, notched_path, panel};

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
}

impl BadgeIcon<'_> {
    pub const SCALE: u32 = 2;
    /// What a drawn badge fills of its slot.
    pub const SIZE: f32 = 48.0;

    /// Draws the badge in the `SIZE`×`SIZE` slot whose top-left is `(x, y)`.
    pub(crate) fn draw(&self, out: &mut String, x: f32, y: f32) {
        match &self.art {
            Some(art) => {
                // Sprites keep their proportions, shrink to fit when taller than wide, and sit
                // in the middle of the slot.
                let (w, h) = (art.w as f32, art.h as f32);
                let k = (Self::SIZE / w.max(h)).min(1.0);
                let (w, h) = (w * k, h * k);
                let _ = write!(
                    out,
                    r#"<image x="{}" y="{}" width="{w}" height="{h}" image-rendering="optimizeSpeed" href="{}"/>"#,
                    x + (Self::SIZE - w) / 2.0,
                    y + (Self::SIZE - h) / 2.0,
                    data_uri_png(art.png)
                );
            }
            None => {
                let (cx, cy) = (x + Self::SIZE / 2.0, y + Self::SIZE / 2.0);
                let _ = write!(
                    out,
                    r##"<circle cx="{cx}" cy="{cy}" r="21" fill="#{:06x}" stroke="#1b1d2b" stroke-width="3"/>"##,
                    self.color & 0xff_ffff
                );
                text(
                    out,
                    cx,
                    cy + 8.5,
                    24.0,
                    PIXEL,
                    true,
                    "#0b1029",
                    "middle",
                    &crate::svg::escape(&self.letter.to_string()),
                );
            }
        }
    }
}

/// Draws a [`Backdrop`] over the whole card, then a dark scrim that keeps panels and text readable.
pub(crate) fn backdrop(out: &mut String, w: f32, h: f32, b: &Backdrop<'_>) {
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
        r##"<rect width="{w}" height="{h}" fill="#0b1029" fill-opacity="0.25"/>"##
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

    use super::*;
    use crate::{ProfileCard, RankCard, Renderer, Theme, WalletCard, encode_png, placeholder_avatar};

    fn scene() -> Vec<u8> {
        encode_png(&RgbaImage::from_pixel(960, 540, image::Rgba([200, 220, 180, 255]))).expect("encodes")
    }

    fn badge(letter: char, art: Option<&[u8]>) -> BadgeIcon<'_> {
        BadgeIcon {
            art: art.map(|png| SpriteArt { png, w: 48, h: 48 }),
            letter,
            color: 0x06d6a0,
        }
    }

    fn profile<'a>(backdrop: Option<Backdrop<'a>>, badges: &'a [BadgeIcon<'a>]) -> ProfileCard<'a> {
        ProfileCard {
            display_name: "Captain Pinch",
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
            theme: Theme::reef(),
        }
    }

    #[test]
    fn cards_without_a_backdrop_keep_the_plain_look() {
        let plain = [
            profile(None, &[]).to_svg(),
            RankCard {
                display_name: "A",
                username: "a",
                avatar_png: &[],
                level: 1,
                xp_current: 1,
                xp_needed: 10,
                rank: 1,
                next_role: None,
                backdrop: None,
                theme: Theme::reef(),
            }
            .to_svg(),
            WalletCard {
                display_name: "A",
                avatar_png: &[],
                shells: 1,
                pearls: 1,
                streak: 1,
                next_daily: "ready",
                pets: 1,
                fish_found: 1,
                fish_total: 2,
                backdrop: None,
                theme: Theme::reef(),
            }
            .to_svg(),
        ];
        for svg in plain {
            assert!(svg.contains(r#"id="bg""#), "the night-sea gradient is drawn");
            assert!(!svg.contains("backdrop") && !svg.contains("slice") && !svg.contains("<circle"));
            assert!(!svg.contains(r#"fill-opacity="0.8""#), "no plates without a background");
        }
    }

    #[test]
    fn a_backdrop_replaces_the_gradient_and_takes_plates() {
        let png = scene();
        let art = Backdrop {
            art: Some(SpriteArt {
                png: &png,
                w: 960,
                h: 540,
            }),
            accent: "#8eddd1",
        };
        let svg = profile(Some(art), &[]).to_svg();
        assert!(svg.contains(r#"preserveAspectRatio="xMidYMid slice""#));
        assert!(!svg.contains(r#"id="bg""#) && !svg.contains(r#"id="backdrop""#));
        assert!(svg.contains(r#"fill-opacity="0.8""#), "loose text sits on plates");

        let placeholder = Backdrop {
            art: None,
            accent: "#8eddd1",
        };
        let svg = profile(Some(placeholder), &[]).to_svg();
        assert!(svg.contains(r#"id="backdrop""#) && svg.contains("#8eddd1"));
        assert!(!svg.contains("slice"));
    }

    #[test]
    fn at_most_six_badges_are_drawn() {
        let png = scene();
        let letters: Vec<_> = "ABCDEFGH".chars().map(|c| badge(c, None)).collect();
        let svg = profile(None, &letters).to_svg();
        assert_eq!(svg.matches("<circle").count(), ProfileCard::MAX_BADGES);
        assert!(svg.contains(">F</text>") && !svg.contains(">G</text>"));

        let art = [badge('A', Some(&png)), badge('B', None)];
        let svg = profile(None, &art).to_svg();
        assert_eq!(
            svg.matches("<circle").count(),
            1,
            "only the badge without art is a placeholder"
        );
        let bare = profile(None, &[]).to_svg().matches("<image").count();
        assert_eq!(svg.matches("<image").count(), bare + 1, "the other draws its sprite");
    }

    #[test]
    fn cards_with_backgrounds_and_badges_render() {
        let r = Renderer::new();
        let png = scene();
        let avatar = placeholder_avatar(1, ProfileCard::AVATAR_SIZE);
        let badges = [badge('A', Some(&png)), badge('B', None)];
        for art in [true, false] {
            let backdrop = Backdrop {
                art: art.then_some(SpriteArt {
                    png: &png,
                    w: 960,
                    h: 540,
                }),
                accent: "#ffc58b",
            };
            let mut card = profile(Some(backdrop), &badges);
            card.avatar_png = &avatar;
            let image = image::load_from_memory(&r.profile_card(&card).expect("renders")).expect("a png");
            assert_eq!((image.width(), image.height()), (960, 540));
        }
    }
}
