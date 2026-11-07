//! Five independently displaced glyphs on a reef pixel card. The PNG carries no answer metadata.
use crate::{Renderer, Theme, svg};
use pinchora_core::{i18n::Lang, t};
use std::fmt::Write;
const GLYPH_SIZE: u32 = 92;
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
        // Clutter stays in narrow bands above and below the letters.
        out.push_str(r#"<defs><clipPath id="well"><rect x="44" y="80" width="552" height="118"/></clipPath></defs><g clip-path="url(#well)">"#);
        for _ in 0..80 {
            let x = 48 + next() % 540;
            let y = if next() % 2 == 0 {
                84 + next() % 9
            } else {
                190 + next() % 5
            };
            let size = 1 + next() % 3;
            let opacity = 0.35 + (next() % 30) as f32 / 100.;
            let _ = write!(
                out,
                r#"<rect x="{x}" y="{y}" width="{size}" height="{size}" fill="{}" opacity="{opacity}"/>"#,
                theme.muted
            );
        }
        for _ in 0..12 {
            let x = 56 + next() % 520;
            let y = if next() % 2 == 0 { 86 } else { 191 };
            let w = 5 + next() % 9;
            let _ = write!(
                out,
                r#"<path d="M{x} {y} h{w}" fill="none" stroke="{}" stroke-width="1.5" opacity="0.55"/>"#,
                theme.muted
            );
        }
        let phase = (next() % 628) as f32 / 100.;
        for (i, glyph) in self.answer.chars().take(5).enumerate() {
            let x = 101 + i as i32 * 107 + (next() % 9) as i32 - 4;
            let wave = (phase + i as f32 * 1.4).sin() * 6.;
            let y = 174 + wave as i32 + (next() % 7) as i32 - 3;
            let rotation = (next() % 15) as i32 - 7;
            let sx = 0.98 + (next() % 7) as f32 / 100.;
            let sy = 0.96 + (next() % 9) as f32 / 100.;
            let text = svg::escape(&glyph.to_string());
            let left = svg::pixel_width(&text, GLYPH_SIZE as f32) / -2.;
            let _ = write!(
                out,
                r#"<g transform="translate({x},{y}) rotate({rotation}) scale({sx},{sy})"><text x="{left}" fill="{}" font-family="Silkscreen" font-size="{GLYPH_SIZE}">{text}</text></g>"#,
                theme.ink
            );
        }
        for line in 0..2 {
            let y = 132 + line * 28 + (next() % 7) as i32 - 3;
            let bend = (next() % 11) as i32 - 5;
            let _ = write!(
                out,
                r#"<path d="M48 {y} Q190 {} 318 {y} T592 {}" fill="none" stroke="{}" stroke-width="1.25" opacity="0.65"/>"#,
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
    fn captcha_alphabet_and_glyph_size_are_readable() {
        assert_eq!(pinchora_core::verification::CAPTCHA_ALPHABET, b"ACDEFHJKPRTUXY34");
        assert_eq!(GLYPH_SIZE, 92);
        assert!(GLYPH_SIZE as f32 / 56. >= 1.6);
        assert!(GLYPH_SIZE as f32 / 56. <= 1.8);
    }
    #[test]
    fn verification_cards_render_in_both_languages() {
        for lang in [Lang::En, Lang::Tr] {
            for (sample, (answer, seed)) in [
                ("ACDEF", 12345),
                ("HJKPR", 987654),
                ("TUXY3", 54321),
                ("4APCX", 333333),
                ("D3FHK", 98765),
                ("RJ4TU", 24680),
                ("XEYCA", 13579),
                ("KPR34", 555555),
            ]
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
