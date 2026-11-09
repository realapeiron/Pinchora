//! The earned and locked achievement card stays readable as its pixel golden.

use std::path::Path;

use pinchora_core::infra::i18n::Lang;
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
        kicker: pinchora_core::t!(lang, "achievements.card.kicker"),
        earned_label: pinchora_core::t!(lang, "achievements.card.earned"),
        locked_label: pinchora_core::t!(lang, "achievements.card.locked"),
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
    for (lang, file) in [(Lang::En, "achievements.png"), (Lang::Tr, "achievements_tr.png")] {
        let card = card(lang);
        if lang == Lang::Tr {
            let svg = card.to_svg();
            for label in ["SUNUCU BAŞARIMLARI", "KAZANILDI", "KİLİTLİ"] {
                assert!(svg.contains(label), "{label}");
            }
            for label in ["SERVER ACHIEVEMENTS", "EARNED", "LOCKED"] {
                assert!(!svg.contains(label));
            }
        }
        let png = renderer.achievements_card(&card).unwrap();
        let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(file);
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
    }
}

#[test]
fn turkish_achievement_progress_uses_local_separators() {
    let mut card = card(Lang::Tr);
    card.cells[0].progress = 1234;
    card.cells[0].target = 10000;
    assert!(card.to_svg().contains("1.234/10.000"));
}
