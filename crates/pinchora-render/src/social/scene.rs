use super::*;

pub(super) fn scene(kind: Interaction, a: &Figure, b: &Figure, at: f32) -> RgbaImage {
    let mut canvas = backdrop(kind);
    twinkles(&mut canvas, at);
    match kind {
        Interaction::Hug => hug(&mut canvas, a, b, at),
        Interaction::Pat => pat(&mut canvas, a, b, at),
        Interaction::Wave => wave(&mut canvas, a, b, at),
        Interaction::HighFive => high_five(&mut canvas, a, b, at),
    }
    canvas
}

/// Little stars in the sky that blink at their own times.
pub(super) fn twinkles(canvas: &mut RgbaImage, at: f32) {
    let white = Rgba([255, 255, 255, 255]);
    for (i, &(x, y)) in [(24, 24), (206, 18), (118, 14), (44, 66), (196, 62), (150, 34), (84, 30)]
        .iter()
        .enumerate()
    {
        let phase = (at * 2.0 + i as f32 * 0.29).fract();
        let r = match (phase * 4.0) as i32 {
            0 => 1,
            1 => 2,
            2 => 1,
            _ => 0,
        };
        if r > 0 {
            sparkle(canvas, x, y, r, white, white);
        } else {
            put(canvas, x, y, white);
        }
    }
}

/// A hop that peaks at `height` while `p` goes from 0 to 1 and back to 0; flat when `p` is at rest.
fn hop(p: f32, height: f32) -> f32 {
    if p <= 0.0 || p >= 1.0 {
        0.0
    } else {
        height * (p * 2.0 * PI).sin().abs()
    }
}

/// Puts both figures down: shadows first, so a shadow never covers a foot.
pub(super) fn stand(
    canvas: &mut RgbaImage,
    kind: Interaction,
    a: (&Figure, Pose),
    b: (&Figure, Pose),
) -> (Placed, Placed) {
    let (pa, pb) = (a.0.place(a.1), b.0.place(b.1));
    pa.shadow(canvas, kind, a.1.lift);
    pb.shadow(canvas, kind, b.1.lift);
    pa.draw(canvas);
    pb.draw(canvas);
    (pa, pb)
}

fn spark(canvas: &mut RgbaImage, x: i32, y: i32, r: i32) {
    sparkle(canvas, x, y, r, Rgba([255, 255, 255, 255]), Rgba([255, 240, 150, 255]));
}

fn hug(canvas: &mut RgbaImage, a: &Figure, b: &Figure, at: f32) {
    // Steps in, holds the hug, steps back out.
    let walk = between(at, 0.04, 0.34) - between(at, 0.70, 0.98);
    let squeeze = between(at, 0.30, 0.42) - between(at, 0.68, 0.78);
    let pulse = (at * TAU * 4.0).sin();
    let pose = |home: f32, to: f32| Pose {
        x: lerp(home, to, walk),
        lift: hop(walk, 7.0) + squeeze * pulse.abs() * 1.5,
        sx: 1.0 - 0.03 * squeeze * pulse,
        sy: 1.0 + 0.03 * squeeze * pulse,
        lean: 6.0 * squeeze + 2.0 * (1.0 - squeeze) * walk,
    };
    let (pa, pb) = (pose(64.0, 102.0), pose(176.0, 138.0));
    let (placed_a, placed_b) = stand(canvas, Interaction::Hug, (a, pa), (b, pb));

    // Arms: held open while they come together, around each other once they meet. The left one
    // reaches round the other's back in front; the right one goes behind.
    let open = 1.0 - squeeze;
    if walk > 0.05 || squeeze > 0.0 {
        for (me, other, figure, drop) in [(&placed_a, &placed_b, a, 3.0), (&placed_b, &placed_a, b, 11.0)] {
            let dir = me.dir as f32;
            let wrap = (other.centre + dir * 9.0, other.far.1 + drop);
            let spread = (me.near.0 + dir * 21.0, me.near.1 + 8.0);
            let wrist = (lerp(wrap.0, spread.0, open), lerp(wrap.1, spread.1, open));
            let pull = (me.near.0 + dir * 8.0, me.near.1 + lerp(8.0, 11.0, open));
            let paint = |layer: &mut RgbaImage| {
                arm(layer, figure, me.near, pull, wrist);
                hand(layer, figure, wrist, dir * lerp(1.7, 0.7, open), me.dir > 0, 1.2);
            };
            if me.dir < 0 {
                behind(canvas, other, paint);
            } else {
                paint(canvas);
            }
        }
    }

    // Hearts drift up from between them while they hold on.
    for j in 0..4 {
        let born = 0.36 + 0.085 * j as f32;
        let age = (at - born).rem_euclid(1.0) / 0.34;
        if at >= 0.34 {
            float_heart(canvas, 120.0 + (j as f32 - 1.5) * 10.0, 46.0, age, 5.0, j as f32 * 1.7);
        }
    }
    if squeeze > 0.5 {
        for (i, &(dx, dy)) in [(-40, -4), (42, 0), (-12, -26), (18, -34)].iter().enumerate() {
            let k = ((at * 3.0 + i as f32 * 0.25).fract() * 3.0) as usize;
            spark(canvas, 120 + dx, 60 + dy, [1, 2, 1][k % 3]);
        }
    }
}

fn pat(canvas: &mut RgbaImage, a: &Figure, b: &Figure, at: f32) {
    let walk = between(at, 0.04, 0.30) - between(at, 0.80, 0.98);
    // The one patted ducks a little to meet the hand, then three pats, the hand coming down and
    // lifting again.
    let bow = between(at, 0.22, 0.32) - between(at, 0.78, 0.90);
    let pats = between(at, 0.30, 0.34) - between(at, 0.76, 0.80);
    let beat = ((at - 0.32) / 0.44 * 3.0).clamp(0.0, 3.0);
    let down = if pats > 0.0 { (beat * PI).sin().abs() } else { 0.0 };
    let squash = 0.05 * pats * down.powi(3);

    let pa = Pose {
        x: lerp(62.0, 112.0, walk),
        lift: hop(walk, 6.0),
        sx: 1.0,
        sy: 1.0 + 0.03 * bow,
        lean: 4.0 * pats + 2.0 * walk,
    };
    let pb = Pose {
        x: lerp(176.0, 148.0, walk),
        lift: 0.0,
        sx: 1.0 + 0.04 * bow + squash * 0.6,
        sy: 1.0 - 0.11 * bow - squash,
        lean: -3.0 * bow,
    };
    let (placed_a, placed_b) = stand(canvas, Interaction::Pat, (a, pa), (b, pb));

    // The hand rests on the other's head and presses down on each pat.
    let reach = between(at, 0.16, 0.34) - between(at, 0.78, 0.92);
    if reach > 0.0 {
        let rest = (placed_a.near.0 + 8.0, placed_a.near.1 + 14.0);
        let lifted = 7.0 * (1.0 - down) * pats;
        let target = (
            placed_b.centre - 9.0,
            placed_b.head_top + 2.0 - lifted - 6.0 * (1.0 - pats),
        );
        let wrist = (lerp(rest.0, target.0, reach), lerp(rest.1, target.1, reach));
        let pull = (wrist.0 - 4.0, placed_a.near.1 - 14.0 * reach + 6.0);
        arm(canvas, a, placed_a.near, pull, wrist);
        hand(
            canvas,
            a,
            wrist,
            lerp(0.3, 1.72 + 0.12 * (1.0 - down), reach),
            true,
            1.25,
        );
    }
    // A puff and a heart for each pat that lands.
    if pats > 0.5 {
        let n = beat.floor() as usize;
        if down > 0.75 {
            let (cx, cy) = (placed_b.centre.round() as i32, placed_b.head_top.round() as i32);
            for (dx, dy) in [(-20, 8), (20, 8), (-16, -2), (16, -2)] {
                spark(canvas, cx + dx, cy + dy, 2);
            }
        }
        float_heart(
            canvas,
            placed_b.centre + [-8.0, 9.0, 0.0][n % 3],
            placed_b.head_top - 6.0,
            beat.fract() * 0.9,
            4.0,
            n as f32,
        );
    }
}

fn wave(canvas: &mut RgbaImage, a: &Figure, b: &Figure, at: f32) {
    // A hops hello, waves, and B hops back, delighted.
    let raise = between(at, 0.12, 0.20) - between(at, 0.84, 0.92);
    let swing = if raise > 0.0 {
        ((at - 0.12) * TAU * 3.6).sin()
    } else {
        0.0
    };
    let pa = Pose {
        x: 76.0 + 4.0 * raise,
        lift: if (0.03..0.12).contains(&at) {
            7.0 * ((at - 0.03) / 0.09 * PI).sin()
        } else {
            0.0
        },
        sx: 1.0,
        sy: 1.0,
        lean: 3.0 * raise,
    };
    let hello = ((at - 0.40) / 0.14).clamp(0.0, 1.0);
    let pb = Pose {
        x: 164.0,
        lift: if (0.40..0.54).contains(&at) {
            9.0 * (hello * PI).sin()
        } else if (0.62..0.72).contains(&at) {
            5.0 * (((at - 0.62) / 0.10) * PI).sin()
        } else {
            0.0
        },
        sx: 1.0,
        sy: 1.0,
        lean: -3.0 * between(at, 0.38, 0.5),
    };
    let (placed_a, placed_b) = stand(canvas, Interaction::Wave, (a, pa), (b, pb));

    if raise > 0.3 {
        let wrist = (
            lerp(placed_a.near.0 + 6.0, placed_a.near.0 + 17.0, raise),
            lerp(placed_a.near.1 + 18.0, placed_a.near.1 - 20.0, raise),
        );
        let pull = (placed_a.near.0 + 16.0, placed_a.near.1 + 2.0);
        arm(canvas, a, placed_a.near, pull, wrist);
        hand(canvas, a, wrist, swing * 0.5, true, 1.3);
        // Lines beside the hand show which way it is going.
        if swing.abs() > 0.4 && raise > 0.8 {
            let color = hex(mix(Interaction::Wave.palette().3, [0x1b, 0x1d, 0x2b], 0.45));
            ticks(canvas, (wrist.0, wrist.1 - 6.0), swing.signum(), color);
        }
    }
    // B answers with a heart.
    if at > 0.46 && at < 0.9 {
        float_heart(
            canvas,
            placed_b.centre,
            placed_b.head_top - 4.0,
            (at - 0.46) / 0.44,
            3.0,
            1.0,
        );
    }
}

fn high_five(canvas: &mut RgbaImage, a: &Figure, b: &Figure, at: f32) {
    // Both run in with an arm up, slap hands in the middle, and drift back.
    let walk = between(at, 0.02, 0.32) - between(at, 0.74, 0.98);
    let clap = between(at, 0.28, 0.34) - between(at, 0.62, 0.74);
    let kick = (between(at, 0.32, 0.38) - between(at, 0.38, 0.56)).max(0.0);
    let pose = |home: f32, to: f32| Pose {
        x: lerp(home, to, walk),
        lift: hop(walk, 8.0) + 4.0 * kick,
        sx: 1.0 - 0.03 * kick,
        sy: 1.0 + 0.04 * kick,
        lean: 4.0 * walk - 3.0 * kick,
    };
    let (pa, pb) = (pose(56.0, 88.0), pose(184.0, 152.0));
    let (placed_a, placed_b) = stand(canvas, Interaction::HighFive, (a, pa), (b, pb));

    let meet = (120.0, 58.0);
    // Standing and waiting, an arm is the sprite's own; a second one drawn at the chest would float there.
    let raised = walk;
    for (me, figure) in [(&placed_a, a), (&placed_b, b)] {
        if raised < 0.12 {
            continue;
        }
        let dir = me.dir as f32;
        let up = (me.near.0 + dir * 12.0, me.near.1 - 22.0);
        let wrist = (
            lerp(me.near.0 + dir * 6.0, lerp(up.0, meet.0 - dir * 3.0, clap), raised),
            lerp(me.near.1 + 18.0, lerp(up.1, meet.1 + 4.0, clap), raised),
        );
        let pull = (me.near.0 + dir * 12.0, me.near.1 + 2.0 - 8.0 * raised);
        arm(canvas, figure, me.near, pull, wrist);
        hand(canvas, figure, wrist, dir * lerp(0.1, 0.35, clap), me.dir > 0, 1.3);
    }
    // The flash where the palms meet.
    let burst = between(at, 0.30, 0.34) * (1.0 - between(at, 0.44, 0.58));
    if burst > 0.02 {
        flash(canvas, (meet.0.round() as i32, meet.1.round() as i32 - 10), burst);
        let age = ((at - 0.32) / 0.3).clamp(0.0, 1.0);
        for k in 0..8 {
            let angle = k as f32 / 8.0 * TAU + 0.3;
            let dist = 16.0 + 28.0 * age;
            spark(
                canvas,
                (meet.0 + angle.cos() * dist).round() as i32,
                (meet.1 - 10.0 + angle.sin() * dist * 0.8).round() as i32,
                if age < 0.6 { 2 } else { 1 },
            );
        }
    }
}

/// A star-shaped flash of light, `size` from 0 to 1.
fn flash(canvas: &mut RgbaImage, centre: (i32, i32), size: f32) {
    let radius = 5.0 + 11.0 * size;
    let mut mask = Vec::new();
    let reach = radius as i32 + 2;
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let (fx, fy) = (dx as f32 + 0.5, dy as f32 + 0.5);
            let (rho, theta) = (fx.hypot(fy), fy.atan2(fx));
            let spikes = (theta * 4.0).cos().abs().powf(3.0);
            if rho <= radius * (0.28 + 0.72 * spikes) {
                let color = if rho < radius * 0.32 {
                    Rgba([255, 255, 255, 255])
                } else if rho < radius * 0.62 {
                    Rgba([255, 247, 180, 255])
                } else {
                    Rgba([255, 226, 102, 255])
                };
                mask.push((centre.0 + dx, centre.1 + dy, color));
            }
        }
    }
    stamp(canvas, &mask, Rgba([0xff, 0x9f, 0x1c, 255]));
}
