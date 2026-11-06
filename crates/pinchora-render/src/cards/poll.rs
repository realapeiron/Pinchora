use std::fmt::Write;

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{CRAB, STAR, background, escape, panel, pixel_width, segment_bar, sprite, truncate_to, wrap},
};

/// One option of a poll and the votes it has.
pub struct PollOption {
    pub label: String,
    pub votes: i64,
}

/// 900-wide poll results card: the question, a bar for each option with its votes and share, a star
/// on the leader (or leaders, on a tie), and the total. Taller with more options and a long question.
pub struct PollCard {
    /// The poll's number, shown as `POLL #12`.
    pub number: i64,
    pub question: String,
    /// Two to five options.
    pub options: Vec<PollOption>,
    /// Closed polls say `CLOSED` and show final results; open ones say `OPEN`.
    pub closed: bool,
    pub theme: Theme,
}

/// Bar colours, one per option.
const BARS: [&str; 5] = ["#ff6b4a", "#06d6a0", "#8fd3ff", "#c4a1ff", "#ff8fb1"];

const ROW_H: f32 = 76.0;
const ROW_GAP: f32 = 10.0;

impl PollCard {
    pub const WIDTH: f32 = 900.0;

    /// The share of `votes` in `total`, rounded to a whole percent; 0 with no votes at all.
    pub fn percent(votes: i64, total: i64) -> i64 {
        if total <= 0 {
            0
        } else {
            (votes * 100 + total / 2) / total
        }
    }

    pub fn to_svg(&self) -> String {
        let t = &self.theme;
        let lines = wrap(&self.question, 40, 2);
        let top = 62.0 + lines.len() as f32 * 38.0 + 22.0;
        let rows = self.options.len() as f32;
        let (w, h) = (Self::WIDTH, top + rows * (ROW_H + ROW_GAP) + 52.0);
        let total: i64 = self.options.iter().map(|o| o.votes.max(0)).sum();
        let best = self.options.iter().map(|o| o.votes).max().unwrap_or(0);

        let mut s = String::with_capacity(32 * 1024);
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        background(&mut s, w, h, &t.bg_top, &t.bg_bottom);
        text(
            &mut s,
            36.0,
            48.0,
            16.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &format!("POLL #{}", self.number),
        );
        let (status, color) = if self.closed {
            ("CLOSED", &t.gold)
        } else {
            ("OPEN", &t.mint)
        };
        // On a plate, so the card's sparkles never land on the letters.
        let chip = pixel_width(status, 16.0) + 24.0;
        panel(&mut s, 864.0 - chip, 24.0, chip, 32.0, 4.0, &t.panel, color);
        text(
            &mut s,
            864.0 - chip / 2.0,
            47.0,
            16.0,
            PIXEL,
            false,
            color,
            "middle",
            status,
        );
        for (i, line) in lines.iter().enumerate() {
            text(
                &mut s,
                36.0,
                92.0 + i as f32 * 38.0,
                30.0,
                SANS,
                true,
                &t.ink,
                "start",
                // Wrapping counts letters, so wide ones still get cut at the edge; bold needs the margin.
                &escape(&truncate_to(line, 30.0, 760.0)),
            );
        }

        for (i, option) in self.options.iter().enumerate() {
            let y = top + i as f32 * (ROW_H + ROW_GAP);
            let leader = best > 0 && option.votes == best;
            let edge = if leader { &t.gold } else { &t.panel_edge };
            panel(&mut s, 36.0, y, 828.0, ROW_H, 4.0, &t.panel, edge);
            let mut label_x = 56.0;
            if leader {
                sprite(&mut s, STAR, label_x, y + 12.0, 3.0, &t.gold, "#fff1e8");
                label_x += 30.0;
            }
            text(
                &mut s,
                label_x,
                y + 30.0,
                22.0,
                SANS,
                true,
                &t.ink,
                "start",
                &escape(&truncate_to(&option.label, 22.0, 520.0 - (label_x - 56.0))),
            );
            let votes = option.votes.max(0);
            text(
                &mut s,
                844.0,
                y + 29.0,
                16.0,
                PIXEL,
                false,
                &t.ink,
                "end",
                &format!(
                    "{votes} VOTE{}   {}%",
                    if votes == 1 { "" } else { "S" },
                    Self::percent(votes, total)
                ),
            );
            let fraction = if total > 0 { votes as f32 / total as f32 } else { 0.0 };
            segment_bar(
                &mut s,
                56.0,
                y + 40.0,
                788.0,
                26.0,
                fraction,
                40,
                BARS[i % BARS.len()],
                &t.panel_edge,
                &t.panel_edge,
            );
        }

        let footer = h - 16.0;
        text(
            &mut s,
            36.0,
            footer,
            14.0,
            PIXEL,
            false,
            &t.muted,
            "start",
            &format!("{total} VOTE{} IN ALL", if total == 1 { "" } else { "S" }),
        );
        sprite(&mut s, CRAB, 752.0, footer - 12.0, 2.0, &t.accent, "#fff1e8");
        text(&mut s, 864.0, footer, 14.0, PIXEL, false, &t.muted, "end", "PINCHORA");
        t.draw_frame(&mut s, w, h);
        s.push_str("</svg>");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Renderer;

    fn card(question: &str, votes: &[i64], closed: bool) -> PollCard {
        PollCard {
            number: 7,
            question: question.to_owned(),
            options: votes
                .iter()
                .enumerate()
                .map(|(i, v)| PollOption {
                    label: format!("Option number {}", i + 1),
                    votes: *v,
                })
                .collect(),
            closed,
            theme: Theme::reef(),
        }
    }

    #[test]
    fn shares_round_to_whole_percents() {
        assert_eq!(PollCard::percent(0, 0), 0);
        assert_eq!(PollCard::percent(1, 3), 33);
        assert_eq!(PollCard::percent(2, 3), 67);
        assert_eq!(PollCard::percent(5, 5), 100);
        assert_eq!(PollCard::percent(1, 8), 13);
    }

    #[test]
    fn renders_every_shape_of_poll() {
        let r = Renderer::new();
        let image = |png: &[u8]| image::load_from_memory(png).expect("a png");
        let two = r.poll_card(&card("Pizza or tacos?", &[0, 0], false)).expect("renders");
        let five = r
            .poll_card(&card(
                "Which one is the very best of them all, really?",
                &[9, 9, 3, 0, 1],
                true,
            ))
            .expect("renders");
        assert!(two.starts_with(b"\x89PNG"));
        assert_eq!(image(&two).width(), 900);
        assert!(
            image(&five).height() > image(&two).height(),
            "more options, a taller card"
        );
    }

    #[test]
    fn awkward_text_is_escaped_and_long_text_is_cut() {
        let long = "x".repeat(300);
        let svg = card(&format!("<b>Tide & \"Pool\"</b> {long}"), &[1, 2], false).to_svg();
        assert!(svg.contains("&lt;b&gt;Tide &amp;"));
        assert!(!svg.contains("<b>"));
        let mut c = card("Q", &[1, 0], true);
        c.options[0].label = "w".repeat(200);
        assert!(Renderer::new().poll_card(&c).is_ok());
    }

    #[test]
    fn only_the_leaders_get_a_star() {
        let stars = |votes: &[i64]| card("Q", votes, false).to_svg().matches("fill=\"#ffd166\"").count();
        let none = stars(&[0, 0, 0]);
        let one = stars(&[3, 1, 0]);
        let tie = stars(&[2, 2, 0]);
        assert!(one > none, "a leader is marked");
        assert!(tie > one, "both tied leaders are marked");
    }
}
