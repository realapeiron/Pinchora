use std::fmt::Write;

use super::{PIXEL, text};
use crate::{
    Theme,
    svg::{SHELL, background, escape, panel, sprite},
};

/// A letter of the word on the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HangmanLetter {
    /// Not found yet: an empty slot.
    Hidden,
    /// Found by a guess.
    Found(char),
    /// Never found; shown when the game is lost.
    Missed(char),
}

/// 720×300 hangman card: a sandcastle the tide takes one wave at a time, the word, and the lives left.
pub struct HangmanCard<'a> {
    /// The word, one line per word.
    pub lines: &'a [Vec<HangmanLetter>],
    pub category: &'a str,
    /// Wrong letters so far, in order.
    pub wrong: &'a [char],
    /// 0 to 6; how far the sea has come.
    pub misses: usize,
    pub status: &'a str,
    pub theme: Theme,
}

const SCENE_W: usize = 50;
const SCENE_H: usize = 30;
const SCALE: f32 = 6.0;

const SKY: [&str; 4] = ["#1d2b53", "#2e3d75", "#46579a", "#6b7fc0"];
const SUN: &str = "#ffd166";
const FAR_SEA: &str = "#2a5d9f";
const FAR_SEA_LIGHT: &str = "#4a86c9";
const SAND: &str = "#f0cf92";
const SAND_SHADE: &str = "#d5ae6c";
const CASTLE: &str = "#f6dca4";
const CASTLE_SHADE: &str = "#dcb978";
const CASTLE_DARK: &str = "#8d6b36";
const POLE: &str = "#7a5230";
const FLAG: &str = "#ff6b4a";
const WAVE: &str = "#3a8fd1";
const WAVE_DEEP: &str = "#2b6fb0";
const FOAM: &str = "#e8f6ff";

/// Where the front waves reach (the top row of the water) after 0 to 6 misses.
const WATER_LINE: [usize; 7] = [30, 28, 26, 24, 22, 19, 14];

type Scene = [[Option<&'static str>; SCENE_W]; SCENE_H];

fn rect(scene: &mut Scene, x0: usize, y0: usize, x1: usize, y1: usize, color: &'static str) {
    for row in scene.iter_mut().take(y1.min(SCENE_H - 1) + 1).skip(y0) {
        for cell in row.iter_mut().take(x1.min(SCENE_W - 1) + 1).skip(x0) {
            *cell = Some(color);
        }
    }
}

fn clear(scene: &mut Scene, x0: usize, y0: usize, x1: usize, y1: usize) {
    for row in scene.iter_mut().take(y1.min(SCENE_H - 1) + 1).skip(y0) {
        for cell in row.iter_mut().take(x1.min(SCENE_W - 1) + 1).skip(x0) {
            *cell = None;
        }
    }
}

/// The beach, sky and a sandcastle with `misses` of its parts already gone.
fn castle_scene(misses: usize) -> Scene {
    // The beach and sky, then the castle on a layer of its own so that parts washed away show the
    // beach again.
    let mut scene: Scene = [[None; SCENE_W]; SCENE_H];
    for (band, color) in SKY.iter().enumerate() {
        rect(&mut scene, 0, band * 3, SCENE_W - 1, band * 3 + 2, color);
    }
    rect(&mut scene, 39, 3, 43, 7, SUN);
    for (x, y) in [(39, 3), (43, 3), (39, 7), (43, 7)] {
        scene[y][x] = Some(SKY[1]);
    }
    rect(&mut scene, 0, 12, SCENE_W - 1, 15, FAR_SEA);
    for x in (2..SCENE_W).step_by(7) {
        rect(&mut scene, x, 13, x + 2, 13, FAR_SEA_LIGHT);
    }
    rect(&mut scene, 0, 16, SCENE_W - 1, SCENE_H - 1, SAND);
    for (x, y) in [(3, 20), (9, 24), (44, 21), (40, 27), (31, 18), (6, 28), (47, 25)] {
        scene[y][x] = Some(SAND_SHADE);
    }

    // The castle, from the left tower to the right one; the ground is row 27.
    let mut castle: Scene = [[None; SCENE_W]; SCENE_H];
    let walls: [(usize, usize, usize); 5] = [(14, 19, 20), (20, 20, 23), (21, 29, 17), (30, 30, 23), (31, 36, 20)];
    for (x0, x1, top) in walls {
        rect(&mut castle, x0, top, x1, 27, CASTLE);
        rect(&mut castle, x1, top, x1, 27, CASTLE_SHADE);
    }
    rect(&mut castle, 14, 27, 36, 27, CASTLE_SHADE);
    // Merlons on top of both towers and the keep.
    for (x0, x1, top) in [(14, 19, 20), (21, 29, 17), (31, 36, 20)] {
        for x in (x0..=x1).step_by(4) {
            rect(&mut castle, x, top - 2, (x + 1).min(x1), top - 1, CASTLE);
        }
        rect(&mut castle, x1, top - 2, x1, top - 1, CASTLE_SHADE);
    }
    // Door and windows.
    rect(&mut castle, 24, 24, 26, 27, CASTLE_DARK);
    rect(&mut castle, 25, 23, 25, 23, CASTLE_DARK);
    for (x, y) in [(23, 19), (27, 19), (16, 22), (33, 22)] {
        rect(&mut castle, x, y, x, y + 1, CASTLE_DARK);
    }
    // The flag on the keep.
    rect(&mut castle, 25, 8, 25, 14, POLE);
    rect(&mut castle, 26, 8, 29, 10, FLAG);
    rect(&mut castle, 26, 8, 29, 8, "#ff9a85");

    // The sea takes it apart, one piece per miss.
    if misses >= 1 {
        clear(&mut castle, 25, 8, 29, 14);
    }
    if misses >= 2 {
        clear(&mut castle, 31, 18, 36, 21);
    }
    if misses >= 3 {
        clear(&mut castle, 14, 18, 19, 21);
    }
    if misses >= 4 {
        clear(&mut castle, 21, 15, 29, 19);
    }
    if misses >= 5 {
        clear(&mut castle, 31, 18, 36, 27);
        clear(&mut castle, 14, 18, 19, 24);
        clear(&mut castle, 20, 15, 30, 22);
        rect(&mut castle, 22, 23, 28, 27, SAND_SHADE);
    }
    if misses >= 6 {
        clear(&mut castle, 12, 15, 38, 27);
    }
    for (y, row) in castle.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            if cell.is_some() {
                scene[y][x] = *cell;
            }
        }
    }

    // The waves, in front of everything.
    let top = WATER_LINE[misses.min(6)];
    if top < SCENE_H {
        const CREST: [isize; 8] = [1, 0, 0, -1, -1, 0, 0, 1];
        for x in 0..SCENE_W {
            let line = (top as isize + CREST[x % 8]).clamp(0, SCENE_H as isize - 1) as usize;
            for (y, row) in scene.iter_mut().enumerate().skip(line) {
                row[x] = Some(if (y - line) % 4 == 3 { WAVE_DEEP } else { WAVE });
            }
            scene[line][x] = Some(FOAM);
            if (x + line).is_multiple_of(3) && line + 1 < SCENE_H {
                scene[line + 1][x] = Some(FOAM);
            }
        }
    }
    scene
}

impl HangmanCard<'_> {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (720.0, 300.0);
        let mut s = String::with_capacity(48 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);

        // The picture: runs of one colour are one rectangle.
        panel(&mut s, 24.0, 24.0, 316.0, 196.0, 8.0, &t.panel, &t.gold);
        let scene = castle_scene(self.misses);
        let (ox, oy) = (32.0, 32.0);
        for (y, row) in scene.iter().enumerate() {
            let mut x = 0;
            while x < SCENE_W {
                let Some(color) = row[x] else {
                    x += 1;
                    continue;
                };
                let start = x;
                while x < SCENE_W && row[x] == Some(color) {
                    x += 1;
                }
                let _ = write!(
                    s,
                    r#"<rect x="{}" y="{}" width="{}" height="{SCALE}" fill="{color}"/>"#,
                    ox + start as f32 * SCALE,
                    oy + y as f32 * SCALE,
                    (x - start) as f32 * SCALE
                );
            }
        }

        // Lives left, as shells that go dark.
        text(&mut s, 28.0, 258.0, 14.0, PIXEL, false, &t.muted, "start", "LIVES");
        for i in 0..6usize {
            let lost = i >= 6usize.saturating_sub(self.misses);
            let color = if lost { "#2e3d75" } else { t.gold.as_str() };
            sprite(
                &mut s,
                SHELL,
                100.0 + i as f32 * 40.0,
                240.0,
                4.0,
                color,
                if lost { "#3b4676" } else { "#ffffff" },
            );
        }

        // The word.
        panel(&mut s, 364.0, 24.0, 332.0, 252.0, 4.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            384.0,
            58.0,
            15.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &escape(&self.category.to_uppercase()),
        );
        let longest = self.lines.iter().map(Vec::len).max().unwrap_or(1).max(1) as f32;
        let slot = ((332.0 - 40.0) / longest).clamp(16.0, 36.0).floor();
        let font = (slot * 0.85).floor();
        let pitch = slot + 26.0;
        let block = self.lines.len() as f32 * pitch;
        let mut y = 76.0 + (136.0 - block) / 2.0 + slot;
        for line in self.lines {
            let line_w = line.len() as f32 * slot;
            let mut x = 364.0 + (332.0 - line_w) / 2.0;
            for letter in line {
                let (ch, color) = match letter {
                    HangmanLetter::Hidden => (None, &t.ink),
                    HangmanLetter::Found(c) => (Some(*c), &t.ink),
                    HangmanLetter::Missed(c) => (Some(*c), &t.accent),
                };
                if let Some(c) = ch {
                    text(
                        &mut s,
                        x + slot / 2.0,
                        y,
                        font,
                        PIXEL,
                        true,
                        color,
                        "middle",
                        &escape(&c.to_ascii_uppercase().to_string()),
                    );
                }
                let _ = write!(
                    s,
                    r#"<rect x="{}" y="{}" width="{}" height="4" fill="{}"/>"#,
                    x + 3.0,
                    y + 8.0,
                    slot - 6.0,
                    t.muted
                );
                x += slot;
            }
            y += pitch;
        }

        // Wrong letters and the status.
        let wrong: String = self
            .wrong
            .iter()
            .map(|c| c.to_ascii_uppercase().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        text(&mut s, 384.0, 228.0, 14.0, PIXEL, false, &t.muted, "start", "WRONG");
        text(
            &mut s,
            470.0,
            228.0,
            16.0,
            PIXEL,
            false,
            &t.accent,
            "start",
            &escape(&wrong),
        );
        text(
            &mut s,
            384.0,
            262.0,
            16.0,
            PIXEL,
            false,
            &t.gold,
            "start",
            &escape(&self.status.to_uppercase()),
        );
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;

    fn card(lines: &[Vec<HangmanLetter>], misses: usize) -> HangmanCard<'_> {
        HangmanCard {
            lines,
            category: "Sea life",
            wrong: &[],
            misses,
            status: "Pick a letter",
            theme: Theme::reef(),
        }
    }

    #[test]
    fn every_miss_changes_the_picture() {
        let lines = vec![vec![HangmanLetter::Hidden; 5]];
        let pictures: Vec<String> = (0..=6).map(|m| card(&lines, m).to_svg()).collect();
        for pair in pictures.windows(2) {
            assert_ne!(pair[0], pair[1]);
        }
        // The flag's light edge goes at the first miss; at six misses no sand castle remains.
        assert!(pictures[0].contains("#ff9a85") && !pictures[1].contains("#ff9a85"));
        assert!(!pictures[6].contains(CASTLE_DARK), "the door is gone");
    }

    #[test]
    fn letters_show_only_when_found_and_lost_ones_in_the_accent_colour() {
        let lines = vec![vec![
            HangmanLetter::Found('a'),
            HangmanLetter::Hidden,
            HangmanLetter::Missed('z'),
        ]];
        let svg = card(&lines, 6).to_svg();
        assert!(svg.contains(">A<") && svg.contains(">Z<"));
        assert!(svg.contains(&format!(r#"fill="{}" text-anchor="middle">Z<"#, Theme::reef().accent)));
        assert_eq!(svg.matches(">B<").count(), 0);
        let png = Renderer::new().hangman_card(&card(&lines, 3)).expect("renders");
        let image = image::load_from_memory(&png).expect("a png");
        assert_eq!((image.width(), image.height()), (720, 300));
    }
}
