//! Stable reference images for the compact info cards.

use std::path::Path;

use image::{RgbaImage, imageops::FilterType};
use pinchora_render::{InfoCard, InfoRow, Renderer, Theme};

fn row(label: &str, value: &str) -> InfoRow {
    InfoRow {
        label: label.into(),
        value: value.into(),
    }
}

fn differing(a: &RgbaImage, b: &RgbaImage) -> usize {
    assert_eq!(a.dimensions(), b.dimensions());
    a.pixels()
        .zip(b.pixels())
        .filter(|(p, q)| p.0.iter().zip(q.0).any(|(x, y)| x.abs_diff(y) > 6))
        .count()
}

#[test]
fn info_cards_match_goldens() {
    let cards = [
        (
            "info-member",
            InfoCard {
                title: "Captain Pinch".into(),
                subtitle: "MEMBER".into(),
                rows: vec![
                    row("ACCOUNT CREATED", "2024-01-15"),
                    row("JOINED", "2024-02-01"),
                    row("ROLES", "3 roles"),
                    row("LEVEL", "12"),
                    row("BADGES", "6"),
                    row("ROLE", "Coral Crew"),
                ],
                image: None,
                theme: Theme::reef(),
            },
        ),
        (
            "info-server",
            InfoCard {
                title: "Pinchora Reef".into(),
                subtitle: "SERVER".into(),
                rows: vec![
                    row("OWNER ID", "123456789"),
                    row("MEMBERS", "1,234"),
                    row("ROLES", "18"),
                    row("CREATED", "2023-06-01"),
                ],
                image: None,
                theme: Theme::reef(),
            },
        ),
    ];
    let update = std::env::var_os("PINCHORA_UPDATE_INFO_GOLDENS").is_some();
    for (name, card) in cards {
        let png = Renderer::new().svg_to_png(&card.to_svg()).expect("renders");
        let full = image::load_from_memory(&png).expect("PNG").to_rgba8();
        let small = image::imageops::resize(&full, full.width() / 2, full.height() / 2, FilterType::Triangle);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(format!("{name}.png"));
        if update {
            small.save(&path).expect("saves golden");
            continue;
        }
        let golden = image::open(&path).expect("golden exists").to_rgba8();
        let off = differing(&golden, &small);
        assert!(
            off * 200 <= (small.width() * small.height()) as usize,
            "{name}: {off} pixels differ"
        );
    }
}
