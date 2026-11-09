use super::*;

// /Haul

/// One thing in a haul.
pub struct HaulTile {
    pub art: EntryPicture,
    pub name: String,
    /// The rarity as a word.
    pub rarity: String,
    /// The colour of its rarity, `0xrrggbb`.
    pub frame: u32,
    /// `NEW`, `x3` and the like; empty for nothing.
    pub tag: String,
    /// What it paid, e.g. `+14 shells`; empty for nothing.
    pub line: String,
    /// Shown only when the haul is this one thing.
    pub flavor: String,
}

/// What a beachcomb, a grabbed card or a crab pot brought: one thing drawn big, or up to eight in a
/// grid.
pub struct HaulCard {
    pub title: String,
    pub subtitle: String,
    pub tiles: Vec<HaulTile>,
    pub footer: String,
    pub theme: Theme,
}

/// The plate behind a haul grid's subtitle as `(x, y, w, h)`: it ends at the card's right margin and holds
/// the text, which is 18 px and ends at 958 on the baseline 60.
pub(super) fn subtitle_plate(subtitle: &str) -> (f32, f32, f32, f32) {
    let w = pixel_width(subtitle, 18.0) + 24.0;
    (970.0 - w, 38.0, w, 30.0)
}

impl HaulCard {
    pub const SINGLE_ART_BOX: (u32, u32) = (224, 176);
    pub const GRID_ART_BOX: (u32, u32) = (120, 96);
    pub const MAX_TILES: usize = 8;

    pub fn to_svg(&self) -> String {
        if self.tiles.len() == 1 {
            self.single_svg()
        } else {
            self.grid_svg()
        }
    }

    fn single_svg(&self) -> String {
        let t = &self.theme;
        let tile = &self.tiles[0];
        let (w, h) = (760.0, 380.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        header(&mut s, t, &self.title, "", 724.0);
        text(
            &mut s,
            724.0,
            60.0,
            18.0,
            PIXEL,
            false,
            &t.gold,
            "end",
            &escape(&self.subtitle),
        );
        let frame = hex(tile.frame);
        panel(&mut s, 36.0, 92.0, 284.0, 240.0, 4.0, &t.panel, &frame);
        if tile.frame == 0x9af5ec {
            let _ = write!(
                s,
                r##"<rect x="42" y="98" width="272" height="228" fill="none" stroke="#e8fffa" stroke-width="4" stroke-dasharray="10 6"/>"##
            );
            for (x, y) in [(48.0, 156.0), (272.0, 120.0), (270.0, 258.0)] {
                sprite(&mut s, STAR, x, y, 3.0, "#8ff6e9", "#ffffff");
            }
        }
        let _ = write!(s, r#"<rect x="40" y="96" width="276" height="6" fill="{frame}"/>"#);
        let cx = 36.0 + 142.0;
        pixel_shadow(&mut s, cx, 306.0, tile.art.w as f32 / 2.3, 8.0, 0.28);
        stand(&mut s, &tile.art, cx, 300.0, None);
        if !tile.tag.is_empty() {
            let chip = pixel_width(&tile.tag, 16.0) + 20.0;
            panel(&mut s, 52.0, 112.0, chip, 30.0, 4.0, "#0b1029", &t.gold);
            text(
                &mut s,
                52.0 + chip / 2.0,
                133.0,
                16.0,
                PIXEL,
                false,
                &t.gold,
                "middle",
                &escape(&tile.tag),
            );
        }
        let x = 352.0;
        let (name, size) = fit_name(&tile.name, 32.0, 22.0, 372.0);
        text(&mut s, x, 138.0, size, SANS, true, &t.ink, "start", &escape(&name));
        text(
            &mut s,
            x,
            170.0,
            16.0,
            PIXEL,
            false,
            &frame,
            "start",
            &escape(&tile.rarity.to_uppercase()),
        );
        for (i, line) in wrap(&tile.flavor, 30, 4).iter().enumerate() {
            text(
                &mut s,
                x,
                210.0 + i as f32 * 28.0,
                18.0,
                SANS,
                false,
                &t.muted,
                "start",
                &escape(&truncate_to(line, 18.0, 372.0)),
            );
        }
        if !tile.line.is_empty() {
            panel(&mut s, x, 290.0, 372.0, 42.0, 4.0, "#0b1029", &t.panel_edge);
            text(
                &mut s,
                x + 186.0,
                318.0,
                18.0,
                PIXEL,
                false,
                &t.gold,
                "middle",
                &escape(&tile.line.to_uppercase()),
            );
        }
        if !self.footer.is_empty() {
            text(
                &mut s,
                w / 2.0,
                362.0,
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

    fn grid_svg(&self) -> String {
        let t = &self.theme;
        let tiles = &self.tiles[..self.tiles.len().min(Self::MAX_TILES)];
        let rows = tiles.len().div_ceil(4).max(1) as f32;
        let (w, h) = (1000.0, 108.0 + rows * 214.0 + 40.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        header(&mut s, t, &self.title, "", 964.0);
        if !self.subtitle.is_empty() {
            // On a plate, because the background's fixed sparkles can land right on the text of a tall card.
            let (px, py, pw, ph) = subtitle_plate(&self.subtitle);
            panel(&mut s, px, py, pw, ph, 4.0, "#0b1029", &t.panel_edge);
            text(
                &mut s,
                958.0,
                60.0,
                18.0,
                PIXEL,
                false,
                &t.gold,
                "end",
                &escape(&self.subtitle),
            );
        }
        for (i, tile) in tiles.iter().enumerate() {
            let in_row = (tiles.len() - (i / 4) * 4).min(4) as f32;
            let row_w = in_row * 224.0 + (in_row - 1.0) * 12.0;
            let x = (w - row_w) / 2.0 + (i % 4) as f32 * 236.0;
            let y = 92.0 + (i / 4) as f32 * 214.0;
            let frame = hex(tile.frame);
            panel(&mut s, x, y, 224.0, 202.0, 4.0, &t.panel, &frame);
            if tile.frame == 0x9af5ec {
                let _ = write!(
                    s,
                    r##"<rect x="{}" y="{}" width="212" height="190" fill="none" stroke="#e8fffa" stroke-width="3" stroke-dasharray="7 5"/>"##,
                    x + 6.0,
                    y + 6.0
                );
                sprite(&mut s, STAR, x + 188.0, y + 58.0, 2.0, "#8ff6e9", "#ffffff");
            }
            let _ = write!(
                s,
                r#"<rect x="{}" y="{}" width="216" height="6" fill="{frame}"/>"#,
                x + 4.0,
                y + 4.0
            );
            if !tile.tag.is_empty() {
                text(
                    &mut s,
                    x + 14.0,
                    y + 32.0,
                    14.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "start",
                    &escape(&tile.tag),
                );
            }
            text(
                &mut s,
                x + 212.0,
                y + 32.0,
                12.0,
                PIXEL,
                false,
                &frame,
                "end",
                &escape(&tile.rarity.to_uppercase()),
            );
            let cx = x + 112.0;
            pixel_shadow(&mut s, cx, y + 142.0, tile.art.w as f32 / 2.3, 6.0, 0.25);
            stand(&mut s, &tile.art, cx, y + 136.0, None);
            let (lines, size) = name_lines(&tile.name, 16.0, 204.0);
            name_text(&mut s, cx, (y + 168.0, y + 158.0), size, &lines, &t.ink);
            if !tile.line.is_empty() {
                text(
                    &mut s,
                    cx,
                    y + 190.0,
                    13.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "middle",
                    &escape(&tile.line.to_uppercase()),
                );
            }
        }
        if !self.footer.is_empty() {
            text(
                &mut s,
                w / 2.0,
                h - 16.0,
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
