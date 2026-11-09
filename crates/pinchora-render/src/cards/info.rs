//! Compact information card shared by the `/info` views.

use std::fmt::Write;

use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::avatar_frame;
use crate::cards::text;
use crate::{
    Theme,
    svg::{CRAB, STAR, background, escape, fit_laid_out, sprite},
};
use pinchora_core::infra::i18n::Lang;

/// Label and value shown on an information card.
pub struct InfoRow {
    pub label: String,
    pub value: String,
}

pub struct InfoCard {
    pub lang: Lang,
    pub title: String,
    pub subtitle: String,
    pub rows: Vec<InfoRow>,
    pub image: Option<Vec<u8>>,
    pub theme: Theme,
}

impl InfoCard {
    pub fn to_svg(&self) -> String {
        let count = self.rows.len().min(18);
        let columns = if count <= 4 { 2 } else { 3 };
        let lines = count.div_ceil(columns);
        let (w, h) = (900.0, 248.0 + lines as f32 * 112.0);
        let t = &self.theme;
        let mut out = String::with_capacity(32_000);
        let _ = write!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut out, w, h, &t.bg_top, &t.bg_bottom);
        if let Some(image) = &self.image {
            avatar_frame(&mut out, image, 32.0, 32.0, 168.0, 8.0, &t.accent, "info-avatar");
        } else {
            t.panel(&mut out, 32.0, 32.0, 168.0, 168.0, 8.0);
            // A role's exact Discord colour is also visible as a large pixel swatch.
            let colour = self.rows.iter().find_map(|row| swatch(&row.value));
            if let Some(colour) = colour {
                let _ = write!(
                    out,
                    r#"<path d="M80 62H152V74H164V146H152V158H80V146H68V74H80Z" fill="{colour}"/>"#
                );
                sprite(&mut out, STAR, 96.0, 90.0, 4.0, &t.ink, &t.ink);
            } else {
                sprite(&mut out, CRAB, 60.0, 76.0, 8.0, &t.accent, &t.gold);
            }
        }
        text(
            &mut out,
            228.0,
            66.0,
            18.0,
            PIXEL,
            false,
            &t.accent,
            "start",
            &escape(&fit_laid_out(
                &self.lang.upper(&self.subtitle),
                PIXEL,
                false,
                18.0,
                630.0,
            )),
        );
        text(
            &mut out,
            228.0,
            123.0,
            38.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&fit_laid_out(&safe_text(&self.title), SANS, true, 38.0, 630.0)),
        );
        let _ = write!(
            out,
            r#"<path d="M228 152H832V156H228Z" fill="{}"/><path d="M228 152H308V156H228Z" fill="{}"/>"#,
            t.panel_edge, t.accent
        );
        text(
            &mut out,
            228.0,
            184.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            "PINCHORA / DISCORD",
        );
        for (index, row) in self.rows.iter().take(18).enumerate() {
            let row_columns = (count - (index / columns) * columns).min(columns);
            let tile_w = (836.0 - (row_columns - 1) as f32 * 12.0) / row_columns as f32;
            let x = 32.0 + (index % columns) as f32 * (tile_w + 12.0);
            let y = 220.0 + (index / columns) as f32 * 112.0;
            t.panel(&mut out, x, y, tile_w, 100.0, 4.0);
            text(
                &mut out,
                x + 18.0,
                y + 29.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &escape(&fit_laid_out(
                    &self.lang.upper(&row.label),
                    PIXEL,
                    false,
                    14.0,
                    tile_w - 36.0,
                )),
            );
            let colour = swatch(&row.value);
            let offset = if let Some(colour) = colour {
                let _ = write!(
                    out,
                    r#"<rect x="{}" y="{}" width="24" height="24" fill="{colour}"/>"#,
                    x + 18.0,
                    y + 51.0
                );
                34.0
            } else {
                0.0
            };
            let numeric =
                row.value.len() <= 10 && row.value.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.');
            let font = if numeric { PIXEL } else { SANS };
            let size = if numeric {
                32.0
            } else if row.value.len() >= 17 && row.value.bytes().all(|b| b.is_ascii_digit()) {
                18.0
            } else {
                23.0
            };
            text(
                &mut out,
                x + 18.0 + offset,
                y + 73.0,
                size,
                font,
                !numeric,
                if numeric { &t.gold } else { &t.ink },
                "start",
                &escape(&fit_laid_out(
                    &safe_text(&row.value),
                    font,
                    !numeric,
                    size,
                    tile_w - 36.0 - offset,
                )),
            );
        }
        t.draw_frame(&mut out, w, h);
        out.push_str("</svg>");
        out
    }
}

fn safe_text(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .collect()
}

fn swatch(value: &str) -> Option<&str> {
    (value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())).then_some(value)
}

#[cfg(test)]
mod tests {
    use crate::cards::info::*;
    #[test]
    fn hostile_names_render() {
        let card = InfoCard {
            lang: Lang::En,
            title: format!("{}\u{202e}<>&", "Z\u{0301}".repeat(500)),
            subtitle: "user".into(),
            rows: vec![InfoRow {
                label: "role".into(),
                value: "مرحبا\u{202e}<bad>&".into(),
            }],
            image: None,
            theme: Theme::reef(),
        };
        assert!(!card.to_svg().contains("<bad>"));
        assert!(!card.to_svg().contains('\u{202e}'));
        crate::Renderer::new().svg_to_png(&card.to_svg()).unwrap();
    }
}
