//! Welcome and goodbye images a server designs on the dashboard: a canvas, a background and ordered layers
//! (avatar, text, sprite, shape), kept as versioned JSON. This is the one validator the API, the bot and
//! (through `rules.json`) the editor agree on; the drawing lives in `pinchora-render`. Pure: no Discord, no
//! database.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::settings::{FieldError, parse_hex_color};

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
}

impl CardKind {
    pub const ALL: [Self; 2] = [Self::Welcome, Self::Goodbye];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Welcome => "welcome",
            Self::Goodbye => "goodbye",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// How the image goes out when the server never chose: the welcome card with its text, as it always
    /// did; goodbyes stay text.
    pub const fn default_delivery(self) -> Delivery {
        match self {
            Self::Welcome => Delivery::WithMessage,
            Self::Goodbye => Delivery::TextOnly,
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
                CardKind::Goodbye => "Send the image with the text, before it, alone, or not at all.",
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

/// Reads a design sent as JSON: the shape first (a setting it does not know is refused by name), then
/// every rule in [`check_design`].
pub fn parse_design(value: &Value, catalog: &dyn Catalog) -> Result<CardDesign, Vec<FieldError>> {
    let design: CardDesign = serde_json::from_value(value.clone()).map_err(|error| {
        vec![FieldError::new(
            "design",
            format!("The design could not be read: {error}."),
        )]
    })?;
    if let Ok(written) = serde_json::to_value(&design)
        && let Some(path) = unknown_key(value, &written, "")
    {
        return Err(vec![FieldError::new(
            "design",
            format!("The design has a setting this version does not know: `{path}`."),
        )]);
    }
    check_design(design, catalog)
}

/// The first key in `sent` that `read` (the same value read and written back) does not have.
fn unknown_key(sent: &Value, read: &Value, at: &str) -> Option<String> {
    match (sent, read) {
        (Value::Object(sent), Value::Object(read)) => sent.iter().find_map(|(key, value)| {
            let path = if at.is_empty() {
                key.clone()
            } else {
                format!("{at}.{key}")
            };
            match read.get(key) {
                None => Some(path),
                Some(inner) => unknown_key(value, inner, &path),
            }
        }),
        (Value::Array(sent), Value::Array(read)) => sent
            .iter()
            .zip(read)
            .enumerate()
            .find_map(|(i, (s, r))| unknown_key(s, r, &format!("{at}[{i}]"))),
        _ => None,
    }
}

/// Checks a design against every rule and returns it tidied (colours lowercase, text boxes as tall as
/// their lines), or every problem found.
pub fn check_design(mut design: CardDesign, catalog: &dyn Catalog) -> Result<CardDesign, Vec<FieldError>> {
    let mut errors = Vec::new();
    let mut fail = |field: &'static str, message: String| errors.push(FieldError::new(field, message));
    if design.version != DESIGN_VERSION {
        fail(
            "design",
            format!(
                "This design is version {}; only version {DESIGN_VERSION} can be saved.",
                design.version
            ),
        );
    }
    if !WIDTH_RANGE.contains(&design.width) {
        fail(
            "width",
            format!(
                "The width goes from {} to {} px.",
                WIDTH_RANGE.start(),
                WIDTH_RANGE.end()
            ),
        );
    }
    if !HEIGHT_RANGE.contains(&design.height) {
        fail(
            "height",
            format!(
                "The height goes from {} to {} px.",
                HEIGHT_RANGE.start(),
                HEIGHT_RANGE.end()
            ),
        );
    }
    if design.dim > DIM_MAX {
        fail("dim", format!("The background can be darkened by at most {DIM_MAX}%."));
    }
    match &mut design.background {
        Background::Transparent => {}
        Background::Solid { color } => {
            if let Err(message) = tidy_color(color) {
                fail("background", format!("Background colour: {message}"));
            }
        }
        Background::Gradient { from, to, .. } => {
            for color in [from, to] {
                if let Err(message) = tidy_color(color) {
                    fail("background", format!("Background gradient: {message}"));
                }
            }
        }
        Background::Sprite { id } => {
            if !id.starts_with(BACKGROUND_PREFIX) || !catalog.has_sprite(id) {
                fail("background", "That background picture doesn't exist.".to_owned());
            }
        }
        Background::Upload { id } => {
            if !is_upload_id(id) {
                fail("background", "That uploaded picture doesn't exist.".to_owned());
            }
        }
    }
    if design.layers.len() > MAX_LAYERS {
        fail("layers", format!("A design has at most {MAX_LAYERS} layers."));
    }
    let (canvas_w, canvas_h) = (design.width, design.height);
    let mut seen = std::collections::HashSet::new();
    for (i, layer) in design.layers.iter_mut().enumerate() {
        let name = format!("Layer {} ({})", i + 1, layer.label());
        let mut bad = |message: String| fail("layers", format!("{name}: {message}"));
        let id_ok = !layer.id.is_empty()
            && layer.id.len() <= LAYER_ID_MAX
            && layer
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !id_ok {
            bad(format!(
                "the id is 1 to {LAYER_ID_MAX} letters, digits, dashes or underscores."
            ));
        } else if !seen.insert(layer.id.clone()) {
            bad("another layer has the same id.".to_owned());
        }
        if !POSITION_RANGE.contains(&layer.x) || !POSITION_RANGE.contains(&layer.y) {
            bad(format!(
                "it sits too far off the canvas ({} to {} px).",
                POSITION_RANGE.start(),
                POSITION_RANGE.end()
            ));
        }
        if !SIZE_RANGE.contains(&layer.w) || !SIZE_RANGE.contains(&layer.h) {
            bad(format!(
                "its width and height go from {} to {} px.",
                SIZE_RANGE.start(),
                SIZE_RANGE.end()
            ));
        }
        if layer.opacity > 100 {
            bad("the opacity goes from 0 to 100%.".to_owned());
        }
        match &mut layer.content {
            Content::Avatar {
                border_color,
                border_width,
                ..
            } => {
                if layer.w != layer.h {
                    bad("an avatar is square: its width and height are the same.".to_owned());
                }
                if layer.w > AVATAR_MAX_SIDE {
                    bad(format!("an avatar is at most {AVATAR_MAX_SIDE} px across."));
                }
                if *border_width > BORDER_MAX || *border_width * 4 > layer.w {
                    bad(format!(
                        "the border is at most {BORDER_MAX} px and a quarter of the avatar's size."
                    ));
                }
                if let Err(message) = tidy_color(border_color) {
                    bad(format!("border colour: {message}"));
                }
            }
            Content::Text {
                text,
                size,
                color,
                max_lines,
                outline_color,
                outline_width,
                ..
            } => {
                let length = text.chars().count();
                if text.trim().is_empty() {
                    bad("write something, or delete the layer.".to_owned());
                } else if length > TEXT_MAX {
                    bad(format!(
                        "a text is at most {TEXT_MAX} characters; this one has {length}."
                    ));
                } else if let Some(unknown) = unknown_placeholder(text) {
                    bad(format!(
                        "{unknown} isn't a placeholder. Use {}.",
                        PLACEHOLDERS.join(" ")
                    ));
                }
                if !TEXT_SIZE_RANGE.contains(size) {
                    bad(format!(
                        "the font size goes from {} to {} px.",
                        TEXT_SIZE_RANGE.start(),
                        TEXT_SIZE_RANGE.end()
                    ));
                }
                if !(1..=MAX_LINES).contains(max_lines) {
                    bad(format!("a text has 1 to {MAX_LINES} lines."));
                }
                if *outline_width > OUTLINE_MAX {
                    bad(format!("the outline is at most {OUTLINE_MAX} px."));
                }
                for c in [color, outline_color] {
                    if let Err(message) = tidy_color(c) {
                        bad(format!("colour: {message}"));
                    }
                }
                layer.h = text_height(*size, *max_lines).clamp(*SIZE_RANGE.start(), *SIZE_RANGE.end());
            }
            Content::Sprite { sprite, color } => {
                let known = GLYPHS.contains(&sprite.as_str())
                    || (STICKER_PREFIXES.iter().any(|p| sprite.starts_with(p)) && catalog.has_sprite(sprite));
                if !known {
                    bad("that sticker doesn't exist.".to_owned());
                }
                if let Err(message) = tidy_color(color) {
                    bad(format!("colour: {message}"));
                }
            }
            Content::Shape {
                fill,
                border_color,
                border_width,
                ..
            } => {
                if *border_width > BORDER_MAX {
                    bad(format!("the border is at most {BORDER_MAX} px."));
                }
                for c in [fill, border_color] {
                    if let Err(message) = tidy_color(c) {
                        bad(format!("colour: {message}"));
                    }
                }
            }
        }
        // After the match: a text box has just been made as tall as its lines.
        if layer.w > canvas_w || layer.h > canvas_h {
            bad(format!("it is bigger than the canvas ({canvas_w}×{canvas_h} px)."));
        }
    }
    let avatars = design
        .layers
        .iter()
        .filter(|l| matches!(l.content, Content::Avatar { .. }))
        .count();
    if avatars > MAX_AVATARS {
        fail("layers", format!("A design has at most {MAX_AVATARS} avatars."));
    }
    if layers_cost(&design.layers) > LAYER_COST_MAX {
        fail(
            "layers",
            format!(
                "The visible layers are too heavy to draw quickly together ({}% of what a card may use). \
                 Shrink, remove or hide some; avatars and long texts weigh the most.",
                cost_share(&design.layers)
            ),
        );
    }
    if errors.is_empty() { Ok(design) } else { Err(errors) }
}

/// `#RRGGBB` as `#rrggbb`, or why it is not a colour.
fn tidy_color(color: &mut String) -> Result<(), String> {
    match parse_hex_color(color) {
        Some(rgb) => {
            *color = format!("#{rgb:06x}");
            Ok(())
        }
        None => Err("colours look like `#ff6b4a`.".to_owned()),
    }
}

/// The first `{word}` in `text` that is not one of the [`PLACEHOLDERS`].
fn unknown_placeholder(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after = &rest[start..];
        let end = after.find('}')?;
        let token = &after[..=end];
        let word = &token[1..token.len() - 1];
        if !word.is_empty()
            && word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && !PLACEHOLDERS.contains(&token)
        {
            return Some(token.to_owned());
        }
        rest = &after[1..];
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn no_art(_: &str) -> bool {
        false
    }

    fn some_art(id: &str) -> bool {
        matches!(id, "bg_starlit_jetty" | "p_drowsepatch_cat")
    }

    fn design(layers: Value) -> Value {
        json!({
            "version": 1,
            "width": 1100,
            "height": 500,
            "background": { "type": "gradient", "from": "#1D2B53", "to": "#0f1631", "pixel": true },
            "layers": layers,
        })
    }

    fn text_layer(text: &str) -> Value {
        json!({
            "id": "title", "type": "text", "x": 50, "y": 252, "w": 1000, "h": 10,
            "text": text, "font": "pixel", "size": 64, "color": "#FFF1E8", "align": "center",
            "max_lines": 1, "outline_color": "#000000",
        })
    }

    fn messages(errors: &[FieldError]) -> Vec<&str> {
        errors.iter().map(|e| e.message.as_str()).collect()
    }

    #[test]
    fn a_good_design_is_read_and_tidied() {
        let avatar = json!({
            "id": "avatar", "type": "avatar", "x": 450, "y": 36, "w": 200, "h": 200,
            "shape": "notched", "border_color": "#ff6b4a", "border_width": 12, "pixelate": true,
        });
        let sticker = json!({
            "id": "cat", "type": "sprite", "x": 0, "y": 0, "w": 64, "h": 64,
            "sprite": "p_drowsepatch_cat", "color": "#ffffff", "opacity": 80,
        });
        let parsed = parse_design(
            &design(json!([avatar, text_layer("WELCOME {user}"), sticker])),
            &some_art,
        )
        .unwrap();
        assert_eq!(parsed.layers.len(), 3);
        assert!(matches!(&parsed.background, Background::Gradient { from, .. } if from == "#1d2b53"));
        let title = &parsed.layers[1];
        assert_eq!(title.h, text_height(64, 1), "a text box is as tall as its lines");
        assert!(matches!(&title.content, Content::Text { color, .. } if color == "#fff1e8"));
        assert_eq!((parsed.layers[0].opacity, parsed.layers[2].opacity), (100, 80));
        assert_eq!(parsed.avatar_size(), Some(176));
        assert!(parsed.pixelated_avatar());
        // Read, written and read again, it stays the same.
        let again = parse_design(&serde_json::to_value(&parsed).unwrap(), &some_art).unwrap();
        assert_eq!(again, parsed);
    }

    #[test]
    fn unknown_settings_and_shapes_are_refused_by_name() {
        let mut layer = text_layer("Hi");
        layer["glow"] = json!(true);
        let errors = parse_design(&design(json!([layer])), &no_art).unwrap_err();
        assert!(errors[0].message.contains("`layers[0].glow`"), "{:?}", errors);
        let mut sent = design(json!([]));
        sent["background"]["shine"] = json!(1);
        assert!(
            parse_design(&sent, &no_art).unwrap_err()[0]
                .message
                .contains("`background.shine`")
        );
        let errors = parse_design(&json!({ "version": 1 }), &no_art).unwrap_err();
        assert!(errors[0].message.starts_with("The design could not be read"));
        let mut odd = text_layer("Hi");
        odd["type"] = json!("video");
        assert!(parse_design(&design(json!([odd])), &no_art).is_err());
    }

    #[test]
    fn every_problem_is_listed_with_its_layer() {
        let mut big = text_layer("Hi {nickname}");
        big["size"] = json!(500);
        big["color"] = json!("red");
        let mut twin = text_layer("Hello");
        twin["max_lines"] = json!(9);
        let lopsided = json!({
            "id": "face", "type": "avatar", "x": 0, "y": 0, "w": 100, "h": 90,
            "shape": "circle", "border_color": "#000000", "border_width": 30,
        });
        let missing = json!({
            "id": "pet", "type": "sprite", "x": 0, "y": 0, "w": 32, "h": 32, "sprite": "p_unicorn", "color": "#ffffff",
        });
        let mut sent = design(json!([big, twin, lopsided, missing]));
        sent["width"] = json!(5000);
        sent["dim"] = json!(95);
        sent["background"] = json!({ "type": "sprite", "id": "p_drowsepatch_cat" });
        let errors = parse_design(&sent, &some_art).unwrap_err();
        let fields: Vec<_> = errors.iter().map(|e| e.field).collect();
        assert_eq!(fields[..3], ["width", "dim", "background"]);
        let text = messages(&errors).join("\n");
        for expected in [
            "Layer 1 (Text): {nickname} isn't a placeholder",
            "Layer 1 (Text): the font size goes from 8 to 200 px.",
            "Layer 1 (Text): colour: colours look like",
            "Layer 2 (Text): another layer has the same id.",
            "Layer 2 (Text): a text has 1 to 4 lines.",
            "Layer 3 (Avatar): an avatar is square",
            "Layer 3 (Avatar): the border is at most 40 px and a quarter",
            "Layer 4 (Sticker): that sticker doesn't exist.",
        ] {
            assert!(text.contains(expected), "{expected:?} in\n{text}");
        }
    }

    #[test]
    fn placeholders_backgrounds_and_limits() {
        assert_eq!(
            unknown_placeholder("{user} joined {server} as #{members} ({ordinal})"),
            None
        );
        assert_eq!(
            unknown_placeholder("a { brace } and {count}"),
            Some("{count}".to_owned())
        );
        assert_eq!(unknown_placeholder("{"), None);
        let blank = parse_design(&design(json!([text_layer("   ")])), &no_art).unwrap_err();
        assert!(blank[0].message.ends_with("write something, or delete the layer."));
        let long = parse_design(&design(json!([text_layer(&"x".repeat(TEXT_MAX + 1))])), &no_art).unwrap_err();
        assert!(long[0].message.contains("this one has 201"));

        let mut sent = design(json!([]));
        sent["background"] = json!({ "type": "upload", "id": "0123456789abcdef0123456789abcdef" });
        let parsed = parse_design(&sent, &no_art).unwrap();
        assert_eq!(parsed.upload(), Some("0123456789abcdef0123456789abcdef"));
        sent["background"]["id"] = json!("../../etc/passwd");
        assert!(parse_design(&sent, &no_art).is_err());
        sent["background"] = json!({ "type": "sprite", "id": "bg_starlit_jetty" });
        assert!(parse_design(&sent, &some_art).is_ok());
        sent["background"] = json!({ "type": "transparent" });
        sent["version"] = json!(2);
        assert!(
            parse_design(&sent, &no_art).unwrap_err()[0]
                .message
                .contains("version 2")
        );

        let many: Vec<Value> = (0..=MAX_LAYERS)
            .map(|i| {
                let mut l = text_layer("Hi");
                l["id"] = json!(format!("t{i}"));
                l
            })
            .collect();
        let errors = parse_design(&design(Value::Array(many)), &no_art).unwrap_err();
        assert_eq!(errors[0].field, "layers");
        assert!(errors[0].message.contains("at most 20 layers"));
    }

    fn avatar_layer(id: &str, size: u32) -> Value {
        json!({
            "id": id, "type": "avatar", "x": 0, "y": 0, "w": size, "h": size,
            "shape": "circle", "border_color": "#000000", "border_width": 0,
        })
    }

    fn shape_layer(id: &str, w: u32, h: u32) -> Value {
        json!({
            "id": id, "type": "shape", "x": 0, "y": 0, "w": w, "h": h,
            "shape": "rect", "fill": "#336699", "border_color": "#000000",
        })
    }

    #[test]
    fn a_layer_is_never_bigger_than_the_canvas() {
        // 1100 x 500 canvas.
        let ok = parse_design(&design(json!([shape_layer("whole", 1100, 500)])), &no_art);
        assert!(ok.is_ok(), "{ok:?}");
        for (w, h) in [(1101, 500), (1100, 501)] {
            let errors = parse_design(&design(json!([shape_layer("big", w, h)])), &no_art).unwrap_err();
            assert_eq!(
                messages(&errors),
                ["Layer 1 (Shape): it is bigger than the canvas (1100×500 px)."]
            );
        }
        // A text box is as tall as its lines, and that counts: 200 px, four lines is 1040 px.
        let mut tall = text_layer("Hi");
        tall["size"] = json!(200);
        tall["max_lines"] = json!(4);
        let errors = parse_design(&design(json!([tall])), &no_art).unwrap_err();
        assert!(messages(&errors)[0].ends_with("it is bigger than the canvas (1100×500 px)."));
        // Sitting partly off the canvas is fine.
        let mut off = shape_layer("off", 1100, 500);
        off["x"] = json!(-400);
        assert!(parse_design(&design(json!([off])), &no_art).is_ok());
    }

    #[test]
    fn avatars_are_limited_in_size_and_number() {
        // The tallest canvas is 1000 px, which already keeps an avatar under its own 1024 px cap.
        let mut sent = design(json!([avatar_layer("a", 1000)]));
        sent["width"] = json!(1600);
        sent["height"] = json!(1000);
        assert!(parse_design(&sent, &no_art).is_ok());
        sent["layers"] = json!([avatar_layer("a", 1025)]);
        let errors = parse_design(&sent, &no_art).unwrap_err();
        assert!(
            messages(&errors).contains(&"Layer 1 (Avatar): an avatar is at most 1024 px across."),
            "{errors:?}"
        );
        // Four fit, a fifth does not, hidden or not.
        let four: Vec<Value> = (0..MAX_AVATARS).map(|i| avatar_layer(&format!("a{i}"), 200)).collect();
        sent["layers"] = json!(four);
        assert!(parse_design(&sent, &no_art).is_ok());
        let mut five = four.clone();
        let mut spare = avatar_layer("a9", 200);
        spare["hidden"] = json!(true);
        five.push(spare);
        sent["layers"] = json!(five);
        let errors = parse_design(&sent, &no_art).unwrap_err();
        assert_eq!(messages(&errors), ["A design has at most 4 avatars."]);
    }

    #[test]
    fn what_the_layers_cost_to_draw_is_capped() {
        let shape = |id: &str| shape_layer(id, 1100, 500);
        let costs = |layers: &Value| {
            let parsed = parse_design(&design(layers.clone()), &no_art).unwrap();
            layers_cost(&parsed.layers)
        };
        // A shape costs its area, an avatar five times that, a text layer by the characters it can show.
        assert_eq!(costs(&json!([shape("s")])), 1100 * 500);
        assert_eq!(costs(&json!([avatar_layer("a", 200)])), 5 * 200 * 200);
        assert_eq!(costs(&json!([text_layer("Hi")])), 2 * TEXT_CHAR_COST);
        // The templates' kind of design sits far below the cap.
        let usual = json!([shape("s"), avatar_layer("a", 200), text_layer("WELCOME {user}")]);
        assert!(costs(&usual) < LAYER_COST_MAX / 4, "{}", costs(&usual));

        // 375 characters of text fit in the cap, a 376th does not.
        let texts = |lengths: &[usize]| -> Vec<Value> {
            lengths
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    let mut t = text_layer(&"x".repeat(*n));
                    t["id"] = json!(format!("t{i}"));
                    t
                })
                .collect()
        };
        let characters = (LAYER_COST_MAX / TEXT_CHAR_COST) as usize;
        assert_eq!(characters, 375);
        assert!(parse_design(&design(json!(texts(&[200, 175]))), &no_art).is_ok());
        let errors = parse_design(&design(json!(texts(&[200, 176]))), &no_art).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field, "layers");
        assert!(
            errors[0]
                .message
                .starts_with("The visible layers are too heavy to draw quickly together (101% of what a card may use)"),
            "{}",
            errors[0].message
        );
        // Hiding a layer takes its cost away.
        let mut heavy = texts(&[200, 176]);
        heavy[0]["hidden"] = json!(true);
        assert!(parse_design(&design(json!(heavy)), &no_art).is_ok());
    }

    #[test]
    fn placeholders_count_at_the_most_they_can_be() {
        assert_eq!(filled_length(""), 0);
        assert_eq!(filled_length("WELCOME"), 7);
        assert_eq!(filled_length("{user}"), NAME_MAX);
        assert_eq!(filled_length("a {server}!"), 3 + SERVER_MAX);
        assert_eq!(
            filled_length("{user}{username}{server}{members}{ordinal}"),
            2 * NAME_MAX + SERVER_MAX + 10 + 12
        );
        // What is not a placeholder is what it looks like; a character is a character, not a byte.
        assert_eq!(filled_length("{nope} { {user"), 14);
        assert_eq!(filled_length("😀李{user}"), 2 + NAME_MAX);
        assert_eq!(cost_share(&[]), 0);
    }

    #[test]
    fn deliveries_follow_the_kind() {
        assert_eq!(check_delivery(CardKind::Welcome, "dm").unwrap(), Delivery::Dm);
        assert_eq!(check_delivery(CardKind::Goodbye, "dm").unwrap_err().field, "delivery");
        assert!(check_delivery(CardKind::Welcome, "loud").is_err());
        for d in Delivery::ALL {
            assert_eq!(Delivery::parse(d.key()), Some(d));
        }
        assert!(!Delivery::TextOnly.has_image() && !Delivery::ImageOnly.has_text());
        assert_eq!(CardKind::Goodbye.default_delivery(), Delivery::TextOnly);
        assert_eq!(Delivery::offered(CardKind::Goodbye).len(), 4);
        for kind in CardKind::ALL {
            assert_eq!(CardKind::parse(kind.key()), Some(kind));
        }
    }
}
