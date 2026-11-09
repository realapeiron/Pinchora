use super::*;

// /Face

fn lens_box(a: &Anchors, i: usize, grow: i32) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = a.eyes[i];
    let near = i == 1;
    let (gx, gy) = if near { (grow + 1, grow) } else { (grow, grow) };
    (x0 - gx, y0 - gy + 1, x1 + gx, y1 + gy)
}

/// One lens: a box with its corners cut, `frame` round the edge and `fill` (glass, shine) inside.
fn lens(f: &mut Pen, (x0, y0, x1, y1): (i32, i32, i32, i32), frame: [u8; 3], fill: Option<([u8; 3], [u8; 3])>) {
    for y in y0..=y1 {
        for x in x0..=x1 {
            let edge = x == x0 || x == x1 || y == y0 || y == y1;
            let corner = (x == x0 || x == x1) && (y == y0 || y == y1);
            if corner {
                continue;
            }
            if edge {
                f.put(x, y, frame);
            } else if let Some((glass, shine)) = fill {
                f.put(x, y, if x == x0 + 1 && y == y0 + 1 { shine } else { glass });
            }
        }
    }
}

/// Rings round both eyes with a bridge and a temple to the ear; `fill` fills the lenses.
fn frames(f: &mut Pen, a: &Anchors, frame: [u8; 3], fill: Option<([u8; 3], [u8; 3])>) {
    for i in 0..2 {
        lens(f, lens_box(a, i, 1), frame, fill);
    }
    let (l, r) = (lens_box(a, 0, 1), lens_box(a, 1, 1));
    f.rect(l.2, l.1 + 1, r.0, l.1 + 1, frame);
    f.rect(r.2, r.1 + 1, a.skull.1 - 4, r.1 + 1, frame);
}

pub(super) fn round_glasses(p: &mut Pens, c: &Ctx<'_>) {
    frames(&mut p.front, c.a, c.t[1], None);
    let (l, r) = (lens_box(c.a, 0, 1), lens_box(c.a, 1, 1));
    p.front.put(l.0 + 1, l.1 + 1, WHITE[4]);
    p.front.put(r.0 + 1, r.1 + 1, WHITE[4]);
}

/// Two dark lenses that span both eyes, so they read as glasses and not as a patch on the near
/// eye: both as tall as the taller eye, the far one at least six wide (on a turned head the far
/// eye is a sliver), and a short temple to the ear.
pub(super) fn sunglasses(p: &mut Pens, c: &Ctx<'_>) {
    let a = c.a;
    let [far, near] = a.eyes;
    let rows = (far.3 - far.1).max(near.3 - near.1) + 2;
    let near_box = (near.0 - 2, near.1, near.2 + 1, near.1 + rows - 1);
    let far_right = far.2 + 1;
    let far_box = ((far.0 - 1).min(far_right - 5), far.1, far_right, far.1 + rows - 1);
    let f = &mut p.front;
    for lens_box in [far_box, near_box] {
        lens(f, lens_box, LINE, Some((c.t[1], c.t[3])));
    }
    f.rect(far_box.2, far_box.1 + 1, near_box.0, far_box.1 + 1, LINE);
    f.rect(
        near_box.2,
        near_box.1 + 1,
        (a.skull.1 - 4).min(near_box.2 + 4),
        near_box.1 + 1,
        LINE,
    );
    f.put(near_box.0 + 2, near_box.1 + 1, c.t[4]);
}

pub(super) fn heart_glasses(p: &mut Pens, c: &Ctx<'_>) {
    let a = c.a;
    let f = &mut p.front;
    for i in 0..2 {
        let (cx, cy) = a.eye_centre(i);
        let x0 = cx - if i == 1 { 3 } else { 2 };
        let rows: &[&str] = if i == 1 {
            &["#.#.#", "#hm#m#", "#mmmmm#", ".#mmm#.", "..#m#..", "...#..."]
        } else {
            &[".#.#.", "#hm#m#", "#mmmm#", ".#mm#.", "..##.."]
        };
        f.rows(x0 - 1, cy - 3, rows, |ch| match ch {
            '#' => Some(LINE),
            'm' => Some(c.t[2]),
            'h' => Some(c.t[4]),
            _ => None,
        });
    }
    let (l, r) = (a.eye_centre(0), a.eye_centre(1));
    f.rect(l.0 + 3, l.1 - 2, r.0 - 4, l.1 - 2, LINE);
    f.rect(r.0 + 4, r.1 - 2, a.skull.1 - 4, r.1 - 2, LINE);
}

pub(super) fn eyepatch(p: &mut Pens, c: &Ctx<'_>) {
    let a = c.a;
    let (x0, y0, x1, y1) = lens_box(a, 1, 1);
    // The strap runs over the brow to the back of the head and down past the far eye.
    p.front.line(x1, y0 + 1, a.skull.1 - 3, a.brow - 1, LINE);
    p.front.line(x0, y0 + 1, a.eyes[0].0 - 2, a.brow - 2, LINE);
    p.part(false, |f| {
        f.ellipse(
            (x0 + x1) as f32 / 2.0 + 0.5,
            (y0 + y1) as f32 / 2.0 + 0.5,
            (x1 - x0) as f32 / 2.0 + 0.6,
            (y1 - y0) as f32 / 2.0 + 0.6,
            CHARCOAL[1],
        );
        f.bevel(CHARCOAL[1], CHARCOAL[3], CHARCOAL[0]);
    });
}

pub(super) fn freckles(p: &mut Pens, c: &Ctx<'_>) {
    let dot = c.skin[0];
    for i in 0..2 {
        let (x0, _, x1, y1) = c.a.eyes[i];
        let y = y1 + 2;
        let spots: &[(i32, i32)] = if i == 1 {
            &[(0, 0), (2, 0), (4, 0), (1, 1), (3, 1)]
        } else {
            &[(0, 0), (2, 0), (1, 1)]
        };
        for &(dx, dy) in spots {
            let x = if i == 1 { x0 - 1 + dx } else { x1 - 2 + dx };
            p.front.put(x, y + dy, dot);
        }
    }
}

pub(super) fn blush(p: &mut Pens, c: &Ctx<'_>) {
    let pink = mix(c.skin[2], [0xff, 0x6f, 0x8a], 0.65);
    let light = mix(c.skin[3], [0xff, 0x9f, 0xb0], 0.55);
    let (_, _, nx1, ny1) = c.a.eyes[1];
    let (fx0, _, _, fy1) = c.a.eyes[0];
    p.front.rect(nx1 - 2, ny1 + 2, nx1 + 1, ny1 + 3, pink);
    p.front.rect(nx1 - 1, ny1 + 2, nx1, ny1 + 2, light);
    p.front.rect(fx0 - 2, fy1 + 2, fx0, fy1 + 3, pink);
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [0, 1, 2].map(|i| (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t).round() as u8)
}
