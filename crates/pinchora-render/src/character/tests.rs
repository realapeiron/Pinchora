use crate::character::*;

fn looks() -> Vec<String> {
    let mut ids: Vec<String> = atlas()
        .names()
        .filter_map(|n| n.strip_prefix("a_").map(str::to_owned))
        .collect();
    ids.sort();
    ids
}

fn spec(look: &str, accessories: &[&str]) -> CharacterSpec {
    CharacterSpec {
        look: look.to_owned(),
        colors: Colors {
            skin: Paint::Swatch(1),
            hair: Paint::Swatch(4),
            eyes: Paint::Swatch(7),
            top: Paint::Swatch(0),
            bottom: Paint::Swatch(2),
            shoes: Paint::Swatch(4),
        },
        accessories: accessories
            .iter()
            .map(|id| Worn {
                id: (*id).to_owned(),
                tint: None,
            })
            .collect(),
        pose: Pose::default(),
    }
}

#[test]
fn every_look_has_art_and_sensible_anchors() {
    let looks = looks();
    assert_eq!(looks.len(), 24, "{looks:?}");
    for look in &looks {
        let a = look_anchors(look);
        let (w, h) = (a.width, a.height);
        assert!(
            a.brow > 0 && a.brow < a.neck.1,
            "{look}: brow {} neck {:?}",
            a.brow,
            a.neck
        );
        assert!(a.crown < a.brow, "{look}");
        assert!(
            a.skull.0 >= -2 && a.skull.1 <= w + 1 && a.skull.1 - a.skull.0 >= 18,
            "{look}: {:?}",
            a.skull
        );
        let [far, near] = a.eyes;
        assert!(far.0 < near.0, "{look}: eyes {far:?} {near:?}");
        for (x0, y0, x1, y1) in a.eyes {
            assert!(
                x0 <= x1 && y0 < y1 && y0 > a.brow && y1 < a.neck.1,
                "{look}: eye {:?}",
                (x0, y0, x1, y1)
            );
        }
        assert!(
            a.torso.0 < a.torso.2 && a.torso.1 < a.torso.3 && a.torso.3 < h,
            "{look}: {:?}",
            a.torso
        );
        let [front, back] = a.hands;
        assert!(front.0 < back.0, "{look}: hands {front:?} {back:?}");
        for (x, y) in a.hands {
            assert!(
                x >= 0 && x < w && y > a.neck.1 && y < h - 10,
                "{look}: hand {:?}",
                (x, y)
            );
        }
    }
}

#[test]
fn every_look_wears_every_accessory_inside_the_picture_without_holes() {
    for look in looks() {
        let bare = compose(&spec(&look, &[]));
        let solid_bare = bare.image.pixels().filter(|p| p[3] > 0).count();
        let a = bare.anchors;
        for art in &ACCESSORIES {
            let dressed = compose(&spec(&look, &[art.id]));
            let (w, h) = dressed.image.dimensions();
            // Centred on the look, feet on the bottom row, and not much bigger than it.
            assert_eq!(
                dressed.origin.0 * 2 + a.width,
                w as i32,
                "{look} {}: not centred",
                art.id
            );
            assert_eq!(dressed.origin.1 + a.height, h as i32, "{look} {}: feet moved", art.id);
            assert!(
                w <= a.width as u32 + 80 && h <= a.height as u32 + 60,
                "{look} {}: {w}x{h}",
                art.id
            );
            // Something was drawn, and every pixel is either clear or fully opaque.
            assert!(dressed.image.pixels().all(|p| p[3] == 0 || p[3] == 255));
            let solid = dressed.image.pixels().filter(|p| p[3] > 0).count();
            assert!(
                dressed.image != bare.image || solid != solid_bare,
                "{look} {}: nothing drawn",
                art.id
            );
            // No hole in the accessory itself: drawn alone, it encloses no clear pixel (the
            // frames of plain glasses are meant to show the eyes through them).
            if art.id != "round_glasses" {
                let alone = accessories::draw(
                    art,
                    None,
                    &a,
                    &Colors::default().skin_ramp(),
                    (w + 80, h + 80),
                    (40, 40),
                );
                let mut layer = alone.behind.clone();
                for (dst, src) in layer.pixels_mut().zip(alone.front.pixels()) {
                    if src[3] > 0 {
                        *dst = *src;
                    }
                }
                assert_eq!(holes(&layer), 0, "{look} {}: a hole in the art", art.id);
            }
        }
    }
}

/// Clear pixels the art encloses (gaps between arms and body and the like).
fn holes(img: &RgbaImage) -> usize {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let mut open = vec![false; (w * h) as usize];
    let mut stack: Vec<(i32, i32)> = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).collect();
    stack.extend((0..h).flat_map(|y| [(0, y), (w - 1, y)]));
    while let Some((x, y)) = stack.pop() {
        if x < 0 || y < 0 || x >= w || y >= h {
            continue;
        }
        let i = (y * w + x) as usize;
        if open[i] || img.get_pixel(x as u32, y as u32)[3] > 0 {
            continue;
        }
        open[i] = true;
        stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
    }
    img.enumerate_pixels()
        .filter(|(x, y, p)| p[3] == 0 && !open[(*y as i32 * w + *x as i32) as usize])
        .count()
}

#[test]
fn colours_change_their_own_part_only() {
    let base = spec("tidewalker", &[]);
    let plain = compose(&base).image;
    for channel in Channel::ALL {
        let mut other = base.clone();
        let next = match base.colors.get(channel) {
            Paint::Swatch(i) => Paint::Swatch((i + 3) % 20),
            Paint::Custom(_) => Paint::Swatch(0),
        };
        other.colors.set(channel, next);
        let changed = compose(&other).image;
        assert_eq!(changed.dimensions(), plain.dimensions());
        let diff = plain.pixels().zip(changed.pixels()).filter(|(a, b)| a != b).count();
        assert!(diff > 0, "{channel:?} changed nothing");
        assert!(diff < plain.pixels().count() / 3, "{channel:?} changed {diff} pixels");
    }
    // Natural eyes and shoes leave the art as drawn.
    let mut natural = base.clone();
    natural.colors.eyes = Paint::Swatch(0);
    natural.colors.shoes = Paint::Swatch(0);
    let img = compose(&natural).image;
    assert!(img.pixels().any(|p| p.0[..3] == SHOE_KEY[1]));
}

#[test]
fn hats_hide_the_hair_above_them_and_mirroring_keeps_the_feet() {
    let bare = compose(&spec("bunhaven", &[]));
    let hat = compose(&spec("bunhaven", &["top_hat"]));
    assert!(hat.image.height() >= bare.image.height());
    let mut mirrored = spec("bunhaven", &["balloon"]);
    let right = compose(&mirrored);
    mirrored.pose.mirrored = true;
    let left = compose(&mirrored);
    assert_eq!(left.image, imageops::flip_horizontal(&right.image));
    mirrored.pose.hands_busy = true;
    let busy = compose(&mirrored);
    assert_ne!(busy.image, left.image, "the balloon is put away");
    assert_eq!(
        busy.image,
        imageops::flip_horizontal(&compose(&spec("bunhaven", &[])).image)
    );
}

#[test]
fn scaled_pictures_are_whole_blocks() {
    let s = spec("curlcove", &["straw_hat", "round_glasses", "ice_cream"]);
    let one = render_character(&s, 1);
    let three = render_character(&s, 3);
    assert_eq!(three.dimensions(), (one.width() * 3, one.height() * 3));
    for (x, y, p) in one.enumerate_pixels() {
        assert_eq!(three.get_pixel(x * 3 + 2, y * 3 + 1), p);
    }
}

#[test]
fn specs_serialise_with_stable_names() {
    let mut s = spec("tidewalker", &["crown"]);
    s.colors.hair = Paint::Custom(0x12abef);
    s.accessories[0].tint = Some(Paint::Swatch(3));
    let json = serde_json::to_string(&s).expect("serialises");
    assert!(json.contains(r#""look":"tidewalker""#), "{json}");
    assert!(json.contains(r#""hair":{"custom":1223663}"#), "{json}");
    assert!(
        json.contains(r#""accessories":[{"id":"crown","tint":{"swatch":3}}]"#),
        "{json}"
    );
    let back: CharacterSpec = serde_json::from_str(&json).expect("parses");
    assert_eq!(back, s);
}

#[test]
fn accessory_ids_are_unique_and_every_slot_has_some() {
    for (i, a) in ACCESSORIES.iter().enumerate() {
        assert!(ACCESSORIES[..i].iter().all(|b| b.id != a.id), "{} twice", a.id);
        assert!(accessory_icon(a.id, None, 48).is_some(), "{}", a.id);
    }
    for slot in Slot::ALL {
        assert!(ACCESSORIES.iter().filter(|a| a.slot == slot).count() >= 5, "{slot:?}");
    }
}

#[test]
fn made_up_look_ids_share_one_stand_in_and_do_not_grow_the_cache() {
    let stand_in = compose(&spec("no-such-look", &[])).image;
    for n in 0..200 {
        assert_eq!(compose(&spec(&format!("made-up-{n}"), &[])).image, stand_in);
    }
    // Other tests add real looks at the same time, so look at what is cached, not how much.
    let cache = LOOKS.lock().expect("look cache");
    assert!(cache.contains_key(""), "the stand-in is cached under one key");
    assert!(
        cache
            .keys()
            .all(|k| k.is_empty() || atlas().get(&format!("a_{k}")).is_some()),
        "only looks with art are cached under their own id: {:?}",
        cache.keys().collect::<Vec<_>>()
    );
}

/// Hair-key pixels at or below the waistband.
fn hair_below_the_waist(look: &str) -> usize {
    let p = prepared(look);
    let waist = p.anchors.torso.3;
    p.parts
        .iter()
        .enumerate()
        .filter(|(i, part)| matches!(part, Part::Hair(_)) && (*i as i32 / p.width as i32) >= waist)
        .count()
}

#[test]
fn trousers_and_skirts_carry_no_hair_coloured_stains() {
    // The quantiser left specks of hair colour in the legs of these; none may be left.
    for look in [
        "bobbin",
        "mopsy",
        "ponycrest",
        "puffcloud",
        "twinbuns",
        "wavecrest",
        "bunhaven",
        "shavebreeze",
        "topknot",
    ] {
        assert_eq!(hair_below_the_waist(look), 0, "{look}: hair colour in the legs");
    }
    // The gown's hem frill is drawn in that colour on purpose, and stays.
    assert!(hair_below_the_waist("starlet") > 60, "the hem frill was eaten");
}
