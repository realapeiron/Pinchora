//! The earned and locked achievement card stays readable as its pixel golden.

use std::path::Path;

use pinchora_core::i18n::Lang;
use pinchora_render::{AchievementCell, AchievementsCard, Renderer, Theme};

fn card(lang: Lang) -> AchievementsCard {
    AchievementsCard {
        lang,
        title: if lang == Lang::Tr {
            "Deniz Yıldızı adlı üyenin başarımları"
        } else {
            "Captain Crab's achievements"
        }
        .into(),
        kicker: if lang == Lang::Tr {
            "SUNUCU BAŞARIMLARI"
        } else {
            "SERVER ACHIEVEMENTS"
        }
        .into(),
        earned_label: if lang == Lang::Tr { "Kazanıldı" } else { "Earned" }.into(),
        locked_label: if lang == Lang::Tr { "Kilitli" } else { "Locked" }.into(),
        page_label: "1/2".into(),
        empty_label: "No achievements".into(),
        cells: vec![
            AchievementCell {
                name: "First pearl".into(),
                description: "Send ten messages".into(),
                icon: "star".into(),
                icon_png: None,
                progress: 10,
                target: 10,
                earned: true,
            },
            AchievementCell {
                name: "The long swim".into(),
                description: "Spend an hour in voice".into(),
                icon: "shell".into(),
                icon_png: None,
                progress: 28,
                target: 60,
                earned: false,
            },
            AchievementCell {
                name: "Yıldızlı geceler".into(),
                description: "Collect a hundred stars".into(),
                icon: "heart".into(),
                icon_png: None,
                progress: 7,
                target: 100,
                earned: false,
            },
        ],
        theme: Theme::reef(),
    }
}

#[test]
fn achievement_card_matches_golden_and_turkish_renders() {
    let renderer = Renderer::new();
    let png = renderer.achievements_card(&card(Lang::En)).unwrap();
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/achievements.png");
    if std::env::var_os("PINCHORA_UPDATE_GOLDENS").is_some() {
        std::fs::write(&expected, &png).unwrap();
    }
    let golden = image::open(&expected).expect("achievement golden missing").to_rgba8();
    let actual = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(golden.dimensions(), actual.dimensions());
    let different = golden
        .pixels()
        .zip(actual.pixels())
        .filter(|(a, b)| a.0.iter().zip(b.0).any(|(x, y)| x.abs_diff(y) > 6))
        .count();
    assert!(different < 100, "{different} pixels changed");
    let tr = renderer.achievements_card(&card(Lang::Tr)).unwrap();
    assert_eq!(image::load_from_memory(&tr).unwrap().width(), 900);
}
