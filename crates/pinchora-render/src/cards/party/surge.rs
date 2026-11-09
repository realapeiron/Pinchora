use super::*;

/// How a storm stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurgeState {
    Rolling,
    Held,
    Broken,
}

/// A sandbag, 8 wide.
const BAG: &[&str] = &["..####..", ".#oooo#.", "#oooooo#", "#o####o#", ".######."];

/// 640×380 storm surge card: the sky, the sea, the sandbag wall and how high it stands against its goal.
pub struct SurgeCard<'a> {
    /// The language of the labels.
    pub lang: Lang,
    /// The wall's height in sandbags, and the height it needs.
    pub wall: i64,
    pub goal: i64,
    pub helpers: i64,
    /// The best helpers with their sandbags.
    pub top: &'a [(String, i64)],
    pub state: SurgeState,
    pub theme: Theme,
}

impl SurgeCard<'_> {
    pub const WIDTH: f32 = 640.0;
    pub const HEIGHT: f32 = 380.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH, Self::HEIGHT);
        let mut s = open(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(
            &mut s,
            28.0,
            52.0,
            24.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &t!(self.lang, "surge.card.title"),
        );
        let (label, tone) = match self.state {
            SurgeState::Rolling => (t!(self.lang, "surge.card.rolling"), &t.accent),
            SurgeState::Held => (t!(self.lang, "surge.card.held"), &t.mint),
            SurgeState::Broken => (t!(self.lang, "surge.card.broken"), &t.gold),
        };
        chip(&mut s, w - 28.0, 26.0, &label, tone, t);

        let bounds = (24.0, 76.0, 592.0, 190.0);
        self.draw_storm_scene(&mut s, bounds);
        let fraction = self.draw_sandbag_wall(&mut s, t, bounds);
        self.draw_progress_and_helpers(&mut s, t, fraction, tone);
        s.push_str("</svg>");
        s
    }
    fn draw_storm_scene(&self, s: &mut String, bounds: (f32, f32, f32, f32)) {
        // The scene: sky, clouds, rain and sea behind the wall.
        let (sx, sy, sw, sh) = bounds;
        let (sky_top, sky_bottom) = match self.state {
            SurgeState::Held => ("#6fb6e8", "#cfeefa"),
            _ => ("#1a2347", "#46579a"),
        };
        let _ = write!(
            s,
            r##"<defs><linearGradient id="storm" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{sky_top}"/><stop offset="1" stop-color="{sky_bottom}"/></linearGradient><clipPath id="scene"><rect x="{sx}" y="{sy}" width="{sw}" height="{sh}"/></clipPath></defs><g clip-path="url(#scene)"><rect x="{sx}" y="{sy}" width="{sw}" height="{sh}" fill="url(#storm)"/>"##
        );
        if self.state == SurgeState::Held {
            let _ = write!(s, r##"<rect x="500" y="92" width="36" height="36" fill="#ffd166"/>"##);
        } else {
            // Cloud banks and rain, in the same places every time.
            for (cx, cy, cw) in [
                (30.0, 80.0, 150.0),
                (170.0, 92.0, 190.0),
                (360.0, 80.0, 170.0),
                (500.0, 94.0, 130.0),
            ] {
                let _ = write!(
                    s,
                    r##"<rect x="{cx}" y="{cy}" width="{cw}" height="22" fill="#2a3566"/><rect x="{}" y="{}" width="{}" height="14" fill="#2a3566"/><rect x="{cx}" y="{}" width="{cw}" height="4" fill="#3a4a85"/>"##,
                    cx + 14.0,
                    cy - 12.0,
                    cw - 40.0,
                    cy + 18.0
                );
            }
            for k in 0..46 {
                let x = 30.0 + (k * 53 % 560) as f32;
                let y = 118.0 + (k * 37 % 80) as f32;
                let _ = write!(
                    s,
                    r##"<rect x="{x}" y="{y}" width="2" height="8" fill="#8fa0d8" fill-opacity="0.8"/>"##
                );
            }
            if self.state == SurgeState::Rolling {
                // A bolt.
                for (bx, by, bw) in [
                    (300.0, 118.0, 10.0),
                    (294.0, 128.0, 10.0),
                    (302.0, 138.0, 8.0),
                    (296.0, 146.0, 8.0),
                ] {
                    let _ = write!(
                        s,
                        r##"<rect x="{bx}" y="{by}" width="{bw}" height="10" fill="#fff7a8"/>"##
                    );
                }
            }
        }
        // The sea: bands of water rising with the danger.
        let sea_top = if self.state == SurgeState::Broken { 190.0 } else { 214.0 };
        let _ = write!(
            s,
            r##"<rect x="{sx}" y="{sea_top}" width="{sw}" height="80" fill="#2c7fb8"/>"##
        );
        for k in 0..30 {
            let x = sx + k as f32 * 20.0;
            let _ = write!(
                s,
                r##"<rect x="{x}" y="{}" width="12" height="4" fill="#e8f7ff"/><rect x="{}" y="{}" width="12" height="4" fill="#3a9bdc"/>"##,
                sea_top - 4.0 + (k % 3) as f32 * 2.0,
                x + 6.0,
                sea_top + 10.0
            );
        }
    }

    fn draw_sandbag_wall(&self, s: &mut String, t: &Theme, bounds: (f32, f32, f32, f32)) -> f32 {
        let (sx, sy, sw, sh) = bounds;
        // The wall: rows of sandbags, as tall as the wall stands against its goal.
        let fraction = if self.goal > 0 {
            (self.wall as f32 / self.goal as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let rows = (fraction * 6.0).ceil() as i32;
        let (bag_w, bag_h) = (8.0 * 4.0, 5.0 * 4.0);
        for row in 0..rows {
            let y = sy + sh - bag_h * (row as f32 + 1.0);
            let offset = if row % 2 == 0 { 0.0 } else { bag_w / 2.0 };
            let mut x = sx + 120.0 - offset;
            while x < sx + sw - 120.0 {
                let shade = if (x as i32 / 32 + row) % 2 == 0 {
                    ("#c9ab6a", "#e8d3a3")
                } else {
                    ("#b99a5a", "#d9c28c")
                };
                sprite(s, BAG, x, y, 4.0, shade.0, shade.1);
                x += bag_w;
            }
        }
        // Little crabs on the sand either side, working.
        sprite(s, CRAB, sx + 40.0, sy + sh - 28.0, 3.0, &t.accent, "#fff1e8");
        sprite(
            s,
            CRAB,
            sx + sw - 40.0 - 27.0,
            sy + sh - 28.0,
            3.0,
            "#ffd166",
            "#fff1e8",
        );
        s.push_str("</g>");
        let _ = write!(
            s,
            r##"<path d="{}" fill="none" stroke="{}" stroke-width="4"/>"##,
            notched_path(sx - 2.0, sy - 2.0, sw + 4.0, sh + 4.0, 4.0),
            t.panel_edge
        );

        fraction
    }
    fn draw_progress_and_helpers(&self, s: &mut String, t: &Theme, fraction: f32, tone: &str) {
        // The bar, and the helpers.
        segment_bar(
            s,
            24.0,
            280.0,
            592.0,
            32.0,
            fraction,
            24,
            tone,
            &t.panel_edge,
            &t.panel_edge,
        );
        text(
            s,
            28.0,
            340.0,
            16.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &t!(
                self.lang,
                "surge.card.wall",
                wall = thousands_in(self.lang, self.wall),
                goal = thousands_in(self.lang, self.goal)
            ),
        );
        text(
            s,
            28.0,
            364.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &t!(
                self.lang,
                "surge.card.helping",
                count = thousands_in(self.lang, self.helpers)
            ),
        );
        for (n, (name, bags)) in self.top.iter().take(3).enumerate() {
            let y = 328.0 + n as f32 * 18.0;
            let medal = [&t.gold, "#d6deee", "#db8f4c"][n];
            let _ = write!(
                s,
                r#"<rect x="316" y="{}" width="8" height="8" fill="{medal}"/>"#,
                y - 8.0
            );
            text(
                s,
                332.0,
                y,
                13.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(name, 13.0, 200.0)),
            );
            text(
                s,
                612.0,
                y,
                13.0,
                PIXEL,
                false,
                &t.muted,
                "end",
                &t!(self.lang, "surge.card.bags", bags = bags),
            );
        }
    }
}
