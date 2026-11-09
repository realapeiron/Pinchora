//! The colours a server's members can pick from: the card palette (`ACCENT_PRESETS`) and the outfit
//! dyes of the pixel characters, the ones that stay readable on Discord's dark chat.

use std::sync::LazyLock;

use pinchora_render::{ACCENT_PRESETS, character::OUTFIT_COLORS, readable};

/// The palette's colours at most: a menu holds 25 options, and the palette leaves the colour roles' own
/// 32 some room for colours somebody typed.
const MAX_PALETTE: usize = 24;

/// One colour of the palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteColor {
    pub name: String,
    /// `#rrggbb`, lower case.
    pub hex: String,
    pub rgb: u32,
}

/// `#rrggbb` for `0xrrggbb`.
pub fn hex_of(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0xff_ffff)
}

/// `0xrrggbb` for `#rrggbb` (as stored); `None` for anything else.
pub fn rgb_of(hex: &str) -> Option<u32> {
    let digits = hex.strip_prefix('#')?;
    (digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
}

static PALETTE: LazyLock<Vec<PaletteColor>> = LazyLock::new(|| {
    let mut found: Vec<PaletteColor> = Vec::new();
    let named = ACCENT_PRESETS
        .iter()
        .map(|(name, rgb)| ((*name).to_owned(), *rgb))
        .chain(OUTFIT_COLORS.iter().map(|s| (s.name.to_owned(), s.rgb)));
    for (name, rgb) in named {
        if rgb == 0 || !readable(rgb) || found.iter().any(|c| c.rgb == rgb || c.name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        found.push(PaletteColor {
            name,
            hex: hex_of(rgb),
            rgb,
        });
        if found.len() == MAX_PALETTE {
            break;
        }
    }
    found
});

/// The palette, in the order members see it.
pub fn palette() -> &'static [PaletteColor] {
    &PALETTE
}

/// The palette colour with this `#rrggbb`, if it is one.
pub fn palette_colour(hex: &str) -> Option<&'static PaletteColor> {
    palette().iter().find(|c| c.hex == hex)
}

#[cfg(test)]
mod tests {
    use crate::commands::roles::automation::palette::*;

    #[test]
    fn the_palette_is_the_card_colours_without_repeats() {
        let colours = palette();
        assert!(
            colours.len() >= 16 && colours.len() <= MAX_PALETTE,
            "{} colours",
            colours.len()
        );
        assert_eq!(colours[0].name, "Coral", "the card palette comes first");
        for (i, c) in colours.iter().enumerate() {
            assert!(readable(c.rgb), "{} is too dark for the chat", c.name);
            assert!(
                !colours[i + 1..]
                    .iter()
                    .any(|o| o.rgb == c.rgb || o.name.eq_ignore_ascii_case(&c.name)),
                "{} is there twice",
                c.name
            );
        }
        assert_eq!(palette_colour("#ff6b4a").map(|c| c.name.as_str()), Some("Coral"));
        assert!(palette_colour("#000000").is_none());
    }

    #[test]
    fn hex_codes_round_trip() {
        assert_eq!(hex_of(0xff6b4a), "#ff6b4a");
        assert_eq!(hex_of(0x00_00ab), "#0000ab");
        assert_eq!(rgb_of("#ff6b4a"), Some(0xff6b4a));
        for bad in ["ff6b4a", "#ff6b4", "#ff6b4g", "#ff6b4a0", ""] {
            assert_eq!(rgb_of(bad), None, "{bad:?}");
        }
    }
}
