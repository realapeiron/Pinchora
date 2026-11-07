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
        let mut seed = self.seed.max(1);
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        // Near-ink clutter survives simple colour thresholding. Clip every distortion to the well.
        out.push_str(r#"<defs><clipPath id="well"><rect x="44" y="80" width="552" height="118"/></clipPath></defs><g clip-path="url(#well)">"#);
        for _ in 0..140 {
            let x = 46 + next() % 545;
            let y = 84 + next() % 108;
            let size = 2 + next() % 4;
            let opacity = 0.3 + (next() % 50) as f32 / 100.;
            let _ = write!(
                out,
                r#"<rect x="{x}" y="{y}" width="{size}" height="{size}" fill="{}" opacity="{opacity}"/>"#,
                theme.ink
            );
        }
        // Incomplete pixel strokes resemble glyph parts, without introducing extra readable letters.
        for _ in 0..18 {
            let x = 55 + next() % 520;
            let y = 90 + next() % 90;
            let w = 7 + next() % 14;
            let h = 5 + next() % 12;
            let _ = write!(
                out,
                r#"<path d="M{x} {y} h{w} v{h}" fill="none" stroke="{}" stroke-width="3" opacity="0.6"/>"#,
                theme.ink
            );
        }
        let phase = (next() % 628) as f32 / 100.;
        for (i, glyph) in self.answer.chars().take(5).enumerate() {
            let x = 103 + i as i32 * 102 + (next() % 23) as i32 - 11;
            let wave = (phase + i as f32 * 1.4).sin() * 12.;
            let y = 151 + wave as i32 + (next() % 13) as i32 - 6;
            let rotation = (next() % 33) as i32 - 16;
            let sx = 0.88 + (next() % 25) as f32 / 100.;
            let sy = 0.9 + (next() % 24) as f32 / 100.;
            let text = svg::escape(&glyph.to_string());
            let _ = write!(
                out,
                r#"<g transform="translate({x},{y}) rotate({rotation}) scale({sx},{sy})"><text x="-22" fill="{}" font-family="Silkscreen" font-size="56">{text}</text></g>"#,
                theme.ink
            );
        }
        // Same-colour crossings connect glyph strokes; thin enough to retain human readability.
        for line in 0..3 {
            let y = 112 + line * 18 + (next() % 13) as i32;
            let bend = (next() % 39) as i32 - 19;
            let _ = write!(
                out,
                r#"<path d="M48 {y} Q190 {} 318 {y} T592 {}" fill="none" stroke="{}" stroke-width="2.5"/>"#,
                y + bend,
                y - bend,
                theme.ink
            );
        }
        out.push_str("</g>");
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
            for (sample, (answer, seed)) in [("A7K3M", 12345), ("R8H6V", 987654), ("N4X9E", 54321)]
                .into_iter()
                .enumerate()
            {
                let card = CaptchaCard { answer, seed, lang };
                let svg = card.to_svg();
                assert!(svg.contains("rotate(") && svg.contains("scale(") && svg.contains(" Q"));
                assert_ne!(svg, CaptchaCard { seed: seed + 1, ..card }.to_svg());
                let png = Renderer::new()
                    .captcha_card(&CaptchaCard { answer, seed, lang })
                    .unwrap();
                let image = image::load_from_memory(&png).unwrap();
                assert_eq!((image.width(), image.height()), (640, 280));
                let path = format!(
                    "{}/../../target/sim/verification/captcha-{}-{}.png",
                    env!("CARGO_MANIFEST_DIR"),
                    if lang == Lang::En { "en" } else { "tr" },
                    sample + 1
                );
                std::fs::create_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
                std::fs::write(path, png).unwrap();
            }
        }
    }
}
