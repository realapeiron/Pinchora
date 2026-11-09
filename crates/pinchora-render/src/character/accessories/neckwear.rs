use super::*;

/// The collar's left and right edge and its row.
fn collar(a: &Anchors) -> (i32, i32, i32) {
    let (nx, ny) = a.neck;
    let half = ((a.torso.2 - a.torso.0) / 4).clamp(5, 9);
    (nx - half, nx + half, ny)
}

pub(super) fn scarf(p: &mut Pens, c: &Ctx<'_>) {
    let (l, r, y) = collar(c.a);
    p.part(false, |f| {
        f.rect(l - 1, y - 2, r + 1, y + 2, c.t[2]);
        f.rect(l - 1, y + 2, r + 1, y + 2, c.t[1]);
        f.rect(l, y - 2, r, y - 2, c.t[3]);
        for x in (l - 1..=r + 1).filter(|x| (x - l) % 4 == 1) {
            f.rect(x, y - 1, x + 1, y + 1, c.t[4]);
        }
        // The loose end hangs down the front.
        let tx = l + 1;
        f.rect(tx, y + 2, tx + 3, y + 10, c.t[2]);
        f.rect(tx, y + 5, tx + 3, y + 6, c.t[4]);
        f.rect(tx + 3, y + 2, tx + 3, y + 10, c.t[1]);
        for x in tx..=tx + 3 {
            if (x - tx) % 2 == 0 {
                f.put(x, y + 11, c.t[2]);
            }
        }
    });
}

fn necklace(p: &mut Pens, a: &Anchors, bead: impl Fn(usize) -> Option<[u8; 3]>, string: [u8; 3]) -> (i32, i32) {
    let (l, r, y) = collar(a);
    let (l, r) = (l + 1, r - 1);
    let mid = (l + r) as f32 / 2.0;
    let half = ((r - l) as f32 / 2.0).max(1.0);
    let mut low = (l, y);
    for (i, x) in (l..=r).enumerate() {
        let k = (x as f32 - mid) / half;
        let dy = (4.5 * (1.0 - k * k)).round() as i32;
        let py = y + dy;
        p.front.put(x, py, bead(i).unwrap_or(string));
        if dy >= low.1 - y {
            low = (x, py);
        }
    }
    low
}

pub(super) fn shell_necklace(p: &mut Pens, c: &Ctx<'_>) {
    let beads = [[0xff, 0xf4, 0xe0], [0xff, 0xb3, 0x9a], [0x7f, 0xd8, 0xd0]];
    let (lx, ly) = necklace(
        p,
        c.a,
        |i| (i % 2 == 0).then_some(beads[(i / 2) % 3]),
        [0xc8, 0xa8, 0x80],
    );
    p.part(false, |f| {
        f.rows(lx - 2, ly + 1, &[".lml.", "lmlml", "mlmlm", ".ddd."], |ch| match ch {
            'l' => Some(c.t[4]),
            'm' => Some(c.t[2]),
            'd' => Some(c.t[1]),
            _ => None,
        });
    });
}

pub(super) fn pearl_necklace(p: &mut Pens, c: &Ctx<'_>) {
    let (lx, ly) = necklace(p, c.a, |i| Some(if i % 2 == 0 { WHITE[4] } else { WHITE[1] }), WHITE[1]);
    p.part(false, |f| {
        f.ellipse(lx as f32 + 0.5, ly as f32 + 2.0, 1.6, 1.6, WHITE[3]);
        f.put(lx, ly + 1, WHITE[4]);
    });
}

pub(super) fn bow_tie(p: &mut Pens, c: &Ctx<'_>) {
    let (nx, ny) = c.a.neck;
    p.part(false, |f| {
        f.rows(
            nx - 4,
            ny,
            &["ll...ll", "lmm.mml", "mmmkmmm", "dmm.mmd", "dd...dd"],
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

pub(super) fn bandana(p: &mut Pens, c: &Ctx<'_>) {
    let (l, r, y) = collar(c.a);
    p.part(false, |f| {
        let mid = (l + r) / 2;
        for row in 0..=7 {
            let half = (r - l) / 2 + 1 - row;
            if half < 0 {
                break;
            }
            f.rect(mid - half, y - 1 + row, mid + half, y - 1 + row, c.t[2]);
        }
        f.rect(l - 1, y - 1, r + 1, y - 1, c.t[1]);
        for (x, yy) in f.cells() {
            if (x + yy * 2).rem_euclid(5) == 0 && f.get(x, yy) == Some(c.t[2]) {
                f.put(x, yy, WHITE[4]);
            }
        }
    });
}

// /Back
