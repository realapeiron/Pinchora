use super::*;

// /Heist

/// A member of the heist crew, as the card lists them.
pub struct CrewView<'a> {
    pub name: &'a str,
    /// The role's number: 0 lookout, 1 locksmith, 2 diver.
    pub role: usize,
}

/// One stage of the heist.
pub struct StageView<'a> {
    pub title: &'a str,
    /// The crew's chance, in percent.
    pub chance: u8,
    /// `None` before the heist runs.
    pub passed: Option<bool>,
    pub line: &'a str,
}

/// The crew's roles, as the key of a short word (`heist.card.<key>`) and a pixel icon colour each.
const ROLE_TAGS: [(&str, &str); 3] = [("lookout", "#8fd3ff"), ("locksmith", "#ffd166"), ("diver", "#06d6a0")];

/// A round vault door in two layers: the steel, then the dial in the middle.
const VAULT_STEEL: &[&str] = &[
    "....#######....",
    "..###ooooo###..",
    ".##ooooooooo##.",
    ".#ooooooooooo#.",
    "##ooooooooooo##",
    "#ooooooooooooo#",
    "#ooooooooooooo#",
    "#ooooooooooooo#",
    "#ooooooooooooo#",
    "#ooooooooooooo#",
    "##ooooooooooo##",
    ".#ooooooooooo#.",
    ".##ooooooooo##.",
    "..###ooooo###..",
    "....#######....",
];
const VAULT_DIAL: &[&str] = &[
    "...............",
    "...............",
    "...............",
    "...o#######o...",
    "....#.....#....",
    "...#...#...#...",
    "...#...#...#...",
    "...#.#####.#...",
    "...#...#...#...",
    "...#...#...#...",
    "....#.....#....",
    "...o#######o...",
    "...............",
    "...............",
    "...............",
];

/// 600 wide heist card: the vault, the three stages with the crew's chance at each, and who is in. The
/// crew's panel is only as tall as its rows (two members a row), so the card is 476 to 560 high.
pub struct HeistCard<'a> {
    /// The language of the labels; the headline, the stage titles and their lines come translated.
    pub lang: Lang,
    pub crew: &'a [CrewView<'a>],
    pub stages: [StageView<'a>; 3],
    pub headline: &'a str,
    /// Shells each member takes home; 0 hides it.
    pub each: i64,
    pub theme: Theme,
}

impl HeistCard<'_> {
    pub const WIDTH: f32 = 600.0;
    /// The most members the card lists.
    const SEATS: usize = 8;

    /// Rows of crew (two to a row) the card makes room for; an empty crew still gets one.
    fn rows(&self) -> usize {
        self.crew.len().clamp(1, Self::SEATS).div_ceil(2)
    }

    /// The card's height: the vault and stages, then a crew panel that fits its rows.
    pub fn height(&self) -> f32 {
        476.0 + (self.rows() - 1) as f32 * 28.0
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (Self::WIDTH, self.height());
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
            &t!(self.lang, "heist.card.title"),
        );
        let opened = self.stages.iter().all(|stage| stage.passed == Some(true));
        let done = self.stages.iter().all(|stage| stage.passed.is_some());
        let busted = done && self.stages.iter().all(|stage| stage.passed == Some(false));
        // Gold for the open vault, red only for a crew that got nothing: a good haul is good news.
        chip(
            &mut s,
            w - 28.0,
            26.0,
            self.headline,
            if opened {
                &t.gold
            } else if busted {
                &t.accent
            } else {
                &t.mint
            },
            t,
        );
        self.draw_vault(&mut s, t, opened, done);
        self.draw_stages(&mut s, t);
        self.draw_crew(&mut s, t, h);
        s.push_str("</svg>");
        s
    }
    fn draw_vault(&self, s: &mut String, t: &Theme, opened: bool, done: bool) {
        // The vault: steel, gold once it is open.
        panel(
            s,
            24.0,
            84.0,
            188.0,
            280.0,
            4.0,
            &t.panel,
            if opened { &t.gold } else { &t.panel_edge },
        );
        let (steel, shine, dial) = if opened {
            ("#8a5a10", "#e3a21e", "#fff1e8")
        } else if done {
            ("#2a3566", "#3a4a85", "#8fa0d8")
        } else {
            ("#2e3d75", "#4a5ba3", "#ffd166")
        };
        sprite(s, VAULT_STEEL, 38.0, 124.0, 10.0, steel, shine);
        sprite(s, VAULT_DIAL, 38.0, 124.0, 10.0, dial, "#ffffff");
        text(
            s,
            118.0,
            330.0,
            14.0,
            PIXEL,
            false,
            if opened { &t.gold } else { &t.muted },
            "middle",
            &if opened {
                t!(self.lang, "heist.card.open")
            } else if done {
                t!(self.lang, "heist.card.shut")
            } else {
                t!(self.lang, "heist.card.sealed")
            },
        );
        if opened {
            // Loot spills out of an open vault.
            sprite(s, STAR, 164.0, 100.0, 4.0, &t.gold, "#fff1e8");
            sprite(s, SHELL, 40.0, 104.0, 3.0, &t.gold, "#ffffff");
            sprite(s, SHELL, 176.0, 336.0, 3.0, &t.gold, "#ffffff");
        }
    }

    fn draw_stages(&self, s: &mut String, t: &Theme) {
        // The stages, each with the crew's chance as a bar under its name.
        for (n, stage) in self.stages.iter().enumerate() {
            let y = 84.0 + n as f32 * 96.0;
            let (edge, mark, tone) = match stage.passed {
                Some(true) => (&t.mint, t!(self.lang, "heist.card.passed"), &t.mint),
                Some(false) => (&t.accent, t!(self.lang, "heist.card.failed"), &t.accent),
                None => (&t.panel_edge, t!(self.lang, "heist.card.to-come"), &t.muted),
            };
            panel(s, 224.0, y, 352.0, 88.0, 4.0, &t.panel, edge);
            text(
                s,
                240.0,
                y + 19.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &t!(self.lang, "heist.card.stage", n = n + 1),
            );
            text(s, 560.0, y + 19.0, 14.0, PIXEL, false, tone, "end", &mark);
            text(
                s,
                240.0,
                y + 43.0,
                18.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(stage.title),
            );
            text(
                s,
                560.0,
                y + 42.0,
                12.0,
                PIXEL,
                false,
                &t.muted,
                "end",
                &t!(self.lang, "heist.card.chance", chance = stage.chance),
            );
            let _ = write!(
                s,
                r##"<rect x="240" y="{by}" width="320" height="6" fill="#0b1029"/><rect x="240" y="{by}" width="{}" height="6" fill="{tone}"/>"##,
                320.0 * f32::from(stage.chance.min(100)) / 100.0,
                by = y + 49.0,
            );
            for (row, line) in wrap(stage.line, 46, 2).iter().enumerate() {
                text(
                    s,
                    240.0,
                    y + 68.0 + row as f32 * 12.0,
                    12.0,
                    SANS,
                    false,
                    &t.muted,
                    "start",
                    &escape(&truncate_to(line, 12.0, 318.0)),
                );
            }
        }
    }

    fn draw_crew(&self, s: &mut String, t: &Theme, h: f32) {
        // The crew.
        panel(s, 24.0, 378.0, 552.0, h - 378.0 - 24.0, 4.0, &t.panel, &t.panel_edge);
        text(
            s,
            44.0,
            406.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &t!(self.lang, "heist.card.crew", count = self.crew.len()),
        );
        if self.each > 0 {
            let label = t!(
                self.lang,
                "heist.card.each",
                amount = thousands_in(self.lang, self.each)
            );
            sprite(
                s,
                SHELL,
                556.0 - pixel_width(&label, 14.0) - 30.0,
                392.0,
                2.0,
                &t.gold,
                "#ffffff",
            );
            text(s, 556.0, 406.0, 14.0, PIXEL, false, &t.gold, "end", &label);
        }
        for (n, member) in self.crew.iter().take(Self::SEATS).enumerate() {
            let (col, row) = ((n % 2) as f32, (n / 2) as f32);
            let (x, y) = (44.0 + col * 266.0, 418.0 + row * 28.0);
            let (tag, color) = ROLE_TAGS[member.role.min(2)];
            let _ = write!(
                s,
                r#"<path d="{}" fill="{color}"/>"#,
                notched_path(x, y + 2.0, 10.0, 20.0, 2.0)
            );
            text(
                s,
                x + 20.0,
                y + 18.0,
                16.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(member.name, 16.0, 120.0)),
            );
            let tag = t!(self.lang, &format!("heist.card.{tag}"));
            text(s, x + 250.0, y + 17.0, 12.0, PIXEL, false, color, "end", &tag);
        }
        if self.crew.is_empty() {
            text(
                s,
                300.0,
                438.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &t!(self.lang, "heist.card.nobody"),
            );
        }
    }
}
