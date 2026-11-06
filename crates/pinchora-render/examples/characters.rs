//! Contact sheets of the character core: `cargo run -p pinchora-render --example characters -- <out-dir>`
//! (default `target/render-samples`). `characters-looks.png` is every look wearing a hat, glasses
//! and a hand item; `characters-<slot>.png` shows every accessory of a slot on a few looks;
//! `characters-colours.png` steps each colour channel.

use image::{Rgba, RgbaImage, imageops};
use pinchora_render::{
    CharacterCard, EditorCell, EditorSection, Renderer, Theme,
    character::{
        ACCESSORIES, Channel, CharacterSpec, Colors, Paint, Slot, Worn, accessory_icon, atlas, render_character,
    },
};

const BG: Rgba<u8> = Rgba([0xdc, 0xe4, 0xf2, 255]);

fn looks() -> Vec<String> {
    let mut ids: Vec<String> = atlas()
        .names()
        .filter_map(|n| n.strip_prefix("a_").map(str::to_owned))
        .collect();
    ids.sort();
    ids
}

fn spec(look: &str, n: usize, wear: &[(&str, Option<u8>)]) -> CharacterSpec {
    let pick = |k: usize| Paint::Swatch(((n * k + k) % 20) as u8);
    CharacterSpec {
        look: look.to_owned(),
        colors: Colors {
            skin: Paint::Swatch((n % 12) as u8),
            hair: pick(7),
            eyes: pick(3),
            top: pick(5),
            bottom: pick(11),
            shoes: pick(13),
        },
        accessories: wear
            .iter()
            .map(|(id, tint)| Worn {
                id: (*id).to_owned(),
                tint: tint.map(Paint::Swatch),
            })
            .collect(),
        pose: Default::default(),
    }
}

/// Lays pictures out `cols` a row in cells of the largest size, feet on a common line.
fn sheet(pictures: &[RgbaImage], cols: usize, scale: u32) -> RgbaImage {
    let cw = pictures.iter().map(RgbaImage::width).max().unwrap_or(1) * scale + 8;
    let ch = pictures.iter().map(RgbaImage::height).max().unwrap_or(1) * scale + 8;
    let rows = pictures.len().div_ceil(cols);
    let mut out = RgbaImage::from_pixel(cw * cols as u32, ch * rows as u32, BG);
    for (i, p) in pictures.iter().enumerate() {
        let big = imageops::resize(p, p.width() * scale, p.height() * scale, imageops::FilterType::Nearest);
        let x = (i % cols) as u32 * cw + (cw - big.width()) / 2;
        let y = (i / cols) as u32 * ch + ch - 4 - big.height();
        imageops::overlay(&mut out, &big, i64::from(x), i64::from(y));
    }
    out
}

fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/render-samples".into());
    std::fs::create_dir_all(&out)?;
    let looks = looks();

    // Every look with a hat, glasses and something in hand, rotating through them.
    let hats = [
        "straw_hat",
        "beanie",
        "sailor_cap",
        "captain_hat",
        "top_hat",
        "witch_hat",
        "crown",
        "bow",
    ];
    let faces = ["round_glasses", "sunglasses", "heart_glasses", "round_glasses"];
    let hands = ["ice_cream", "balloon", "lantern", "flower", "star_wand", "fishing_rod"];
    let started = std::time::Instant::now();
    let dressed: Vec<RgbaImage> = looks
        .iter()
        .enumerate()
        .map(|(n, look)| {
            let wear = [
                (hats[n % hats.len()], Some((n * 5 % 16) as u8)),
                (faces[n % faces.len()], None),
                (hands[n % hands.len()], Some((n * 3 % 16) as u8)),
            ];
            render_character(&spec(look, n, &wear), 1)
        })
        .collect();
    println!("{} characters in {:?}", dressed.len(), started.elapsed());
    sheet(&dressed, 8, 3).save(format!("{out}/characters-looks.png"))?;
    let bare: Vec<RgbaImage> = looks
        .iter()
        .enumerate()
        .map(|(n, l)| render_character(&spec(l, n, &[]), 1))
        .collect();
    sheet(&bare, 8, 3).save(format!("{out}/characters-bare.png"))?;

    // Every accessory of each slot on four different looks.
    let models = ["tidewalker", "puffcloud", "bobbin", "capewind"];
    for slot in Slot::ALL {
        let mut pictures = Vec::new();
        for (m, look) in models.iter().enumerate() {
            for art in ACCESSORIES.iter().filter(|a| a.slot == slot) {
                pictures.push(render_character(&spec(look, m + 2, &[(art.id, None)]), 1));
            }
        }
        let cols = ACCESSORIES.iter().filter(|a| a.slot == slot).count();
        sheet(&pictures, cols, 3).save(format!("{out}/characters-{}.png", slot.key()))?;
    }

    // Each channel stepped through its swatches on one look.
    let mut rows = Vec::new();
    for channel in Channel::ALL {
        for i in 0..channel.swatches().len() {
            let mut s = spec("bobbin", 1, &[]);
            s.colors.set(channel, Paint::Swatch(i as u8));
            rows.push(render_character(&s, 1));
        }
    }
    sheet(&rows, 20, 2).save(format!("{out}/characters-colours.png"))?;

    // Release timing of one editor preview's character.
    let s = spec(
        "curlcove",
        3,
        &[("witch_hat", Some(4)), ("scarf", None), ("balloon", Some(8))],
    );
    let started = std::time::Instant::now();
    for _ in 0..20 {
        let _ = render_character(&s, 1);
    }
    println!("one character: {:?}", started.elapsed() / 20);
    for wear in [
        &[][..],
        &[("witch_hat", None)],
        &[("scarf", None)],
        &[("balloon", None)],
        &[("cape", None)],
    ] {
        let s = spec("curlcove", 3, wear);
        let started = std::time::Instant::now();
        for _ in 0..20 {
            let _ = render_character(&s, 1);
        }
        println!("{wear:?}: {:?}", started.elapsed() / 20);
    }

    // A whole editor preview, as one click of the Accessories tab draws it: the character, the
    // worn strip, the slot's icons, and the card to PNG.
    let r = Renderer::new();
    let s = spec(
        "curlcove",
        3,
        &[("witch_hat", Some(4)), ("scarf", None), ("balloon", Some(8))],
    );
    let started = std::time::Instant::now();
    let runs = 10;
    let mut png = Vec::new();
    for _ in 0..runs {
        let wearing = Slot::ALL
            .iter()
            .map(|slot| EditorCell {
                art: s
                    .accessories
                    .iter()
                    .find(|w| ACCESSORIES.iter().any(|a| a.id == w.id && a.slot == *slot))
                    .and_then(|w| accessory_icon(&w.id, w.tint, 1)),
                label: slot.label().into(),
                current: *slot == Slot::Head,
                ..Default::default()
            })
            .collect();
        let items = ACCESSORIES
            .iter()
            .filter(|a| a.slot == Slot::Head)
            .map(|a| EditorCell {
                art: accessory_icon(a.id, None, 1),
                label: a.id.into(),
                note: "900".into(),
                ..Default::default()
            })
            .collect();
        png = r.character_card(&CharacterCard {
            character: render_character(&s, 1),
            name: "Captain Pinch".into(),
            status: ("DRAFT".into(), false),
            tabs: [
                "Look",
                "Skin",
                "Hair",
                "Eyes",
                "Top",
                "Bottom",
                "Shoes",
                "Accessories",
                "Presets",
                "Outfits",
            ]
            .map(String::from)
            .into(),
            tab: 7,
            caption: "Head · Coral Witch Hat · Lemon".into(),
            sections: vec![
                EditorSection {
                    title: "Wearing".into(),
                    cols: 5,
                    cells: wearing,
                },
                EditorSection {
                    title: "Head".into(),
                    cols: 5,
                    cells: items,
                },
            ],
            hint: "< > step · the menu jumps · save keeps it".into(),
            theme: Theme::reef(),
        })?;
    }
    println!("editor preview: {:?}", started.elapsed() / runs);
    std::fs::write(format!("{out}/character-editor.png"), png)?;
    Ok(())
}
