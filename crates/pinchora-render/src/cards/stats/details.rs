use super::*;

impl StatsCard {
    pub(super) fn heat(&self, s: &mut String, heat: &StatHeat, y: f32) {
        let t = &self.theme;
        t.panel(s, LEFT, y, INNER, HEAT_H, 4.0);
        self.panel_head(s, &heat.title, y, None);
        let max = heat.grid.iter().flatten().copied().max().unwrap_or(0);
        // The legend: quiet to busy.
        let mut lx = LEFT + INNER - 20.0;
        text(
            s,
            lx,
            y + 30.0,
            11.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &escape(&heat.more),
        );
        lx -= pixel_width(&heat.more, 11.0) + 8.0;
        for color in HEAT.iter().rev() {
            lx -= 12.0;
            let _ = write!(
                s,
                r#"<rect x="{lx}" y="{}" width="10" height="10" fill="{color}"/>"#,
                y + 21.0
            );
        }
        text(
            s,
            lx - 8.0,
            y + 30.0,
            11.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &escape(&heat.less),
        );
        let (x0, top) = (LEFT + 64.0, y + 48.0);
        let cell = (INNER - 84.0) / 24.0;
        let row = 22.0;
        for (d, hours) in heat.grid.iter().enumerate() {
            let ry = top + d as f32 * row;
            text(
                s,
                x0 - 10.0,
                ry + 14.0,
                11.0,
                PIXEL,
                false,
                &t.muted,
                "end",
                &escape(&truncate_pixel_to(&heat.weekdays[d], 11.0, 48.0)),
            );
            for (hour, n) in hours.iter().enumerate() {
                let fill = heat_step(*n, max).map_or(HEAT_EMPTY, |step| HEAT[step]);
                let _ = write!(
                    s,
                    r#"<rect x="{}" y="{ry}" width="{}" height="{}" fill="{fill}"/>"#,
                    x0 + hour as f32 * cell,
                    cell - 2.0,
                    row - 2.0
                );
            }
        }
        let below = top + 7.0 * row + 16.0;
        for hour in [0usize, 6, 12, 18] {
            text(
                s,
                x0 + hour as f32 * cell,
                below,
                11.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &format!("{hour:02}:00"),
            );
        }
        text(
            s,
            x0 + 24.0 * cell - 2.0,
            below,
            11.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            "24:00",
        );
    }

    pub(super) fn list(&self, s: &mut String, list: &StatList, x: f32, y: f32, w: f32) {
        let t = &self.theme;
        let h = Self::list_height();
        t.panel(s, x, y, w, h, 4.0);
        text(
            s,
            x + 20.0,
            y + 30.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &escape(&truncate_pixel_to(&list.title, 14.0, w - 40.0)),
        );
        if list.rows.is_empty() {
            text(
                s,
                x + w / 2.0,
                y + h / 2.0 + 8.0,
                15.0,
                SANS,
                false,
                &t.muted,
                "middle",
                &escape(&truncate_to(&list.empty, 15.0, w - 40.0)),
            );
            return;
        }
        let most = list.rows.iter().map(|r| r.value).max().unwrap_or(0).max(1);
        for (i, row) in list.rows.iter().take(LIST_ROWS).enumerate() {
            let ry = y + 56.0 + i as f32 * LIST_ROW;
            text(
                s,
                x + 20.0,
                ry + 16.0,
                13.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &format!("{}", i + 1),
            );
            let shown_w = pixel_width(&row.shown, 13.0);
            text(
                s,
                x + w - 20.0,
                ry + 16.0,
                13.0,
                PIXEL,
                false,
                &t.ink,
                "end",
                &escape(&row.shown),
            );
            text(
                s,
                x + 44.0,
                ry + 17.0,
                16.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(&row.name, 16.0, w - 84.0 - shown_w)),
            );
            let track = w - 64.0;
            let filled = (row.value.max(0) as f32 / most as f32 * track).max(2.0);
            let _ = write!(
                s,
                r#"<rect x="{}" y="{}" width="{track}" height="6" fill="{GRID}"/><rect x="{}" y="{}" width="{filled}" height="6" fill="{}"/>"#,
                x + 44.0,
                ry + 26.0,
                x + 44.0,
                ry + 26.0,
                SERIES[0]
            );
        }
    }
}
