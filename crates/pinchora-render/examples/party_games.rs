//! Renders the party game cards with sample data: `cargo run -p pinchora-render --example party -- <out-dir>`.

use image::RgbaImage;
use pinchora_render::{
    EntriesCard, EntryHeader, EntryRow, LettersCard, PartyLobbyCard, PearlCard, PearlSeat, PostcardCard, PromptCard,
    PromptTone, RaceCard, Racer, RacerState, RatherCard, Renderer, StandingRow, StandingsCard, Theme, encode_png,
};

fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/render-samples".into());
    std::fs::create_dir_all(&out)?;
    let r = Renderer::new();
    let save = |name: &str, png: Vec<u8>| std::fs::write(format!("{out}/party-{name}.png"), png);

    let players: Vec<String> = [
        "Captain Pinch",
        "Marlowe",
        "Sandy Toes",
        "A very long display name indeed",
    ]
    .map(String::from)
    .to_vec();
    save(
        "lobby",
        r.party_lobby_card(&PartyLobbyCard {
            lang: pinchora_core::infra::i18n::Lang::En,
            title: "Hot Pearl",
            tagline: "A syllable appears. Type a word that contains it before the pearl pops!",
            players: &players,
            min: 2,
            max: 8,
            footer: "Ready: the host can press start",
            theme: Theme::reef(),
        })?,
    )?;

    for (name, n) in [("pearl-3", 3usize), ("pearl-8", 8)] {
        let seats: Vec<PearlSeat> = (0..n)
            .map(|i| PearlSeat {
                name: [
                    "Captain Pinch",
                    "Marlowe",
                    "Sandy Toes",
                    "Tide",
                    "Pip",
                    "A very long display name",
                    "Bo",
                    "Cy",
                ][i],
                lives: if i == 1 { 1 } else { 2 },
                max_lives: 2,
                out: i == 2 && n > 3,
                current: i == 0,
            })
            .collect();
        save(
            name,
            r.pearl_card(&PearlCard {
                lang: pinchora_core::infra::i18n::Lang::En,
                syllable: "ear",
                seats: &seats,
                banner: "Marlowe played PEARL  ·  Captain Pinch is up",
                last_word: Some("pearl"),
                theme: Theme::reef(),
            })?,
        )?;
    }

    let rows = [
        StandingRow {
            rank: 1,
            name: "Marlowe",
            score: "7 pts",
            note: "+25 shells",
            winner: true,
        },
        StandingRow {
            rank: 2,
            name: "Captain Pinch",
            score: "3 pts",
            note: "+13 shells",
            winner: false,
        },
        StandingRow {
            rank: 3,
            name: "Sandy Toes",
            score: "0 pts",
            note: "daily limit reached",
            winner: false,
        },
    ];
    save(
        "standings",
        r.standings_card(&StandingsCard {
            title: "Acronym Tide",
            subtitle: "Final scores after 3 rounds",
            rows: &rows,
            footer: "Thanks for playing",
            theme: Theme::reef(),
        })?,
    )?;

    save(
        "letters",
        r.letters_card(&LettersCard {
            title: "Acronym Tide",
            round: "Round 2/3",
            letters: &['T', 'W', 'R', 'S'],
            hint: "One word for each letter, in order. Press the button and type your phrase.",
            status: "2 of 5 phrases in",
            theme: Theme::reef(),
        })?,
    )?;

    let phrases = [
        "Tiny whales ride sleds",
        "Terrific wizards refuse sandwiches, politely, while hopping on one leg",
        "Tired walruses rarely sing",
    ];
    let anonymous: Vec<EntryRow> = phrases
        .iter()
        .map(|text| EntryRow {
            text,
            author: None,
            votes: None,
            winner: false,
        })
        .collect();
    save(
        "vote",
        r.entries_card(&EntriesCard {
            lang: pinchora_core::infra::i18n::Lang::En,
            title: "Acronym Tide",
            subtitle: "Which phrase is best for T · W · R · S?",
            chip: "Round 2/3",
            header: None,
            rows: &anonymous,
            footer: "You can not vote for your own",
            theme: Theme::reef(),
        })?,
    )?;
    let revealed: Vec<EntryRow> = phrases
        .iter()
        .zip([
            ("Marlowe", 3, true),
            ("Captain Pinch", 1, false),
            ("Sandy Toes", 0, false),
        ])
        .map(|(text, (author, votes, winner))| EntryRow {
            text,
            author: Some(author),
            votes: Some(votes),
            winner,
        })
        .collect();
    let scene = encode_png(&RgbaImage::from_fn(960, 540, |x, y| {
        image::Rgba([(x / 4) as u8, (y / 3) as u8, 200, 255])
    }))?;
    save(
        "reveal-postcard",
        r.entries_card(&EntriesCard {
            lang: pinchora_core::infra::i18n::Lang::En,
            title: "Caption the Postcard",
            subtitle: "The votes are in",
            chip: "Round 1/3",
            header: Some(EntryHeader {
                png: &scene,
                w: 960,
                h: 540,
            }),
            rows: &revealed,
            footer: "1 point a vote, 2 more for the winner",
            theme: Theme::reef(),
        })?,
    )?;

    let rows = [[1u8, 2, 3, 1, 1], [2, 2, 3, 1, 2], [3, 3, 3, 3, 3]];
    let racers = [
        Racer {
            name: "Marlowe",
            rows: &rows,
            state: RacerState::Solved(3),
        },
        Racer {
            name: "Captain Pinch",
            rows: &rows[..2],
            state: RacerState::Playing,
        },
        Racer {
            name: "Sandy Toes",
            rows: &rows[..1],
            state: RacerState::Playing,
        },
        Racer {
            name: "Tide",
            rows: &[],
            state: RacerState::Failed,
        },
    ];
    save(
        "race",
        r.race_card(&RaceCard {
            lang: pinchora_core::infra::i18n::Lang::En,
            title: "Wordle Race",
            racers: &racers,
            footer: "First to find the word wins",
            theme: Theme::reef(),
        })?,
    )?;
    save(
        "postcard",
        r.postcard_card(&PostcardCard {
            lang: pinchora_core::infra::i18n::Lang::En,
            scene: &scene,
            place: "Driftwood Dunes",
            stamp: "Round 1/3",
            theme: Theme::reef(),
        })?,
    )?;

    let open = RatherCard {
        lang: pinchora_core::infra::i18n::Lang::En,
        category: "Silly",
        a: "Have hiccups that sound like a duck",
        b: "Sneeze glitter every time",
        votes: None,
        closed: false,
        theme: Theme::reef(),
    };
    save("rather", r.rather_card(&open)?)?;
    save(
        "rather-result",
        r.rather_card(&RatherCard {
            lang: pinchora_core::infra::i18n::Lang::En,
            votes: Some((7, 3)),
            closed: true,
            ..open
        })?,
    )?;

    for (name, tone, text, category) in [
        ("turn", PromptTone::Turn, "Truth or dare?", "Cozy"),
        (
            "truth",
            PromptTone::Truth,
            "What is your favorite snack to eat at midnight?",
            "Food",
        ),
        (
            "dare",
            PromptTone::Dare,
            "Describe your perfect cozy night in four sentences, and make every one of them rhyme with the last.",
            "Cozy",
        ),
    ] {
        save(
            name,
            r.prompt_card(&PromptCard {
                lang: pinchora_core::infra::i18n::Lang::En,
                tone,
                name: "Captain Pinch",
                text,
                category,
                footer: "Do it, or pass: passing is always fine",
                theme: Theme::reef(),
            })?,
        )?;
    }
    Ok(())
}
