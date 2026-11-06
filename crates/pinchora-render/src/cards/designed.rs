//! Welcome and goodbye images from a server's own design (`pinchora_core::card_design`): the background,
//! then every visible layer bottom to top. The bot and the dashboard's preview both draw through here.

use std::{
    collections::HashMap,
    fmt::Write,
    sync::{Arc, LazyLock, Mutex},
};

use pinchora_core::card_design::{
    Align, AvatarShape, Background, CardDesign, Content, Direction, Font, LINE_HEIGHT, Layer, ShapeKind,
};

use super::{PIXEL, SANS};
use crate::{
    character::atlas,
    encode_png,
    svg::{
        CRAB, HEART, SHELL, STAR, background, data_uri_png, escape, fit_laid_out, notched_path, thousands, truncate,
        wrap_laid_out,
    },
};

/// What the placeholders of a design's text turn into.
#[derive(Debug, Clone, Default)]
pub struct CardFill<'a> {
    /// `{user}`: the member's name as the server shows it.
    pub user: &'a str,
    /// `{username}`: their Discord username.
    pub username: &'a str,
    /// `{server}`.
    pub server: &'a str,
    /// `{members}` and `{ordinal}`: how many members the server has, the new one included.
    pub members: Option<u64>,
}

/// A card drawn from a design.
pub struct DesignedCard<'a> {
    pub design: &'a CardDesign,
    /// The member's avatar as PNG, square, [`CardDesign::avatar_size`] px (any size works; it is scaled).
    pub avatar_png: &'a [u8],
    /// The uploaded background as PNG, when the design has one; without it the canvas stays empty there.
    pub upload_png: Option<&'a [u8]>,
    pub fill: &'a CardFill<'a>,
}

/// Names and server names in a card, at most, in characters.
const NAME_MAX: usize = 32;
const SERVER_MAX: usize = 40;
/// The colour behind an avatar inside its border, as on every card.
const AVATAR_BACK: &str = "#0b1029";
/// The light pixels of the glyph stickers.
const GLYPH_HIGHLIGHT: &str = "#fff1e8";
/// Scaled backgrounds kept at once (each atlas picture at a few scales).
const BACKGROUND_CACHE: usize = 64;

/// `1234` → `1,234th`.
pub fn ordinal(n: u64) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{}{suffix}", thousands(n as i64))
}

/// `text` with its placeholders filled in from `fill`, in one pass: a name that looks like a placeholder is
/// not filled in again.
pub fn fill_placeholders(text: &str, fill: &CardFill<'_>) -> String {
    let value = |key: &str| -> Option<String> {
        Some(match key {
            "user" => truncate(fill.user, NAME_MAX),
            "username" => truncate(fill.username, NAME_MAX),
            "server" => truncate(fill.server, SERVER_MAX),
            "members" => fill.members.map_or_else(|| "?".to_owned(), |n| thousands(n as i64)),
            "ordinal" => fill.members.map_or_else(|| "newest".to_owned(), ordinal),
            _ => return None,
        })
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}').and_then(|end| Some((end, value(&after[..end])?))) {
            Some((end, filled)) => {
                out.push_str(&filled);
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

impl DesignedCard<'_> {
    pub fn to_svg(&self) -> String {
        let d = self.design;
        let (w, h) = (d.width as f32, d.height as f32);
        let mut s = String::with_capacity(64 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        self.background(&mut s, w, h);
        if d.dim > 0 {
            let _ = write!(
                s,
                r##"<rect width="{w}" height="{h}" fill="#000000" fill-opacity="{}"/>"##,
                f32::from(d.dim) / 100.0
            );
        }
        for (i, layer) in d.layers.iter().enumerate() {
            if layer.hidden || layer.opacity == 0 {
                continue;
            }
            self.layer(&mut s, i, layer);
        }
        s.push_str("</svg>");
        s
    }

    fn background(&self, s: &mut String, w: f32, h: f32) {
        match &self.design.background {
            Background::Transparent => {}
            Background::Solid { color } => {
                let _ = write!(s, r#"<rect width="{w}" height="{h}" fill="{}"/>"#, escape(color));
            }
            Background::Gradient {
                from,
                to,
                direction,
                pixel,
            } => gradient(s, w, h, &escape(from), &escape(to), *direction, *pixel),
            Background::Sprite { id } => {
                if let Some((uri, x, y, sw, sh)) = scaled_background(id, self.design.width, self.design.height) {
                    let _ = write!(
                        s,
                        r#"<image x="{x}" y="{y}" width="{sw}" height="{sh}" image-rendering="optimizeSpeed" href="{uri}"/>"#
                    );
                }
            }
            Background::Upload { .. } => {
                if let Some(png) = self.upload_png {
                    let _ = write!(
                        s,
                        r#"<image x="0" y="0" width="{w}" height="{h}" preserveAspectRatio="xMidYMid slice" href="{}"/>"#,
                        data_uri_png(png)
                    );
                }
            }
        }
    }

    fn layer(&self, s: &mut String, index: usize, layer: &Layer) {
        let alpha = f32::from(layer.opacity.min(100)) / 100.0;
        let (x, y, w, h) = (layer.x as f32, layer.y as f32, layer.w as f32, layer.h as f32);
        // A borderless shape fades its fill; anything else fades as a whole.
        let plain_shape = matches!(layer.content, Content::Shape { border_width: 0, .. });
        let group = alpha < 1.0 && !plain_shape;
        if group {
            let _ = write!(s, r#"<g opacity="{alpha}">"#);
        }
        match &layer.content {
            Content::Avatar {
                shape,
                border_color,
                border_width,
                pixelate,
            } => avatar(
                s,
                self.avatar_png,
                *shape,
                x,
                y,
                w,
                *border_width as f32,
                &escape(border_color),
                *pixelate,
                &format!("clip-{index}"),
            ),
            Content::Text {
                text,
                font,
                bold,
                size,
                color,
                align,
                uppercase,
                max_lines,
                shadow,
                outline_color,
                outline_width,
            } => {
                let filled = fill_placeholders(text, self.fill);
                let filled = if *uppercase { filled.to_uppercase() } else { filled };
                let style = TextStyle {
                    family: match font {
                        Font::Pixel => PIXEL,
                        Font::Sans => SANS,
                    },
                    bold: *bold,
                    size: *size as f32,
                    color: escape(color),
                    shadow: *shadow,
                    outline: (*outline_width > 0).then(|| (escape(outline_color), *outline_width as f32)),
                };
                text_block(s, &filled, x, y, w, *align, *max_lines as usize, &style);
            }
            Content::Sprite { sprite, color } => sticker(s, sprite, &escape(color), x, y, w, h),
            Content::Shape {
                shape,
                fill,
                border_color,
                border_width,
            } => {
                let fade = if plain_shape { alpha } else { 1.0 };
                shape_of(
                    s,
                    *shape,
                    x,
                    y,
                    w,
                    h,
                    &escape(fill),
                    fade,
                    &escape(border_color),
                    *border_width as f32,
                );
            }
        }
        if group {
            s.push_str("</g>");
        }
    }
}

/// The card's gradient. Vertical with the pixel look is exactly the background every other card has.
fn gradient(s: &mut String, w: f32, h: f32, from: &str, to: &str, direction: Direction, pixel: bool) {
    if pixel && direction == Direction::Vertical {
        background(s, w, h, from, to);
        return;
    }
    let (x2, y2) = match direction {
        Direction::Vertical => (0, 1),
        Direction::Horizontal => (1, 0),
        Direction::Diagonal => (1, 1),
    };
    let _ = write!(
        s,
        r#"<defs><linearGradient id="bg" x1="0" y1="0" x2="{x2}" y2="{y2}"><stop offset="0" stop-color="{from}"/><stop offset="1" stop-color="{to}"/></linearGradient></defs><rect width="{w}" height="{h}" fill="url(#bg)"/>"#
    );
    if pixel {
        // The dither and sparkles of `background`, over a gradient running another way.
        let mut look = String::new();
        background(&mut look, w, h, from, to);
        if let Some(start) = look.find(r#"<pattern id="dither""#)
            && let Some(end) = look[start..].find("</pattern>")
        {
            let _ = write!(
                s,
                r#"<defs>{}</defs><rect width="{w}" height="{h}" fill="url(#dither)"/>"#,
                &look[start..start + end + "</pattern>".len()]
            );
        }
        if let Some(sparkles) = look.find(r#"<rect x="#) {
            s.push_str(&look[sparkles..]);
        }
    }
}

/// An atlas background scaled up whole until it covers `w`×`h`, centred: its data URI, where it goes and how
/// big it is. Scaled by a whole number so every pixel of the art stays a crisp block.
fn scaled_background(id: &str, w: u32, h: u32) -> Option<(Arc<String>, i64, i64, u32, u32)> {
    static CACHE: LazyLock<Mutex<HashMap<(String, u32), Arc<String>>>> = LazyLock::new(Mutex::default);
    let sprite = atlas().get(id)?;
    let (sw, sh) = (sprite.width().max(1), sprite.height().max(1));
    let k = w.div_ceil(sw).max(h.div_ceil(sh)).max(1);
    let key = (id.to_owned(), k);
    let cached = CACHE.lock().expect("background cache").get(&key).cloned();
    let uri = match cached {
        Some(uri) => uri,
        None => {
            let big = image::imageops::resize(sprite.as_ref(), sw * k, sh * k, image::imageops::FilterType::Nearest);
            let uri = Arc::new(data_uri_png(&encode_png(&big).ok()?));
            let mut cache = CACHE.lock().expect("background cache");
            if cache.len() >= BACKGROUND_CACHE {
                cache.clear();
            }
            cache.insert(key, uri.clone());
            uri
        }
    };
    let (bw, bh) = (sw * k, sh * k);
    Some((
        uri,
        (i64::from(w) - i64::from(bw)) / 2,
        (i64::from(h) - i64::from(bh)) / 2,
        bw,
        bh,
    ))
}

/// An atlas sprite at its own size, as a data URI.
fn sprite_uri(id: &str) -> Option<(Arc<String>, u32, u32)> {
    static CACHE: LazyLock<Mutex<HashMap<String, (Arc<String>, u32, u32)>>> = LazyLock::new(Mutex::default);
    if let Some(found) = CACHE.lock().expect("sticker cache").get(id) {
        return Some(found.clone());
    }
    let sprite = atlas().get(id)?;
    let made = (
        Arc::new(data_uri_png(&encode_png(&sprite).ok()?)),
        sprite.width(),
        sprite.height(),
    );
    // Only real atlas names reach here, so the cache is bounded by the atlas.
    CACHE.lock().expect("sticker cache").insert(id.to_owned(), made.clone());
    Some(made)
}

fn avatar(
    s: &mut String,
    png: &[u8],
    shape: AvatarShape,
    x: f32,
    y: f32,
    size: f32,
    border: f32,
    edge: &str,
    pixelate: bool,
    clip: &str,
) {
    let inner = (size - 2.0 * border).max(1.0);
    let (ix, iy) = (x + border, y + border);
    let clip_path = match shape {
        AvatarShape::Notched => {
            // The notch is the border, as on every card; a borderless one still gets a step.
            let p = if border > 0.0 {
                border
            } else {
                (size / 16.0).max(2.0).round()
            };
            if border > 0.0 {
                let _ = write!(
                    s,
                    r#"<path d="{}" fill="{edge}"/><path d="{}" fill="{AVATAR_BACK}"/>"#,
                    notched_path(x, y, size, size, p),
                    notched_path(ix, iy, inner, inner, p),
                );
            }
            format!(r#"<path d="{}"/>"#, notched_path(ix, iy, inner, inner, p))
        }
        AvatarShape::Circle => {
            let (cx, cy) = (x + size / 2.0, y + size / 2.0);
            if border > 0.0 {
                let _ = write!(
                    s,
                    r#"<circle cx="{cx}" cy="{cy}" r="{}" fill="{edge}"/><circle cx="{cx}" cy="{cy}" r="{}" fill="{AVATAR_BACK}"/>"#,
                    size / 2.0,
                    inner / 2.0
                );
            }
            format!(r#"<circle cx="{cx}" cy="{cy}" r="{}"/>"#, inner / 2.0)
        }
        AvatarShape::Square | AvatarShape::Rounded => {
            let r = if shape == AvatarShape::Rounded {
                size * 0.16
            } else {
                0.0
            };
            let ir = (r - border).max(0.0);
            if border > 0.0 {
                let _ = write!(
                    s,
                    r#"<rect x="{x}" y="{y}" width="{size}" height="{size}" rx="{r}" fill="{edge}"/><rect x="{ix}" y="{iy}" width="{inner}" height="{inner}" rx="{ir}" fill="{AVATAR_BACK}"/>"#
                );
            }
            format!(r#"<rect x="{ix}" y="{iy}" width="{inner}" height="{inner}" rx="{ir}"/>"#)
        }
    };
    let rendering = if pixelate {
        r#" image-rendering="optimizeSpeed""#
    } else {
        ""
    };
    let _ = write!(
        s,
        r#"<clipPath id="{clip}">{clip_path}</clipPath><image x="{ix}" y="{iy}" width="{inner}" height="{inner}" preserveAspectRatio="none"{rendering} clip-path="url(#{clip})" href="{}"/>"#,
        data_uri_png(png)
    );
}

struct TextStyle<'a> {
    family: &'a str,
    bold: bool,
    size: f32,
    color: String,
    shadow: bool,
    /// Colour and width.
    outline: Option<(String, f32)>,
}

/// `text` wrapped to `w` in at most `max_lines` lines, the first baseline one font size below `y`.
fn text_block(
    s: &mut String,
    text: &str,
    x: f32,
    y: f32,
    w: f32,
    align: Align,
    max_lines: usize,
    style: &TextStyle<'_>,
) {
    let lines = if max_lines <= 1 {
        let one: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        vec![fit_laid_out(&one, style.family, style.bold, style.size, w)]
    } else {
        wrap_laid_out(text, style.family, style.bold, style.size, w, max_lines)
    };
    let (ax, anchor) = match align {
        Align::Left => (x, "start"),
        Align::Center => (x + w / 2.0, "middle"),
        Align::Right => (x + w, "end"),
    };
    let weight = if style.bold { "bold" } else { "normal" };
    let (family, size) = (style.family, style.size);
    let outline = style.outline.as_ref().map_or_else(String::new, |(color, width)| {
        let join = if family == PIXEL { "miter" } else { "round" };
        format!(
            r#" stroke="{color}" stroke-width="{}" stroke-linejoin="{join}" paint-order="stroke""#,
            width * 2.0
        )
    });
    let drop = (size / 16.0).round().max(2.0);
    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let body = escape(line);
        let base = y + size + i as f32 * size * LINE_HEIGHT;
        if style.shadow {
            let _ = write!(
                s,
                r##"<text x="{}" y="{}" font-family="{family}" font-size="{size}" font-weight="{weight}" fill="#000000" fill-opacity="0.45"{} text-anchor="{anchor}">{body}</text>"##,
                ax + drop,
                base + drop,
                outline.replace(r#"stroke=""#, r#"stroke-opacity="0.45" stroke=""#),
            );
        }
        let _ = write!(
            s,
            r#"<text x="{ax}" y="{base}" font-family="{family}" font-size="{size}" font-weight="{weight}" fill="{}"{outline} text-anchor="{anchor}">{body}</text>"#,
            style.color
        );
    }
}

/// A glyph in `color`, or an atlas sprite, as big as fits the box without changing its shape, centred.
fn sticker(s: &mut String, id: &str, color: &str, x: f32, y: f32, w: f32, h: f32) {
    let glyph = match id {
        "crab" => Some(CRAB),
        "heart" => Some(HEART),
        "star" => Some(STAR),
        "shell" => Some(SHELL),
        _ => None,
    };
    if let Some(rows) = glyph {
        let (cols, count) = (
            rows.iter().map(|r| r.len()).max().unwrap_or(1) as f32,
            rows.len() as f32,
        );
        let scale = (w / cols).min(h / count);
        let (gx, gy) = (x + (w - cols * scale) / 2.0, y + (h - count * scale) / 2.0);
        crate::svg::sprite(s, rows, gx, gy, scale, color, GLYPH_HIGHLIGHT);
        return;
    }
    let Some((uri, sw, sh)) = sprite_uri(id) else {
        return;
    };
    let scale = (w / sw as f32).min(h / sh as f32);
    let (dw, dh) = (sw as f32 * scale, sh as f32 * scale);
    let _ = write!(
        s,
        r#"<image x="{}" y="{}" width="{dw}" height="{dh}" image-rendering="optimizeSpeed" href="{uri}"/>"#,
        x + (w - dw) / 2.0,
        y + (h - dh) / 2.0
    );
}

fn shape_of(
    s: &mut String,
    shape: ShapeKind,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    fill: &str,
    alpha: f32,
    edge: &str,
    border: f32,
) {
    let fade = if alpha < 1.0 {
        format!(r#" fill-opacity="{alpha}""#)
    } else {
        String::new()
    };
    // A border is drawn inside the shape, so the shape keeps the size it was given.
    let stroke = if border > 0.0 {
        format!(r#" stroke="{edge}" stroke-width="{border}""#)
    } else {
        String::new()
    };
    let (bx, by, bw, bh) = (
        x + border / 2.0,
        y + border / 2.0,
        (w - border).max(0.0),
        (h - border).max(0.0),
    );
    match shape {
        ShapeKind::Rect => {
            let _ = write!(
                s,
                r#"<rect x="{bx}" y="{by}" width="{bw}" height="{bh}" fill="{fill}"{fade}{stroke}/>"#
            );
        }
        ShapeKind::Rounded => {
            let r = (w.min(h) * 0.2 - border / 2.0).max(0.0);
            let _ = write!(
                s,
                r#"<rect x="{bx}" y="{by}" width="{bw}" height="{bh}" rx="{r}" fill="{fill}"{fade}{stroke}/>"#
            );
        }
        ShapeKind::Ellipse => {
            let _ = write!(
                s,
                r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" fill="{fill}"{fade}{stroke}/>"#,
                x + w / 2.0,
                y + h / 2.0,
                bw / 2.0,
                bh / 2.0
            );
        }
        ShapeKind::Notched => {
            let p = (w.min(h) / 12.0).round().clamp(2.0, 16.0).max(border);
            if border > 0.0 {
                let _ = write!(s, r#"<path d="{}" fill="{edge}"/>"#, notched_path(x, y, w, h, p));
            }
            let _ = write!(
                s,
                r#"<path d="{}" fill="{fill}"{fade}/>"#,
                notched_path(
                    x + border,
                    y + border,
                    (w - 2.0 * border).max(0.0),
                    (h - 2.0 * border).max(0.0),
                    p
                )
            );
        }
        ShapeKind::Dashes => {
            let dash = h * 2.5;
            let mut bx = x;
            while bx < x + w && dash > 0.0 {
                let len = dash.min(x + w - bx);
                let _ = write!(
                    s,
                    r#"<rect x="{bx}" y="{y}" width="{len}" height="{h}" fill="{fill}"{fade}/>"#
                );
                bx += dash * 2.0;
            }
        }
    }
}
