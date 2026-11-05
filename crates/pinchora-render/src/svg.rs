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
    fn text_helpers() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
        assert_eq!(escape("<a&'b'>"), "&lt;a&amp;&apos;b&apos;&gt;");
        let lines = wrap("one two three four five six seven", 9, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'));
    }
}
