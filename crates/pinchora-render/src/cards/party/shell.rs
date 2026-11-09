use super::*;

// /Shell game

/// A shell lying on the sand, one colour and a lighter one, 14 wide.
const CUP: &[&str] = &[
    "....######....",
    "..##oooooo##..",
    ".#oo#o##o#oo#.",
    ".#o#oo##oo#o#.",
    "#oo#oo##oo#oo#",
    "#o#ooo##ooo#o#",
    "##############",
];

/// A pearl.
const PEARL: &[&str] = &["..###..", ".#ooo#.", "#ooooo#", "#ooooo#", ".#ooo#.", "..###.."];

/// 520×320 shell game: three shells on a table, one with the pearl under it.
pub struct ShellCard<'a> {
    /// The language of the labels; the status comes translated, and in capitals.
    pub lang: Lang,
    pub stake: i64,
    /// The shell the player picked.
    pub picked: Option<usize>,
    /// Where the pearl was, once it is shown.
    pub pearl: Option<usize>,
    pub status: &'a str,
    /// Shells paid for a right guess, shown when the game is over.
    pub payout: i64,
    pub theme: Theme,
}

impl ShellCard<'_> {
    pub const WIDTH: f32 = 520.0;
    pub const HEIGHT: f32 = 320.0;

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
            &t!(self.lang, "shells.card.title"),
        );
        let label = t!(
            self.lang,
            "shells.card.stake",
            stake = compact_in(self.lang, self.stake),
            payout = compact_in(self.lang, self.payout.max(0))
        );
        chip(&mut s, w - 28.0, 26.0, &label, &t.gold, t);
        // The table.
        panel(&mut s, 24.0, 84.0, 472.0, 150.0, 4.0, "#8a5a34", "#5b3a20");
        for plank in 0..4 {
            let y = 92.0 + plank as f32 * 36.0;
            let _ = write!(
                s,
                r##"<rect x="32" y="{y}" width="456" height="3" fill="#6f4627"/><rect x="32" y="{}" width="456" height="3" fill="#a06b3f"/>"##,
                y + 3.0
            );
        }
        for shell in 0..3usize {
            let x = 52.0 + shell as f32 * 150.0;
            let lifted = self.pearl.is_some();
            let mine = self.picked == Some(shell);
            // The pick is green when the pearl was under it, red when it was not.
            let (body, light) = match (mine, self.pearl == self.picked) {
                (true, true) => (t.mint.as_str(), "#b4f5df"),
                (true, false) => (t.accent.as_str(), "#ffb9a8"),
                (false, _) => ("#d8a7c8", "#f6e0ee"),
            };
            let y = if lifted { 112.0 } else { 134.0 };
            if self.pearl == Some(shell) {
                // The pearl sits where the shell was.
                sprite(&mut s, PEARL, x + 33.0, 176.0, 6.0, "#9aa6c8", "#fff7e0");
            }
            // The shell shadow.
            let _ = write!(
                s,
                r##"<rect x="{}" y="214" width="{}" height="6" fill="#000000" fill-opacity="0.25"/>"##,
                x + 8.0,
                84.0
            );
            sprite(&mut s, CUP, x, y, 7.0, body, light);
            let number = (shell + 1).to_string();
            let tone = if mine { &t.gold } else { &t.muted };
            text(&mut s, x + 49.0, 262.0, 20.0, PIXEL, false, tone, "middle", &number);
            if mine {
                text(
                    &mut s,
                    x + 49.0,
                    282.0,
                    12.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "middle",
                    &t!(self.lang, "shells.card.your-pick"),
                );
            }
        }
        // The status sits on a plate of its own, so the card's fixed sparkles never land beside the words.
        panel(&mut s, 24.0, 288.0, 472.0, 26.0, 4.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            w / 2.0,
            306.0,
            16.0,
            PIXEL,
            false,
            &t.ink,
            "middle",
            &escape(&self.lang.upper(self.status)),
        );
        s.push_str("</svg>");
        s
    }
}
