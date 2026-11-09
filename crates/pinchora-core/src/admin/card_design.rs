//! Welcome and goodbye images a server designs on the dashboard: a canvas, a background and ordered layers
//! (avatar, text, sprite, shape), kept as versioned JSON. This is the one validator the API, the bot and
//! (through `rules.json`) the editor agree on; the drawing lives in `pinchora-render`. Pure: no Discord, no
//! database.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::admin::settings::FieldError;
use crate::admin::settings::parse_hex_color;

/// The JSON shape this code reads and writes. A design from an older version is moved forward on read.
pub const DESIGN_VERSION: u32 = 1;
pub const WIDTH_RANGE: RangeInclusive<u32> = 300..=1600;
pub const HEIGHT_RANGE: RangeInclusive<u32> = 150..=1000;
pub const MAX_LAYERS: usize = 20;
/// Where a layer's top-left corner may be, in canvas pixels: partly off the canvas is fine.
pub const POSITION_RANGE: RangeInclusive<i32> = -2000..=4000;
/// How wide and tall a layer may be.
pub const SIZE_RANGE: RangeInclusive<u32> = 4..=3200;
/// An avatar layer's side, border included, at most; the bot fetches no more than 512 px of the picture.
pub const AVATAR_MAX_SIDE: u32 = 1024;
/// How many avatar layers a design has, at most.
pub const MAX_AVATARS: usize = 4;
/// What drawing the visible layers may cost, at most, in pixel-equivalents: a shape or sticker costs its
/// width times height ([`layer_cost`]). Measured on a 1600×1000 card, 1 million of these is about 8 ms, so the
/// worst design that passes draws in well under 400 ms (`pinchora-render/tests/cost.rs` builds it).
pub const LAYER_COST_MAX: u64 = 30_000_000;
/// An avatar costs this many times its area: the picture is scaled, clipped and often faded.
pub const AVATAR_COST_FACTOR: u64 = 5;
/// A text layer costs this much per character it can show. Outlined, shadowed, faded emoji and CJK (the
/// fallback fonts' big outlines) take 0.6 ms a character, whatever the size and the room around them.
pub const TEXT_CHAR_COST: u64 = 80_000;
pub const LAYER_ID_MAX: usize = 24;
/// A text layer's text, in characters, before its placeholders are filled in.
pub const TEXT_MAX: usize = 200;
pub const TEXT_SIZE_RANGE: RangeInclusive<u32> = 8..=200;
pub const MAX_LINES: u32 = 4;
pub const BORDER_MAX: u32 = 40;
pub const OUTLINE_MAX: u32 = 12;
/// How dark the scrim over the background may be, in percent.
pub const DIM_MAX: u8 = 90;
/// How far apart two lines of a text layer are, in font sizes; a text layer's box is this tall per line.
pub const LINE_HEIGHT: f32 = 1.3;
/// What a text layer can say about the member and the server.
pub const PLACEHOLDERS: [&str; 5] = ["{user}", "{username}", "{server}", "{members}", "{ordinal}"];
/// How long a member's name and a server's name are drawn on a card, at most, in characters.
pub const NAME_MAX: usize = 32;
pub const SERVER_MAX: usize = 40;
/// The most characters each of the [`PLACEHOLDERS`] can fill in, in the same order: a name, a username, a
/// server's name, a member count (`25,000,000`) and an ordinal (`25,000,000th`).
pub const FILLED_MAX: [usize; 5] = [NAME_MAX, NAME_MAX, SERVER_MAX, 10, 12];
/// The little pixel drawings every card can use, coloured to taste.
pub const GLYPHS: [&str; 4] = ["crab", "heart", "star", "shell"];
/// Atlas sprites that can go on a card as stickers: pets, critters and fish, badges, furniture.
pub const STICKER_PREFIXES: [&str; 4] = ["p_", "c_", "b_", "f_"];
/// Atlas pictures that can be a card's background.
pub const BACKGROUND_PREFIX: &str = "bg_";
/// An uploaded background as sent, at most.
pub const UPLOAD_MAX_BYTES: usize = 4 * 1024 * 1024;
/// An uploaded picture as kept (re-encoded PNG), at most: a noisy picture is shrunk until it fits, so its
/// decoding stays a small part of drawing a card.
pub const UPLOAD_STORED_MAX_BYTES: usize = 1024 * 1024;
/// An uploaded picture's sides before it is shrunk, at most.
pub const UPLOAD_MAX_SIDE: u32 = 4096;
/// Uploads a server keeps; the oldest unused ones go first.
pub const MAX_UPLOADS: usize = 12;

/// Which image a design is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Welcome,
    Goodbye,
    Boost,
    LevelUp,
}

impl CardKind {
    pub const ALL: [Self; 4] = [Self::Welcome, Self::Goodbye, Self::Boost, Self::LevelUp];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::Goodbye => "goodbye",
            Self::Boost => "boost",
            Self::LevelUp => "level_up",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// How the image goes out when the server never chose: the welcome card with its text, as it always
    /// did; goodbyes stay text.
    pub const fn default_delivery(self) -> Delivery {
        match self {
            Self::Welcome | Self::LevelUp => Delivery::WithMessage,
            Self::Goodbye | Self::Boost => Delivery::TextOnly,
        }
    }
}

/// How the image and the text are sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// One message in the channel: the text with the image under it.
    WithMessage,
    /// The image in the channel first, then the text as its own message.
    BeforeMessage,
    /// Only the image in the channel.
    ImageOnly,
    /// Only the text: no image.
    TextOnly,
    /// The text and the image in a direct message to the new member (welcome only).
    Dm,
}

impl Delivery {
    pub const ALL: [Self; 5] = [
        Self::WithMessage,
        Self::BeforeMessage,
        Self::ImageOnly,
        Self::TextOnly,
        Self::Dm,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::WithMessage => "with_message",
            Self::BeforeMessage => "before_message",
            Self::ImageOnly => "image_only",
            Self::TextOnly => "text_only",
            Self::Dm => "dm",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.key() == key)
    }

    /// Whether an image is drawn at all.
    pub const fn has_image(self) -> bool {
        !matches!(self, Self::TextOnly)
    }

    /// Whether the text goes out.
    pub const fn has_text(self) -> bool {
        !matches!(self, Self::ImageOnly)
    }

    /// The deliveries `kind` offers, in the order the pickers list them: someone who left has no DMs open
    /// to the server any more.
    pub fn offered(kind: CardKind) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|d| kind == CardKind::Welcome || *d != Self::Dm)
            .collect()
    }
}

/// The delivery `key` names, if `kind` offers it.
pub fn check_delivery(kind: CardKind, key: &str) -> Result<Delivery, FieldError> {
    Delivery::parse(key)
        .filter(|d| Delivery::offered(kind).contains(d))
        .ok_or_else(|| {
            let message = match kind {
                CardKind::Welcome => {
                    "Send the image with the text, before it, alone, not at all, or by direct message."
                }
                CardKind::Goodbye | CardKind::Boost | CardKind::LevelUp => {
                    "Send the image with the text, before it, alone, or not at all."
                }
            };
            FieldError::new("delivery", message)
        })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardDesign {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub background: Background,
    /// A black scrim over the background, in percent, so text stays readable over a busy picture.
    #[serde(default)]
    pub dim: u8,
    /// Description Discord's screen readers announce for the attached image.
    #[serde(default)]
    pub alt_text: String,
    /// Bottom to top.
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Background {
    Transparent,
    Solid {
        color: String,
    },
    Gradient {
        from: String,
        to: String,
        #[serde(default)]
        direction: Direction,
        /// The pixel look: a faint dither and a few sparkles, like every Pinchora card.
        #[serde(default)]
        pixel: bool,
    },
    /// One of the pixel-art backgrounds from the atlas (`bg_*`), scaled up whole and cropped to the canvas.
    Sprite {
        id: String,
    },
    /// A picture the server uploaded, cropped to cover the canvas.
    Upload {
        id: String,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Vertical,
    Horizontal,
    Diagonal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// Made by the editor; unique in the design.
    pub id: String,
    #[serde(default)]
    pub hidden: bool,
    /// The editor will not move it; the card ignores this.
    #[serde(default)]
    pub locked: bool,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    /// Percent.
    #[serde(default = "opaque")]
    pub opacity: u8,
    #[serde(flatten)]
    pub content: Content,
}

fn opaque() -> u8 {
    100
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    /// The member's avatar, square: `w` is its size.
    Avatar {
        shape: AvatarShape,
        border_color: String,
        border_width: u32,
        /// Drawn as chunky pixels, like the classic card.
        #[serde(default)]
        pixelate: bool,
    },
    /// Text with placeholders, wrapped to `w` in at most `max_lines` lines; `h` follows the font size.
    Text {
        text: String,
        font: Font,
        #[serde(default)]
        bold: bool,
        size: u32,
        color: String,
        align: Align,
        #[serde(default)]
        uppercase: bool,
        max_lines: u32,
        #[serde(default)]
        shadow: bool,
        outline_color: String,
        #[serde(default)]
        outline_width: u32,
    },
    /// A sticker: an atlas sprite (pets, critters, badges, furniture) or one of the [`GLYPHS`] in `color`.
    Sprite { sprite: String, color: String },
    Shape {
        shape: ShapeKind,
        fill: String,
        border_color: String,
        #[serde(default)]
        border_width: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvatarShape {
    Circle,
    Square,
    Rounded,
    /// Pixel-art corners cut one step in, as on every Pinchora card.
    Notched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Font {
    /// Silkscreen, the pixel font.
    Pixel,
    /// Noto Sans.
    Sans,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rect,
    Rounded,
    Notched,
    Ellipse,
    /// A row of blocks, each 2.5 times as long as the shape is tall, with gaps as long: the stage edge
    /// of the classic card.
    Dashes,
}

impl Layer {
    /// What the layer is, for messages and the editor's list.
    pub fn label(&self) -> &'static str {
        match self.content {
            Content::Avatar { .. } => "Avatar",
            Content::Text { .. } => "Text",
            Content::Sprite { .. } => "Sticker",
            Content::Shape { .. } => "Shape",
        }
    }
}

/// How tall a text layer of `size` px with `lines` lines is.
pub fn text_height(size: u32, lines: u32) -> u32 {
    (size as f32 * LINE_HEIGHT * lines.max(1) as f32).round() as u32
}

/// How many characters `text` takes up once its placeholders are filled in, each counted at the most it can
/// be ([`FILLED_MAX`]): a member's name may be anything.
pub fn filled_length(text: &str) -> usize {
    let mut total = 0;
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        total += rest[..open].chars().count();
        let after = &rest[open..];
        match PLACEHOLDERS
            .iter()
            .zip(FILLED_MAX)
            .find(|(p, _)| after.starts_with(**p))
        {
            Some((placeholder, most)) => {
                total += most;
                rest = &after[placeholder.len()..];
            }
            None => {
                total += 1;
                rest = &after[1..];
            }
        }
    }
    total + rest.chars().count()
}

/// What drawing `layer` costs, in the pixel-equivalents of [`LAYER_COST_MAX`].
pub fn layer_cost(layer: &Layer) -> u64 {
    let area = u64::from(layer.w) * u64::from(layer.h);
    match &layer.content {
        Content::Avatar { .. } => area.saturating_mul(AVATAR_COST_FACTOR),
        Content::Text { text, .. } => (filled_length(text) as u64).saturating_mul(TEXT_CHAR_COST),
        Content::Sprite { .. } | Content::Shape { .. } => area,
    }
}

/// How much of a card's allowance `layers` use, in whole percent, rounded up: 100 or less is a design that
/// fits.
pub fn cost_share(layers: &[Layer]) -> u64 {
    layers_cost(layers).saturating_mul(100).div_ceil(LAYER_COST_MAX)
}

/// What drawing the visible `layers` costs together.
pub fn layers_cost(layers: &[Layer]) -> u64 {
    layers
        .iter()
        .filter(|l| !l.hidden)
        .fold(0, |sum: u64, l| sum.saturating_add(layer_cost(l)))
}

impl CardDesign {
    /// The uploaded picture the design needs, if any.
    pub fn upload(&self) -> Option<&str> {
        match &self.background {
            Background::Upload { id } => Some(id),
            _ => None,
        }
    }

    /// The side the avatar is drawn at inside its border, the largest if there are several; `None`
    /// without a visible avatar.
    pub fn avatar_size(&self) -> Option<u32> {
        self.layers
            .iter()
            .filter(|l| !l.hidden)
            .filter_map(|l| match l.content {
                Content::Avatar { border_width, .. } => Some(l.w.saturating_sub(2 * border_width).max(1)),
                _ => None,
            })
            .max()
    }

    /// Whether a visible avatar layer is drawn in chunky pixels.
    pub fn pixelated_avatar(&self) -> bool {
        self.layers
            .iter()
            .any(|l| !l.hidden && matches!(l.content, Content::Avatar { pixelate: true, .. }))
    }
}

/// What the validator needs to know about the art: whether an atlas sprite exists.
pub trait Catalog {
    fn has_sprite(&self, id: &str) -> bool;
}

impl<F: Fn(&str) -> bool> Catalog for F {
    fn has_sprite(&self, id: &str) -> bool {
        self(id)
    }
}

/// Whether `id` is what an upload's id looks like: 32 lowercase hex digits.
pub fn is_upload_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests;

mod validation;
pub use validation::{check_design, parse_design};

#[cfg(test)]
use validation::unknown_placeholder;
