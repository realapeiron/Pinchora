use super::*;

/// A wing shape: an ellipse leaning `tilt` pixels per row, behind the back.
pub(super) fn wing(f: &mut Pen, cx: f32, cy: f32, rx: f32, ry: f32, tilt: f32, c: [u8; 3]) {
    for y in (cy - ry).floor() as i32..=(cy + ry).ceil() as i32 {
        let shift = (y as f32 - cy) * tilt;
        for x in (cx - rx - ry * tilt.abs()).floor() as i32..=(cx + rx + ry * tilt.abs()).ceil() as i32 {
            let (dx, dy) = ((x as f32 + 0.5 - cx - shift) / rx, (y as f32 + 0.5 - cy) / ry);
            if dx * dx + dy * dy <= 1.0 {
                f.put(x, y, c);
            }
        }
    }
}

pub(super) fn fairy_wings(p: &mut Pens, c: &Ctx<'_>) {
    let (tx0, ty, tx1, _) = c.a.torso;
    let mid = c.a.neck.0 as f32 + 3.0;
    let y = ty as f32 + 7.0;
    let span = ((tx1 - tx0) as f32 / 2.0).min(13.0);
    p.part(true, |f| {
        wing(f, mid + span + 5.0, y - 6.0, 7.5, 6.0, -0.5, c.t[3]);
        wing(f, mid - span - 5.0, y - 6.0, 7.5, 6.0, 0.5, c.t[3]);
        wing(f, mid + span + 2.0, y + 5.0, 5.0, 4.0, 0.4, c.t[2]);
        wing(f, mid - span - 2.0, y + 5.0, 5.0, 4.0, -0.4, c.t[2]);
        f.bevel(c.t[3], c.t[4], c.t[2]);
        // Veins from the shoulders out.
        f.line(
            (mid + span) as i32,
            y as i32,
            (mid + span + 10.0) as i32,
            (y - 9.0) as i32,
            c.t[1],
        );
        f.line(
            (mid - span) as i32,
            y as i32,
            (mid - span - 10.0) as i32,
            (y - 9.0) as i32,
            c.t[1],
        );
        f.put((mid + span + 8.0) as i32, (y - 9.0) as i32, WHITE[4]);
        f.put((mid - span - 8.0) as i32, (y - 9.0) as i32, WHITE[4]);
    });
}

pub(super) fn bat_wings(p: &mut Pens, c: &Ctx<'_>) {
    let (tx0, ty, tx1, _) = c.a.torso;
    let mid = c.a.neck.0 as f32 + 3.0;
    let span = ((tx1 - tx0) as f32 / 2.0 - 2.0).min(11.0);
    let y = ty as f32 + 5.0;
    // One wing, outward along +x: a bony top edge to a hooked tip, scalloped below.
    let shape = [
        (0.0, 2.0),
        (5.0, -5.0),
        (11.0, -9.0),
        (17.0, -8.0),
        (18.5, -10.0),
        (18.0, 2.0),
        (14.5, 0.0),
        (11.5, 6.0),
        (8.0, 3.0),
        (4.5, 9.0),
        (0.0, 7.0),
    ];
    p.part(true, |f| {
        for side in [-1.0f32, 1.0] {
            let base = mid + side * span;
            let pts: Vec<(f32, f32)> = shape.iter().map(|&(dx, dy)| (base + side * dx, y + dy)).collect();
            f.poly(&pts, c.t[2]);
            for &(dx, dy) in &shape[5..10] {
                f.line(
                    base as i32,
                    y as i32 + 1,
                    (base + side * dx * 0.9) as i32,
                    (y + dy * 0.6 - 2.0) as i32,
                    c.t[1],
                );
            }
            f.line(
                base as i32,
                y as i32 + 1,
                (base + side * 11.0) as i32,
                (y - 8.0) as i32,
                c.t[3],
            );
            f.line(
                (base + side * 11.0) as i32,
                (y - 8.0) as i32,
                (base + side * 17.5) as i32,
                (y - 9.0) as i32,
                c.t[3],
            );
        }
    });
}

pub(super) fn cape(p: &mut Pens, c: &Ctx<'_>) {
    let a = c.a;
    let (tx0, ty, tx1, _) = a.torso;
    let bottom = a.height - 18;
    let (l0, r0) = (tx0 as f32 + 3.0, tx1 as f32 - 2.0);
    let (l1, r1) = (tx0 as f32 - 4.0, tx1 as f32 + 8.0);
    p.part(true, |f| {
        let pts = [
            (l0, ty as f32),
            (r0, ty as f32),
            (r1, bottom as f32 - 2.0),
            ((l1 + r1) / 2.0 + 3.0, bottom as f32 + 1.0),
            (l1, bottom as f32 - 1.0),
        ];
        f.poly(&pts, c.t[2]);
        f.bevel(c.t[2], c.t[3], c.t[1]);
        // Folds running down from the shoulders, and the lining along the back edge.
        for k in [0.25f32, 0.55, 0.8] {
            let (xa, xb) = (l0 + (r0 - l0) * k, l1 + (r1 - l1) * k);
            f.line(xa as i32, ty + 8, xb as i32, bottom - 1, c.t[1]);
        }
        for y in ty + 4..bottom - 1 {
            let k = (y - ty) as f32 / (bottom - ty) as f32;
            let r = (r0 + (r1 - r0) * k) as i32;
            if f.has(r, y) {
                f.put(r, y, c.t[4]);
                f.put(r - 1, y, c.t[3]);
            }
        }
    });
    // The clasp at the collar.
    let (nx, ny) = a.neck;
    p.part(false, |f| {
        f.rows(nx - 1, ny, &["gg", "gw"], |ch| match ch {
            'g' => Some(GOLD[2]),
            'w' => Some(GOLD[4]),
            _ => None,
        });
    });
}

pub(super) fn surfboard(p: &mut Pens, c: &Ctx<'_>) {
    let (_, ty, tx1, _) = c.a.torso;
    let (cx, cy) = (tx1 as f32 + 3.0, ty as f32 + 4.0);
    let lean = -0.3;
    p.part(true, |f| {
        // A long board leaning back: an ellipse whose rows are shifted as it leans.
        let (rx, ry) = (6.0, 23.0);
        for y in (cy - ry) as i32..=(cy + ry) as i32 {
            let shift = (y as f32 - cy) * lean;
            for x in (cx - rx - 8.0) as i32..=(cx + rx + 8.0) as i32 {
                let (dx, dy) = ((x as f32 + 0.5 - cx - shift) / rx, (y as f32 + 0.5 - cy) / ry);
                if dx * dx + dy * dy <= 1.0 {
                    f.put(x, y, c.t[2]);
                }
            }
        }
        f.bevel(c.t[2], c.t[3], c.t[1]);
        // A white stripe down the middle and a stripe of the dye's dark across it.
        for y in (cy - ry + 3.0) as i32..=(cy + ry - 3.0) as i32 {
            let x = (cx + (y as f32 - cy) * lean).round() as i32;
            f.put(x, y, WHITE[3]);
            f.put(x - 1, y, WHITE[2]);
        }
        let band = (cy + 8.0) as i32;
        for x in (cx - 12.0) as i32..=(cx + 12.0) as i32 {
            if f.has(x, band) {
                f.put(x, band, c.t[0]);
                f.put(x, band + 1, c.t[1]);
            }
        }
    });
}

// /Hand

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
