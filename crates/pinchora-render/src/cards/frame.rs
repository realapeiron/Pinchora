//! The pixel frames a player can put around their cards. Each is a few rectangles per edge cell,
//! drawn in SVG over the finished card, so one routine fits every card size.

use std::fmt::Write;

use crate::{
    Frame,
    svg::{mix, notched_path},
};

/// How thick every frame is; the cards keep this much clear at their edges.
pub const BAND: f32 = 8.0;
/// The corner pieces are a little bigger than the band.
const CORNER: f32 = 12.0;

/// Draws `frame` around a card of `w`×`h`. `accent` colours the neon one.
pub(crate) fn draw_frame(out: &mut String, frame: Frame, w: f32, h: f32, accent: &str) {
    match frame {
        Frame::Plain => {}
        Frame::Rope => rope(out, w, h),
        Frame::Coral => coral(out, w, h),
        Frame::Pearl => pearl(out, w, h),
        Frame::Gold => gold(out, w, h),
        Frame::Neon => neon(out, w, h, accent),
    }
}

#[derive(Clone, Copy)]
enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

/// One side of the card, in a frame of reference of its own: `u` runs along the side from the
/// first corner, `v` runs inwards from the card's edge.
struct Edge {
    side: Side,
    w: f32,
    h: f32,
}

impl Edge {
    /// Draws the rectangle `u..u+du`, `v..v+dv` of this side.
    fn rect(&self, out: &mut String, u: f32, v: f32, du: f32, dv: f32, fill: &str) {
        let (x, y, w, h) = match self.side {
            Side::Top => (BAND + u, v, du, dv),
            Side::Bottom => (BAND + u, self.h - v - dv, du, dv),
            Side::Left => (v, BAND + u, dv, du),
            Side::Right => (self.w - v - dv, BAND + u, dv, du),
        };
        let _ = write!(out, r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}"/>"#);
    }
}

/// Runs `cell(edge, index, u, length)` for every cell of `unit` along all four sides, between
/// the corners. The cells are whole and sit on the 2 px grid, centred on their side, so no pixel
/// is blended; the corner pieces cover the few pixels left over at either end.
fn along(w: f32, h: f32, unit: f32, mut cell: impl FnMut(&Edge, usize, f32, f32)) {
    for (n, side) in [Side::Top, Side::Right, Side::Bottom, Side::Left]
        .into_iter()
        .enumerate()
    {
        let span = match side {
            Side::Top | Side::Bottom => w,
            Side::Left | Side::Right => h,
        } - 2.0 * BAND;
        let count = (span / unit).floor().max(1.0) as usize;
        let start = ((span - count as f32 * unit) / 4.0).floor() * 2.0;
        let edge = Edge { side, w, h };
        for i in 0..count {
            // The index goes on counting round the card, so a pattern does not repeat per side.
            cell(&edge, n * 1000 + i, start + i as f32 * unit, unit);
        }
    }
}

/// The card's four corners, `size` square.
fn corners(w: f32, h: f32, size: f32) -> [(f32, f32); 4] {
    [(0.0, 0.0), (w - size, 0.0), (0.0, h - size), (w - size, h - size)]
}

fn rect(out: &mut String, x: f32, y: f32, w: f32, h: f32, fill: &str) {
    let _ = write!(out, r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}"/>"#);
}

/// A ring between two insets from the card's edge. `notch` rounds the inner corners one pixel
/// step; the outer ones stay square, like the card.
fn ring(out: &mut String, w: f32, h: f32, outer: f32, inner: f32, notch: f32, fill: &str, opacity: f32) {
    let _ = write!(
        out,
        r#"<path d="{} {}" fill="{fill}" fill-opacity="{opacity}" fill-rule="evenodd"/>"#,
        notched_path(outer, outer, w - 2.0 * outer, h - 2.0 * outer, 0.0),
        notched_path(inner, inner, w - 2.0 * inner, h - 2.0 * inner, notch),
    );
}

/// A cheap, stable shuffle of `n`, for patterns that should look uneven but never change.
fn scatter(n: usize) -> u32 {
    let mut x = (n as u32).wrapping_add(0x9e37_79b9).wrapping_mul(0x85eb_ca6b);
    x ^= x >> 13;
    x = x.wrapping_mul(0xc2b2_ae35);
    x ^ (x >> 16)
}

/// Twisted hemp: slanted strands on a dark core, with a knot in every corner.
fn rope(out: &mut String, w: f32, h: f32) {
    ring(out, w, h, 0.0, BAND, 0.0, "#4a2f17", 1.0);
    along(w, h, 8.0, |edge, _, u, _| {
        for step in 0..4 {
            let s = step as f32 * 2.0;
            edge.rect(out, u + s, s, 4.0, 2.0, "#d9b077");
            edge.rect(out, u + s, s, 2.0, 2.0, "#f2d9a6");
        }
    });
    for (x, y) in corners(w, h, CORNER) {
        rect(out, x, y, CORNER, CORNER, "#3d2610");
        rect(out, x + 2.0, y + 2.0, CORNER - 4.0, CORNER - 4.0, "#b98a4e");
        rect(out, x + 4.0, y + 4.0, CORNER - 8.0, CORNER - 8.0, "#e3c08a");
        rect(out, x + 4.0, y + 4.0, 2.0, 2.0, "#fff1e8");
    }
}

/// A reef: two tones of coral with a skyline of branches growing inwards.
fn coral(out: &mut String, w: f32, h: f32) {
    ring(out, w, h, 0.0, 4.0, 0.0, "#c2413f", 1.0);
    ring(out, w, h, 4.0, BAND, 0.0, "#ff6b4a", 1.0);
    along(w, h, 8.0, |edge, index, u, length| {
        let roll = scatter(index);
        // Fingers of coral rise from most cells, short or tall; a few cells stay bare.
        let height = match roll % 8 {
            0 => 0.0,
            1..=3 => 2.0,
            _ => 4.0,
        };
        if height > 0.0 {
            let left = u + 1.0;
            edge.rect(out, left, BAND, length - 2.0, height, "#ff6b4a");
            edge.rect(out, left, BAND + height - 2.0, length - 2.0, 2.0, "#ff9e7a");
            edge.rect(out, left, BAND + height - 2.0, 2.0, 2.0, "#ffd2c0");
        }
        if roll >> 8 & 3 == 0 {
            // A pale polyp on the outer row.
            edge.rect(out, u + 3.0, 0.0, 2.0, 2.0, "#ffd2c0");
        }
    });
    for (x, y) in corners(w, h, CORNER) {
        rect(out, x, y, CORNER, CORNER, "#c2413f");
        rect(out, x + 2.0, y + 2.0, CORNER - 4.0, CORNER - 4.0, "#ff6b4a");
        rect(out, x + 4.0, y + 4.0, 4.0, 4.0, "#ff9e7a");
        rect(out, x + 4.0, y + 4.0, 2.0, 2.0, "#ffd2c0");
    }
}

/// A string of pearls on a dusky band.
fn pearl(out: &mut String, w: f32, h: f32) {
    ring(out, w, h, 0.0, BAND, 0.0, "#4b4478", 1.0);
    along(w, h, 12.0, |edge, _, u, length| {
        // An eight-pixel pearl in the middle of its cell, corners cut by a pixel.
        let at = u + (length - 8.0) / 2.0;
        edge.rect(out, at + 1.0, 0.0, 6.0, 8.0, "#f4eefc");
        edge.rect(out, at, 1.0, 8.0, 6.0, "#f4eefc");
        edge.rect(out, at + 4.0, 5.0, 3.0, 2.0, "#c9bfe3");
        edge.rect(out, at + 6.0, 3.0, 2.0, 3.0, "#c9bfe3");
        edge.rect(out, at + 2.0, 1.0, 2.0, 2.0, "#ffffff");
    });
    for (x, y) in corners(w, h, CORNER) {
        rect(out, x + 2.0, y, CORNER - 4.0, CORNER, "#f4eefc");
        rect(out, x, y + 2.0, CORNER, CORNER - 4.0, "#f4eefc");
        rect(out, x + 6.0, y + 8.0, 4.0, 2.0, "#c9bfe3");
        rect(out, x + 8.0, y + 6.0, 2.0, 4.0, "#c9bfe3");
        rect(out, x + 2.0, y + 2.0, 4.0, 4.0, "#ffffff");
    }
}

/// A bevelled gold moulding with an engraved line and a stud in every corner.
fn gold(out: &mut String, w: f32, h: f32) {
    ring(out, w, h, 0.0, 2.0, 0.0, "#8a5a14", 1.0);
    ring(out, w, h, 2.0, 6.0, 0.0, "#ffd166", 1.0);
    ring(out, w, h, 6.0, BAND, 0.0, "#c98a1e", 1.0);
    along(w, h, 16.0, |edge, _, u, length| {
        edge.rect(out, u + length / 2.0 - 4.0, 3.0, 8.0, 2.0, "#fff0b8");
        edge.rect(out, u, 3.0, 2.0, 2.0, "#e0ac6e");
    });
    for (x, y) in corners(w, h, CORNER) {
        rect(out, x, y, CORNER, CORNER, "#8a5a14");
        rect(out, x + 2.0, y + 2.0, CORNER - 4.0, CORNER - 4.0, "#ffd166");
        rect(out, x + CORNER - 4.0, y + 2.0, 2.0, CORNER - 4.0, "#c98a1e");
        rect(out, x + 2.0, y + CORNER - 4.0, CORNER - 4.0, 2.0, "#c98a1e");
        rect(out, x + 4.0, y + 4.0, 2.0, 2.0, "#fff0b8");
    }
}

/// A glowing tube in the accent colour: a bright line with a haze either side, in a dark channel
/// so it still glows on a bright background picture.
fn neon(out: &mut String, w: f32, h: f32, accent: &str) {
    ring(out, w, h, 0.0, BAND, 0.0, "#0b1029", 0.75);
    ring(out, w, h, 0.0, 2.0, 0.0, accent, 0.34);
    ring(out, w, h, 2.0, 6.0, 4.0, accent, 1.0);
    ring(out, w, h, 3.0, 5.0, 2.0, &mix(accent, "#ffffff", 0.7), 0.95);
    ring(out, w, h, 6.0, BAND, 4.0, accent, 0.3);
    ring(out, w, h, BAND, BAND + 4.0, 4.0, accent, 0.12);
}

#[cfg(test)]
mod tests {
    use crate::cards::frame::*;

    fn svg(frame: Frame, w: f32, h: f32) -> String {
        let mut s = String::new();
        draw_frame(&mut s, frame, w, h, "#ff6b4a");
        s
    }

    #[test]
    fn plain_draws_nothing_and_the_rest_do() {
        assert_eq!(svg(Frame::Plain, 960.0, 540.0), "");
        for frame in &Frame::ALL[1..] {
            let s = svg(*frame, 960.0, 540.0);
            assert!(s.len() > 200, "{frame:?} draws something");
            assert!(!s.contains("NaN") && !s.contains("inf"), "{frame:?}");
        }
    }

    #[test]
    fn a_frame_stays_inside_the_card() {
        // Every rectangle lies inside the card, for the sizes the cards have.
        for (w, h) in [(960.0, 540.0), (934.0, 282.0), (900.0, 300.0)] {
            for frame in &Frame::ALL[1..] {
                let s = svg(*frame, w, h);
                for tag in s.split("<rect ").skip(1) {
                    let num = |key: &str| -> f32 {
                        let start = tag.find(&format!("{key}=\"")).expect(key) + key.len() + 2;
                        tag[start..]
                            .split('"')
                            .next()
                            .expect("value")
                            .parse()
                            .expect("a number")
                    };
                    let (x, y, rw, rh) = (num("x"), num("y"), num("width"), num("height"));
                    // A rope strand's last wrap may spill 2 px under a corner piece.
                    assert!(
                        x >= -0.01 && y >= -0.01 && x + rw <= w + 2.01 && y + rh <= h + 2.01,
                        "{frame:?} on {w}x{h}: {tag}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_neon_frame_takes_the_accent() {
        let mut s = String::new();
        draw_frame(&mut s, Frame::Neon, 960.0, 540.0, "#06d6a0");
        assert!(s.contains("#06d6a0") && !s.contains("#ff6b4a"));
        assert!(
            !svg(Frame::Gold, 960.0, 540.0).contains("#ff6b4a"),
            "the others keep their colours"
        );
    }
}
