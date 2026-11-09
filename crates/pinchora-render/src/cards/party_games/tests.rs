use super::*;

use image::RgbaImage;

use crate::{Renderer, encode_png};

fn size(svg: &str) -> (u32, u32) {
    let png = Renderer::new().svg_to_png(svg).expect("renders");
    let image = image::load_from_memory(&png).expect("a png");
    (image.width(), image.height())
}

fn scene() -> Vec<u8> {
    encode_png(&RgbaImage::from_pixel(960, 540, image::Rgba([120, 180, 220, 255]))).expect("encodes")
}

fn names(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("Player number {i}")).collect()
}

#[test]
fn the_lobby_shows_seats_and_what_is_missing() {
    let players = names(2);
    let card = PartyLobbyCard {
        lang: Lang::En,
        title: "Hot Pearl",
        tagline: "Type a word with the syllable before the pearl pops.",
        players: &players,
        min: 3,
        max: 8,
        footer: "Press start",
        theme: Theme::reef(),
    };
    let svg = card.to_svg();
    assert!(svg.contains("HOT PEARL") && svg.contains("2/8") && svg.contains("Player number 2"));
    assert!(svg.contains("open seat") && svg.contains("WAITING FOR 1 MORE PLAYER"));
    let (w, h) = size(&svg);
    assert_eq!(w, 900);
    assert!(h > 300);
    let ready = PartyLobbyCard { min: 2, ..card };
    assert!(ready.to_svg().contains("PRESS START"));
}

#[test]
fn the_pearl_table_seats_everyone_and_marks_the_holder() {
    for n in [2usize, 3, 5, 8] {
        let seats: Vec<PearlSeat> = (0..n)
            .map(|i| PearlSeat {
                name: "Somebody with a very long name indeed",
                lives: 1 + (i as i32 % 2),
                max_lives: 2,
                out: i == 1 && n > 2,
                current: i == 0,
            })
            .collect();
        let card = PearlCard {
            lang: Lang::En,
            syllable: "ear",
            seats: &seats,
            banner: "Somebody has 25 seconds",
            last_word: Some("pearl"),
            theme: Theme::reef(),
        };
        let svg = card.to_svg();
        assert!(svg.contains(">EAR<") && svg.contains("HOLDS THE PEARL") && svg.contains("LAST WORD PEARL"));
        assert_eq!(svg.matches("HOLDS THE PEARL").count(), 1);
        assert_eq!(svg.contains(">OUT<"), n > 2);
        assert_eq!(size(&svg), (900, 560));
    }
}

#[test]
fn standings_rank_the_winner_first_and_list_what_it_paid() {
    let rows = [
        StandingRow {
            rank: 1,
            name: "Ada",
            score: "7 pts",
            note: "+25 shells",
            winner: true,
        },
        StandingRow {
            rank: 2,
            name: "Bo",
            score: "3 pts",
            note: "",
            winner: false,
        },
    ];
    let card = StandingsCard {
        title: "Acronym Tide",
        subtitle: "Final scores after 3 rounds",
        rows: &rows,
        footer: "Thanks for playing",
        theme: Theme::reef(),
    };
    let svg = card.to_svg();
    assert!(svg.contains("7 PTS") && svg.contains("+25 SHELLS") && svg.contains(">Ada<"));
    assert_eq!(size(&svg).0, 900);
}

#[test]
fn letters_and_entries_draw_what_players_need_to_see() {
    let letters = LettersCard {
        title: "Acronym Tide",
        round: "Round 2/3",
        letters: &['T', 'W', 'R', 'S'],
        hint: "One word per letter, in order.",
        status: "2 of 5 phrases in",
        theme: Theme::reef(),
    };
    let svg = letters.to_svg();
    assert!(svg.contains(">T<") && svg.contains(">S<") && svg.contains("ROUND 2/3"));
    assert_eq!(size(&svg), (900, 380));

    let rows = [
        EntryRow {
            text: "Tiny whales ride sleds",
            author: None,
            votes: None,
            winner: false,
        },
        EntryRow {
            text: "Terrific wizards refuse sandwiches, politely, while hopping on one leg, and singing loudly",
            author: None,
            votes: None,
            winner: false,
        },
    ];
    let anonymous = EntriesCard {
        lang: Lang::En,
        title: "Which is best",
        subtitle: "Pick the best",
        chip: "Round 1/3",
        header: None,
        rows: &rows,
        footer: "Closing soon",
        theme: Theme::reef(),
    };
    let svg = anonymous.to_svg();
    assert!(
        svg.contains("Tiny whales ride sleds") && !svg.contains("VOTE"),
        "no tallies while anonymous"
    );
    let png = scene();
    let revealed_rows = [
        EntryRow {
            text: "Tiny whales ride sleds",
            author: Some("Ada"),
            votes: Some(2),
            winner: true,
        },
        EntryRow {
            text: "Nothing",
            author: Some("Bo"),
            votes: Some(0),
            winner: false,
        },
    ];
    let reveal = EntriesCard {
        lang: Lang::En,
        header: Some(EntryHeader {
            png: &png,
            w: 960,
            h: 540,
        }),
        rows: &revealed_rows,
        title: "Results",
        ..anonymous
    };
    let svg = reveal.to_svg();
    assert!(svg.contains("2 VOTES") && svg.contains("0 VOTES") && svg.contains(">Ada<") && svg.contains("<image"));
    let (w, h) = size(&svg);
    assert_eq!(w, 900);
    assert!(h > 520, "the picture adds height: {h}");
}

#[test]
fn the_race_shows_colours_but_never_letters() {
    let rows = [[1u8, 2, 3, 1, 1], [3, 3, 3, 3, 3]];
    let racers = [
        Racer {
            name: "Ada",
            rows: &rows,
            state: RacerState::Solved(2),
        },
        Racer {
            name: "Bo",
            rows: &rows[..1],
            state: RacerState::Playing,
        },
        Racer {
            name: "Cy",
            rows: &[],
            state: RacerState::Failed,
        },
    ];
    let card = RaceCard {
        lang: Lang::En,
        title: "Wordle Race",
        racers: &racers,
        footer: "First to solve wins",
        theme: Theme::reef(),
    };
    let svg = card.to_svg();
    assert!(svg.contains("SOLVED IN 2") && svg.contains("5 LEFT") && svg.contains("OUT OF TRIES"));
    assert_eq!(size(&svg), (900, 440));
}

#[test]
fn a_postcard_keeps_its_picture_pixel_for_pixel() {
    let png = scene();
    let svg = PostcardCard {
        lang: Lang::En,
        scene: &png,
        place: "Driftwood Dunes",
        stamp: "Round 1/3",
        theme: Theme::reef(),
    }
    .to_svg();
    assert!(svg.contains("GREETINGS FROM") && svg.contains("DRIFTWOOD DUNES") && svg.contains("ROUND 1/3"));
    assert!(svg.contains(r#"width="960" height="540""#));
    assert_eq!(size(&svg), (1040, 700));
}

#[test]
fn would_you_rather_shows_the_split_only_when_asked() {
    let open = RatherCard {
        lang: Lang::En,
        category: "Silly",
        a: "Have hiccups that sound like a duck",
        b: "Sneeze glitter",
        votes: None,
        closed: false,
        theme: Theme::reef(),
    };
    let svg = open.to_svg();
    assert!(svg.contains("PICK A SIDE") && !svg.contains('%'));
    let result = RatherCard {
        lang: Lang::En,
        votes: Some((3, 1)),
        closed: true,
        ..open
    };
    let svg = result.to_svg();
    assert!(svg.contains("75%") && svg.contains("25%") && svg.contains("FINAL: 4 VOTES"));
    assert_eq!(size(&svg), (900, 470));
    assert_eq!(RatherCard::percent(0, 0), 0);
    assert_eq!(RatherCard::percent(1, 3), 33);
}

#[test]
fn prompt_cards_grow_with_their_text() {
    let short = PromptCard {
        lang: Lang::En,
        tone: PromptTone::Turn,
        name: "Ada",
        text: "Truth or dare?",
        category: "",
        footer: "Only Ada can pick",
        theme: Theme::reef(),
    };
    let long = PromptCard {
        lang: Lang::En,
        tone: PromptTone::Dare,
        text: "Describe your perfect cozy night in four sentences, and make every one of them rhyme with the last.",
        category: "Cozy",
        ..PromptCard { name: "Ada", ..short }
    };
    let (short_svg, long_svg) = (
        PromptCard {
            lang: Lang::En,
            tone: PromptTone::Truth,
            name: "Ada",
            text: "Truth or dare?",
            category: "Ocean",
            footer: "x",
            theme: Theme::reef(),
        }
        .to_svg(),
        long.to_svg(),
    );
    assert!(short_svg.contains(">TRUTH<") && long_svg.contains(">DARE<") && long_svg.contains("COZY"));
    assert!(size(&long_svg).1 > size(&short_svg).1);
}
/// The Turkish labels are drawn in capitals with the letters the pixel font has, and fit their places.
#[test]
fn turkish_cards_say_their_labels_and_still_fit() {
    let tr = Lang::Tr;
    let players = names(2);
    let lobby = PartyLobbyCard {
        lang: tr,
        title: "SICAK İNCİ",
        tagline: "Bir hece çıkar. İnci patlamadan onu içeren bir kelime yaz!",
        players: &players,
        min: 3,
        max: 8,
        footer: "HAZIR: KURUCU BAŞLAT DÜĞMESİNE BASABİLİR",
        theme: Theme::reef(),
    }
    .to_svg();
    assert!(lobby.contains("boş yer") && lobby.contains("1 OYUNCU DAHA BEKLENİYOR"));
    assert_eq!(size(&lobby).0, 900);

    let seats = [
        PearlSeat {
            name: "Ada",
            lives: 1,
            max_lives: 2,
            out: false,
            current: true,
        },
        PearlSeat {
            name: "Bo",
            lives: 0,
            max_lives: 2,
            out: true,
            current: false,
        },
    ];
    let pearl = PearlCard {
        lang: tr,
        syllable: "İŞ",
        seats: &seats,
        banner: "ADA ÇAĞLAYAN YAZDI  ·  SIRA BO KULLANICISINDA",
        last_word: Some("ışık"),
        theme: Theme::reef(),
    }
    .to_svg();
    assert!(pearl.contains("İNCİ ONDA") && pearl.contains("ELENDİ") && pearl.contains("SON KELİME IŞIK"));
    assert_eq!(size(&pearl), (900, 560));

    let rows = [[1u8, 2, 3, 1, 1]];
    let racers = [
        Racer {
            name: "Ada",
            rows: &rows,
            state: RacerState::Solved(6),
        },
        Racer {
            name: "Bo",
            rows: &rows,
            state: RacerState::Playing,
        },
        Racer {
            name: "Cy",
            rows: &[],
            state: RacerState::Failed,
        },
        Racer {
            name: "Di",
            rows: &[],
            state: RacerState::Solved(1),
        },
    ];
    let race = RaceCard {
        lang: tr,
        title: "WORDLE YARIŞI",
        racers: &racers,
        footer: "KELİMEYİ İLK BULAN KAZANIR",
        theme: Theme::reef(),
    }
    .to_svg();
    assert!(race.contains("6 DENEMEDE ÇÖZDÜ") && race.contains("5 HAK KALDI") && race.contains("HAKKI BİTTİ"));
    assert_eq!(size(&race), (900, 440));

    let entries = [EntryRow {
        text: "Minik balinalar kızakla kayıyor",
        author: Some("Ada"),
        votes: Some(2),
        winner: true,
    }];
    let reveal = EntriesCard {
        lang: tr,
        title: "KISALTMA GELGİTİ",
        subtitle: "Harfler: T · W · R",
        chip: "TUR 1/3",
        header: None,
        rows: &entries,
        footer: "OY BAŞINA 1 PUAN, KAZANANA 2 PUAN DAHA",
        theme: Theme::reef(),
    }
    .to_svg();
    assert!(reveal.contains("2 OY") && reveal.contains("TUR 1/3"));

    let postcard = PostcardCard {
        lang: tr,
        scene: &scene(),
        place: "ATEŞBÖCEĞİ PİKNİĞİ",
        stamp: "TUR 1/3",
        theme: Theme::reef(),
    }
    .to_svg();
    assert!(postcard.contains("SELAMLAR") && postcard.contains("POSTA"));
    assert_eq!(size(&postcard), (1040, 700));

    let rather = RatherCard {
        lang: tr,
        category: "OKYANUS",
        a: "Bir yunusla yarışmak",
        b: "Bir deniz kaplumbağasıyla yarışmak",
        votes: Some((1, 2)),
        closed: true,
        theme: Theme::reef(),
    };
    let svg = rather.to_svg();
    assert!(svg.contains("HANGİSİNİ SEÇERDİN") && svg.contains("SON SONUÇ: 3 OY") && svg.contains("67%"));
    let open = RatherCard {
        votes: None,
        closed: false,
        ..rather
    };
    assert!(open.to_svg().contains("DÜĞMELERLE BİR TARAF SEÇ"));

    let badges: Vec<String> = [PromptTone::Turn, PromptTone::Truth, PromptTone::Dare]
        .into_iter()
        .map(|tone| {
            PromptCard {
                lang: tr,
                tone,
                name: "Ada",
                text: "Doğruluk mu, cesaret mi?",
                category: "",
                footer: "YALNIZCA SIRASI GELEN OYUNCU SEÇEBİLİR",
                theme: Theme::reef(),
            }
            .to_svg()
        })
        .collect();
    assert!(badges[0].contains("SIRA SENDE") && badges[1].contains("DOĞRULUK") && badges[2].contains("CESARET"));

    // `PINCHORA_PREVIEW_DIR=some/dir cargo test -p pinchora-render turkish_cards` saves the pictures to look at.
    if let Some(dir) = std::env::var_os("PINCHORA_PREVIEW_DIR").map(std::path::PathBuf::from) {
        std::fs::create_dir_all(&dir).expect("preview dir");
        let named = [
            ("lobby", &lobby),
            ("pearl", &pearl),
            ("race", &race),
            ("reveal", &reveal),
        ];
        for (name, svg) in named.into_iter().chain([("postcard", &postcard), ("rather", &svg)]) {
            let png = Renderer::new().svg_to_png(svg).expect("renders");
            std::fs::write(dir.join(format!("party-tr-{name}.png")), png).expect("saves");
        }
    }
}
