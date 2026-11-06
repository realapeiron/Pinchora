//! Ready-made welcome and goodbye designs for the editor's gallery. The classic ones are the welcome card
//! Pinchora always drew, one per theme, layer for layer; a server that never opened the editor gets the
//! classic card in its theme, headline and accent.

use pinchora_core::card_design::{
    Align, AvatarShape, Background, CardDesign, CardKind, Content, DESIGN_VERSION, Direction, Font, Layer, ShapeKind,
    text_height,
};

use crate::{Theme, svg::truncate};

/// Every template by key, with the name the gallery shows, in gallery order.
pub const TEMPLATES: [(&str, &str); 8] = [
    ("classic-reef", "Classic reef"),
    ("classic-sunset", "Classic sunset"),
    ("classic-forest", "Classic forest"),
    ("classic-candy", "Classic candy"),
    ("classic-midnight", "Classic midnight"),
    ("starlit-banner", "Starlit banner"),
    ("clean-card", "Clean card"),
    ("pet-party", "Pet party"),
];

/// The headline of the classic card, at most, in characters.
const TITLE_MAX: usize = 18;

/// The template `key` for `kind`, if there is one.
pub fn template(key: &str, kind: CardKind) -> Option<CardDesign> {
    if let Some(theme) = key.strip_prefix("classic-") {
        return Theme::PRESETS
            .contains(&theme)
            .then(|| classic(kind, theme, None, None));
    }
    match key {
        "starlit-banner" => Some(starlit_banner(kind)),
        "clean-card" => Some(clean_card(kind)),
        "pet-party" => Some(pet_party(kind)),
        _ => None,
    }
}

/// The classic card: the night-sea gradient of `theme`, a stage floor with two crabs, the pixelated
/// avatar in a notched frame, the headline, the name and a line about the server. `title` and `accent`
/// replace the theme's headline and accent colour, as `/config welcome-card` sets them.
pub fn classic(kind: CardKind, theme: &str, title: Option<&str>, accent: Option<u32>) -> CardDesign {
    let t = Theme::preset(theme).with_accent(accent);
    let headline = title.unwrap_or(match kind {
        CardKind::Welcome => pinchora_core::settings::DEFAULT_WELCOME_TITLE,
        CardKind::Goodbye => "Goodbye",
    });
    let line = match kind {
        CardKind::Welcome => "to {server} · member #{members}",
        CardKind::Goodbye => "left {server} · {members} members now",
    };
    CardDesign {
        version: DESIGN_VERSION,
        width: 1100,
        height: 500,
        background: Background::Gradient {
            from: t.bg_top.clone(),
            to: t.bg_bottom.clone(),
            direction: Direction::Vertical,
            pixel: true,
        },
        dim: 0,
        layers: vec![
            shape("floor", ShapeKind::Rect, (0, 440, 1100, 60), &t.panel, 55),
            shape("floor-edge", ShapeKind::Dashes, (0, 440, 1100, 8), &t.panel_edge, 60),
            glyph("crab-left", "crab", (70, 392, 54, 36), &t.accent),
            glyph("crab-right", "crab", (976, 392, 54, 36), &t.gold),
            avatar("avatar", AvatarShape::Notched, (450, 36, 200), 12, &t.accent, true),
            text(
                "title",
                &truncate(headline, TITLE_MAX).to_uppercase(),
                Font::Pixel,
                false,
                64,
                &t.ink,
                (50, 252, 1000),
            ),
            text("name", "{user}", Font::Sans, true, 36, &t.accent, (50, 332, 1000)),
            text("line", line, Font::Sans, false, 22, &t.muted, (50, 388, 1000)),
        ],
    }
}

/// A wide pixel-art night by the jetty, the avatar in a round gold frame and the text beside it.
fn starlit_banner(kind: CardKind) -> CardDesign {
    let (title, line) = match kind {
        CardKind::Welcome => ("WELCOME", "You are our {ordinal} member!"),
        CardKind::Goodbye => ("SEE YOU", "{server} has {members} members now"),
    };
    let mut headline = text("title", title, Font::Pixel, false, 56, "#ffd166", (370, 104, 760));
    let mut name = text("name", "{user}", Font::Sans, true, 48, "#fff1e8", (370, 172, 760));
    for layer in [&mut headline, &mut name] {
        if let Content::Text { shadow, .. } = &mut layer.content {
            *shadow = true;
        }
        if let Content::Text { align, .. } = &mut layer.content {
            *align = Align::Left;
        }
    }
    let mut about = text("line", line, Font::Sans, false, 26, "#8fd3ff", (370, 246, 760));
    if let Content::Text { align, .. } = &mut about.content {
        *align = Align::Left;
    }
    CardDesign {
        version: DESIGN_VERSION,
        width: 1200,
        height: 400,
        background: Background::Sprite {
            id: "bg_starlit_jetty".to_owned(),
        },
        dim: 25,
        layers: vec![
            shape("plate", ShapeKind::Rounded, (340, 84, 800, 232), "#0b1029", 60),
            avatar("avatar", AvatarShape::Circle, (70, 80, 240), 8, "#ffd166", false),
            headline,
            name,
            about,
        ],
    }
}

/// No background at all: a card that floats on Discord's own colours, the round avatar on top.
fn clean_card(kind: CardKind) -> CardDesign {
    let line = match kind {
        CardKind::Welcome => "just joined {server}",
        CardKind::Goodbye => "has left {server}",
    };
    let mut panel = shape("panel", ShapeKind::Rounded, (0, 0, 1000, 360), "#141b3d", 92);
    if let Content::Shape {
        border_color,
        border_width,
        ..
    } = &mut panel.content
    {
        *border_color = "#2e3d75".to_owned();
        *border_width = 4;
    }
    CardDesign {
        version: DESIGN_VERSION,
        width: 1000,
        height: 360,
        background: Background::Transparent,
        dim: 0,
        layers: vec![
            panel,
            avatar("avatar", AvatarShape::Circle, (410, 34, 180), 6, "#8fd3ff", false),
            text("name", "{user}", Font::Sans, true, 44, "#fff1e8", (40, 226, 920)),
            text("line", line, Font::Sans, false, 24, "#8fa0d8", (40, 290, 920)),
        ],
    }
}

/// A garden full of pets around a candy-coloured plate.
fn pet_party(kind: CardKind) -> CardDesign {
    let (title, line) = match kind {
        CardKind::Welcome => ("WELCOME!", "member #{members} of {server}"),
        CardKind::Goodbye => ("BYE BYE!", "{server} will miss you"),
    };
    let mut plate = shape("plate", ShapeKind::Notched, (250, 256, 600, 210), "#4a2f66", 88);
    if let Content::Shape {
        border_color,
        border_width,
        ..
    } = &mut plate.content
    {
        *border_color = "#ff8fb1".to_owned();
        *border_width = 6;
    }
    let mut headline = text("title", title, Font::Pixel, false, 48, "#fff1e8", (250, 276, 600));
    if let Content::Text { shadow, .. } = &mut headline.content {
        *shadow = true;
    }
    CardDesign {
        version: DESIGN_VERSION,
        width: 1100,
        height: 500,
        background: Background::Sprite {
            id: "bg_cherryfoam_garden".to_owned(),
        },
        dim: 15,
        layers: vec![
            plate,
            glyph("heart", "heart", (232, 64, 42, 36), "#ff4d6d"),
            glyph("star", "star", (826, 64, 42, 36), "#ffd166"),
            sticker("cat", "p_drowsepatch_cat", (70, 340, 144, 104)),
            sticker("pup", "p_tumblebutton_pup", (886, 310, 144, 136)),
            avatar("avatar", AvatarShape::Notched, (470, 40, 160), 10, "#ff8fb1", true),
            headline,
            text("name", "{user}", Font::Sans, true, 36, "#ffd166", (270, 340, 560)),
            text("line", line, Font::Sans, false, 22, "#fff1e8", (270, 394, 560)),
        ],
    }
}

fn frame(id: &str, (x, y, w, h): (i32, i32, u32, u32), content: Content) -> Layer {
    Layer {
        id: id.to_owned(),
        hidden: false,
        locked: false,
        x,
        y,
        w,
        h,
        opacity: 100,
        content,
    }
}

fn shape(id: &str, shape: ShapeKind, at: (i32, i32, u32, u32), fill: &str, opacity: u8) -> Layer {
    Layer {
        opacity,
        ..frame(
            id,
            at,
            Content::Shape {
                shape,
                fill: fill.to_owned(),
                border_color: "#000000".to_owned(),
                border_width: 0,
            },
        )
    }
}

fn glyph(id: &str, name: &str, at: (i32, i32, u32, u32), color: &str) -> Layer {
    frame(
        id,
        at,
        Content::Sprite {
            sprite: name.to_owned(),
            color: color.to_owned(),
        },
    )
}

fn sticker(id: &str, sprite: &str, at: (i32, i32, u32, u32)) -> Layer {
    glyph(id, sprite, at, "#ffffff")
}

fn avatar(
    id: &str,
    shape: AvatarShape,
    (x, y, size): (i32, i32, u32),
    border: u32,
    color: &str,
    pixelate: bool,
) -> Layer {
    frame(
        id,
        (x, y, size, size),
        Content::Avatar {
            shape,
            border_color: color.to_owned(),
            border_width: border,
            pixelate,
        },
    )
}

fn text(id: &str, body: &str, font: Font, bold: bool, size: u32, color: &str, (x, y, w): (i32, i32, u32)) -> Layer {
    frame(
        id,
        (x, y, w, text_height(size, 1)),
        Content::Text {
            text: body.to_owned(),
            font,
            bold,
            size,
            color: color.to_owned(),
            align: Align::Center,
            uppercase: false,
            max_lines: 1,
            shadow: false,
            outline_color: "#000000".to_owned(),
            outline_width: 0,
        },
    )
}
