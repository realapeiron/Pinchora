//! Original reef atlas cards, including house scenes built by the shared isometric renderer.
use crate::{
    Theme,
    cards::{PIXEL, SANS, text},
    encode_png,
    room::{RoomScene, SceneItem, SceneLayer, render_room},
    sprites::{SpriteStore, placeholder_tile},
    svg::{CRAB, STAR, background_plain, data_uri_png, escape, panel, sprite, truncate_to, wrap_laid_out},
};
use std::{fmt::Write, sync::Arc};
pub struct ReefCard<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub prompt: &'a str,
    pub footer: &'a str,
    pub rows: &'a [String],
    pub cells: &'a [u8],
    pub columns: usize,
    pub selected: Option<usize>,
    pub room: Option<usize>,
    pub critter: Option<usize>,
}
impl ReefCard<'_> {
    pub fn svg(&self) -> anyhow::Result<String> {
        let t = Theme::reef();
        let mut s =
            String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="900" height="760" viewBox="0 0 900 760">"#);
        background_plain(&mut s, 900., 760., &t.bg_top, &t.bg_bottom);
        for i in 0..30 {
            let x = (i * 79 % 880) as f32;
            let y = (i * 113 % 730) as f32;
            let _ = write!(
                s,
                r##"<rect x="{x}" y="{y}" width="3" height="3" fill="#7194ae" opacity="0.25"/>"##
            );
        }
        text(
            &mut s,
            34.,
            55.,
            28.,
            PIXEL,
            false,
            &t.ink,
            "start",
            &escape(self.title),
        );
        text(
            &mut s,
            34.,
            89.,
            17.,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(self.subtitle),
        );
        panel(&mut s, 24., 110., 550., 556., 6., &t.panel, &t.panel_edge);
        panel(&mut s, 594., 110., 282., 556., 6., &t.panel, &t.panel_edge);
        let graphic = self.draw_graphic(&mut s)?;
        self.draw_prompt(&mut s, &t, graphic);
        self.draw_rows_and_footer(&mut s, &t);
        s.push_str("</svg>");
        Ok(s)
    }
    fn draw_graphic(&self, s: &mut String) -> anyhow::Result<bool> {
        let graphic = !self.cells.is_empty() || self.room.is_some() || self.critter.is_some();
        if !self.cells.is_empty() {
            let n = self.columns.max(1);
            let size = 420. / n as f32;
            for (i, &v) in self.cells.iter().enumerate() {
                let (x, y) = (89. + (i % n) as f32 * size, 140. + (i / n) as f32 * size);
                let color = match v {
                    0 => "#172e45",
                    1 => "#ff9e7a",
                    2 => "#6ff0c8",
                    3 => "#ffd166",
                    4 => "#8fd3ff",
                    5 => "#d6f1ff",
                    _ => "#436579",
                };
                panel(
                    s,
                    x + 2.,
                    y + 2.,
                    size - 4.,
                    size - 4.,
                    3.,
                    color,
                    if self.selected == Some(i) { "#ffffff" } else { color },
                );
                if n == 4 {
                    text(
                        s,
                        x + size / 2.,
                        y + size / 2. + 8.,
                        22.,
                        PIXEL,
                        false,
                        "#172e45",
                        "middle",
                        &(i + 1).to_string(),
                    );
                } else if v == 1 {
                    sprite(s, CRAB, x + 8., y + 15., 3., "#b8436f", "#fff1e8");
                } else if v == 3 {
                    sprite(s, STAR, x + 15., y + 15., 4., "#172e45", "#fff1e8");
                }
            }
        }
        if let Some(room) = self.room {
            let png = room_png(room)?;
            let _ = write!(
                s,
                r#"<image href="{}" x="42" y="150" width="510" height="420" image-rendering="optimizeSpeed"/>"#,
                data_uri_png(&png)
            );
        }
        if let Some(critter) = self.critter {
            let store = SpriteStore::embedded();
            let names = [
                "p_crumbclaw_crab",
                "p_pebblepouch_hermit",
                "c_plumspot_puffer",
                "c_pearlpatch_koi",
                "p_buttonbell_jelly",
                "p_sprinklearm_octopus",
                "c_petalwake_manta",
                "c_creamcurl_whelk",
            ];
            if let Some(img) = store.get(names[critter % 8]) {
                let png = encode_png(&img)?;
                let _ = write!(
                    s,
                    r#"<image href="{}" x="170" y="180" width="260" height="260" image-rendering="optimizeSpeed"/>"#,
                    data_uri_png(&png)
                );
            }
        }
        Ok(graphic)
    }

    fn draw_prompt(&self, s: &mut String, t: &Theme, graphic: bool) {
        let mut lines = Vec::new();
        let max_lines = if graphic { 2 } else { 13 };
        for paragraph in self.prompt.lines() {
            if lines.len() >= max_lines {
                break;
            }
            if paragraph.is_empty() {
                lines.push(String::new());
            } else {
                lines.extend(wrap_laid_out(
                    paragraph,
                    SANS,
                    false,
                    if graphic { 19. } else { 25. },
                    490.,
                    max_lines - lines.len(),
                ));
            }
        }
        if !graphic && lines.len() < 10 {
            // A quiet pixel reef below the prose; never behind a long question or story.
            for i in 0..8 {
                let x = 62. + i as f32 * 63.;
                let h = 12. + (i % 3) as f32 * 10.;
                let _ = write!(
                    s,
                    r##"<path d="M{x} 646 v-{h} h8 v-8 h8 v8 h-8 v{h}" fill="#245658"/>"##
                );
            }
            sprite(s, CRAB, 260., 610., 5., &t.accent, &t.ink);
        }
        for (i, line) in lines.iter().enumerate() {
            text(
                s,
                52.,
                if graphic { 600. } else { 155. } + i as f32 * 35.,
                if graphic { 19. } else { 25. },
                SANS,
                false,
                &t.ink,
                "start",
                &escape(line),
            );
        }
    }

    fn draw_rows_and_footer(&self, s: &mut String, t: &Theme) {
        for (i, row) in self.rows.iter().take(12).enumerate() {
            let (label, x) = if let Some(label) = row.strip_prefix('★') {
                sprite(s, STAR, 610., 131. + i as f32 * 42., 2., &t.gold, &t.ink);
                (label, 632.)
            } else {
                (row.as_str(), 612.)
            };
            text(
                s,
                x,
                145. + i as f32 * 42.,
                17.,
                SANS,
                false,
                &t.ink,
                "start",
                &escape(&truncate_to(label, 17., 857. - x)),
            );
        }
        for (i, line) in wrap_laid_out(self.footer, SANS, false, 17., 818., 2).iter().enumerate() {
            text(
                s,
                40.,
                701. + i as f32 * 24.,
                17.,
                SANS,
                false,
                &t.mint,
                "start",
                &escape(line),
            );
        }
    }
}
pub fn room_png(kind: usize) -> anyhow::Result<Vec<u8>> {
    let store = SpriteStore::embedded();
    let names = match kind % 4 {
        0 => ["f_polypillow_bed", "f_coilcap_desk_lamp"],
        1 => ["f_stillwarm_stove", "f_pebblewash_sink"],
        2 => ["f_foliobud_bookcase", "f_paperleg_desk"],
        _ => ["f_tidequilt_sofa", "f_capsulecurve_sofa"],
    };
    let items = names
        .iter()
        .enumerate()
        .filter_map(|(i, name)| {
            store.get(name).map(|sprite| SceneItem {
                sprite,
                x: 1 + i as u32 * 3,
                y: 1,
                w: 2,
                d: 2,
                layer: SceneLayer::Floor,
                flipped: false,
                selected: false,
                lift: 0,
            })
        })
        .collect();
    encode_png(&render_room(&RoomScene {
        width: 7,
        depth: 5,
        wallpaper: Arc::new(placeholder_tile("reef-cream", true)),
        floor: Arc::new(placeholder_tile("reef-wood", false)),
        items,
        cursor: None,
        grid: false,
        scale: 1,
    }))
}
