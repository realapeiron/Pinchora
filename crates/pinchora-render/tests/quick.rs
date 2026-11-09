//! The quick canvas games share one frame, but every board/prompt variant has a golden.
use pinchora_core::infra::i18n::Lang;
use pinchora_render::{QuickCard, QuickDetails, QuickHeat, QuickKind, Renderer, Theme};
use std::path::Path;

#[test]
fn quick_cards_match_goldens() {
    let renderer = Renderer::new();
    for lang in Lang::ALL {
        let tr = lang == Lang::Tr;
        let games = [
            (
                "memory",
                if tr { "Kabuk Eşleştirme" } else { "Shell Match" },
                (0..16)
                    .map(|i| match i {
                        0 | 5 => "1".into(),
                        2 => "3".into(),
                        7 => "4".into(),
                        8 => "5".into(),
                        11 => "6".into(),
                        14 => "7".into(),
                        15 => "8".into(),
                        _ => "?".into(),
                    })
                    .collect::<Vec<String>>(),
                4,
                String::new(),
                false,
            ),
            (
                "minesweeper",
                if tr { "Deniz Mayınları" } else { "Sea Mines" },
                (0..64)
                    .map(|i| match i {
                        0 | 1 | 8 => "·".into(),
                        2 | 9 | 16 => "1".into(),
                        10 | 17 => "2".into(),
                        18 => "F".into(),
                        24 => "3".into(),
                        _ => "?".into(),
                    })
                    .collect(),
                8,
                String::new(),
                false,
            ),
            (
                "higherlower",
                if tr { "Gelgit Tablosu" } else { "Tide Table" },
                vec![],
                0,
                "8".into(),
                false,
            ),
            (
                "anagram",
                if tr { "Karışık Kabuklar" } else { "Scrambled Shells" },
                vec![],
                0,
                if tr { "BUKAK" } else { "LAPER" }.into(),
                false,
            ),
            (
                "typing",
                if tr { "Gelgiti Yaz" } else { "Type the Tide" },
                vec![],
                0,
                if tr {
                    "Küçük yengeç resifin yanında bir inci buldu."
                } else {
                    "The little crab found a pearl near the reef."
                }
                .into(),
                true,
            ),
            (
                "guessnumber",
                if tr { "Sıcak Soğuk" } else { "Hot and Cold" },
                vec![],
                0,
                "42".into(),
                false,
            ),
            (
                "othello",
                if tr { "İnci Çevirme" } else { "Pearl Flip" },
                (0..64)
                    .map(|i| {
                        match i {
                            27 | 36 => "X",
                            28 | 35 => "O",
                            19 | 26 | 37 | 44 => "*",
                            _ => ".",
                        }
                        .into()
                    })
                    .collect(),
                8,
                String::new(),
                false,
            ),
            (
                "nhie",
                if tr { "Ben Hiç" } else { "Never Have I Ever" },
                vec![],
                0,
                if tr {
                    "Ben hiç kumdan kale yapmadım."
                } else {
                    "I have never built a sandcastle."
                }
                .into(),
                false,
            ),
            (
                "fun",
                if tr { "Sihirli Deniz Kabuğu" } else { "Magic Conch" },
                vec![],
                0,
                if tr {
                    "Merakına güven."
                } else {
                    "Trust your curiosity."
                }
                .into(),
                false,
            ),
        ];
        for (name, title, cells, columns, prompt, distort) in games {
            let kind = match name {
                "memory" => QuickKind::Memory,
                "minesweeper" => QuickKind::Minesweeper,
                "higherlower" => QuickKind::HigherLower,
                "anagram" => QuickKind::Anagram,
                "typing" => QuickKind::Typing,
                "guessnumber" => QuickKind::GuessNumber,
                "othello" => QuickKind::Othello,
                "nhie" => QuickKind::Nhie,
                _ => QuickKind::Conch,
            };
            let card = QuickCard {
                kind,
                details: QuickDetails {
                    names: ["Captain Pinch", "Marlowe"],
                    turn: Some(0),
                    last: match kind {
                        QuickKind::Memory => Some(7),
                        QuickKind::Minesweeper => Some(17),
                        _ => None,
                    },
                    matched: &[0, 5],
                    heat: Some(QuickHeat::Warm),
                    progress: 3,
                    ..Default::default()
                },
                lang,
                title,
                subtitle: if tr { "RESİFTE BİR OYUN" } else { "A GAME BY THE REEF" },
                cells: &cells,
                columns,
                prompt: &prompt,
                status: if tr { "SIRA SENDE" } else { "YOUR TURN" },
                theme: Theme::reef(),
                distort,
            };
            golden(&renderer, name, &card);
        }
    }
}

fn golden(renderer: &Renderer, name: &str, card: &QuickCard<'_>) {
    let png = renderer.quick_card(card).unwrap();
    let file = format!("quick_{name}_{}.png", card.lang.code());
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(file);
    if std::env::var_os("PINCHORA_UPDATE_GOLDENS").is_some() {
        std::fs::write(&path, &png).unwrap();
    }
    let golden = image::open(&path).expect("quick golden missing").to_rgba8();
    let actual = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(golden.dimensions(), actual.dimensions());
    let changed = golden
        .pixels()
        .zip(actual.pixels())
        .filter(|(a, b)| a.0.iter().zip(b.0).any(|(x, y)| x.abs_diff(y) > 6))
        .count();
    assert!(changed < 100, "{name} {} changed {changed} pixels", card.lang.code());
}

#[test]
fn quick_states_match_goldens() {
    let renderer = Renderer::new();
    for lang in Lang::ALL {
        let tr = lang == Lang::Tr;
        let base = QuickCard {
            kind: QuickKind::Minesweeper,
            details: QuickDetails {
                finished: true,
                last: Some(27),
                ..Default::default()
            },
            lang,
            title: if tr { "Deniz Mayınları" } else { "Sea Mines" },
            subtitle: if tr { "RESİFTE BİR OYUN" } else { "A GAME BY THE REEF" },
            cells: &[],
            columns: 8,
            prompt: "",
            status: if tr {
                "OYUN BİTTİ · MAYINLAR AÇILDI"
            } else {
                "GAME OVER · MINES REVEALED"
            },
            theme: Theme::reef(),
            distort: false,
        };
        let mines: Vec<String> = (0..64)
            .map(|i| match i {
                27 | 30 | 42 | 50 => "*".into(),
                19 => "F".into(),
                0..=7 => (i + 1).to_string(),
                8..=18 => "·".into(),
                _ => "?".into(),
            })
            .collect();
        golden(
            &renderer,
            "minesweeper_over",
            &QuickCard {
                cells: &mines,
                ..base.clone()
            },
        );
        let big: Vec<String> = (0..100)
            .map(|i| {
                match i {
                    0..=8 => "·",
                    10..=18 => "1",
                    22 | 35 => "F",
                    _ => "?",
                }
                .into()
            })
            .collect();
        golden(
            &renderer,
            "minesweeper_10",
            &QuickCard {
                cells: &big,
                columns: 10,
                details: QuickDetails {
                    last: Some(35),
                    ..Default::default()
                },
                status: if tr {
                    "2 BAYRAK · BİR KARE SEÇ"
                } else {
                    "2 FLAGS · CHOOSE A SQUARE"
                },
                ..base.clone()
            },
        );
        let faces: Vec<String> = (0..16).map(|i| (i % 8 + 1).to_string()).collect();
        golden(
            &renderer,
            "memory_over",
            &QuickCard {
                kind: QuickKind::Memory,
                title: if tr { "Kabuk Eşleştirme" } else { "Shell Match" },
                cells: &faces,
                columns: 4,
                details: QuickDetails {
                    finished: true,
                    matched: &(0..16).collect::<Vec<_>>(),
                    ..Default::default()
                },
                status: if tr {
                    "8 ÇİFT · HEPSİ EŞLEŞTİ"
                } else {
                    "8 PAIRS · ALL MATCHED"
                },
                ..base.clone()
            },
        );
        let pearls: Vec<String> = (0..64).map(|i| if i % 3 == 0 { "X" } else { "O" }.into()).collect();
        golden(
            &renderer,
            "othello_over",
            &QuickCard {
                kind: QuickKind::Othello,
                title: if tr { "İnci Çevirme" } else { "Pearl Flip" },
                cells: &pearls,
                details: QuickDetails {
                    names: ["Captain Pinch", "Marlowe"],
                    finished: true,
                    winner: Some(0),
                    last: Some(63),
                    ..Default::default()
                },
                status: if tr {
                    "CAPTAIN PINCH KAZANDI · 42–22"
                } else {
                    "CAPTAIN PINCH WINS · 42–22"
                },
                ..base.clone()
            },
        );
        golden(
            &renderer,
            "anagram_letters",
            &QuickCard {
                kind: QuickKind::Anagram,
                title: if tr { "Karışık Kabuklar" } else { "Scrambled Shells" },
                prompt: if tr { "işğçöü" } else { "SHORELINE" },
                status: if tr {
                    "TÜRKÇE HARFLER"
                } else {
                    "REARRANGE THE LETTERS"
                },
                ..base.clone()
            },
        );
        golden(
            &renderer,
            "typing_result",
            &QuickCard {
                kind: QuickKind::Typing,
                title: if tr { "Gelgiti Yaz" } else { "Type the Tide" },
                prompt: if tr {
                    "Ilık bir rüzgar kıyıya deniz kabukları taşıdı."
                } else {
                    "A warm breeze carried shells across the shore."
                },
                details: QuickDetails {
                    finished: true,
                    wpm: Some(72),
                    ..Default::default()
                },
                distort: true,
                status: if tr { "DOĞRU · 72 WPM" } else { "CORRECT · 72 WPM" },
                ..base.clone()
            },
        );
        for (name, heat, prompt) in [
            ("guessnumber_cold", Some(QuickHeat::Cold), "12"),
            ("guessnumber_hot", Some(QuickHeat::Hot), "67"),
            ("guessnumber_start", None, "?"),
        ] {
            golden(
                &renderer,
                name,
                &QuickCard {
                    kind: QuickKind::GuessNumber,
                    title: if tr { "Sıcak Soğuk" } else { "Hot and Cold" },
                    prompt,
                    details: QuickDetails {
                        heat,
                        ..Default::default()
                    },
                    status: if tr {
                        "1–100 ARASINDA TAHMİN ET"
                    } else {
                        "GUESS A NUMBER FROM 1–100"
                    },
                    ..base.clone()
                },
            );
        }
        for (name, kind, prompt) in [
            ("fun_roll", QuickKind::Roll, "5 / 6"),
            ("fun_roll_100", QuickKind::Roll, "87 / 100"),
            (
                "fun_choose",
                QuickKind::Choose,
                if tr {
                    "Kıyıda uzun bir yürüyüş, ardından sıcak çikolata ve deniz fenerinin yanında sohbet"
                } else {
                    "A walk along the shore followed by hot chocolate and stories beside the lighthouse"
                },
            ),
        ] {
            golden(
                &renderer,
                name,
                &QuickCard {
                    kind,
                    title: if tr { "Sihirli Deniz Kabuğu" } else { "Magic Conch" },
                    prompt,
                    status: if tr { "RESİFİN YANITI" } else { "THE REEF'S ANSWER" },
                    ..base.clone()
                },
            );
        }
    }
}
