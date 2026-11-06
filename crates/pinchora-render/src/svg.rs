//! Small SVG building blocks for the pixel-art card style.

use std::{borrow::Cow, fmt::Write};

use base64::Engine;
use resvg::usvg;
use unicode_segmentation::UnicodeSegmentation;

/// Whether a card may carry `c` in its text. XML 1.0 forbids the control characters (tab and line breaks are
/// dropped too: a card is one line) and U+FFFE and U+FFFF, and a parser may refuse the other noncharacters
/// (U+FDD0 to U+FDEF and the last two code points of every plane) as well; a name holding one must not stop a
/// card from drawing.
fn is_card_char(c: char) -> bool {
    let n = c as u32;
    !c.is_control() && !(0xFDD0..=0xFDEF).contains(&n) && n & 0xFFFE != 0xFFFE
}

/// `svg` without the characters an XML document cannot hold (see [`is_card_char`]; tabs and line breaks stay),
/// so text that reached the markup unescaped cannot stop a card from drawing either.
pub(crate) fn scrub(svg: &str) -> Cow<'_, str> {
    let fine = |c: char| matches!(c, '\t' | '\n' | '\r') || is_card_char(c);
    if svg.chars().all(fine) {
        Cow::Borrowed(svg)
    } else {
        Cow::Owned(svg.chars().filter(|&c| fine(c)).collect())
    }
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if !is_card_char(c) => {}
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
            // Emoji draw in Noto Emoji, 1.27 em across; the wide scripts take an em.
            c if c as u32 >= 0x1f000 || (0x2600..0x2c00).contains(&(c as u32)) => 1.27,
            c if c as u32 >= 0x2e80 || (0x2190..0x2c00).contains(&(c as u32)) => 1.0,
            _ => 0.6,
        })
        .sum();
    em * size
}

/// How wide `text` is in the pixel font at `size` px. Plain ASCII and the Turkish letters are exact from a table:
/// every glyph is a whole number of font pixels, eight to the em. Anything else (accents, other scripts, emoji)
/// is laid out for real, so font fallback and the empty box the pixel font draws for what it lacks are counted as
/// the renderer draws them.
pub fn pixel_width(text: &str, size: f32) -> f32 {
    if !text.chars().all(|c| c.is_ascii() || turkish_pixels(c).is_some()) {
        return laid_out_width(text, crate::cards::PIXEL, false, size);
    }
    let pixels: u32 = text
        .chars()
        .map(|c| {
            turkish_pixels(c).unwrap_or(match c {
                'I' | 'i' | '.' | '!' | '\'' | ' ' | ':' | ';' | '|' => 3,
                ',' | '(' | ')' => 4,
                'E' | 'F' | 'L' | 'T' | 'Z' | 'e' | 'f' | 'l' | 't' | 'z' | '-' | '1' => 5,
                'M' | 'N' | 'V' | 'W' | 'X' | 'Y' | 'm' | 'n' | 'v' | 'w' | 'x' | 'y' => 7,
                _ => 6,
            })
        })
        .sum();
    pixels as f32 * size / 8.0
}

/// How many font pixels the pixel font advances for a Turkish letter: `İ` and `ı` are an `I` wide, the others
/// a letter wide. The font draws all of them itself (`İ ı Ş ş Ğ ğ` were added to Silkscreen for these cards).
const fn turkish_pixels(c: char) -> Option<u32> {
    match c {
        'İ' | 'ı' => Some(3),
        'Ş' | 'ş' | 'Ğ' | 'ğ' | 'Ç' | 'ç' | 'Ö' | 'ö' | 'Ü' | 'ü' | 'Â' | 'â' => Some(6),
        _ => None,
    }
}

/// How wide the renderer draws `text` in `family` at `size` px, laid out with the card fonts.
/// A text it cannot lay out counts as a full em per character, which is wider than anything drawn.
pub fn laid_out_width(text: &str, family: &str, bold: bool, size: f32) -> f32 {
    let weight = if bold { "bold" } else { "normal" };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><text font-family="{family}" font-size="{size}" font-weight="{weight}">{}</text></svg>"#,
        escape(text)
    );
    let options = usvg::Options {
        fontdb: crate::fonts().clone(),
        font_family: crate::cards::SANS.into(),
        ..Default::default()
    };
    match usvg::Tree::from_str(&svg, &options) {
        Ok(tree) => tree.root().abs_bounding_box().width(),
        Err(_) => text.graphemes(true).count() as f32 * size,
    }
}

/// Cuts `text` to what the renderer draws in at most `max` px in `family` at `size` px, ending
/// with an ellipsis when shortened.
pub fn fit_laid_out(text: &str, family: &str, bold: bool, size: f32, max: f32) -> String {
    fit_by(text, max, |t| laid_out_width(t, family, bold, size))
}

/// Cuts `text` to what fits `max` px in the pixel font at `size` px, ending with an ellipsis when
/// shortened.
pub fn truncate_pixel_to(text: &str, size: f32, max: f32) -> String {
    fit_by(text, max, |t| pixel_width(t, size))
}

/// The longest prefix of `text`, whole graphemes, that `width` measures within `max` once an
/// ellipsis follows it; `text` itself when it fits. Found by halving, so a long name is measured
/// a handful of times.
fn fit_by(text: &str, max: f32, width: impl Fn(&str) -> f32) -> String {
    if width(text) <= max {
        return text.to_owned();
    }
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    let cut = |n: usize| format!("{}…", graphemes[..n].concat());
    let (mut fits, mut over) = (0, graphemes.len());
    while over - fits > 1 {
        let mid = (fits + over) / 2;
        if width(&cut(mid)) <= max {
            fits = mid;
        } else {
            over = mid;
        }
    }
    cut(fits)
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

/// Greedy word wrap by drawn width: lines of at most `max` px in `family` at `size` px, bold or not, so a
/// wide box is filled whatever the letters are. When text is left over after `max_lines` lines, the last
/// one ends with an ellipsis.
pub fn wrap_laid_out(text: &str, family: &str, bold: bool, size: f32, max: f32, max_lines: usize) -> Vec<String> {
    let width = |t: &str| laid_out_width(t, family, bold, size);
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut lines: Vec<String> = Vec::new();
    // Where in `words` each line starts, to put the rest of the text on the last line when it is cut.
    let mut starts: Vec<usize> = vec![0];
    let mut line = String::new();
    for (i, word) in words.iter().enumerate() {
        if line.is_empty() {
            line = (*word).to_owned();
            continue;
        }
        let candidate = format!("{line} {word}");
        if width(&candidate) <= max {
            line = candidate;
        } else {
            lines.push(std::mem::replace(&mut line, (*word).to_owned()));
            starts.push(i);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    let max_lines = max_lines.max(1);
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        lines[max_lines - 1] = words[starts[max_lines - 1]..].join(" ");
    }
    // A word wider than the box on its own, or the rest of a long text, is cut with an ellipsis.
    lines.iter().map(|l| fit_by(l, max, width)).collect()
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
    // One decimal, and none when it would be a zero: `49K`, not `49.0K`.
    let scaled = |value: f64, unit: &str| {
        let text = format!("{value:.1}");
        format!("{sign}{}{unit}", text.strip_suffix(".0").unwrap_or(&text))
    };
    match a {
        a if a >= 1e9 => scaled(a / 1e9, "B"),
        a if a >= 1e6 => scaled(a / 1e6, "M"),
        a if a >= 1e4 => scaled(a / 1e3, "K"),
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

/// A shadow in pixel-art steps: an ellipse of half-sizes `rx` and `ry` centred on `(cx, cy)`,
/// cut into bands two pixels tall, each as wide as the ellipse is there, to a whole two pixels.
/// Nothing is anti-aliased, so it stays as crisp as the sprite on it.
pub fn pixel_shadow(out: &mut String, cx: f32, cy: f32, rx: f32, ry: f32, alpha: f32) {
    let bands = ry.round().max(1.0) as i32;
    let top = cy - bands as f32;
    let mut path = String::new();
    for k in 0..bands {
        // Where the middle of this band lies, from -1 at the top of the shadow to 1 at the bottom.
        let along = (2 * k + 1 - bands) as f32 / bands as f32;
        let half = ((rx * (1.0 - along * along).sqrt() / 2.0).round() * 2.0).max(2.0);
        let _ = write!(
            path,
            "M{},{} h{} v2 h-{} Z ",
            cx - half,
            top + 2.0 * k as f32,
            half * 2.0,
            half * 2.0
        );
    }
    let _ = write!(
        out,
        r##"<path d="{}" fill="#000000" fill-opacity="{alpha}"/>"##,
        path.trim_end()
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

/// Background gradient plus a faint 4px dither, shared by every card, with a few sparkles on top.
pub fn background(out: &mut String, w: f32, h: f32, top: &str, bottom: &str) {
    background_plain(out, w, h, top, bottom);
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

/// The background without the sparkles, for cards whose text would meet them.
pub fn background_plain(out: &mut String, w: f32, h: f32, top: &str, bottom: &str) {
    let _ = write!(
        out,
        r##"<defs>
<linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient>
<pattern id="dither" width="8" height="8" patternUnits="userSpaceOnUse"><rect width="4" height="4" fill="#ffffff" fill-opacity="0.025"/><rect x="4" y="4" width="4" height="4" fill="#ffffff" fill-opacity="0.025"/></pattern>
</defs>
<rect width="{w}" height="{h}" fill="url(#bg)"/><rect width="{w}" height="{h}" fill="url(#dither)"/>"##
    );
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
        assert_eq!(compact(49_000), "49K");
        assert_eq!(compact(2_000_000), "2M");
        assert_eq!(compact(-12_000), "-12K");
        assert_eq!(compact(10_049), "10K");
    }

    #[test]
    fn the_plain_background_has_no_sparkles() {
        let sparkle = r##"width="4" height="4" fill="#ffffff" fill-opacity="0.35""##;
        let (mut with, mut without) = (String::new(), String::new());
        background(&mut with, 500.0, 400.0, "#000000", "#111111");
        background_plain(&mut without, 500.0, 400.0, "#000000", "#111111");
        assert_eq!(with.matches(sparkle).count(), 6);
        assert_eq!(without.matches(sparkle).count(), 0);
        assert!(with.starts_with(&without), "the sparkles are the only difference");
    }

    #[test]
    fn wrapping_by_width_fills_the_box() {
        let text = "Describe your perfect cozy night in four sentences, and make every one of them rhyme";
        let lines = wrap_laid_out(text, crate::cards::SANS, true, 30.0, 780.0, 4);
        assert!((2..=3).contains(&lines.len()), "{lines:?}");
        assert_eq!(lines.join(" "), text, "nothing is lost when it fits");
        for line in &lines {
            assert!(laid_out_width(line, crate::cards::SANS, true, 30.0) <= 780.0, "{line}");
        }
        // Every line but the last is nearly full, where a count of characters leaves the right side empty.
        for line in &lines[..lines.len() - 1] {
            assert!(laid_out_width(line, crate::cards::SANS, true, 30.0) > 600.0, "{line}");
        }
        // Too much text ends in an ellipsis on the last allowed line, and one long word is cut to fit.
        let cut = wrap_laid_out(&"word ".repeat(80), crate::cards::SANS, true, 30.0, 780.0, 2);
        assert_eq!(cut.len(), 2);
        assert!(cut[1].ends_with('…'), "{cut:?}");
        let long = wrap_laid_out(&"w".repeat(80), crate::cards::SANS, true, 30.0, 780.0, 2);
        assert_eq!(long.len(), 1);
        assert!(long[0].ends_with('…') && laid_out_width(&long[0], crate::cards::SANS, true, 30.0) <= 780.0);
        assert!(wrap_laid_out("   ", crate::cards::SANS, true, 30.0, 780.0, 2).is_empty());
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
        // The fallback fonts: an em for a Chinese letter, 1.27 for an emoji.
        assert!((sans_width("李", 100.0) - 100.0).abs() < 1.0);
        assert!((sans_width("😀✨", 100.0) - 254.0).abs() < 1.0);
        assert_eq!(pixel_width("NN", 16.0), 28.0);
        assert_eq!(pixel_width("I", 8.0), 3.0);
        assert!(pixel_width("MARLOW", 14.0) > pixel_width("MARLOW", 8.0));
        let name = "Blushbun Bed (Rose)";
        assert_eq!(truncate_to(name, 13.0, 400.0), name);
        let cut = truncate_to(name, 13.0, 90.0);
        assert!(cut.ends_with('…') && sans_width(&cut, 13.0) <= 90.0, "{cut}");
        assert!(cut.len() > 6, "it keeps what fits: {cut}");
    }

    /// The code points XML 1.0 cannot hold and the noncharacters a parser may refuse.
    const INVALID: &str = "\u{FFFE}\u{FFFF}\u{1FFFE}\u{1FFFF}\u{10FFFE}\u{10FFFF}\u{FDD0}\u{FDEF}\u{0}\u{1}\u{8}\u{B}\u{C}\u{1F}\u{7F}\u{85}";

    #[test]
    fn code_points_xml_cannot_hold_are_dropped_from_text_and_the_rest_is_kept() {
        assert_eq!(escape(&format!("a{INVALID}b")), "ab");
        // Tabs and line breaks never reach a card's one-line text either.
        assert_eq!(escape("a\tb\nc\rd"), "abcd");
        // What XML allows stays: letters, emoji, CJK, the replacement character, private use, the
        // first and last valid code points around the noncharacters.
        let fine = "Zoë ✨ 李 😀 \u{FFFD}\u{E000}\u{FFFC}\u{10000}\u{10FFFD}\u{FDCF}\u{FDF0}";
        assert_eq!(escape(fine), fine);
        assert_eq!(escape("<&>\"'"), "&lt;&amp;&gt;&quot;&apos;");
        // Every one of them, alone and in a name, leaves text the XML parser takes.
        for c in INVALID.chars() {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><text>x{}y</text></svg>"#,
                escape(&c.to_string())
            );
            assert!(
                usvg::Tree::from_str(&svg, &usvg::Options::default()).is_ok(),
                "U+{:04X}",
                c as u32
            );
        }
        // The check is the XML one: without escaping, the parser refuses them.
        let raw =
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><text>{INVALID}</text></svg>"#);
        assert!(usvg::Tree::from_str(&raw, &usvg::Options::default()).is_err());
    }

    #[test]
    fn a_document_is_scrubbed_of_them_before_it_is_drawn() {
        let clean = "<svg>\n\t<g/>\r\n</svg>";
        assert!(matches!(scrub(clean), Cow::Borrowed(_)), "clean text is not copied");
        assert_eq!(scrub(&format!("<svg>\n{INVALID}<g/></svg>")), "<svg>\n<g/></svg>");
        // A card that let such text past `escape` still draws.
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><text x="2" y="14" font-size="12" id="{INVALID}">A{INVALID}B</text></svg>"#
        );
        assert!(crate::Renderer::new().svg_to_png(&svg).is_ok());
    }

    #[test]
    fn emoji_and_cjk_come_from_the_fallback_fonts() {
        // A character no font has draws as the same empty box every time; one a fallback font has draws as
        // its own picture, in the pixel font and in Noto Sans alike.
        let r = crate::Renderer::new();
        for family in [crate::cards::PIXEL, crate::cards::SANS] {
            let draw = |text: &str| {
                let svg = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><rect width="96" height="96" fill="#fff"/><text x="8" y="72" font-family="{family}" font-size="64">{text}</text></svg>"##
                );
                r.svg_to_png(&svg).expect("renders")
            };
            let missing = draw("አ");
            assert_eq!(missing, draw("ጸ"), "{family}: what no font has is one box");
            let shown = ["李", "あ", "한", "😀", "✨", "❤", "🇹🇷"].map(|c| (c, draw(c)));
            for (c, png) in &shown {
                assert_ne!(*png, missing, "{family}: {c} is a box");
            }
            for (i, (a, pa)) in shown.iter().enumerate() {
                for (b, pb) in &shown[i + 1..] {
                    assert_ne!(pa, pb, "{family}: {a} and {b} look the same");
                }
            }
        }
    }

    #[test]
    fn the_turkish_letters_are_measured_from_the_table_as_the_font_draws_them() {
        let same = |a: f32, b: f32| (a - b).abs() < 0.01;
        for c in "İıŞşĞğÇçÖöÜüÂâ".chars() {
            let text = c.to_string();
            let drawn = laid_out_width(&text, crate::cards::PIXEL, false, 16.0);
            assert!(
                same(pixel_width(&text, 16.0), drawn),
                "{c}: {} vs {drawn}",
                pixel_width(&text, 16.0)
            );
        }
        // Each is as wide as the letter it is made from, so a Turkish text is as wide as its ASCII twin.
        assert!(same(pixel_width("ŞİRİN ĞIÇ", 14.0), pixel_width("SIRIN GIC", 14.0)));
        assert!(same(pixel_width("ışık", 14.0), pixel_width("isik", 14.0)));
    }

    #[test]
    fn text_is_measured_as_drawn_even_where_the_pixel_font_has_no_letters() {
        // Plain ASCII comes from the table; an accent keeps the width of its letter.
        let same = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(same(pixel_width("ZOË", 14.0), pixel_width("ZOE", 14.0)));
        assert!(same(pixel_width("ÑÑ", 8.0), pixel_width("NN", 8.0)));
        // What no font has draws as an empty box of the same width each time, a whole number of
        // font pixels wide.
        let one = pixel_width("አ", 8.0);
        assert!((5.0..=8.0).contains(&one), "{one}");
        assert!(same(pixel_width("አአአ", 8.0), 3.0 * one));
        // The wide scripts and emoji are drawn by the fallback fonts, about an em across, and measured there.
        let han = pixel_width("李", 8.0);
        assert!((6.0..=8.0).contains(&han), "{han}");
        assert!((pixel_width("李李李", 8.0) - 3.0 * han).abs() < 3.0);
        let emoji = pixel_width("😀", 14.0);
        assert!((14.0..=19.0).contains(&emoji), "{emoji}");
        // Another script falls back to the sans font and is measured there.
        assert!(pixel_width("Анна", 14.0) > 0.0);
        // Cutting follows the measure: the cut text fits, is a prefix and ends in an ellipsis,
        // whichever letters it holds; text that fits is left alone.
        for text in [
            "MMMMMMMMMMMMMMMMMMMMMM",
            "李李李李李李李李李李李李",
            "👨‍👩‍👧‍👦👨‍👩‍👧‍👦👨‍👩‍👧‍👦👨‍👩‍👧‍👦👨‍👩‍👧‍👦👨‍👩‍👧‍👦👨‍👩‍👧‍👦👨‍👩‍👧‍👦",
            "ЖЖЖЖЖЖЖЖЖЖЖЖЖЖЖ",
            "አአአአአአአአአአአአአአ",
        ] {
            let cut = truncate_pixel_to(text, 14.0, 100.0);
            assert!(cut.ends_with('…') && pixel_width(&cut, 14.0) <= 100.0, "{text}: {cut}");
            assert!(text.starts_with(cut.trim_end_matches('…')));
            let wide = fit_laid_out(text, crate::cards::SANS, true, 44.0, 200.0);
            assert!(
                wide.ends_with('…') && laid_out_width(&wide, crate::cards::SANS, true, 44.0) <= 200.0,
                "{wide}"
            );
        }
        assert_eq!(truncate_pixel_to("Marlow", 14.0, 100.0), "Marlow");
        assert_eq!(fit_laid_out("Pinch", crate::cards::SANS, true, 44.0, 400.0), "Pinch");
        assert_eq!(truncate_pixel_to("", 14.0, 10.0), "");
    }

    #[test]
    fn a_pixel_shadow_is_an_ellipse_in_whole_steps() {
        let mut s = String::new();
        pixel_shadow(&mut s, 100.0, 50.0, 20.0, 3.0, 0.3);
        let bands: Vec<(f32, f32, f32)> = s
            .trim_start_matches("<path d=\"")
            .split(" Z")
            .filter(|b| b.starts_with('M') || b.trim_start().starts_with('M'))
            .map(|b| {
                let b = b.trim().trim_start_matches('M');
                let (xy, rest) = b.split_once(" h").expect("a band");
                let (x, y) = xy.split_once(',').expect("a point");
                let width = rest.split(' ').next().expect("width");
                (x.parse().expect("x"), y.parse().expect("y"), width.parse().expect("w"))
            })
            .collect();
        assert_eq!(bands.len(), 3, "{s}");
        // Two pixels apart, centred on x = 100, even in width, the middle band the widest.
        for (i, (x, y, w)) in bands.iter().enumerate() {
            assert_eq!(*y, 47.0 + 2.0 * i as f32, "{s}");
            assert_eq!(x + w / 2.0, 100.0);
            assert_eq!(w % 4.0, 0.0, "whole two-pixel steps on either side: {w}");
        }
        assert!(bands[1].2 >= bands[0].2 && bands[1].2 >= bands[2].2, "{s}");
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
