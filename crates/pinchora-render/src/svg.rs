//! Small SVG building blocks for the pixel-art card style.

use std::fmt::Write;

use base64::Engine;
use unicode_segmentation::UnicodeSegmentation;

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Cuts `text` to `max` graphemes, ending with an ellipsis when shortened.
pub fn truncate(text: &str, max: usize) -> String {
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    if graphemes.len() <= max {
        return text.to_owned();
    }
    let mut out: String = graphemes[..max.saturating_sub(1)].concat();
    out.push('…');
    out
}

/// What each printable ASCII character, from the space, takes in Noto Sans, in ems; read off the
/// renderer, so names can be fitted into a box of pixels.
const SANS_EM: [f32; 95] = [
    0.246, 0.246, 0.400, 0.640, 0.580, 0.890, 0.730, 0.207, 0.297, 0.297, 0.500, 0.570, 0.252, 0.321, 0.246, 0.370,
    0.580, 0.525, 0.580, 0.580, 0.596, 0.575, 0.580, 0.583, 0.580, 0.580, 0.270, 0.270, 0.570, 0.570, 0.570, 0.480,
    0.930, 0.676, 0.649, 0.628, 0.731, 0.549, 0.517, 0.737, 0.734, 0.339, 0.286, 0.628, 0.522, 0.909, 0.753, 0.795,
    0.602, 0.795, 0.628, 0.557, 0.599, 0.726, 0.636, 0.978, 0.617, 0.599, 0.586, 0.300, 0.370, 0.300, 0.570, 0.500,
    0.500, 0.559, 0.617, 0.488, 0.617, 0.572, 0.374, 0.617, 0.612, 0.233, 0.270, 0.541, 0.231, 0.949, 0.612, 0.615,
    0.617, 0.617, 0.413, 0.482, 0.374, 0.612, 0.538, 0.827, 0.549, 0.538, 0.477, 0.300, 0.270, 0.300, 0.570,
];

/// About how wide `text` is in Noto Sans at `size` px: close enough to cut a name to fit a box.
pub fn sans_width(text: &str, size: f32) -> f32 {
    let em: f32 = text
        .chars()
        .map(|c| match c {
            ' '..='~' => SANS_EM[c as usize - 32],
            // Emoji and the wide scripts.
            c if c as u32 >= 0x2e80 || (0x2190..0x2c00).contains(&(c as u32)) => 1.0,
            _ => 0.6,
        })
        .sum();
    em * size
}

/// How wide `text` is in the pixel font at `size` px: every glyph is a whole number of font
/// pixels, eight to the em, so this is exact.
pub fn pixel_width(text: &str, size: f32) -> f32 {
    let pixels: u32 = text
        .chars()
        .map(|c| match c {
            'I' | 'i' | '.' | '!' | '\'' | ' ' | ':' | ';' | '|' => 3,
            ',' | '(' | ')' => 4,
            'E' | 'F' | 'L' | 'T' | 'Z' | 'e' | 'f' | 'l' | 't' | 'z' | '-' | '1' => 5,
            'M' | 'N' | 'V' | 'W' | 'X' | 'Y' | 'm' | 'n' | 'v' | 'w' | 'x' | 'y' => 7,
            _ => 6,
        })
        .sum();
    pixels as f32 * size / 8.0
}

/// Cuts `text` to what fits `max` px in Noto Sans at `size`, ending with an ellipsis when shortened.
pub fn truncate_to(text: &str, size: f32, max: f32) -> String {
    if sans_width(text, size) <= max {
        return text.to_owned();
    }
    let mut out = String::new();
    for g in text.graphemes(true) {
        if sans_width(&format!("{out}{g}…"), size) > max {
            break;
        }
        out.push_str(g);
    }
    out.push('…');
    out
}

/// Greedy word wrap by grapheme count; the last allowed line gets an ellipsis.
pub fn wrap(text: &str, per_line: usize, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if candidate.graphemes(true).count() <= per_line {
            line = candidate;
        } else {
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            line = truncate(word, per_line);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        let last = lines.last_mut().expect("max_lines > 0");
        *last = truncate(&format!("{last} …"), per_line);
    }
    lines
}

/// `12345` → `12,345`.
pub fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// `1234567` → `1.2M`, for tight boxes.
pub fn compact(n: i64) -> String {
    let a = n.unsigned_abs() as f64;
    let sign = if n < 0 { "-" } else { "" };
    match a {
        a if a >= 1e9 => format!("{sign}{:.1}B", a / 1e9),
        a if a >= 1e6 => format!("{sign}{:.1}M", a / 1e6),
        a if a >= 1e4 => format!("{sign}{:.1}K", a / 1e3),
        _ => thousands(n),
    }
}

/// Rectangle path with one-step notched corners, the "rounded" corner of pixel art.
pub fn notched_path(x: f32, y: f32, w: f32, h: f32, p: f32) -> String {
    format!(
        "M{a},{y} H{b} V{c} H{r} V{d} H{b} V{bot} H{a} V{d} H{x} V{c} H{a} Z",
        a = x + p,
        b = x + w - p,
        c = y + p,
        d = y + h - p,
        r = x + w,
        bot = y + h,
    )
}

/// A notched panel with a `border`-thick outline.
pub fn panel(out: &mut String, x: f32, y: f32, w: f32, h: f32, border: f32, fill: &str, edge: &str) {
    let _ = write!(
        out,
        r#"<path d="{}" fill="{edge}"/><path d="{}" fill="{fill}"/>"#,
        notched_path(x, y, w, h, border),
        notched_path(x + border, y + border, w - 2.0 * border, h - 2.0 * border, border),
    );
}

/// Like [`panel`], with a fill that lets what is behind show through: the outline is a ring, so
/// its colour does not shine through the fill.
pub fn panel_alpha(out: &mut String, x: f32, y: f32, w: f32, h: f32, border: f32, fill: &str, edge: &str, alpha: f32) {
    let inner = notched_path(x + border, y + border, w - 2.0 * border, h - 2.0 * border, border);
    let _ = write!(
        out,
        r#"<path d="{} {inner}" fill="{edge}" fill-rule="evenodd"/><path d="{inner}" fill="{fill}" fill-opacity="{alpha}"/>"#,
        notched_path(x, y, w, h, border),
    );
}

/// A translucent dark plate behind text that would otherwise sit on a background picture.
pub fn plate(out: &mut String, x: f32, y: f32, w: f32, h: f32, alpha: f32) {
    let _ = write!(
        out,
        r##"<path d="{}" fill="#0b1029" fill-opacity="{alpha}"/>"##,
        notched_path(x, y, w, h, 4.0)
    );
}

/// Blends `#rrggbb` colours: `0` is `from`, `1` is `to`. Anything unreadable counts as black.
pub fn mix(from: &str, to: &str, t: f32) -> String {
    let rgb = |c: &str| {
        let v = c
            .strip_prefix('#')
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or(0);
        [(v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff]
    };
    let (a, b) = (rgb(from), rgb(to));
    let channel = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t.clamp(0.0, 1.0)).round() as u32;
    format!("#{:02x}{:02x}{:02x}", channel(0), channel(1), channel(2))
}

/// Segmented progress bar.
pub fn segment_bar(
    out: &mut String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    fraction: f32,
    segments: u32,
    fill: &str,
    empty: &str,
    frame: &str,
) {
    let pad = 6.0;
    panel(out, x, y, w, h, 4.0, "#0b1029", frame);
    let gap = 3.0;
    let inner_w = w - 2.0 * pad;
    let seg_w = (inner_w - gap * (segments - 1) as f32) / segments as f32;
    let lit = (fraction.clamp(0.0, 1.0) * segments as f32).round() as u32;
    for i in 0..segments {
        let sx = x + pad + i as f32 * (seg_w + gap);
        let color = if i < lit { fill } else { empty };
        let _ = write!(
            out,
            r#"<rect x="{sx}" y="{}" width="{seg_w}" height="{}" fill="{color}"/>"#,
            y + pad,
            h - 2.0 * pad
        );
        if i < lit {
            // Top highlight row, the classic pixel bevel.
            let _ = write!(
                out,
                r##"<rect x="{sx}" y="{}" width="{seg_w}" height="3" fill="#ffffff" fill-opacity="0.35"/>"##,
                y + pad
            );
        }
    }
}

/// Draws a bitmap sprite: one string per row, `#` = `color`, `o` = highlight, anything else empty.
pub fn sprite(out: &mut String, rows: &[&str], x: f32, y: f32, scale: f32, color: &str, highlight: &str) {
    for (r, row) in rows.iter().enumerate() {
        for (c, ch) in row.chars().enumerate() {
            let fill = match ch {
                '#' => color,
                'o' => highlight,
                _ => continue,
            };
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{scale}" height="{scale}" fill="{fill}"/>"#,
                x + c as f32 * scale,
                y + r as f32 * scale
            );
        }
    }
}

pub const HEART: &[&str] = &[".##.##.", "#o#####", "#######", ".#####.", "..###..", "...#..."];
pub const STAR: &[&str] = &["...#...", "..###..", "#######", ".#####.", ".##.##.", "#.....#"];
pub const SHELL: &[&str] = &["..###..", ".#o#o#.", "#o#o#o#", "#######", ".#.#.#.", "..###.."];
pub const CRAB: &[&str] = &[
    "#.......#",
    "##.....##",
    ".#.o.o.#.",
    ".#######.",
    "#########",
    ".#.#.#.#.",
];

pub fn data_uri_png(png: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

/// Background gradient plus a faint 4px dither, shared by every card.
pub fn background(out: &mut String, w: f32, h: f32, top: &str, bottom: &str) {
    let _ = write!(
        out,
        r##"<defs>
<linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient>
<pattern id="dither" width="8" height="8" patternUnits="userSpaceOnUse"><rect width="4" height="4" fill="#ffffff" fill-opacity="0.025"/><rect x="4" y="4" width="4" height="4" fill="#ffffff" fill-opacity="0.025"/></pattern>
</defs>
<rect width="{w}" height="{h}" fill="url(#bg)"/><rect width="{w}" height="{h}" fill="url(#dither)"/>"##
    );
    // A few fixed sparkles; fixed so cards are deterministic and cacheable.
    for (sx, sy) in [
        (0.06, 0.12),
        (0.93, 0.08),
        (0.88, 0.9),
        (0.4, 0.05),
        (0.7, 0.94),
        (0.03, 0.85),
    ] {
        let _ = write!(
            out,
            r##"<rect x="{}" y="{}" width="4" height="4" fill="#ffffff" fill-opacity="0.35"/>"##,
            (w * sx).round(),
            (h * sy).round()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formats() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(thousands(-1000), "-1,000");
        assert_eq!(compact(999), "999");
        assert_eq!(compact(12_500), "12.5K");
        assert_eq!(compact(3_400_000), "3.4M");
    }

    #[test]
    fn colours_mix() {
        assert_eq!(mix("#000000", "#ffffff", 0.5), "#808080");
        assert_eq!(mix("#ff6b4a", "#0f1631", 0.0), "#ff6b4a");
        assert_eq!(mix("#ff6b4a", "#0f1631", 1.0), "#0f1631");
        assert_eq!(mix("nonsense", "#ffffff", 1.0), "#ffffff");
    }

    #[test]
    fn text_widths_follow_the_fonts() {
        // Read off the renderer: four `w` in Noto Sans at 100 px ink 312 px; the pixel font's
        // wide glyphs are seven font pixels, at 16 px two pixels each.
        assert!((sans_width("wwww", 100.0) - 330.8).abs() < 1.0);
        assert_eq!(pixel_width("NN", 16.0), 28.0);
        assert_eq!(pixel_width("I", 8.0), 3.0);
        assert!(pixel_width("MARLOW", 14.0) > pixel_width("MARLOW", 8.0));
        let name = "Blushbun Bed (Rose)";
        assert_eq!(truncate_to(name, 13.0, 400.0), name);
        let cut = truncate_to(name, 13.0, 90.0);
        assert!(cut.ends_with('…') && sans_width(&cut, 13.0) <= 90.0, "{cut}");
        assert!(cut.len() > 6, "it keeps what fits: {cut}");
    }

    #[test]
    fn text_helpers() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
        assert_eq!(escape("<a&'b'>"), "&lt;a&amp;&apos;b&apos;&gt;");
        let lines = wrap("one two three four five six seven", 9, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'));
    }
}
