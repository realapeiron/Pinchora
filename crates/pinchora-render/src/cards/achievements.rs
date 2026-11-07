//! A paged server achievement card in the same pixel panels as the other cards.

use std::fmt::Write;

use pinchora_core::i18n::Lang;

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{CRAB, HEART, SHELL, STAR, background, data_uri_png, escape, fit_laid_out, sprite},
};

pub struct AchievementCell {
    pub name: String,
    pub description: String,
    pub icon: String,
    pub icon_png: Option<Vec<u8>>,
    pub progress: i64,
    pub target: i64,
    pub earned: bool,
}

pub struct AchievementsCard {
    pub lang: Lang,
    pub title: String,
    pub kicker: String,
    pub earned_label: String,
    pub locked_label: String,
    pub page_label: String,
    pub empty_label: String,
    pub cells: Vec<AchievementCell>,
    pub theme: Theme,
}

impl AchievementsCard {
    pub fn to_svg(&self) -> String {
        let w = 900.0;
        let h = 170.0 + self.cells.len().max(1) as f32 * 104.0 + 38.0;
        let t = &self.theme;
        let mut s = String::with_capacity(16_000);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(
            &mut s,
            38.0,
            52.0,
            16.0,
            PIXEL,
            false,
            &t.accent,
            "start",
            &escape(&self.lang.upper(&self.kicker)),
        );
        text(
            &mut s,
            38.0,
            112.0,
            35.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&fit_laid_out(&self.title, SANS, true, 35.0, 740.0)),
        );
        text(
            &mut s,
            850.0,
            52.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &escape(&self.lang.upper(&self.page_label)),
        );
        let _ = write!(s, r#"<path d="M38 136H862V140H38Z" fill="{}"/>"#, t.panel_edge);
        if self.cells.is_empty() {
            text(
                &mut s,
                450.0,
                230.0,
                19.0,
                SANS,
                false,
                &t.muted,
                "middle",
                &escape(&self.empty_label),
            );
        }
        for (n, cell) in self.cells.iter().enumerate() {
            let y = 158.0 + n as f32 * 104.0;
            t.panel(&mut s, 38.0, y, 824.0, 92.0, 4.0);
            let color = if cell.earned { &t.gold } else { &t.muted };
            let _ = write!(
                s,
                r#"<path d="M54 {y1}h70v68H54Z" fill="{}"/>"#,
                t.panel_edge,
                y1 = y + 12.0
            );
            if let Some(png) = &cell.icon_png {
                let opacity = if cell.earned { 1.0 } else { 0.36 };
                let _ = write!(
                    s,
                    r#"<image x="64" y="{top}" width="50" height="50" opacity="{opacity}" href="{}"/>"#,
                    data_uri_png(png),
                    top = y + 20.0
                );
            } else {
                let rows = match cell.icon.as_str() {
                    "shell" => SHELL,
                    "crab" => CRAB,
                    "heart" => HEART,
                    _ => STAR,
                };
                let icon_width = rows[0].len() as f32 * 5.0;
                sprite(&mut s, rows, 89.0 - icon_width / 2.0, y + 31.0, 5.0, color, color);
            }
            text(
                &mut s,
                144.0,
                y + 31.0,
                21.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&fit_laid_out(&cell.name, SANS, true, 21.0, 530.0)),
            );
            text(
                &mut s,
                144.0,
                y + 54.0,
                14.0,
                SANS,
                false,
                &t.muted,
                "start",
                &escape(&fit_laid_out(&cell.description, SANS, false, 14.0, 520.0)),
            );
            let ratio = if cell.earned {
                1.0
            } else {
                (cell.progress.max(0) as f32 / cell.target.max(1) as f32).clamp(0.0, 1.0)
            };
            let _ = write!(
                s,
                r#"<rect x="144" y="{bar}" width="570" height="8" fill="{}"/>"#,
                t.panel_edge,
                bar = y + 70.0
            );
            if ratio > 0.0 {
                let _ = write!(
                    s,
                    r#"<rect x="144" y="{bar}" width="{}" height="8" fill="{}"/>"#,
                    570.0 * ratio,
                    color,
                    bar = y + 70.0
                );
            }
            let label = if cell.earned {
                &self.earned_label
            } else {
                &self.locked_label
            };
            text(
                &mut s,
                838.0,
                y + 32.0,
                13.0,
                PIXEL,
                false,
                color,
                "end",
                &escape(&self.lang.upper(label)),
            );
            text(
                &mut s,
                838.0,
                y + 72.0,
                13.0,
                PIXEL,
                false,
                &t.muted,
                "end",
                &format!("{}/{}", cell.progress.min(cell.target), cell.target),
            );
        }
        s.push_str("</svg>");
        s
    }
}
