//! The cards of the party games: lobbies, the Hot Pearl table, standings, acronym letters, anonymous
//! entries and their reveal, the Wordle Race boards, the postcard, Would You Rather and Truth or Dare.

use std::fmt::Write;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::text;
use crate::{
    Theme,
    svg::{
        CRAB, HEART, SHELL, STAR, background_plain, data_uri_png, escape, mix, panel, pixel_width, sprite, truncate_to,
        wrap, wrap_laid_out,
    },
};

const WIDTH: f32 = 900.0;

fn open(w: f32, h: f32) -> String {
    let mut s = String::with_capacity(24 * 1024);
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
    );
    s
}

/// The shared gradient and dither, with sparkles only in the margins. The shared sparkles sit at fixed
/// fractions of the card, and on these cards that puts a white speck on a heading or the end of a footer.
fn background(out: &mut String, w: f32, h: f32, t: &Theme) {
    background_plain(out, w, h, &t.bg_top, &t.bg_bottom);
    for (x, y) in [
        (10.0, 0.14 * h),
        (w - 16.0, 0.09 * h),
        (w - 20.0, 0.88 * h),
        (0.4 * w, 8.0),
        (14.0, 0.82 * h),
        (0.72 * w, h - 10.0),
    ] {
        let _ = write!(
            out,
            r##"<rect x="{}" y="{}" width="4" height="4" fill="#ffffff" fill-opacity="0.35"/>"##,
            x.round(),
            y.round()
        );
    }
}

fn heading(s: &mut String, t: &Theme, lang: Lang, title: &str, right: &str) {
    text(
        s,
        36.0,
        62.0,
        30.0,
        PIXEL,
        false,
        &t.ink,
        "start",
        &escape(&lang.upper(title)),
    );
    if !right.is_empty() {
        let chip = pixel_width(right, 16.0) + 28.0;
        panel(s, WIDTH - 36.0 - chip, 30.0, chip, 36.0, 4.0, &t.panel, &t.gold);
        text(
            s,
            WIDTH - 36.0 - chip / 2.0,
            55.0,
            16.0,
            PIXEL,
            false,
            &t.gold,
            "middle",
            &escape(&lang.upper(right)),
        );
    }
}

fn footer(s: &mut String, t: &Theme, lang: Lang, y: f32, line: &str) {
    text(
        s,
        WIDTH / 2.0,
        y,
        15.0,
        PIXEL,
        false,
        &t.muted,
        "middle",
        &escape(&lang.upper(line)),
    );
}

/// A filled disc in pixel steps: rows of `step` pixels, each as wide as the disc is there.
fn pixel_disc(out: &mut String, cx: f32, cy: f32, r: f32, step: f32, fill: &str) {
    let rows = (2.0 * r / step).round() as i32;
    for k in 0..rows {
        let y = -r + (k as f32 + 0.5) * step;
        let half = ((r * r - y * y).max(0.0).sqrt() / step).round() * step;
        if half <= 0.0 {
            continue;
        }
        let _ = write!(
            out,
            r#"<rect x="{}" y="{}" width="{}" height="{step}" fill="{fill}"/>"#,
            cx - half,
            cy - r + k as f32 * step,
            half * 2.0
        );
    }
}

fn rect(out: &mut String, x: f32, y: f32, w: f32, h: f32, fill: &str) {
    let _ = write!(out, r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}"/>"#);
}

/// Hearts for lives left, dim ones for lives lost, right-aligned to `x_end`.
fn hearts(out: &mut String, t: &Theme, x_end: f32, y: f32, left: i32, total: i32) {
    for k in 0..total {
        let x = x_end - (total - k) as f32 * 20.0;
        if k < left {
            sprite(out, HEART, x, y, 2.4, &t.heart, "#ffd0da");
        } else {
            sprite(out, HEART, x, y, 2.4, "#2e3d75", "#3b4676");
        }
    }
}

mod lobby;
use lobby::truncate_pixel;
pub use lobby::*;

mod entries;
pub use entries::*;

mod prompts;
pub use prompts::*;

#[cfg(test)]
#[path = "party_games/tests.rs"]
mod tests;
