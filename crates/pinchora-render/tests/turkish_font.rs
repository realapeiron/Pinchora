//! The pixel font (Silkscreen, with İ ı Ş ş Ğ ğ added for Turkish) has every letter of the Turkish alphabet, and
//! draws them as whole font pixels so they stay crisp on a card.

use pinchora_render::{FONT_FILES, Renderer};

/// The Turkish alphabet in both cases, the circumflexed vowels Turkish keeps for loanwords (`kâr`, `hâlâ`) and the
/// letters Turkish names borrow (`q w x`), which the font always had.
const TURKISH: &str = "ABCÇDEFGĞHIİJKLMNOÖPRSŞTUÜVYZabcçdefgğhıijklmnoöprsştuüvyzÂâÎîÛûQWXqwx";

fn font(name: &str) -> &'static [u8] {
    FONT_FILES
        .iter()
        .find(|(file, _)| *file == name)
        .expect("a bundled font")
        .1
}

/// Whether `font` maps `c` to a glyph of its own rather than `.notdef`: reads the Unicode (3, 1) cmap subtable,
/// format 4, which is the one Silkscreen carries.
fn has_glyph(font: &[u8], c: char) -> bool {
    let u16_at = |at: usize| usize::from(u16::from_be_bytes([font[at], font[at + 1]]));
    let u32_at = |at: usize| u16_at(at) << 16 | u16_at(at + 2);
    let cmap = (0..u16_at(4))
        .map(|i| 12 + 16 * i)
        .find(|&record| &font[record..record + 4] == b"cmap")
        .map(|record| u32_at(record + 8))
        .expect("a cmap table");
    let subtable = (0..u16_at(cmap + 2))
        .map(|i| cmap + 4 + 8 * i)
        .find(|&record| u16_at(record) == 3 && u16_at(record + 2) == 1)
        .map(|record| cmap + u32_at(record + 4))
        .expect("a (3, 1) cmap subtable");
    assert_eq!(u16_at(subtable), 4, "a format 4 subtable");
    let segments = u16_at(subtable + 6) / 2;
    let ends = subtable + 14;
    let starts = ends + 2 * segments + 2;
    let deltas = starts + 2 * segments;
    let offsets = deltas + 2 * segments;
    let code = c as usize;
    for i in 0..segments {
        if code > u16_at(ends + 2 * i) {
            continue;
        }
        if code < u16_at(starts + 2 * i) {
            return false;
        }
        let delta = u16_at(deltas + 2 * i);
        let offset = u16_at(offsets + 2 * i);
        let glyph = if offset == 0 {
            code + delta
        } else {
            match u16_at(offsets + 2 * i + offset + 2 * (code - u16_at(starts + 2 * i))) {
                0 => 0,
                glyph => glyph + delta,
            }
        };
        return glyph & 0xFFFF != 0;
    }
    false
}

#[test]
fn the_pixel_font_has_every_turkish_letter() {
    for file in ["Silkscreen-Regular.ttf", "Silkscreen-Bold.ttf"] {
        let missing: String = TURKISH.chars().filter(|&c| !has_glyph(font(file), c)).collect();
        assert!(missing.is_empty(), "{file} has no glyph for: {missing}");
    }
    // The check itself can say no: Silkscreen has no Cyrillic.
    assert!(!has_glyph(font("Silkscreen-Regular.ttf"), 'Ж'));
}

/// `text` in the pixel font at `size`, black on white at whole pixels, as RGBA.
fn draw(text: &str, size: u32, bold: bool) -> (u32, u32, Vec<u8>) {
    let weight = if bold { "bold" } else { "normal" };
    let (w, h) = ((text.chars().count() as u32 + 2) * size, size * 3);
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><rect width="{w}" height="{h}" fill="#fff"/><text x="{size}" y="{y}" font-family="Silkscreen" font-weight="{weight}" font-size="{size}" fill="#000">{text}</text></svg>"##,
        y = size * 2,
    );
    let png = Renderer::new().svg_to_png(&svg).expect("renders");
    let image = image::load_from_memory(&png).expect("a png").to_rgba8();
    (image.width(), image.height(), image.into_raw())
}

/// A font pixel is `size / 8` px, so at 8 and 24 a letter built of whole font pixels lands on whole pixels, while
/// one that sits half a font pixel off (as Silkscreen's own Ö and Ü used to) blurs: its edge pixels are grey.
#[test]
fn turkish_letters_are_made_of_whole_font_pixels() {
    let mut blurred = Vec::new();
    for bold in [false, true] {
        for size in [8, 24] {
            for letter in TURKISH.chars() {
                let (_, _, rgba) = draw(&letter.to_string(), size, bold);
                // Half covered is a blur; the hairline Silkscreen's own O leaves inside its counter is not.
                let grey = rgba.chunks(4).filter(|p| p[0] > 40 && p[0] < 216).count();
                let ink = rgba.chunks(4).filter(|p| p[0] <= 40).count();
                assert!(ink > 0, "{letter} drew nothing at {size} px");
                if grey > 0 {
                    blurred.push(format!(
                        "{letter} ({}, {size} px)",
                        if bold { "bold" } else { "regular" }
                    ));
                }
            }
        }
    }
    assert!(blurred.is_empty(), "blurred letters: {}", blurred.join(", "));
}

/// The new letters are the font's own: İ is an I with a dot, Ş an S with a tail, Ğ a G with a breve, and none of
/// them is another letter or a fallback font's glyph.
#[test]
fn the_new_letters_differ_from_their_plain_letters() {
    for bold in [false, true] {
        for (turkish, plain) in [('İ', 'I'), ('Ş', 'S'), ('Ğ', 'G'), ('ş', 's'), ('ğ', 'g')] {
            let (_, _, a) = draw(&turkish.to_string(), 24, bold);
            let (_, _, b) = draw(&plain.to_string(), 24, bold);
            assert_ne!(a, b, "{turkish} looks like {plain}");
        }
        // Silkscreen has no lower case: ı is the capital I, and so are i and I.
        let (_, _, dotless) = draw("ı", 24, bold);
        let (_, _, capital) = draw("I", 24, bold);
        assert_eq!(dotless, capital);
        // İ keeps the dot above the bar: more rows of ink than the bar alone, the same advance.
        let (_, _, dotted) = draw("İ", 24, bold);
        let ink = |rgba: &[u8]| rgba.chunks(4).filter(|p| p[0] <= 24).count();
        assert!(ink(&dotted) > ink(&capital), "the dot adds ink");
    }
}
