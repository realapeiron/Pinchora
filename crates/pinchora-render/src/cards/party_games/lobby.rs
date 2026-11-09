use super::*;

// /Lobby

/// 900-wide lobby: what the game is, who has joined, and how many more it needs.
pub struct PartyLobbyCard<'a> {
    /// The language of the labels on the card.
    pub lang: Lang,
    pub title: &'a str,
    pub tagline: &'a str,
    pub players: &'a [String],
    pub min: usize,
    pub max: usize,
    pub footer: &'a str,
    pub theme: Theme,
}

impl PartyLobbyCard<'_> {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let max = self.max.clamp(2, 10);
        let rows = max.div_ceil(2);
        let lines = wrap(self.tagline, 62, 2);
        let top = 96.0 + lines.len() as f32 * 28.0 + 12.0;
        let h = top + rows as f32 * 56.0 + 70.0;
        let mut s = open(WIDTH, h);
        background(&mut s, WIDTH, h, t);
        heading(
            &mut s,
            t,
            self.lang,
            self.title,
            &format!("{}/{}", self.players.len(), max),
        );
        for (i, line) in lines.iter().enumerate() {
            text(
                &mut s,
                36.0,
                100.0 + i as f32 * 28.0,
                19.0,
                SANS,
                false,
                &t.muted,
                "start",
                &escape(&truncate_to(line, 19.0, 828.0)),
            );
        }
        for seat in 0..max {
            let (col, row) = ((seat % 2) as f32, (seat / 2) as f32);
            let (x, y) = (36.0 + col * 424.0, top + row * 56.0);
            let name = self.players.get(seat);
            let edge = if seat == 0 && name.is_some() {
                &t.gold
            } else {
                &t.panel_edge
            };
            panel(&mut s, x, y, 404.0, 48.0, 4.0, &t.panel, edge);
            text(
                &mut s,
                x + 18.0,
                y + 32.0,
                16.0,
                PIXEL,
                false,
                if name.is_some() { &t.gold } else { &t.panel_edge },
                "start",
                &format!("{}", seat + 1),
            );
            match name {
                Some(name) => {
                    text(
                        &mut s,
                        x + 52.0,
                        y + 32.0,
                        20.0,
                        SANS,
                        true,
                        &t.ink,
                        "start",
                        &escape(&truncate_to(name, 20.0, 300.0)),
                    );
                    if seat == 0 {
                        sprite(&mut s, STAR, x + 404.0 - 40.0, y + 14.0, 3.0, &t.gold, "#fff1e8");
                    }
                }
                None => text(
                    &mut s,
                    x + 52.0,
                    y + 31.0,
                    17.0,
                    SANS,
                    false,
                    &mix(&t.muted, &t.bg_bottom, 0.5),
                    "start",
                    &t!(self.lang, "party.card.open-seat"),
                ),
            }
        }
        let need = self.min.saturating_sub(self.players.len());
        let line = if need == 0 {
            self.footer.to_owned()
        } else {
            t!(self.lang, "party.card.waiting", count = need)
        };
        footer(&mut s, t, self.lang, h - 26.0, &line);
        s.push_str("</svg>");
        s
    }
}

// /Hot Pearl

/// One player at the Hot Pearl table.
pub struct PearlSeat<'a> {
    pub name: &'a str,
    pub lives: i32,
    pub max_lives: i32,
    pub out: bool,
    /// Holds the pearl.
    pub current: bool,
}

/// 900×560 Hot Pearl table: the pearl with its syllable in the middle, the players round it.
pub struct PearlCard<'a> {
    /// The language of the labels on the card.
    pub lang: Lang,
    pub syllable: &'a str,
    pub seats: &'a [PearlSeat<'a>],
    /// The line under the pearl: whose turn it is and for how long, or what just happened.
    pub banner: &'a str,
    /// The last word played, if any.
    pub last_word: Option<&'a str>,
    pub theme: Theme,
}

impl PearlCard<'_> {
    const CHIP: (f32, f32) = (204.0, 56.0);

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (WIDTH, 560.0);
        let mut s = open(w, h);
        background(&mut s, w, h, t);
        let (cx, cy) = (450.0, 262.0);

        // A table of dusk-coloured water under the pearl.
        let _ = write!(
            s,
            r##"<ellipse cx="{cx}" cy="{cy}" rx="250" ry="170" fill="#0b1029" fill-opacity="0.45"/><ellipse cx="{cx}" cy="{cy}" rx="250" ry="170" fill="none" stroke="{}" stroke-width="4" stroke-dasharray="10 10"/>"##,
            t.panel_edge
        );

        // The pearl: a pixel disc, a lit side, a shadow and a few glints, with a fuse burning on top.
        pixel_disc(&mut s, cx, cy + 6.0, 82.0, 6.0, "#8f84c4");
        pixel_disc(&mut s, cx - 2.0, cy + 2.0, 78.0, 6.0, "#d9d0f0");
        pixel_disc(&mut s, cx - 8.0, cy - 6.0, 62.0, 6.0, "#f1ecfb");
        rect(&mut s, cx - 52.0, cy - 54.0, 18.0, 12.0, "#ffffff");
        rect(&mut s, cx - 34.0, cy - 60.0, 12.0, 6.0, "#ffffff");
        rect(&mut s, cx - 62.0, cy - 36.0, 6.0, 12.0, "#ffffff");
        rect(&mut s, cx - 6.0, cy - 112.0, 6.0, 30.0, "#c98a1e");
        rect(&mut s, cx - 6.0, cy - 112.0, 6.0, 6.0, "#ffd166");
        sprite(&mut s, STAR, cx - 15.0, cy - 138.0, 4.0, &t.accent, "#ffd166");
        let syllable = self.lang.upper(self.syllable);
        text(
            &mut s,
            cx - 2.0,
            cy + 18.0,
            46.0,
            PIXEL,
            true,
            "#2e3d75",
            "middle",
            &escape(&syllable),
        );

        // The players on an ellipse, starting at the top and going clockwise.
        let n = self.seats.len().max(1) as f32;
        for (i, seat) in self.seats.iter().enumerate() {
            let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / n;
            let (px, py) = (cx + 330.0 * angle.cos(), cy + 190.0 * angle.sin());
            let (cw, ch) = Self::CHIP;
            let (x, y) = ((px - cw / 2.0).round(), (py - ch / 2.0).round());
            let (fill, edge) = match (seat.out, seat.current) {
                (true, _) => (mix(&t.panel, "#000000", 0.35), "#262f55".to_owned()),
                (_, true) => (mix(&t.panel, &t.gold, 0.25), t.gold.clone()),
                _ => (t.panel.clone(), t.panel_edge.clone()),
            };
            panel(&mut s, x, y, cw, ch, 4.0, &fill, &edge);
            let ink = if seat.out { &t.muted } else { &t.ink };
            text(
                &mut s,
                x + 14.0,
                y + 25.0,
                17.0,
                SANS,
                true,
                ink,
                "start",
                &escape(&truncate_to(seat.name, 17.0, cw - 28.0)),
            );
            if seat.out {
                text(
                    &mut s,
                    x + 14.0,
                    y + 46.0,
                    12.0,
                    PIXEL,
                    false,
                    &t.muted,
                    "start",
                    &t!(self.lang, "hotpearl.card.out"),
                );
            } else {
                hearts(&mut s, t, x + cw - 10.0, y + 31.0, seat.lives, seat.max_lives);
                if seat.current {
                    text(
                        &mut s,
                        x + 14.0,
                        y + 47.0,
                        11.0,
                        PIXEL,
                        false,
                        &t.gold,
                        "start",
                        &t!(self.lang, "hotpearl.card.holds"),
                    );
                }
            }
        }

        // Under the table, where the players' seats cannot reach.
        text(
            &mut s,
            cx,
            516.0,
            15.0,
            PIXEL,
            false,
            &t.gold,
            "middle",
            &escape(&truncate_pixel(&self.lang.upper(self.banner), 15.0, 840.0)),
        );
        if let Some(word) = self.last_word {
            text(
                &mut s,
                cx,
                540.0,
                13.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &escape(&t!(self.lang, "hotpearl.card.last-word", word = self.lang.upper(word))),
            );
        }
        s.push_str("</svg>");
        s
    }
}

pub(super) fn truncate_pixel(text: &str, size: f32, max: f32) -> String {
    crate::svg::truncate_pixel_to(text, size, max)
}

// /Standings

/// One line of the standings.
pub struct StandingRow<'a> {
    pub rank: u32,
    pub name: &'a str,
    /// What they scored: `5 pts`, `lives 2`, `SOLVED IN 3`.
    pub score: &'a str,
    /// What it paid, or a note: `+34 shells`.
    pub note: &'a str,
    pub winner: bool,
}

/// 900-wide standings: a ranked list with a star on the winner, the end of every party game.
pub struct StandingsCard<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub rows: &'a [StandingRow<'a>],
    pub footer: &'a str,
    pub theme: Theme,
}

impl StandingsCard<'_> {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let h = 150.0 + self.rows.len().min(10) as f32 * 62.0 + 40.0;
        let mut s = open(WIDTH, h);
        background(&mut s, WIDTH, h, t);
        heading(&mut s, t, Lang::En, self.title, "");
        text(
            &mut s,
            36.0,
            102.0,
            19.0,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&truncate_to(self.subtitle, 19.0, 828.0)),
        );
        for (i, row) in self.rows.iter().take(10).enumerate() {
            let y = 126.0 + i as f32 * 62.0;
            let edge = if row.winner { &t.gold } else { &t.panel_edge };
            let fill = if row.winner {
                mix(&t.panel, &t.gold, 0.12)
            } else {
                t.panel.clone()
            };
            panel(&mut s, 36.0, y, 828.0, 54.0, 4.0, &fill, edge);
            text(
                &mut s,
                70.0,
                y + 36.0,
                22.0,
                PIXEL,
                false,
                edge,
                "middle",
                &format!("{}", row.rank),
            );
            let mut x = 106.0;
            if row.winner {
                sprite(&mut s, STAR, x, y + 14.0, 3.4, &t.gold, "#fff1e8");
                x += 34.0;
            }
            text(
                &mut s,
                x,
                y + 35.0,
                22.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(row.name, 22.0, 400.0 - (x - 106.0))),
            );
            text(
                &mut s,
                840.0,
                y + 24.0,
                16.0,
                PIXEL,
                false,
                &t.ink,
                "end",
                &escape(&Lang::En.upper(row.score)),
            );
            if !row.note.is_empty() {
                text(
                    &mut s,
                    840.0,
                    y + 45.0,
                    13.0,
                    PIXEL,
                    false,
                    &t.mint,
                    "end",
                    &escape(&Lang::En.upper(row.note)),
                );
            }
        }
        footer(&mut s, t, Lang::En, h - 16.0, self.footer);
        s.push_str("</svg>");
        s
    }
}

// /Acronym letters

/// 900×380 prompt of Acronym Tide: the letters to build a phrase from.
pub struct LettersCard<'a> {
    pub title: &'a str,
    pub round: &'a str,
    pub letters: &'a [char],
    pub hint: &'a str,
    pub status: &'a str,
    pub theme: Theme,
}

impl LettersCard<'_> {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (WIDTH, 380.0);
        let mut s = open(w, h);
        background(&mut s, w, h, t);
        heading(&mut s, t, Lang::En, self.title, self.round);
        let n = self.letters.len().max(1) as f32;
        let (tile, gap) = (120.0, 22.0);
        let x0 = (w - (n * tile + (n - 1.0) * gap)) / 2.0;
        for (i, letter) in self.letters.iter().enumerate() {
            let x = x0 + i as f32 * (tile + gap);
            panel(&mut s, x, 100.0, tile, tile, 6.0, &t.panel, &t.gold);
            rect(&mut s, x + 6.0, 106.0, tile - 12.0, 6.0, "#ffffff");
            let _ = write!(
                s,
                r##"<rect x="{}" y="106" width="{}" height="6" fill="#ffffff" fill-opacity="0.18"/>"##,
                x + 6.0,
                tile - 12.0
            );
            text(
                &mut s,
                x + tile / 2.0,
                100.0 + 84.0,
                68.0,
                PIXEL,
                true,
                &t.gold,
                "middle",
                &escape(&letter.to_ascii_uppercase().to_string()),
            );
            if i + 1 < self.letters.len() {
                sprite(&mut s, SHELL, x + tile + 3.0, 148.0, 2.4, &t.accent, "#ffd0c0");
            }
        }
        for (i, line) in wrap(self.hint, 66, 2).iter().enumerate() {
            text(
                &mut s,
                w / 2.0,
                270.0 + i as f32 * 28.0,
                20.0,
                SANS,
                false,
                &t.ink,
                "middle",
                &escape(&truncate_to(line, 20.0, 828.0)),
            );
        }
        footer(&mut s, t, Lang::En, 350.0, self.status);
        s.push_str("</svg>");
        s
    }
}
