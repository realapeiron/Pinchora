use std::fmt::Write;

use pinchora_core::{i18n::Lang, t};

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{
        CRAB, STAR, background_plain, escape, fit_laid_out, laid_out_width, panel, pixel_width, sprite, wrap_laid_out,
    },
};

/// One command on a help page: what to type, what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpRow {
    /// With its slash, e.g. `/house edit`.
    pub command: String,
    pub text: String,
    /// Marked with a star: only members who can manage the server get anything out of it.
    pub manager: bool,
}

/// 900-wide page of the `/help` guide: the area's name, one line on it, and a row for each command,
/// with the page number in the corner. A row's text may take a second line; the card grows to fit.
pub struct HelpCard {
    /// The language of the labels; the rows and title come translated.
    pub lang: Lang,
    pub title: String,
    pub intro: String,
    /// 1-based, as shown.
    pub page: usize,
    pub pages: usize,
    pub rows: Vec<HelpRow>,
    pub theme: Theme,
}

const WIDTH: f32 = 900.0;
const BODY_X: f32 = 36.0;
const BODY_W: f32 = 828.0;
const COMMAND_X: f32 = 68.0;
const COMMAND_W: f32 = 268.0;
const TEXT_X: f32 = 352.0;
const TEXT_W: f32 = 492.0;
const COMMAND_SIZE: f32 = 18.0;
const TEXT_SIZE: f32 = 15.0;
const ONE_LINE: f32 = 38.0;
const TWO_LINES: f32 = 58.0;
const BODY_PAD: f32 = 12.0;
const HEAD: f32 = 150.0;
const FOOT: f32 = 56.0;

impl HelpCard {
    /// The tallest a page's rows may stand together; the guide starts a new page past it.
    pub const BODY_MAX: f32 = 560.0;

    /// How many lines (one or two) the row's text takes, or `None` when even two are not enough.
    fn lines(text: &str) -> Option<Vec<String>> {
        let lines = wrap_laid_out(text, SANS, false, TEXT_SIZE, TEXT_W, 3);
        (lines.len() <= 2).then_some(lines)
    }

    /// Long command paths wrap at spaces, keeping every callable name visible.
    fn command_lines(command: &str) -> Vec<String> {
        wrap_laid_out(command, SANS, true, COMMAND_SIZE, COMMAND_W, 3)
    }

    /// Whether both columns can be drawn whole in at most two lines.
    pub fn fits(row: &HelpRow) -> bool {
        let lines = Self::command_lines(&row.command);
        lines.len() <= 2
            && lines.join(" ") == row.command
            && lines
                .iter()
                .all(|line| laid_out_width(line, SANS, true, COMMAND_SIZE) <= COMMAND_W)
            && Self::lines(&row.text).is_some()
    }

    /// How tall the row stands on the card.
    pub fn row_height(row: &HelpRow) -> f32 {
        if Self::command_lines(&row.command).len() > 1 || Self::lines(&row.text).is_some_and(|lines| lines.len() > 1) {
            TWO_LINES
        } else {
            ONE_LINE
        }
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let body: f32 = self.rows.iter().map(Self::row_height).sum::<f32>() + 2.0 * BODY_PAD;
        let (w, h) = (WIDTH, HEAD + body + FOOT);
        let mut s = String::with_capacity(32 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        // The sparkles stay out: they would land on the rows.
        background_plain(&mut s, w, h, &t.bg_top, &t.bg_bottom);

        text(
            &mut s,
            36.0,
            44.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &t!(self.lang, "card.help-title"),
        );
        let chip_text = t!(self.lang, "card.page", page = self.page, pages = self.pages);
        let chip = pixel_width(&chip_text, 16.0) + 24.0;
        panel(&mut s, 864.0 - chip, 24.0, chip, 32.0, 4.0, &t.panel, &t.panel_edge);
        text(
            &mut s,
            864.0 - chip / 2.0,
            47.0,
            16.0,
            PIXEL,
            false,
            &t.ink,
            "middle",
            &chip_text,
        );
        let (title_font, title_bold) = if self.lang == Lang::Tr {
            (PIXEL, false)
        } else {
            (SANS, true)
        };
        text(
            &mut s,
            36.0,
            92.0,
            34.0,
            title_font,
            title_bold,
            &t.ink,
            "start",
            &escape(&fit_laid_out(&self.title, title_font, title_bold, 34.0, 828.0)),
        );
        text(
            &mut s,
            36.0,
            122.0,
            16.0,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&fit_laid_out(&self.intro, SANS, false, 16.0, 828.0)),
        );

        panel(&mut s, BODY_X, HEAD - 8.0, BODY_W, body, 4.0, &t.panel, &t.panel_edge);
        let mut y = HEAD - 8.0 + BODY_PAD;
        let mut any_manager = false;
        for (i, row) in self.rows.iter().enumerate() {
            let height = Self::row_height(row);
            if i > 0 {
                let _ = write!(
                    s,
                    r#"<rect x="{}" y="{}" width="{}" height="2" fill="{}"/>"#,
                    BODY_X + 16.0,
                    y - 1.0,
                    BODY_W - 32.0,
                    t.panel_edge
                );
            }
            let middle = y + height / 2.0;
            if row.manager {
                any_manager = true;
                sprite(&mut s, STAR, BODY_X + 12.0, middle - 9.0, 2.0, &t.gold, "#fff1e8");
            }
            let command_lines = Self::command_lines(&row.command);
            let first = middle + 6.0 - (command_lines.len() as f32 - 1.0) * 10.0;
            for (n, line) in command_lines.iter().enumerate() {
                text(
                    &mut s,
                    COMMAND_X,
                    first + n as f32 * 20.0,
                    COMMAND_SIZE,
                    SANS,
                    true,
                    &t.ink,
                    "start",
                    &escape(line),
                );
            }
            let lines =
                Self::lines(&row.text).unwrap_or_else(|| wrap_laid_out(&row.text, SANS, false, TEXT_SIZE, TEXT_W, 2));
            let first = middle + 5.0 - (lines.len() as f32 - 1.0) * 9.0;
            for (n, line) in lines.iter().enumerate() {
                text(
                    &mut s,
                    TEXT_X,
                    first + n as f32 * 18.0,
                    TEXT_SIZE,
                    SANS,
                    false,
                    &t.muted,
                    "start",
                    &escape(line),
                );
            }
            y += height;
        }

        let foot = h - 20.0;
        if any_manager {
            sprite(&mut s, STAR, 36.0, foot - 12.0, 2.0, &t.gold, "#fff1e8");
            text(
                &mut s,
                58.0,
                foot,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &t!(self.lang, "card.needs-manage-server"),
            );
        }
        sprite(&mut s, CRAB, 752.0, foot - 12.0, 2.0, &t.accent, "#fff1e8");
        text(&mut s, 864.0, foot, 14.0, PIXEL, false, &t.muted, "end", "PINCHORA");
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;

    fn row(command: &str, text: &str, manager: bool) -> HelpRow {
        HelpRow {
            command: command.to_owned(),
            text: text.to_owned(),
            manager,
        }
    }

    fn card(rows: Vec<HelpRow>) -> HelpCard {
        HelpCard {
            lang: Lang::En,
            title: "House".to_owned(),
            intro: "Decorate rooms, visit friends and show off.".to_owned(),
            page: 2,
            pages: 12,
            rows,
            theme: Theme::reef(),
        }
    }

    #[test]
    fn a_long_text_takes_a_second_line_and_the_card_grows() {
        let short = row("/house view", "Look at your house", false);
        let long = row(
            "/house visit",
            "Visit someone's house, and the first visit each day pays you shells for the trip",
            false,
        );
        assert!(HelpCard::fits(&short) && HelpCard::fits(&long));
        assert!(HelpCard::row_height(&long) > HelpCard::row_height(&short));
        let image = |rows| {
            let png = Renderer::new().svg_to_png(&card(rows).to_svg()).expect("renders");
            image::load_from_memory(&png).expect("a png")
        };
        let (one, two) = (image(vec![short.clone()]), image(vec![long]));
        assert_eq!(one.width(), 900);
        assert!(two.height() > one.height());
    }

    #[test]
    fn rows_that_would_be_cut_are_told_apart() {
        let wide = row(&format!("/{}", "unbreakable".repeat(12)), "x", false);
        assert!(!HelpCard::fits(&wide), "a command wider than its column");
        let endless = row("/x", &"word ".repeat(40), false);
        assert!(!HelpCard::fits(&endless), "a text of more than two lines");
    }

    #[test]
    fn consolidated_paths_wrap_without_truncating_the_action() {
        let row = row("/progression leveling xp-import-mee6", "Import server XP", true);
        assert!(HelpCard::fits(&row));
        assert_eq!(HelpCard::row_height(&row), TWO_LINES);
        let lines = HelpCard::command_lines(&row.command);
        assert_eq!(lines.join(" "), row.command);
        let svg = card(vec![row]).to_svg();
        assert!(svg.contains("xp-import-mee6"));
        Renderer::new().svg_to_png(&svg).expect("wrapped help renders");
    }

    #[test]
    fn managers_get_a_star_and_a_legend() {
        let plain = card(vec![row("/house view", "Look at your house", false)]).to_svg();
        let starred = card(vec![row("/config show", "Show the settings", true)]).to_svg();
        assert!(!plain.contains("NEEDS MANAGE SERVER"));
        assert!(starred.contains("NEEDS MANAGE SERVER"));
    }

    #[test]
    fn awkward_text_is_escaped() {
        let svg = card(vec![row("/a<b>", "Tide & \"Pool\" <i>", false)]).to_svg();
        assert!(svg.contains("/a&lt;b&gt;") && svg.contains("Tide &amp;"));
        assert!(!svg.contains("<i>"));
    }
}
