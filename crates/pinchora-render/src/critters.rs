//! Procedural pixel critters and finds, drawn until atlas sprites `cr_<id>` replace them.
//!
//! Every shape is a small grid of letters: `b` the body, `a` its shadow, `c` and `d` its lights, `x`
//! the accent colour with `z` its shadow and `y` its light, `w` the white of an eye and `p` its pupil.
//! A dark outline is added around the whole shape. A *look* is a shape, a body palette, an accent
//! palette and a pattern, so a few dozen shapes make a few hundred distinct creatures.

use image::{Rgba, RgbaImage};

use crate::sprites::variant_ramp;

/// The outline round every creature and find.
const OUTLINE: Rgba<u8> = Rgba([0x1b, 0x1d, 0x2b, 255]);

/// The shapes of living critters.
pub const CRITTER_SHAPES: [&str; 14] = [
    "crab", "hermit", "star", "urchin", "snail", "shrimp", "jelly", "seahorse", "octopus", "turtle", "clam", "slug",
    "anemone", "puffer",
];

/// The shapes of things the tide leaves.
pub const FIND_SHAPES: [&str; 14] = [
    "conch",
    "scallop",
    "dollar",
    "glass",
    "pebble",
    "driftwood",
    "feather",
    "rope",
    "coin",
    "tooth",
    "pearl",
    "key",
    "weed",
    "coral",
];

/// How the body's main colour is broken up with the accent.
pub const PATTERNS: [&str; 3] = ["plain", "stripes", "spots"];

/// The grid of a shape, at most 14 wide and 14 tall (the outline makes it 16).
fn shape(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "crab" => &[
            "xx..........xx",
            "xyx........xyx",
            "xxx..w..w..xxx",
            ".xxx.p..p.xxx.",
            "..xbbbbbbbbx..",
            "...bcbbbbcb...",
            "..bbbbbbbbbb..",
            "..abbbbbbbba..",
            "...aaaaaaaa...",
            "..x.x....x.x..",
        ],
        "hermit" => &[
            "......cccc....",
            "....ccddbbc...",
            "...cdbxxxbbc..",
            "..cdbxbbbxbbc.",
            "..cbbxbxxbxba.",
            "..cbbxbbbbxba.",
            "..abbbxxxxbba.",
            "...aabbbbbaa..",
            ".w.p.aaaaaa...",
            ".xxxxbb.xx.xx.",
            "xyx.x.........",
        ],
        "star" => &[
            ".......bb.....",
            ".......bb.....",
            "......bccb....",
            "......bcdb....",
            "bbbbbbbcdbbbbb",
            ".bcccbwcdwcccb",
            "..bbcbbpbpbcb.",
            "...abbbbbbba..",
            "...abbbbbbaa..",
            "..abbba.abbba.",
            "..abba...abba.",
            ".abb.......bba",
        ],
        "urchin" => &[
            ".a..a.aa.a..a.",
            "..a.a.aa.a.a..",
            "a..abbbbbba..a",
            ".a.bbccbbbb.a.",
            "aa.bcdbbbbb.aa",
            "..bbbwpbwpbb..",
            "aabbbbbbbbbbaa",
            "..abbbbbbbba..",
            ".a.aabbbbaa.a.",
            "a..a.aaaa.a..a",
            "...a..a.a..a..",
        ],
        "snail" => &[
            "....xxxx......",
            "..xxyyxxxx....",
            ".xyyxxxxxxx...",
            ".xyxxyyyxxx...",
            ".xxxyxxyxxx...",
            ".xxxyyyxxxz.w.",
            "..xxxxxxxzzbwp",
            "bbbbbbbbbbbbbb",
            ".bcbbbbbbbbbb.",
            "..aabbbbbbaa..",
        ],
        "shrimp" => &[
            "..........x...",
            "...bbbbb...x..",
            "..bccccbbb..x.",
            ".bcbbbbbbbb.x.",
            ".bbabbabbbwp..",
            "bbbabbabbbbbbb",
            "bba.abbabbbaa.",
            "bb...aaaaaa...",
            "bba..x.x.x.x..",
            ".bba..........",
            "..xxx.........",
            "..xyx.........",
        ],
        "jelly" => &[
            "....bbbbbb....",
            "..bbccccccbb..",
            ".bccddddddccb.",
            ".bcdddddddddb.",
            "bbcdwpddwpdcbb",
            "bbcddddddddcbb",
            "baabbbbbbbbaab",
            ".x.x.x..x.x.x.",
            ".x..x.x.x..x..",
            "x..x..x..x.x..",
            ".x...x.x...x..",
        ],
        "seahorse" => &[
            "....bbbb......",
            "...bccbbb.....",
            "...bwpbbbbb...",
            "...bbbbbbb....",
            "....bbbb......",
            "...xbbbb......",
            "..xybbbbb.....",
            "...xbbbbb.....",
            "....bbbb......",
            "....abbb......",
            "....abbba.....",
            "...bbbaa......",
            "...bbba.......",
            "....bb........",
        ],
        "octopus" => &[
            "....bbbbbb....",
            "..bbccccccbb..",
            ".bccddddddccb.",
            ".bcddddddddcb.",
            ".bbwpbbbbwpbb.",
            ".bbbbbbbbbbbb.",
            ".abbbbbbbbbba.",
            "bb.bb.bb.bb.bb",
            "bb.bb.bb.bb.bb",
            ".bb..bb.bb..bb",
            "xb...xb.bx...x",
        ],
        "turtle" => &[
            "......bbb.....",
            "....bbbbbb....",
            "..xxxxxxxxxx..",
            ".xbbxbbxbbbbx.",
            "xbbxbbbbxbbbbx",
            "xbbbbxxbbbbbbx",
            ".xbbxbbxbbbbx.",
            "..xxxxxxxxxx..",
            ".bb.bbbbbb.bb.",
            ".bb.bb..bb.bb.",
        ],
        "clam" => &[
            "....xxxxxx....",
            "..xxyyyyyyxx..",
            ".xyyxxxxxxyyx.",
            "xyxxxyyyyxxxyx",
            "xyxxyyxxyyxxyx",
            "xxxxxxxxxxxxxx",
            "zzzzzzzzzzzzzz",
            "bbcbbbbbbbbcbb",
            ".bbbwpbbwpbbb.",
            "..bbbbbbbbbb..",
            "...aaaaaaaa...",
        ],
        "slug" => &[
            "..x......x....",
            "..xy....xy....",
            "..bb....bb....",
            ".bbbbbbbbbb...",
            "bbcbbbbbbbbb..",
            "bwpbxbbxbbxbb.",
            "bbbbbxbbxbbxbb",
            ".bbbbbbbbbbbbb",
            "..aaaaaaaaaaaa",
        ],
        "anemone" => &[
            "x..x.xx.x..x..",
            ".x.x.xx.x.x...",
            ".xyxyxxyxyx...",
            "..xxxxxxxx....",
            "x.xxxxxxxx.x..",
            ".xxxbbbbxxx...",
            "..xbwpbwpbx...",
            "...bbbbbbb....",
            "...abbbbba....",
            "..aabbbbbaa...",
            "..aaaaaaaaa...",
        ],
        "puffer" => &[
            "...bbbbbbbb...",
            "..bccccccbbb..",
            ".bcddddddbbbb.",
            ".bcdwwdddbbbbz",
            "bbcdwpdddbbbzz",
            "bbcdddddbbbbz.",
            "bbxbbbbbbbbbb.",
            ".abbxbbxbbxba.",
            ".aabbbbbbbbaa.",
            "..xa.aaaa.ax..",
            "..xx......xx..",
        ],
        // Finds.
        "conch" => &[
            "...xxxxx......",
            "..xyyxxxxx....",
            ".xyxxyyxxxx...",
            ".xyxyxxyxxxbb.",
            ".xxxxyyxxxbbc.",
            ".xxxyxxxxbbcb.",
            "..xxxxxxbbcb..",
            "...zxxxbbcba..",
            "....zzbbbbaa..",
            ".....aaaaa....",
        ],
        "scallop" => &[
            "....xxxxxx....",
            "..xxbxxbxxbx..",
            ".xbxxbxxbxxbbx",
            ".xbxbxxbxbxxbx",
            "xbxxbxxbxxbxbx",
            "xbxbxxbxxbxxbx",
            "xxbxxbxxbxxbxx",
            ".xzzzzzzzzzzx.",
            "..zzxxxxxxzz..",
            "....aabbaa....",
        ],
        "dollar" => &[
            "....bbbbbb....",
            "..bbccccccbb..",
            ".bccdddddddcb.",
            ".bcddxddxddcb.",
            "bcddxdxdxxddcb",
            "bcdxddxxddxdcb",
            "bcddxdxddxddab",
            ".bcddxddxddab.",
            ".bbaddddddabb.",
            "..bbaaaaaabb..",
        ],
        "glass" => &[
            "....bbbb......",
            "..bbccbbbb....",
            ".bccddcbbbbb..",
            ".bcdddcbbbbbb.",
            "bbcddcbbbbbbba",
            "bbccbbbbbbbbba",
            ".bbbbbbbbbbaa.",
            "..bbbbbbbbaa..",
            "...aabbbaaa...",
            ".....aaaa.....",
        ],
        "pebble" => &[
            "....bbbbbb....",
            "..bbccccbbbb..",
            ".bccddcbbbbbb.",
            ".bcdddbbbbbbb.",
            "bbcddbbbbbbbba",
            "bbcbbbbbbbbbaa",
            ".bbbbbbbbbbaa.",
            "..abbbbbbaaa..",
            "...aaaaaaaa...",
        ],
        "driftwood" => &[
            "..............",
            "..bbbbbbbbbb..",
            ".bccccccccccbb",
            "bxxbbbbbbbbbba",
            "bxyxbbbbbbbbba",
            "bxxxbbbbbbbbaa",
            ".bxbaaaaaaaaa.",
            "..bbaaaaaaa...",
        ],
        "feather" => &[
            "..........bb..",
            ".........bcb..",
            "........bcdbb.",
            ".......bcdbb..",
            "......bcdbbb..",
            ".....bcdbbbx..",
            "....bcdbbbxx..",
            "...bcdbbbbx...",
            "..bcbbbba.....",
            ".xbbbaa.......",
            "x.xaa.........",
        ],
        "rope" => &[
            "....xxxxxx....",
            "..xxyxxxxxxx..",
            ".xxyxxzzxxxxx.",
            ".xyxxz..zxxxx.",
            "xyxxz....zxxx.",
            "xxxxz....zxxz.",
            ".xxxxz..zxxzz.",
            ".xxxxxxxxxzz..",
            "..zxxxxxxzz...",
            "...zzzzzzz....",
        ],
        "coin" => &[
            "....xxxxxx....",
            "..xxyyyyyyxx..",
            ".xyyxxxxxxyyx.",
            ".xyxxbbbbxxyx.",
            "xyxxbxxxxbxxyx",
            "xyxxbxddxbxxyx",
            "xyxxbxxxxbxxzx",
            ".xyxxbbbbxxzx.",
            ".xxzxxxxxxzxx.",
            "..xzzzzzzzzx..",
        ],
        "tooth" => &[
            "...bbbbbbbb...",
            "...bccccccb...",
            "....bcddddb...",
            "....bcdddba...",
            ".....bcddba...",
            ".....bcdba....",
            "......bcba....",
            "......bba.....",
            ".......ba.....",
        ],
        "pearl" => &[
            "...bbbbbb.....",
            ".bbccccccbb...",
            ".bcddddddcb...",
            "bcddwwddddcb..",
            "bcdwwdddddcb..",
            "bcdddddddcab..",
            "bbcddddddcab..",
            ".bbcddddcabb..",
            "..bbaaaaabb...",
            "....xxxxxx....",
        ],
        "key" => &[
            "..bbbb........",
            ".bccccb.......",
            "bcb..bcb......",
            "bcb..bcb......",
            ".bcccb.bbbbbbb",
            "..bbba.bcccccb",
            "......bcbbbbbb",
            ".....bcb.b.b..",
            "....bcb.bcbbcb",
            "....bbb.bbbbbb",
        ],
        "weed" => &[
            "..x...x...x...",
            "..xx..xx..xx..",
            "...xx..xx..xx.",
            "..xxx.xxx.xxx.",
            "..xyxxxxyxxxy.",
            "...xxyxxxxyx..",
            "..xxxxyxxxxxx.",
            "...xxxxxxxxx..",
            "....zxxxxzz...",
            ".....zzzzz....",
        ],
        "coral" => &[
            "..x.....x..x..",
            "..xx....xx.xy.",
            "..xyx...xyxxy.",
            "...xxx..xxxx..",
            "x...xxxxxxx...",
            "xy...xxxxx..x.",
            ".xx..xxxxx.xy.",
            "..xxxxxxxxxx..",
            "...xxxxxxxx...",
            "....zzzzzz....",
        ],
        _ => return None,
    })
}

/// Whether `name` is a shape this module draws.
pub fn is_shape(name: &str) -> bool {
    shape(name).is_some()
}

/// The colour a pattern gives the body pixel at `(x, y)`: `true` for the accent.
fn accented(pattern: &str, x: u32, y: u32) -> bool {
    match pattern {
        "stripes" => y % 3 == 1,
        "spots" => (x * 3 + y * 5).is_multiple_of(7) && (x + y).is_multiple_of(2),
        _ => false,
    }
}

/// Draws a shape in `body` and `accent` palette names (as in [`variant_ramp`]) with a `pattern`.
/// `None` when the shape or a palette is unknown.
pub fn draw(shape_name: &str, body: &str, accent: &str, pattern: &str) -> Option<RgbaImage> {
    let rows = shape(shape_name)?;
    let (body, accent) = (variant_ramp(body)?, variant_ramp(accent)?);
    let ink = |c: [u8; 3]| Rgba([c[0], c[1], c[2], 255]);
    let (w, h) = (rows[0].len() as u32 + 2, rows.len() as u32 + 2);
    let mut img = RgbaImage::new(w, h);
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.bytes().enumerate() {
            let (px, py) = (x as u32, y as u32);
            let color = match ch {
                b'b' if accented(pattern, px, py) => ink(accent[2]),
                b'b' => ink(body[2]),
                b'a' => ink(body[1]),
                b'c' => ink(body[3]),
                b'd' => ink(body[4]),
                b'x' => ink(accent[2]),
                b'y' => ink(accent[3]),
                b'z' => ink(accent[1]),
                b'w' => Rgba([0xff, 0xf1, 0xe8, 255]),
                b'p' => OUTLINE,
                _ => continue,
            };
            img.put_pixel(px + 1, py + 1, color);
        }
    }
    // The outline: every empty pixel that touches the shape.
    let solid = img.clone();
    for (x, y, px) in img.enumerate_pixels_mut() {
        if px[3] != 0 {
            continue;
        }
        let filled = |dx: i32, dy: i32| {
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 && solid.get_pixel(nx as u32, ny as u32)[3] != 0
        };
        if filled(-1, 0) || filled(1, 0) || filled(0, -1) || filled(0, 1) {
            *px = OUTLINE;
        }
    }
    Some(img)
}

#[cfg(test)]
mod tests {
    use crate::critters::*;

    #[test]
    fn every_shape_is_a_clean_grid() {
        for name in CRITTER_SHAPES.iter().chain(&FIND_SHAPES) {
            let rows = shape(name).unwrap_or_else(|| panic!("{name} is listed but not drawn"));
            assert!(rows.len() <= 14, "{name} is {} rows", rows.len());
            for row in rows {
                assert_eq!(row.len(), 14, "{name}: {row:?} is not 14 wide");
                assert!(
                    row.bytes().all(|b| b".abcdxyzwp".contains(&b)),
                    "{name}: {row:?} uses an unknown letter"
                );
            }
        }
    }

    #[test]
    fn a_look_is_outlined_and_patterns_change_it() {
        let plain = draw("crab", "sand", "coral", "plain").expect("a crab");
        assert_eq!((plain.width(), plain.height()), (16, 12));
        assert_eq!(plain.get_pixel(0, 0)[3], 0, "corners stay empty");
        assert!(plain.pixels().any(|p| *p == OUTLINE));
        let striped = draw("crab", "sand", "coral", "stripes").expect("a crab");
        assert_ne!(plain, striped);
        assert!(draw("dragon", "sand", "coral", "plain").is_none());
        assert!(draw("crab", "chartreuse", "coral", "plain").is_none());
    }
}
