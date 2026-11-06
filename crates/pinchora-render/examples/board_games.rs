//! Renders the board and puzzle cards with sample data:
//! `cargo run -p pinchora-render --example board_games -- <out-dir>`.

use image::RgbaImage;
use pinchora_render::{
    CheckersCard, ChessCard, FleetCard, PicrossCard, RadarCard, Renderer, Reveal, SeaView, ShipView, SpriteArt, Theme,
    Tide2048Card, encode_png, sprites::SpriteStore,
};

fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/render-samples".into());
    std::fs::create_dir_all(&out)?;
    let r = Renderer::new();

    // Chess: the start position after e4 e5 Nf3, a knight picked up.
    let back = [4u8, 2, 3, 5, 6, 3, 2, 4];
    let mut pieces = [0u8; 64];
    for file in 0..8 {
        pieces[file] = back[file];
        pieces[8 + file] = 1;
        pieces[48 + file] = 7;
        pieces[56 + file] = back[file] + 6;
    }
    pieces[12] = 0;
    pieces[28] = 1;
    pieces[52] = 0;
    pieces[36] = 7;
    pieces[6] = 0;
    pieces[21] = 2;
    let card = ChessCard {
        lang: pinchora_core::i18n::Lang::En,
        pieces: &pieces,
        names: ["Captain Pinch", "Marlowe"],
        turn: Some(1),
        winner: None,
        last: Some((6, 21)),
        selected: Some(57),
        targets: &[40, 42, 51],
        check: None,
        status: "Marlowe to move",
        pot: 200,
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/chess.png"), r.chess_card(&card)?)?;
    let mut check = pieces;
    check[60] = 0;
    check[53] = 12;
    let card = ChessCard {
        pieces: &check,
        turn: None,
        winner: Some(0),
        check: Some(53),
        selected: None,
        targets: &[],
        status: "Captain Pinch wins by checkmate",
        ..card
    };
    std::fs::write(format!("{out}/chess-over.png"), r.chess_card(&card)?)?;
    let long = ChessCard {
        names: ["Captain Pinchora of the Reef", "Marlowe the Mariner of Seas"],
        status: "Captain Pinchora of the Reef wins by resignation (Marlowe the Mariner of Seas gave up)",
        ..card
    };
    std::fs::write(format!("{out}/chess-long-status.png"), r.chess_card(&long)?)?;

    // Checkers: a middle game with a king.
    let mut cells = [0u8; 32];
    for (sq, cell) in cells.iter_mut().enumerate() {
        *cell = match sq {
            0..=7 => 2,
            12 | 13 => 2,
            16 | 22 => 1,
            24..=31 => 1,
            _ => 0,
        };
    }
    cells[9] = 4;
    cells[18] = 3;
    let card = CheckersCard {
        lang: pinchora_core::i18n::Lang::En,
        cells: &cells,
        names: ["Captain Pinch", "Marlowe"],
        turn: Some(0),
        winner: None,
        last: &[13, 9],
        selected: Some(18),
        targets: &[13, 14],
        status: "Captain Pinch to move",
        pot: 0,
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/checkers.png"), r.checkers_card(&card)?)?;

    // Sea Fleet.
    let ships = |sunk: [bool; 5]| -> Vec<ShipView> {
        [
            (0, true, 0),
            (50, true, 1),
            (9, false, 2),
            (96, true, 3),
            (45, false, 4),
        ]
        .iter()
        .enumerate()
        .map(|(i, &(cell, horizontal, ship))| ShipView {
            cell,
            horizontal,
            ship,
            sunk: sunk[i],
        })
        .collect()
    };
    let mut shots = vec!['.'; 100];
    for (cell, c) in [
        (0, 'h'),
        (1, 'h'),
        (11, 'm'),
        (23, 'm'),
        (50, 'h'),
        (51, 'h'),
        (52, 'h'),
        (53, 'h'),
        (77, 'm'),
        (99, 'm'),
    ] {
        shots[cell] = c;
    }
    let shots: String = shots.into_iter().collect();
    let none: Vec<ShipView> = Vec::new();
    let sunk_only: Vec<ShipView> = ships([false, true, false, false, false])
        .into_iter()
        .filter(|s| s.sunk)
        .collect();
    let card = RadarCard {
        lang: pinchora_core::i18n::Lang::En,
        seas: [
            SeaView {
                name: "Captain Pinch",
                shots: &shots,
                ships: sunk_only,
                afloat: 4,
            },
            SeaView {
                name: "Marlowe",
                shots: &shots,
                ships: none,
                afloat: 5,
            },
        ],
        turn: Some(0),
        winner: None,
        last: Some((0, 53)),
        status: "Captain Pinch to fire",
        pot: 400,
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/radar.png"), r.radar_card(&card)?)?;
    let card = FleetCard {
        lang: pinchora_core::i18n::Lang::En,
        title: "Your fleet",
        subtitle: "Marlowe's shots so far",
        sea: SeaView {
            name: "You",
            shots: &shots,
            ships: ships([false, true, false, false, false]),
            afloat: 4,
        },
        sunk: [false, true, false, false, false],
        footer: "Back to the channel to fire",
        last: Some(53),
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/fleet.png"), r.fleet_card(&card)?)?;

    // Pixel picross: a fish, half done, then revealed. `a` is the accent colour, `w` white, `e` an eye.
    let picture = [
        "..........",
        "....###...",
        "a..#####..",
        "aaa#######",
        "aaa####e##",
        "aaa#wwwww#",
        "a..#####..",
        "....###...",
        "..........",
        "..........",
    ];
    let solution: [bool; 100] = std::array::from_fn(|i| picture[i / 10].as_bytes()[i % 10] != b'.');
    let runs = |line: Vec<bool>| -> Vec<u8> {
        let mut out = Vec::new();
        let mut n = 0u8;
        for v in line {
            if v {
                n += 1
            } else if n > 0 {
                out.push(n);
                n = 0
            }
        }
        if n > 0 {
            out.push(n)
        }
        if out.is_empty() {
            out.push(0)
        }
        out
    };
    let rows: Vec<Vec<u8>> = (0..10)
        .map(|y| runs((0..10).map(|x| solution[y * 10 + x]).collect()))
        .collect();
    let cols: Vec<Vec<u8>> = (0..10)
        .map(|x| runs((0..10).map(|y| solution[y * 10 + x]).collect()))
        .collect();
    let mut grid = [false; 100];
    grid[..50].copy_from_slice(&solution[..50]);
    grid[63] = true;
    let card = PicrossCard {
        title: "Daily picross #12",
        subtitle: "Day 3 streak",
        grid: &grid,
        rows: &rows,
        cols: &cols,
        row_done: std::array::from_fn(|y| y < 5),
        col_done: [false; 10],
        footer: "Pick a row, then its filled cells",
        reveal: None,
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/picross.png"), r.picross_card(&card)?)?;
    let cells: [u8; 100] = std::array::from_fn(|i| {
        let letter = picture[i / 10].as_bytes()[i % 10];
        if letter == b'.' {
            return 0;
        }
        if letter == b'e' {
            return 10;
        }
        let tone = if i < 10 || !solution[i - 10] {
            1
        } else if i + 10 >= 100 || !solution[i + 10] {
            3
        } else {
            2
        };
        tone + match letter {
            b'a' => 3,
            b'w' => 6,
            _ => 0,
        }
    });
    let card = PicrossCard {
        title: "Daily picross #12",
        subtitle: "Solved!",
        grid: &solution,
        row_done: [true; 10],
        col_done: [true; 10],
        footer: "+55 shells, a 3 day streak",
        reveal: Some(Reveal {
            name: "Fish",
            color: "#ff9e7a",
            accent: "#ffd166",
            cells: &cells,
        }),
        ..card
    };
    std::fs::write(format!("{out}/picross-solved.png"), r.picross_card(&card)?)?;

    // Tide 2048 with fish from the atlas.
    let store = SpriteStore::embedded();
    let ladder = [
        "pebblepin_shrimp",
        "mintcap_limpet",
        "sandloop_cockle",
        "toaststripe_minnow",
        "mossbutton_perch",
        "plumspot_puffer",
        "reefruff_lionfish",
        "mossmosaic_lobster",
        "chartcurl_nautilus",
        "petalwake_manta",
        "aurorawoven_leviathan",
        "dawncrown_coelacanth",
        "moonkeel_chest",
    ];
    let scaled: Vec<(Vec<u8>, u32, u32)> = ladder
        .iter()
        .map(|id| {
            let sprite = store.get(&format!("c_{id}")).expect("a fish sprite");
            let big: RgbaImage = image::imageops::resize(
                &*sprite,
                sprite.width() * 3,
                sprite.height() * 3,
                image::imageops::FilterType::Nearest,
            );
            (encode_png(&big).expect("png"), big.width(), big.height())
        })
        .collect();
    let art: Vec<SpriteArt<'_>> = scaled
        .iter()
        .map(|(png, w, h)| SpriteArt { png, w: *w, h: *h })
        .collect();
    let cells: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 0, 1, 0];
    let card = Tide2048Card {
        cells: &cells,
        score: 20_480,
        best: 131_072,
        creatures: &art,
        fresh: Some(15),
        status: "Biggest tile: 8192, the chest!",
        lang: pinchora_core::i18n::Lang::En,
        theme: Theme::reef(),
    };
    std::fs::write(format!("{out}/tide2048.png"), r.tide2048_card(&card)?)?;
    println!("wrote board samples to {out}");
    Ok(())
}
