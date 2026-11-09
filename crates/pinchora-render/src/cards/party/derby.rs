use super::*;

// /Derby

/// One lane of the derby lobby.
pub struct DerbyLane<'a> {
    pub name: &'a str,
    /// The lane colour, `0xrrggbb`.
    pub accent: u32,
    /// The crab, already scaled 2×.
    pub art: SpriteArt<'a>,
    /// The names of the players who backed it, in the order they joined.
    pub backers: &'a [String],
}

/// 600×470 derby lobby: the six crabs on their lanes, who backs each, and what is at stake.
pub struct DerbyCard<'a> {
    /// The language of the labels; the status comes translated, and in capitals.
    pub lang: Lang,
    /// The six crabs, in lane order.
    pub lanes: &'a [DerbyLane<'a>],
    /// Shells each racer put up; 0 for a friendly race.
    pub stake: i64,
    pub racers: usize,
    /// What the card says it is: `PICK YOUR CRAB`, `THEY ARE OFF`.
    pub status: &'a str,
    pub theme: Theme,
}

impl DerbyCard<'_> {
    pub const WIDTH: f32 = 600.0;
    pub const HEIGHT: f32 = 470.0;

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
            &t!(self.lang, "derby.card.title"),
        );
        if self.stake > 0 {
            let pool = self.stake * self.racers as i64;
            let label = t!(self.lang, "derby.card.pool", pool = compact_in(self.lang, pool));
            chip(&mut s, w - 28.0, 26.0, &label, &t.gold, t);
            sprite(&mut s, SHELL, 28.0, 68.0, 3.0, &t.gold, "#ffffff");
            text(
                &mut s,
                56.0,
                86.0,
                16.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &t!(self.lang, "derby.card.entry", stake = compact_in(self.lang, self.stake)),
            );
        } else {
            chip(&mut s, w - 28.0, 26.0, &t!(self.lang, "derby.card.for-fun"), &t.mint, t);
            text(
                &mut s,
                28.0,
                86.0,
                16.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &t!(self.lang, "derby.card.no-stakes"),
            );
        }
        for (n, lane) in self.lanes.iter().take(6).enumerate() {
            let (col, row) = ((n % 2) as f32, (n / 2) as f32);
            let (x, y) = (24.0 + col * 280.0, 104.0 + row * 112.0);
            let accent = hex(lane.accent);
            panel(
                &mut s,
                x,
                y,
                272.0,
                104.0,
                4.0,
                &t.panel,
                &mix(&accent, &t.panel_edge, 0.45),
            );
            let _ = write!(
                s,
                r#"<path d="{}" fill="{accent}"/>"#,
                notched_path(x + 6.0, y + 6.0, 10.0, 92.0, 2.0)
            );
            art(&mut s, &lane.art, x + 26.0, y + 18.0);
            text(
                &mut s,
                x + 112.0,
                y + 36.0,
                16.0,
                PIXEL,
                false,
                &accent,
                "start",
                &t!(self.lang, "derby.card.lane", n = n + 1),
            );
            text(
                &mut s,
                x + 112.0,
                y + 62.0,
                22.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(lane.name, 22.0, 150.0)),
            );
            let backers = lane.backers.len();
            let line = match backers {
                0 => t!(self.lang, "derby.card.no-backers"),
                n => t!(self.lang, "derby.card.backers", count = n),
            };
            let tone = if backers == 0 { &t.muted } else { &t.mint };
            text(&mut s, x + 112.0, y + 82.0, 14.0, PIXEL, false, tone, "start", &line);
            if let Some(first) = lane.backers.first() {
                let more = if backers > 1 {
                    format!(" +{}", backers - 1)
                } else {
                    String::new()
                };
                let shown = truncate_to(first, 13.0, 140.0 - 8.0 * more.len() as f32);
                text(
                    &mut s,
                    x + 112.0,
                    y + 98.0,
                    13.0,
                    SANS,
                    false,
                    &t.muted,
                    "start",
                    &escape(&format!("{shown}{more}")),
                );
            }
        }
        panel(&mut s, 24.0, 432.0, 552.0, 28.0, 4.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            w / 2.0,
            452.0,
            14.0,
            PIXEL,
            false,
            &t.gold,
            "middle",
            &escape(&self.lang.upper(self.status)),
        );
        s.push_str("</svg>");
        s
    }
}
