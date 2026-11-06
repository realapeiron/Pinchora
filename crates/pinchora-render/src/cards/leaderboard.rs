use std::fmt::Write;

use super::{PIXEL, SANS, avatar_frame, text};
use crate::{
    Theme,
    svg::{CRAB, background, compact, escape, panel, sprite, truncate},
};

/// One ranked row.
pub struct LeaderboardEntry {
    pub rank: i64,
    /// PNG, [`LeaderboardCard::AVATAR_SIZE`] square.
    pub avatar_png: Vec<u8>,
    pub name: String,
    /// Main figure, e.g. `LV 27` or the shell count.
    pub value: String,
    /// Smaller line under it, e.g. `64,820 XP`.
    pub detail: String,
}

/// One place on the previous week's podium, shown under the rows.
pub struct LastWeekEntry {
    /// 1 to 3.
    pub rank: i64,
    pub name: String,
    /// What they earned, e.g. `4,210 XP`.
    pub value: String,
}

/// 900×716 leaderboard card with up to ten rows; the first three get gold, silver and bronze. With a
/// `last_week` podium the card grows by a panel of three places under the rows.
pub struct LeaderboardCard {
    pub title: String,
    pub entries: Vec<LeaderboardEntry>,
    /// Zero-based; shown as `PAGE n`.
    pub page: u32,
    /// The previous period's top places (up to three); empty leaves the panel out.
    pub last_week: Vec<LastWeekEntry>,
    /// What a page with no rows says.
    pub empty: &'static str,
    pub theme: Theme,
}

impl LeaderboardCard {
    pub const AVATAR_SIZE: u32 = 32;
    pub const ROWS: usize = 10;

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let podium = !self.last_week.is_empty();
        let (w, h) = (900.0, if podium { 826.0 } else { 716.0 });
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
            &escape(&truncate(&self.title.to_uppercase(), 28)),
        );
        text(
            &mut s,
            864.0,
            60.0,
            18.0,
            PIXEL,
            false,
            &t.gold,
            "end",
            &format!("PAGE {}", self.page + 1),
        );

        if self.entries.is_empty() {
            panel(&mut s, 36.0, 88.0, 828.0, 160.0, 4.0, &t.panel, &t.panel_edge);
            text(
                &mut s,
                450.0,
                176.0,
                24.0,
                SANS,
                false,
                &t.muted,
                "middle",
                &escape(self.empty),
            );
        }
        let medals = [t.gold.as_str(), "#c9d1e6", "#d08a52"];
        for (i, e) in self.entries.iter().take(Self::ROWS).enumerate() {
            let y = 88.0 + i as f32 * 58.0;
            let medal = usize::try_from(e.rank - 1).ok().and_then(|r| medals.get(r)).copied();
            let edge = medal.unwrap_or(&t.panel_edge);
            panel(&mut s, 36.0, y, 828.0, 52.0, 4.0, &t.panel, edge);
            match medal {
                Some(color) => {
                    panel(&mut s, 48.0, y + 10.0, 32.0, 32.0, 4.0, color, color);
                    text(
                        &mut s,
                        64.0,
                        y + 33.0,
                        18.0,
                        PIXEL,
                        false,
                        "#1b1d2b",
                        "middle",
                        &e.rank.to_string(),
                    );
                }
                None => text(
                    &mut s,
                    64.0,
                    y + 33.0,
                    18.0,
                    PIXEL,
                    false,
                    &t.muted,
                    "middle",
                    &compact(e.rank),
                ),
            }
            avatar_frame(&mut s, &e.avatar_png, 96.0, y + 6.0, 40.0, 4.0, edge, &format!("a{i}"));
            text(
                &mut s,
                152.0,
                y + 34.0,
                22.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate(&e.name, 26)),
            );
            let (value_y, detail) = if e.detail.is_empty() {
                (y + 33.0, false)
            } else {
                (y + 28.0, true)
            };
            text(
                &mut s,
                844.0,
                value_y,
                20.0,
                PIXEL,
                false,
                &t.ink,
                "end",
                &escape(&e.value),
            );
            if detail {
                text(
                    &mut s,
                    844.0,
                    y + 44.0,
                    11.0,
                    PIXEL,
                    false,
                    &t.muted,
                    "end",
                    &escape(&e.detail),
                );
            }
        }

        if podium {
            self.last_week_panel(&mut s, &medals);
        }
        sprite(&mut s, CRAB, 36.0, h - 30.0, 3.0, &t.accent, "#fff1e8");
        text(&mut s, 864.0, h - 14.0, 14.0, PIXEL, false, &t.muted, "end", "PINCHORA");
        s.push_str("</svg>");
        s
    }
}

impl LeaderboardCard {
    /// The previous week's top three, side by side in a panel under the rows.
    fn last_week_panel(&self, s: &mut String, medals: &[&str; 3]) {
        let t = &self.theme;
        let top = 678.0;
        panel(s, 36.0, top, 828.0, 100.0, 4.0, &t.panel, &t.panel_edge);
        text(
            s,
            56.0,
            top + 28.0,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            "LAST WEEK'S TOP 3",
        );
        for (i, e) in self.last_week.iter().take(3).enumerate() {
            let x = 56.0 + i as f32 * 268.0;
            let color = usize::try_from(e.rank - 1)
                .ok()
                .and_then(|r| medals.get(r))
                .copied()
                .unwrap_or(&t.muted);
            panel(s, x, top + 44.0, 36.0, 36.0, 4.0, color, color);
            text(
                s,
                x + 18.0,
                top + 69.0,
                18.0,
                PIXEL,
                false,
                "#1b1d2b",
                "middle",
                &e.rank.to_string(),
            );
            text(
                s,
                x + 48.0,
                top + 60.0,
                18.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate(&e.name, 15)),
            );
            text(
                s,
                x + 48.0,
                top + 78.0,
                11.0,
                PIXEL,
                false,
                &t.muted,
                "start",
                &escape(&e.value),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Renderer, placeholder_avatar};

    fn entry(rank: i64, name: &str) -> LeaderboardEntry {
        LeaderboardEntry {
            rank,
            avatar_png: placeholder_avatar(rank as u64, LeaderboardCard::AVATAR_SIZE),
            name: name.to_owned(),
            value: format!("LV {rank}"),
            detail: "1,000 XP".to_owned(),
        }
    }

    #[test]
    fn renders_full_empty_and_awkward_pages() {
        let r = Renderer::new();
        let card = |entries| LeaderboardCard {
            title: "Server leaderboard".to_owned(),
            entries,
            page: 3,
            last_week: Vec::new(),
            empty: "Nobody here yet.",
            theme: Theme::reef(),
        };
        let full = (1..=10)
            .map(|n| entry(n, "<b>Tide & \"Pool\"</b> with a very long display name indeed"))
            .collect();
        for entries in [full, Vec::new(), vec![entry(41, "Solo")]] {
            let png = r.leaderboard_card(&card(entries)).expect("renders");
            assert!(png.starts_with(b"\x89PNG"));
        }
        assert!(card(vec![entry(1, "A<B")]).to_svg().contains("A&lt;B"));
    }

    #[test]
    fn the_podium_makes_the_card_taller_and_survives_awkward_names() {
        let r = Renderer::new();
        let place = |rank, name: &str| LastWeekEntry {
            rank,
            name: name.to_owned(),
            value: "4,210 XP".to_owned(),
        };
        let card = |last_week| LeaderboardCard {
            title: "Weekly XP".to_owned(),
            entries: (1..=3).map(|n| entry(n, "Tide")).collect(),
            page: 0,
            last_week,
            empty: "Nobody here yet.",
            theme: Theme::reef(),
        };
        let plain = r.leaderboard_card(&card(Vec::new())).expect("renders");
        let image = |png: &[u8]| image::load_from_memory(png).expect("a png");
        assert_eq!(image(&plain).height(), 716);
        for podium in [
            vec![
                place(1, "<b>Anemone & Co</b> with a very long name"),
                place(2, "Nori"),
                place(3, "kelp"),
            ],
            vec![place(1, "Solo")],
        ] {
            let tall = r.leaderboard_card(&card(podium)).expect("renders");
            assert_eq!(image(&tall).height(), 826);
        }
        assert!(card(vec![place(1, "A<B")]).to_svg().contains("A&lt;B"));
    }
}
