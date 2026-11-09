use super::*;

use image::{AnimationDecoder, codecs::gif::GifDecoder};

use crate::character::{Colors, Paint, Worn};

fn person(look: &str, n: u8, accessories: &[&str]) -> CharacterSpec {
    CharacterSpec {
        look: look.to_owned(),
        colors: Colors {
            skin: Paint::Swatch(n % 20),
            hair: Paint::Swatch(n * 5 % 20),
            eyes: Paint::Swatch(n * 3 % 20),
            top: Paint::Swatch(n * 7 % 20),
            bottom: Paint::Swatch(n * 11 % 20),
            shoes: Paint::Swatch(n * 13 % 20),
        },
        accessories: accessories
            .iter()
            .map(|id| Worn {
                id: (*id).to_owned(),
                tint: None,
            })
            .collect(),
        pose: Default::default(),
    }
}

#[test]
fn every_kind_makes_a_short_looping_gif_that_decodes() {
    let sa = person("tidewalker", 1, &["straw_hat", "balloon"]);
    let sb = person("longreef", 5, &["bow", "round_glasses", "ice_cream"]);
    let (a, b) = (Person { spec: &sa }, Person { spec: &sb });
    let (fa, fb) = (Figure::new(a, 1), Figure::new(b, -1));
    for kind in Interaction::ALL {
        let started = std::time::Instant::now();
        let gif = interaction_gif(kind, a, b).expect("encodes");
        let took = started.elapsed();
        println!("{kind:?}: {} KB in {took:?}", gif.len() / 1024);
        if !cfg!(debug_assertions) {
            assert!(took < std::time::Duration::from_millis(200), "{kind:?} took {took:?}");
        }
        assert!(gif.len() < 1_000_000, "{kind:?} is {} bytes", gif.len());
        let decoder = GifDecoder::new(std::io::Cursor::new(&gif)).expect("a GIF");
        let frames = decoder.into_frames().collect_frames().expect("frames");
        assert_eq!(frames.len(), FRAMES, "{kind:?}");
        assert_eq!(frames[0].buffer().dimensions(), (W as u32 * SCALE, H as u32 * SCALE));
        for i in 0..FRAMES {
            let colours: std::collections::HashSet<_> = scene(kind, &fa, &fb, i as f32 / FRAMES as f32)
                .pixels()
                .map(|p| p.0)
                .collect();
            assert!(colours.len() <= 256, "{kind:?} frame {i} has {} colours", colours.len());
        }
        assert_ne!(frames[0].buffer(), frames[FRAMES / 2].buffer(), "{kind:?} moves");
    }
}

#[test]
fn figures_face_each_other_with_their_shoulders_on_the_shirt() {
    for look in ["tidewalker", "longreef", "capewind", "curlcove", "puffcloud", "bobbin"] {
        let spec = person(look, 2, &["cape", "witch_hat"]);
        for dir in [1, -1] {
            let f = Figure::new(Person { spec: &spec }, dir);
            let (x0, y0, x1, y1) = f.top;
            assert!(x0 < x1 && y0 < y1, "{look}: {:?}", f.top);
            assert!(x1 < f.sprite.width() as i32 && y1 < f.sprite.height() as i32, "{look}");
            let px = f.sprite.get_pixel(((x0 + x1) / 2) as u32, (y0 + 3) as u32);
            assert_eq!(px[3], 255, "{look} dir {dir}: the shirt is where the box says");
        }
    }
}

#[test]
fn every_look_poses_every_scene_within_a_gifs_palette() {
    let store = crate::character::atlas();
    let mut looks: Vec<String> = ["tidewalker", "longreef", "capewind", "curlcove", "puffcloud", "bobbin"]
        .map(String::from)
        .into();
    looks.extend((0..2).map(|n| format!("missing{n}")));
    let gear: [&[&str]; 4] = [
        &[],
        &["top_hat", "sunglasses", "scarf", "fairy_wings", "lantern"],
        &["crown", "bat_wings", "fishing_rod"],
        &["beanie", "backpack", "star_wand", "heart_glasses"],
    ];
    for (n, look) in looks.iter().enumerate() {
        let spec = person(look, n as u8, gear[n % gear.len()]);
        // A look with no art yet is drawn as the stand-in figure.
        assert!(store.get(&format!("a_{look}")).is_some() || look.starts_with("missing"));
        let (a, b) = (
            Figure::new(Person { spec: &spec }, 1),
            Figure::new(Person { spec: &spec }, -1),
        );
        for kind in Interaction::ALL {
            for at in [0.0, 0.35, 0.6] {
                let frame = scene(kind, &a, &b, at);
                let used: std::collections::HashSet<_> = frame.pixels().map(|p| p.0).collect();
                assert!(used.len() <= 256, "{look} {kind:?} at {at}: {} colours", used.len());
                assert!(
                    frame.pixels().all(|p| p[3] == 255),
                    "{look} {kind:?} at {at} has clear pixels"
                );
            }
        }
    }
}

#[test]
fn a_high_five_waits_with_no_hand_held_out() {
    let (sa, sb) = (person("tidewalker", 1, &["straw_hat"]), person("longreef", 5, &["bow"]));
    let (a, b) = (
        Figure::new(Person { spec: &sa }, 1),
        Figure::new(Person { spec: &sb }, -1),
    );
    // At rest both are just standing: the arms are the sprites' own, and a second one drawn at the
    // chest used to float there beside each of them.
    let rest = |x: f32| Pose {
        x,
        lift: 0.0,
        sx: 1.0,
        sy: 1.0,
        lean: 0.0,
    };
    let mut standing = backdrop(Interaction::HighFive);
    twinkles(&mut standing, 0.0);
    stand(
        &mut standing,
        Interaction::HighFive,
        (&a, rest(56.0)),
        (&b, rest(184.0)),
    );
    assert_eq!(scene(Interaction::HighFive, &a, &b, 0.0), standing);
}

#[test]
fn the_patter_keeps_its_feet_on_its_shadow() {
    let (sa, sb) = (person("tidewalker", 1, &["straw_hat"]), person("longreef", 5, &["bow"]));
    let (a, b) = (
        Figure::new(Person { spec: &sa }, 1),
        Figure::new(Person { spec: &sb }, -1),
    );
    let (sky, shadow) = (backdrop(Interaction::Pat), hex(Interaction::Pat.palette().3));
    // While it pats the patter stands at x 112 (see `pat`); the lowest row of the figure there is
    // followed straight away by the shadow.
    for at in [0.45, 0.55, 0.65] {
        let frame = scene(Interaction::Pat, &a, &b, at);
        let figure_at = |x: i32, y: i32| {
            let px = frame.get_pixel(x as u32, y as u32);
            px != sky.get_pixel(x as u32, y as u32) && *px != shadow
        };
        let feet = (0..H)
            .rev()
            .find(|&y| (100..124).any(|x| figure_at(x, y)))
            .expect("the patter");
        assert!(
            (100..124).any(|x| *frame.get_pixel(x as u32, feet as u32 + 1) == shadow),
            "at {at}: its feet end at row {feet}, the shadow is not right under them"
        );
    }
}

#[test]
fn keys_round_trip() {
    for kind in Interaction::ALL {
        assert_eq!(Interaction::from_key(kind.key()), Some(kind));
    }
    assert_eq!(Interaction::from_key("kiss"), None);
}

/// Writes a contact sheet of every kind's frames for eyeballing:
/// `cargo test -p pinchora-render social_preview -- --ignored --nocapture`, then open
/// `target/social-preview/*.png`.
#[test]
#[ignore]
fn social_preview() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/social-preview");
    std::fs::create_dir_all(&dir).unwrap();
    let pairs: [(CharacterSpec, CharacterSpec); 3] = [
        (
            person("tidewalker", 1, &["straw_hat", "balloon", "scarf"]),
            person("longreef", 5, &["bow", "round_glasses", "ice_cream"]),
        ),
        (
            person("curlcove", 2, &["beanie", "backpack", "flower"]),
            person("puffcloud", 6, &["crown", "cape", "star_wand"]),
        ),
        (
            person("capewind", 3, &["captain_hat", "eyepatch", "fishing_rod"]),
            person("bobbin", 0, &["witch_hat", "bat_wings", "lantern"]),
        ),
    ];
    for (n, (sa, sb)) in pairs.iter().enumerate() {
        let (a, b) = (
            Figure::new(Person { spec: sa }, 1),
            Figure::new(Person { spec: sb }, -1),
        );
        for kind in Interaction::ALL {
            let cols = 4u32;
            let rows = (FRAMES as u32).div_ceil(cols);
            let (fw, fh) = (W as u32, H as u32);
            let mut sheet = RgbaImage::new(fw * cols, fh * rows);
            for i in 0..FRAMES {
                let frame = scene(kind, &a, &b, i as f32 / FRAMES as f32);
                imageops::overlay(
                    &mut sheet,
                    &frame,
                    (i as u32 % cols * fw) as i64,
                    (i as u32 / cols * fh) as i64,
                );
            }
            sheet.save(dir.join(format!("{n}-{}.png", kind.key()))).unwrap();
            std::fs::write(
                dir.join(format!("{n}-{}.gif", kind.key())),
                interaction_gif(kind, Person { spec: sa }, Person { spec: sb }).unwrap(),
            )
            .unwrap();
        }
    }
}
