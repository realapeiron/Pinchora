//! Original procedural reef scenery with atlas fish/furniture, integer pixel scaling.
use crate::cards::PIXEL;
use crate::cards::SANS;
use crate::cards::text;
use crate::{
    Theme,
    svg::{background, data_uri_png, escape, panel, sprite, truncate_to},
};
use std::fmt::Write;

pub struct ReefTile {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub label: String,
}
#[derive(Clone, Copy)]
pub enum ReefScene {
    Aquarium,
    Garden,
    Dive,
    Dock,
    Craft,
    Season,
    Care,
}
pub struct ReefCard {
    pub title: String,
    pub subtitle: String,
    pub scene: ReefScene,
    pub tiles: Vec<ReefTile>,
    pub lines: Vec<String>,
    pub footer: String,
    pub progress: f32,
}
const WEED: &[&str] = &[
    "..o....o..",
    "..#o..o#..",
    "...#..#...",
    "o..#..#.o.",
    "#o.#..#o#.",
    ".#.#..##..",
    "..##..#...",
    "...#o#....",
    "....#.....",
    "...###...",
];
const PEARL: &[&str] = &[
    "...ooo...",
    ".oo####o.",
    "oo######o",
    "o########",
    "o########",
    "o########",
    ".#######.",
    "..#####..",
    "...###...",
];
const KEEPER: &[&str] = &[
    "...######...",
    "..#oooooo#..",
    ".##########.",
    "..#o####o#..",
    "..#o#oo#o#..",
    "..#oo##oo#..",
    "...#oooo#...",
    "..########..",
    ".#oo####oo#.",
    ".#oo####oo#.",
    "..########..",
    "...##..##...",
];
impl ReefCard {
    pub fn to_svg(&self) -> String {
        let t = Theme::reef();
        let mut s = String::new();
        s.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg" width="960" height="640" viewBox="0 0 960 640">"#);
        background(&mut s, 960., 640., &t.bg_top, &t.bg_bottom);
        panel(&mut s, 24., 24., 912., 84., 6., &t.panel, &t.panel_edge);
        text(
            &mut s,
            48.,
            59.,
            25.,
            PIXEL,
            true,
            &t.gold,
            "start",
            &escape(&truncate_to(&self.title, 25., 850.)),
        );
        text(
            &mut s,
            48.,
            88.,
            16.,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&truncate_to(&self.subtitle, 16., 850.)),
        );
        panel(&mut s, 24., 128., 560., 432., 6., "#123247", "#4f8fba");
        // Pixel water ribbons and bubbles; no gradients on sprites.
        for i in 0..10 {
            let x = 40 + i * 53;
            let y = 148 + (i * 37) % 290;
            let _ = write!(
                s,
                r##"<rect x="{x}" y="{y}" width="4" height="4" fill="#9fdde0" opacity="0.5"/>"##
            );
        }
        for i in 0..7 {
            let y = 184 + i * 48;
            let _ = write!(s, r##"<path d="M32 {y}h544" stroke="#25536b" stroke-width="4"/>"##);
        }
        s.push_str(r##"<path d="M32 520h544v32H32z" fill="#a79b7c"/><path d="M32 520h544" stroke="#eadbb2" stroke-width="8"/>"##);
        for x in [44., 520.] {
            sprite(&mut s, WEED, x, 480., 5., "#448b74", "#9bca82");
        }
        match self.scene {
            ReefScene::Dock => {
                sprite(&mut s, KEEPER, 210., 216., 12., "#bd8663", "#e9c89b");
                s.push_str(r##"<path d="M258 267h8v8h-8zM294 267h8v8h-8zM276 285h12v8h-12z" fill="#1b1d2b"/>"##);
                s.push_str(r##"<path d="M112 368h380v24H112zM144 392v128m316-128v128" stroke="#735c50" stroke-width="16" fill="#c4966a"/>"##);
            }
            ReefScene::Garden => {
                s.push_str(r##"<path d="M88 404h432v48H88z" fill="#735c50" stroke="#c4966a" stroke-width="8"/>"##);
                if self.progress >= 0. {
                    let scale = 3. + (self.progress.clamp(0., 1.) * 6.).floor();
                    for i in 0..3 {
                        sprite(
                            &mut s,
                            WEED,
                            110. + i as f32 * 140.,
                            408. - 10. * scale,
                            scale,
                            "#448b74",
                            "#b3dc8a",
                        );
                    }
                }
            }
            ReefScene::Dive => {
                for i in 0..3 {
                    let fill = if self.progress * 3. > i as f32 {
                        "#e9d9e9"
                    } else {
                        "#355269"
                    };
                    sprite(&mut s, PEARL, 98. + i as f32 * 150., 280., 9., fill, "#ffffff");
                }
            }
            ReefScene::Season | ReefScene::Care => {
                sprite(&mut s, PEARL, 230., 210., 16., "#d6b576", "#fff0b2");
            }
            _ => {}
        }
        for (i, tile) in self.tiles.iter().take(6).enumerate() {
            let x = 48. + (i % 3) as f32 * 174.;
            let y = 160. + (i / 3) as f32 * 168.;
            let scale = (140 / tile.width.max(1)).min(100 / tile.height.max(1)).clamp(1, 6);
            let w = tile.width * scale;
            let h = tile.height * scale;
            let _ = write!(
                s,
                r#"<image x="{}" y="{}" width="{w}" height="{h}" image-rendering="optimizeSpeed" href="{}"/>"#,
                x + 72. - w as f32 / 2.,
                y + 110. - h as f32,
                data_uri_png(&tile.png)
            );
            text(
                &mut s,
                x + 72.,
                y + 140.,
                13.,
                SANS,
                true,
                &t.ink,
                "middle",
                &escape(&truncate_to(&tile.label, 13., 164.)),
            );
        }
        panel(&mut s, 604., 128., 332., 432., 6., &t.panel, &t.panel_edge);
        for (i, line) in self.lines.iter().take(10).enumerate() {
            text(
                &mut s,
                626.,
                163. + i as f32 * 36.,
                16.,
                SANS,
                i == 0,
                if i == 0 { &t.gold } else { &t.ink },
                "start",
                &escape(&truncate_to(line, 16., 288.)),
            );
        }
        s.push_str(r##"<rect x="626" y="528" width="288" height="8" fill="#19354a"/>"##);
        let _ = write!(
            s,
            r##"<rect x="626" y="528" width="{}" height="8" fill="#85c6a8"/>"##,
            288. * self.progress.clamp(0., 1.)
        );
        text(
            &mut s,
            40.,
            602.,
            16.,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&truncate_to(&self.footer, 16., 880.)),
        );
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use crate::cards::reef_loop::*;
    #[test]
    fn atlas_pixels_scale_without_interpolation() {
        let mut sprite = image::RgbaImage::new(2, 2);
        sprite.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        sprite.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
        sprite.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
        sprite.put_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        let card = ReefCard {
            title: String::new(),
            subtitle: String::new(),
            scene: ReefScene::Aquarium,
            tiles: vec![ReefTile {
                png: crate::encode_png(&sprite).unwrap(),
                width: 2,
                height: 2,
                label: String::new(),
            }],
            lines: vec![],
            footer: String::new(),
            progress: 0.,
        };
        let png = crate::Renderer::new().svg_to_png(&card.to_svg()).unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        // The first source pixel occupies exactly x=114..120,y=258..264 at 6×.
        for y in 259..263 {
            for x in 115..119 {
                assert_eq!(image.get_pixel(x, y).0, [255, 0, 0, 255]);
            }
        }
        assert_eq!(image.get_pixel(121, 260).0, [0, 255, 0, 255]);
    }
}
