use super::*;

impl QuickCard<'_> {
    pub(super) fn coordinates(&self, s: &mut String, x: f32, y: f32, size: f32, n: usize) {
        let color = boards::coordinate_color(&self.theme);
        for i in 0..n {
            text(
                s,
                x + (i as f32 + 0.5) * size,
                y - 10.0,
                14.0,
                PIXEL,
                false,
                &color,
                "middle",
                &((b'A' + i as u8) as char).to_string(),
            );
            text(
                s,
                x - 10.0,
                y + (i as f32 + 0.5) * size + 5.0,
                14.0,
                PIXEL,
                false,
                &color,
                "middle",
                &(i + 1).to_string(),
            );
        }
    }

    pub(super) fn othello(&self, s: &mut String) {
        let t = &self.theme;
        for side in 0..2 {
            let count = self
                .cells
                .iter()
                .filter(|c| c.as_str() == if side == 0 { "O" } else { "X" })
                .count();
            let active = self.details.turn == Some(side);
            let won = self.details.winner == Some(side);
            let state = if won {
                t!(self.lang, "quick-card.winner")
            } else if active {
                t!(self.lang, "quick-card.your-move")
            } else {
                t!(self.lang, "quick-card.pearls")
            };
            let note = format!("{count:02} · {}", self.lang.upper(&state));
            let name = self.details.names[side as usize];
            boards::player_panel(
                s,
                t,
                (24.0 + side as f32 * 270.0, 110.0),
                (active, won),
                &if name.is_empty() {
                    if side == 0 {
                        t!(self.lang, "quick-card.white-pearl")
                    } else {
                        t!(self.lang, "quick-card.black-pearl")
                    }
                } else {
                    name.to_owned()
                },
                (&escape(&note), if active { &t.mint } else { &t.gold }),
                |s, x, y| pearl(s, x, y + 3.0, 3.0, side == 0),
            );
        }
        panel(s, 30.0, 202.0, 520.0, 496.0, 6.0, "#1a2a66", &t.panel_edge);
        let (ox, oy, size) = (58.0, 230.0, 58.0);
        self.coordinates(s, ox, oy, size, 8);
        for (i, cell) in self.cells.iter().take(64).enumerate() {
            let (x, y) = (ox + (i % 8) as f32 * size, oy + (i / 8) as f32 * size);
            let base = if (i / 8 + i % 8) % 2 == 0 { "#245658" } else { "#1c454c" };
            rect(s, x, y, size, size, base);
            rect(s, x, y, size, 1.0, "#387074");
            rect(s, x, y, 1.0, size, "#387074");
            if self.details.last == Some(i) {
                rect(s, x, y, size, size, &mix(base, &t.gold, 0.4));
            }
            match cell.as_str() {
                "O" | "X" => pearl(s, x + 8.0, y + 8.0, 3.0, cell == "O"),
                "*" if !self.details.finished => {
                    let _ = write!(s, r#"<g opacity="0.45">"#);
                    pearl(s, x + 22.0, y + 22.0, 1.0, self.details.turn == Some(0));
                    s.push_str("</g>");
                    rim(s, x + 20.0, y + 20.0, 18.0, &t.mint);
                }
                _ => {}
            }
        }
        self.label(
            s,
            290.0,
            714.0,
            500.0,
            if self.details.finished {
                t!(self.lang, "quick-card.final-pearl-count")
            } else {
                t!(self.lang, "quick-card.glowing-dots-are-legal-moves")
            },
            &t.muted,
            "middle",
        );
    }
}
