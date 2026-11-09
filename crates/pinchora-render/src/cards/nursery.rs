use std::fmt::Write;

use pinchora_core::infra::i18n::Lang;
use pinchora_core::t;

use crate::cards::PIXEL;
use crate::cards::PetStatus;
use crate::cards::SANS;
use crate::cards::text;
use crate::{
    Theme,
    svg::{STAR, background, data_uri_png, escape, panel, plate, sprite, truncate_to},
};

/// One pet in the nursery grid.
pub struct NurseryPet {
    /// Sprite already scaled up with nearest-neighbour, at most [`NurseryCard::SPRITE_BOX`].
    pub sprite_png: Vec<u8>,
    pub sprite_w: u32,
    pub sprite_h: u32,
    pub name: String,
    pub level: i32,
    /// `0..=1` of the way to the next level; 1 for a pet at the top.
    pub xp: f32,
    /// hunger, fun, energy, hygiene (0..=100).
    pub needs: [i16; 4],
    /// The colour of its rarity, `0xrrggbb`.
    pub frame: u32,
    pub active: bool,
    pub status: PetStatus,
    /// The badge for a pet that is not simply home, e.g. `ASLEEP` or `AWAY 2 H`.
    pub badge: String,
}

/// 1000 px wide card with up to 8 pets in a 4×2 grid, tall enough for the rows it has (352 px for
/// one row, 600 px for two): the nursery.
pub struct NurseryCard {
    /// The language of the words the card draws itself; the title, subtitle, footer and badges come translated.
    pub lang: Lang,
    pub title: String,
    pub subtitle: String,
    pub pets: Vec<NurseryPet>,
    pub footer: String,
    pub theme: Theme,
}

const FOOD: &[&str] = &[
    "...#...", "..###..", ".#####.", "#######", "#######", ".#####.", "..###..",
];
const FUN: &[&str] = &[".#####.", "#.###.#", "#######", "#.###.#", "##...##", ".#####."];
const ENERGY: &[&str] = &["...###.", "..###..", ".#####.", "...###.", "..##...", ".#....."];
const CLEAN: &[&str] = &[
    "...#...", "..###..", ".#####.", "#o#####", "#######", ".#####.", "..###..",
];

impl NurseryCard {
    pub const SPRITE_BOX: (u32, u32) = (96, 84);
    /// Pets in a row of the grid, and rows on a card.
    const PER_ROW: usize = 4;
    const MAX_ROWS: usize = 2;

    /// Rows of cells the card draws: one for an empty nursery too, which shows a note instead.
    fn rows(&self) -> usize {
        self.pets.len().div_ceil(Self::PER_ROW).clamp(1, Self::MAX_ROWS)
    }

    /// The card's width and height in px.
    pub fn size(&self) -> (u32, u32) {
        (1000, 104 + 248 * self.rows() as u32)
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = {
            let (w, h) = self.size();
            (w as f32, h as f32)
        };
        let mut s = String::with_capacity(96 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(
            &mut s,
            36.0,
            62.0,
            30.0,
            PIXEL,
            false,
            &t.ink,
            "start",
            &escape(&self.lang.upper(&self.title)),
        );
        text(
            &mut s,
            964.0,
            60.0,
            18.0,
            PIXEL,
            false,
            &t.gold,
            "end",
            &escape(&self.subtitle),
        );

        let icons: [(&[&str], &str); 4] = [
            (FOOD, &t.accent),
            (FUN, &t.heart),
            (ENERGY, &t.mint),
            (CLEAN, "#8fd3ff"),
        ];
        for (i, pet) in self.pets.iter().take(Self::PER_ROW * Self::MAX_ROWS).enumerate() {
            let (col, row) = ((i % Self::PER_ROW) as f32, (i / Self::PER_ROW) as f32);
            let (x, y) = (36.0 + col * 236.0, 88.0 + row * 248.0);
            let frame = format!("#{:06x}", pet.frame);
            let opacity = match pet.status {
                PetStatus::Awake | PetStatus::Back => 1.0,
                PetStatus::Asleep => 0.55,
                PetStatus::Away => 0.2,
            };
            panel(&mut s, x, y, 220.0, 232.0, 4.0, &t.panel, &frame);
            let _ = write!(
                s,
                r#"<rect x="{}" y="{}" width="212" height="6" fill="{frame}"/>"#,
                x + 4.0,
                y + 4.0
            );
            if pet.active {
                sprite(&mut s, STAR, x + 14.0, y + 16.0, 2.4, &t.gold, "#fff7d6");
            }
            if !pet.badge.is_empty() {
                let color = match pet.status {
                    PetStatus::Back => &t.mint,
                    PetStatus::Away => &t.gold,
                    _ => "#b8c4ff",
                };
                text(
                    &mut s,
                    x + 206.0,
                    y + 31.0,
                    13.0,
                    PIXEL,
                    false,
                    color,
                    "end",
                    &escape(&pet.badge),
                );
            }

            // The portrait, sitting on a shadow.
            let (bw, bh) = Self::SPRITE_BOX;
            let sx = x + 110.0 - pet.sprite_w as f32 / 2.0;
            let sy = y + 40.0 + (bh as f32 - pet.sprite_h as f32);
            let _ = write!(
                s,
                r##"<ellipse cx="{}" cy="{}" rx="{}" ry="6" fill="#000000" fill-opacity="{}"/><image x="{sx}" y="{sy}" width="{}" height="{}" opacity="{opacity}" href="{}"/>"##,
                x + 110.0,
                y + 40.0 + bh as f32 + 2.0,
                (pet.sprite_w as f32 / 2.2).min(bw as f32 / 2.0),
                0.25 * opacity,
                pet.sprite_w,
                pet.sprite_h,
                data_uri_png(&pet.sprite_png)
            );
            if pet.status == PetStatus::Asleep {
                for (n, size) in [14.0, 20.0].into_iter().enumerate() {
                    text(
                        &mut s,
                        x + 162.0 + n as f32 * 16.0,
                        y + 84.0 - n as f32 * 18.0,
                        size,
                        PIXEL,
                        false,
                        "#b8c4ff",
                        "start",
                        "Z",
                    );
                }
            }
            if pet.status == PetStatus::Away {
                plate(&mut s, x + 62.0, y + 70.0, 96.0, 34.0, 0.7);
                text(
                    &mut s,
                    x + 110.0,
                    y + 94.0,
                    20.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "middle",
                    &t!(self.lang, "card.nursery.away"),
                );
            }

            text(
                &mut s,
                x + 110.0,
                y + 146.0,
                17.0,
                SANS,
                true,
                &t.ink,
                "middle",
                // Cut to the cell's width (bold runs wider than the regular widths it measures).
                &escape(&truncate_to(&pet.name, 18.5, 196.0)),
            );
            // Level, and how far to the next one.
            text(
                &mut s,
                x + 18.0,
                y + 170.0,
                14.0,
                PIXEL,
                false,
                &t.ink,
                "start",
                &t!(self.lang, "card.nursery.level", level = pet.level),
            );
            let bar = 104.0;
            let _ = write!(
                s,
                r##"<rect x="{bx}" y="{by}" width="{bar}" height="8" fill="#0b1029"/><rect x="{bx}" y="{by}" width="{}" height="8" fill="{}"/><rect x="{bx}" y="{by}" width="{}" height="2" fill="#ffffff" fill-opacity="0.35"/>"##,
                bar * pet.xp.clamp(0.0, 1.0),
                t.gold,
                bar * pet.xp.clamp(0.0, 1.0),
                bx = x + 98.0,
                by = y + 160.0,
            );
            // The four needs as little icons that dim when the need runs low, with a bar under each.
            for (n, ((bitmap, color), value)) in icons.iter().zip(pet.needs).enumerate() {
                let ix = x + 22.0 + n as f32 * 45.0;
                let lit = if value >= 25 { color } else { "#3a4a86" };
                sprite(&mut s, bitmap, ix + 4.0, y + 184.0, 3.2, lit, "#ffffff");
                let _ = write!(
                    s,
                    r##"<rect x="{ix}" y="{}" width="32" height="5" fill="#0b1029"/><rect x="{ix}" y="{}" width="{}" height="5" fill="{color}"/>"##,
                    y + 212.0,
                    y + 212.0,
                    32.0 * f32::from(value).clamp(0.0, 100.0) / 100.0
                );
            }
        }
        if self.pets.is_empty() {
            text(
                &mut s,
                500.0,
                h / 2.0 + 40.0,
                22.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &t!(self.lang, "card.nursery.none"),
            );
        }
        text(
            &mut s,
            964.0,
            h - 12.0,
            13.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &escape(&self.footer),
        );
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use image::RgbaImage;

    use crate::cards::nursery::*;
    use crate::{Renderer, encode_png};

    fn card(pets: usize) -> NurseryCard {
        let sprite = encode_png(&RgbaImage::from_pixel(48, 48, image::Rgba([200, 90, 60, 255]))).expect("encodes");
        NurseryCard {
            lang: Lang::En,
            title: "Nursery".to_owned(),
            subtitle: "PAGE 1/1".to_owned(),
            pets: (0..pets)
                .map(|n| NurseryPet {
                    sprite_png: sprite.clone(),
                    sprite_w: 48,
                    sprite_h: 48,
                    name: format!("Pet {n}"),
                    level: 10 + n as i32,
                    xp: 0.5,
                    needs: [80, 70, 60, 50],
                    frame: 0x8fa0d8,
                    active: n == 0,
                    status: PetStatus::Awake,
                    badge: String::new(),
                })
                .collect(),
            footer: format!("{pets} PETS"),
            theme: Theme::reef(),
        }
    }

    #[test]
    fn the_card_is_as_tall_as_its_rows() {
        for (pets, height) in [(0, 352), (1, 352), (4, 352), (5, 600), (8, 600), (12, 600)] {
            let card = card(pets);
            assert_eq!(card.size(), (1000, height), "{pets} pets");
            let svg = card.to_svg();
            assert!(svg.contains(&format!(r#"height="{height}""#)), "{pets} pets");
            // The footer sits just above the bottom edge, whatever the height.
            assert!(svg.contains(&format!(r#"y="{}""#, height - 12)), "{pets} pets");
        }
    }

    #[test]
    fn a_short_card_renders_at_its_own_size() {
        let r = Renderer::new();
        for (pets, height) in [(0, 352), (3, 352), (6, 600)] {
            let png = r.nursery_card(&card(pets)).expect("renders");
            let image = image::load_from_memory(&png).expect("a png");
            assert_eq!((image.width(), image.height()), (1000, height), "{pets} pets");
        }
    }
}
