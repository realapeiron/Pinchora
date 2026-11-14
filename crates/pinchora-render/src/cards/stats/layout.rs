use super::*;

impl StatsCard {
    pub(super) fn header(&self, s: &mut String) {
        let t = &self.theme;
        let chip = pixel_width(&self.period, 16.0) + 24.0;
        text(
            s,
            LEFT,
            48.0,
            16.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &escape(&truncate_pixel_to(&self.kicker, 16.0, 820.0 - chip)),
        );
        // On a plate, so the card's sparkles never land on the letters.
        crate::svg::panel(s, 864.0 - chip, 24.0, chip, 32.0, 4.0, &t.panel, &t.mint);
        text(
            s,
            864.0 - chip / 2.0,
            47.0,
            16.0,
            PIXEL,
            false,
            &t.mint,
            "middle",
            &escape(&self.period),
        );
        text(
            s,
            LEFT,
            94.0,
            30.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&truncate_to(&self.title, 30.0, 800.0)),
        );
        text(
            s,
            LEFT,
            124.0,
            16.0,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&truncate_to(&self.range, 16.0, 820.0)),
        );
    }

    pub(super) fn tiles(&self, s: &mut String, y: f32) {
        let t = &self.theme;
        let n = self.tiles.len().clamp(1, 4) as f32;
        let width = (INNER - GAP * (n - 1.0)) / n;
        for (i, tile) in self.tiles.iter().take(4).enumerate() {
            let x = LEFT + i as f32 * (width + GAP);
            t.panel(s, x, y, width, TILE_H, 4.0);
            text(
                s,
                x + 18.0,
                y + 28.0,
                12.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &escape(&truncate_pixel_to(&tile.label, 12.0, width - 36.0)),
            );
            text(
                s,
                x + 18.0,
                y + 62.0,
                28.0,
                PIXEL,
                false,
                &t.ink,
                "start",
                &escape(&truncate_pixel_to(&tile.value, 28.0, width - 36.0)),
            );
            if let Some(note) = &tile.note {
                text(
                    s,
                    x + 18.0,
                    y + 82.0,
                    13.0,
                    SANS,
                    false,
                    &t.muted,
                    "start",
                    &escape(&truncate_to(note, 13.0, width - 36.0)),
                );
            }
        }
    }

    /// The panel title and, on the right, what the top line of the scale stands for.
    pub(super) fn panel_head(&self, s: &mut String, title: &str, y: f32, right: Option<&str>) {
        let t = &self.theme;
        text(
            s,
            LEFT + 20.0,
            y + 30.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &escape(&truncate_pixel_to(title, 14.0, 560.0)),
        );
        if let Some(right) = right {
            text(
                s,
                LEFT + INNER - 20.0,
                y + 30.0,
                12.0,
                PIXEL,
                false,
                &t.muted,
                "end",
                &escape(right),
            );
        }
    }
}
