//! Compact information card shared by the `/info` views.

use std::fmt::Write;

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{background_plain, data_uri_png, escape, fit_laid_out},
};

/// Label and value shown on an information card.
pub struct InfoRow {
    pub label: String,
    pub value: String,
}

pub struct InfoCard {
    pub title: String,
    pub subtitle: String,
    pub rows: Vec<InfoRow>,
    pub image: Option<Vec<u8>>,
    pub theme: Theme,
}

impl InfoCard {
    pub fn to_svg(&self) -> String {
        let (w, h) = (900.0, 170.0 + self.rows.len().min(9) as f32 * 53.0);
        let mut out = String::with_capacity(16_000);
        let _ = write!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        let t = &self.theme;
        background_plain(&mut out, w, h, &t.bg_top, &t.bg_bottom);
        t.panel(&mut out, 28.0, 28.0, 844.0, h - 56.0, 4.0);
        let left = if self.image.is_some() { 156.0 } else { 52.0 };
        if let Some(image) = &self.image {
            let uri = data_uri_png(image);
            let _ = write!(
                out,
                r#"<image href="{uri}" x="52" y="52" width="80" height="80" preserveAspectRatio="xMidYMid slice"/>"#
            );
            let _ = write!(
                out,
                r#"<rect x="50" y="50" width="84" height="84" fill="none" stroke="{}" stroke-width="4"/>"#,
                t.accent
            );
        }
        text(
            &mut out,
            left,
            78.0,
            30.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&fit_laid_out(&self.title, SANS, true, 30.0, 820.0 - left)),
        );
        text(
            &mut out,
            left,
            111.0,
            17.0,
            PIXEL,
            false,
            &t.accent,
            "start",
            &escape(&fit_laid_out(&self.subtitle, PIXEL, false, 17.0, 820.0 - left)),
        );
        for (index, row) in self.rows.iter().take(9).enumerate() {
            let y = 166.0 + index as f32 * 53.0;
            let _ = write!(
                out,
                r#"<rect x="52" y="{}" width="796" height="1" fill="{}"/>"#,
                y - 20.0,
                t.panel_edge
            );
            text(
                &mut out,
                52.0,
                y,
                16.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &escape(&fit_laid_out(&row.label, PIXEL, false, 16.0, 210.0)),
            );
            text(
                &mut out,
                280.0,
                y,
                20.0,
                SANS,
                false,
                &t.ink,
                "start",
                &escape(&fit_laid_out(&row.value, SANS, false, 20.0, 555.0)),
            );
        }
        t.draw_frame(&mut out, w, h);
        out.push_str("</svg>");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hostile_names_render() {
        let card = InfoCard {
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
        crate::Renderer::new().svg_to_png(&card.to_svg()).unwrap();
    }
}
