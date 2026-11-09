//! The party game cards: the hermit crab derby lobby, share or grab, the shell game, the treasure heist
//! and the storm surge.

use std::fmt::Write;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::SpriteArt;
use crate::cards::text;
use crate::{
    Theme,
    svg::{
        CRAB, HEART, SHELL, STAR, background, compact_in, data_uri_png, escape, mix, notched_path, panel, pixel_width,
        segment_bar, sprite, thousands_in, truncate_to, wrap,
    },
};

fn open(w: f32, h: f32) -> String {
    let mut s = String::with_capacity(40 * 1024);
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
    );
    s
}

/// A small plate with a pixel-font label, whose right edge is at `right`.
fn chip(s: &mut String, right: f32, y: f32, label: &str, color: &str, theme: &Theme) {
    let w = pixel_width(label, 16.0) + 24.0;
    panel(s, right - w, y, w, 32.0, 4.0, &theme.panel, color);
    text(s, right - w / 2.0, y + 23.0, 16.0, PIXEL, false, color, "middle", label);
}

/// A sprite from art already scaled to the size it is drawn at.
fn art(s: &mut String, art: &SpriteArt<'_>, x: f32, y: f32) {
    let _ = write!(
        s,
        r#"<image x="{x}" y="{y}" width="{}" height="{}" image-rendering="optimizeSpeed" href="{}"/>"#,
        art.w,
        art.h,
        data_uri_png(art.png)
    );
}

fn hex(color: u32) -> String {
    format!("#{:06x}", color & 0xff_ffff)
}

mod derby;
pub use derby::*;

mod grab;
pub use grab::*;

mod shell;
pub use shell::*;

mod heist;
pub use heist::*;

mod surge;
pub use surge::*;

#[cfg(test)]
#[path = "party/tests.rs"]
mod tests;
