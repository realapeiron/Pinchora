use super::*;

/// A wing shape: an ellipse leaning `tilt` pixels per row, behind the back.
fn wing(f: &mut Pen, (cx, cy): (f32, f32), (rx, ry): (f32, f32), tilt: f32, c: [u8; 3]) {
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
        wing(f, (mid + span + 5.0, y - 6.0), (7.5, 6.0), -0.5, c.t[3]);
        wing(f, (mid - span - 5.0, y - 6.0), (7.5, 6.0), 0.5, c.t[3]);
        wing(f, (mid + span + 2.0, y + 5.0), (5.0, 4.0), 0.4, c.t[2]);
        wing(f, (mid - span - 2.0, y + 5.0), (5.0, 4.0), -0.4, c.t[2]);
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

pub(super) fn backpack(p: &mut Pens, c: &Ctx<'_>) {
    let (_, ty, _, _) = c.a.torso;
    let (nx, ny) = c.a.neck;
    let (x0, x1) = (nx + 5, nx + 17);
    let (y0, y1) = (ty + 1, ty + 18);
    p.part(true, |f| {
        f.rect(x0, y0 + 1, x1, y1, c.t[2]);
        f.rect(x0 + 1, y0, x1 - 1, y0, c.t[2]);
        f.bevel(c.t[2], c.t[3], c.t[1]);
        // The flap, its buckle and the pocket.
        f.rect(x0 + 1, y0 + 1, x1 - 1, y0 + 5, c.t[1]);
        f.rect(x0 + 2, y0 + 1, x1 - 2, y0 + 1, c.t[2]);
        f.rect(x0 + 1, y0 + 6, x1 - 1, y0 + 6, c.t[0]);
        f.rect(x0 + 4, y1 - 7, x1 - 1, y1 - 1, c.t[3]);
        f.rect(x0 + 4, y1 - 7, x1 - 1, y1 - 7, c.t[4]);
        f.rect(x0 + 4, y1 - 1, x1 - 1, y1 - 1, c.t[1]);
        f.rect(x0 + 7, y0 + 5, x0 + 8, y0 + 7, GOLD[2]);
    });
    // A strap over the front shoulder.
    p.part(false, |f| {
        f.rect(nx + 3, ny + 1, nx + 4, ty + 13, c.t[1]);
        f.put(nx + 3, ny + 1, c.t[2]);
        f.rect(nx + 3, ty + 9, nx + 4, ty + 9, GOLD[2]);
    });
}
