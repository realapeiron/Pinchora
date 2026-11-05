mod badges;
mod character;
mod connect4;
mod frame;
mod grid;
mod hangman;
mod leaderboard;
mod levelup;
mod nursery;
mod pet;
mod profile;
mod rank;
mod showcase;
mod wallet;
mod welcome;
mod wordle;

pub use badges::{BadgeCell, BadgeCollectionCard, BadgeState};
pub use character::CharacterCard;
pub use connect4::Connect4Card;
pub use grid::{GridSlot, ItemGridCard};
pub use hangman::{HangmanCard, HangmanLetter};
pub use leaderboard::{LeaderboardCard, LeaderboardEntry};
pub use levelup::LevelUpCard;
pub use nursery::{NurseryCard, NurseryPet};
pub use pet::{PetCard, PetStatus};
pub use profile::ProfileCard;
pub use rank::{NextRole, RankCard};
pub use showcase::{ShowcaseKind, ShowcaseSlot};
pub use wallet::WalletCard;
pub use welcome::WelcomeCard;
pub use wordle::{ABSENT, CORRECT, PRESENT, WordleCard, WordleRow};

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

    use super::*;
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
            theme: Theme::reef(),
        }
    }

    #[test]
    fn cards_without_a_backdrop_keep_the_plain_look() {
        let plain = [
            profile(None, &[]).to_svg(),
            RankCard {
                display_name: "A",
                title: None,
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

    fn rank(title: Option<&str>) -> RankCard<'_> {
        RankCard {
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

    #[test]
    fn a_title_takes_its_own_line_under_the_name() {
        let plain = profile(None, &[]).to_svg();
        assert!(!plain.contains("TIDE ANGLER"), "no title, no line");
        let mut card = profile(None, &[]);
        card.title = Some("Tide <Angler>");
        let svg = card.to_svg();
        assert!(
            svg.contains("TIDE &lt;ANGLER&gt;"),
            "escaped and in the pixel font's capitals"
        );
        assert!(
            svg.contains(r#"y="80""#) && svg.contains("@captain.pinch"),
            "the name moves up for it"
        );
        // The same under a background picture: the plate grows to keep the handle on it.
        let png = scene();
        card.backdrop = Some(Backdrop {
            art: Some(SpriteArt {
                png: &png,
                w: 960,
                h: 540,
            }),
            accent: "#8eddd1",
        });
        assert!(card.to_svg().contains("TIDE &lt;ANGLER&gt;"));

        assert!(!rank(None).to_svg().contains("TIDE ANGLER"));
        let svg = rank(Some("Tide Angler")).to_svg();
        assert!(svg.contains("TIDE ANGLER") && svg.contains(r#"y="98""#));
    }

    #[test]
    fn badges_sit_on_plates_tinted_by_rarity() {
        let epic = 0x9b6bc4;
        assert!(!profile(None, &[]).to_svg().contains("#9b6bc4"), "no badge, no plate");
        let mut purple = badge('A', None);
        purple.rarity = epic;
        let svg = profile(None, &[purple]).to_svg();
        assert!(
            svg.contains(r##"fill="#9b6bc4"/><path"##),
            "the plate edge is the rarity colour"
        );
        assert!(!svg.contains("#3a9bdc"), "and no other badge colour appears");

        // Six plates never reach the crab or run off the card.
        let six: Vec<_> = "ABCDEF".chars().map(|c| badge(c, None)).collect();
        let svg = profile(None, &six).to_svg();
        assert_eq!(svg.matches(r##"fill="#3a9bdc"/><path"##).count(), 6);
        let last_plate_right = 450.0 + 5.0 * (BadgeIcon::PLATE + 3.0) + BadgeIcon::PLATE;
        assert!(last_plate_right + 12.0 < 804.0, "{last_plate_right}");
    }

    #[test]
    fn a_huge_total_stays_short_enough_for_the_badges() {
        let mut card = profile(None, &[]);
        card.total_xp = 123_456_789;
        assert!(card.to_svg().contains("TOTAL 123.5M XP"));
        card.total_xp = 999_999;
        assert!(card.to_svg().contains("TOTAL 999,999 XP"));
    }

    fn collection<'a>(art: &'a [u8]) -> Vec<BadgeCell<'a>> {
        let icon = |letter, rarity| BadgeIcon {
            art: (letter == 'A').then_some(SpriteArt { png: art, w: 48, h: 48 }),
            letter,
            color: 0x06d6a0,
            rarity,
        };
        vec![
            BadgeCell {
                icon: icon('A', 0x8fa0d8),
                name: "Little Beacon",
                rarity: "Common",
                state: BadgeState::Earned {
                    new: true,
                    worn: Some(1),
                },
            },
            BadgeCell {
                icon: icon('B', 0xffd166),
                name: "Guiding Light",
                rarity: "Legendary",
                state: BadgeState::Earned { new: false, worn: None },
            },
            BadgeCell {
                icon: icon('C', 0x3a9bdc),
                name: "Fin Finder",
                rarity: "Rare",
                state: BadgeState::Locked { have: 3, at: 10 },
            },
            BadgeCell {
                icon: icon('D', 0x9b6bc4),
                name: "Pearl Wyrm",
                rarity: "Epic",
                state: BadgeState::ForSale { price: 25_000 },
            },
        ]
    }

    const LEGEND: [(&str, u32); 2] = [("Common", 0x8fa0d8), ("Legendary", 0xffd166)];

    #[test]
    fn the_collection_marks_earned_new_locked_and_for_sale() {
        let png = scene();
        let cells = collection(&png);
        let card = BadgeCollectionCard {
            cells: &cells,
            legend: &LEGEND,
            theme: Theme::reef(),
        };
        let svg = card.to_svg();
        assert!(svg.contains(">2/4</text>"), "two of four are earned");
        assert!(svg.contains(">NEW</text>") && svg.matches(">NEW</text>").count() == 1);
        assert!(svg.contains(">1</text>"), "the worn badge shows its place");
        assert!(svg.contains(">3/10</text>"), "locked badges show progress");
        assert!(svg.contains(">25,000</text>"), "shop badges show the price");
        assert!(svg.contains("Little Beacon") && svg.contains("Fin Finder"));
        assert_eq!(
            svg.matches("<image").count(),
            1,
            "only the badge with art draws an image"
        );
        assert!(
            !svg.contains(r#"filter="url(#locked)""#),
            "and that one is earned, so not darkened"
        );
        let mut locked = collection(&png);
        locked[0].state = BadgeState::Locked { have: 1, at: 5 };
        let svg = BadgeCollectionCard {
            cells: &locked,
            legend: &LEGEND,
            theme: Theme::reef(),
        }
        .to_svg();
        assert!(
            svg.contains(r#"filter="url(#locked)""#),
            "locked art goes through the dark filter"
        );
        assert!(
            svg.contains(">1/4</text>") && !svg.contains(">NEW</text>"),
            "one earned of four, and nothing new"
        );
        assert_eq!(BadgeCollectionCard::height_for(40), 100 + 5 * 134 + 56);
        assert_eq!(BadgeCollectionCard::height_for(41), 100 + 6 * 134 + 56);
    }

    #[test]
    fn the_collection_renders_at_its_height() {
        let r = Renderer::new();
        let png = scene();
        let mut cells = collection(&png);
        // Forty badges, a third of them locked, as in the real collection.
        for i in 0..36 {
            let (letter, state) = match i % 3 {
                0 => ('A', BadgeState::Locked { have: i, at: 100 }),
                1 => ('B', BadgeState::ForSale { price: 1_500 }),
                _ => (
                    'C',
                    BadgeState::Earned {
                        new: i == 5,
                        worn: None,
                    },
                ),
            };
            cells.push(BadgeCell {
                icon: BadgeIcon {
                    art: (i % 2 == 0).then_some(SpriteArt {
                        png: &png,
                        w: 48,
                        h: 48,
                    }),
                    letter,
                    color: 0xff6b4a,
                    rarity: 0x5bbf6a,
                },
                name: "Moonlong Mornings",
                rarity: "Uncommon",
                state,
            });
        }
        assert_eq!(cells.len(), 40);
        let card = BadgeCollectionCard {
            cells: &cells,
            legend: &LEGEND,
            theme: Theme::reef(),
        };
        let image = image::load_from_memory(&r.badge_collection(&card).expect("renders")).expect("a png");
        assert_eq!(
            (image.width(), image.height()),
            (BadgeCollectionCard::WIDTH, BadgeCollectionCard::height_for(40))
        );
    }

    fn tile<'a>(kind: ShowcaseKind, png: &'a [u8], name: &'a str) -> ShowcaseSlot<'a> {
        let (w, h) = if kind == ShowcaseKind::House {
            ShowcaseSlot::HOUSE_BOX
        } else {
            (32, 24)
        };
        ShowcaseSlot {
            kind,
            art: SpriteArt { png, w, h },
            name,
        }
    }

    #[test]
    fn a_card_with_nothing_chosen_keeps_its_classic_layout() {
        // The rows the card has always had: the XP label, the stats, the about-me panel, the total.
        let svg = profile(None, &[]).to_svg();
        for classic in [
            r#"y="424""#,
            r#"y="318""#,
            r#"y="362""#,
            r#"y="170""#,
            r#"y="200""#,
            r#"y="512""#,
        ] {
            assert!(svg.contains(classic), "{classic}");
        }
        assert!(!svg.contains("FURNITURE") && !svg.contains("HOME") && !svg.contains("evenodd"));
        // Showing nothing and a plain, normal style are the same card.
        let mut explicit = profile(None, &[]);
        explicit.theme = Theme::reef().with_style(Opacity::Normal, Frame::Plain);
        assert_eq!(explicit.to_svg(), svg);
    }

    #[test]
    fn a_showcase_squeezes_the_card_to_fit_a_row_of_tiles() {
        let png = scene();
        let slots = [
            tile(ShowcaseKind::Fish, &png, "Amberwink Squidlet"),
            tile(ShowcaseKind::Pet, &png, "Sir Clicks"),
            tile(ShowcaseKind::Furniture, &png, "Blushbun Bed (Rose)"),
            tile(ShowcaseKind::House, &png, "Tide Pool Shack"),
        ];
        let mut card = profile(None, &[]);
        card.showcase = &slots;
        let svg = card.to_svg();
        for caption in ["FISH", "PET", "FURNITURE", "HOME"] {
            assert!(svg.contains(&format!(">{caption}</text>")), "{caption}");
        }
        assert!(svg.contains("Tide Pool Shack") && svg.contains("Sir Clicks"));
        assert!(!svg.contains(r#"y="424""#), "the XP bar moved down to make room");
        assert!(svg.contains(r#"y="383""#), "the stats row sits under the showcase");
        assert!(svg.contains(r#"y="432""#));
        // Four tiles are all there is room for.
        let mut five: Vec<_> = slots.iter().map(|s| tile(s.kind, &png, s.name)).collect();
        five.push(tile(ShowcaseKind::Fish, &png, "Fifth"));
        card.showcase = &five;
        assert!(!card.to_svg().contains("Fifth"));
    }

    #[test]
    fn the_footer_stays_clear_of_a_frame() {
        // The frame's band is 8 px; the lowest thing on the card is the badge plates.
        let band = 540.0 - super::frame::BAND;
        for (rows, what) in [
            (super::profile::Rows::classic(6.0), "classic with a frame"),
            (super::profile::Rows::showcase(), "showcase"),
        ] {
            assert!(rows.foot_y + BadgeIcon::PLATE <= band, "{what}: {}", rows.foot_y);
            assert!(rows.plate.0 + rows.plate.1 <= band, "{what}: the plate");
        }
        // And the classic rows are the old ones when there is no frame.
        assert_eq!(super::profile::Rows::classic(0.0).foot_y, 482.0);
        let mut framed = profile(None, &[]);
        framed.theme = Theme::reef().with_style(Opacity::Normal, Frame::Gold);
        let svg = framed.to_svg();
        assert!(
            svg.contains(r#"y="418""#) && !svg.contains(r#"y="424""#),
            "the footer rose 6 px for the band"
        );
    }

    #[test]
    fn showcase_tiles_stay_inside_the_row_whatever_the_names() {
        let png = scene();
        let long = "Wolfgang Amadeus Mozartwolfgangamadeus";
        for count in 1..=4 {
            let slots: Vec<_> = [
                ShowcaseKind::Fish,
                ShowcaseKind::Pet,
                ShowcaseKind::Furniture,
                ShowcaseKind::House,
            ]
            .into_iter()
            .take(count)
            .map(|kind| tile(kind, &png, long))
            .collect();
            let mut svg = String::new();
            showcase::draw_row(&mut svg, &Theme::reef(), &slots, 296.0, 260.0, 624.0);
            // Every panel edge path starts at its left corner and stays within the 624 px.
            let mut right = 0.0f32;
            for path in svg.split("<path d=\"M").skip(1) {
                for point in path.split_whitespace().take(2) {
                    let x: f32 = point.split(',').next().expect("x").parse().unwrap_or(0.0);
                    right = right.max(x);
                }
            }
            assert!(right <= 920.0 + 0.01 && right > 296.0, "{count} tiles reach {right}");
            assert!(svg.contains('…'), "names that do not fit are cut");
            assert!(!svg.contains(long));
        }
        let mut none = String::new();
        showcase::draw_row(&mut none, &Theme::reef(), &[], 296.0, 260.0, 624.0);
        assert!(none.is_empty(), "no tiles, no row");
    }

    #[test]
    fn a_partner_gets_a_heart_in_the_corner_of_the_about_me_panel() {
        let plain = profile(None, &[]).to_svg();
        assert!(!plain.contains("MARLOW"));
        let mut card = profile(None, &[]);
        card.partner = Some("Marlow <3".to_owned());
        let svg = card.to_svg();
        assert!(svg.contains("MARLOW &lt;3"), "escaped, in capitals");
        let hearts = |s: &str| s.matches(&format!(r##"fill="{}""##, Theme::reef().heart)).count();
        assert!(hearts(&svg) > hearts(&plain), "a heart sprite comes with it");
        // A name too long for the corner is cut.
        card.partner = Some("An Extremely Long Partner Name Indeed".to_owned());
        assert!(card.to_svg().contains('…'));
        // The row holds up on the busy layout too.
        let png = scene();
        let slots = [tile(ShowcaseKind::Fish, &png, "Fish")];
        card.showcase = &slots;
        card.character = Some(SpriteArt {
            png: &png,
            w: 60,
            h: 100,
        });
        assert!(card.to_svg().contains("AN EXTREMELY LONG"));
    }

    #[test]
    fn style_shows_on_the_profile_rank_and_wallet_cards() {
        let themed = |opacity, frame| Theme::reef().with_style(opacity, frame);
        let rank = |theme| RankCard { theme, ..rank(None) };
        let wallet = |theme| WalletCard {
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
            theme,
        };
        let plain = (
            profile(None, &[]).to_svg().len(),
            rank(Theme::reef()).to_svg().len(),
            wallet(Theme::reef()).to_svg().len(),
        );
        for frame in &Frame::ALL[1..] {
            let mut card = profile(None, &[]);
            card.theme = themed(Opacity::Normal, *frame);
            assert!(card.to_svg().len() > plain.0, "{frame:?} on the profile");
            assert!(
                rank(themed(Opacity::Normal, *frame)).to_svg().len() > plain.1,
                "{frame:?} on rank"
            );
            assert!(
                wallet(themed(Opacity::Normal, *frame)).to_svg().len() > plain.2,
                "{frame:?} on wallet"
            );
        }
        // See-through panels use a ring and a translucent fill; normal panels do not.
        let mut light = profile(None, &[]);
        light.theme = themed(Opacity::Light, Frame::Plain);
        assert!(light.to_svg().contains("evenodd"));
        assert!(
            wallet(themed(Opacity::Light, Frame::Plain))
                .to_svg()
                .contains("evenodd")
        );
        assert!(!wallet(Theme::reef()).to_svg().contains("evenodd"));
        // Over a picture the veil and the plates follow the opacity.
        let png = scene();
        let art = |opacity| {
            let mut card = profile(
                Some(Backdrop {
                    art: Some(SpriteArt {
                        png: &png,
                        w: 960,
                        h: 540,
                    }),
                    accent: "#8eddd1",
                }),
                &[],
            );
            card.theme = themed(opacity, Frame::Plain);
            card.to_svg()
        };
        assert!(art(Opacity::Light).contains(r#"fill-opacity="0.1""#));
        assert!(art(Opacity::Normal).contains(r#"fill-opacity="0.25""#));
        assert!(art(Opacity::Solid).contains(r#"fill-opacity="0.55""#));
        assert!(art(Opacity::Solid).contains(r#"fill-opacity="0.95""#));
    }
}
