//! The heaviest card the validator lets through still draws within the render budget.
//!
//! `pinchora_core::admin::card_design` caps what the layers may cost together (`LAYER_COST_MAX`), caps a layer at the
//! canvas, and allows four avatars. This builds the designs that spend that allowance on what costs the most
//! per unit, on the biggest canvas with the busiest background, and times them: in release the slowest must
//! draw in 400 ms. Run it with `cargo test --release -p pinchora-render --test cost -- --nocapture` (the
//! timing test is skipped in debug builds, where nothing is fast).
//!
//! Measured on an Apple M-series laptop, release, best of five, while the machine was loaded with other builds:
//! all text 270 ms, four avatars and the text that is left 232 ms, a pile of 20 faded layers 241 ms, all text
//! over a noisy upload 267 ms. The pixel gradient with the dimming alone is about 25 ms of each. What it took to
//! get there: charging text by its characters (emoji and CJK cost 0.6 ms each, outlined and faded), keeping
//! uploads to 1 MiB, and writing designed cards with the fast PNG level (the default one took 300 ms on a photo).

use std::time::Instant;

use image::RgbaImage;
use pinchora_core::admin::card_design::AVATAR_COST_FACTOR;
use pinchora_core::admin::card_design::AVATAR_MAX_SIDE;
use pinchora_core::admin::card_design::CardDesign;
use pinchora_core::admin::card_design::LAYER_COST_MAX;
use pinchora_core::admin::card_design::MAX_AVATARS;
use pinchora_core::admin::card_design::MAX_LAYERS;
use pinchora_core::admin::card_design::TEXT_CHAR_COST;
use pinchora_core::admin::card_design::UPLOAD_STORED_MAX_BYTES;
use pinchora_core::admin::card_design::layers_cost;
use pinchora_core::admin::card_design::parse_design;
use pinchora_render::{CardFill, DesignedCard, Renderer, character::atlas, encode_png, placeholder_avatar};
use serde_json::{Value, json};

const BUDGET_MS: f64 = 400.0;
const RUNS: usize = 5;
const UPLOAD_ID: &str = "0123456789abcdef0123456789abcdef";

const FILL: CardFill<'static> = CardFill {
    lang: pinchora_core::infra::i18n::Lang::En,
    user: "Captain Pinch the Third of Reef",
    username: "captain.pinch",
    server: "Pinchora Reef",
    members: Some(1234),
};

/// The busiest backgrounds: a pixel gradient across the card under the strongest dimming, and a noisy
/// uploaded picture of the biggest size the upload route keeps.
fn pixel_background() -> Value {
    json!({ "type": "gradient", "from": "#1d2b53", "to": "#0f1631", "direction": "diagonal", "pixel": true })
}

fn upload_background() -> Value {
    json!({ "type": "upload", "id": UPLOAD_ID })
}

fn design(background: Value, layers: Vec<Value>) -> Value {
    json!({
        "version": 1, "width": 1600, "height": 1000, "background": background, "dim": 90, "layers": layers,
    })
}

/// A faded avatar with its border, not pixelated: the dearest way to draw one.
fn avatar(i: usize, side: u32) -> Value {
    json!({
        "id": format!("avatar{i}"), "type": "avatar", "x": 0, "y": 0, "w": side, "h": side, "opacity": 50,
        "shape": "circle", "border_color": "#ffcc00", "border_width": 40, "pixelate": false,
    })
}

/// `chars` characters of emoji and CJK split by spaces (the fallback fonts' big outlines, and a font switch at
/// every word: the slowest text found), in the sans font, faded, outlined and shadowed.
fn text(i: usize, chars: usize) -> Value {
    let body: String = "😀🎉✨🦀🐙 李小龍 ".chars().cycle().take(chars).collect();
    json!({
        "id": format!("text{i}"), "type": "text", "x": 0, "y": 0, "w": 1600, "h": 125, "opacity": 50,
        "text": body, "font": "sans", "bold": true, "size": 24,
        "color": "#ffffff", "align": "center", "max_lines": 4, "shadow": true,
        "outline_color": "#000000", "outline_width": 12,
    })
}

/// A faded shape the size of the canvas with a border, or a faded sticker as big.
fn pile(i: usize) -> Value {
    if i % 6 == 5 {
        json!({
            "id": format!("pile{i}"), "type": "sprite", "x": 0, "y": 0, "w": 1500, "h": 1000, "opacity": 50,
            "sprite": "p_drowsepatch_cat", "color": "#ff4d6d",
        })
    } else {
        json!({
            "id": format!("pile{i}"), "type": "shape", "x": 0, "y": 0, "w": 1500, "h": 1000, "opacity": 50,
            "shape": "ellipse", "fill": "#336699", "border_color": "#ffcc00", "border_width": 40,
        })
    }
}

/// Every design here spends what it may and no more, the way that costs the most to draw.
fn worst_designs() -> Vec<(&'static str, Value)> {
    // The characters of text the allowance buys, in layers of at most 125.
    let texts = |chars: usize| {
        (0..chars.div_ceil(125))
            .map(|i| text(i, 125.min(chars - i * 125)))
            .collect::<Vec<_>>()
    };
    let all_text = (LAYER_COST_MAX / TEXT_CHAR_COST) as usize;
    // The room four avatars leave for text.
    let side = AVATAR_MAX_SIDE.min(1000);
    let left = LAYER_COST_MAX - MAX_AVATARS as u64 * AVATAR_COST_FACTOR * u64::from(side * side);
    let mut mixed: Vec<Value> = (0..MAX_AVATARS).map(|i| avatar(i, side)).collect();
    mixed.extend(texts((left / TEXT_CHAR_COST) as usize));
    vec![
        ("all text", design(pixel_background(), texts(all_text))),
        (
            "all text on a noisy upload",
            design(upload_background(), texts(all_text)),
        ),
        (
            "four avatars and the text that is left",
            design(pixel_background(), mixed),
        ),
        (
            "a pile of faded layers",
            design(pixel_background(), (0..MAX_LAYERS).map(pile).collect()),
        ),
    ]
}

fn catalog(id: &str) -> bool {
    atlas().get(id).is_some()
}

/// A picture of random pixels as big as the upload route keeps a noisy one: about a megabyte of PNG.
fn noisy_upload() -> Vec<u8> {
    let mut state = 12345u32;
    let noise = RgbaImage::from_fn(630, 410, |_, _| {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let v = state.to_le_bytes();
        image::Rgba([v[1], v[2], v[3], 255])
    });
    let png = encode_png(&noise).expect("encodes");
    assert!(png.len() <= UPLOAD_STORED_MAX_BYTES, "{} bytes", png.len());
    png
}

#[test]
fn the_worst_designs_are_valid_and_one_more_layer_is_not() {
    for (name, sent) in worst_designs() {
        let parsed = parse_design(&sent, &catalog).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let cost = layers_cost(&parsed.layers);
        assert!(
            cost <= LAYER_COST_MAX && cost * 10 >= LAYER_COST_MAX * 8,
            "{name}: spends {cost}"
        );
        // A heavier layer on top of what is spent is refused, even the cheapest kind that still costs.
        let mut more = sent.clone();
        let layers = more["layers"].as_array_mut().expect("layers");
        if layers.len() < MAX_LAYERS {
            layers.push(json!({
                "id": "one-more", "type": "shape", "x": 0, "y": 0, "w": 1600, "h": 1000, "opacity": 50,
                "shape": "rect", "fill": "#336699", "border_color": "#000000", "border_width": 0,
            }));
            assert!(parse_design(&more, &catalog).is_err(), "{name}: one more fits");
        }
    }
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timings only mean something in a release build")]
fn the_worst_designs_draw_within_the_budget() {
    let upload = noisy_upload();
    let avatar_png = placeholder_avatar(7, 512);
    let renderer = Renderer::new();
    let mut slowest: f64 = 0.0;
    for (name, sent) in worst_designs() {
        let design: CardDesign = parse_design(&sent, &catalog).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let card = DesignedCard {
            design: &design,
            avatar_png: &avatar_png,
            upload_png: design.upload().map(|_| upload.as_slice()),
            fill: &FILL,
        };
        let mut best = f64::MAX;
        for _ in 0..RUNS {
            let started = Instant::now();
            let png = renderer.designed_card(&card).expect("renders");
            best = best.min(started.elapsed().as_secs_f64() * 1000.0);
            assert!(png.len() > 100);
        }
        println!("{name:<42} {best:>6.0} ms (best of {RUNS})");
        slowest = slowest.max(best);
    }
    assert!(
        slowest <= BUDGET_MS,
        "the worst valid design takes {slowest:.0} ms, over {BUDGET_MS} ms"
    );
}
