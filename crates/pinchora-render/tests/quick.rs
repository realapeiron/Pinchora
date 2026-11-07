//! The quick canvas games share one frame, but every board/prompt variant has a golden.
use pinchora_core::i18n::Lang;
use pinchora_render::{QuickCard, Renderer, Theme};
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
                    .map(|i| if i == 0 { "1".into() } else { "?".into() })
                    .collect::<Vec<String>>(),
                4,
                String::new(),
                false,
            ),
            (
                "minesweeper",
                if tr { "Deniz Mayınları" } else { "Sea Mines" },
                (0..64)
                    .map(|i| {
                        if i == 17 {
                            "2".into()
                        } else if i == 18 {
                            "F".into()
                        } else {
                            "?".into()
                        }
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
                if tr { "KABUK" } else { "PEARL" }.into(),
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
                "?".into(),
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
            let card = QuickCard {
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
            let png = renderer.quick_card(&card).unwrap();
            let file = format!("quick_{name}_{}.png", lang.code());
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
            assert!(changed < 100, "{name} {} changed {changed} pixels", lang.code());
        }
    }
}
