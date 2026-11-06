//! A look prepared for drawing: every pixel sorted by what it shows (skin, hair, shirt, eyes…)
//! so a recolour is one table lookup, and the anchor points accessories hang on, found in the
//! art itself.

use image::{Rgba, RgbaImage};

/// The outline every sprite is drawn with.
pub const OUTLINE: [u8; 3] = [0x1b, 0x1d, 0x2b];

pub const SKIN_KEY: [[u8; 3]; 4] = [
    [0x8a, 0x4b, 0x3b],
    [0xc9, 0x7a, 0x5c],
    [0xf0, 0xa9, 0x8a],
    [0xff, 0xd8, 0xc2],
];
pub const HAIR_KEY: [[u8; 3]; 4] = [
    [0x3b, 0x1f, 0x6b],
    [0x5e, 0x35, 0xa8],
    [0x8a, 0x5a, 0xd8],
    [0xb9, 0x94, 0xf0],
];
pub const BOTTOM_KEY: [[u8; 3]; 4] = [
    [0x1d, 0x2b, 0x53],
    [0x2e, 0x3d, 0x75],
    [0x4a, 0x5b, 0xa3],
    [0x8f, 0xa0, 0xd8],
];
pub const TOP_KEY: [[u8; 3]; 5] = [
    [0x0b, 0x5e, 0x6b],
    [0x0f, 0x8a, 0x9a],
    [0x19, 0xb8, 0xc8],
    [0x6f, 0xe0, 0xea],
    [0xc0, 0xf6, 0xfa],
];
/// The brown ramp the looks draw shoes in (also used for small details elsewhere).
pub const SHOE_KEY: [[u8; 3]; 3] = [[0x3b, 0x2a, 0x24], [0x6b, 0x4a, 0x3a], [0xa0, 0x70, 0x5a]];

/// What one pixel of a look shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    Clear,
    Keep,
    Skin(u8),
    Hair(u8),
    Top(u8),
    Bottom(u8),
    /// A shoe pixel in the brown shoe ramp.
    Shoe(u8),
    /// A light pixel on a shoe drawn in a skin colour (laces, soles); the shade of skin it was.
    ShoeShine(u8),
    /// Part of an eye: 0 the darkest (top of the iris) to 2 the lightest.
    Eye(u8),
}

/// Points of a look that accessories are placed by, in the look's own pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchors {
    pub width: i32,
    pub height: i32,
    /// The row a hat's brim sits on: a little above the eyes.
    pub brow: i32,
    /// Left and right edge of the head at the brow, a little hair included.
    pub skull: (i32, i32),
    /// Left and right edge of all the hair at the brow (big hair is wider than the skull).
    pub hair: (i32, i32),
    /// The highest pixel over the middle of the head.
    pub crown: i32,
    /// The far (left) and near (right) eye, `(x0, y0, x1, y1)` inclusive; the looks face left.
    pub eyes: [(i32, i32, i32, i32); 2],
    /// Where the collar is: the middle of the shirt's first row.
    pub neck: (i32, i32),
    /// Box around the shirt between the collar and the bottoms, `(x0, y0, x1, y1)`.
    pub torso: (i32, i32, i32, i32),
    /// The lowest point of the front (left) and back (right) hand.
    pub hands: [(i32, i32); 2],
}

impl Anchors {
    /// Middle of the head between the eyes and the back of the skull.
    pub fn head_x(&self) -> i32 {
        (self.skull.0 + self.skull.1) / 2
    }

    pub fn eye_centre(&self, i: usize) -> (i32, i32) {
        let (x0, y0, x1, y1) = self.eyes[i];
        ((x0 + x1) / 2, (y0 + y1 + 1) / 2)
    }
}

/// A look ready to recolour and dress.
pub(crate) struct Prepared {
    pub width: u32,
    pub height: u32,
    /// The art with stray edge pixels cleaned off.
    pub pixels: Vec<[u8; 4]>,
    pub parts: Vec<Part>,
    pub anchors: Anchors,
}

fn rgb_of(px: &[u8; 4]) -> [u8; 3] {
    [px[0], px[1], px[2]]
}

impl Prepared {
    pub fn new(sprite: &RgbaImage) -> Self {
        let (w, h) = sprite.dimensions();
        let mut pixels: Vec<[u8; 4]> = sprite
            .pixels()
            .map(|p| if p[3] > 127 { [p[0], p[1], p[2], 255] } else { [0; 4] })
            .collect();
        clean_fringe(&mut pixels, w as i32, h as i32);
        let grid = Grid {
            w: w as i32,
            h: h as i32,
            px: &pixels,
        };
        let mut parts: Vec<Part> = pixels
            .iter()
            .map(|p| {
                if p[3] == 0 {
                    return Part::Clear;
                }
                let c = rgb_of(p);
                let find = |keys: &[[u8; 3]]| keys.iter().position(|k| *k == c).map(|i| i as u8);
                if let Some(i) = find(&SKIN_KEY) {
                    Part::Skin(i)
                } else if let Some(i) = find(&HAIR_KEY) {
                    Part::Hair(i)
                } else if let Some(i) = find(&TOP_KEY) {
                    Part::Top(i)
                } else if let Some(i) = find(&BOTTOM_KEY) {
                    Part::Bottom(i)
                } else {
                    Part::Keep
                }
            })
            .collect();

        let shirt_top = grid
            .first_row(0, |c| TOP_KEY[1..].contains(&c), 4)
            .unwrap_or(h as i32 * 2 / 5);
        let bottoms_top = grid
            .first_row(shirt_top + 5, |c| BOTTOM_KEY[1..].contains(&c), 4)
            .unwrap_or(h as i32 * 3 / 5);
        settle_strays(&grid, &mut parts, shirt_top);
        mark_shoes(&grid, &mut parts);
        let eyes = find_eyes(&grid, shirt_top);
        if let Some(eyes) = eyes {
            for eye in eyes {
                mark_eye(&grid, &mut parts, eye);
            }
        }
        let anchors = anchors(&grid, shirt_top, bottoms_top, eyes);
        Self {
            width: w,
            height: h,
            pixels,
            parts,
            anchors,
        }
    }
}

/// Read access to a look's pixels with bounds checks.
struct Grid<'a> {
    w: i32,
    h: i32,
    px: &'a [[u8; 4]],
}

impl Grid<'_> {
    fn at(&self, x: i32, y: i32) -> Option<[u8; 3]> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        let p = &self.px[(y * self.w + x) as usize];
        (p[3] > 0).then(|| rgb_of(p))
    }

    fn is(&self, x: i32, y: i32, test: impl Fn([u8; 3]) -> bool) -> bool {
        self.at(x, y).is_some_and(test)
    }

    /// The first row from `from` down with at least `n` pixels passing `test`.
    fn first_row(&self, from: i32, test: impl Fn([u8; 3]) -> bool, n: usize) -> Option<i32> {
        (from.max(0)..self.h).find(|&y| (0..self.w).filter(|&x| self.is(x, y, &test)).count() >= n)
    }
}

fn is_skin(c: [u8; 3]) -> bool {
    SKIN_KEY.contains(&c)
}

fn is_shoe(c: [u8; 3]) -> bool {
    SHOE_KEY.contains(&c)
}

/// The art was cut from a sheet and colour-quantised, which leaves single dark pixels outside the
/// outline. A pixel that is not outline but touches both the background and the outline is one.
fn clean_fringe(pixels: &mut [[u8; 4]], w: i32, h: i32) {
    let solid = pixels.to_vec();
    let get = |x: i32, y: i32| -> Option<[u8; 3]> {
        if x < 0 || y < 0 || x >= w || y >= h {
            return None;
        }
        let p = &solid[(y * w + x) as usize];
        (p[3] > 0).then(|| rgb_of(p))
    };
    for y in 0..h {
        for x in 0..w {
            let Some(c) = get(x, y) else { continue };
            if c == OUTLINE {
                continue;
            }
            let near = [(1, 0), (-1, 0), (0, 1), (0, -1)].map(|(dx, dy)| get(x + dx, y + dy));
            let open = near.iter().filter(|n| n.is_none()).count();
            let lined = near.iter().filter(|n| **n == Some(OUTLINE)).count();
            if open >= 1 && lined >= 1 && open + lined >= 3 {
                pixels[(y * w + x) as usize] = [0; 4];
            }
        }
    }
}

/// Steps to the four sides of a pixel, and to all eight around it.
const SIDES: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const AROUND: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];

/// Connected groups (4-way) of the cells for which `test` holds.
fn groups(w: i32, h: i32, test: impl Fn(i32, i32) -> bool) -> Vec<Vec<(i32, i32)>> {
    groups_via(w, h, &SIDES, test)
}

/// Connected groups of the cells for which `test` holds, cells joined by any of `steps`.
fn groups_via(w: i32, h: i32, steps: &[(i32, i32)], test: impl Fn(i32, i32) -> bool) -> Vec<Vec<(i32, i32)>> {
    let mut seen = vec![false; (w * h) as usize];
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if seen[(y * w + x) as usize] || !test(x, y) {
                continue;
            }
            let mut group = vec![(x, y)];
            seen[(y * w + x) as usize] = true;
            let mut i = 0;
            while i < group.len() {
                let (gx, gy) = group[i];
                i += 1;
                for (dx, dy) in steps {
                    let (nx, ny) = (gx + dx, gy + dy);
                    if nx >= 0 && ny >= 0 && nx < w && ny < h && !seen[(ny * w + nx) as usize] && test(nx, ny) {
                        seen[(ny * w + nx) as usize] = true;
                        group.push((nx, ny));
                    }
                }
            }
            out.push(group);
        }
    }
    out
}

fn luma(c: [u8; 3]) -> i32 {
    i32::from(c[0]) * 3 + i32::from(c[1]) * 6 + i32::from(c[2])
}

/// The shade of `keys` nearest in brightness to `c`.
fn nearest(keys: &[[u8; 3]], c: [u8; 3]) -> u8 {
    let l = luma(c);
    (0..keys.len()).min_by_key(|&i| (luma(keys[i]) - l).abs()).unwrap_or(0) as u8
}

/// What a pixel is made of, for telling neighbours apart: 1 skin, 2 hair, 3 top, 4 bottom, 0 the rest.
fn kind(p: Part) -> u8 {
    match p {
        Part::Skin(_) => 1,
        Part::Hair(_) => 2,
        Part::Top(_) => 3,
        Part::Bottom(_) => 4,
        _ => 0,
    }
}

/// The art was colour-quantised, so a few pixels of one part landed on another part's key: the
/// navy of the trousers in a head of hair, a speck of skin in a pair of shorts. Clothes colours
/// above the collar are hair, and a lone pixel walled in by one other part joins that part.
fn settle_strays(grid: &Grid<'_>, parts: &mut [Part], shirt_top: i32) {
    let w = grid.w;
    for y in 0..shirt_top.min(grid.h) {
        for x in 0..w {
            let i = (y * w + x) as usize;
            parts[i] = match parts[i] {
                Part::Bottom(k) => Part::Hair(k),
                Part::Top(k) if y < shirt_top - 1 => Part::Hair(k.min(3)),
                other => other,
            };
        }
    }
    let before = parts.to_vec();
    for y in 0..grid.h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let mine = kind(before[i]);
            if mine == 0 {
                continue;
            }
            let near: Vec<u8> = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .filter_map(|(dx, dy)| {
                    let (nx, ny) = (x + dx, y + dy);
                    (nx >= 0 && ny >= 0 && nx < w && ny < grid.h).then(|| kind(before[(ny * w + nx) as usize]))
                })
                .collect();
            if near.contains(&mine) {
                continue;
            }
            for other in 1..=4u8 {
                if other != mine && near.iter().filter(|&&k| k == other).count() >= 3 {
                    let c = rgb_of(&grid.px[i]);
                    parts[i] = match other {
                        1 => Part::Skin(nearest(&SKIN_KEY, c)),
                        2 => Part::Hair(nearest(&HAIR_KEY, c)),
                        3 => Part::Top(nearest(&TOP_KEY, c)),
                        _ => Part::Bottom(nearest(&BOTTOM_KEY, c)),
                    };
                }
            }
        }
    }
    settle_stains(grid, parts, shirt_top);
}

/// The most pixels a stain of hair colour on the trousers has.
const STAIN_MAX: usize = 40;

/// Stains are the same quantiser mistake in bigger pieces: a few pixels of hair colour in a pair
/// of trousers or a skirt, joined to each other so the lone-pixel pass leaves them. A small
/// group of hair colour (pixels touching corner to corner are one group) below the collar that
/// only trousers border (the outline does not count) becomes trousers; a speck of up to three
/// pixels may have one other pixel against it. A bigger group with anything else against it is
/// not a stain, and a band that goes on (the hem frill of the gown) is too big to be one: both
/// stay.
fn settle_stains(grid: &Grid<'_>, parts: &mut [Part], shirt_top: i32) {
    let (w, h) = (grid.w, grid.h);
    let before = parts.to_vec();
    let hair_at = |x: i32, y: i32| matches!(before[(y * w + x) as usize], Part::Hair(_));
    for group in groups_via(w, h, &AROUND, |x, y| y >= shirt_top && hair_at(x, y)) {
        if group.len() > STAIN_MAX {
            continue;
        }
        let (mut bottoms, mut others) = (0, 0);
        for &(x, y) in &group {
            for (dx, dy) in SIDES {
                let (nx, ny) = (x + dx, y + dy);
                if group.contains(&(nx, ny)) || matches!(grid.at(nx, ny), None | Some(OUTLINE)) {
                    continue;
                }
                match kind(before[(ny * w + nx) as usize]) {
                    4 => bottoms += 1,
                    _ => others += 1,
                }
            }
        }
        if bottoms >= 2 && (others == 0 || (group.len() <= 3 && others * 2 <= bottoms)) {
            for (x, y) in group {
                let i = (y * w + x) as usize;
                parts[i] = Part::Bottom(nearest(&BOTTOM_KEY, rgb_of(&grid.px[i])));
            }
        }
    }
}

/// Shoes are the brown ramp in a band at the bottom of the sprite (boots reach higher than
/// sneakers). Skin-coloured specks inside a shoe are laces and soles, not feet.
fn mark_shoes(grid: &Grid<'_>, parts: &mut [Part]) {
    let (w, h) = (grid.w, grid.h);
    let mut top = h;
    let mut gap = 0;
    for y in (h / 2..h).rev() {
        if (0..w).filter(|&x| grid.is(x, y, is_shoe)).count() >= 2 {
            top = y;
            gap = 0;
        } else if top < h {
            gap += 1;
            if gap > 1 {
                break;
            }
        }
    }
    for y in top..h {
        for x in 0..w {
            if let Some(i) = grid.at(x, y).and_then(|c| SHOE_KEY.iter().position(|k| *k == c)) {
                parts[(y * w + x) as usize] = Part::Shoe(i as u8);
            }
        }
    }
    for group in groups(w, h, |x, y| y >= top && grid.is(x, y, is_skin)) {
        if group.len() > 12 {
            continue;
        }
        let (mut shoe, mut other) = (0, 0);
        for &(x, y) in &group {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if group.contains(&(nx, ny)) {
                    continue;
                }
                match grid.at(nx, ny) {
                    Some(c) if is_shoe(c) => shoe += 1,
                    Some(c) if c == OUTLINE => {}
                    _ => other += 1,
                }
            }
        }
        if shoe >= 1 && shoe * 10 >= (shoe + other) * 6 {
            for (x, y) in group {
                if let Part::Skin(i) = parts[(y * w + x) as usize] {
                    parts[(y * w + x) as usize] = Part::ShoeShine(i);
                }
            }
        }
    }
}

/// Eyes are short runs of dark pixels with skin on both sides, stacked a few rows deep, in the
/// face above the collar. Returns the far (left) and near (right) eye.
fn find_eyes(grid: &Grid<'_>, shirt_top: i32) -> Option<[(i32, i32, i32, i32); 2]> {
    let (w, h) = (grid.w, grid.h);
    let face_top = (0..shirt_top).find(|&y| (0..w).any(|x| grid.is(x, y, is_skin)))?;
    let dark = |c: [u8; 3]| c == OUTLINE || is_shoe(c) || c == [0xff, 0xf1, 0xe8] || c == [0xff, 0xd1, 0x66];
    let mut marked = vec![false; (w * h) as usize];
    for y in face_top..shirt_top {
        let mut x = 1;
        while x < w {
            if grid.is(x, y, dark) && grid.is(x - 1, y, is_skin) {
                let mut end = x;
                while end < w && grid.is(end, y, dark) {
                    end += 1;
                }
                let has_outline = (x..end).any(|i| grid.at(i, y) == Some(OUTLINE));
                if end - x <= 6 && has_outline && grid.is(end, y, is_skin) {
                    for i in x..end {
                        if grid.is(i, y, |c| c == OUTLINE || is_shoe(c)) {
                            marked[(y * w + i) as usize] = true;
                        }
                    }
                }
                x = end;
            } else {
                x += 1;
            }
        }
    }
    // Eight-way groups, so the lid and the iris below it are one eye.
    let mut seen = vec![false; marked.len()];
    let mut boxes = Vec::new();
    for start in 0..marked.len() {
        if !marked[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let (mut x0, mut y0, mut x1, mut y1, mut n) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN, 0);
        while let Some(i) = stack.pop() {
            let (x, y) = (i as i32 % w, i as i32 / w);
            (x0, y0, x1, y1, n) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y), n + 1);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx >= 0 && ny >= 0 && nx < w && ny < h {
                        let j = (ny * w + nx) as usize;
                        if marked[j] && !seen[j] {
                            seen[j] = true;
                            stack.push(j);
                        }
                    }
                }
            }
        }
        let low_enough = (y0 as f32) < face_top as f32 + (shirt_top - face_top) as f32 * 0.75;
        if y1 - y0 >= 2 && low_enough {
            boxes.push((n, (x0, y0, x1, y1)));
        }
    }
    boxes.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    let first = boxes.first()?.1;
    let second = boxes
        .iter()
        .skip(1)
        .map(|(_, b)| *b)
        .find(|b| (b.1 - first.1).abs() <= 6 && ((b.0 + b.2) - (first.0 + first.2)).abs() >= 8);
    let second = second.unwrap_or_else(|| {
        // One eye found: the other one sits as far again to its left.
        let (x0, y0, x1, y1) = first;
        (x0 - 8, y0, x1 - 9, y1)
    });
    Some(if first.0 < second.0 {
        [first, second]
    } else {
        [second, first]
    })
}

/// Tags an eye's iris pixels: the lid on top stays outline; below it the iris runs dark to light.
fn mark_eye(grid: &Grid<'_>, parts: &mut [Part], (x0, y0, x1, y1): (i32, i32, i32, i32)) {
    for y in y0 + 1..=y1 {
        for x in x0..=x1 {
            let Some(c) = grid.at(x, y) else { continue };
            let shade = if is_shoe(c) {
                2
            } else if c == OUTLINE {
                if y == y0 + 1 {
                    0
                } else if y == y1 && y1 - y0 >= 3 {
                    2
                } else {
                    1
                }
            } else {
                continue;
            };
            // An outline pixel with no skin or iris beside it is the face's edge, not the eye.
            let beside = [(1, 0), (-1, 0)].iter().any(|(dx, _)| {
                grid.is(x + dx, y, |n| {
                    is_skin(n) || is_shoe(n) || n == OUTLINE || n == [0xff, 0xf1, 0xe8]
                })
            });
            if beside {
                parts[(y * grid.w + x) as usize] = Part::Eye(shade);
            }
        }
    }
}

fn anchors(grid: &Grid<'_>, shirt_top: i32, bottoms_top: i32, eyes: Option<[(i32, i32, i32, i32); 2]>) -> Anchors {
    let (w, h) = (grid.w, grid.h);
    let shirt = |c: [u8; 3]| TOP_KEY[1..].contains(&c);
    let span = |y0: i32, y1: i32, test: &dyn Fn([u8; 3]) -> bool| {
        let mut r = (i32::MAX, i32::MIN);
        for y in y0.max(0)..y1.min(h) {
            for x in 0..w {
                if grid.is(x, y, test) {
                    r = (r.0.min(x), r.1.max(x));
                }
            }
        }
        (r.0 <= r.1).then_some(r)
    };
    let torso_x = span(shirt_top, bottoms_top, &shirt).unwrap_or((w / 5, w * 4 / 5));
    let collar = span(shirt_top, shirt_top + 2, &shirt).unwrap_or(torso_x);
    let eyes = eyes.unwrap_or_else(|| {
        let y = shirt_top * 3 / 5;
        [(w / 3 - 1, y, w / 3, y + 3), (w / 2, y, w / 2 + 1, y + 3)]
    });
    let eye_top = eyes[0].1.min(eyes[1].1);
    let eye_bottom = eyes[0].3.max(eyes[1].3);
    let brow = (eye_top - 4).max(2);
    // The face's width at the eyes, then a little hair either side.
    let face = span(eye_top, eye_bottom + 1, &is_skin).unwrap_or((eyes[0].0 - 3, eyes[1].2 + 8));
    let skull = (face.0 - 3, face.1 + 3);
    let mid = (skull.0 + skull.1) / 2;
    // The run of pixels through the middle of the head a little above the brow.
    let row = (brow - 2).max(0);
    let mut hair = (mid, mid);
    while hair.0 > 0 && grid.at(hair.0 - 1, row).is_some() {
        hair.0 -= 1;
    }
    while hair.1 < w - 1 && grid.at(hair.1 + 1, row).is_some() {
        hair.1 += 1;
    }
    let crown = (0..h)
        .find(|&y| (mid - 4..=mid + 4).any(|x| grid.at(x, y).is_some()))
        .unwrap_or(0);

    // Hands: skin below the collar at the sides of the body, the outermost blob on each side.
    let centre = (torso_x.0 + torso_x.1) / 2;
    let mut front: Option<(i32, i32, i32)> = None;
    let mut back: Option<(i32, i32, i32)> = None;
    let limit = (bottoms_top + 18).min(h);
    for group in groups(w, h, |x, y| y >= shirt_top + 4 && y < limit && grid.is(x, y, is_skin)) {
        if group.len() < 5 {
            continue;
        }
        let lowest = group.iter().map(|p| p.1).max().unwrap_or(0);
        let xs: Vec<i32> = group.iter().filter(|p| p.1 >= lowest - 2).map(|p| p.0).collect();
        let x = xs.iter().sum::<i32>() / xs.len() as i32;
        let reach = (x - centre).abs();
        let slot = if x < centre { &mut front } else { &mut back };
        if slot.is_none_or(|(_, _, r)| reach > r) {
            *slot = Some((x, lowest, reach));
        }
    }
    let hand_y = bottoms_top + 4;
    Anchors {
        width: w,
        height: h,
        brow,
        skull,
        hair,
        crown,
        eyes,
        neck: ((collar.0 + collar.1) / 2, shirt_top),
        torso: (torso_x.0, shirt_top, torso_x.1, bottoms_top),
        hands: [
            front.map_or((torso_x.0 - 1, hand_y), |(x, y, _)| (x, y)),
            back.map_or((torso_x.1 + 1, hand_y), |(x, y, _)| (x, y)),
        ],
    }
}

/// The look's pixels with each part painted by `paint` (`None`: keep the drawn colour).
pub(crate) fn recolor(prepared: &Prepared, paint: impl Fn(Part) -> Option<[u8; 3]>) -> RgbaImage {
    let mut table: Vec<(Part, Option<[u8; 3]>)> = Vec::with_capacity(32);
    let mut out = RgbaImage::new(prepared.width, prepared.height);
    for ((px, part), dst) in prepared.pixels.iter().zip(&prepared.parts).zip(out.pixels_mut()) {
        if *part == Part::Clear {
            continue;
        }
        let colour = match table.iter().find(|(p, _)| p == part) {
            Some((_, c)) => *c,
            None => {
                let c = if *part == Part::Keep { None } else { paint(*part) };
                table.push((*part, c));
                c
            }
        };
        let [r, g, b] = colour.unwrap_or([px[0], px[1], px[2]]);
        *dst = Rgba([r, g, b, 255]);
    }
    out
}
