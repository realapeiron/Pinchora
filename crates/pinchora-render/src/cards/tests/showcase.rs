use super::*;

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
    assert!(svg.contains(r#"y="392""#), "the stats row sits under the showcase");
    assert!(svg.contains(r#"y="438""#));
    // Four tiles are all there is room for.
    let mut five: Vec<_> = slots.iter().map(|s| tile(s.kind, &png, s.name)).collect();
    five.push(tile(ShowcaseKind::Fish, &png, "Fifth"));
    card.showcase = &five;
    assert!(!card.to_svg().contains("Fifth"));
}

#[test]
fn the_footer_stays_clear_of_a_frame() {
    // The frame's band is 8 px; the lowest thing on the card is the badge plates.
    let band = 540.0 - crate::cards::frame::BAND;
    for (rows, what) in [
        (crate::cards::profile::Rows::classic(6.0), "classic with a frame"),
        (crate::cards::profile::Rows::showcase(), "showcase"),
    ] {
        assert!(rows.foot_y + BadgeIcon::PLATE <= band, "{what}: {}", rows.foot_y);
        assert!(rows.plate.0 + rows.plate.1 <= band, "{what}: the plate");
    }
    // And the classic rows are the old ones when there is no frame.
    assert_eq!(crate::cards::profile::Rows::classic(0.0).foot_y, 482.0);
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
        showcase::draw_row(
            &mut svg,
            &Theme::reef(),
            pinchora_core::infra::i18n::Lang::En,
            &slots,
            296.0,
            260.0,
            624.0,
        );
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
    showcase::draw_row(
        &mut none,
        &Theme::reef(),
        pinchora_core::infra::i18n::Lang::En,
        &[],
        296.0,
        260.0,
        624.0,
    );
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

/// What a test changes about the card it draws over a flat picture.
#[derive(Default)]
struct Variation<'a> {
    name: Option<&'a str>,
    bio: Option<&'a str>,
    partner: Option<&'a str>,
    /// The PNG of a 60×100 character in the corner.
    character: Option<&'a [u8]>,
}

/// The card as pixels, drawn over a picture that is one colour everywhere.
fn over_flat(rgb: [u8; 3], theme: Theme, variation: Variation<'_>) -> RgbaImage {
    let picture = encode_png(&RgbaImage::from_pixel(
        960,
        540,
        image::Rgba([rgb[0], rgb[1], rgb[2], 255]),
    ))
    .expect("encodes");
    let avatar = placeholder_avatar(1, ProfileCard::AVATAR_SIZE);
    let mut card = profile(
        Some(Backdrop {
            art: Some(SpriteArt {
                png: &picture,
                w: 960,
                h: 540,
            }),
            accent: "#8eddd1",
        }),
        &[],
    );
    card.avatar_png = &avatar;
    card.theme = theme;
    card.display_name = variation.name.unwrap_or(card.display_name);
    card.bio = variation.bio.unwrap_or(card.bio);
    card.partner = variation.partner.map(str::to_owned);
    card.character = variation.character.map(|png| SpriteArt { png, w: 60, h: 100 });
    let png = Renderer::new().profile_card(&card).expect("renders");
    image::load_from_memory(&png).expect("a png").to_rgba8()
}

/// WCAG contrast of two colours.
fn contrast(a: u32, b: u32) -> f32 {
    let (a, b) = (crate::style::luminance(a) + 0.05, crate::style::luminance(b) + 0.05);
    a.max(b) / a.min(b)
}

fn rgb_of(px: &image::Rgba<u8>) -> u32 {
    (u32::from(px[0]) << 16) | (u32::from(px[1]) << 8) | u32::from(px[2])
}

fn hex_of(color: &str) -> u32 {
    u32::from_str_radix(color.trim_start_matches('#'), 16).expect("a colour")
}

#[test]
fn small_labels_stay_readable_at_every_opacity_over_the_brightest_picture() {
    // Plain white is the worst a background can be. Under the three places loose or panel
    // text sits (a panel, the plate behind the name, and the footer's two plates) the
    // muted label colour keeps the contrast WCAG asks of small text, and the ink the
    // higher one of body text.
    for opacity in Opacity::ALL {
        let theme = Theme::reef().with_style(opacity, Frame::Plain);
        let (muted, ink) = (hex_of(&theme.muted), hex_of(&theme.ink));
        let card = over_flat([255, 255, 255], theme, Variation::default());
        for (what, x, y) in [
            ("a stat panel", 150, 340),
            ("the name's plate", 900, 60),
            ("the footer's plates", 640, 500),
        ] {
            let under = rgb_of(card.get_pixel(x, y));
            assert!(
                contrast(muted, under) >= 4.5,
                "{opacity:?}: muted on {what} is {:.2} over #{under:06x}",
                contrast(muted, under)
            );
            assert!(
                contrast(ink, under) >= 7.0,
                "{opacity:?}: ink on {what} is {:.2}",
                contrast(ink, under)
            );
        }
    }
    // The lift is for see-through panels only: the others keep the colour they always had.
    assert_eq!(
        Theme::reef().with_style(Opacity::Solid, Frame::Plain).muted,
        Theme::reef().muted
    );
    assert_ne!(
        Theme::reef().with_style(Opacity::Light, Frame::Plain).muted,
        Theme::reef().muted
    );
    // And asking twice does not lift it twice.
    let once = Theme::reef().with_style(Opacity::Light, Frame::Plain);
    assert_eq!(once.clone().with_style(Opacity::Light, Frame::Rope).muted, once.muted);
}

#[test]
fn the_footer_has_a_plate_of_its_own_over_a_picture() {
    let darkness = |card: &RgbaImage, y: u32| card.get_pixel(640, y)[0];
    for opacity in Opacity::ALL {
        let theme = Theme::reef().with_style(opacity, Frame::Plain);
        let card = over_flat([255, 255, 255], theme, Variation::default());
        // Above the XP label the one plate shows; in the footer the second one is stacked on it.
        assert!(
            darkness(&card, 500) < darkness(&card, 406),
            "{opacity:?}: the footer is darker than the XP rows ({} vs {})",
            darkness(&card, 500),
            darkness(&card, 406)
        );
    }
}

#[test]
fn a_partners_heart_sits_next_to_the_name_whatever_it_is_written_in() {
    // The gap between the heart and the first ink of the name, read off the picture. The
    // heart is red, the name cream, the panel navy.
    let gap = |partner: &str| -> i32 {
        let card = over_flat(
            [0, 0, 0],
            Theme::reef(),
            Variation {
                partner: Some(partner),
                ..Default::default()
            },
        );
        let (mut heart_right, mut name_left) = (0, i32::MAX);
        for y in 154..180 {
            for x in 400..900 {
                let px = card.get_pixel(x, y);
                if px.0[..3] == [0xff, 0x4d, 0x6d] {
                    heart_right = heart_right.max(x as i32);
                }
            }
        }
        for y in 154..180 {
            for x in (heart_right + 1) as u32..900 {
                if card.get_pixel(x, y).0[..3] == [0xff, 0xf1, 0xe8] {
                    name_left = name_left.min(x as i32);
                }
            }
        }
        assert!(
            heart_right > 0 && name_left < i32::MAX,
            "{partner:?}: heart and name are drawn"
        );
        name_left - heart_right
    };
    let plain = gap("Marlow");
    assert!((6..=16).contains(&plain), "ASCII: {plain}");
    for partner in ["Zoë ✨ 李", "Ñandú Çelik", "Анна", "李李李", "👨‍👩‍👧‍👦", "ŁŁŁ"] {
        let g = gap(partner);
        assert!((plain - 5..=plain + 5).contains(&g), "{partner:?}: {g} against {plain}");
    }
}

#[test]
fn a_name_never_runs_into_the_character_or_off_the_card() {
    // The ink of the name: cream pixels in its rows. With a character in the corner the name
    // ends before it; without one it ends inside the card, whatever the letters.
    let png = scene();
    let ink_right = |name: &str, with_character: bool| -> u32 {
        let card = over_flat(
            [0, 0, 0],
            Theme::reef(),
            Variation {
                name: Some(name),
                character: with_character.then_some(&png[..]),
                ..Default::default()
            },
        );
        let mut right = 0;
        for y in 44..92 {
            for x in 290..960 {
                if card.get_pixel(x, y).0[..3] == [0xff, 0xf1, 0xe8] {
                    right = right.max(x);
                }
            }
        }
        right
    };
    for name in [
        "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW",
        "MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM",
        "Bartholomew Fizzlebottom Smythe!",
        "Zoë ✨ 李 Zoë ✨ 李 Zoë ✨ 李",
        "ЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖ",
    ] {
        let with = ink_right(name, true);
        assert!(with > 296 && with < 832, "{name:?} with a character ends at {with}");
        let without = ink_right(name, false);
        assert!(without > 296 && without < 924, "{name:?} alone ends at {without}");
    }
    // A short name is left whole.
    assert!(ink_right("Pinch", true) < 500);
}

#[test]
fn a_bio_of_wide_letters_stops_at_the_panels_edge_or_the_character() {
    let png = scene();
    let ink_right = |bio: &str, with_character: bool| -> u32 {
        let card = over_flat(
            [0, 0, 0],
            Theme::reef(),
            Variation {
                bio: Some(bio),
                character: with_character.then_some(&png[..]),
                ..Default::default()
            },
        );
        let mut right = 0;
        for y in 172..262 {
            for x in 310..960 {
                if card.get_pixel(x, y).0[..3] == [0xff, 0xf1, 0xe8] {
                    right = right.max(x);
                }
            }
        }
        right
    };
    for bio in [
        "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW",
        "MMMMMM WWWWWW MMMMMM WWWWWW MMMMMM WWWWWW MMMMMM WWWWWW",
        "SHOUTING IS WHAT SOME PLAYERS DO IN THEIR ABOUT ME PANELS, LOUDLY AND WIDELY",
    ] {
        let with = ink_right(bio, true);
        assert!(with > 316 && with <= 826, "{bio:?} beside a character ends at {with}");
        let without = ink_right(bio, false);
        assert!(without > 316 && without <= 904, "{bio:?} alone ends at {without}");
    }
}

#[test]
fn showcase_art_and_shadows_sit_on_whole_pixels() {
    let png = scene();
    for count in 1..=4 {
        let slots: Vec<_> = [
            ShowcaseKind::Fish,
            ShowcaseKind::Pet,
            ShowcaseKind::Furniture,
            ShowcaseKind::House,
        ]
        .into_iter()
        .take(count)
        .map(|kind| tile(kind, &png, "Name"))
        .collect();
        let mut svg = String::new();
        showcase::draw_row(
            &mut svg,
            &Theme::reef(),
            pinchora_core::infra::i18n::Lang::En,
            &slots,
            296.0,
            254.0,
            624.0,
        );
        // Every image and every shadow band starts on a whole pixel and keeps whole sizes.
        for tag in svg.split("<image ").skip(1) {
            let head = tag.split("href=").next().expect("attributes");
            for number in head.split('"').skip(1).step_by(2).take(4) {
                assert!(!number.contains('.'), "{count} tiles: {head}");
            }
            assert!(head.contains(r#"image-rendering="optimizeSpeed""#) || head.contains("preserveAspectRatio"));
        }
        for path in svg.split(r##"fill="#000000" fill-opacity"##).take(1) {
            assert!(!path.contains('.') || !path.contains("<path d=\"M"), "{path}");
        }
    }
    let mut shadow = String::new();
    crate::svg::pixel_shadow(&mut shadow, 100.0, 50.0, 20.0, 4.0, 0.3);
    assert!(!shadow.contains('.') || shadow.contains("0.3"), "{shadow}");
    // Four bands, two pixels each, the middle ones the widest, all centred on x = 100.
    assert_eq!(shadow.matches('M').count(), 4, "{shadow}");
    assert_eq!(shadow.matches("v2").count(), 4);
}

#[test]
fn style_shows_on_the_profile_rank_and_wallet_cards() {
    let themed = |opacity, frame| Theme::reef().with_style(opacity, frame);
    let rank = |theme| RankCard { theme, ..rank(None) };
    let wallet = |theme| WalletCard {
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
