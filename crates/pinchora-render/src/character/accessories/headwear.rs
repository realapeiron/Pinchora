use super::*;

pub(super) fn straw_hat(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, hx, b) = hat_span(c.a);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    p.part(false, |f| {
        f.dome(cx, b - 1, half - 1.0, 9.0, STRAW[2]);
        f.ellipse(cx, b as f32 + 0.5, half + 6.5, 2.4, STRAW[2]);
        f.bevel(STRAW[2], STRAW[3], STRAW[1]);
        // Woven straw: a light stitch every third pixel on the crown.
        for (x, y) in f.cells() {
            if y < b - 3 && (x + 2 * y).rem_euclid(4) == 0 && f.get(x, y) == Some(STRAW[2]) {
                f.put(x, y, STRAW[1]);
            }
        }
        // The ribbon round the crown and a little bow at its front.
        for x in (x0 + 1)..=(x1 - 1) {
            if f.has(x, b - 3) {
                f.put(x, b - 3, c.t[2]);
                f.put(x, b - 2, c.t[1]);
            }
        }
        let bx = x0 + 3;
        f.rows(bx - 1, b - 5, &[".ab.", "abba", ".bb."], |ch| match ch {
            'a' => Some(c.t[3]),
            'b' => Some(c.t[2]),
            _ => None,
        });
        // The brim's lower edge in shade.
        for x in (cx - half - 7.0) as i32..=(cx + half + 7.0) as i32 {
            if f.has(x, b + 2) && !f.has(x, b + 3) {
                f.put(x, b + 2, STRAW[1]);
            }
        }
    });
}

pub(super) fn beanie(p: &mut Pens, c: &Ctx<'_>) {
    let (s0, s1, _, _) = hat_span(c.a);
    let (x0, x1) = (s0.max(c.a.skull.0 - 2), s1.min(c.a.skull.1 + 2));
    let (hx, b) = ((x0 + x1) / 2, c.a.brow);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    let tall = (half * 0.8).clamp(10.0, 15.0);
    p.part(false, |f| {
        f.dome(cx, b - 2, half - 0.5, tall, c.t[2]);
        f.bevel(c.t[2], c.t[3], c.t[1]);
        // Knit stripes.
        for (x, y) in f.cells() {
            if (b - 2 - y) % 4 == 2 && f.get(x, y) == Some(c.t[2]) {
                f.put(x, y, c.t[3]);
            }
        }
        // The folded cuff, ribbed.
        f.rect(x0 - 1, b - 2, x1 + 1, b + 1, c.t[1]);
        for x in (x0 - 1)..=(x1 + 1) {
            if (x - x0) % 2 == 0 {
                f.rect(x, b - 1, x, b + 1, c.t[0]);
            }
        }
        f.rect(x0 - 1, b - 2, x1 + 1, b - 2, c.t[2]);
        // The pompom, sitting on top.
        let top = b - 2 - tall as i32;
        f.ellipse(cx + 1.0, top as f32 - 1.5, 3.8, 3.4, WHITE[3]);
        f.put(hx, top - 3, WHITE[4]);
        f.put(hx + 1, top - 4, WHITE[4]);
        f.put(hx + 3, top, WHITE[1]);
        f.put(hx + 4, top - 1, WHITE[1]);
    });
}

pub(super) fn sailor_cap(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, hx, b) = hat_span(c.a);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    p.part(false, |f| {
        f.dome(cx, b - 1, half - 1.0, 7.0, WHITE[2]);
        f.bevel(WHITE[2], WHITE[4], WHITE[1]);
        // The band round the bottom, with a gold anchor at the front.
        f.rect(x0 + 1, b - 1, x1 - 1, b + 1, c.t[2]);
        f.rect(x0 + 1, b + 1, x1 - 1, b + 1, c.t[1]);
        f.rect(x0 + 1, b - 1, x1 - 1, b - 1, c.t[3]);
        let ax = x0 + 5;
        f.rows(ax - 1, b - 1, &[".g.", "ggg", ".g."], |ch| {
            (ch == 'g').then_some(GOLD[2])
        });
    });
}

pub(super) fn captain_hat(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, hx, b) = hat_span(c.a);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    p.part(false, |f| {
        // The white top sits wider than the band, like a plate.
        f.ellipse(cx + 1.0, b as f32 - 6.0, half + 2.0, 4.0, WHITE[2]);
        f.bevel(WHITE[2], WHITE[4], WHITE[1]);
        f.rect(x0 + 1, b - 3, x1 - 1, b, c.t[2]);
        f.rect(x0 + 1, b - 3, x1 - 1, b - 3, GOLD[2]);
        f.rect(x0 + 1, b, x1 - 1, b, c.t[1]);
        // The badge at the front.
        let ax = x0 + 6;
        f.rows(ax - 2, b - 5, &[".ggg.", "gwgwg", ".ggg."], |ch| match ch {
            'g' => Some(GOLD[2]),
            'w' => Some(GOLD[4]),
            _ => None,
        });
    });
    // The black visor over the front of the brow.
    p.part(false, |f| {
        f.ellipse(x0 as f32 + 5.0, b as f32 + 1.5, 8.0, 1.6, CHARCOAL[1]);
        f.rect(x0 - 1, b + 1, x0 + 7, b + 1, CHARCOAL[3]);
    });
}

pub(super) fn top_hat(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, hx, b) = hat_span(c.a);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    let (l, r) = ((cx - half + 4.0) as i32, (cx + half - 4.0) as i32);
    p.part(false, |f| {
        f.rect(l, b - 17, r, b - 1, CHARCOAL[2]);
        f.bevel(CHARCOAL[2], CHARCOAL[3], CHARCOAL[1]);
        f.rect(l, b - 17, r, b - 17, CHARCOAL[3]);
        f.rect(l, b - 5, r, b - 3, c.t[2]);
        f.rect(l, b - 3, r, b - 3, c.t[1]);
        f.rect(l, b - 5, r, b - 5, c.t[3]);
        // A shine down the front of the tube.
        f.rect(l + 2, b - 15, l + 2, b - 8, CHARCOAL[4]);
        f.ellipse(cx, b as f32 + 0.5, half + 3.5, 1.8, CHARCOAL[1]);
        f.rect((cx - half - 2.0) as i32, b, (cx + half + 2.0) as i32, b, CHARCOAL[2]);
    });
}

pub(super) fn witch_hat(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, hx, b) = hat_span(c.a);
    let half = (x1 - x0) as f32 / 2.0;
    let cx = hx as f32 + 0.5;
    p.part(false, |f| {
        // A cone narrowing to a tip that flops back.
        let height = 22;
        for i in 0..=height {
            let y = b - 1 - i;
            let k = i as f32 / height as f32;
            let w = (half - 2.0) * (1.0 - k).powf(0.85) + 0.5;
            let bend = 6.0 * k * k;
            f.rect((cx - w + bend) as i32, y, (cx + w + bend) as i32, y, c.t[2]);
        }
        f.bevel(c.t[2], c.t[3], c.t[1]);
        f.ellipse(cx, b as f32 + 0.5, half + 7.0, 2.3, c.t[1]);
        f.rect((cx - half - 6.0) as i32, b - 1, (cx + half + 6.0) as i32, b - 1, c.t[2]);
        // The band and its buckle.
        for x in x0..=x1 {
            if f.get(x, b - 3) == Some(c.t[2]) || f.get(x, b - 3) == Some(c.t[3]) || f.get(x, b - 3) == Some(c.t[1]) {
                f.put(x, b - 3, c.t[0]);
                f.put(x, b - 2, c.t[0]);
            }
        }
        f.rows(hx - 4, b - 4, &["gggg", "g..g", "gggg"], |ch| {
            (ch == 'g').then_some(GOLD[2])
        });
    });
}

pub(super) fn crown(p: &mut Pens, c: &Ctx<'_>) {
    let x = c.a.head_x() - 7;
    let y = c.a.crown - 4;
    p.part(false, |f| {
        f.rows(
            x,
            y,
            &[
                ".w.....w.....w.",
                ".g....ggg....g.",
                ".gg..ggggg..gg.",
                ".gggggggggggggg",
                ".hhhhhhhhhhhhhh",
                ".gg#rg##bg##rgg",
                ".ggg#ggrgg#gggg",
                ".ddddddddddddd.",
            ],
            |ch| match ch {
                'w' => Some(GOLD[4]),
                'g' => Some(GOLD[2]),
                'h' => Some(GOLD[3]),
                'd' => Some(GOLD[1]),
                'r' => Some([0xe2, 0x3d, 0x52]),
                'b' => Some([0x4a, 0xa8, 0xec]),
                '#' => Some(GOLD[1]),
                _ => None,
            },
        );
    });
}

pub(super) fn bow(p: &mut Pens, c: &Ctx<'_>) {
    let x = c.a.skull.0 + 1;
    let y = c.a.crown + 2;
    p.part(false, |f| {
        f.rows(
            x,
            y,
            &[
                ".ll......ll.",
                "lmmm....mmml",
                "lmmmm..mmmml",
                "mmmmmkkmmmmm",
                "dmmmmkkmmmmd",
                ".ddm.dd.mdd.",
                "....d..d....",
            ],
            |ch| match ch {
                'l' => Some(c.t[3]),
                'm' => Some(c.t[2]),
                'd' => Some(c.t[1]),
                'k' => Some(c.t[0]),
                _ => None,
            },
        );
    });
}

pub(super) fn flower_crown(p: &mut Pens, c: &Ctx<'_>) {
    let (x0, x1, _, b) = head(c.a);
    let y = b - 1;
    p.part(false, |f| {
        f.rect(x0, y, x1, y + 1, LEAF[1]);
        let mut x = x0;
        let mut i = 0;
        while x <= x1 {
            let petal = match i % 3 {
                0 => c.t,
                1 => [WHITE[1], WHITE[2], WHITE[3], WHITE[4], WHITE[4]],
                _ => [GOLD[0], GOLD[1], GOLD[2], GOLD[3], GOLD[4]],
            };
            f.rows(x - 1, y - 2, &[".p.", "pcp", ".p."], |ch| match ch {
                'p' => Some(petal[3]),
                'c' => Some(if i % 3 == 2 { [0xff, 0x8a, 0x30] } else { GOLD[2] }),
                _ => None,
            });
            f.put(x + 2, y, LEAF[3]);
            x += 5;
            i += 1;
        }
    });
}
