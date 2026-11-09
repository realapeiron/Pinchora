use std::fmt::Write;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::boards::COORDINATE_SIZE;
use crate::cards::boards::coordinate_color;
use crate::cards::boards::fitted_pixel_text;
use crate::cards::text;
use crate::{
    Theme,
    svg::{
        SHELL, background, background_plain, compact_in, escape, mix, notched_path, panel, sprite, truncate,
        truncate_to,
    },
};

/// The colours of the five ships, longest first: galleon, frigate, brig, sloop and dinghy.
pub const SHIP_COLORS: [&str; 5] = ["#c98a1e", "#e0674a", "#9b6bc4", "#3a9bdc", "#06d6a0"];
/// The ships' English names; the cards use `seafleet.ship-<index>` of the catalogs.
pub const SHIP_NAMES: [&str; 5] = ["Galleon", "Frigate", "Brig", "Sloop", "Dinghy"];
const SHIP_LENGTHS: [usize; 5] = [5, 4, 3, 3, 2];

const CELL: f32 = 34.0;
const SEA: f32 = CELL * 10.0;
/// What a panel around one sea takes: room for the letters above and the numbers beside.
const PANEL_W: f32 = SEA + 44.0;
const PANEL_H: f32 = SEA + 40.0;

/// A ship that is shown on a sea: where its first cell is (`y * 10 + x`), which way it lies and which ship it is.
#[derive(Debug, Clone, Copy)]
pub struct ShipView {
    pub cell: usize,
    pub horizontal: bool,
    /// An index into [`SHIP_NAMES`].
    pub ship: usize,
    pub sunk: bool,
}

/// One sea as drawn on a card.
pub struct SeaView<'a> {
    pub name: &'a str,
    /// The shots fired into it: 100 characters of `.`, `m` (miss) and `h` (hit).
    pub shots: &'a str,
    /// The ships shown: all of them on a player's own sea, only the sunk ones on the radar.
    pub ships: Vec<ShipView>,
    /// How many of the five ships still float.
    pub afloat: usize,
}

fn sea(out: &mut String, t: &Theme, x: f32, y: f32, view: &SeaView<'_>, last: Option<usize>, frame: &str) {
    panel(out, x, y, PANEL_W, PANEL_H, 6.0, "#10204a", frame);
    let (gx, gy) = (x + 28.0, y + 24.0);
    let ink = coordinate_color(t);
    for i in 0..10 {
        text(
            out,
            gx + i as f32 * CELL + CELL / 2.0,
            gy - 8.0,
            COORDINATE_SIZE,
            PIXEL,
            false,
            &ink,
            "middle",
            &char::from(b'A' + i as u8).to_string(),
        );
        text(
            out,
            gx - 13.0,
            gy + i as f32 * CELL + CELL / 2.0 + 5.0,
            COORDINATE_SIZE,
            PIXEL,
            false,
            &ink,
            "middle",
            &(i + 1).to_string(),
        );
    }
    for row in 0..10 {
        for col in 0..10 {
            let shade = if (row + col) % 2 == 0 { "#16346a" } else { "#133062" };
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{CELL}" height="{CELL}" fill="{shade}"/>"#,
                gx + col as f32 * CELL,
                gy + row as f32 * CELL
            );
        }
    }
    // Ships, as hulls with a row of portholes.
    for ship in &view.ships {
        let len = SHIP_LENGTHS[ship.ship];
        let (cx, cy) = (gx + (ship.cell % 10) as f32 * CELL, gy + (ship.cell / 10) as f32 * CELL);
        let (w, h) = if ship.horizontal {
            (len as f32 * CELL, CELL)
        } else {
            (CELL, len as f32 * CELL)
        };
        let color = if ship.sunk {
            mix(SHIP_COLORS[ship.ship], "#000000", 0.55)
        } else {
            SHIP_COLORS[ship.ship].to_owned()
        };
        let _ = write!(
            out,
            r##"<path d="{}" fill="#0b1029"/><path d="{}" fill="{color}"/>"##,
            notched_path(cx + 2.0, cy + 2.0, w - 4.0, h - 4.0, 6.0),
            notched_path(cx + 4.0, cy + 4.0, w - 8.0, h - 8.0, 4.0),
        );
        let _ = write!(
            out,
            r##"<rect x="{}" y="{}" width="{}" height="4" fill="#ffffff" fill-opacity="0.3"/>"##,
            cx + 8.0,
            cy + 6.0,
            if ship.horizontal { w - 16.0 } else { 4.0 }
        );
        for k in 0..len {
            let (px, py) = if ship.horizontal {
                (cx + k as f32 * CELL + CELL / 2.0 - 3.0, cy + CELL / 2.0 - 3.0)
            } else {
                (cx + CELL / 2.0 - 3.0, cy + k as f32 * CELL + CELL / 2.0 - 3.0)
            };
            let _ = write!(
                out,
                r##"<rect x="{px}" y="{py}" width="6" height="6" fill="#0b1029" fill-opacity="0.55"/>"##
            );
        }
    }
    // Shots on top: a flame where a ship was hit, a ring of spray where the sea was.
    for (cell, shot) in view.shots.chars().enumerate().take(100) {
        let (cx, cy) = (gx + (cell % 10) as f32 * CELL, gy + (cell / 10) as f32 * CELL);
        match shot {
            'h' => {
                let _ = write!(
                    out,
                    r##"<rect x="{}" y="{}" width="20" height="20" fill="#ef476f"/><rect x="{}" y="{}" width="12" height="12" fill="#ff9e4a"/><rect x="{}" y="{}" width="6" height="6" fill="#ffe08a"/>"##,
                    cx + 7.0,
                    cy + 7.0,
                    cx + 11.0,
                    cy + 11.0,
                    cx + 14.0,
                    cy + 14.0
                );
            }
            'm' => {
                let _ = write!(
                    out,
                    r##"<rect x="{}" y="{}" width="10" height="10" fill="#dff3ff" fill-opacity="0.85"/><rect x="{}" y="{}" width="4" height="4" fill="#16346a"/>"##,
                    cx + 12.0,
                    cy + 12.0,
                    cx + 15.0,
                    cy + 15.0
                );
            }
            _ => {}
        }
    }
    if let Some(cell) = last {
        let (cx, cy) = (gx + (cell % 10) as f32 * CELL, gy + (cell / 10) as f32 * CELL);
        let _ = write!(
            out,
            r##"<path d="{} {}" fill="{}" fill-rule="evenodd"/>"##,
            notched_path(cx, cy, CELL, CELL, 0.0),
            notched_path(cx + 3.0, cy + 3.0, CELL - 6.0, CELL - 6.0, 0.0),
            t.gold
        );
    }
}

/// Five pips, lit for the ships still afloat.
fn pips(out: &mut String, t: &Theme, x: f32, y: f32, afloat: usize) {
    for i in 0..5 {
        let fill = if i < afloat { &t.mint } else { &t.panel_edge };
        let _ = write!(
            out,
            r##"<rect x="{}" y="{y}" width="14" height="14" fill="{fill}" stroke="#0b1029" stroke-width="2"/>"##,
            x + i as f32 * 20.0
        );
    }
}

/// 836×550 radar: both players' waters side by side with the shots fired into them, and who is aiming where.
pub struct RadarCard<'a> {
    /// The language of the labels; the status line comes translated, and in capitals.
    pub lang: Lang,
    pub seas: [SeaView<'a>; 2],
    /// Which player is to fire (0 or 1); the other's waters are the target. `None` when the game is over.
    pub turn: Option<u8>,
    pub winner: Option<u8>,
    /// The sea (0 or 1) and the cell of the last shot.
    pub last: Option<(u8, usize)>,
    pub status: &'a str,
    /// Shells on the table; 0 hides the pot.
    pub pot: i64,
    pub theme: Theme,
}

impl RadarCard<'_> {
    pub const WIDTH: u32 = 836;
    pub const HEIGHT: u32 = 550;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH as f32, Self::HEIGHT as f32);
        let mut s = String::with_capacity(64 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        for side in 0..2usize {
            let x = 24.0 + side as f32 * (PANEL_W + 20.0);
            let aimed = self.turn.is_some_and(|turn| usize::from(turn) != side);
            let lost = self.winner.is_some_and(|win| usize::from(win) != side);
            let lit = aimed || self.winner == Some(side as u8);
            let edge = if lit { &t.gold } else { &t.panel_edge };
            panel(&mut s, x, 20.0, PANEL_W, 56.0, 4.0, &t.panel, edge);
            let view = &self.seas[side];
            text(
                &mut s,
                x + 16.0,
                56.0,
                20.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(view.name, 20.0, 236.0)),
            );
            pips(&mut s, t, x + PANEL_W - 116.0, 28.0, view.afloat);
            let note = if self.winner == Some(side as u8) {
                t!(self.lang, "board.card.winner")
            } else if lost {
                t!(self.lang, "seafleet.card.sunk")
            } else if aimed {
                t!(self.lang, "seafleet.card.fire-here")
            } else {
                t!(self.lang, "seafleet.card.waiting")
            };
            let color = if aimed || self.winner == Some(side as u8) {
                &t.gold
            } else {
                &t.muted
            };
            text(
                &mut s,
                x + PANEL_W - 16.0,
                66.0,
                11.0,
                PIXEL,
                false,
                color,
                "end",
                &note,
            );
            let last = self
                .last
                .filter(|&(sea, _)| usize::from(sea) == side)
                .map(|(_, cell)| cell);
            sea(&mut s, t, x, 84.0, view, last, edge);
        }
        panel(&mut s, 24.0, 470.0, w - 48.0, 56.0, 4.0, &t.panel, &t.panel_edge);
        // The pot sits at the right; the words get what is left of the panel.
        let max = if self.pot > 0 { 654.0 } else { 748.0 };
        fitted_pixel_text(
            &mut s,
            (44.0, 505.0),
            max,
            &t.ink,
            "start",
            &self.lang.upper(self.status),
        );
        if self.pot > 0 {
            sprite(&mut s, SHELL, w - 126.0, 486.0, 3.0, &t.gold, "#ffffff");
            text(
                &mut s,
                w - 44.0,
                505.0,
                17.0,
                PIXEL,
                false,
                &t.gold,
                "end",
                &compact_in(self.lang, self.pot),
            );
        }
        s.push_str("</svg>");
        s
    }
}

/// 620×500: one player's own sea with every ship on it, a list of the fleet beside it. Shows the fleet being
/// placed (no shots) and the fleet under fire.
pub struct FleetCard<'a> {
    /// The language of the labels; title, subtitle and footer come translated, and in capitals.
    pub lang: Lang,
    pub title: &'a str,
    pub subtitle: &'a str,
    pub sea: SeaView<'a>,
    /// Which ships are sunk, by index.
    pub sunk: [bool; 5],
    pub footer: &'a str,
    pub last: Option<usize>,
    pub theme: Theme,
}

impl FleetCard<'_> {
    pub const WIDTH: u32 = 620;
    pub const HEIGHT: u32 = 500;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH as f32, Self::HEIGHT as f32);
        let mut s = String::with_capacity(48 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background_plain(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(
            &mut s,
            24.0,
            46.0,
            24.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &escape(&truncate(&self.lang.upper(self.title), 24)),
        );
        text(
            &mut s,
            24.0,
            68.0,
            12.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &escape(&truncate(&self.lang.upper(self.subtitle), 48)),
        );
        sea(&mut s, t, 24.0, 80.0, &self.sea, self.last, &t.panel_edge);
        // The fleet list.
        let lx = 24.0 + PANEL_W + 14.0;
        panel(&mut s, lx, 80.0, w - lx - 24.0, PANEL_H, 4.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            lx + 14.0,
            108.0,
            12.0,
            PIXEL,
            false,
            &t.gold,
            "start",
            &t!(self.lang, "seafleet.card.your-fleet"),
        );
        for ship in 0..5 {
            let y = 128.0 + ship as f32 * 52.0;
            let sunk = self.sunk[ship];
            let color = if sunk {
                mix(SHIP_COLORS[ship], "#000000", 0.55)
            } else {
                SHIP_COLORS[ship].to_owned()
            };
            for k in 0..SHIP_LENGTHS[ship] {
                let _ = write!(
                    s,
                    r##"<rect x="{}" y="{y}" width="16" height="16" fill="{color}" stroke="#0b1029" stroke-width="2"/>"##,
                    lx + 14.0 + k as f32 * 18.0
                );
            }
            text(
                &mut s,
                lx + 14.0,
                y + 36.0,
                15.0,
                SANS,
                true,
                if sunk { &t.muted } else { &t.ink },
                "start",
                &escape(&if sunk {
                    t!(
                        self.lang,
                        "seafleet.card.ship-sunk",
                        ship = t!(self.lang, &format!("seafleet.ship-{ship}"))
                    )
                } else {
                    t!(self.lang, &format!("seafleet.ship-{ship}"))
                }),
            );
        }
        text(
            &mut s,
            w / 2.0,
            h - 22.0,
            13.0,
            PIXEL,
            false,
            &t.muted,
            "middle",
            &escape(&truncate(&self.lang.upper(self.footer), 60)),
        );
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use crate::Renderer;
    use crate::cards::fleet::*;

    fn ships() -> Vec<ShipView> {
        vec![
            ShipView {
                cell: 0,
                horizontal: true,
                ship: 0,
                sunk: false,
            },
            ShipView {
                cell: 90,
                horizontal: true,
                ship: 4,
                sunk: true,
            },
            ShipView {
                cell: 9,
                horizontal: false,
                ship: 2,
                sunk: false,
            },
        ]
    }

    fn shots() -> String {
        let mut shots = vec!['.'; 100];
        shots[0] = 'h';
        shots[55] = 'm';
        shots[91] = 'h';
        shots.into_iter().collect()
    }

    #[test]
    fn the_radar_draws_both_seas_their_shots_and_who_is_aimed_at() {
        let text = shots();
        let card = RadarCard {
            lang: Lang::En,
            seas: [
                SeaView {
                    name: "Marlowe",
                    shots: &text,
                    ships: Vec::new(),
                    afloat: 5,
                },
                SeaView {
                    name: "Cap",
                    shots: &text,
                    ships: ships(),
                    afloat: 4,
                },
            ],
            turn: Some(0),
            winner: None,
            last: Some((1, 55)),
            status: "Marlowe to fire",
            pot: 0,
            theme: Theme::reef(),
        };
        let svg = card.to_svg();
        assert_eq!(svg.matches("FIRE HERE").count(), 1, "only the waters being aimed at");
        assert_eq!(svg.matches("WAITING").count(), 1);
        assert_eq!(svg.matches(r##"fill="#ef476f""##).count(), 4, "two flames on each sea");
        let won = RadarCard {
            winner: Some(0),
            turn: None,
            ..card
        };
        let svg = won.to_svg();
        assert!(svg.contains("WINNER!") && svg.contains("FLEET SUNK"));
    }

    #[test]
    fn the_radar_and_the_fleet_render_to_pngs_of_the_documented_size() {
        let text = shots();
        let radar = RadarCard {
            lang: Lang::En,
            seas: [
                SeaView {
                    name: "A",
                    shots: &text,
                    ships: Vec::new(),
                    afloat: 5,
                },
                SeaView {
                    name: "B",
                    shots: &text,
                    ships: ships(),
                    afloat: 3,
                },
            ],
            turn: Some(1),
            winner: None,
            last: None,
            status: "B to fire",
            pot: 120,
            theme: Theme::reef(),
        };
        let png = Renderer::new().radar_card(&radar).expect("renders");
        let image = image::load_from_memory(&png).expect("a png");
        assert_eq!((image.width(), image.height()), (RadarCard::WIDTH, RadarCard::HEIGHT));
        let fleet = FleetCard {
            lang: Lang::En,
            title: "Your fleet",
            subtitle: "Harbour line",
            sea: SeaView {
                name: "Me",
                shots: &text,
                ships: ships(),
                afloat: 2,
            },
            sunk: [false, false, false, false, true],
            footer: "Shuffle or confirm",
            last: Some(55),
            theme: Theme::reef(),
        };
        let png = Renderer::new().fleet_card(&fleet).expect("renders");
        let image = image::load_from_memory(&png).expect("a png");
        assert_eq!((image.width(), image.height()), (FleetCard::WIDTH, FleetCard::HEIGHT));
    }
}
