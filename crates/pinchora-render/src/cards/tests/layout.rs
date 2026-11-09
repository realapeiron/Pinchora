use super::*;

#[test]
fn cards_without_a_backdrop_keep_the_plain_look() {
    let plain = [
        profile(None, &[]).to_svg(),
        RankCard {
            lang: pinchora_core::infra::i18n::Lang::En,
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
            lang: pinchora_core::infra::i18n::Lang::En,
            display_name: "A",
            avatar_png: &[],
            shells: 1,
            pearls: 1,
            streak: 1,
            next_daily: "ready",
            next_weekly: "ready",
            next_monthly: "ready",
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
        lang: pinchora_core::infra::i18n::Lang::En,
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
        lang: pinchora_core::infra::i18n::Lang::En,
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
        lang: pinchora_core::infra::i18n::Lang::En,
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
