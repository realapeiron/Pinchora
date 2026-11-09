use super::*;

/// What the crab pot is doing.
#[derive(Debug, Clone, PartialEq)]
pub enum PotState {
    /// Nothing in the water.
    Empty,
    /// Soaking: `left` as a phrase, `fraction` of the way done, `hours` the soak was set for.
    Soaking { left: String, fraction: f32, hours: u32 },
    /// Done; waiting to be hauled in.
    Ready { hours: u32 },
}

/// 800×340 scene: the crab pot on the sea floor with a buoy on the surface, and a panel saying what
/// it is up to.
pub struct PotCard {
    pub level: u8,
    pub max_level: u8,
    pub state: PotState,
    /// How fast this pot catches, e.g. `1.5 catches an hour`.
    pub rate: String,
    /// The next upgrade, e.g. `Level 2 for 400 shells`; empty at the top.
    pub upgrade: String,
    /// The language of the labels; the rate and the upgrade come written.
    pub lang: Lang,
    pub theme: Theme,
}

/// The pot: `o` outline, `w` wire, `r` rope, `s` the shadow on the wire, `b` a bait bag.
const POT: &[&str] = &[
    "......oooooooooooo......",
    "....oowwwwwwwwwwwwoo....",
    "...owwwwwwooooowwwwwo...",
    "..owwwwwwo.....owwwwwo..",
    "..owwswwwo.....owwswwo..",
    ".owwwwwwwwoooooowwwwwwo.",
    ".owwswwwwwwwwwwwwwwswwo.",
    ".owwwwwwwwwwwwwwwwwwwwo.",
    ".owswwwwwwwwbbwwwwwwswo.",
    ".owwwwwwwwwwbbwwwwwwwwo.",
    ".oswwwwwwwwwwwwwwwwwwso.",
    "..oooooooooooooooooooo..",
];

const BUOY: &[&str] = &["..oo..", ".orro.", "orrrro", "owwwwo", "orrrro", ".orro.", "..oo.."];

impl PotCard {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (800.0, 340.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        header(
            &mut s,
            t,
            &t!(self.lang, "crabpot.card.title"),
            &t!(
                self.lang,
                "crabpot.card.level",
                level = self.level,
                max = self.max_level
            ),
            764.0,
        );
        self.draw_seabed_and_pot(&mut s, t, w);
        self.draw_status_panel(&mut s, t);
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
    fn draw_seabed_and_pot(&self, s: &mut String, t: &Theme, w: f32) {
        // The sea: a lighter band at the top, the sea floor in sand at the bottom.
        let _ = write!(
            s,
            r##"<rect x="0" y="96" width="{w}" height="150" fill="#2c7fb8" fill-opacity="0.35"/><rect x="0" y="246" width="{w}" height="94" fill="#d2b77f"/><rect x="0" y="246" width="{w}" height="6" fill="#a88d5f"/>"##
        );
        for (x, y) in [
            (60.0, 276.0),
            (150.0, 308.0),
            (250.0, 286.0),
            (300.0, 318.0),
            (120.0, 260.0),
        ] {
            let _ = write!(s, r##"<rect x="{x}" y="{y}" width="10" height="4" fill="#a88d5f"/>"##);
        }
        // Sea weed on the floor.
        for (x, hgt) in [(30.0, 40.0), (46.0, 56.0), (340.0, 48.0), (356.0, 36.0)] {
            let _ = write!(
                s,
                r##"<rect x="{x}" y="{}" width="8" height="{hgt}" fill="#4f9f5a"/>"##,
                250.0 - hgt
            );
        }
        let rope = "#c9a46a";
        let pot_x = 110.0;
        let pot_y = 246.0 - 12.0 * 8.0 + 4.0;
        let soaking = !matches!(self.state, PotState::Empty);
        if soaking {
            // Rope up to the buoy on the surface.
            let _ = write!(
                s,
                r#"<rect x="{}" y="108" width="4" height="{}" fill="{rope}"/>"#,
                pot_x + 94.0,
                pot_y - 108.0
            );
            pixels(
                s,
                BUOY,
                pot_x + 94.0 - 10.0,
                86.0,
                4.0,
                &[('o', "#1b1d2b"), ('r', "#ff6b4a"), ('w', "#fff1e8")],
            );
        }
        pixel_shadow(s, pot_x + 96.0, 252.0, 90.0, 6.0, 0.25);
        pixels(
            s,
            POT,
            pot_x,
            pot_y,
            8.0,
            &[('o', "#1b1d2b"), ('w', "#c9a46a"), ('s', "#8a6b3a"), ('b', "#ff9e7a")],
        );
        if matches!(self.state, PotState::Ready { .. }) {
            for (sx, sy) in [
                (pot_x + 40.0, pot_y - 30.0),
                (pot_x + 130.0, pot_y - 40.0),
                (pot_x + 190.0, pot_y - 16.0),
            ] {
                sprite(s, STAR, sx, sy, 3.0, &t.gold, "#fff7d6");
            }
        }
    }

    fn draw_status_panel(&self, s: &mut String, t: &Theme) {
        // The panel on the right.
        panel(s, 430.0, 96.0, 334.0, 214.0, 4.0, &t.panel, &t.panel_edge);
        let (status, color) = match &self.state {
            PotState::Empty => (t!(self.lang, "crabpot.card.empty"), t.muted.clone()),
            PotState::Soaking { .. } => (t!(self.lang, "crabpot.card.soaking"), t.accent.clone()),
            PotState::Ready { .. } => (t!(self.lang, "crabpot.card.ready"), t.mint.clone()),
        };
        text(s, 454.0, 140.0, 24.0, PIXEL, false, &color, "start", &status);
        match &self.state {
            PotState::Empty => {
                text(
                    s,
                    454.0,
                    176.0,
                    18.0,
                    SANS,
                    false,
                    &t.ink,
                    "start",
                    &escape(&t!(self.lang, "crabpot.card.set-hint")),
                );
            }
            PotState::Soaking { left, fraction, hours } => {
                text(
                    s,
                    454.0,
                    176.0,
                    18.0,
                    SANS,
                    false,
                    &t.ink,
                    "start",
                    &escape(&t!(self.lang, "crabpot.card.soak-left", left = left, hours = hours)),
                );
                segment_bar(
                    s,
                    454.0,
                    190.0,
                    286.0,
                    28.0,
                    *fraction,
                    12,
                    &t.accent,
                    "#1d2b53",
                    &t.panel_edge,
                );
            }
            PotState::Ready { hours } => {
                text(
                    s,
                    454.0,
                    176.0,
                    18.0,
                    SANS,
                    false,
                    &t.ink,
                    "start",
                    &escape(&t!(self.lang, "crabpot.card.soak-done", hours = hours)),
                );
            }
        }
        text(
            s,
            454.0,
            252.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &escape(&self.lang.upper(&self.rate)),
        );
        let upgrade = if self.upgrade.is_empty() {
            t!(self.lang, "crabpot.card.top-level")
        } else {
            self.lang.upper(&self.upgrade)
        };
        text(s, 454.0, 286.0, 14.0, PIXEL, false, &t.gold, "start", &escape(&upgrade));
    }
}
