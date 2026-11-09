use super::*;

// /Would You Rather

/// 900×470 Would You Rather: the two sides, and the split once it is shown.
pub struct RatherCard<'a> {
    /// The language of the labels on the card.
    pub lang: Lang,
    pub category: &'a str,
    pub a: &'a str,
    pub b: &'a str,
    /// Votes for each side, shown as bars when the card is a result.
    pub votes: Option<(i64, i64)>,
    pub closed: bool,
    pub theme: Theme,
}

impl RatherCard<'_> {
    pub fn percent(votes: i64, total: i64) -> i64 {
        if total <= 0 {
            0
        } else {
            (votes * 100 + total / 2) / total
        }
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (WIDTH, 470.0);
        let mut s = open(w, h);
        background(&mut s, w, h, t);
        heading(&mut s, t, self.lang, &t!(self.lang, "rather.card.title"), self.category);
        let total = self.votes.map_or(0, |(a, b)| a + b);
        for (i, (label, tone)) in [(self.a, &t.accent), (self.b, &t.mint)].into_iter().enumerate() {
            let x = 36.0 + i as f32 * 424.0;
            let ahead = self
                .votes
                .is_some_and(|(a, b)| total > 0 && if i == 0 { a > b } else { b > a });
            panel(
                &mut s,
                x,
                92.0,
                404.0,
                300.0,
                6.0,
                &mix(&t.panel, tone, 0.14),
                if ahead { &t.gold } else { tone },
            );
            panel(&mut s, x + 16.0, 108.0, 54.0, 54.0, 4.0, "#0b1029", tone);
            text(
                &mut s,
                x + 43.0,
                148.0,
                32.0,
                PIXEL,
                true,
                tone,
                "middle",
                if i == 0 { "A" } else { "B" },
            );
            if ahead {
                sprite(&mut s, STAR, x + 404.0 - 52.0, 118.0, 4.0, &t.gold, "#fff1e8");
            }
            for (j, line) in wrap(label, 21, 5).iter().enumerate() {
                text(
                    &mut s,
                    x + 202.0,
                    208.0 + j as f32 * 34.0,
                    26.0,
                    SANS,
                    true,
                    &t.ink,
                    "middle",
                    &escape(&truncate_to(line, 26.0, 372.0)),
                );
            }
            if let Some((a, b)) = self.votes {
                let votes = if i == 0 { a } else { b };
                let share = Self::percent(votes, total);
                text(
                    &mut s,
                    x + 202.0,
                    372.0,
                    34.0,
                    // The pixel font has no good percent sign at this size.
                    SANS,
                    true,
                    tone,
                    "middle",
                    &format!("{share}%"),
                );
                let bar = 372.0 * (share as f32 / 100.0);
                rect(&mut s, x + 16.0, 380.0, 372.0, 6.0, "#0b1029");
                rect(&mut s, x + 16.0, 380.0, bar, 6.0, tone);
            }
        }
        let line = match self.votes {
            Some((a, b)) if self.closed => t!(self.lang, "rather.card.final", count = a + b),
            Some(_) => t!(self.lang, "rather.card.so-far"),
            None => t!(self.lang, "rather.card.pick"),
        };
        footer(&mut s, t, self.lang, 430.0, &line);
        s.push_str("</svg>");
        s
    }
}

// /Truth or Dare

/// What a prompt card is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTone {
    /// Somebody's turn to pick.
    Turn,
    Truth,
    Dare,
}

/// 900-wide card of Truth or Dare: whose turn it is, and the truth or the dare they got.
pub struct PromptCard<'a> {
    /// The language of the labels on the card.
    pub lang: Lang,
    pub tone: PromptTone,
    pub name: &'a str,
    /// The truth or dare; for a turn card, a line asking which they pick.
    pub text: &'a str,
    pub category: &'a str,
    pub footer: &'a str,
    pub theme: Theme,
}

impl PromptCard<'_> {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (badge, tone) = match self.tone {
            PromptTone::Turn => (t!(self.lang, "dare.card.turn"), t.gold.clone()),
            PromptTone::Truth => (t!(self.lang, "dare.card.truth"), t.mint.clone()),
            PromptTone::Dare => (t!(self.lang, "dare.card.dare"), t.accent.clone()),
        };
        let lines = wrap_laid_out(self.text, SANS, true, 30.0, 780.0, 4);
        let h = 250.0 + lines.len() as f32 * 44.0;
        let mut s = open(WIDTH, h);
        background(&mut s, WIDTH, h, t);
        let chip = pixel_width(&badge, 22.0) + 40.0;
        panel(&mut s, 36.0, 32.0, chip, 48.0, 4.0, &mix(&t.panel, &tone, 0.2), &tone);
        text(
            &mut s,
            36.0 + chip / 2.0,
            65.0,
            22.0,
            PIXEL,
            true,
            &tone,
            "middle",
            &escape(&badge),
        );
        if !self.category.is_empty() {
            let c = pixel_width(self.category, 15.0) + 28.0;
            panel(&mut s, WIDTH - 36.0 - c, 38.0, c, 36.0, 4.0, &t.panel, &t.panel_edge);
            text(
                &mut s,
                WIDTH - 36.0 - c / 2.0,
                62.0,
                15.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &escape(&self.lang.upper(self.category)),
            );
        }
        text(
            &mut s,
            36.0,
            132.0,
            26.0,
            SANS,
            true,
            &t.gold,
            "start",
            &escape(&truncate_to(self.name, 26.0, 760.0)),
        );
        panel(
            &mut s,
            36.0,
            152.0,
            828.0,
            24.0 + lines.len() as f32 * 44.0 + 20.0,
            4.0,
            &t.panel,
            &tone,
        );
        for (i, line) in lines.iter().enumerate() {
            text(
                &mut s,
                60.0,
                200.0 + i as f32 * 44.0,
                30.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(line),
            );
        }
        footer(&mut s, t, self.lang, h - 20.0, self.footer);
        s.push_str("</svg>");
        s
    }
}
