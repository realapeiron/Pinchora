use super::*;

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
    check_canvas(&mut design, catalog, &mut fail);
    if design.layers.len() > MAX_LAYERS {
        fail("layers", format!("A design has at most {MAX_LAYERS} layers."));
    }
    let (canvas_w, canvas_h) = (design.width, design.height);
    let mut seen = std::collections::HashSet::new();
    for (i, layer) in design.layers.iter_mut().enumerate() {
        let name = format!("Layer {} ({})", i + 1, layer.label());
        let mut bad = |message: String| fail("layers", format!("{name}: {message}"));
        check_layer(layer, &mut seen, catalog, &mut bad);
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
pub(super) fn unknown_placeholder(text: &str) -> Option<String> {
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

fn check_canvas(design: &mut CardDesign, catalog: &dyn Catalog, fail: &mut dyn FnMut(&'static str, String)) {
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
    if design.alt_text.chars().count() > 1024 {
        fail("alt_text", "Image alt text is at most 1024 characters.".to_owned());
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
}

fn check_avatar(layer: &mut Layer, bad: &mut dyn FnMut(String)) {
    if let Content::Avatar {
        border_color,
        border_width,
        ..
    } = &mut layer.content
    {
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
}

fn check_text(layer: &mut Layer, bad: &mut dyn FnMut(String)) {
    if let Content::Text {
        text,
        size,
        color,
        max_lines,
        outline_color,
        outline_width,
        ..
    } = &mut layer.content
    {
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
}

fn check_sticker(layer: &mut Layer, catalog: &dyn Catalog, bad: &mut dyn FnMut(String)) {
    if let Content::Sprite { sprite, color } = &mut layer.content {
        let known = GLYPHS.contains(&sprite.as_str())
            || (STICKER_PREFIXES.iter().any(|p| sprite.starts_with(p)) && catalog.has_sprite(sprite));
        if !known {
            bad("that sticker doesn't exist.".to_owned());
        }
        if let Err(message) = tidy_color(color) {
            bad(format!("colour: {message}"));
        }
    }
}

fn check_shape(layer: &mut Layer, bad: &mut dyn FnMut(String)) {
    if let Content::Shape {
        fill,
        border_color,
        border_width,
        ..
    } = &mut layer.content
    {
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

fn check_layer(
    layer: &mut Layer,
    seen: &mut std::collections::HashSet<String>,
    catalog: &dyn Catalog,
    bad: &mut dyn FnMut(String),
) {
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
        Content::Avatar { .. } => check_avatar(layer, bad),
        Content::Text { .. } => check_text(layer, bad),
        Content::Sprite { .. } => check_sticker(layer, catalog, bad),
        Content::Shape { .. } => check_shape(layer, bad),
    }
}
