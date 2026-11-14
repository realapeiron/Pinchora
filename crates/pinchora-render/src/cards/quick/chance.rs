use super::*;

impl QuickCard<'_> {
    pub(super) fn higher(&self, s: &mut String) {
        let t = &self.theme;
        panel(s, 132.0, 124.0, 202.0, 248.0, 6.0, "#a88d5f", &t.gold);
        panel(s, 128.0, 120.0, 202.0, 244.0, 6.0, PAPER, &t.gold);
        self.label(s, 147.0, 152.0, 80.0, self.prompt, INK, "start");
        self.label(s, 309.0, 345.0, 80.0, self.prompt, INK, "end");
        let face = self.prompt.parse::<usize>().unwrap_or(1).saturating_sub(1) % FACES.len();
        art(s, FACES[face], 157.0, 168.0, 144.0, 110.0);
        text(s, 229.0, 323.0, 46.0, PIXEL, false, INK, "middle", &escape(self.prompt));
        self.back(s, 410.0, 120.0, 202.0, 244.0);
        sprite(
            s,
            &[
                "...#...", "..###..", ".#####.", "#######", "..###..", "..###..", "..###..",
            ],
            353.0,
            181.0,
            5.0,
            &t.mint,
            PAPER,
        );
        sprite(
            s,
            &[
                "..###..", "..###..", "..###..", "#######", ".#####.", "..###..", "...#...",
            ],
            353.0,
            268.0,
            5.0,
            &t.accent,
            PAPER,
        );
        self.label(
            s,
            229.0,
            395.0,
            230.0,
            t!(self.lang, "quick-card.current-card"),
            &t.muted,
            "middle",
        );
        self.label(
            s,
            511.0,
            395.0,
            230.0,
            t!(self.lang, "quick-card.higher-or-lower"),
            &t.mint,
            "middle",
        );
        for i in 0..8 {
            rect(
                s,
                342.0 + i as f32 * 8.0,
                335.0,
                5.0,
                8.0,
                if i < self.details.progress {
                    &t.gold
                } else {
                    &t.panel_edge
                },
            );
        }
    }

    pub(super) fn guess(&self, s: &mut String) {
        let t = &self.theme;
        panel(s, 50.0, 123.0, 208.0, 277.0, 5.0, &t.panel, &t.panel_edge);
        // Stepped glass thermometer with a square bulb and calibrated ticks.
        panel(s, 125.0, 158.0, 40.0, 173.0, 6.0, PAPER, "#8fa0d8");
        panel(s, 112.0, 313.0, 66.0, 54.0, 10.0, PAPER, "#8fa0d8");
        let (level, color, label) = match self.details.heat {
            Some(QuickHeat::Cold) => (30.0, "#5bc0eb", t!(self.lang, "quick-card.cold")),
            Some(QuickHeat::Warm) => (86.0, "#ffd166", t!(self.lang, "quick-card.warm")),
            Some(QuickHeat::Hot) => (142.0, "#ff6b4a", t!(self.lang, "quick-card.hot")),
            Some(QuickHeat::Found) => (154.0, "#06d6a0", t!(self.lang, "quick-card.found")),
            None => (0.0, "#8fa0d8", t!(self.lang, "quick-card.awaiting-a-guess")),
        };
        rect(s, 136.0, 324.0 - level, 18.0, level + 10.0, color);
        panel(s, 123.0, 325.0, 44.0, 30.0, 6.0, color, color);
        rect(s, 128.0, 330.0, 6.0, 8.0, PAPER);
        for i in 0..7 {
            rect(
                s,
                174.0,
                173.0 + i as f32 * 22.0,
                if i % 2 == 0 { 18.0 } else { 10.0 },
                3.0,
                &t.muted,
            );
        }
        self.label(
            s,
            154.0,
            147.0,
            180.0,
            t!(self.lang, "quick-card.hot"),
            &t.accent,
            "middle",
        );
        self.label(
            s,
            154.0,
            390.0,
            180.0,
            t!(self.lang, "quick-card.cold"),
            "#5bc0eb",
            "middle",
        );
        self.label(
            s,
            478.0,
            159.0,
            370.0,
            t!(self.lang, "quick-card.a-secret-between-1-100"),
            &t.muted,
            "middle",
        );
        text(
            s,
            478.0,
            265.0,
            76.0,
            PIXEL,
            false,
            &t.ink,
            "middle",
            &escape(self.prompt),
        );
        self.label(s, 478.0, 319.0, 370.0, label, color, "middle");
        self.prose(
            s,
            478.0,
            365.0,
            360.0,
            t!(self.lang, "quick-card.every-guess-brings-a-new-clue"),
            18.0,
            &t.muted,
            2,
        );
    }

    pub(super) fn fortune(&self, s: &mut String) {
        let t = &self.theme;
        panel(s, 40.0, 120.0, 240.0, 280.0, 6.0, "#243066", &t.panel_edge);
        if self.kind == QuickKind::Roll {
            let n = self
                .prompt
                .split('/')
                .next()
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(0);
            panel(s, 91.0, 191.0, 144.0, 140.0, 10.0, "#a88d5f", "#7d6a4a");
            panel(s, 83.0, 179.0, 144.0, 140.0, 10.0, PAPER, "#e8d3a3");
            if (1..=6).contains(&n) {
                let dots: &[(f32, f32)] = match n {
                    1 => &[(1.0, 1.0)],
                    2 => &[(0.0, 0.0), (2.0, 2.0)],
                    3 => &[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)],
                    4 => &[(0.0, 0.0), (2.0, 0.0), (0.0, 2.0), (2.0, 2.0)],
                    5 => &[(0.0, 0.0), (2.0, 0.0), (1.0, 1.0), (0.0, 2.0), (2.0, 2.0)],
                    _ => &[(0.0, 0.0), (2.0, 0.0), (0.0, 1.0), (2.0, 1.0), (0.0, 2.0), (2.0, 2.0)],
                };
                for (x, y) in dots {
                    panel(s, 106.0 + x * 38.0, 202.0 + y * 36.0, 20.0, 20.0, 3.0, INK, INK);
                }
            } else {
                text(s, 155.0, 270.0, 48.0, PIXEL, false, INK, "middle", &n.to_string());
            }
        } else {
            for (x, y) in [(67.0, 170.0), (237.0, 190.0), (225.0, 325.0)] {
                sprite(s, &[".#.", "###", ".#."], x, y, 4.0, &t.gold, PAPER);
            }
            art(s, "c_creamcurl_whelk", 64.0, 177.0, 192.0, 163.0);
        }
        self.label(
            s,
            160.0,
            375.0,
            208.0,
            match self.kind {
                QuickKind::Roll => t!(self.lang, "quick-card.dice-roll"),
                QuickKind::Choose => t!(self.lang, "quick-card.chosen-for-you"),
                _ => t!(self.lang, "quick-card.the-conch-speaks"),
            },
            &t.gold,
            "middle",
        );
        self.label(
            s,
            496.0,
            156.0,
            380.0,
            t!(self.lang, "quick-card.the-reef-has-answered"),
            &t.mint,
            "middle",
        );
        self.prose(s, 496.0, 230.0, 370.0, self.prompt, 30.0, &t.ink, 4);
        sprite(s, SHELL, 478.0, 380.0, 4.0, &t.gold, PAPER);
    }
}
