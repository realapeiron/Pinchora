use super::*;

impl StatsCard {
    pub(super) fn columns(&self, s: &mut String, y: f32) {
        let t = &self.theme;
        t.panel(s, LEFT, y, INNER, CHART_H, 4.0);
        let tallest = self.days.iter().copied().max().unwrap_or(0).max(0);
        let top = nice_max(tallest);
        self.panel_head(s, &self.chart_title, y, None);
        let (x0, x1) = (LEFT + 64.0, LEFT + INNER - 20.0);
        let (plot_top, base) = (y + 52.0, y + CHART_H - 36.0);
        // A hairline at the top of the scale and halfway, the numbers they stand for on the left.
        for (fraction, value) in [(0.0, top), (0.5, top / 2), (1.0, 0)] {
            let gy = plot_top + (base - plot_top) * fraction;
            let _ = write!(
                s,
                r#"<rect x="{x0}" y="{gy}" width="{}" height="{}" fill="{GRID}"/>"#,
                x1 - x0,
                if fraction == 1.0 { 2.0 } else { 1.0 }
            );
            // A scale of one has no whole number halfway.
            if fraction != 0.5 || top % 2 == 0 {
                text(
                    s,
                    x0 - 10.0,
                    gy + 4.0,
                    11.0,
                    PIXEL,
                    false,
                    &t.muted,
                    "end",
                    &compact_in(self.lang, value),
                );
            }
        }
        if tallest == 0 {
            text(
                s,
                (x0 + x1) / 2.0,
                (plot_top + base) / 2.0 + 6.0,
                16.0,
                SANS,
                false,
                &t.muted,
                "middle",
                &escape(&truncate_to(&self.empty, 16.0, x1 - x0 - 40.0)),
            );
        } else {
            let n = self.days.len().max(1) as f32;
            let slot = (x1 - x0) / n;
            let bar = (slot - 2.0).clamp(1.0, 24.0);
            let peak = self.days.iter().position(|v| *v == tallest);
            for (i, value) in self.days.iter().enumerate() {
                if *value <= 0 {
                    continue;
                }
                let height = ((*value as f32 / top as f32) * (base - plot_top)).max(2.0);
                let bx = x0 + i as f32 * slot + (slot - bar) / 2.0;
                column(s, bx, base - height, bar, height, SERIES[0]);
                if Some(i) == peak {
                    // The busiest day says how busy it was; the scale and the dashboard carry the rest.
                    let label = thousands_in(self.lang, *value);
                    let half = pixel_width(&label, 11.0) / 2.0;
                    let lx = (bx + bar / 2.0).clamp(x0 + half, x1 - half);
                    text(s, lx, base - height - 8.0, 11.0, PIXEL, false, &t.ink, "middle", &label);
                }
            }
        }
        text(
            s,
            x0,
            y + CHART_H - 14.0,
            11.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &escape(&self.first_day),
        );
        text(
            s,
            x1,
            y + CHART_H - 14.0,
            11.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &escape(&self.last_day),
        );
    }

    pub(super) fn flows(&self, s: &mut String, flows: &StatFlows, y: f32) {
        let t = &self.theme;
        t.panel(s, LEFT, y, INNER, FLOWS_H, 4.0);
        self.panel_head(s, &flows.title, y, None);
        // The legend: a swatch beside each total, the words in ink.
        let mut lx = LEFT + INNER - 20.0;
        for (label, color) in [(&flows.leaves_label, SERIES[2]), (&flows.joins_label, SERIES[1])] {
            let width = pixel_width(label, 12.0);
            text(s, lx, y + 30.0, 12.0, PIXEL, false, &t.ink, "end", &escape(label));
            lx -= width + 10.0;
            let _ = write!(
                s,
                r#"<rect x="{}" y="{}" width="10" height="10" fill="{color}"/>"#,
                lx - 10.0,
                y + 20.0
            );
            lx -= 30.0;
        }
        let (x0, x1) = (LEFT + 64.0, LEFT + INNER - 20.0);
        let (top, bottom) = (y + 48.0, y + FLOWS_H - 16.0);
        let mid = (top + bottom) / 2.0;
        let most = flows.joins.iter().chain(&flows.leaves).copied().max().unwrap_or(0);
        let scale = nice_max(most);
        let _ = write!(
            s,
            r#"<rect x="{x0}" y="{}" width="{}" height="2" fill="{GRID}"/>"#,
            mid - 1.0,
            x1 - x0
        );
        text(
            s,
            x0 - 10.0,
            top + 8.0,
            11.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &format!("+{}", compact_in(self.lang, scale)),
        );
        text(
            s,
            x0 - 10.0,
            bottom,
            11.0,
            PIXEL,
            false,
            &t.muted,
            "end",
            &format!("-{}", compact_in(self.lang, scale)),
        );
        let n = flows.joins.len().max(flows.leaves.len()).max(1) as f32;
        let slot = (x1 - x0) / n;
        let bar = (slot - 2.0).clamp(1.0, 24.0);
        let reach = mid - top - 2.0;
        for i in 0..n as usize {
            let bx = x0 + i as f32 * slot + (slot - bar) / 2.0;
            let joins = flows.joins.get(i).copied().unwrap_or(0);
            let leaves = flows.leaves.get(i).copied().unwrap_or(0);
            if joins > 0 {
                let height = (joins as f32 / scale as f32 * reach).max(2.0);
                column(s, bx, mid - 1.0 - height, bar, height, SERIES[1]);
            }
            if leaves > 0 {
                let height = (leaves as f32 / scale as f32 * reach).max(2.0);
                let _ = write!(
                    s,
                    r#"<rect x="{bx}" y="{}" width="{bar}" height="{height}" fill="{}"/>"#,
                    mid + 1.0,
                    SERIES[2]
                );
            }
        }
    }
}
