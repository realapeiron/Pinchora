//! Cards drawn from a server's design: the classic template is the old welcome card pixel for pixel, every
//! template matches its golden picture, and nothing a member's name holds stops a card from drawing.
//!
//! Rewrite the goldens after changing a template on purpose:
//! `PINCHORA_UPDATE_GOLDENS=1 cargo test -p pinchora-render --test designed`. Every card is also saved
//! full size under `target/designed/` to look at.

use std::path::{Path, PathBuf};

use image::{RgbaImage, imageops::FilterType};
use pinchora_core::{
    card_design::{Background, CardDesign, CardKind, Content, Direction, check_design},
    i18n::Lang,
    t,
};
use pinchora_render::{
    CardFill, DesignedCard, Renderer, Theme, WelcomeCard, character::atlas, fill_placeholders, ordinal,
    placeholder_avatar, templates,
};

const FILL: CardFill<'static> = CardFill {
    lang: pinchora_core::i18n::Lang::En,
    user: "Captain Pinch",
    username: "captain.pinch",
    server: "Pinchora Reef",
    members: Some(1234),
};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn out_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/designed");
    std::fs::create_dir_all(&dir).expect("output folder");
    dir
}

fn draw(design: &CardDesign, fill: &CardFill<'_>) -> RgbaImage {
    let avatar = placeholder_avatar(7, design.avatar_size().unwrap_or(176));
    let card = DesignedCard {
        design,
        avatar_png: &avatar,
        upload_png: None,
        fill,
    };
    let png = Renderer::new().designed_card(&card).expect("renders");
    image::load_from_memory(&png).expect("a png").to_rgba8()
}

/// How many pixels differ by more than `tolerance` in some channel.
fn differing(a: &RgbaImage, b: &RgbaImage, tolerance: u8) -> usize {
    assert_eq!(a.dimensions(), b.dimensions());
    a.pixels()
        .zip(b.pixels())
        .filter(|(p, q)| p.0.iter().zip(q.0).any(|(x, y)| x.abs_diff(y) > tolerance))
        .count()
}

#[test]
fn the_classic_template_is_the_old_welcome_card() {
    let r = Renderer::new();
    let avatar = placeholder_avatar(7, WelcomeCard::AVATAR_SIZE);
    for theme in Theme::PRESETS {
        for (title, accent, lang) in [
            (None, None, Lang::En),
            (Some("Hi there"), Some(0x06d6a0), Lang::En),
            (None, None, Lang::Tr),
            (Some("İyi ki geldin"), None, Lang::Tr),
        ] {
            let old = WelcomeCard {
                display_name: FILL.user,
                avatar_png: &avatar,
                server_name: FILL.server,
                member_number: FILL.members,
                title: &title.map_or_else(|| t!(lang, "events.default-welcome-title"), str::to_owned),
                lang,
                theme: Theme::preset(theme).with_accent(accent),
            };
            let old = image::load_from_memory(&r.welcome_card(&old).expect("renders"))
                .expect("a png")
                .to_rgba8();
            let design = templates::classic(CardKind::Welcome, theme, title, accent, lang);
            let fill = CardFill { lang, ..FILL };
            let new = draw(&design, &fill);
            assert_eq!(differing(&old, &new, 0), 0, "{theme} {title:?} {accent:?} {lang:?}");
        }
    }
}

/// A Turkish server's classic cards: the default headline in Turkish capitals (the dotted `İ`) and the line in
/// Turkish, the member placeholders kept for the fill. Saved under `target/designed/` to look at.
#[test]
fn the_classic_card_speaks_the_server_language() {
    let texts = |design: &CardDesign| -> Vec<String> {
        design
            .layers
            .iter()
            .filter_map(|layer| match &layer.content {
                Content::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    let welcome = templates::classic(CardKind::Welcome, "reef", None, None, Lang::Tr);
    assert_eq!(texts(&welcome), ["HOŞ GELDİN", "{user}", "{server} · {members}. üye"]);
    let goodbye = templates::classic(CardKind::Goodbye, "sunset", None, None, Lang::Tr);
    assert_eq!(
        texts(&goodbye),
        [
            "GÜLE GÜLE",
            "{user}",
            "{server} sunucusundan ayrıldı · artık {members} üye"
        ]
    );
    // A headline the server wrote keeps its words, in Turkish capitals.
    let own = templates::classic(CardKind::Welcome, "reef", Some("Aramıza hoş geldin"), None, Lang::Tr);
    assert_eq!(texts(&own)[0], "ARAMIZA HOŞ GELDİN");
    for (design, name) in [(welcome, "classic-tr-welcome.png"), (goodbye, "classic-tr-goodbye.png")] {
        draw(&design, &FILL).save(out_dir().join(name)).expect("saved");
    }
}

#[test]
fn every_template_is_valid_and_matches_its_golden() {
    let update = std::env::var_os("PINCHORA_UPDATE_GOLDENS").is_some();
    let known = |id: &str| atlas().get(id).is_some();
    let mut failures = Vec::new();
    for (key, _) in templates::TEMPLATES {
        for kind in CardKind::ALL {
            let design = templates::template(key, kind).expect("a template");
            let checked = check_design(design.clone(), &known).unwrap_or_else(|e| panic!("{key}: {e:?}"));
            assert_eq!(checked, design, "{key} is already tidy");
            let card = draw(&design, &FILL);
            assert_eq!(card.dimensions(), (design.width, design.height));
            let name = format!("{key}-{}.png", kind.key());
            card.save(out_dir().join(&name)).expect("saved");
            // The classic goodbyes differ from the reef one only in colour; one golden covers them.
            if kind == CardKind::Goodbye && key.starts_with("classic-") && key != "classic-reef" {
                continue;
            }
            // A quarter of the size: enough to see a layer move, small enough to keep in the repository.
            let small = image::imageops::resize(&card, design.width / 4, design.height / 4, FilterType::Triangle);
            let path = golden_dir().join(&name);
            if update {
                std::fs::create_dir_all(golden_dir()).expect("golden folder");
                small.save(&path).expect("golden saved");
                continue;
            }
            let golden = image::open(&path)
                .unwrap_or_else(|e| panic!("{name}: {e}; write the goldens with PINCHORA_UPDATE_GOLDENS=1"))
                .to_rgba8();
            // Rasterisers on other CPUs may round a few edge pixels differently.
            let off = differing(&golden, &small, 6);
            if off * 200 > (small.width() * small.height()) as usize {
                failures.push(format!("{name}: {off} pixels differ"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(templates::template("classic-neon", CardKind::Welcome).is_none());
    assert!(templates::template("disco", CardKind::Welcome).is_none());
}

#[test]
fn placeholders_fill_in_and_hostile_names_still_draw() {
    assert_eq!(
        fill_placeholders("{user} ({username}) joined {server}: #{members}, the {ordinal}", &FILL),
        "Captain Pinch (captain.pinch) joined Pinchora Reef: #1,234, the 1,234th"
    );
    for (n, want) in [
        (1, "1st"),
        (2, "2nd"),
        (3, "3rd"),
        (4, "4th"),
        (11, "11th"),
        (12, "12th"),
        (13, "13th"),
    ] {
        assert_eq!(ordinal(n), want);
    }
    assert_eq!(ordinal(101), "101st");
    assert_eq!(ordinal(1112), "1,112th");
    let sneaky = CardFill {
        lang: pinchora_core::i18n::Lang::En,
        user: "{server}",
        ..FILL
    };
    assert_eq!(fill_placeholders("{user} {nope} {", &sneaky), "{server} {nope} {");
    let unknown = CardFill { members: None, ..FILL };
    assert_eq!(fill_placeholders("#{members} {ordinal}", &unknown), "#? newest");

    let hostile = "Zoë ✨ 李 😀 <b>&amp;\"'</b> \u{FFFE}\u{FFFF}\u{FDD0}\u{0}\u{7}\u{85} end";
    let fill = CardFill {
        lang: pinchora_core::i18n::Lang::En,
        user: hostile,
        username: hostile,
        server: hostile,
        members: Some(1),
    };
    for (key, _) in templates::TEMPLATES {
        let design = templates::template(key, CardKind::Welcome).expect("a template");
        let card = draw(&design, &fill);
        assert_eq!(card.width(), design.width);
    }
}

#[test]
fn gradients_draw_every_way_with_and_without_the_pixel_look() {
    let mut design = templates::template("clean-card", CardKind::Welcome).expect("a template");
    design.layers.clear();
    for direction in [Direction::Vertical, Direction::Horizontal, Direction::Diagonal] {
        for pixel in [false, true] {
            design.background = Background::Gradient {
                from: "#000000".to_owned(),
                to: "#ffffff".to_owned(),
                direction,
                pixel,
            };
            let card = draw(&design, &FILL);
            let at = |x: u32, y: u32| card.get_pixel(x, y).0[0];
            let (right, bottom) = (design.width - 2, design.height - 2);
            let (tl, tr, bl, br) = (at(1, 1), at(right, 1), at(1, bottom), at(right, bottom));
            let ok = match direction {
                Direction::Vertical => tl < 40 && bl > 200 && tl.abs_diff(tr) < 8,
                Direction::Horizontal => tl < 40 && tr > 200 && tl.abs_diff(bl) < 8,
                Direction::Diagonal => tl < 40 && br > 200 && (80..180).contains(&tr),
            };
            assert!(ok, "{direction:?} pixel {pixel}: {tl} {tr} {bl} {br}");
        }
    }
}

#[test]
fn hidden_and_invisible_layers_are_not_drawn() {
    let mut design = templates::template("clean-card", CardKind::Welcome).expect("a template");
    let full = draw(&design, &FILL);
    for layer in &mut design.layers {
        layer.hidden = true;
    }
    let empty = draw(&design, &FILL);
    assert!(
        empty.pixels().all(|p| p.0[3] == 0),
        "a transparent card with nothing on it"
    );
    assert!(full.pixels().any(|p| p.0[3] == 255));
    for layer in &mut design.layers {
        layer.hidden = false;
        layer.opacity = 0;
    }
    assert!(draw(&design, &FILL).pixels().all(|p| p.0[3] == 0));
}
