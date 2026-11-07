//! Stable reference images for the compact info cards.

use std::path::Path;

use image::{RgbaImage, imageops::FilterType};
use pinchora_core::{i18n::Lang, t};
use pinchora_render::{InfoCard, InfoRow, Renderer, Theme, placeholder_avatar};

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
    let mut cards = Vec::new();
    for lang in [Lang::En, Lang::Tr] {
        let suffix = if lang == Lang::Tr { "-tr" } else { "" };
        for (name, subtitle, title, values, image) in [
            (
                "info-member",
                "user",
                "Captain Pinch · İpek",
                vec![
                    ("account-created", "2024-01-15"),
                    ("joined", "2024-02-01"),
                    ("boost", "yes"),
                    ("roles", "3"),
                    ("level", "12"),
                    ("badges", "6"),
                    ("title", "Tide Angler"),
                    ("role", "Coral Crew"),
                    ("role", "Reef Guardian"),
                ],
                Some(placeholder_avatar(42, 208)),
            ),
            (
                "info-server",
                "server",
                "Pinchora Reef",
                vec![
                    ("owner", "123456789012345678"),
                    ("members", "1,234"),
                    ("roles", "18"),
                    ("created", "2023-06-01"),
                    ("boost", "24"),
                    ("pinchora-members", "986"),
                ],
                Some(placeholder_avatar(73, 208)),
            ),
            (
                "info-role",
                "role",
                "Coral Crew",
                vec![
                    ("id", "123456789012345678"),
                    ("color", "#FF6B4A"),
                    ("position", "12"),
                    ("mentionable", "yes"),
                    ("hoisted", "no"),
                ],
                None,
            ),
            (
                "info-channel",
                "channel",
                "reef-lounge",
                vec![
                    ("id", "123456789012345678"),
                    ("type", "text-channel"),
                    ("parent", "Community"),
                    ("topic", "A little corner of the reef"),
                ],
                None,
            ),
            (
                "info-members",
                "members",
                "Pinchora Reef",
                vec![("members", "1,234"), ("pinchora-members", "986")],
                Some(placeholder_avatar(73, 208)),
            ),
        ] {
            cards.push((
                format!("{name}{suffix}"),
                InfoCard {
                    lang,
                    title: title.into(),
                    subtitle: t!(lang, &format!("info.{subtitle}")),
                    rows: values
                        .into_iter()
                        .map(|(label, value)| {
                            row(
                                &t!(lang, &format!("info.{label}")),
                                &match value {
                                    "text-channel" => t!(lang, "info.text-channel"),
                                    "yes" => t!(lang, "info.yes"),
                                    "no" => t!(lang, "info.no"),
                                    _ => value.into(),
                                },
                            )
                        })
                        .collect(),
                    image,
                    theme: Theme::reef(),
                },
            ));
        }
    }
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
