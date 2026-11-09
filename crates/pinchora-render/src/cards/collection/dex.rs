use super::*;

// /Dex

/// One slot of a dex page.
pub struct DexCell {
    /// The slot's number in the whole book, from 1.
    pub number: usize,
    pub name: String,
    pub art: EntryPicture,
    /// The colour of its rarity, `0xrrggbb`.
    pub frame: u32,
    /// Whether the player has it; locked slots are dark and nameless.
    pub owned: bool,
    /// How many times they met it.
    pub caught: i32,
}

/// 1000×620 card with up to 18 slots in a 6×3 grid: one page of the Tide Pool Dex.
pub struct DexCard {
    pub title: String,
    /// The tab, e.g. `CRITTERS`.
    pub tab: String,
    /// The owner's name.
    pub owner: String,
    pub found: usize,
    pub total: usize,
    pub page: usize,
    pub pages: usize,
    pub cells: Vec<DexCell>,
    pub footer: String,
    /// The language of the labels; the title, tab and footer come written.
    pub lang: Lang,
    pub theme: Theme,
}

impl DexCard {
    pub const PER_PAGE: usize = 18;
    pub const ART_BOX: (u32, u32) = (96, 80);
    const COLUMNS: usize = 6;
    const CELL: (f32, f32) = (148.0, 148.0);
    const GAP: f32 = 8.0;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let (w, h) = (1000.0, 620.0);
        let mut s = open_svg(w, h);
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        s.push_str(SILHOUETTE);
        header(
            &mut s,
            t,
            &self.title,
            &format!(
                "{} {}/{}",
                self.tab,
                crate::svg::thousands_in(self.lang, self.found as i64),
                crate::svg::thousands_in(self.lang, self.total as i64)
            ),
            964.0,
        );
        let fraction = if self.total == 0 {
            0.0
        } else {
            self.found as f32 / self.total as f32
        };
        segment_bar(
            &mut s,
            36.0,
            70.0,
            300.0,
            20.0,
            fraction,
            15,
            &t.mint,
            "#1d2b53",
            &t.panel_edge,
        );
        text(
            &mut s,
            964.0,
            84.0,
            16.0,
            SANS,
            true,
            &t.muted,
            "end",
            &escape(&truncate_to(&self.owner, 16.0, 420.0)),
        );

        let (cw, ch) = Self::CELL;
        for (i, cell) in self.cells.iter().take(Self::PER_PAGE).enumerate() {
            let (col, row) = ((i % Self::COLUMNS) as f32, (i / Self::COLUMNS) as f32);
            let (x, y) = (36.0 + col * (cw + Self::GAP), 100.0 + row * (ch + Self::GAP));
            let frame = if cell.owned {
                hex(cell.frame)
            } else {
                t.panel_edge.clone()
            };
            panel(&mut s, x, y, cw, ch, 4.0, &t.panel, &frame);
            if cell.owned && cell.frame == 0x9af5ec {
                let _ = write!(
                    s,
                    r##"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="#e8fffa" stroke-width="2" stroke-dasharray="6 4"/>"##,
                    x + 5.0,
                    y + 5.0,
                    cw - 10.0,
                    ch - 10.0
                );
                sprite(&mut s, STAR, x + 8.0, y + 40.0, 2.0, "#8ff6e9", "#ffffff");
            }
            if cell.owned {
                let _ = write!(
                    s,
                    r#"<rect x="{}" y="{}" width="{}" height="5" fill="{frame}"/>"#,
                    x + 4.0,
                    y + 4.0,
                    cw - 8.0
                );
            }
            text(
                &mut s,
                x + 12.0,
                y + 28.0,
                12.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &format!("#{:02}", cell.number),
            );
            if cell.owned && cell.caught > 1 {
                text(
                    &mut s,
                    x + cw - 12.0,
                    y + 28.0,
                    12.0,
                    PIXEL,
                    false,
                    &t.gold,
                    "end",
                    &format!("x{}", crate::svg::thousands_in(self.lang, cell.caught as i64)),
                );
            }
            let cx = x + cw / 2.0;
            if cell.owned {
                pixel_shadow(&mut s, cx, y + 106.0, cell.art.w as f32 / 2.4, 6.0, 0.25);
            }
            stand(&mut s, &cell.art, cx, y + 102.0, (!cell.owned).then_some("unseen"));
            let (lines, size, ink) = if cell.owned {
                let (lines, size) = name_lines(&cell.name, 14.0, cw - 16.0);
                (lines, size, &t.ink)
            } else {
                (vec!["???".to_owned()], 14.0, &t.panel_edge)
            };
            name_text(&mut s, cx, (y + 132.0, y + 123.0), size, &lines, ink);
        }
        if !self.footer.is_empty() {
            text(
                &mut s,
                36.0,
                598.0,
                14.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &escape(&self.footer),
            );
        }
        text(
            &mut s,
            964.0,
            598.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &t!(
                self.lang,
                "dex.card.page",
                page = self.page + 1,
                pages = self.pages.max(1)
            ),
        );
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}
