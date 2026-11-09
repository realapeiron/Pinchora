//! The cards of the collection games: the Tide Pool Dex, a critter drop, a bottle washing ashore,
//! a haul (one find or a whole crab pot) and the crab pot itself.

use std::fmt::Write;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::text;
use crate::{
    Theme,
    svg::{
        STAR, background, data_uri_png, escape, fit_laid_out, laid_out_width, mix, notched_path, panel, pixel_shadow,
        pixel_width, segment_bar, sprite, truncate_to, wrap, wrap_laid_out,
    },
};

/// A critter or find as a picture: already scaled up with nearest-neighbour.
#[derive(Clone)]
pub struct EntryPicture {
    pub png: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

/// Draws `art` standing on `bottom`, centred on `cx`.
fn stand(out: &mut String, art: &EntryPicture, cx: f32, bottom: f32, filter: Option<&str>) {
    let filter = filter.map_or(String::new(), |f| format!(r#" filter="url(#{f})""#));
    let _ = write!(
        out,
        r#"<image x="{}" y="{}" width="{}" height="{}"{filter} href="{}"/>"#,
        (cx - art.w as f32 / 2.0).round(),
        (bottom - art.h as f32).round(),
        art.w,
        art.h,
        data_uri_png(&art.png)
    );
}

/// The dark silhouette a creature not found yet is drawn as.
const SILHOUETTE: &str = r#"<defs><filter id="unseen" x="0" y="0" width="1" height="1" color-interpolation-filters="sRGB"><feColorMatrix type="matrix" values="0.14 0.14 0.14 0 0.02  0.14 0.14 0.18 0 0.03  0.18 0.18 0.26 0 0.07  0 0 0 0.95 0"/></filter></defs>"#;

/// A name that has to fit `max` px on one line: at `size`, or at the largest whole size down to `min` that
/// holds it, so a long name (Turkish ones run long) is shrunk before it is cut; only a name too long for
/// `min` ends with an ellipsis. Returns the text and the size to draw it at.
fn fit_name(name: &str, size: f32, min: f32, max: f32) -> (String, f32) {
    let width = |at: f32| laid_out_width(name, SANS, true, at);
    let measured = width(size);
    if measured <= max {
        return (name.to_owned(), size);
    }
    let mut at = (size * max / measured).floor().clamp(min, size);
    while at > min && width(at) > max {
        at -= 1.0;
    }
    if width(at) <= max {
        (name.to_owned(), at)
    } else {
        (fit_laid_out(name, SANS, true, at, max), at)
    }
}

/// A name for a small box: one line at `size` when it fits `max` px, else two lines at the largest of `size - 1`
/// to `size - 3` that holds all of it, so a long name is read whole. Returns its lines and their size.
fn name_lines(name: &str, size: f32, max: f32) -> (Vec<String>, f32) {
    if laid_out_width(name, SANS, true, size) <= max {
        return (vec![name.to_owned()], size);
    }
    let whole = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let (mut lines, mut at) = (Vec::new(), size);
    for step in 1..=3 {
        at = size - step as f32;
        lines = wrap_laid_out(name, SANS, true, at, max, 2);
        if lines.join(" ") == whole {
            break;
        }
    }
    (lines, at)
}

/// Draws a name from [`name_lines`] centred on `cx`: a single line sits on `single`, two lines on `first` and
/// 14 px below it.
fn name_text(out: &mut String, cx: f32, (single, first): (f32, f32), size: f32, lines: &[String], ink: &str) {
    for (i, line) in lines.iter().enumerate() {
        let y = if lines.len() == 1 {
            single
        } else {
            first + i as f32 * 14.0
        };
        text(out, cx, y, size, SANS, true, ink, "middle", &escape(line));
    }
}

fn hex(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0xff_ffff)
}

/// The title row every collection card shares.
fn header(s: &mut String, t: &Theme, title: &str, right: &str, right_x: f32) {
    text(
        s,
        36.0,
        62.0,
        30.0,
        PIXEL,
        false,
        &t.ink,
        "start",
        &escape(&title.to_uppercase()),
    );
    if !right.is_empty() {
        text(s, right_x, 60.0, 18.0, PIXEL, false, &t.gold, "end", &escape(right));
    }
}

fn open_svg(w: f32, h: f32) -> String {
    let mut s = String::with_capacity(96 * 1024);
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
    );
    s
}

/// A pixel picture from a grid of palette letters; `.` is empty.
fn pixels(out: &mut String, rows: &[&str], x: f32, y: f32, scale: f32, palette: &[(char, &str)]) {
    for (r, row) in rows.iter().enumerate() {
        for (c, ch) in row.chars().enumerate() {
            let Some((_, fill)) = palette.iter().find(|(key, _)| *key == ch) else {
                continue;
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

mod drop;
pub use drop::*;

mod dex;
pub use dex::*;

mod haul;
#[cfg(test)]
use haul::subtitle_plate;
pub use haul::*;

mod pot;
pub use pot::*;

#[cfg(test)]
#[path = "collection/tests.rs"]
mod tests;
