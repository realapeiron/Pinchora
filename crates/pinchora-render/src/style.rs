//! What a player chooses about the look of their cards: the accent colour, how much of the
//! background shows through the panels, and the frame around the card.

/// How much of the background shows through the panels of a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Opacity {
    /// See-through panels, a light veil over the background picture.
    Light,
    /// The look cards had before this was a choice.
    #[default]
    Normal,
    /// A dark, nearly opaque veil: the background becomes a quiet texture.
    Solid,
}

impl Opacity {
    pub const ALL: [Self; 3] = [Self::Light, Self::Normal, Self::Solid];

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Normal => "Normal",
            Self::Solid => "Solid",
        }
    }

    /// The number stored in the database (`users.panel_opacity`).
    pub fn index(self) -> i16 {
        match self {
            Self::Light => 0,
            Self::Normal => 1,
            Self::Solid => 2,
        }
    }

    /// The opacity stored as `index`; anything unknown counts as Normal.
    pub fn from_index(index: i16) -> Self {
        match index {
            0 => Self::Light,
            2 => Self::Solid,
            _ => Self::Normal,
        }
    }

    /// Opacity of the dark veil over a background picture.
    pub(crate) fn scrim(self) -> f32 {
        match self {
            Self::Light => 0.1,
            Self::Normal => 0.25,
            Self::Solid => 0.55,
        }
    }

    /// Opacity of the plates behind loose text on a background picture. Light ones stay dark
    /// enough for small text to read over the brightest picture.
    pub(crate) fn plate(self) -> f32 {
        match self {
            Self::Light => 0.7,
            Self::Normal => 0.8,
            Self::Solid => 0.95,
        }
    }

    /// Opacity of a panel's fill. Even the see-through ones keep enough of it for the
    /// lifted label colour (see [`crate::Theme::with_style`]) to read over a bright picture.
    pub(crate) fn panel(self) -> f32 {
        match self {
            Self::Light => 0.74,
            Self::Normal | Self::Solid => 1.0,
        }
    }

    /// Opacity of the extra plate under the card's footer, on top of the plate everything
    /// loose sits on: the footer text is small and the crab and the badges are busy.
    pub(crate) const FOOTER_PLATE: f32 = 0.5;
}

/// A pixel frame drawn around the whole card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Frame {
    /// No frame: the look cards had before frames existed.
    #[default]
    Plain,
    Rope,
    Coral,
    Pearl,
    Gold,
    Neon,
}

impl Frame {
    pub const ALL: [Self; 6] = [
        Self::Plain,
        Self::Rope,
        Self::Coral,
        Self::Pearl,
        Self::Gold,
        Self::Neon,
    ];

    /// The text stored in the database (`users.frame`).
    pub fn id(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Rope => "rope",
            Self::Coral => "coral",
            Self::Pearl => "pearl",
            Self::Gold => "gold",
            Self::Neon => "neon",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Plain => "Plain",
            Self::Rope => "Rope",
            Self::Coral => "Coral",
            Self::Pearl => "Pearl",
            Self::Gold => "Gold",
            Self::Neon => "Neon",
        }
    }

    /// The level that unlocks the frame; 0 is free.
    pub fn unlock_level(self) -> u32 {
        match self {
            Self::Plain | Self::Rope => 0,
            Self::Coral => 10,
            Self::Pearl => 20,
            Self::Gold => 30,
            Self::Neon => 40,
        }
    }

    pub fn is_unlocked(self, level: u32) -> bool {
        level >= self.unlock_level()
    }
}

/// Accent colours to pick from, drawn from the card palette: name and `0xrrggbb`.
pub const ACCENT_PRESETS: [(&str, u32); 12] = [
    ("Coral", 0xff6b4a),
    ("Peach", 0xff9e7a),
    ("Honey", 0xffd166),
    ("Meadow", 0xa6e39a),
    ("Moss", 0x5bbf6a),
    ("Mint", 0x06d6a0),
    ("Sky", 0x8fd3ff),
    ("Ocean", 0x3a9bdc),
    ("Periwinkle", 0x8fa0d8),
    ("Violet", 0x9b6bc4),
    ("Rose", 0xff8fb1),
    ("Cream", 0xfff1e8),
];

/// Why a typed colour was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentError {
    /// Not a `#rgb` or `#rrggbb` code.
    Malformed,
    /// Too dark to read against the night-sea panels.
    TooDark,
}

/// The least contrast an accent needs against the darkest panel colour (WCAG's bar for graphics).
const MIN_CONTRAST: f32 = 3.0;

/// The card's darkest colour, which the accent is drawn on (the XP bar's well, the avatar's fill).
const FLOOR: u32 = 0x0b1029;

/// The colour in `0xrrggbb` for a typed hex code: `#ff6b4a`, `ff6b4a` or the short `#f64`.
/// Codes the card could not show are refused.
pub fn parse_accent(input: &str) -> Result<u32, AccentError> {
    let code = input.trim().trim_start_matches('#');
    let digits: Option<Vec<u32>> = code.chars().map(|c| c.to_digit(16)).collect();
    let rgb = match digits.as_deref() {
        Some([r, g, b]) => ((r * 0x11) << 16) | ((g * 0x11) << 8) | (b * 0x11),
        Some([r1, r2, g1, g2, b1, b2]) => (((r1 << 4) | r2) << 16) | (((g1 << 4) | g2) << 8) | ((b1 << 4) | b2),
        _ => return Err(AccentError::Malformed),
    };
    if readable(rgb) {
        Ok(rgb)
    } else {
        Err(AccentError::TooDark)
    }
}

/// Whether `rgb` stands out enough against the card's darkest colour.
pub fn readable(rgb: u32) -> bool {
    let (light, dark) = (luminance(rgb) + 0.05, luminance(FLOOR) + 0.05);
    light / dark >= MIN_CONTRAST
}

/// Relative luminance, `0.0` black to `1.0` white.
pub(crate) fn luminance(rgb: u32) -> f32 {
    let channel = |shift: u32| {
        let c = ((rgb >> shift) & 0xff) as f32 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

#[cfg(test)]
mod tests {
    use crate::style::*;

    #[test]
    fn frames_round_trip_and_unlock_by_level() {
        for frame in Frame::ALL {
            assert_eq!(Frame::from_id(frame.id()), Some(frame));
        }
        assert_eq!(Frame::from_id("diamond"), None);
        let levels: Vec<u32> = Frame::ALL.iter().map(|f| f.unlock_level()).collect();
        assert_eq!(levels, [0, 0, 10, 20, 30, 40]);
        assert!(Frame::Rope.is_unlocked(1) && !Frame::Coral.is_unlocked(9) && Frame::Coral.is_unlocked(10));
        assert_eq!(Frame::default(), Frame::Plain);
    }

    #[test]
    fn opacity_round_trips_and_defaults_to_normal() {
        for opacity in Opacity::ALL {
            assert_eq!(Opacity::from_index(opacity.index()), opacity);
        }
        assert_eq!(Opacity::from_index(7), Opacity::Normal);
        assert_eq!(Opacity::default(), Opacity::Normal);
        let (light, solid) = (Opacity::Light, Opacity::Solid);
        assert!(light.scrim() < Opacity::Normal.scrim() && Opacity::Normal.scrim() < solid.scrim());
        assert!(light.plate() < Opacity::Normal.plate() && Opacity::Normal.plate() < solid.plate());
    }

    #[test]
    fn typed_colours_are_checked() {
        assert_eq!(parse_accent("#ff6b4a"), Ok(0xff6b4a));
        assert_eq!(parse_accent("  FF6B4A "), Ok(0xff6b4a));
        assert_eq!(parse_accent("#f84"), Ok(0xff8844));
        for bad in ["", "#", "ff6b4", "#ff6b4a0", "#gg0000", "red", "#ff 6b4a"] {
            assert_eq!(parse_accent(bad), Err(AccentError::Malformed), "{bad:?}");
        }
        for dark in ["#000000", "#111111", "#1d2b53", "#0b1029", "#0000ff", "#222"] {
            assert_eq!(parse_accent(dark), Err(AccentError::TooDark), "{dark}");
        }
        assert_eq!(parse_accent("#ffffff"), Ok(0xffffff));
    }

    #[test]
    fn every_preset_is_readable() {
        for (name, rgb) in ACCENT_PRESETS {
            assert!(readable(rgb), "{name}");
        }
        let names: std::collections::HashSet<_> = ACCENT_PRESETS.iter().map(|(n, _)| *n).collect();
        assert_eq!(names.len(), ACCENT_PRESETS.len(), "names are unique");
    }
}
