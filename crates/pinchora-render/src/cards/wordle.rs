use std::fmt::Write;

use super::{PIXEL, text};
use crate::{
    Theme,
    svg::{background, escape, mix, panel},
};

/// The fill of a tile whose letter is not in the word: a neutral grey. It used to be a slate blue so close to
/// the blue of a key nobody has tried ([`IDLE_KEY`]) that the two read as the same thing.
pub const WRONG_LETTER: &str = "#6b7186";
/// The fill of a keyboard key nobody has tried.
pub const IDLE_KEY: &str = "#2e3d75";

/// How a letter matched: used for guess tiles and keyboard keys.
pub const ABSENT: u8 = 1;
pub const PRESENT: u8 = 2;
pub const CORRECT: u8 = 3;

/// One guess: its five letters and how each matched ([`ABSENT`], [`PRESENT`] or [`CORRECT`]).
#[derive(Debug, Clone, Copy)]
pub struct WordleRow {
    pub letters: [char; 5],
    pub marks: [u8; 5],
}

/// 440×660 wordle board: a title, six rows of five tiles and a keyboard summary.
pub struct WordleCard<'a> {
    pub title: &'a str,
    /// The guesses so far, oldest first; the remaining rows of the six are drawn empty.
    pub rows: &'a [WordleRow],
    /// The keyboard summary, a row of keys at a time: each key is its capital letter and 0 for not tried, else
    /// [`ABSENT`], [`PRESENT`] or [`CORRECT`].
    pub keyboard: &'a [Vec<(char, u8)>],
    pub footer: &'a str,
    pub theme: Theme,
}

impl WordleCard<'_> {
    pub const TRIES: usize = 6;
    const TILE: f32 = 56.0;
    const GAP: f32 = 8.0;

    fn colors(&self, mark: u8) -> (String, String, String) {
        // fill, edge, letter
        let t = &self.theme;
        match mark {
            CORRECT => (t.mint.clone(), mix(&t.mint, "#000000", 0.4), "#0b1029".into()),
            PRESENT => (t.gold.clone(), mix(&t.gold, "#000000", 0.4), "#0b1029".into()),
            ABSENT => (WRONG_LETTER.into(), "#454b5e".into(), t.ink.clone()),
            _ => (t.panel.clone(), t.panel_edge.clone(), t.ink.clone()),
        }
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (440.0, 660.0);
        let mut s = String::with_capacity(32 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);

        panel(&mut s, 24.0, 20.0, 392.0, 48.0, 4.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            220.0,
            52.0,
            20.0,
            PIXEL,
            false,
            &t.gold,
            "middle",
            &escape(&self.title.to_uppercase()),
        );

        let grid_w = 5.0 * Self::TILE + 4.0 * Self::GAP;
        let x0 = (w - grid_w) / 2.0;
        for row in 0..Self::TRIES {
            for col in 0..5 {
                let (x, y) = (
                    x0 + col as f32 * (Self::TILE + Self::GAP),
                    84.0 + row as f32 * (Self::TILE + Self::GAP),
                );
                let guess = self.rows.get(row);
                let mark = guess.map_or(0, |g| g.marks[col]);
                let (fill, edge, ink) = self.colors(mark);
                panel(&mut s, x, y, Self::TILE, Self::TILE, 3.0, &fill, &edge);
                if let Some(g) = guess {
                    let _ = write!(
                        s,
                        r##"<rect x="{}" y="{}" width="{}" height="4" fill="#ffffff" fill-opacity="0.2"/>"##,
                        x + 3.0,
                        y + 3.0,
                        Self::TILE - 6.0
                    );
                    let letter = g.letters[col].to_uppercase().to_string();
                    text(
                        &mut s,
                        x + Self::TILE / 2.0,
                        y + 38.0,
                        30.0,
                        PIXEL,
                        true,
                        &ink,
                        "middle",
                        &escape(&letter),
                    );
                }
            }
        }

        // Keyboard summary.
        for (r, keys) in self.keyboard.iter().enumerate() {
            let (key_w, key_h, gap) = (34.0, 40.0, 4.0);
            let n = keys.len() as f32;
            let kx0 = (w - (n * key_w + (n - 1.0) * gap)) / 2.0;
            let ky = 484.0 + r as f32 * (key_h + 6.0);
            for (i, (letter, mark)) in keys.iter().enumerate() {
                let (fill, edge, ink) = match *mark {
                    0 => (IDLE_KEY.into(), "#1d2b53".into(), t.ink.clone()),
                    ABSENT => ("#141b3d".into(), "#0b1029".into(), t.muted.clone()),
                    other => self.colors(other),
                };
                let kx = kx0 + i as f32 * (key_w + gap);
                panel(&mut s, kx, ky, key_w, key_h, 3.0, &fill, &edge);
                text(
                    &mut s,
                    kx + key_w / 2.0,
                    ky + 27.0,
                    17.0,
                    PIXEL,
                    true,
                    &ink,
                    "middle",
                    &escape(&letter.to_uppercase().to_string()),
                );
            }
        }
        text(
            &mut s,
            220.0,
            640.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "middle",
            &escape(&self.footer.to_uppercase()),
        );
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;

    fn row(word: &str, marks: [u8; 5]) -> WordleRow {
        WordleRow {
            letters: word.chars().collect::<Vec<_>>().try_into().expect("five letters"),
            marks,
        }
    }

    #[test]
    fn colours_follow_the_marks_and_empty_rows_stay_empty() {
        let rows = [row("crane", [ABSENT, PRESENT, CORRECT, ABSENT, ABSENT])];
        let keyboard: Vec<Vec<(char, u8)>> = ["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"]
            .iter()
            .map(|row| row.chars().map(|c| (c, if c == 'A' { CORRECT } else { 0 })).collect())
            .collect();
        let card = WordleCard {
            title: "Practice",
            rows: &rows,
            keyboard: &keyboard,
            footer: "Try 2 of 6",
            theme: Theme::reef(),
        };
        let svg = card.to_svg();
        assert!(svg.contains(">PRACTICE<") && svg.contains(">C<") && svg.contains(">E<"));
        // Six rows of five tiles and 26 keys are panels; a guess adds a highlight strip per tile.
        assert_eq!(svg.matches(r##"fill-opacity="0.2""##).count(), 5);
        assert!(
            svg.contains("#06d6a0") && svg.contains("#ffd166"),
            "green and yellow tiles"
        );
        let png = Renderer::new().wordle_card(&card).expect("renders");
        let image = image::load_from_memory(&png).expect("a png");
        assert_eq!((image.width(), image.height()), (440, 660));
    }
}
