//! Colours a player can pick for each part of their character, and the shading ramps made from
//! them. A ramp keeps the art's light and shade: darker steps lean toward violet, lighter ones
//! toward gold, and every step stays far enough from its neighbours, and from the outline, to show.

use serde::{Deserialize, Serialize};

/// A colour as picked: one of a channel's swatches, or a player's own `0xrrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paint {
    Swatch(u8),
    Custom(u32),
}

impl Default for Paint {
    fn default() -> Self {
        Self::Swatch(0)
    }
}

impl Paint {
    /// `"3"` for a swatch, `"#a1b2c3"` for a custom colour: how saved codes write it.
    pub fn code(self) -> String {
        match self {
            Self::Swatch(i) => i.to_string(),
            Self::Custom(rgb) => format!("#{rgb:06x}"),
        }
    }

    /// Reads [`Paint::code`]; a swatch must be below `swatches`.
    pub fn parse(text: &str, swatches: usize) -> Option<Self> {
        if let Some(hex) = text.strip_prefix('#') {
            return (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| u32::from_str_radix(hex, 16).ok().map(Self::Custom))
                .flatten();
        }
        if text.is_empty() || text.len() > 3 || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        text.parse::<usize>()
            .ok()
            .filter(|&i| i < swatches)
            .map(|i| Self::Swatch(i as u8))
    }
}

/// One colour on a channel's menu. `ramp` is a hand-tuned dark → light ramp the first colours
/// keep from before ramps were derived; the rest are derived from `rgb`. `rgb` 0 with no ramp
/// means "as drawn".
#[derive(Debug, Clone, Copy)]
pub struct Swatch {
    pub name: &'static str,
    pub rgb: u32,
    pub ramp: Option<&'static [u32]>,
}

const fn hand(name: &'static str, ramp: &'static [u32]) -> Swatch {
    Swatch {
        name,
        rgb: ramp[2],
        ramp: Some(ramp),
    }
}

const fn tone(name: &'static str, rgb: u32) -> Swatch {
    Swatch { name, rgb, ramp: None }
}

/// The art's own colours, untouched.
const NATURAL: Swatch = Swatch {
    name: "Natural",
    rgb: 0,
    ramp: None,
};

impl Swatch {
    /// Whether this is the "as drawn" choice.
    pub fn is_natural(&self) -> bool {
        self.rgb == 0 && self.ramp.is_none()
    }
}

pub const SKIN_TONES: [Swatch; 20] = [
    hand("Porcelain", &[0x9a6a5a, 0xd9a08a, 0xf6cdb6, 0xfff0e6]),
    hand("Peach", &[0x8a4b3b, 0xc97a5c, 0xf0a98a, 0xffd8c2]),
    hand("Sand", &[0x7d5238, 0xb9805a, 0xe0ab7f, 0xf5d2ad]),
    hand("Honey", &[0x6e4426, 0xa8693c, 0xd09359, 0xebbd86]),
    hand("Caramel", &[0x5a3520, 0x8c5530, 0xb7774a, 0xd9a170]),
    hand("Cocoa", &[0x3f2418, 0x6b3e26, 0x93603c, 0xb98457]),
    hand("Espresso", &[0x2a1812, 0x4a2c1e, 0x6e4530, 0x8f6146]),
    hand("Seafoam", &[0x2f6b5e, 0x4f9f8a, 0x7fcdb5, 0xbdf0e0]),
    tone("Ivory", 0xf7dcc8),
    tone("Rosy", 0xf2b3a2),
    tone("Olive", 0xc9a079),
    tone("Golden", 0xe2ae70),
    tone("Bronze", 0xa8714a),
    tone("Mahogany", 0x7e4a32),
    tone("Ebony", 0x5c3a2a),
    tone("Lilac", 0xc6a8ea),
    tone("Sky", 0x9ccff2),
    tone("Mint", 0xa6e6bf),
    tone("Coral", 0xff9f8a),
    tone("Stone", 0xb7b9c9),
];

pub const HAIR_COLORS: [Swatch; 20] = [
    hand("Ink", &[0x14141c, 0x26283a, 0x3b3f58, 0x5a6080]),
    hand("Chestnut", &[0x2e1a12, 0x4f2e1e, 0x744630, 0x9a6448]),
    hand("Hazel", &[0x4a2e18, 0x75502a, 0xa0743c, 0xc99a5a]),
    hand("Auburn", &[0x4a1a14, 0x7a2e1e, 0xa8472c, 0xd06a44]),
    hand("Ginger", &[0x7a3a10, 0xb85a1a, 0xe8822e, 0xffb066]),
    hand("Honey blonde", &[0x8a6420, 0xc0922e, 0xe8c25a, 0xfde59a]),
    hand("Platinum", &[0x8f8a80, 0xc2bdb0, 0xe8e4d8, 0xfffcf2]),
    hand("Silver", &[0x45495c, 0x6a6f86, 0xa3a8bd, 0xdfe3f0]),
    hand("Bubblegum", &[0x7a2a4f, 0xb8436f, 0xe86a92, 0xffb3cb]),
    hand("Ocean", &[0x1f3f6b, 0x2c6fb8, 0x4a9bdc, 0x9fd8ff]),
    hand("Kelp", &[0x1f4d3a, 0x2f7d4f, 0x4f9f5a, 0x8fd88a]),
    hand("Violet", &[0x3b1f6b, 0x5e35a8, 0x8a5ad8, 0xb994f0]),
    tone("Cherry", 0xc8344e),
    tone("Strawberry", 0xff7d86),
    tone("Peach", 0xffae86),
    tone("Lemon", 0xffdd5c),
    tone("Mint", 0x7ee0b6),
    tone("Teal", 0x2fb5ad),
    tone("Lavender", 0xbfa4ff),
    tone("Snow", 0xeef0f8),
];

/// Tops and bottoms: the first twelve are the furniture palette variants, ramps and all.
pub const OUTFIT_COLORS: [Swatch; 20] = [
    hand("Coral", &[0x7a2a3a, 0xc2413f, 0xff6b4a, 0xff9e7a, 0xffd2c0]),
    hand("Mint", &[0x0b5e4c, 0x079b74, 0x06d6a0, 0x6ff0c8, 0xc9fff0]),
    hand("Navy", &[0x0f1631, 0x1d2b53, 0x2e3d75, 0x4a5ba3, 0x8fa0d8]),
    hand("Cream", &[0x7d6a5f, 0xb8a08f, 0xe6d3c3, 0xf4e6da, 0xfff1e8]),
    hand("Plum", &[0x3a2350, 0x5a3a7a, 0x7d55a3, 0x9b6bc4, 0xcfb2ea]),
    hand("Gold", &[0x6b4410, 0xa06a14, 0xc98a1e, 0xffd166, 0xfff0b8]),
    hand("Sky", &[0x1f5f8b, 0x2c7fb8, 0x3a9bdc, 0x8fd3ff, 0xd6f1ff]),
    hand("Moss", &[0x1f4d3a, 0x2f7d4f, 0x4f9f5a, 0x5bbf6a, 0xa6e39a]),
    hand("Rose", &[0x7a2a4f, 0xb8436f, 0xe86a92, 0xff8fb1, 0xffd0df]),
    hand("Charcoal", &[0x1b1d2b, 0x2c2f3f, 0x45495c, 0x6a6f86, 0xa3a8bd]),
    hand("Sand", &[0x7d6a4a, 0xa88d5f, 0xd2b77f, 0xe8d3a3, 0xf8edd0]),
    hand("Lilac", &[0x4b3d6b, 0x6f5c99, 0x9a86c7, 0xc2b3e6, 0xe8e0fb]),
    tone("Cherry", 0xe23d52),
    tone("Tangerine", 0xff8a30),
    tone("Lemon", 0xffd84a),
    tone("Teal", 0x1aa9a1),
    tone("Denim", 0x4a78c0),
    tone("Forest", 0x2f7a46),
    tone("Cocoa", 0x8a5a3c),
    tone("Snow", 0xf1f2f7),
];

pub const EYE_COLORS: [Swatch; 20] = [
    NATURAL,
    tone("Brown", 0x8a5634),
    tone("Hazel", 0xa8822e),
    tone("Amber", 0xe0a02c),
    tone("Green", 0x3f9e4e),
    tone("Emerald", 0x18b87a),
    tone("Teal", 0x1aa8b8),
    tone("Blue", 0x3a7ad8),
    tone("Sky", 0x62b4ff),
    tone("Navy", 0x34449c),
    tone("Grey", 0x8a90a8),
    tone("Silver", 0xc4c9d8),
    tone("Violet", 0x8a5ad8),
    tone("Lilac", 0xb994f0),
    tone("Pink", 0xff6aa8),
    tone("Red", 0xe03a3a),
    tone("Coral", 0xff7a5a),
    tone("Gold", 0xffc83a),
    tone("Mint", 0x6fe0b0),
    tone("Ink", 0x3a3c52),
];

pub const SHOE_COLORS: [Swatch; 20] = [
    NATURAL,
    tone("Tan", 0xc49464),
    tone("Cocoa", 0x7a5442),
    tone("Black", 0x454656),
    tone("White", 0xf0f0f4),
    tone("Cream", 0xeee0cc),
    tone("Red", 0xd83a3a),
    tone("Coral", 0xff6b4a),
    tone("Orange", 0xff8c2e),
    tone("Gold", 0xe8b830),
    tone("Pink", 0xff8fb1),
    tone("Mint", 0x4ad8a0),
    tone("Moss", 0x4f9f5a),
    tone("Teal", 0x1aa8a0),
    tone("Sky", 0x5ab0f0),
    tone("Navy", 0x3a4a85),
    tone("Plum", 0x7d55a3),
    tone("Lilac", 0xb0a0e0),
    tone("Sand", 0xd2b77f),
    tone("Grey", 0x8a8fa6),
];

/// Colours an accessory can be dyed; `None` on a worn accessory means its own colours.
pub const ACCESSORY_TINTS: [Swatch; 16] = [
    tone("Coral", 0xff6b4a),
    tone("Cherry", 0xe23d52),
    tone("Tangerine", 0xff8a30),
    tone("Gold", 0xf2c040),
    tone("Lemon", 0xffe066),
    tone("Mint", 0x34d8a0),
    tone("Moss", 0x4f9f5a),
    tone("Teal", 0x1aa9a1),
    tone("Sky", 0x4aa8ec),
    tone("Navy", 0x34458a),
    tone("Plum", 0x7d55a3),
    tone("Lilac", 0xb39ae6),
    tone("Rose", 0xff7fa8),
    tone("Cream", 0xefe2d0),
    tone("Charcoal", 0x4a4e62),
    tone("Snow", 0xf4f5fa),
];

pub fn rgb(v: u32) -> [u8; 3] {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

pub fn hex(c: [u8; 3]) -> u32 {
    (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2])
}

// /OKLab

fn to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// `(L, C, h°)` of an sRGB colour.
pub(crate) fn oklch(c: [u8; 3]) -> (f32, f32, f32) {
    let [r, g, b] = c.map(to_linear);
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    let lightness = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
    let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
    let bb = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
    (lightness, a.hypot(bb), bb.atan2(a).to_degrees().rem_euclid(360.0))
}

/// Linear RGB of an OKLCH colour; may fall outside 0..1.
fn linear_rgb(lightness: f32, chroma: f32, hue: f32) -> [f32; 3] {
    let (a, b) = (chroma * hue.to_radians().cos(), chroma * hue.to_radians().sin());
    let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

/// The sRGB colour nearest to an OKLCH one: chroma is given up until it fits.
pub(crate) fn from_oklch(lightness: f32, chroma: f32, hue: f32) -> [u8; 3] {
    let lightness = lightness.clamp(0.0, 1.0);
    let mut chroma = chroma.max(0.0);
    let mut lin = linear_rgb(lightness, chroma, hue);
    for _ in 0..24 {
        if lin.iter().all(|c| (-0.000_5..=1.000_5).contains(c)) {
            break;
        }
        chroma *= 0.88;
        lin = linear_rgb(lightness, chroma, hue);
    }
    lin.map(|c| (from_linear(c.clamp(0.0, 1.0)) * 255.0).round() as u8)
}

/// Turns `hue` by up to `degrees` toward `target` along the shorter way round.
fn lean(hue: f32, target: f32, degrees: f32) -> f32 {
    let diff = (target - hue + 540.0).rem_euclid(360.0) - 180.0;
    (hue + diff.clamp(-degrees, degrees)).rem_euclid(360.0)
}

/// Darkest a shade may be: a clear step above the outline (`#1b1d2b`, L 0.24).
const FLOOR: f32 = 0.31;
/// Lightest a shade may be.
const CEILING: f32 = 0.975;
const STEP_DOWN: f32 = 0.13;
const STEP_UP: f32 = 0.105;
/// Smallest lightness step between two neighbours that still reads as shading.
const MIN_STEP: f32 = 0.045;

/// A dark → light ramp of `len` shades with `base` at index `at`. A colour too dark or too
/// light to shade around is moved until every step shows.
pub fn derive_ramp(base: [u8; 3], len: usize, at: usize) -> Vec<[u8; 3]> {
    let (mut l, c, h) = oklch(base);
    let below = at as f32;
    let above = (len - 1 - at) as f32;
    let mut up = STEP_UP;
    if l - below * STEP_DOWN < FLOOR {
        l = FLOOR + below * STEP_DOWN;
    }
    if above > 0.0 && l + above * up > CEILING {
        up = ((CEILING - l) / above).max(MIN_STEP);
        if l + above * up > CEILING {
            l = CEILING - above * up;
        }
    }
    // Greys keep their hue: the lean fades in with chroma.
    let warmth = (c / 0.05).min(1.0);
    (0..len)
        .map(|i| {
            let k = i as f32 - below;
            if k < 0.0 {
                let steps = -k;
                let chroma = c * [1.0, 0.94, 0.82, 0.7][(steps as usize).min(3)];
                let hue = lean(h, 290.0, 7.0 * steps * warmth);
                from_oklch(l - steps * STEP_DOWN, chroma, hue)
            } else if k > 0.0 {
                let chroma = c * [1.0, 0.78, 0.5, 0.32][(k as usize).min(3)];
                let hue = lean(h, 85.0, 5.0 * k * warmth);
                from_oklch(l + k * up, chroma, hue)
            } else {
                from_oklch(l, c, h)
            }
        })
        .collect()
}

/// The ramp a swatch or custom colour gives a key ramp of `len` shades whose middle is `at`;
/// `None` for "as drawn".
pub fn ramp_of(swatches: &[Swatch], paint: Paint, len: usize, at: usize) -> Option<Vec<[u8; 3]>> {
    match paint {
        Paint::Custom(v) => Some(derive_ramp(rgb(v), len, at)),
        Paint::Swatch(i) => {
            let swatch = swatches.get(usize::from(i)).unwrap_or(&swatches[0]);
            if swatch.is_natural() {
                return None;
            }
            match swatch.ramp {
                Some(ramp) if ramp.len() >= len => Some(ramp[..len].iter().map(|&v| rgb(v)).collect()),
                _ => Some(derive_ramp(rgb(swatch.rgb), len, at)),
            }
        }
    }
}

/// What a typed colour turns into once its shading must show: the middle of its ramp. Equal to
/// the colour unless it was too dark or too light.
pub fn shadeable(v: u32) -> u32 {
    hex(derive_ramp(rgb(v), 5, 2)[2])
}

/// Parses `#rrggbb`, `rrggbb` or `#rgb` (case and spaces forgiven).
pub fn parse_hex(text: &str) -> Option<u32> {
    let t = text.trim().trim_start_matches('#').to_ascii_lowercase();
    if !t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match t.len() {
        6 => u32::from_str_radix(&t, 16).ok(),
        3 => {
            let v = u32::from_str_radix(&t, 16).ok()?;
            let (r, g, b) = ((v >> 8) & 0xf, (v >> 4) & 0xf, v & 0xf);
            Some((r * 17) << 16 | (g * 17) << 8 | (b * 17))
        }
        _ => None,
    }
}

/// The colour a swatch or custom paint shows as, for menus and cards; `None` for "as drawn".
pub fn display_rgb(swatches: &[Swatch], paint: Paint) -> Option<u32> {
    match paint {
        Paint::Custom(v) => Some(shadeable(v)),
        Paint::Swatch(i) => {
            let s = swatches.get(usize::from(i))?;
            (!s.is_natural()).then_some(s.rgb)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklch_round_trips() {
        for v in [0xff6b4a, 0x1b1d2b, 0xffffff, 0x000000, 0x3a9bdc, 0x8a4b3b, 0x7fcdb5] {
            let (l, c, h) = oklch(rgb(v));
            let back = from_oklch(l, c, h);
            for (a, b) in back.iter().zip(rgb(v)) {
                assert!(
                    (i16::from(*a) - i16::from(b)).abs() <= 1,
                    "{v:06x} came back as {back:?}"
                );
            }
        }
    }

    #[test]
    fn derived_ramps_shade_every_colour() {
        let outline = oklch(rgb(0x1b1d2b)).0;
        for v in [
            0x000000, 0x101010, 0xffffff, 0xfffff0, 0x808080, 0xff0000, 0x00ff00, 0x0000ff, 0xff6b4a,
        ] {
            for (len, at) in [(4, 2), (5, 2), (3, 1)] {
                let ramp = derive_ramp(rgb(v), len, at);
                assert_eq!(ramp.len(), len);
                let ls: Vec<f32> = ramp.iter().map(|&c| oklch(c).0).collect();
                assert!(
                    ls[0] - outline >= 0.05,
                    "{v:06x}: darkest {} is lost in the outline",
                    ls[0]
                );
                for pair in ls.windows(2) {
                    assert!(pair[1] - pair[0] >= 0.03, "{v:06x} {len}/{at}: {ls:?}");
                }
            }
        }
        // A colour in the middle of the range is kept as it was typed.
        assert_eq!(shadeable(0xff6b4a), 0xff6b4a);
        assert_ne!(shadeable(0x050505), 0x050505, "near-black is lifted");
    }

    #[test]
    fn derived_ramps_resemble_the_hand_tuned_ones() {
        for swatch in OUTFIT_COLORS.iter().chain(&SKIN_TONES).chain(&HAIR_COLORS) {
            let Some(hand) = swatch.ramp else { continue };
            // Very dark hand ramps go below the floor a derived ramp keeps above the outline.
            if oklch(rgb(hand[0])).0 < FLOOR {
                continue;
            }
            let derived = derive_ramp(rgb(hand[2]), hand.len(), 2);
            for (i, (&h, d)) in hand.iter().zip(&derived).enumerate() {
                let (lh, ld) = (oklch(rgb(h)).0, oklch(*d).0);
                assert!((lh - ld).abs() < 0.12, "{} shade {i}: {lh} vs {ld}", swatch.name);
            }
        }
    }

    #[test]
    fn paints_parse_their_own_codes() {
        for paint in [
            Paint::Swatch(0),
            Paint::Swatch(19),
            Paint::Custom(0xa1b2c3),
            Paint::Custom(0),
        ] {
            assert_eq!(Paint::parse(&paint.code(), 20), Some(paint));
        }
        for bad in ["", "20", "-1", "+1", "#12345", "#1234567", "#ggg000", "a", "0x1"] {
            assert_eq!(Paint::parse(bad, 20), None, "{bad:?}");
        }
        assert_eq!(parse_hex(" #FF6B4A "), Some(0xff6b4a));
        assert_eq!(parse_hex("f64"), Some(0xff6644));
        assert_eq!(parse_hex("#12"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
    }

    #[test]
    fn every_list_has_twenty_named_colours() {
        for list in [&SKIN_TONES, &HAIR_COLORS, &OUTFIT_COLORS, &EYE_COLORS, &SHOE_COLORS] {
            assert_eq!(list.len(), 20);
            let mut names: Vec<_> = list.iter().map(|s| s.name).collect();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), 20, "names repeat");
            assert!(names.iter().all(|n| n.chars().count() <= 20));
        }
    }
}
