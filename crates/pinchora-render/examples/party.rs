//! Renders the party game cards and a sample derby GIF: `cargo run -p pinchora-render --example party -- <out-dir>`.

use image::{RgbaImage, imageops};
use pinchora_render::{
    CrewView, DerbyCard, DerbyLane, DerbyScene, HeistCard, Mood, PickView, Renderer, ShareGrabCard, ShellCard,
    SpriteArt, StageView, SurgeCard, SurgeState, Theme, derby_gif, sprites::SpriteStore, tint_crab,
};

const SHELLS: [[u32; 3]; 6] = [
    [0x9a2f33, 0xe04a3f, 0xff8a6a],
    [0x1f5f8b, 0x3a9bdc, 0x8fd3ff],
    [0x0b5e4c, 0x06b58a, 0x6ff0c8],
    [0x8a5a10, 0xe3a21e, 0xffd166],
    [0x4b2f6b, 0x7d55a3, 0xc2a1ea],
    [0xa84b13, 0xf08a24, 0xffc27a],
];
const ACCENTS: [u32; 6] = [0xff6b4a, 0x3a9bdc, 0x06d6a0, 0xffd166, 0xb48ae0, 0xf08a24];
const NAMES: [&str; 6] = ["Pebble", "Scuttle", "Kelp", "Barnacle", "Clawdia", "Tidbit"];

fn png(image: &RgbaImage) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).expect("png");
    out.into_inner()
}

fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/render-samples".into());
    std::fs::create_dir_all(&out)?;
    let r = Renderer::new();
    let store = SpriteStore::embedded();
    let base = store.get("p_pebblepouch_hermit").expect("the hermit crab sprite");
    let crabs: Vec<RgbaImage> = SHELLS.iter().map(|shell| tint_crab(&base, *shell)).collect();

    // The lobby.
    let big: Vec<Vec<u8>> = crabs
        .iter()
        .map(|c| {
            png(&imageops::resize(
                c,
                c.width() * 2,
                c.height() * 2,
                imageops::FilterType::Nearest,
            ))
        })
        .collect();
    let backers: Vec<Vec<String>> = vec![
        vec!["Captain Pinch".into(), "Marlowe".into(), "Sam".into()],
        vec![],
        vec!["A very long display name indeed".into()],
        vec![],
        vec!["Ann".into(), "Bo".into()],
        vec![],
    ];
    let lanes: Vec<DerbyLane<'_>> = (0..6)
        .map(|n| DerbyLane {
            name: NAMES[n],
            accent: ACCENTS[n],
            art: SpriteArt {
                png: &big[n],
                w: base.width() * 2,
                h: base.height() * 2,
            },
            backers: &backers[n],
        })
        .collect();
    let card = DerbyCard {
        lanes: &lanes,
        stake: 50,
        racers: 6,
        status: "Pick your crab, the race starts soon",
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/derby-lobby.png"), r.derby_card(&card)?)?;
    std::fs::write(
        format!("{out}/derby-lobby-free.png"),
        r.derby_card(&DerbyCard {
            stake: 0,
            racers: 0,
            ..card
        })?,
    )?;

    // The race: made-up steps, six crabs of different pace.
    let frames: Vec<[f32; 6]> = (0..=66)
        .map(|step| {
            let t = step as f32 / 56.0;
            [1.0, 1.08, 0.96, 1.04, 0.9, 1.02].map(|pace| (t * pace).min(1.12))
        })
        .collect();
    let scene = DerbyScene {
        crabs: &crabs,
        accents: ACCENTS,
        names: NAMES,
        frames: &frames,
        order: [1, 5, 3, 0, 2, 4],
    };
    let gif = derby_gif(&scene)?;
    println!("derby gif: {} KB", gif.len() / 1024);
    std::fs::write(format!("{out}/derby.gif"), gif)?;

    // Share or grab.
    for (n, (picks, banner, mood)) in [
        (
            [PickView::Locked, PickView::Thinking],
            "Waiting for both of you",
            Mood::Wait,
        ),
        (
            [PickView::Share, PickView::Share],
            "Both shared, 30 shells each",
            Mood::Good,
        ),
        (
            [PickView::Grab, PickView::Share],
            "Captain Pinch grabbed it all",
            Mood::Bad,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let card = ShareGrabCard {
            names: ["Captain Pinch", "Marlowe"],
            picks,
            pool: 60,
            wager: n == 2,
            banner,
            mood,
            theme: Theme::reef(),
        };
        std::fs::write(format!("{out}/share-{n}.png"), r.share_grab_card(&card)?)?;
    }

    // Shell game.
    for (n, (picked, pearl, status)) in [
        (None, None, "Pick a shell"),
        (Some(0), Some(2), "Not under that one"),
        (Some(2), Some(2), "You found it, +50"),
    ]
    .into_iter()
    .enumerate()
    {
        let card = ShellCard {
            stake: 20,
            picked,
            pearl,
            status,
            payout: 50,
            theme: Theme::reef(),
        };
        std::fs::write(format!("{out}/shells-{n}.png"), r.shell_card(&card)?)?;
    }

    // Heist.
    let crew = [
        CrewView {
            name: "Captain Pinch",
            role: 0,
        },
        CrewView {
            name: "Marlowe",
            role: 1,
        },
        CrewView { name: "Sam", role: 2 },
        CrewView { name: "Ann", role: 2 },
        CrewView { name: "Bo", role: 0 },
    ];
    let titles = ["Slip past the guards", "Crack the clam lock", "Haul out the loot"];
    let lines = [
        "The guard crabs argue about lunch and never look up.",
        "The lock snaps shut on a claw and rings the alarm.",
        "Bag after bag floats out on a trail of bubbles.",
    ];
    for (n, (passed, headline)) in [
        ([None, None, None], "CREW 5/8"),
        ([Some(true), Some(false), Some(true)], "A GOOD HAUL"),
        ([Some(true), Some(true), Some(true)], "THE VAULT IS OPEN"),
    ]
    .into_iter()
    .enumerate()
    {
        let stage = |i: usize| StageView {
            title: titles[i],
            chance: [70, 82, 75][i],
            passed: passed[i],
            line: if passed[i].is_some() { lines[i] } else { "" },
        };
        let card = HeistCard {
            crew: &crew,
            stages: [stage(0), stage(1), stage(2)],
            headline,
            each: if n > 0 { 25 * n as i64 } else { 0 },
            theme: Theme::reef(),
        };
        std::fs::write(format!("{out}/heist-{n}.png"), r.heist_card(&card)?)?;
    }

    // Storm surge.
    let top = vec![
        ("Captain Pinch".to_owned(), 14),
        ("Marlowe".to_owned(), 9),
        ("Sam".to_owned(), 4),
    ];
    for (n, (wall, state)) in [
        (6, SurgeState::Rolling),
        (31, SurgeState::Rolling),
        (56, SurgeState::Held),
        (12, SurgeState::Broken),
    ]
    .into_iter()
    .enumerate()
    {
        let card = SurgeCard {
            wall,
            goal: 56,
            helpers: 4,
            top: &top,
            state,
            theme: Theme::reef(),
        };
        std::fs::write(format!("{out}/surge-{n}.png"), r.surge_card(&card)?)?;
    }
    Ok(())
}
