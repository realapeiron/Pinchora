//! What the chess and draughts cards share: bitmaps, player panels and the 8×8 board.

use std::fmt::Write;

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{escape, mix, notched_path, panel, truncate},
};

/// The two sides' colours (body, light): coral for the first player, sky blue for the second.
pub(super) const COLORS: [(&str, &str); 2] = [("#ff6b4a", "#ffb9a8"), ("#5bc0eb", "#bfe9fa")];

/// The ink of outlines in every sprite.
pub(super) const OUTLINE: &str = "#0b1029";

/// Draws a bitmap, one string per row, joining neighbouring cells of one colour into a single rectangle.
/// `color` says what a character looks like; `None` leaves the cell empty.
pub(super) fn bitmap(
    out: &mut String,
    rows: &[&str],
    x: f32,
    y: f32,
    scale: f32,
    color: &dyn Fn(char) -> Option<String>,
) {
    for (r, row) in rows.iter().enumerate() {
        let cells: Vec<char> = row.chars().collect();
        let mut c = 0;
        while c < cells.len() {
            let Some(fill) = color(cells[c]) else {
                c += 1;
                continue;
            };
            let start = c;
            while c < cells.len() && color(cells[c]).as_deref() == Some(fill.as_str()) {
                c += 1;
            }
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{scale}" fill="{fill}"/>"#,
                x + start as f32 * scale,
                y + r as f32 * scale,
                (c - start) as f32 * scale
            );
        }
    }
}

/// A player's panel: an icon, the name and a note under it. The side to move or the winner gets a gold edge.
pub(super) fn player_panel(
    out: &mut String,
    t: &Theme,
    (x, y): (f32, f32),
    (active, won): (bool, bool),
    name: &str,
    (note, note_color): (&str, &str),
    icon: impl FnOnce(&mut String, f32, f32),
) {
    let lit = active || won;
    let edge = if lit { &t.gold } else { &t.panel_edge };
    let fill = if lit {
        mix(&t.panel, &t.gold, 0.12)
    } else {
        t.panel.clone()
    };
    panel(out, x, y, 262.0, 76.0, 4.0, &fill, edge);
    icon(out, x + 16.0, y + 14.0);
    text(
        out,
        x + 72.0,
        y + 32.0,
        20.0,
        SANS,
        true,
        &t.ink,
        "start",
        &escape(&truncate(name, 13)),
    );
    text(out, x + 72.0, y + 60.0, 14.0, PIXEL, false, note_color, "start", note);
}

/// The status line along the bottom of a card, with the pot beside it when shells are on the table.
pub(super) fn status_panel(out: &mut String, t: &Theme, y: f32, status: &str, pot: i64) {
    panel(out, 24.0, y, 532.0, 56.0, 4.0, &t.panel, &t.panel_edge);
    let max = if pot > 0 { 26 } else { 40 };
    text(
        out,
        44.0,
        y + 35.0,
        17.0,
        PIXEL,
        false,
        &t.ink,
        "start",
        &escape(&truncate(&status.to_uppercase(), max)),
    );
    if pot > 0 {
        crate::svg::sprite(out, crate::svg::SHELL, 462.0, y + 16.0, 3.0, &t.gold, "#ffffff");
        text(
            out,
            536.0,
            y + 35.0,
            17.0,
            PIXEL,
            false,
            &t.gold,
            "end",
            &crate::svg::compact(pot),
        );
    }
}

/// How a square of the 8×8 board is marked on top of its colour.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Marks {
    /// A square the last move touched.
    pub last: bool,
    /// The square the player picked up.
    pub selected: bool,
    /// A square the picked piece can go to, and whether it takes something there.
    pub target: Option<bool>,
    /// A king in check.
    pub danger: bool,
}

pub(super) const SQUARE: f32 = 60.0;
const LIGHT_SQUARE: &str = "#3d4f8f";
const DARK_SQUARE: &str = "#243066";

/// Draws the board panel at `(x, y)`: the squares with their marks, rank numbers and file letters.
/// `marks(row, col)` takes the row from the top. Returns where the top left square starts.
pub(super) fn board(
    out: &mut String,
    t: &Theme,
    (x, y): (f32, f32),
    marks: &dyn Fn(usize, usize) -> Marks,
    dark_only: bool,
) -> (f32, f32) {
    panel(out, x, y, 520.0, 522.0, 6.0, "#1a2a66", &t.panel_edge);
    let (ox, oy) = (x + 20.0, y + 20.0);
    for row in 0..8 {
        for col in 0..8 {
            let dark = (row + col) % 2 == 1;
            let (sx, sy) = (ox + col as f32 * SQUARE, oy + row as f32 * SQUARE);
            let _ = write!(
                out,
                r#"<rect x="{sx}" y="{sy}" width="{SQUARE}" height="{SQUARE}" fill="{}"/>"#,
                if dark { DARK_SQUARE } else { LIGHT_SQUARE }
            );
            let m = marks(row, col);
            if dark_only && !dark {
                continue;
            }
            if m.last {
                // The squares of the last move turn gold.
                let _ = write!(
                    out,
                    r##"<rect x="{sx}" y="{sy}" width="{SQUARE}" height="{SQUARE}" fill="{}"/>"##,
                    mix(if dark { DARK_SQUARE } else { LIGHT_SQUARE }, "#ffd166", 0.5)
                );
            }
            if m.danger {
                let _ = write!(
                    out,
                    r##"<rect x="{sx}" y="{sy}" width="{SQUARE}" height="{SQUARE}" fill="#ef476f" fill-opacity="0.55"/>"##
                );
            }
            if m.selected {
                let _ = write!(
                    out,
                    r#"<path d="{} {}" fill="{}" fill-rule="evenodd"/>"#,
                    notched_path(sx, sy, SQUARE, SQUARE, 0.0),
                    notched_path(sx + 4.0, sy + 4.0, SQUARE - 8.0, SQUARE - 8.0, 0.0),
                    t.mint
                );
            }
        }
    }
    // Rank numbers down the left side and file letters along the bottom.
    for i in 0..8 {
        text(
            out,
            ox - 9.0,
            oy + i as f32 * SQUARE + SQUARE / 2.0 + 4.0,
            11.0,
            PIXEL,
            false,
            &t.muted,
            "middle",
            &(8 - i).to_string(),
        );
        text(
            out,
            ox + i as f32 * SQUARE + SQUARE / 2.0,
            oy + 8.0 * SQUARE + 15.0,
            11.0,
            PIXEL,
            false,
            &t.muted,
            "middle",
            &char::from(b'a' + i as u8).to_string(),
        );
    }
    (ox, oy)
}

/// The mint dots and brackets drawn over the squares a picked piece can reach; called after the
/// pieces so they sit on top.
pub(super) fn targets(out: &mut String, t: &Theme, (ox, oy): (f32, f32), marks: &dyn Fn(usize, usize) -> Marks) {
    for row in 0..8 {
        for col in 0..8 {
            let Some(takes) = marks(row, col).target else { continue };
            let (sx, sy) = (ox + col as f32 * SQUARE, oy + row as f32 * SQUARE);
            if takes {
                // Corner brackets around a piece that can be taken.
                for (dx, dy, sx_, sy_) in [
                    (0.0, 0.0, 1.0, 1.0),
                    (SQUARE, 0.0, -1.0, 1.0),
                    (0.0, SQUARE, 1.0, -1.0),
                    (SQUARE, SQUARE, -1.0, -1.0),
                ] {
                    let (cx, cy) = (sx + dx, sy + dy);
                    let _ = write!(
                        out,
                        r#"<path d="M{} {} h{} v{} h{} v{} h{} Z" fill="{}"/>"#,
                        cx,
                        cy,
                        14.0 * sx_,
                        5.0 * sy_,
                        -9.0 * sx_,
                        9.0 * sy_,
                        -5.0 * sx_,
                        t.mint
                    );
                }
            } else {
                let _ = write!(
                    out,
                    r##"<rect x="{}" y="{}" width="14" height="14" fill="{}" stroke="{OUTLINE}" stroke-width="2"/>"##,
                    sx + SQUARE / 2.0 - 7.0,
                    sy + SQUARE / 2.0 - 7.0,
                    t.mint
                );
            }
        }
    }
}
