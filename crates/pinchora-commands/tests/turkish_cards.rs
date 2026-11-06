//! Turkish content names are longer than the English ones and use letters (ç ğ ı ö ş ü İ) the pixel font may lack.
//! These tests draw the cards that show content names with the longest Turkish names and save the PNGs under
//! `target/sim/turkish_cards/` so they can be looked at.

use std::path::Path;

use image::{Rgba, RgbaImage};
use pinchora_core::i18n::Lang;
use pinchora_game::Content;
use pinchora_render::{GridSlot, ItemGridCard, Renderer, Theme, encode_png};

fn out(name: &str) -> std::path::PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .parent()
        .expect("target dir")
        .join("sim")
        .join("turkish_cards");
    std::fs::create_dir_all(&dir).expect("create the output dir");
    dir.join(name)
}

fn is_png(png: &[u8]) -> bool {
    png.starts_with(b"\x89PNG\r\n\x1a\n") && png.len() > 2_000
}

fn blank_sprite() -> Vec<u8> {
    encode_png(&RgbaImage::from_pixel(96, 88, Rgba([200, 150, 110, 255]))).expect("encodes")
}

/// The eight items whose Turkish names are the longest, as the shop catalogue would show them.
#[test]
fn the_item_grid_holds_the_longest_turkish_names() {
    let content = Content::get();
    let mut items: Vec<_> = content.items().collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.name_in(Lang::Tr).chars().count()));
    let slots: Vec<GridSlot> = items
        .iter()
        .take(8)
        .map(|i| GridSlot {
            sprite_png: blank_sprite(),
            sprite_w: 96,
            sprite_h: 88,
            name: i.name_in(Lang::Tr),
            line1: "12.500".to_owned(),
            line2: format!(
                "{} · {}×{}",
                i.base.rarity.label_in(Lang::Tr).to_uppercase(),
                i.base.footprint[0],
                i.base.footprint[1]
            ),
            bar: None,
            frame: i.base.rarity.color(),
            badge: i.base.category.label_in(Lang::Tr).to_uppercase(),
            shell_price: true,
        })
        .collect();
    let card = ItemGridCard {
        title: format!("{} katalog", pinchora_game::Category::WindowsDoors.label_in(Lang::Tr)),
        subtitle: "SAYFA 1/3".to_owned(),
        slots,
        footer: "BU REYONDA 24 ÜRÜN VAR".to_owned(),
        theme: Theme::reef(),
    };
    let png = Renderer::new().item_grid(&card).expect("renders");
    assert!(is_png(&png));
    std::fs::write(out("item_grid_longest.png"), png).expect("write png");
}
