use super::*;

pub(super) fn lantern(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    let x = hx + 2;
    p.part(false, |f| {
        f.rows(
            x - 4,
            hy - 1,
            &[
                "...mm....",
                "..m..m...",
                "...mm....",
                ".mmmmmmm.",
                "mmmmmmmmm",
                ".m.wwy.m.",
                ".myyyyym.",
                ".myyyyom.",
                ".myyyoom.",
                ".myyoooom",
                ".mmmmmmm.",
                "mmmmmmmmm",
                ".mm...mm.",
            ],
            |ch| match ch {
                'm' => Some(CHARCOAL[2]),
                'w' => Some([0xff, 0xfb, 0xe0]),
                'y' => Some([0xff, 0xe0, 0x66]),
                'o' => Some([0xff, 0xb0, 0x3a]),
                _ => None,
            },
        );
        f.rect(x - 3, hy + 3, x + 3, hy + 3, CHARCOAL[3]);
    });
}

pub(super) fn fishing_rod(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    let tip = (hx + 13, hy - 38);
    p.front.line(tip.0, tip.1, tip.0 + 1, tip.1 + 22, WHITE[1]);
    p.part(false, |f| {
        f.line(hx - 1, hy + 3, tip.0, tip.1, WOOD[2]);
        f.line(hx, hy + 3, tip.0 + 1, tip.1, WOOD[3]);
        f.rect(hx - 1, hy - 1, hx + 1, hy + 1, CHARCOAL[3]);
        f.put(hx + 2, hy, CHARCOAL[2]);
    });
    p.part(false, |f| {
        f.rows(tip.0, tip.1 + 22, &[".r.", "rrr", "www", ".w."], |ch| match ch {
            'r' => Some(c.t[2]),
            'w' => Some(WHITE[3]),
            _ => None,
        });
    });
}

pub(super) fn ice_cream(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    let wafer = [[0xb8, 0x78, 0x3a], [0xe6, 0xae, 0x62], [0xf6, 0xcc, 0x86]];
    p.part(false, |f| {
        f.rows(
            hx - 4,
            hy - 9,
            &[
                "lwvwvwv", "lvwvwvw", ".wvwvw.", ".vwvwv.", "..wvw..", "..vwv..", "...w...", "...v...", "...w...",
            ],
            |ch| match ch {
                'l' => Some(wafer[2]),
                'w' => Some(wafer[1]),
                'v' => Some(wafer[0]),
                _ => None,
            },
        );
    });
    p.part(false, |f| {
        f.ellipse(hx as f32, hy as f32 - 13.0, 4.8, 4.4, c.t[2]);
        f.bevel(c.t[2], c.t[3], c.t[1]);
        // A drip over the cone's rim and a cherry on top.
        f.rect(hx - 3, hy - 10, hx - 3, hy - 8, c.t[2]);
        f.rect(hx + 1, hy - 10, hx + 1, hy - 9, c.t[2]);
        f.put(hx - 2, hy - 15, c.t[4]);
        f.put(hx - 1, hy - 16, c.t[4]);
    });
    p.part(false, |f| {
        f.rows(hx, hy - 20, &[".s", "rr", "rR"], |ch| match ch {
            's' => Some(LEAF[2]),
            'r' => Some([0xe2, 0x3d, 0x52]),
            'R' => Some([0xff, 0x9f, 0xa8]),
            _ => None,
        });
    });
}

pub(super) fn balloon(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    let knot = (hx.max(c.a.skull.1 - 2) + 9, c.a.neck.1 - 8);
    p.part(false, |f| {
        f.ellipse(knot.0 as f32 + 0.5, knot.1 as f32 - 7.5, 7.0, 8.0, c.t[2]);
        f.bevel(c.t[2], c.t[3], c.t[1]);
        f.rect(knot.0 - 4, knot.1 - 12, knot.0 - 4, knot.1 - 9, c.t[4]);
        f.rect(knot.0 - 3, knot.1 - 13, knot.0 - 2, knot.1 - 13, c.t[4]);
        f.rows(knot.0 - 1, knot.1, &[".k.", "kkk"], |ch| (ch == 'k').then_some(c.t[1]));
    });
    // The string sways from the knot down to the hand.
    let steps = (hy - knot.1 - 2).max(1);
    for i in 0..=steps {
        let y = knot.1 + 2 + i;
        let k = i as f32 / steps as f32;
        let x = knot.0 as f32 + (hx - knot.0) as f32 * k + (k * 7.0).sin() * 1.2;
        p.front.put(x.round() as i32, y, WHITE[1]);
    }
}

pub(super) fn flower(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    p.part(false, |f| {
        f.rect(hx, hy - 12, hx, hy + 2, LEAF[1]);
        f.rows(hx + 1, hy - 8, &["ll.", "lLl", ".l."], |ch| match ch {
            'l' => Some(LEAF[2]),
            'L' => Some(LEAF[3]),
            _ => None,
        });
        f.rows(hx - 3, hy - 5, &[".ll", "lL."], |ch| match ch {
            'l' => Some(LEAF[2]),
            'L' => Some(LEAF[3]),
            _ => None,
        });
    });
    p.part(false, |f| {
        f.rows(
            hx - 4,
            hy - 20,
            &[
                "..ppp..", ".pPpPp.", "ppPcPpp", "pPcCcPp", "ppPcPpp", ".pPpPp.", "..ppp..",
            ],
            |ch| match ch {
                'p' => Some(c.t[2]),
                'P' => Some(c.t[3]),
                'c' => Some(GOLD[2]),
                'C' => Some(GOLD[4]),
                _ => None,
            },
        );
    });
}

pub(super) fn star_wand(p: &mut Pens, c: &Ctx<'_>) {
    let (hx, hy) = c.a.hands[1];
    p.part(false, |f| {
        f.line(hx, hy + 2, hx + 3, hy - 13, WHITE[3]);
        for k in [hy - 2, hy - 6, hy - 10] {
            let x = hx + (hy + 2 - k) * 3 / 15;
            f.put(x, k, [0xff, 0x8f, 0xb1]);
        }
    });
    p.part(false, |f| {
        f.rows(
            hx - 1,
            hy - 23,
            &[
                "....s....",
                "...sss...",
                "...sSs...",
                "sssssssss",
                ".ssSSsss.",
                "..sssss..",
                "..ss.ss..",
                ".ss...ss.",
            ],
            |ch| match ch {
                's' => Some(c.t[2]),
                'S' => Some(c.t[4]),
                _ => None,
            },
        );
        f.bevel(c.t[2], c.t[3], c.t[1]);
    });
    p.front.put(hx + 9, hy - 22, WHITE[4]);
    p.front.put(hx - 3, hy - 16, WHITE[4]);
}
