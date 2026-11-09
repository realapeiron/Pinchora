//! Renders the collection cards with sample data: `cargo run -p pinchora-render --example collection -- <out-dir>`.

use image::{RgbaImage, imageops};
use pinchora_render::{
    BottleCard, DexCard, DexCell, DropCard, DropCritter, EntryPicture, HaulCard, HaulTile, PotCard, PotState, Renderer,
    Theme,
    critters::{CRITTER_SHAPES, FIND_SHAPES, draw},
    encode_png,
};

/// A drawn shape scaled `factor` times.
fn picture(shape: &str, body: &str, accent: &str, pattern: &str, factor: u32) -> EntryPicture {
    let sprite = draw(shape, body, accent, pattern).expect("a shape");
    let (w, h) = (sprite.width() * factor, sprite.height() * factor);
    let big = imageops::resize(&sprite, w, h, imageops::FilterType::Nearest);
    EntryPicture {
        png: encode_png(&big).expect("encodes"),
        w,
        h,
    }
}

fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/render-samples".into());
    std::fs::create_dir_all(&out)?;
    let r = Renderer::new();

    // A contact sheet of every shape in three palettes and patterns.
    let looks = [
        ("sand", "coral", "plain"),
        ("moss", "gold", "stripes"),
        ("plum", "mint", "spots"),
    ];
    let shapes: Vec<&str> = CRITTER_SHAPES.iter().chain(&FIND_SHAPES).copied().collect();
    let scale = 6;
    let (cell_w, cell_h) = (16 * scale + 8, 16 * scale + 8);
    let mut sheet = RgbaImage::from_pixel(
        cell_w * looks.len() as u32 * 2,
        cell_h * shapes.len().div_ceil(2) as u32,
        image::Rgba([0x1d, 0x2b, 0x53, 255]),
    );
    for (i, name) in shapes.iter().enumerate() {
        for (j, (body, accent, pattern)) in looks.iter().enumerate() {
            let sprite = draw(name, body, accent, pattern).expect("a shape");
            let big = imageops::resize(
                &sprite,
                sprite.width() * scale,
                sprite.height() * scale,
                imageops::FilterType::Nearest,
            );
            let col = (i % 2) as u32 * looks.len() as u32 + j as u32;
            let row = (i / 2) as u32;
            imageops::overlay(&mut sheet, &big, (col * cell_w + 4) as i64, (row * cell_h + 4) as i64);
        }
    }
    sheet.save(format!("{out}/critter-shapes.png"))?;

    // A critter drop with one of each look of rarity.
    let drop = DropCard {
        title: "Critter drop".into(),
        subtitle: "Grab one!".into(),
        cards: vec![
            DropCritter {
                art: picture("crab", "sand", "coral", "plain", 9),
                name: "Pebblesnap Crab".into(),
                rarity: "Common".into(),
                frame: 0x8fa0d8,
                habitat: "Shore".into(),
                sparkles: 0,
            },
            DropCritter {
                art: picture("jelly", "lilac", "mint", "spots", 9),
                name: "Dreamdrift Jelly".into(),
                rarity: "Rare".into(),
                frame: 0x3a9bdc,
                habitat: "Deep water".into(),
                sparkles: 1,
            },
            DropCritter {
                art: picture("seahorse", "cream", "gold", "spots", 9),
                name: "Moonpearl Seahorse".into(),
                rarity: "Legendary".into(),
                frame: 0xffd166,
                habitat: "Deep water".into(),
                sparkles: 3,
            },
        ],
        footer: "Press 1, 2 or 3 · one card each · gone in 90 seconds".into(),
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/collection-drop.png"), r.drop_card(&drop)?)?;

    // The bottle.
    for seed in [0u32, 7] {
        let bottle = BottleCard {
            title: "Message in a bottle".into(),
            subtitle: "Washed ashore".into(),
            seed,
            theme: Theme::reef(),
        };
        std::fs::write(format!("{out}/collection-bottle-{seed}.png"), r.bottle_card(&bottle)?)?;
    }

    // The dex: a page with some found and some not.
    let rarity_colors = [0x8fa0d8, 0x5bbf6a, 0x3a9bdc, 0x9b6bc4, 0xffd166];
    let palettes = ["sand", "moss", "plum", "sky", "rose", "gold", "mint", "navy"];
    let cells: Vec<DexCell> = (0..18)
        .map(|i| DexCell {
            number: i + 1,
            name: [
                "Pebblesnap Crab",
                "Buttonstar",
                "Mossback Snail",
                "Spindle Urchin",
                "Gumdrop Jelly",
                "Wobblecap Anemone",
            ][i % 6]
                .into(),
            art: picture(
                CRITTER_SHAPES[i % 14],
                palettes[i % 8],
                palettes[(i + 3) % 8],
                ["plain", "stripes", "spots"][i % 3],
                5,
            ),
            frame: rarity_colors[i % 5],
            owned: i % 3 != 2,
            caught: if i % 4 == 0 { 3 } else { 1 },
        })
        .collect();
    let dex = DexCard {
        title: "Tide pool dex".into(),
        tab: "CRITTERS".into(),
        owner: "Captain Pinch".into(),
        found: 12,
        total: 48,
        page: 0,
        pages: 3,
        cells,
        footer: "FRAGMENTS 134".into(),
        lang: pinchora_core::infra::i18n::Lang::En,
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/collection-dex.png"), r.dex_card(&dex)?)?;

    // A haul: one thing, then a crab pot's worth.
    let tile = |shape: &str, name: &str, rarity: &str, frame: u32, tag: &str, line: &str| HaulTile {
        art: picture(shape, "sand", "coral", "plain", 8),
        name: name.into(),
        rarity: rarity.into(),
        frame,
        tag: tag.into(),
        line: line.into(),
        flavor: "Hums quietly when nobody is listening, and louder when somebody is.".into(),
    };
    let single = HaulCard {
        title: "Beachcomb".into(),
        subtitle: "Low tide".into(),
        tiles: vec![tile(
            "conch",
            "Rosy Spiral Conch",
            "Uncommon",
            0x5bbf6a,
            "NEW",
            "+35 shells",
        )],
        footer: "Next walk in 20 minutes".into(),
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/collection-haul-1.png"), r.haul_card(&single)?)?;
    let grid = HaulCard {
        title: "Crab pot".into(),
        subtitle: "8 h soak".into(),
        tiles: (0..7)
            .map(|i| {
                tile(
                    CRITTER_SHAPES[i * 2 % 14],
                    ["Pebblesnap Crab", "Buttonstar", "Mossback Snail", "Spindle Urchin"][i % 4],
                    "Common",
                    0x8fa0d8,
                    if i % 2 == 0 { "NEW" } else { "x2" },
                    "+6 shells",
                )
            })
            .collect(),
        footer: "Total +64 shells, +9 fragments".into(),
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/collection-haul-7.png"), r.haul_card(&grid)?)?;

    // The pot in each state.
    for (name, state) in [
        ("empty", PotState::Empty),
        (
            "soaking",
            PotState::Soaking {
                left: "3 h 20 m".into(),
                fraction: 0.58,
                hours: 8,
            },
        ),
        ("ready", PotState::Ready { hours: 4 }),
    ] {
        let pot = PotCard {
            level: 2,
            max_level: 5,
            state,
            rate: "1.75 catches an hour".into(),
            upgrade: "Level 3 costs 1,000 shells".into(),
            lang: pinchora_core::infra::i18n::Lang::En,
            theme: Theme::reef(),
        };
        std::fs::write(format!("{out}/collection-pot-{name}.png"), r.pot_card(&pot)?)?;
    }
    Ok(())
}
