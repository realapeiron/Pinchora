use super::*;

impl QuickCard<'_> {
    pub(super) fn anagram(&self, s: &mut String) {
        let t = &self.theme;
        self.label(
            s,
            370.0,
            140.0,
            620.0,
            t!(self.lang, "quick-card.a-word-washed-ashore"),
            &t.gold,
            "middle",
        );
        let word = self.lang.upper(self.prompt);
        let letters: Vec<char> = word.chars().take(20).collect();
        let count = letters.len().max(1);
        let cols = count.min(10);
        let size = (620.0 / cols as f32).min(82.0).floor();
        for (row, chunk) in letters.chunks(cols).enumerate() {
            let x0 = (740.0 - size * chunk.len() as f32) / 2.0;
            for (i, c) in chunk.iter().enumerate() {
                let (x, y) = (x0 + i as f32 * size, 176.0 + row as f32 * 100.0);
                panel(s, x + 3.0, y + 6.0, size - 6.0, 78.0, 4.0, "#a88d5f", "#7d6a4a");
                panel(s, x + 3.0, y, size - 6.0, 76.0, 4.0, PAPER, "#e8d3a3");
                text(
                    s,
                    x + size / 2.0,
                    y + 49.0,
                    36.0,
                    PIXEL,
                    false,
                    INK,
                    "middle",
                    &escape(&c.to_string()),
                );
                rect(s, x + 10.0, y + 66.0, size - 23.0, 2.0, "#d2b77f");
                if count <= 10 {
                    rect(s, x + 14.0, 315.0, size - 28.0, 4.0, &t.muted);
                }
            }
        }
        self.label(
            s,
            370.0,
            391.0,
            620.0,
            t!(self.lang, "quick-card.rearrange-every-letter"),
            &t.mint,
            "middle",
        );
    }

    pub(super) fn typing(&self, s: &mut String) {
        let t = &self.theme;
        panel(s, 45.0, 143.0, 656.0, 162.0, 5.0, "#a88d5f", "#7d6a4a");
        panel(s, 40.0, 136.0, 656.0, 162.0, 5.0, PAPER, "#e8d3a3");
        for i in 0..4 {
            rect(s, 58.0, 170.0 + i as f32 * 32.0, 620.0, 1.0, "#d2b77f");
        }
        rect(s, 76.0, 147.0, 2.0, 140.0, "#e6a99b");
        let lines = wrap_laid_out(self.prompt, SANS, true, 23.0, 574.0, 3);
        for (row, line) in lines.iter().enumerate() {
            let y = 226.0 - lines.len().saturating_sub(1) as f32 * 19.0 + row as f32 * 38.0;
            // Keep one shaped text run so spaces, kerning and Turkish glyph advances remain intact.
            // SVG's per-character rotations and relative offsets add a gentle wave to the paper.
            let mut rotations = String::new();
            let mut offsets = String::new();
            let mut previous = 0;
            for (i, _) in line.chars().enumerate() {
                let angle = if self.distort {
                    (i as i32 % 5 - 2) as f32 * 0.7
                } else {
                    0.0
                };
                let offset = if self.distort { i as i32 % 3 - 1 } else { 0 };
                let _ = write!(rotations, "{angle} ");
                let _ = write!(offsets, "{} ", offset - previous);
                previous = offset;
            }
            let _ = write!(
                s,
                r#"<text x="96" y="{y}" font-family="{SANS}" font-size="23" font-weight="bold" fill="{INK}" rotate="{rotations}" dy="{offsets}" xml:space="preserve">{}</text>"#,
                escape(line)
            );
        }
        self.label(
            s,
            44.0,
            337.0,
            480.0,
            t!(self.lang, "quick-card.type-the-sentence-exactly"),
            &t.mint,
            "start",
        );
        let speed = t!(
            self.lang,
            "quick-card.speed",
            wpm = self.details.wpm.map_or_else(|| "—".into(), |n| n.to_string())
        );
        self.label(s, 695.0, 337.0, 170.0, &speed, &t.gold, "end");
        for i in 0..30 {
            rect(
                s,
                44.0 + i as f32 * 22.0,
                354.0,
                17.0,
                18.0,
                if self.details.wpm.is_some_and(|n| i * 4 < n) {
                    if i < 15 { &t.mint } else { &t.gold }
                } else {
                    &t.panel_edge
                },
            );
        }
        self.label(s, 44.0, 397.0, 120.0, "0", &t.muted, "start");
        self.label(
            s,
            695.0,
            397.0,
            160.0,
            t!(self.lang, "quick-card.speed", wpm = 120),
            &t.muted,
            "end",
        );
    }

    pub(super) fn question(&self, s: &mut String) {
        let t = &self.theme;
        panel(s, 69.0, 141.0, 610.0, 220.0, 6.0, "#2e3d75", &t.panel_edge);
        panel(s, 60.0, 132.0, 610.0, 220.0, 6.0, PAPER, "#e8d3a3");
        sprite(
            s,
            &["###.###", "#...#..", "#...#.."],
            84.0,
            154.0,
            5.0,
            "#c98a1e",
            PAPER,
        );
        self.prose(s, 370.0, 221.0, 518.0, self.prompt, 28.0, INK, 3);
        self.label(
            s,
            228.0,
            392.0,
            260.0,
            t!(self.lang, "quick-card.i-have"),
            &t.accent,
            "middle",
        );
        self.label(
            s,
            512.0,
            392.0,
            260.0,
            t!(self.lang, "quick-card.i-have-never"),
            &t.mint,
            "middle",
        );
    }
}
