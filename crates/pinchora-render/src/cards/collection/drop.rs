use super::*;

// /Drop

/// One of the cards of a critter drop.
pub struct DropCritter {
    pub art: EntryPicture,
    pub name: String,
    /// The rarity as a word.
    pub rarity: String,
    /// The colour of the rarity, `0xrrggbb`.
    pub frame: u32,
    /// Where it lives, as a short phrase.
    pub habitat: String,
    /// Stars around the card: 0 for the common ones, up to 3 for a legendary.
    pub sparkles: u8,
}

/// 1000×460 card with up to three critters side by side, numbered for the buttons under it.
pub struct DropCard {
    pub title: String,
    pub subtitle: String,
    pub cards: Vec<DropCritter>,
    pub footer: String,
    pub theme: Theme,
}

impl DropCard {
    pub const ART_BOX: (u32, u32) = (200, 150);

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (1000.0, 460.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        header(&mut s, t, &self.title, &self.subtitle, 964.0);
        for (i, card) in self.cards.iter().take(3).enumerate() {
            let (x, y) = (36.0 + i as f32 * 312.0, 96.0);
            let frame = hex(card.frame);
            panel(&mut s, x, y, 296.0, 320.0, 4.0, &t.panel, &frame);
            let _ = write!(
                s,
                r#"<rect x="{}" y="{}" width="288" height="6" fill="{frame}"/>"#,
                x + 4.0,
                y + 4.0
            );
            // The number the button carries.
            panel(&mut s, x + 16.0, y + 22.0, 48.0, 48.0, 4.0, "#0b1029", &frame);
            text(
                &mut s,
                x + 40.0,
                y + 56.0,
                28.0,
                PIXEL,
                false,
                &t.ink,
                "middle",
                &(i + 1).to_string(),
            );
            text(
                &mut s,
                x + 280.0,
                y + 52.0,
                14.0,
                PIXEL,
                false,
                &frame,
                "end",
                &escape(&card.rarity.to_uppercase()),
            );
            let cx = x + 148.0;
            pixel_shadow(&mut s, cx, y + 238.0, card.art.w as f32 / 2.2, 8.0, 0.28);
            stand(&mut s, &card.art, cx, y + 232.0, None);
            let stars = [(x + 36.0, y + 118.0), (x + 252.0, y + 98.0), (x + 232.0, y + 190.0)];
            for (sx, sy) in stars.iter().take(card.sparkles.min(3) as usize) {
                sprite(&mut s, STAR, *sx, *sy, 3.0, &t.gold, "#fff7d6");
            }
            if card.sparkles == 4 {
                // Four bright pixel corners and a second inset border identify Glimmer even at thumbnail size.
                let _ = write!(
                    s,
                    r##"<rect x="{}" y="{}" width="280" height="304" fill="none" stroke="#e8fffa" stroke-width="4" stroke-dasharray="12 8"/>"##,
                    x + 8.0,
                    y + 8.0
                );
                for (sx, sy) in [
                    (x + 22.0, y + 80.0),
                    (x + 254.0, y + 76.0),
                    (x + 26.0, y + 212.0),
                    (x + 250.0, y + 214.0),
                ] {
                    sprite(&mut s, STAR, sx, sy, 3.0, "#8ff6e9", "#ffffff");
                }
            }
            text(
                &mut s,
                cx,
                y + 276.0,
                24.0,
                SANS,
                true,
                &t.ink,
                "middle",
                &escape(&truncate_to(&card.name, 24.0, 264.0)),
            );
            text(
                &mut s,
                cx,
                y + 304.0,
                13.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &escape(&card.habitat.to_uppercase()),
            );
        }
        if !self.footer.is_empty() {
            text(
                &mut s,
                w / 2.0,
                446.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "middle",
                &escape(&self.footer),
            );
        }
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}

// /Bottle

/// 800×340 scene: a bottle with a rolled-up note bobbing on the waves, a different glass and cork per
/// `seed`.
pub struct BottleCard {
    pub title: String,
    pub subtitle: String,
    pub seed: u32,
    pub theme: Theme,
}

impl BottleCard {
    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (800.0, 340.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        header(&mut s, t, &self.title, &self.subtitle, 764.0);
        // The water: bands of waves, lighter towards the surface.
        let sea = "#2c7fb8";
        let _ = write!(
            s,
            r#"<rect x="0" y="190" width="{w}" height="150" fill="{}"/>"#,
            mix(sea, &t.bg_bottom, 0.45)
        );
        for row in 0..6u32 {
            let y = 190.0 + row as f32 * 26.0;
            let shade = mix("#8fd3ff", &t.bg_bottom, 0.45 + row as f32 * 0.08);
            let shift = ((self.seed >> (row * 3)) % 8) as f32 * 8.0;
            let mut x = -shift;
            while x < w {
                let len = 24.0 + ((x as u32 / 8 + row) % 3) as f32 * 8.0;
                let _ = write!(s, r#"<rect x="{x}" y="{y}" width="{len}" height="4" fill="{shade}"/>"#);
                x += len + 40.0 + ((x as u32 / 8 + row * 5) % 4) as f32 * 8.0;
            }
        }
        // The bottle, bobbing a little differently per seed.
        let glass = [
            ("#8fd3ff", "#d6f1ff"),
            ("#6ff0c8", "#c9fff0"),
            ("#c2b3e6", "#e8e0fb"),
            ("#ffd166", "#fff0b8"),
        ][(self.seed % 4) as usize];
        let cork = ["#b97f4a", "#d2b77f", "#ff9e7a"][((self.seed / 4) % 3) as usize];
        let bob = ((self.seed / 12) % 3) as f32 * 4.0;
        let (bx, by) = (150.0, 118.0 + bob);
        pixel_shadow(&mut s, bx + 150.0, 196.0 + bob, 150.0, 6.0, 0.25);
        let ink = "#1b1d2b";
        // The outlines first, so the glass of the body and of the neck meet without a seam.
        let _ = write!(
            s,
            r#"<path d="{}" fill="{ink}"/><rect x="{}" y="{}" width="72" height="56" fill="{ink}"/><rect x="{}" y="{}" width="48" height="64" fill="{ink}"/>"#,
            notched_path(bx - 8.0, by - 8.0, 200.0, 96.0, 16.0),
            bx + 176.0,
            by + 12.0,
            bx + 240.0,
            by + 8.0
        );
        let _ = write!(
            s,
            r#"<path d="{}" fill="{}"/><rect x="{}" y="{}" width="64" height="40" fill="{}"/><rect x="{}" y="{}" width="32" height="48" fill="{cork}"/>"#,
            notched_path(bx, by, 184.0, 80.0, 16.0),
            glass.0,
            bx + 176.0,
            by + 20.0,
            glass.0,
            bx + 248.0,
            by + 16.0
        );
        // The shine, and the note rolled up inside.
        let _ = write!(
            s,
            r##"<rect x="{}" y="{}" width="136" height="8" fill="{}"/><rect x="{}" y="{}" width="64" height="8" fill="{}"/><rect x="{}" y="{}" width="120" height="32" fill="#fff1e8"/><rect x="{}" y="{}" width="96" height="4" fill="#b8a08f"/><rect x="{}" y="{}" width="72" height="4" fill="#b8a08f"/><rect x="{}" y="{}" width="32" height="8" fill="#ffffff"/>"##,
            bx + 24.0,
            by + 12.0,
            glass.1,
            bx + 188.0,
            by + 28.0,
            glass.1,
            bx + 28.0,
            by + 24.0,
            bx + 40.0,
            by + 32.0,
            bx + 40.0,
            by + 44.0,
            bx + 252.0,
            by + 16.0
        );
        // Foam in front of it, and a few sparkles.
        for (fx, fy, fw) in [(150.0, 202.0, 72.0), (330.0, 208.0, 56.0), (260.0, 196.0, 40.0)] {
            let _ = write!(
                s,
                r##"<rect x="{fx}" y="{fy}" width="{fw}" height="6" fill="#ffffff" fill-opacity="0.55"/>"##
            );
        }
        for (sx, sy) in [(560.0, 120.0), (640.0, 150.0), (600.0, 90.0)] {
            sprite(&mut s, STAR, sx, sy, 3.0, &t.gold, "#fff7d6");
        }
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}
