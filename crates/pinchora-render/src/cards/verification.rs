//! Five independently displaced glyphs on a reef pixel card. The PNG carries no answer metadata.
use crate::{Renderer, Theme, svg};
use pinchora_core::{i18n::Lang, t};
use std::fmt::Write;
pub struct CaptchaCard<'a> {
    pub answer: &'a str,
    pub seed: u64,
    pub lang: Lang,
}
impl CaptchaCard<'_> {
    pub fn to_svg(&self) -> String {
        let theme = Theme::reef();
        let mut out =
            String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="640" height="280" viewBox="0 0 640 280">"#);
        svg::background_plain(&mut out, 640., 280., &theme.bg_top, &theme.bg_bottom);
        svg::panel(&mut out, 16., 16., 608., 248., 4., &theme.panel, &theme.panel_edge);
        let title = svg::escape(&t!(self.lang, "verification.card_title"));
        let hint = svg::escape(&t!(self.lang, "verification.card_hint"));
        let _ = write!(
            out,
            r#"<text x="40" y="55" fill="{}" font-family="Silkscreen" font-size="18">{title}</text><text x="40" y="235" fill="{}" font-family="Noto Sans" font-size="16">{hint}</text>"#,
            theme.accent, theme.ink
        );
        svg::panel(&mut out, 40., 76., 560., 126., 3., &theme.bg_top, &theme.panel_edge);
        let mut seed = self.seed;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..85 {
            let x = 46 + next() % 545;
            let y = 84 + next() % 108;
            let _ = write!(
                out,
                r#"<rect x="{x}" y="{y}" width="3" height="3" fill="{}" opacity="0.35"/>"#,
                theme.accent
            );
        }
        for (i, glyph) in self.answer.chars().take(5).enumerate() {
            let x = 87 + i as i32 * 101 + (next() % 9) as i32 - 4;
            let y = 157 + (next() % 15) as i32 - 7;
            let skew = (next() % 13) as i32 - 6;
            let text = svg::escape(&glyph.to_string());
            let _ = write!(
                out,
                r#"<g transform="translate({x},{y}) skewX({skew})"><text fill="{}" font-family="Silkscreen" font-weight="normal" font-size="55">{text}</text></g>"#,
                theme.ink
            );
        }
        out.push_str("</svg>");
        out
    }
}
impl Renderer {
    pub fn captcha_card(&self, card: &CaptchaCard<'_>) -> anyhow::Result<Vec<u8>> {
        self.svg_to_png(&card.to_svg())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verification_cards_render_in_both_languages() {
        for lang in [Lang::En, Lang::Tr] {
            let png = Renderer::new()
                .captcha_card(&CaptchaCard {
                    answer: "A7K3M",
                    seed: 12345,
                    lang,
                })
                .unwrap();
            let image = image::load_from_memory(&png).unwrap();
            assert_eq!((image.width(), image.height()), (640, 280));
            let path = format!(
                "{}/../../target/sim/verification/captcha-{}.png",
                env!("CARGO_MANIFEST_DIR"),
                if lang == Lang::En { "en" } else { "tr" }
            );
            std::fs::create_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
            std::fs::write(path, png).unwrap();
        }
    }
}
